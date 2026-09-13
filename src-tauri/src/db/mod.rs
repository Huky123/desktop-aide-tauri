use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn derive_conversation_title(first_message: &str) -> String {
    let lines: Vec<&str> = first_message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let candidate = lines
        .iter()
        .copied()
        .find(|line| !line.starts_with("[图片:") && !line.starts_with("[文件:"))
        .or_else(|| lines.first().copied())
        .unwrap_or("");

    let friendly = if let Some(name) = candidate
        .strip_prefix("[文件:")
        .and_then(|value| value.strip_suffix(']'))
    {
        format!("文件 · {}", name.trim())
    } else if let Some(name) = candidate
        .strip_prefix("[图片:")
        .and_then(|value| value.strip_suffix(']'))
    {
        format!("图片 · {}", name.trim())
    } else {
        candidate.split_whitespace().collect::<Vec<_>>().join(" ")
    };

    friendly.chars().take(30).collect()
}

#[cfg(test)]
mod tests {
    use super::{derive_conversation_title, Database, ImageRef, MessageDto};

    /// 打开一个临时目录下的数据库（测试专用）
    fn temp_db() -> (Database, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("desktop-aide-db-test-{}", uuid::Uuid::new_v4()));
        let db = Database::open(&dir).expect("打开测试数据库失败");
        (db, dir)
    }

    fn message(id: &str, role: &str, content: &str, ts: i64) -> MessageDto {
        MessageDto {
            id: id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: ts,
            attachments: None,
            local_image_paths: None,
        }
    }

    #[test]
    fn derives_title_from_first_text_line() {
        assert_eq!(
            derive_conversation_title("  分析这份报告  \n[文件: report.md]"),
            "分析这份报告"
        );
    }

    #[test]
    fn derives_friendly_attachment_title() {
        assert_eq!(
            derive_conversation_title("[文件: report.md]"),
            "文件 · report.md"
        );
        assert_eq!(
            derive_conversation_title("[图片: screenshot.png]"),
            "图片 · screenshot.png"
        );
    }

    /// 事务版保存：消息 + 图片引用 + 时间戳 + 自动标题一次性落库
    #[test]
    fn save_message_with_images_persists_everything_atomically() {
        let (db, dir) = temp_db();
        let now = chrono::Utc::now().timestamp_millis();
        db.create_conversation("c1", "新对话", now)
            .expect("创建对话应成功");

        let refs = vec![
            ImageRef {
                id: "img-1".into(),
                msg_id: "m1".into(),
                path: "images/a.png".into(),
                mime_type: "image/png".into(),
                size: 10,
            },
            ImageRef {
                id: "img-2".into(),
                msg_id: "m1".into(),
                path: "images/b.png".into(),
                mime_type: "image/png".into(),
                size: 20,
            },
        ];
        db.save_message_with_images(
            "c1",
            &message("m1", "user", "分析这份报告", now),
            &refs,
            now,
        )
        .expect("事务保存应成功");

        assert_eq!(db.load_messages("c1").expect("加载消息").len(), 1);
        assert_eq!(db.load_image_refs("m1").expect("加载图片引用").len(), 2);
        let conv = db
            .list_conversations()
            .expect("列出对话")
            .into_iter()
            .find(|c| c.id == "c1")
            .expect("应存在对话 c1");
        assert_eq!(conv.title, "分析这份报告", "首条用户消息应自动标题");
        assert_eq!(conv.updated_at, now, "应更新会话时间戳");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// 清除全部数据后应仍有默认对话（事务保证不会留下空库）
    #[test]
    fn clear_all_data_rebuilds_default_conversation() {
        let (db, dir) = temp_db();
        let now = chrono::Utc::now().timestamp_millis();
        db.save_message_with_images("default", &message("m1", "user", "hi", now), &[], now)
            .expect("保存应成功");
        db.clear_all_data().expect("清除应成功");

        assert!(db.load_messages("default").expect("加载消息").is_empty());
        let conversations = db.list_conversations().expect("列出对话");
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].id, "default");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// 撤回删除：消息与图片引用应一并删除（同一事务）
    #[test]
    fn delete_messages_from_timestamp_removes_refs() {
        let (db, dir) = temp_db();
        let now = chrono::Utc::now().timestamp_millis();
        db.create_conversation("c1", "新对话", now).expect("建对话");
        let refs = vec![ImageRef {
            id: "img-1".into(),
            msg_id: "m1".into(),
            path: "images/a.png".into(),
            mime_type: "image/png".into(),
            size: 10,
        }];
        db.save_message_with_images("c1", &message("m1", "user", "问题", now), &refs, now)
            .expect("保存应成功");

        let deleted = db
            .delete_messages_from_timestamp("c1", now)
            .expect("删除应成功");
        assert_eq!(deleted, 1);
        assert!(db.load_messages("c1").expect("加载消息").is_empty());
        assert!(db.load_image_refs("m1").expect("加载引用").is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }
}

