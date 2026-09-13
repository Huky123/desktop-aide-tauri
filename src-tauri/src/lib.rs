use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{ContextMenu, Menu, MenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use tauri::PhysicalSize;
use tauri::Runtime;
use tauri::State;
use tauri::WebviewWindow;
use tokio::sync::{Mutex, RwLock};

mod ai_service;
mod commands;
mod config;
mod db;
mod image_store;
mod ocr;
mod paths;
mod screen_capture;
mod state;
mod window;

use crate::ai_service::agent::Agent;
use crate::ai_service::provider;
use crate::config::manager::ConfigManager;
use crate::db::Database;
use crate::image_store::ImageStore;
use crate::state::AppState;

const TRAY_ID_MAIN: &str = "main";
const MENU_ID_OPEN: &str = "open";
const MENU_ID_QUIT: &str = "quit";

#[tauri::command]
async fn toggle_panel(
    state: State<'_, AppState>,
    window: WebviewWindow,
    expand: bool,
) -> Result<(), String> {
    // 双击拖拽区域会让 Windows 最大化无边框窗口。切换气泡/面板前必须
    // 先退出最大化，否则 set_size 只会修改还原尺寸，窗口仍保持最大化。
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();

        // Windows 保留最大化前的还原尺寸；此时读取它并覆盖可能被旧版本
        // 错误保存的屏幕尺寸，确保下一次展开仍回到正常面板大小。
        if let Ok(size) = window.inner_size() {
            let mut mgr = state.config_manager.lock().await;
            let mut cfg = mgr.get().clone();
            cfg.panel_width = size.width.max(420);
            cfg.panel_height = size.height.max(600);
            let _ = mgr.update(cfg)?;
        }
    }

    // 截图模式结束后窗口可能处于全屏状态；先退出全屏，
    // 否则 set_size 只修改还原尺寸，窗口仍保持全屏。
    if window.is_fullscreen().unwrap_or(false) {
        let _ = window.set_fullscreen(false);
    }

    if expand {
        let (w, h) = {
            let mgr = state.config_manager.lock().await;
            let cfg = mgr.get();
            (cfg.panel_width.max(420), cfg.panel_height.max(600))
        };
        let _ = window.set_resizable(true);
        let _ = window.set_min_size(Some(PhysicalSize::new(420, 600)));
        let _ = window.set_size(PhysicalSize::new(w, h));
    } else {
        let _ = window.set_resizable(false);
        let _ = window.set_size(PhysicalSize::new(64, 64));
    }
    Ok(())
}

/// 保存用户拖拽后的面板尺寸（前端防抖后调用）
#[tauri::command]
async fn save_panel_size(
    state: State<'_, AppState>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let width = width.max(420);
    let height = height.max(600);
    let mut mgr = state.config_manager.lock().await;
    let mut cfg = mgr.get().clone();
    cfg.panel_width = width;
    cfg.panel_height = height;
    // 面板尺寸与 AI 配置无关，无需关心被替换的旧配置
    let _ = mgr.update(cfg)?;
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    graceful_quit(&app);
}

fn graceful_quit(app: &AppHandle) {
    let _ = app.remove_tray_by_id(TRAY_ID_MAIN);
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.destroy() {
            log::warn!("销毁主窗口失败，直接退出: {error}");
            app.exit(0);
            return;
        }

        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            app.exit(0);
        });
    } else {
        app.exit(0);
    }
}

#[tauri::command]
fn show_bubble_menu(window: tauri::Window) -> Result<(), String> {
    let menu = build_assistant_menu(&window).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())
}

fn build_assistant_menu<R, M>(manager: &M) -> tauri::Result<Menu<R>>
where
    R: Runtime,
    M: Manager<R>,
{
    MenuBuilder::new(manager)
        .text(MENU_ID_OPEN, "打开助手")
        .separator()
        .text(MENU_ID_QUIT, "退出应用")
        .build()
}

fn show_assistant(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit("tray-open-panel", ());
}

fn request_tray_exit(app: &AppHandle) {
    let busy = app
        .try_state::<AppState>()
        .map(|state| state.agent_busy.load(Ordering::SeqCst))
        .unwrap_or(false);
    if busy {
        show_assistant(app);
        let _ = app.emit("app-exit-requested", ());
    } else {
        graceful_quit(app);
    }
}

