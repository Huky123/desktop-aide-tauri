//! 屏幕截图：GDI BitBlt 抓取屏幕像素，WinRT BitmapEncoder 编码 PNG。

use base64::Engine;
use std::mem::size_of;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SetWindowDisplayAffinity, SM_CXSCREEN, SM_CYSCREEN, WDA_EXCLUDEFROMCAPTURE,
    WDA_NONE,
};

/// 截图期间把指定窗口从捕获中排除（WDA_EXCLUDEFROMCAPTURE）。
///
/// 相比"隐藏窗口再截图"，窗口保持可见（用户无感知、无闪烁），
/// 仅截图中窗口位置显示为黑色。Drop 时自动恢复 WDA_NONE。
pub struct CaptureExclusionGuard {
    hwnd: HWND,
    active: bool,
}

impl CaptureExclusionGuard {
    pub fn new(hwnd: HWND) -> Self {
        let mut guard = Self {
            hwnd,
            active: false,
        };
        unsafe {
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_ok() {
                guard.active = true;
                // 等待 DWM 应用排除（官方建议至少 32ms）
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
        }
        guard
    }

    /// WDA 是否成功生效（旧版 Windows 可能不支持，调用方可降级处理）
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for CaptureExclusionGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                let _ = SetWindowDisplayAffinity(self.hwnd, WDA_NONE);
            }
        }
    }
}

/// 截取主屏幕，返回 (PNG base64, 物理宽度, 物理高度)
pub fn capture_primary_screen() -> Result<(String, u32, u32), String> {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err(format!("获取屏幕尺寸失败: {width}x{height}"));
        }

        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("获取屏幕 DC 失败".to_string());
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            ReleaseDC(None, screen_dc);
            return Err("创建内存 DC 失败".to_string());
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, screen_dc);
            return Err("创建位图失败".to_string());
        }
        let old: HGDIOBJ = SelectObject(mem_dc, bitmap.into());

        // 复制屏幕像素到内存位图
        let blt_result = BitBlt(mem_dc, 0, 0, width, height, Some(screen_dc), 0, 0, SRCCOPY);

        // 读取像素（top-down BGRA，32bpp）
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let copied = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // 清理 GDI 对象（无论成功与否）
        SelectObject(mem_dc, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        if let Err(e) = blt_result {
            return Err(format!("BitBlt 失败: {e}"));
        }
        if copied == 0 {
            return Err("GetDIBits 读取像素失败".to_string());
        }

        let png = encode_png_bgra(&pixels, width as u32, height as u32)?;
        Ok((png, width as u32, height as u32))
    }
}

/// 将 BGRA 像素编码为 PNG base64（WinRT BitmapEncoder，零额外依赖）
fn encode_png_bgra(pixels: &[u8], width: u32, height: u32) -> Result<String, String> {
    let stream = InMemoryRandomAccessStream::new().map_err(|e| format!("创建内存流失败: {e}"))?;
    let encoder_id =
        BitmapEncoder::PngEncoderId().map_err(|e| format!("获取 PNG 编码器 ID 失败: {e}"))?;
    let encoder = BitmapEncoder::CreateAsync(encoder_id, &stream)
        .map_err(|e| format!("创建 PNG 编码器失败: {e}"))?
        .get()
        .map_err(|e| format!("等待编码器创建失败: {e}"))?;

    encoder
        .SetPixelData(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Ignore,
            width,
            height,
            96.0,
            96.0,
            pixels,
        )
        .map_err(|e| format!("设置像素数据失败: {e}"))?;
    encoder
        .FlushAsync()
        .map_err(|e| format!("刷新编码失败: {e}"))?
        .get()
        .map_err(|e| format!("等待编码完成失败: {e}"))?;

    // 读取编码后的字节
    stream.Seek(0).map_err(|e| format!("定位流失败: {e}"))?;
    let size = stream.Size().map_err(|e| format!("读取流大小失败: {e}"))?;
    let reader =
        DataReader::CreateDataReader(&stream).map_err(|e| format!("创建流读取器失败: {e}"))?;
    reader
        .LoadAsync(size as u32)
        .map_err(|e| format!("加载流失败: {e}"))?
        .get()
        .map_err(|e| format!("等待流加载失败: {e}"))?;
    let mut bytes = vec![0u8; size as usize];
    reader
        .ReadBytes(&mut bytes)
        .map_err(|e| format!("读取流失败: {e}"))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
