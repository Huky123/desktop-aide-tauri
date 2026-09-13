use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 数据目录解析（bootstrap 注册表模式）。
///
/// `%APPDATA%/DesktopAide/bootstrap.json` 是唯一的"注册表"文件，
/// 记录用户自定义的数据目录；config.json、chat_history.db、images/
/// 全部位于数据目录内。默认数据目录 = `%APPDATA%/DesktopAide`。
/// 迁移时复制全部文件后再更新 bootstrap 并重启应用，实现真正的便携迁移。
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// 当前生效的数据目录（绝对路径）
    pub data_dir: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Bootstrap {
    /// 自定义数据目录；空字符串表示使用默认位置
    #[serde(default)]
    data_dir: String,
}

impl AppPaths {
    /// 启动时解析数据目录（读取 bootstrap，失败或为空则回落到默认位置）
    pub fn resolve() -> Self {
        Self {
            data_dir: resolve_data_dir(),
        }
    }
}

/// `%APPDATA%/DesktopAide` —— 注册表文件与默认数据目录所在位置
pub fn app_root() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("DesktopAide")
}

/// 默认数据目录 = app_root()
pub fn default_data_dir() -> PathBuf {
    app_root()
}

fn bootstrap_file() -> PathBuf {
    app_root().join("bootstrap.json")
}

/// 解析当前数据目录：读取 bootstrap.json，无效则回落到默认位置
pub fn resolve_data_dir() -> PathBuf {
    let content = match fs::read_to_string(bootstrap_file()) {
        Ok(c) => c,
        Err(_) => return default_data_dir(),
    };
    let bootstrap: Bootstrap = match serde_json::from_str(&content) {
        Ok(b) => b,
        Err(_) => return default_data_dir(),
    };
    let dir = PathBuf::from(bootstrap.data_dir.trim());
    if !bootstrap.data_dir.trim().is_empty() && dir.is_absolute() {
        dir
    } else {
        default_data_dir()
    }
}

/// 将数据目录写入 bootstrap.json（原子写入：临时文件 + rename）。
/// 写入默认位置时记为 ""（空串表示默认），保持注册表文件最小化。
pub fn save_bootstrap(data_dir: &Path) -> Result<(), String> {
    let root = app_root();
    fs::create_dir_all(&root).map_err(|e| format!("创建应用目录失败: {e}"))?;
    let stored = if same_dir(data_dir, &default_data_dir()) {
        String::new()
    } else {
        data_dir.to_string_lossy().to_string()
    };
    let bootstrap = Bootstrap { data_dir: stored };
    let content =
        serde_json::to_string_pretty(&bootstrap).map_err(|e| format!("序列化失败: {e}"))?;
    let target = bootstrap_file();
    let tmp = target.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| format!("写入注册表失败: {e}"))?;
    fs::rename(&tmp, &target).map_err(|e| format!("替换注册表失败: {e}"))?;
    Ok(())
}

/// Windows 下路径比较忽略大小写
pub fn same_dir(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}
