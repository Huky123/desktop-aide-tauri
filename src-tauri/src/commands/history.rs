use crate::db::{AttachmentJson, ConversationDto, ImageRef, MessageDto, SearchResult};
use crate::image_store::ImageStore;
use crate::state::AppState;
use std::collections::HashMap;
use tauri::State;

// ── 图片提取（save 时：base64 → 文件）──

/// 扫描 Markdown 内容中的 data:image URI → 保存为文件 → 替换为 imgref://{id}
fn extract_content_images(
    content: &str,
    msg_id: &str,
    store: &ImageStore,
) -> (String, Vec<ImageRef>) {
    let mut refs = Vec::new();
    let mut out = String::with_capacity(content.len());
    let mut rest = content;

    loop {
        let Some(pos) = rest.find("data:image/") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..pos]);
        let chunk = &rest[pos..];
        let Some(end) = chunk.find(')') else {
            out.push_str(chunk);
            break;
        };
        let data_uri = &chunk[..end];
        if let Some(comma) = data_uri.find(',') {
            let header = &data_uri[..comma];
            let mime = header
                .find(';')
                .map(|s| &header[5..s])
                .unwrap_or("image/png");
            let b64 = &data_uri[comma + 1..];
            match store.save_image(b64, mime) {
                Ok(rec) => {
                    refs.push(ImageRef {
                        id: rec.id.clone(),
                        msg_id: msg_id.to_string(),
                        path: rec.path,
                        mime_type: rec.mime_type,
                        size: rec.size as i64,
                    });
                    out.push_str(&format!("imgref://{}", rec.id));
                }
                Err(e) => {
                    log::warn!("[history] 内容图片保存失败: {e}");
                    out.push_str(data_uri); // 降级保留原 data URI
                }
            }
        } else {
            out.push_str(data_uri);
        }
        // 保留 Markdown 图片语法的右括号；data_uri 本身不包含它。
        rest = &chunk[end..];
    }
    (out, refs)
}

/// 从附件列表中提取 base64 → 文件（data 长度 > 100 判定为 base64）
fn extract_attachment_images(
    atts: &[AttachmentJson],
    msg_id: &str,
    store: &ImageStore,
) -> (Vec<AttachmentJson>, Vec<ImageRef>) {
    let mut refs = Vec::new();
    let new = atts
        .iter()
        .map(|a| {
            if a.att_type == "image" && a.data.len() > 100 {
                match store.save_image(&a.data, &a.mime_type) {
                    Ok(rec) => {
                        let path = rec.path.clone();
                        refs.push(ImageRef {
                            id: rec.id.clone(),
                            msg_id: msg_id.to_string(),
                            path: rec.path,
                            mime_type: rec.mime_type,
                            size: rec.size as i64,
                        });
                        AttachmentJson {
                            data: path,
                            ..a.clone()
                        }
                    }
                    Err(e) => {
                        log::warn!("[history] 附件图片保存失败: {e}");
                        a.clone()
                    }
                }
            } else {
                a.clone()
            }
        })
        .collect();
    (new, refs)
}

// ── 图片重建（load 时：文件引用 → asset protocol 路径）──

fn local_image_paths(img_refs: &[ImageRef], store: &ImageStore) -> HashMap<String, String> {
    img_refs
        .iter()
        .map(|image| (image.id.clone(), store.absolute_path(&image.path)))
        .collect()
}

