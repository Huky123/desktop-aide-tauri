use base64::Engine;
use std::path::PathBuf;

/// 图片文件存储管理器
///
/// 图片存储在数据目录的 `images/` 子目录下（默认 `%APPDATA%/DesktopAide/images`，
/// 用户可在设置中自定义数据目录），文件名格式：`{uuid}.{ext}`
pub struct ImageStore {
    root: PathBuf,
}

/// 图片元数据记录（存入 SQLite images 表）
#[derive(Debug, Clone)]
pub struct ImageRecord {
    pub id: String,
    pub path: String,
    pub mime_type: String,
    pub size: u64,
}

impl ImageStore {
    /// 创建 ImageStore，在数据目录下建立 `images/` 存储目录。
    /// 自动迁移旧版（开发运行时）保存在可执行文件目录下的图片。
    pub fn new(data_dir: &std::path::Path) -> Result<Self, String> {
        let root = data_dir.join("images");
        std::fs::create_dir_all(&root).map_err(|e| format!("创建图片存储目录失败: {e}"))?;

        // 从旧版可执行文件目录迁移已有图片（早期版本曾把图片存在 exe 同级目录）。
        // 数据库中的文件名引用无需变更，改的是根目录。
        if let Ok(exe) = std::env::current_exe() {
            let legacy_root = exe.parent().map(|p| p.join("images")).unwrap_or_default();
            if legacy_root != root && legacy_root.is_dir() {
                let mut migrated = 0usize;
                if let Ok(entries) = std::fs::read_dir(&legacy_root) {
                    for entry in entries.flatten() {
                        if !entry
                            .file_type()
                            .map(|kind| kind.is_file())
                            .unwrap_or(false)
                        {
                            continue;
                        }
                        let destination = root.join(entry.file_name());
                        if !destination.exists()
                            && std::fs::copy(entry.path(), &destination).is_ok()
                        {
                            migrated += 1;
                        }
                    }
                }
                if migrated > 0 {
                    log::info!("[ImageStore] 已迁移 {migrated} 张旧版图片到数据目录");
                }
            }
        }

        Ok(Self { root })
    }

    /// 返回图片存储根目录，供 Tauri asset protocol 精确授权。
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// 从 base64 数据保存图片到文件。
    /// 返回 ImageRecord（含文件路径和元数据）。
    pub fn save_image(&self, base64_data: &str, mime_type: &str) -> Result<ImageRecord, String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| format!("base64 解码失败: {e}"))?;

        let ext = mime_to_ext(mime_type);
        let id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{}.{}", id, ext);
        let file_path = self.root.join(&filename);

        std::fs::write(&file_path, &bytes).map_err(|e| format!("写入图片文件失败: {e}"))?;

        log::info!(
            "[ImageStore] 保存图片: {} ({} bytes, {})",
            filename,
            bytes.len(),
            mime_type
        );

        Ok(ImageRecord {
            id,
            path: filename.clone(),
            mime_type: mime_type.to_string(),
            size: bytes.len() as u64,
        })
    }

    /// 读取图片文件并转换为 data URI。
    pub fn load_as_data_uri(&self, path: &str) -> Result<String, String> {
        let file_path = self.root.join(path);
        let bytes =
            std::fs::read(&file_path).map_err(|e| format!("读取图片文件失败 ({}): {e}", path))?;
        let mime = ext_to_mime(path).unwrap_or("image/png");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(format!("data:{};base64,{}", mime, b64))
    }

    /// 返回图片文件的绝对路径，供 Tauri asset protocol 转换为 WebView URL。
    pub fn absolute_path(&self, path: &str) -> String {
        self.root.join(path).to_string_lossy().into_owned()
    }

    /// 删除图片文件。
    pub fn delete_image(&self, path: &str) {
        let file_path = self.root.join(path);
        if let Err(e) = std::fs::remove_file(&file_path) {
            log::warn!("[ImageStore] 删除图片文件失败 ({}): {e}", path);
        } else {
            log::info!("[ImageStore] 已删除图片: {}", path);
        }
    }

    /// 删除多个图片文件（用于消息撤回/对话删除时批量清理）。
    pub fn delete_images(&self, paths: &[String]) {
        for path in paths {
            self.delete_image(path);
        }
    }

    /// 删除 images 目录下所有图片文件（用于 clear_all_data）。
    /// 不会删除目录本身。
    pub fn delete_all_images(&self) {
        match std::fs::read_dir(&self.root) {
            Ok(entries) => {
                let mut count = 0;
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && std::fs::remove_file(entry.path()).is_ok()
                    {
                        count += 1;
                    }
                }
                if count > 0 {
                    log::info!("[ImageStore] 已清除全部 {} 张图片", count);
                }
            }
            Err(e) => {
                log::warn!("[ImageStore] 读取图片目录失败: {e}");
            }
        }
    }
}

/// MIME 类型 → 文件扩展名
fn mime_to_ext(mime_type: &str) -> &str {
    match mime_type.to_lowercase().as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => {
            // 尝试从 MIME 中提取子类型
            if let Some(sub) = mime_type.split('/').nth(1) {
                if sub.len() <= 5 && sub.chars().all(|c| c.is_alphanumeric()) {
                    return sub;
                }
            }
            "png"
        }
    }
}

/// 文件扩展名 → MIME 类型（用于读取时重建 data URI）
fn ext_to_mime(path: &str) -> Option<&str> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    })
}