/// 注册全局热键：
/// - Ctrl+Alt+Space：切换面板（前端监听 global-toggle-panel）
/// - Ctrl+Alt+C：截图提问（前端监听 capture-hotkey）
fn register_global_hotkeys(app: &AppHandle) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    let toggle = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
    app.global_shortcut()
        .on_shortcut(toggle, |handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle.emit("global-toggle-panel", ());
            }
        })
        .map_err(|e| format!("注册切换热键失败: {e}"))?;

    let capture = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyC);
    app.global_shortcut()
        .on_shortcut(capture, |handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = handle.emit("capture-hotkey", ());
            }
        })
        .map_err(|e| format!("注册截图热键失败: {e}"))?;

    log::info!("[hotkey] 已注册: Ctrl+Alt+Space 切换面板 / Ctrl+Alt+C 截图提问");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_ID_OPEN => show_assistant(app),
            MENU_ID_QUIT => request_tray_exit(app),
            _ => {}
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage({
            // 解析数据目录（bootstrap 注册表 → 用户自定义或默认 %APPDATA%/DesktopAide）
            let app_paths = paths::AppPaths::resolve();
            let data_dir = app_paths.data_dir.clone();
            log::info!("[paths] 数据目录: {}", data_dir.display());
            AppState {
                config_manager: Mutex::new(ConfigManager::new(&data_dir)),
                agent: Arc::new(RwLock::new(Agent::new(provider::from_config(
                    &Default::default(),
                )))),
                agent_busy: AtomicBool::new(false),
                database: Arc::new(Mutex::new(
                    Database::open(&data_dir).expect("无法打开聊天记录数据库"),
                )),
                current_conversation_id: Mutex::new("default".to_string()),
                image_store: Mutex::new(ImageStore::new(&data_dir).expect("无法创建图片存储目录")),
                data_dir,
                screenshot_data: tokio::sync::Mutex::new(None),
                ai_cancel: tokio::sync::Mutex::new(None),
            }
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // 设置窗口置顶层 (HWND_TOPMOST)
            let window = app.get_webview_window("main").unwrap();
            window::make_topmost(&window);

            let tray_menu = build_assistant_menu(app)?;
            let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID_MAIN)
                .menu(&tray_menu)
                .tooltip("桌面助手")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_assistant(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            tray_builder.build(app)?;

            // 设置初始大小（气泡态）
            let _ = window.set_size(PhysicalSize::new(64, 64));

            // 显示窗口
            let _ = window.show();

            // 初始化 AI Agent（使用已加载的配置）
            let state = app.state::<AppState>();
            let image_root = {
                let store = state.image_store.blocking_lock();
                store.root().to_path_buf()
            };
            app.asset_protocol_scope()
                .allow_directory(&image_root, true)?;
            log::info!(
                "[ImageStore] 已允许 asset protocol 访问: {}",
                image_root.display()
            );

            let config = {
                let mgr = state.config_manager.blocking_lock();
                mgr.get().clone()
            };
            let adapter = provider::from_config(&config);
            let mut agent = Agent::new(adapter);
            // 模型用途（对话/识图/出图）由用户配置决定
            agent.set_model_kind(crate::ai_service::agent::ModelKind::from_config(
                &config.model_kind,
            ));
            // 恢复上次探明的通路结论：省掉启动后第一次请求的重新探测
            agent.set_learned_transport(&config.learned_transport);
            {
                let mut agent_lock = state.agent.blocking_write();
                *agent_lock = agent;
            }

            // 注册全局热键（失败不阻塞启动，仅记录警告）
            if let Err(e) = register_global_hotkeys(app.handle()) {
                log::warn!("[hotkey] 全局热键注册失败: {e}");
            }

            // 恢复定时提醒：未来到期的照常调度；应用关闭期间已到期的在启动时补发，
            // 避免"离线期间到期 = 提醒静默丢失"
            {
                let state = app.state::<AppState>();
                let now = chrono::Utc::now().timestamp_millis();
                let reminders = state
                    .database
                    .blocking_lock()
                    .list_reminders()
                    .unwrap_or_default();
                let total = reminders.len();
                let missed = reminders.iter().filter(|r| r.remind_at <= now).count();
                for reminder in reminders {
                    commands::reminder::schedule_reminder(app.handle().clone(), reminder);
                }
                if total > 0 {
                    log::info!(
                        "[reminder] 已恢复 {total} 条提醒（其中 {missed} 条已到期，启动时补发）"
                    );
                }
            }

            log::info!("桌面助手初始化完成");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            toggle_panel,
            quit_app,
            show_bubble_menu,
            commands::ai::get_config,
            commands::ai::save_config,
            commands::ai::list_ai_models,
            commands::ai::probe_ai_config,
            commands::ai::ai_chat,
            commands::ai::stop_generation,
            commands::ai::reset_conversation,
            commands::ai::clear_all_data,
            commands::ai::generate_conversation_title,
            commands::history::save_message,
            commands::history::clear_history_messages,
            commands::history::create_conversation,
            commands::history::list_conversations,
            commands::history::search_messages,
            commands::history::rename_conversation,
            commands::history::delete_conversation,
            commands::history::switch_conversation,
            commands::history::retract_message,
            commands::screenshot::capture_screen,
            commands::screenshot::get_screenshot_data,
            commands::screenshot::submit_screenshot_capture,
            commands::screenshot::cancel_screenshot,
            commands::storage::get_storage_info,
            commands::storage::move_data_dir,
            commands::storage::save_base64_image,
            commands::reminder::create_reminder,
            save_panel_size,
            window::is_foreground_fullscreen,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