/// 将附件中的文件路径还原为 base64 data
pub(crate) fn reconstruct_attachment_images(
    atts: &[AttachmentJson],
    store: &ImageStore,
) -> Vec<AttachmentJson> {
    atts.iter()
        .map(|a| {
            // 文件路径特征：长度短（< 100）、包含连字符（UUID 格式）
            if a.att_type == "image" && a.data.len() < 100 && a.data.contains('-') {
                if let Ok(data_uri) = store.load_as_data_uri(&a.data) {
                    let b64 = data_uri
                        .find(',')
                        .map(|p| data_uri[p + 1..].to_string())
                        .unwrap_or_default();
                    return AttachmentJson {
                        data: b64,
                        ..a.clone()
                    };
                }
            }
            a.clone()
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════
// 对话管理
// ═══════════════════════════════════════════════════════════

#[tauri::command]
pub async fn create_conversation(
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<ConversationDto, String> {
    state
        .database
        .lock()
        .await
        .delete_all_empty_conversations()?;
    let id = uuid::Uuid::new_v4().to_string();
    let title = title.unwrap_or_else(|| "新对话".to_string());
    let now = chrono::Utc::now().timestamp_millis();
    let conv = {
        state
            .database
            .lock()
            .await
            .create_conversation(&id, &title, now)?
    };
    state.agent.write().await.reset();
    *state.current_conversation_id.lock().await = id;
    Ok(conv)
}

#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
) -> Result<Vec<ConversationDto>, String> {
    state.database.lock().await.list_conversations()
}

/// 按关键词搜索所有会话的消息内容
#[tauri::command]
pub async fn search_messages(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SearchResult>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    state.database.lock().await.search_messages(query)
}

#[tauri::command]
pub async fn rename_conversation(
    state: State<'_, AppState>,
    conv_id: String,
    new_title: String,
) -> Result<(), String> {
    let t: String = new_title.trim().chars().take(100).collect();
    if t.is_empty() {
        return Err("标题不能为空".to_string());
    }
    state
        .database
        .lock()
        .await
        .rename_conversation(&conv_id, &t)
}

#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, AppState>,
    conv_id: String,
) -> Result<(), String> {
    // 清理图片文件
    let paths = state
        .database
        .lock()
        .await
        .delete_image_refs_for_conv(&conv_id)
        .unwrap_or_default();
    if !paths.is_empty() {
        state.image_store.lock().await.delete_images(&paths);
    }

    let db = state.database.lock().await;
    db.delete_conversation(&conv_id)?;

    let mut current = state.current_conversation_id.lock().await;
    if *current == conv_id {
        let remaining = db.list_conversations()?;
        if let Some(next) = remaining.first() {
            *current = next.id.clone();
            let msgs = db.load_messages(&next.id)?;
            let pairs = pair_user_assistant(&msgs);
            drop(db);
            state.agent.write().await.rebuild_context_from_pairs(&pairs);
        } else {
            let now = chrono::Utc::now().timestamp_millis();
            let new_id = uuid::Uuid::new_v4().to_string();
            db.create_conversation(&new_id, "新对话", now)?;
            *current = new_id;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn switch_conversation(
    state: State<'_, AppState>,
    conv_id: String,
) -> Result<Vec<MessageDto>, String> {
    {
        let db = state.database.lock().await;
        if !db.list_conversations()?.iter().any(|c| c.id == conv_id) {
            return Err(format!("对话 {conv_id} 不存在"));
        }
    }
    let previous_id = state.current_conversation_id.lock().await.clone();
    if previous_id != conv_id {
        state
            .database
            .lock()
            .await
            .delete_conversation_if_empty(&previous_id)?;
    }
    *state.current_conversation_id.lock().await = conv_id.clone();

    // 加载消息 + 图片引用
    let (raw, img_map) = {
        let db = state.database.lock().await;
        let raw = db.load_messages(&conv_id)?;
        let mut map: HashMap<String, Vec<ImageRef>> = HashMap::new();
        for m in &raw {
            if let Ok(r) = db.load_image_refs(&m.id) {
                if !r.is_empty() {
                    map.insert(m.id.clone(), r);
                }
            }
        }
        (raw, map)
    };

    let store = state.image_store.lock().await;
    let messages: Vec<MessageDto> = raw
        .into_iter()
        .map(|m| {
            let local_image_paths = img_map
                .get(&m.id)
                .map(|refs| local_image_paths(refs, &store));
            let attachments = m
                .attachments
                .map(|a| reconstruct_attachment_images(&a, &store));
            MessageDto {
                attachments,
                local_image_paths,
                ..m
            }
        })
        .collect();
    drop(store);

    let pairs = pair_user_assistant(&messages);
    state.agent.write().await.rebuild_context_from_pairs(&pairs);
    Ok(messages)
}

// ═══════════════════════════════════════════════════════════
// 消息持久化（含图片提取/重建）
// ═══════════════════════════════════════════════════════════

#[tauri::command]
pub async fn save_message(
    state: State<'_, AppState>,
    msg: MessageDto,
    conv_id: Option<String>,
) -> Result<MessageDto, String> {
    let now = chrono::Utc::now().timestamp_millis();

    // 步骤 1：提取图片到文件系统
    let (processed_content, processed_atts, all_refs, local_paths) = {
        let store = state.image_store.lock().await;
        let (content, cr) = extract_content_images(&msg.content, &msg.id, &store);
        let (atts, ar) = if let Some(ref a) = msg.attachments {
            extract_attachment_images(a, &msg.id, &store)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut refs = cr;
        refs.extend(ar);
        let paths = local_image_paths(&refs, &store);
        (content, atts, refs, paths)
    };

    // 步骤 2：存入数据库
    let conv_id = match conv_id {
        Some(id) => id,
        None => state.current_conversation_id.lock().await.clone(),
    };
    let db = state.database.lock().await;
    if !db
        .list_conversations()?
        .iter()
        .any(|conversation| conversation.id == conv_id)
    {
        return Err(format!("对话 {conv_id} 不存在"));
    }

    // 构建处理后的 MessageDto
    let db_msg = MessageDto {
        id: msg.id.clone(),
        role: msg.role.clone(),
        content: processed_content,
        attachments: if msg.attachments.is_some() {
            Some(processed_atts)
        } else {
            None
        },
        local_image_paths: None,
        timestamp: msg.timestamp,
    };

    // 消息 + 图片引用 + 会话时间戳 + 自动标题 在同一事务内落库，
    // 避免中途失败留下"消息存在但图片 404"的坏记录
    db.save_message_with_images(&conv_id, &db_msg, &all_refs, now)?;
    Ok(MessageDto {
        local_image_paths: (!local_paths.is_empty()).then_some(local_paths),
        ..db_msg
    })
}

#[tauri::command]
pub async fn clear_history_messages(
    state: State<'_, AppState>,
    conv_id: Option<String>,
) -> Result<(), String> {
    let conv_id = match conv_id {
        Some(id) => id,
        None => state.current_conversation_id.lock().await.clone(),
    };
    let paths = state
        .database
        .lock()
        .await
        .delete_image_refs_for_conv(&conv_id)
        .unwrap_or_default();
    if !paths.is_empty() {
        state.image_store.lock().await.delete_images(&paths);
    }
    let db = state.database.lock().await;
    db.clear_messages(&conv_id)?;
    db.touch_conversation(&conv_id, chrono::Utc::now().timestamp_millis())?;
    Ok(())
}

#[tauri::command]
pub async fn retract_message(
    state: State<'_, AppState>,
    msg_id: String,
) -> Result<Vec<MessageDto>, String> {
    if state.agent_busy.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("AI 正在响应中，请稍后再试".to_string());
    }

    let conv_id = state.current_conversation_id.lock().await.clone();
    let target_ts = {
        let db = state.database.lock().await;
        db.load_messages(&conv_id)?
            .iter()
            .find(|m| m.id == msg_id)
            .ok_or("未找到要撤回的消息")?
            .timestamp
    };

    // 收集受影响的消息 ID → 清理图片
    let deleted_ids: Vec<String> = {
        let db = state.database.lock().await;
        db.load_messages(&conv_id)?
            .into_iter()
            .filter(|m| m.timestamp >= target_ts)
            .map(|m| m.id)
            .collect()
    };

    {
        let db = state.database.lock().await;
        let store = state.image_store.lock().await;
        for mid in &deleted_ids {
            if let Ok(refs) = db.delete_image_refs_for_msg(mid) {
                store.delete_images(&refs.iter().map(|r| r.path.clone()).collect::<Vec<_>>());
            }
        }
    }

    // 删除消息
    let db = state.database.lock().await;
    db.delete_messages_from_timestamp(&conv_id, target_ts)?;
    db.touch_conversation(&conv_id, chrono::Utc::now().timestamp_millis())?;
    let remaining = db.load_messages(&conv_id)?;
    if remaining.is_empty() {
        db.rename_conversation(&conv_id, "新对话")?;
    }
    drop(db);

    let pairs = pair_user_assistant(&remaining);
    state.agent.write().await.rebuild_context_from_pairs(&pairs);
    Ok(remaining)
}

// ═══════════════════════════════════════════════════════════
// 辅助
// ═══════════════════════════════════════════════════════════

/// 将 (user, assistant) 消息配对为上下文重建输入。
/// 供切换/撤回/配置变更后恢复 Agent 记忆使用。
///
/// 图片处理策略：文件附件内容注入 `<file>` 块（与旧行为一致）；
/// 图片附件仅对最近 `RECENT_IMAGE_TURNS` 轮注入完整 base64（视觉模型可理解），
/// 更早的轮次只保留 `[图片: name]` 占位 —— 平衡多模态记忆与 token 开销，
/// 避免 base64 长串导致历史被 token 估算快速裁剪。
pub(crate) fn pair_user_assistant(messages: &[MessageDto]) -> Vec<(String, String)> {
    const RECENT_IMAGE_TURNS: usize = 2;

    // 先收集所有 (user, assistant) 配对的位置
    let mut pair_indexes = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        if messages[i].role == "user"
            && i + 1 < messages.len()
            && messages[i + 1].role == "assistant"
        {
            pair_indexes.push(i);
            i += 2;
        } else {
            i += 1;
        }
    }

    let mut pairs = Vec::new();
    for (rank, &user_idx) in pair_indexes.iter().enumerate() {
        // rank 从 0（最旧）到 len-1（最新）；判断是否为最新 RECENT_IMAGE_TURNS 轮
        let is_recent = rank + RECENT_IMAGE_TURNS >= pair_indexes.len();
        let mut user_content = messages[user_idx].content.clone();
        if let Some(attachments) = &messages[user_idx].attachments {
            for attachment in attachments {
                if attachment.att_type == "file" && !attachment.data.is_empty() {
                    user_content.push_str(&format!(
                        "\n\n<file name=\"{}\">\n{}\n</file>",
                        attachment.name, attachment.data
                    ));
                } else if attachment.att_type == "image" {
                    if is_recent && !attachment.data.is_empty() {
                        // 最近轮次：注入完整 base64，视觉模型可理解
                        user_content.push_str(&format!(
                            "\n\n![{}](data:{};base64,{})",
                            attachment.name, attachment.mime_type, attachment.data
                        ));
                    } else {
                        // 较早轮次：仅保留占位
                        user_content.push_str(&format!("\n[图片: {}]", attachment.name));
                    }
                }
            }
        }
        pairs.push((user_content, messages[user_idx + 1].content.clone()));
    }
    pairs
}
