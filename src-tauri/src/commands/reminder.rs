use crate::db::{Database, Reminder};
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::task::JoinHandle;

/// 已调度的提醒任务表：reminder id → 等待触发的 JoinHandle。
///
/// 之前的实现只 `tokio::spawn` 睡到点，`cancel_reminder` 仅删数据库行 →
/// 用户取消后到点仍然弹提醒。现在取消时 abort 对应任务，触发前再用
/// 「删除数据库行」做一次原子认领，双重保险。
fn reminder_tasks() -> &'static Mutex<HashMap<String, JoinHandle<()>>> {
    static TASKS: OnceLock<Mutex<HashMap<String, JoinHandle<()>>>> = OnceLock::new();
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取消某条提醒已调度的任务（若存在）。供 command 与工具执行器共用。
pub(crate) fn cancel_scheduled_reminder(id: &str) {
    let handle = reminder_tasks()
        .lock()
        .ok()
        .and_then(|mut tasks| tasks.remove(id));
    if let Some(handle) = handle {
        handle.abort();
        log::info!("[reminder] 已取消调度任务 (id={id})");
    }
}

/// 从任务表移除自身记录（任务正常结束后清理，避免表无限增长）
fn forget_scheduled_reminder(id: &str) {
    if let Ok(mut tasks) = reminder_tasks().lock() {
        tasks.remove(id);
    }
}

/// 解析时间表达式 → 触发时间戳（毫秒）。
///
/// 支持相对时间：「10秒」「5分钟」「2小时」「1天」（可带"后"）；
/// 支持绝对时间：「14:30」（今天该时刻，已过则明天）。
fn parse_remind_time(expr: &str, now: i64) -> Option<i64> {
    let expr = expr.trim().trim_end_matches('后');

    // 相对时间：数字 + 单位
    let num_end = expr
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(expr.len());
    if num_end > 0 {
        let num: i64 = expr[..num_end].parse().ok()?;
        if num <= 0 {
            return None;
        }
        let unit = expr[num_end..].trim();
        let millis = match unit {
            "秒" | "s" | "sec" => num * 1000,
            "分钟" | "分" | "min" | "m" => num * 60_000,
            "小时" | "时" | "hour" | "h" => num * 3_600_000,
            "天" | "日" | "day" | "d" => num * 86_400_000,
            _ => return None,
        };
        return Some(now + millis);
    }

    // 绝对时间 HH:MM（今天；若已过则顺延到明天）
    if let Some((h, m)) = expr.split_once(':') {
        let hour: u32 = h.trim().parse().ok()?;
        let minute: u32 = m.trim().parse().ok()?;
        if hour >= 24 || minute >= 60 {
            return None;
        }
        let now_local = chrono::Local::now();
        let today = now_local.date_naive();
        if let Some(target) = today
            .and_hms_opt(hour, minute, 0)
            .and_then(|t| t.and_local_timezone(chrono::Local).single())
        {
            let ts = target.timestamp_millis();
            if ts > now {
                return Some(ts);
            }
        }
        // 今天已过 → 明天
        let tomorrow = today + chrono::Days::new(1);
        return tomorrow
            .and_hms_opt(hour, minute, 0)
            .and_then(|t| t.and_local_timezone(chrono::Local).single())
            .map(|dt| dt.timestamp_millis());
    }

    None
}

/// 提醒触发事件载荷（前端监听 `reminder-fired`，方案 A：气泡脉冲唤醒 + 面板内助手消息）。
#[derive(Clone, serde::Serialize)]
pub struct ReminderFiredPayload {
    /// 提醒内容
    pub content: String,
    /// 触发时间（毫秒时间戳）
    pub fired_at: i64,
}