/// 前端传递的附件 DTO（与前端 Attachment 类型对应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentJson {
    #[serde(rename = "type")]
    pub att_type: String,
    pub id: String,
    #[serde(default)]
    pub data: String,
    #[serde(default)]
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub source: Option<String>,
}

/// 前端传递的消息 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(default)]
    pub attachments: Option<Vec<AttachmentJson>>,
    #[serde(
        default,
        rename = "localImagePaths",
        skip_serializing_if = "Option::is_none"
    )]
    pub local_image_paths: Option<std::collections::HashMap<String, String>>,
}

/// 对话列表项 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDto {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// 最后一条消息的预览（前 60 字符）
    #[serde(default)]
    pub preview: String,
    /// 消息总数
    #[serde(default)]
    pub message_count: u32,
}

/// 图片文件引用（存储在 images 表中，实际文件在磁盘上）
#[derive(Debug, Clone)]
pub struct ImageRef {
    pub id: String,
    pub msg_id: String,
    pub path: String,
    pub mime_type: String,
    pub size: i64,
}

/// 消息搜索结果条目
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub conversation_id: String,
    pub title: String,
    /// 匹配消息的预览（关键词附近上下文）
    pub preview: String,
    pub timestamp: i64,
}

/// 定时提醒条目
#[derive(Debug, Clone, serde::Serialize)]
pub struct Reminder {
    pub id: String,
    /// 触发时间（毫秒时间戳）
    pub remind_at: i64,
    pub content: String,
    pub created_at: i64,
}

/// 截取关键词附近的上下文作为预览（字符安全，最多 max_chars 字符）
fn preview_around(content: &str, query: &str, max_chars: usize) -> String {
    let Some(byte_pos) = content.find(query) else {
        return content.chars().take(max_chars).collect();
    };
    let char_pos = content[..byte_pos].chars().count();
    let total_chars = content.chars().count();
    let start_char = char_pos.saturating_sub(15);
    let end_char = (char_pos + query.chars().count() + 25).min(total_chars);
    let preview: String = content
        .chars()
        .skip(start_char)
        .take(end_char - start_char)
        .collect();
    let mut out = String::new();
    if start_char > 0 {
        out.push('…');
    }
    out.push_str(&preview);
    if end_char < total_chars {
        out.push('…');
    }
    out
}

/// 插入单条消息（供 `save_message` 与事务版共用）
fn insert_message_row(conn: &Connection, conv_id: &str, msg: &MessageDto) -> Result<(), String> {
    let attachments_json = msg
        .attachments
        .as_ref()
        .filter(|a| !a.is_empty())
        .map(|a| serde_json::to_string(a).unwrap_or_default());

    conn.execute(
        "INSERT OR REPLACE INTO messages (id, conversation_id, role, content, attachments_json, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            msg.id,
            conv_id,
            msg.role,
            msg.content,
            attachments_json,
            msg.timestamp,
        ],
    )
    .map_err(|e| format!("保存消息失败: {e}"))?;
    Ok(())
}

/// 插入图片引用行（供 `save_image_ref` 与事务版共用）
fn insert_image_ref_row(
    conn: &Connection,
    id: &str,
    msg_id: &str,
    path: &str,
    mime_type: &str,
    size: i64,
    created_at: i64,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO images (id, msg_id, path, mime_type, size, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, msg_id, path, mime_type, size, created_at],
    )
    .map_err(|e| format!("保存图片引用失败: {e}"))?;
    Ok(())
}

/// 更新会话 updated_at（供 `touch_conversation` 与事务版共用）
fn touch_conversation_row(conn: &Connection, id: &str, now: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| format!("更新时间戳失败: {e}"))?;
    Ok(())
}

