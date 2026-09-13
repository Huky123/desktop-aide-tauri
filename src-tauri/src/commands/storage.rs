use crate::paths;
use crate::state::AppState;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri::State;

/// 存储位置信息（供设置页展示）
#[derive(Clone, Serialize)]
pub struct StorageInfo {
    /// 当前数据目录
    pub data_dir: String,
    /// 默认数据目录
    pub default_dir: String,
    /// 是否使用了自定义位置
    pub is_custom: bool,
}

/// 查询当前存储位置
#[tauri::command]
pub async fn get_storage_info(state: State<'_, AppState>) -> Result<StorageInfo, String> {
    Ok(StorageInfo {
        data_dir: state.data_dir.to_string_lossy().to_string(),
        default_dir: paths::default_data_dir().to_string_lossy().to_string(),
        is_custom: !paths::same_dir(&state.data_dir, &paths::default_data_dir()),
    })
}

/// 迁移数据目录：复制 config.json / chat_history.db / images/ 到新位置，
/// 更新 bootstrap 注册表后重启应用。
/// 复制而非移动 —— 迁移失败或用户反悔时源数据始终保留。
#[tauri::command]
pub async fn move_data_dir(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    target: String,
) -> Result<(), String> {
    // ── 1. 目标目录校验 ──
    let target = target.trim().trim_matches('"').to_string();
    if target.is_empty() {
        return Err("请选择存储位置".to_string());
    }
    let target_dir = PathBuf::from(&target);
    if !target_dir.is_absolute() {
        return Err("存储位置必须是绝对路径".to_string());
    }
    if paths::same_dir(&target_dir, &state.data_dir) {
        return Err("新位置与当前存储位置相同".to_string());
    }
    // 防止把数据迁移到当前数据目录的子目录内（会产生递归复制）
    if target_dir.starts_with(&state.data_dir) {
        return Err("新位置不能位于当前数据目录内部".to_string());
    }

    // ── 2. AI 响应中禁止迁移（避免复制到半写入状态）──
    if state.agent_busy.load(Ordering::SeqCst) {
        return Err("AI 正在响应中，请稍后再试".to_string());
    }

    // ── 3. 创建目录 + 可写性探测 ──
    fs::create_dir_all(&target_dir).map_err(|e| format!("创建目标目录失败: {e}"))?;
    let probe = target_dir.join(".desktop-aide-write-probe");
    fs::write(&probe, b"ok")
        .map_err(|e| format!("目标位置不可写 ({e})，请选择其他目录（如非系统盘的普通文件夹）"))?;
    let _ = fs::remove_file(&probe);

    let source_dir = &state.data_dir;

    // ── 4. 复制数据（保持 db 锁，确保数据库快照一致）──
    // config.json
    let config_src = source_dir.join("config.json");
    if config_src.exists() {
        fs::copy(&config_src, target_dir.join("config.json"))
            .map_err(|e| format!("复制配置文件失败: {e}"))?;
    }

    // chat_history.db（先 WAL checkpoint 再复制）
    {
        let db = state.database.lock().await;
        db.checkpoint_and_copy_to(&target_dir)?;
    }

    // images/
    let images_src = source_dir.join("images");
    if images_src.is_dir() {
        copy_dir_recursive(&images_src, &target_dir.join("images"))?;
    }

    // ── 5. 写入 bootstrap 注册表 ──
    paths::save_bootstrap(&target_dir)?;

    log::info!(
        "[storage] 数据目录已迁移: {} → {}",
        source_dir.display(),
        target_dir.display()
    );

    // ── 6. 重启应用使新路径生效（restart 返回 !，永不返回）──
    tauri::process::restart(&app.env())
}

/// 将 base64 图片数据写入用户选择的路径（灯箱"保存到…"）。
/// 前端已通过 save 对话框确认目标路径，这里只负责解码与写盘。
#[tauri::command]
pub fn save_base64_image(data: String, mime_type: String, path: String) -> Result<(), String> {
    use base64::Engine;

    let path = path.trim().trim_matches('"');
    if path.is_empty() {
        return Err("保存路径为空".to_string());
    }
    let target = std::path::PathBuf::from(path);
    if !target.is_absolute() {
        return Err("保存路径必须是绝对路径".to_string());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("图片数据解码失败: {e}"))?;
    if bytes.is_empty() {
        return Err("图片数据为空".to_string());
    }

    // 按 MIME 补齐扩展名（若用户未提供）
    let mut target = target;
    if target.extension().is_none() {
        let ext = match mime_type.split('/').nth(1) {
            Some("jpeg") => "jpg",
            Some("svg+xml") => "svg",
            Some(other) if !other.is_empty() => other,
            _ => "png",
        };
        target.set_extension(ext);
    }

    std::fs::write(&target, &bytes)
        .map_err(|e| format!("写入图片失败 ({}): {e}", target.display()))?;
    log::info!(
        "[storage] 保存图片到用户路径: {} ({} bytes)",
        target.display(),
        bytes.len()
    );
    Ok(())
}

/// 递归复制目录（仅文件与目录，跳过符号链接等特殊项）
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建图片目录失败: {e}"))?;
    let entries = fs::read_dir(src).map_err(|e| format!("读取源图片目录失败: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取文件类型失败: {e}"))?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target).map_err(|e| {
                format!("复制图片 {} 失败: {e}", entry.file_name().to_string_lossy())
            })?;
        }
    }
    Ok(())
}