/// 调度提醒：等待到触发时间 → **原子认领**（删除数据库行，删到才说明未被取消）
/// → 广播 `reminder-fired` 事件（应用内展示，不依赖系统通知）。
///
/// 取消语义：`cancel_scheduled_reminder` 会 abort 本任务；即便任务已进入触发阶段，
/// 认领失败（行已被取消删除）也不会触发。
pub(crate) fn schedule_reminder(app: AppHandle, reminder: Reminder) {
    let id = reminder.id.clone();
    let content = reminder.content.clone();
    let remind_at = reminder.remind_at;
    let now = chrono::Utc::now().timestamp_millis();
    // 已过期的提醒（例如应用关闭期间到期）立即补发
    let wait_ms = (remind_at - now).max(0) as u64;

    // 同 id 重复调度时先取消旧任务，避免一条提醒触发两次
    cancel_scheduled_reminder(&id);

    // 注册表 key 单独保留一份（id 会被 move 进任务闭包）
    let registry_id = id.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

        // 原子认领：删到行 = 提醒仍然有效；删不到 = 已被用户取消
        let claimed = match app.try_state::<AppState>() {
            Some(state) => {
                let db = state.database.lock().await;
                match db.delete_reminder(&id) {
                    Ok(removed) => removed,
                    Err(e) => {
                        log::warn!("[reminder] 认领提醒失败，仍按触发处理: {e}");
                        true
                    }
                }
            }
            // 无应用状态（测试环境）：直接触发
            None => true,
        };

        if !claimed {
            log::info!("[reminder] 提醒已被取消，跳过触发 (id={id})");
            forget_scheduled_reminder(&id);
            return;
        }

        // 方案 A：只广播事件，由主窗口前端完成「气泡脉冲 → 面板展开 → 面板内助手消息」
        log::info!("[reminder] 提醒触发: {content}");
        let payload = ReminderFiredPayload {
            content: content.clone(),
            fired_at: chrono::Utc::now().timestamp_millis(),
        };
        let _ = app.emit("reminder-fired", &payload);

        forget_scheduled_reminder(&id);
    });

    if let Ok(mut tasks) = reminder_tasks().lock() {
        tasks.insert(registry_id, handle);
    }
}

/// 创建提醒核心逻辑（无 Tauri 依赖，供 command 与工具执行器复用）：
/// 解析时间表达式 → 校验 → 写入数据库 → 返回 Reminder。
pub(crate) fn create_reminder_core(
    database: &Database,
    time_expr: &str,
    content: &str,
) -> Result<Reminder, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let remind_at = parse_remind_time(time_expr, now)
        .ok_or_else(|| "无法解析时间，请用「10分钟」「2小时」「14:30」等格式".to_string())?;
    if remind_at <= now {
        return Err("提醒时间必须晚于当前时间".to_string());
    }
    let content = if content.trim().is_empty() {
        "（无内容）".to_string()
    } else {
        content.trim().to_string()
    };
    let id = uuid::Uuid::new_v4().to_string();
    database.create_reminder(&id, remind_at, &content, now)?;
    Ok(Reminder {
        id,
        remind_at,
        content,
        created_at: now,
    })
}

/// 创建定时提醒：解析时间表达式 → 存数据库 → 调度通知
#[tauri::command]
pub async fn create_reminder(
    app: AppHandle,
    state: State<'_, AppState>,
    time_expr: String,
    content: String,
) -> Result<Reminder, String> {
    let reminder = {
        let db = state.database.lock().await;
        create_reminder_core(&db, &time_expr, &content)?
    };
    schedule_reminder(app, reminder.clone());
    log::info!(
        "[reminder] 已创建提醒: {} @ {}",
        reminder.content,
        reminder.remind_at
    );
    Ok(reminder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> Database {
        let dir = std::env::temp_dir().join(format!(
            "desktop-aide-reminder-test-{}",
            uuid::Uuid::new_v4()
        ));
        Database::open(&dir).expect("打开测试数据库失败")
    }

    #[test]
    fn core_creates_future_reminder() {
        let db = temp_db();
        let r = create_reminder_core(&db, "10分钟", "喝水").expect("应成功");
        assert!(r.remind_at > chrono::Utc::now().timestamp_millis());
        assert_eq!(r.content, "喝水");
        assert!(!r.id.is_empty());
    }

    #[test]
    fn core_defaults_empty_content() {
        let db = temp_db();
        let r = create_reminder_core(&db, "1小时", "   ").expect("应成功");
        assert_eq!(r.content, "（无内容）");
    }

    #[test]
    fn core_rejects_invalid_or_past_time() {
        let db = temp_db();
        assert!(create_reminder_core(&db, "明天下午", "x").is_err());
        assert!(create_reminder_core(&db, "0分钟", "x").is_err());
        assert!(create_reminder_core(&db, "-1分钟", "x").is_err());
    }

    #[test]
    fn core_persists_to_database() {
        let db = temp_db();
        create_reminder_core(&db, "5分钟", "休息").expect("应成功");
        let list = db.list_reminders().expect("应能列出");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "休息");
    }

    /// 取消调度任务：应从注册表移除并 abort 掉等待中的任务
    #[tokio::test]
    async fn cancel_scheduled_reminder_aborts_and_removes_task() {
        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        });
        reminder_tasks()
            .lock()
            .expect("锁")
            .insert("reminder-x".to_string(), handle);

        cancel_scheduled_reminder("reminder-x");

        assert!(
            !reminder_tasks()
                .lock()
                .expect("锁")
                .contains_key("reminder-x"),
            "取消后注册表不应保留该任务"
        );
    }

    /// 取消不存在的任务应安全无副作用
    #[test]
    fn cancel_unknown_reminder_is_noop() {
        cancel_scheduled_reminder("no-such-reminder");
    }
}
