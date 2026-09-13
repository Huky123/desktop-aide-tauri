use crate::ai_service::agent::Agent;
use crate::config::manager::ConfigManager;
use crate::db::Database;
use crate::image_store::ImageStore;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 应用全局状态
pub struct AppState {
    pub config_manager: Mutex<ConfigManager>,
    pub agent: Arc<RwLock<Agent>>,
    pub agent_busy: AtomicBool,
    /// SQLite 聊天记录数据库（Arc 共享：工具执行器等需要并发访问）
    pub database: Arc<tokio::sync::Mutex<Database>>,
    /// 当前活跃的对话 ID
    pub current_conversation_id: Mutex<String>,
    /// 图片文件存储管理器
    pub image_store: Mutex<ImageStore>,
    /// 当前数据目录（config.json / chat_history.db / images/ 的根）
    pub data_dir: PathBuf,
    /// 最近一次全屏截图数据（供独立截图窗口拉取）
    pub screenshot_data: Mutex<Option<ScreenshotData>>,
    /// 当前 AI 请求的取消通道（停止生成时 send，请求结束时 take）
    pub ai_cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

/// 全屏截图数据（PNG base64 + 物理尺寸）
#[derive(Clone, serde::Serialize)]
pub struct ScreenshotData {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
}