/// 首条用户消息自动标题（供 `auto_title_conversation` 与事务版共用）
fn auto_title_row(conn: &Connection, id: &str, first_message: &str) -> Result<(), String> {
    let title = derive_conversation_title(first_message);
    if title.is_empty() {
        return Ok(());
    }
    conn.execute(
        "UPDATE conversations SET title = ?1
         WHERE id = ?2 AND title = '新对话'",
        params![title, id],
    )
    .map_err(|e| format!("自动标题失败: {e}"))?;
    Ok(())
}

/// SQLite 数据库管理器
pub struct Database {
    conn: Mutex<Connection>,
    /// 数据库文件路径（用于迁移复制）
    db_path: PathBuf,
}

impl Database {
    /// 打开（或创建）数据目录下的数据库，自动建表
    pub fn open(data_dir: &std::path::Path) -> Result<Self, String> {
        let db_path = data_dir.join("chat_history.db");
        std::fs::create_dir_all(data_dir).map_err(|e| format!("创建数据目录失败: {e}"))?;

        let conn = Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {e}"))?;

        // WAL 模式提升并发性能
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("设置 WAL 模式失败: {e}"))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '新对话',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL DEFAULT 'default',
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                attachments_json TEXT,
                timestamp INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS images (
                id TEXT PRIMARY KEY,
                msg_id TEXT NOT NULL,
                path TEXT NOT NULL,
                mime_type TEXT NOT NULL DEFAULT 'image/png',
                size INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reminders (
                id TEXT PRIMARY KEY,
                remind_at INTEGER NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reminders_at
                ON reminders(remind_at);
            CREATE INDEX IF NOT EXISTS idx_messages_conv
                ON messages(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_messages_ts
                ON messages(conversation_id, timestamp);
            CREATE INDEX IF NOT EXISTS idx_images_msg
                ON images(msg_id);",
        )
        .map_err(|e| format!("建表失败: {e}"))?;

        let db = Self {
            conn: Mutex::new(conn),
            db_path,
        };

        // 确保默认对话存在
        db.ensure_default_conversation()?;

        Ok(db)
    }

    /// 在持有数据库锁期间对 WAL 做 checkpoint 并复制数据库文件到目标目录。
    /// 供"迁移存储位置"使用：保证复制出的文件是一致且完整的快照。
    pub fn checkpoint_and_copy_to(&self, target_dir: &Path) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

        let dest = target_dir.join("chat_history.db");
        std::fs::copy(&self.db_path, &dest).map_err(|e| format!("复制数据库文件失败: {e}"))?;

        // 防御性复制 WAL/SHM 副作用文件（checkpoint 后通常为空或已删除）
        for suffix in ["-wal", "-shm"] {
            let side_file = PathBuf::from(format!("{}{}", self.db_path.display(), suffix));
            if side_file.exists() {
                let _ = std::fs::copy(
                    &side_file,
                    PathBuf::from(format!("{}{}", dest.display(), suffix)),
                );
            }
        }
        Ok(())
    }

    // ── 对话元数据操作 ──

