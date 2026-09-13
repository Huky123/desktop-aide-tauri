use super::{OcrEngine as OcrTrait, OcrResult, OcrTextBlock};
use windows::core::HSTRING;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap,
};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::FileAccessMode;
use windows::Storage::StorageFile;

/// Windows.Media.Ocr 引擎 —— 系统内置，零额外依赖
/// 使用 Windows 10+ 内置 OCR，支持中文简体，作为 PaddleOCR 的降级方案
pub struct WinRtOcr;

impl WinRtOcr {
    pub fn new() -> Self {
        Self
    }

    /// 从 SoftwareBitmap 执行 OCR 并解析结果（内部共用逻辑）
    fn recognize_bitmap(engine: &OcrEngine, bitmap: &SoftwareBitmap) -> Result<OcrResult, String> {
        let ocr_result = engine
            .RecognizeAsync(bitmap)
            .map_err(|e| format!("WinRtOcr: 识别失败: {e}"))?
            .get()
            .map_err(|e| format!("WinRtOcr: 等待识别失败: {e}"))?;

        // 解析 OCR 结果：按行提取文字
        let lines = ocr_result
            .Lines()
            .map_err(|e| format!("WinRtOcr: 获取行失败: {e}"))?;

        let mut text_blocks = Vec::new();
        let mut full_text_parts = Vec::new();

        let line_count = lines
            .Size()
            .map_err(|e| format!("WinRtOcr: 获取行数失败: {e}"))?;

        for i in 0..line_count {
            let line = lines
                .GetAt(i)
                .map_err(|e| format!("WinRtOcr: 获取第{}行失败: {e}", i))?;

            let words = line
                .Words()
                .map_err(|e| format!("WinRtOcr: 获取词失败: {e}"))?;

            let mut line_text = String::new();
            let word_count = words
                .Size()
                .map_err(|e| format!("WinRtOcr: 获取词数失败: {e}"))?;

            for j in 0..word_count {
                if let Ok(word) = words.GetAt(j) {
                    if let Ok(text) = word.Text() {
                        line_text.push_str(&text.to_string());
                    }
                }
            }

            if !line_text.is_empty() {
                full_text_parts.push(line_text.clone());
                text_blocks.push(OcrTextBlock {
                    text: line_text,
                    confidence: 0.9,
                    box_coords: [0i32; 8],
                });
            }
        }

        let full_text = full_text_parts.join("\n");

        Ok(OcrResult {
            text_blocks,
            full_text,
        })
    }
}

impl OcrTrait for WinRtOcr {
    fn recognize_file(&self, image_path: &str) -> Result<OcrResult, String> {
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|e| format!("WinRtOcr: 系统不支持 OCR 或语言包未安装: {e}"))?;

        let path_hstring = HSTRING::from(image_path);

        // 打开图片文件
        let file = StorageFile::GetFileFromPathAsync(&path_hstring)
            .map_err(|e| format!("WinRtOcr: 打开文件失败: {e}"))?
            .get()
            .map_err(|e| format!("WinRtOcr: 等待文件失败: {e}"))?;

        // 打开文件流
        let stream = file
            .OpenAsync(FileAccessMode::Read)
            .map_err(|e| format!("WinRtOcr: 打开流失败: {e}"))?
            .get()
            .map_err(|e| format!("WinRtOcr: 等待流失败: {e}"))?;

        // 从流解码图片
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|e| format!("WinRtOcr: 解码失败: {e}"))?
            .get()
            .map_err(|e| format!("WinRtOcr: 等待解码失败: {e}"))?;

        // 获取 BGRA8 格式的位图（OCR 需要此格式）
        let bitmap = decoder
            .GetSoftwareBitmapConvertedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
            )
            .map_err(|e| format!("WinRtOcr: 位图转换失败: {e}"))?
            .get()
            .map_err(|e| format!("WinRtOcr: 等待位图失败: {e}"))?;

        Self::recognize_bitmap(&engine, &bitmap)
    }

    fn is_available(&self) -> bool {
        OcrEngine::TryCreateFromUserProfileLanguages().is_ok()
    }
}
