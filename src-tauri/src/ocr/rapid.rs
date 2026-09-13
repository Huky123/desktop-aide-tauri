use super::{build_ocr_result, convert_box_coords, OcrEngine, OcrResult, OcrTextBlock};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// RapidOCR-json 引擎
/// 通过命令行调用 RapidOCR-json.exe（轻量级 OCR，无需 AVX）
pub struct RapidOcr {
    exe_path: PathBuf,
}

#[derive(Deserialize)]
struct RapidResponse {
    code: i32,
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct RapidBlock {
    text: String,
    #[serde(default, rename = "box")]
    r#box: Option<Vec<[i32; 2]>>,
    #[serde(default)]
    score: Option<f64>,
}

impl RapidOcr {
    pub fn new() -> Self {
        Self {
            exe_path: Self::find_exe(),
        }
    }

    /// 查找 RapidOCR-json 可执行文件
    fn find_exe() -> PathBuf {
        let exe_name = "RapidOCR-json.exe";

        // 1. 优先查找应用数据目录
        if let Some(data_dir) = dirs::data_dir() {
            let path = data_dir
                .join("DesktopAide")
                .join("RapidOCR-json")
                .join(exe_name);
            if path.exists() {
                log::info!("找到 RapidOCR-json: {:?}", path);
                return path;
            }
        }

        // 2. 查找可执行文件同目录下的 resources
        if let Ok(exe_dir) = std::env::current_exe() {
            if let Some(parent) = exe_dir.parent() {
                let path = parent
                    .join("resources")
                    .join("RapidOCR-json")
                    .join(exe_name);
                if path.exists() {
                    return path;
                }
                // 开发模式下指向项目根目录的 resources/RapidOCR-json
                let dev_path = parent
                    .join("..")
                    .join("..")
                    .join("resources")
                    .join("RapidOCR-json")
                    .join(exe_name);
                if dev_path.exists() {
                    return dev_path;
                }
            }
        }

        // 3. 最后回退到当前工作目录
        PathBuf::from(exe_name)
    }

    /// 使用命令行参数方式调用 RapidOCR-json（一次性进程）
    fn call_once(&self, image_path: &str) -> Result<Vec<RapidBlock>, String> {
        // 工作目录设为 exe 所在目录，确保 models/ 能被找到
        let work_dir = self.exe_path.parent().unwrap_or(std::path::Path::new("."));

        let output = Command::new(&self.exe_path)
            .arg(format!("--image={}", image_path))
            .current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("启动 RapidOCR-json 失败: {}. 路径: {:?}", e, self.exe_path))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("RapidOCR-json 进程异常: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // RapidOCR-json 在 stdout 中混合了启动信息（如 "RapidOCR-json v1.1.0\nOCR init completed.\n"）
        // 需要提取 JSON 行
        let json_line = stdout
            .lines()
            .find(|line| line.trim_start().starts_with('{'))
            .unwrap_or(&stdout);

        let response: RapidResponse = serde_json::from_str(json_line)
            .map_err(|e| format!("解析 RapidOCR 响应失败: {}. 原始输出: {}", e, stdout))?;

        match response.code {
            100 => {
                // 成功：data 是数组 [{text, box, score}, ...]
                let blocks: Vec<RapidBlock> =
                    serde_json::from_value(response.data).map_err(|e| {
                        format!("解析 RapidOCR 数据块失败: {}. 原始data: {:?}", e, stdout)
                    })?;
                Ok(blocks)
            }
            101 => {
                // 未检测到文字，返回空结果
                Ok(vec![])
            }
            _ => {
                // 错误
                let err_msg = response.data.as_str().unwrap_or("未知错误");
                Err(format!(
                    "RapidOCR 识别失败, code={}: {}",
                    response.code, err_msg
                ))
            }
        }
    }
}

impl OcrEngine for RapidOcr {
    fn recognize_file(&self, image_path: &str) -> Result<OcrResult, String> {
        let blocks = self.call_once(image_path)?;

        let text_blocks: Vec<OcrTextBlock> = blocks
            .iter()
            .map(|b| OcrTextBlock {
                text: b.text.clone(),
                confidence: b.score.unwrap_or(0.0) as f32,
                box_coords: convert_box_coords(&b.r#box),
            })
            .collect();

        Ok(build_ocr_result(text_blocks))
    }

    fn is_available(&self) -> bool {
        self.exe_path.exists()
    }
}
