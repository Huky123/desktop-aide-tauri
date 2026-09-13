use tauri::WebviewWindow;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowPlacement, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_MAXIMIZE, WINDOWPLACEMENT,
};

/// 将窗口设置为最顶层（Win+D 不隐藏）
pub fn make_topmost(window: &WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        let hwnd = HWND(hwnd.0);
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

/// 检测前台窗口是否最大化（全屏）
#[tauri::command]
pub fn is_foreground_fullscreen() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }

        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..WINDOWPLACEMENT::default()
        };
        if GetWindowPlacement(hwnd, &mut placement).is_err() {
            return false;
        }

        // SW_SHOWMAXIMIZED (3) = 最大化
        placement.showCmd == SW_MAXIMIZE.0 as u32
    }
}
