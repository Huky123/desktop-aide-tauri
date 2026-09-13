use crate::screen_capture;
use crate::state::{AppState, ScreenshotData};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow};

/// 截图窗口 label
const SCREENSHOT_WINDOW_LABEL: &str = "screenshot";

/// 截图窗口数据（前端提交的裁剪附件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotAttachment {
    pub data: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// 截图：通过 WDA_EXCLUDEFROMCAPTURE 把主窗口从截图中排除 →
/// BitBlt 全屏 → 创建独立全屏截图窗口。
///
/// 窗口保持可见（无闪烁），截图中窗口位置显示为黑色。
/// 主窗口保持原尺寸不动（不进入全屏），截图交互全部发生在
/// 独立的"截图窗口"中，确认后通过事件把裁剪结果发回主窗口。
#[tauri::command]
pub async fn capture_screen(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // 1. 抓取全屏（排除自身窗口；WDA 不可用时降级为隐藏窗口）
    let (image_base64, width, height) = {
        let hwnd = window
            .hwnd()
            .map_err(|e| format!("获取窗口句柄失败: {e}"))?;
        let guard = crate::screen_capture::CaptureExclusionGuard::new(hwnd);
        if !guard.is_active() {
            // 旧版系统不支持 WDA_EXCLUDEFROMCAPTURE：降级为隐藏窗口
            let _ = window.hide();
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        let result = screen_capture::capture_primary_screen();
        if !guard.is_active() {
            let _ = window.show();
        }
        result?
    };

    // 4. 暂存截图数据，供截图窗口拉取
    *state.screenshot_data.lock().await = Some(ScreenshotData {
        image_base64,
        width,
        height,
    });

    // 5. 创建/显示独立全屏截图窗口
    show_screenshot_window(&app)
}

/// 截图窗口拉取截图数据
#[tauri::command]
pub async fn get_screenshot_data(
    state: tauri::State<'_, AppState>,
) -> Result<ScreenshotData, String> {
    state
        .screenshot_data
        .lock()
        .await
        .clone()
        .ok_or_else(|| "没有可用的截图数据".to_string())
}

/// 截图窗口确认：把裁剪附件广播给主窗口，关闭截图窗口
#[tauri::command]
pub async fn submit_screenshot_capture(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    attachment: ScreenshotAttachment,
) -> Result<(), String> {
    // 清空暂存数据（防止重复提交）
    *state.screenshot_data.lock().await = None;

    // 广播给所有窗口（主窗口监听后加入待发送附件）
    let _ = app.emit("screenshot-captured", attachment);

    // 关闭截图窗口
    if let Some(win) = app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

/// 截图窗口取消：清空数据并关闭窗口
#[tauri::command]
pub async fn cancel_screenshot(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    *state.screenshot_data.lock().await = None;
    if let Some(win) = app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

/// 创建（或重新显示）独立全屏截图窗口
fn show_screenshot_window(app: &AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(SCREENSHOT_WINDOW_LABEL) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let url = screenshot_window_url(app);
    tauri::WebviewWindowBuilder::new(app, SCREENSHOT_WINDOW_LABEL, url)
        .title("截图提问")
        .fullscreen(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .build()
        .map_err(|e| format!("创建截图窗口失败: {e}"))?;
    log::info!("[screenshot] 截图窗口已创建");
    Ok(())
}

/// 截图窗口 URL：dev 指向 Vite 的 #screenshot 路由，prod 指向打包资源
fn screenshot_window_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(debug_assertions)]
    {
        let dev_url = app
            .config()
            .build
            .dev_url
            .clone()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "http://localhost:5173".to_string());
        WebviewUrl::External(
            format!("{}/#screenshot", dev_url.trim_end_matches('/'))
                .parse()
                .expect("无效的 devUrl"),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        WebviewUrl::App("index.html#screenshot".into())
    }
}