    /// 确保 conversations 表中至少有一条默认对话
    fn ensure_default_conversation(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;

        let exists: bool = conn
            .query_row("SELECT COUNT(*) > 0 FROM conversations", [], |row| {
                row.get(0)
            })
            .map_err(|e| format!("查询对话表失败: {e}"))?;

        if !exists {
            let now = chrono::Utc::now().timestamp_millis();
            // 尝试从现有 messages 获取最早/最晚时间戳
            let (earliest, latest) = conn
                .query_row(
                    "SELECT COALESCE(MIN(timestamp), ?1), COALESCE(MAX(timestamp), ?1)
                     FROM messages WHERE conversation_id = 'default'",
                    params![now],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .unwrap_or((now, now));

            conn.execute(
                "INSERT INTO conversations (id, title, created_at, updated_at)
                 VALUES ('default', '默认对话', ?1, ?2)",
                params![earliest, latest],
            )
            .map_err(|e| format!("创建默认对话失败: {e}"))?;
        }

        Ok(())
    }

    /// 创建新对话
    pub fn create_conversation(
        &self,
        id: &str,
        title: &str,
        now: i64,
    ) -> Result<ConversationDto, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)",
            params![id, title, now],
        )
        .map_err(|e| format!("创建对话失败: {e}"))?;

        Ok(ConversationDto {
            id: id.to_string(),
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            preview: String::new(),
            message_count: 0,
        })
    }

    /// 列出所有对话（按更新时间倒序）
    pub fn list_conversations(&self) -> Result<Vec<ConversationDto>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.title, c.created_at, c.updated_at,
                        (SELECT content FROM messages m2
                         WHERE m2.conversation_id = c.id
                         ORDER BY m2.timestamp DESC LIMIT 1) AS preview,
                        (SELECT COUNT(*) FROM messages m3
                         WHERE m3.conversation_id = c.id) AS message_count
                 FROM conversations c
                 ORDER BY c.updated_at DESC",
            )
            .map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let preview_raw: Option<String> = row.get(4)?;
                // 预览只取前 60 字符
                let preview = preview_raw
                    .unwrap_or_default()
                    .chars()
                    .take(60)
                    .collect::<String>();
                Ok(ConversationDto {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    preview,
                    message_count: row.get(5).unwrap_or(0),
                })
            })
            .map_err(|e| format!("查询对话列表失败: {e}"))?;

        let mut convs = Vec::new();
        for row in rows {
            convs.push(row.map_err(|e| format!("读取行失败: {e}"))?);
        }
        Ok(convs)
    }

    /// 重命名对话
    pub fn rename_conversation(&self, id: &str, title: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        conn.execute(
            "UPDATE conversations SET title = ?1 WHERE id = ?2",
            params![title, id],
        )
        .map_err(|e| format!("重命名对话失败: {e}"))?;
        Ok(())
    }

    /// 删除对话及其所有消息（事务）。
    /// 注意：调用方应先通过 delete_image_refs_for_conv 清理图片文件和引用，
    /// 此处的 DELETE FROM images 是防御性清理（防止调用方遗漏导致孤儿行）。
    pub fn delete_conversation(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        // 防御性清理：如果调用方已通过 delete_image_refs_for_conv 删除，此处为 no-op
        conn.execute(
            "DELETE FROM images WHERE msg_id IN (
                SELECT id FROM messages WHERE conversation_id = ?1
            )",
            params![id],
        )
        .map_err(|e| format!("删除对话图片引用失败: {e}"))?;
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![id],
        )
        .map_err(|e| format!("删除对话消息失败: {e}"))?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])
            .map_err(|e| format!("删除对话失败: {e}"))?;
        Ok(())
    }

    /// 删除尚未产生任何消息的临时对话。
    pub fn delete_conversation_if_empty(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM conversations
                 WHERE id = ?1
                   AND NOT EXISTS (SELECT 1 FROM messages WHERE conversation_id = ?1)",
                params![id],
            )
            .map_err(|e| format!("清理空对话失败: {e}"))?;
        Ok(deleted > 0)
    }

    /// 清理上次异常退出或直接关闭应用后遗留的所有空对话。
    pub fn delete_all_empty_conversations(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        conn.execute(
            "DELETE FROM conversations
             WHERE NOT EXISTS (
                 SELECT 1 FROM messages WHERE messages.conversation_id = conversations.id
             )",
            [],
        )
        .map_err(|e| format!("清理空对话失败: {e}"))
    }

    /// 更新对话的 updated_at 时间戳
    pub fn touch_conversation(&self, id: &str, now: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        touch_conversation_row(&conn, id, now)
    }

    // ── 消息 CRUD ──

    /// 原子保存「消息 + 图片引用 + 会话时间戳（+ 首条用户消息自动标题）」。
    ///
    /// 之前 save_message / save_image_ref / touch_conversation / auto_title_conversation
    /// 是四条独立语句，中途失败会留下"消息已写入但图片引用缺失"（前端表现为图片 404）
    /// 的坏记录。这里用事务包起来，任一步失败整体回滚。
    pub fn save_message_with_images(
        &self,
        conv_id: &str,
        msg: &MessageDto,
        image_refs: &[ImageRef],
        now: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("开启事务失败: {e}"))?;

        insert_message_row(&tx, conv_id, msg)?;
        for image in image_refs {
            insert_image_ref_row(
                &tx,
                &image.id,
                &image.msg_id,
                &image.path,
                &image.mime_type,
                image.size,
                now,
            )?;
        }
        touch_conversation_row(&tx, conv_id, now)?;
        if msg.role == "user" {
            auto_title_row(&tx, conv_id, &msg.content)?;
        }

        tx.commit().map_err(|e| format!("提交事务失败: {e}"))?;
        Ok(())
    }

    /// 加载指定会话的所有消息（按时间戳排序）
    pub fn load_messages(&self, conv_id: &str) -> Result<Vec<MessageDto>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, role, content, attachments_json, timestamp
                 FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt
            .query_map(params![conv_id], |row| {
                let attachments_json: Option<String> = row.get(3)?;
                let attachments: Option<Vec<AttachmentJson>> = attachments_json
                    .filter(|j| !j.is_empty())
                    .and_then(|j| serde_json::from_str(&j).ok());

                Ok(MessageDto {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    attachments,
                    local_image_paths: None,
                    timestamp: row.get(4)?,
                })
            })
            .map_err(|e| format!("查询消息失败: {e}"))?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|e| format!("读取行失败: {e}"))?);
        }

        Ok(messages)
    }

    /// 删除指定会话的所有消息。
    /// 注意：调用方应先通过 delete_image_refs_for_conv 清理图片文件和引用。
    pub fn clear_messages(&self, conv_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("开启事务失败: {e}"))?;
        // 防御性清理图片引用
        tx.execute(
            "DELETE FROM images WHERE msg_id IN (
                SELECT id FROM messages WHERE conversation_id = ?1
            )",
            params![conv_id],
        )
        .map_err(|e| format!("清除图片引用失败: {e}"))?;
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![conv_id],
        )
        .map_err(|e| format!("清除消息失败: {e}"))?;
        tx.commit().map_err(|e| format!("提交事务失败: {e}"))?;
        Ok(())
    }

    /// 删除指定会话中从给定时间戳开始的所有消息（用于消息撤回）。
    /// 返回删除的消息数量。
    /// 注意：调用方应先通过 delete_image_refs_for_msg 清理每个受影响消息的图片文件和引用。
    pub fn delete_messages_from_timestamp(
        &self,
        conv_id: &str,
        from_ts: i64,
    ) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("开启事务失败: {e}"))?;
        // 防御性清理图片引用
        tx.execute(
            "DELETE FROM images WHERE msg_id IN (
                SELECT id FROM messages WHERE conversation_id = ?1 AND timestamp >= ?2
            )",
            params![conv_id, from_ts],
        )
        .map_err(|e| format!("删除图片引用失败: {e}"))?;
        let deleted = tx
            .execute(
                "DELETE FROM messages WHERE conversation_id = ?1 AND timestamp >= ?2",
                params![conv_id, from_ts],
            )
            .map_err(|e| format!("删除消息失败: {e}"))?;
        tx.commit().map_err(|e| format!("提交事务失败: {e}"))?;
        Ok(deleted)
    }

    /// 清除所有对话和消息数据，重建默认对话。
    /// 用于"设置 → 清除所有数据"功能。
    /// 事务保证：不会出现"已删除但默认对话未重建"的空库状态。
    pub fn clear_all_data(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("开启事务失败: {e}"))?;
        tx.execute_batch(
            "DELETE FROM images;
             DELETE FROM messages;
             DELETE FROM conversations;
             INSERT INTO conversations (id, title, created_at, updated_at)
             VALUES ('default', '默认对话', strftime('%s','now') * 1000, strftime('%s','now') * 1000);",
        )
        .map_err(|e| format!("清除所有数据失败: {e}"))?;
        tx.commit().map_err(|e| format!("提交事务失败: {e}"))?;
        Ok(())
    }

    // ── 图片文件引用 CRUD ──

    /// 加载指定消息的所有图片引用
    pub fn load_image_refs(&self, msg_id: &str) -> Result<Vec<ImageRef>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, msg_id, path, mime_type, size
                 FROM images WHERE msg_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|e| format!("准备查询失败: {e}"))?;

        let rows = stmt
            .query_map(params![msg_id], |row| {
                Ok(ImageRef {
                    id: row.get(0)?,
                    msg_id: row.get(1)?,
                    path: row.get(2)?,
                    mime_type: row.get(3)?,
                    size: row.get(4)?,
                })
            })
            .map_err(|e| format!("查询图片引用失败: {e}"))?;

        let mut refs = Vec::new();
        for row in rows {
            refs.push(row.map_err(|e| format!("读取行失败: {e}"))?);
        }
        Ok(refs)
    }

    /// 删除指定消息的图片引用
    pub fn delete_image_refs_for_msg(&self, msg_id: &str) -> Result<Vec<ImageRef>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;

        // 先查询要删除的引用（用于后续文件清理）
        let refs = {
            let mut stmt = conn
                .prepare("SELECT id, msg_id, path, mime_type, size FROM images WHERE msg_id = ?1")
                .map_err(|e| format!("准备查询失败: {e}"))?;
            let rows = stmt
                .query_map(params![msg_id], |row| {
                    Ok(ImageRef {
                        id: row.get(0)?,
                        msg_id: row.get(1)?,
                        path: row.get(2)?,
                        mime_type: row.get(3)?,
                        size: row.get(4)?,
                    })
                })
                .map_err(|e| format!("查询图片引用失败: {e}"))?;
            let mut refs = Vec::new();
            for row in rows {
                refs.push(row.map_err(|e| format!("读取行失败: {e}"))?);
            }
            refs
        };

        conn.execute("DELETE FROM images WHERE msg_id = ?1", params![msg_id])
            .map_err(|e| format!("删除图片引用失败: {e}"))?;

        Ok(refs)
    }

    /// 按关键词搜索消息（LIKE 匹配 content，含所属会话标题），按时间倒序返回最多 50 条
    pub fn search_messages(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let pattern = format!("%{}%", query);
        let mut stmt = conn
            .prepare(
                "SELECT m.conversation_id, c.title, m.content, m.timestamp
                 FROM messages m
                 INNER JOIN conversations c ON c.id = m.conversation_id
                 WHERE m.content LIKE ?1
                 ORDER BY m.timestamp DESC
                 LIMIT 50",
            )
            .map_err(|e| format!("准备查询失败: {e}"))?;
        let rows = stmt
            .query_map(params![pattern], |row| {
                let content: String = row.get(2)?;
                Ok(SearchResult {
                    conversation_id: row.get(0)?,
                    title: row.get(1)?,
                    preview: preview_around(&content, query, 80),
                    timestamp: row.get(3)?,
                })
            })
            .map_err(|e| format!("查询消息失败: {e}"))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("读取行失败: {e}"))?);
        }
        Ok(results)
    }

    /// 删除指定对话的所有图片引用（返回路径列表用于文件清理）
    pub fn delete_image_refs_for_conv(&self, conv_id: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;

        let paths: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT i.path FROM images i
                     INNER JOIN messages m ON i.msg_id = m.id
                     WHERE m.conversation_id = ?1",
                )
                .map_err(|e| format!("准备查询失败: {e}"))?;
            let rows = stmt
                .query_map(params![conv_id], |row| row.get(0))
                .map_err(|e| format!("查询图片路径失败: {e}"))?;
            let mut paths = Vec::new();
            for row in rows {
                paths.push(row.map_err(|e| format!("读取行失败: {e}"))?);
            }
            paths
        };

        conn.execute(
            "DELETE FROM images WHERE msg_id IN (
                SELECT id FROM messages WHERE conversation_id = ?1
            )",
            params![conv_id],
        )
        .map_err(|e| format!("删除对话图片引用失败: {e}"))?;

        Ok(paths)
    }

    // ── 定时提醒 CRUD ──

    /// 创建提醒
    pub fn create_reminder(
        &self,
        id: &str,
        remind_at: i64,
        content: &str,
        now: i64,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        conn.execute(
            "INSERT INTO reminders (id, remind_at, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, remind_at, content, now],
        )
        .map_err(|e| format!("创建提醒失败: {e}"))?;
        Ok(())
    }

    /// 列出所有提醒（按触发时间升序）
    pub fn list_reminders(&self) -> Result<Vec<Reminder>, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, remind_at, content, created_at FROM reminders ORDER BY remind_at ASC",
            )
            .map_err(|e| format!("准备查询失败: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Reminder {
                    id: row.get(0)?,
                    remind_at: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("查询提醒失败: {e}"))?;
        let mut reminders = Vec::new();
        for row in rows {
            reminders.push(row.map_err(|e| format!("读取行失败: {e}"))?);
        }
        Ok(reminders)
    }

    /// 删除提醒（返回是否删除成功）
    pub fn delete_reminder(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("数据库锁失败: {e}"))?;
        let deleted = conn
            .execute("DELETE FROM reminders WHERE id = ?1", params![id])
            .map_err(|e| format!("删除提醒失败: {e}"))?;
        Ok(deleted > 0)
    }
}
