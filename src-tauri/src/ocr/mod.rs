use serde::{Deserialize, Serialize};

mod paddle;
mod rapid;
mod winrt;

pub use paddle::PaddleOcr;
pub use rapid::RapidOcr;
pub use winrt::WinRtOcr;

/// OCR 识别结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text_blocks: Vec<OcrTextBlock>,
    pub full_text: String,
}

/// 单个 OCR 文字块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrTextBlock {
    pub text: String,
    pub confidence: f32,
    /// 包围盒坐标 [左上x, 左上y, 右上x, 右上y, 右下x, 右下y, 左下x, 左下y]
    pub box_coords: [i32; 8],
}

/// 将引擎的 box 格式统一转换为 8 坐标数组
pub(crate) fn convert_box_coords(coords: &Option<Vec<[i32; 2]>>) -> [i32; 8] {
    match coords {
        Some(coords) if coords.len() == 4 => [
            coords[0][0],
            coords[0][1], // 左上
            coords[1][0],
            coords[1][1], // 右上
            coords[2][0],
            coords[2][1], // 右下
            coords[3][0],
            coords[3][1], // 左下
        ],
        _ => [0i32; 8],
    }
}

/// 将文字块列表合并为 OcrResult
pub(crate) fn build_ocr_result(text_blocks: Vec<OcrTextBlock>) -> OcrResult {
    let full_text: String = text_blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<&str>>()
        .join("\n");
    OcrResult {
        text_blocks,
        full_text,
    }
}

/// 对外暴露的 OCR 文件识别入口（引擎级联：Rapid → Paddle → WinRt）
pub fn try_ocr_on_file(image_path: &std::path::Path) -> Option<OcrResult> {
    let path_str = image_path.to_string_lossy();

    let rapid = RapidOcr::new();
    if rapid.is_available() {
        if let Ok(result) = rapid.recognize_file(&path_str) {
            log::info!("RapidOCR 成功: {} 个文字块", result.text_blocks.len());
            return Some(result);
        }
    }

    let paddle = PaddleOcr::new();
    if paddle.is_available() {
        if let Ok(result) = paddle.recognize_file(&path_str) {
            log::info!("PaddleOCR 成功: {} 个文字块", result.text_blocks.len());
            return Some(result);
        }
    }

    let winrt = WinRtOcr::new();
    match winrt.recognize_file(&path_str) {
        Ok(result) => {
            log::info!("WinRtOcr 成功: {} 个文字块", result.text_blocks.len());
            Some(result)
        }
        Err(e) => {
            log::warn!("WinRtOcr 失败: {}", e);
            None
        }
    }
}

/// OCR 引擎抽象接口
pub trait OcrEngine: Send {
    /// 对图片文件进行 OCR 识别
    fn recognize_file(&self, image_path: &str) -> Result<OcrResult, String>;

    /// 检测引擎是否可用
    fn is_available(&self) -> bool;
}
