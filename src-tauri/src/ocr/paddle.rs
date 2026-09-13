use super::{build_ocr_result, convert_box_coords, OcrEngine, OcrResult, OcrTextBlock};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// PaddleOCR-json 进程管理器
/// 通过 stdio 与 PaddleOCR-json.exe 通信
pub struct PaddleOcr {
    exe_path: PathBuf,
}

#[derive(Deserialize)]
struct PaddleResponse {
    code: i32,
    data: Vec<PaddleBlock>,
}

#[derive(Deserialize)]
struct PaddleBlock {
    text: String,
    #[serde(default, rename = "box")]
    r#box: Option<Vec<[i32; 2]>>,
    #[serde(default)]
    score: Option<f64>,
}

impl PaddleOcr {
    pub fn new() -> Self {
        Self {
            exe_path: Self::find_exe(),
        }
    }

    /// 查找 PaddleOCR-json 可执行文件
    fn find_exe() -> PathBuf {
        let exe_name = if cfg!(windows) {
            "PaddleOCR-json.exe"
        } else {
            "PaddleOCR-json"
        };

        // 1. 优先查找应用数据目录
        if let Some(data_dir) = dirs::data_dir() {
            let path = data_dir
                .join("DesktopAide")
                .join("PaddleOCR-json")
                .join(exe_name);
            if path.exists() {
                return path;
            }
        }

        // 2. 查找可执行文件同目录下的 resources
        if let Ok(exe_dir) = std::env::current_exe() {
            if let Some(parent) = exe_dir.parent() {
                let path = parent
                    .join("resources")
                    .join("PaddleOCR-json")
                    .join(exe_name);
                if path.exists() {
                    return path;
                }
                // 开发模式下可能在 src-tauri 目录
                let dev_path = parent
                    .join("..")
                    .join("resources")
                    .join("PaddleOCR-json")
                    .join(exe_name);
                if dev_path.exists() {
                    return dev_path;
                }
            }
        }

        // 3. 最后回退到当前工作目录
        PathBuf::from(exe_name)
    }

    /// 使用命令行参数方式调用 PaddleOCR-json（一次性进程）
    fn call_once(&self, image_path: &str) -> Result<Vec<PaddleBlock>, String> {
        let output = Command::new(&self.exe_path)
            .arg(format!("-image_path={}", image_path))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("启动 PaddleOCR-json 失败: {}. 路径: {:?}", e, self.exe_path))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("PaddleOCR-json 进程异常: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let response: PaddleResponse = serde_json::from_str(&stdout)
            .map_err(|e| format!("解析 PaddleOCR 响应失败: {}. 原始输出: {}", e, stdout))?;

        if response.code == 101 {
            // 未检测到文字，返回空结果
            return Ok(vec![]);
        }

        if response.code != 100 {
            return Err(format!("PaddleOCR 识别失败, 错误码: {}", response.code));
        }

        Ok(response.data)
    }
}

impl OcrEngine for PaddleOcr {
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
