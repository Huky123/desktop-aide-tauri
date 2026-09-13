//! 工具层：工具描述、执行器、参数校验。
//!
//! 当前工具：`get_current_time`（零副作用）、`create_reminder` / `list_reminders` /
//! `cancel_reminder`（应用内副作用）、`web_search`（联网搜索，需设置中开启）。
//! 按开关过滤后的当轮工具列表由 `tools_for_request(enable_tools, enable_web_search)` 生成。
//!
//! 规格偏差说明（先指出，非默默修改）：
//! - 骨架中 `ToolContext.app` 直接持有 `AppHandle`；改为 `schedule_reminder`
//!   回调注入（详见 ToolContext 注释），隔离 tauri 依赖，保证测试可达代码
//!   不链接 wry 窗口代码（否则纯 cargo 测试 exe 在 Windows 加载即崩）。
//! - `database` 为必填（Arc 共享），单测用临时目录数据库构造。
//! - 文件工具所需字段（data_dir/allowed_dirs/require_confirmation）待文件工具
//!   实现时补充，属下一迭代。

use crate::ai_service::web_search::SearchConfig;
use chrono::{Datelike, Local, Weekday};

/// 工具描述（注册表条目，随请求发送给模型）
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema（object）
    pub parameters: serde_json::Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// 模型返回的一次工具调用
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// OpenAI: tool_call_id；Anthropic: tool_use id
    pub id: String,
    pub name: String,
    /// 已解析的参数对象
    pub arguments: serde_json::Value,
}

/// 工具执行输出（MVP 仅文本；图片等后续扩展）
#[derive(Debug, Clone)]
pub enum ToolOutput {
    Text(String),
}

/// 工具执行上下文：提供工具所需的运行时资源。
/// - `database`：全局数据库（Arc 共享，提醒落库等）。
/// - `schedule_reminder`：提醒创建后的调度回调。生产由 ai_chat 注入
///   （包装 schedule_reminder，需要宿主 AppHandle）；测试传 None（仅落库）。
///
/// 规格偏差说明（先指出，非默默修改）：
/// - 骨架设计为 `app: AppHandle` 直接持有宿主句柄；但那样会让测试二进制
///   链接 wry 窗口代码，而纯 cargo 测试 exe 无 comctl32 v6 清单，
///   在 Windows 上加载即崩（STATUS_ENTRYPOINT_NOT_FOUND）。改用回调注入
///   把 tauri 依赖隔离在生产接线层，测试可达代码保持零 tauri 依赖。
pub struct ToolContext {
    pub database: std::sync::Arc<tokio::sync::Mutex<crate::db::Database>>,
    pub schedule_reminder: Option<Box<dyn Fn(crate::db::Reminder) + Send + Sync>>,
    /// 联网搜索配置（设置中开启「允许 AI 联网搜索」时 Some，否则 None）。
    /// None 时调用 web_search 工具会返回明确的未启用错误。
    pub web_search: Option<SearchConfig>,
}

/// 当前本地时间工具
fn time_spec() -> ToolSpec {
    ToolSpec::new(
        "get_current_time",
        "获取当前本地日期与时间（含中文星期）。无需任何参数。",
        serde_json::json!({ "type": "object", "properties": {} }),
    )
}

/// 创建提醒工具
fn reminder_spec() -> ToolSpec {
    ToolSpec::new(
        "create_reminder",
        "创建定时提醒工具。当用户请求定时提醒（如「10分钟后提醒我喝水」「2小时后叫我开会」「14:30 提醒我」）时，必须调用本工具完成，不要只做口头承诺。支持相对时间（如「10分钟」「2小时」）或绝对时间（如「14:30」）。提醒到期后会在应用内展示：气泡脉冲唤醒 + 面板内助手提醒消息（不依赖系统通知）。",
        serde_json::json!({
            "type": "object",
            "properties": {
                "time_expr": {
                    "type": "string",
                    "description": "时间表达式，如「10分钟」「2小时」「14:30」"
                },
                "content": {
                    "type": "string",
                    "description": "提醒内容"
                }
            },
            "required": ["time_expr", "content"]
        }),
    )
}

/// 列出提醒工具
fn list_reminders_spec() -> ToolSpec {
    ToolSpec::new(
        "list_reminders",
        "列出当前所有待触发的定时提醒（内容、触发时间、id）。当用户想要查看或取消提醒时调用；取消前必须先调用本工具拿到 id。",
        serde_json::json!({ "type": "object", "properties": {} }),
    )
}

/// 取消提醒工具
fn cancel_reminder_spec() -> ToolSpec {
    ToolSpec::new(
        "cancel_reminder",
        "取消指定 id 的定时提醒。id 必须来自 list_reminders 的结果。取消前应先用 list_reminders 查看当前提醒，并与用户确认要取消哪一条（如「当前有这些提醒，要取消哪个？」），不要未经确认擅自取消。",
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "要取消的提醒 id（来自 list_reminders）"
                }
            },
            "required": ["id"]
        }),
    )
}

/// 联网搜索工具
fn web_search_spec() -> ToolSpec {
    ToolSpec::new(
        "web_search",
        "联网搜索工具。当用户的问题需要最新或实时信息——如新闻、时事、股价、汇率、天气、赛事比分、政策法规、产品发布、人物近况，或所问内容可能发生在你的知识截止日期之后，或用户明确要求「搜索/查一下/去网上看看/联网/最新消息」——必须调用本工具获取网页搜索结果，然后基于搜索结果回答，并在引用具体网页内容时附上来源链接。可先调用 get_current_time 确认今天的日期，以判断信息是否需要联网核实。一次只搜索一个主题，搜索词用简洁的关键词或问题。",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "搜索关键词或问题，尽量简洁聚焦"
                },
                "max_results": {
                    "type": "integer",
                    "description": "返回的结果条数（1-8，默认 5）"
                }
            },
            "required": ["query"]
        }),
    )
}

/// 内置工具注册表（含 web_search；是否真正发给模型由 `tools_for_request` 按开关过滤）
pub fn builtin_tools() -> Vec<ToolSpec> {
    vec![
        time_spec(),
        reminder_spec(),
        list_reminders_spec(),
        cancel_reminder_spec(),
        web_search_spec(),
    ]
}

/// 按用户开关组合生成当轮请求实际携带的工具列表：
/// - 开启「工具调用」→ 时间/提醒工具；
/// - 开启「允许 AI 联网搜索」→ 追加 web_search（单独开启联网时也附带 get_current_time，
///   便于模型判断信息时效性）。
pub fn tools_for_request(enable_tools: bool, enable_web_search: bool) -> Vec<ToolSpec> {
    let mut list = Vec::new();
    if enable_tools {
        list.push(time_spec());
        list.push(reminder_spec());
        list.push(list_reminders_spec());
        list.push(cancel_reminder_spec());
    } else if enable_web_search {
        list.push(time_spec());
    }
    if enable_web_search {
        list.push(web_search_spec());
    }
    list
}

/// 参数校验：MVP 仅校验参数为 JSON 对象；必填字段/长度钳制随各工具补充
fn validate_args(args: &serde_json::Value, spec: &ToolSpec) -> Result<(), String> {
    if !args.is_object() {
        return Err(format!("工具 {} 的参数必须是 JSON 对象", spec.name));
    }
    Ok(())
}

/// 执行工具调用。未知工具或参数不合法返回 Err（不 panic）。
pub async fn execute_tool(
    name: &str,
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<ToolOutput, String> {
    let spec = builtin_tools()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| format!("未知工具: {name}"))?;
    validate_args(args, &spec)?;
    match name {
        "get_current_time" => Ok(ToolOutput::Text(get_current_time())),
        "create_reminder" => {
            let time_expr = args
                .get("time_expr")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "工具 create_reminder 缺少参数 time_expr".to_string())?;
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let reminder = {
                let db = ctx.database.lock().await;
                crate::commands::reminder::create_reminder_core(&db, time_expr, content)?
            };
            match &ctx.schedule_reminder {
                Some(schedule) => schedule(reminder.clone()),
                None => log::warn!(
                    "[tool] create_reminder 无调度回调（测试环境），已落库但未调度应用内提醒"
                ),
            }
            let when = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(reminder.remind_at)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_else(|| reminder.remind_at.to_string());
            Ok(ToolOutput::Text(format!(
                "已创建提醒：{}（{} 触发）",
                reminder.content, when
            )))
        }
        "list_reminders" => {
            let db = ctx.database.lock().await;
            let reminders = db.list_reminders()?;
            if reminders.is_empty() {
                return Ok(ToolOutput::Text("当前没有待触发的提醒".to_string()));
            }
            let lines: Vec<String> = reminders
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let when = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(r.remind_at)
                        .map(|t| {
                            t.with_timezone(&chrono::Local)
                                .format("%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_else(|| r.remind_at.to_string());
                    format!("{}. {}（{} 触发，id: {}）", i + 1, r.content, when, r.id)
                })
                .collect();
            Ok(ToolOutput::Text(format!(
                "当前有 {} 条待触发提醒:\n{}",
                lines.len(),
                lines.join("\n")
            )))
        }
        "cancel_reminder" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "工具 cancel_reminder 缺少参数 id".to_string())?;
            let db = ctx.database.lock().await;
            let removed = db.delete_reminder(id)?;
            if removed {
                // 同时中止已调度的等待任务，避免"取消后到点仍触发"
                crate::commands::reminder::cancel_scheduled_reminder(id);
                Ok(ToolOutput::Text(format!("已取消提醒（id: {id}）")))
            } else {
                Err(format!("未找到提醒（id: {id}），可能已被取消或已触发"))
            }
        }
        "web_search" => {
            let cfg = ctx.web_search.clone().ok_or_else(|| {
                "联网搜索未启用：请先在「设置 → 通用」中开启「允许 AI 联网搜索」".to_string()
            })?;
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if query.is_empty() {
                return Err("工具 web_search 缺少参数 query".to_string());
            }
            if query.chars().count() > 200 {
                return Err("搜索关键词过长（最多 200 字）".to_string());
            }
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(crate::ai_service::web_search::DEFAULT_MAX_RESULTS);
            let outcome = crate::ai_service::web_search::search(&query, max_results, &cfg).await?;
            Ok(ToolOutput::Text(
                crate::ai_service::web_search::format_search_outcome(&outcome),
            ))
        }
        _ => Err(format!("工具 {name} 已注册但未实现执行器")),
    }
}

/// 当前本地时间，格式 "YYYY-MM-DD HH:MM:SS 星期X"
fn get_current_time() -> String {
    let now = Local::now();
    let weekday = match now.weekday() {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    };
    format!("{} {weekday}", now.format("%Y-%m-%d %H:%M:%S"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试用 ToolContext：临时数据库 + 无调度回调（不调度系统通知）
    fn test_ctx() -> ToolContext {
        let dir =
            std::env::temp_dir().join(format!("desktop-aide-tool-test-{}", uuid::Uuid::new_v4()));
        let db = crate::db::Database::open(&dir).expect("打开测试数据库失败");
        ToolContext {
            database: std::sync::Arc::new(tokio::sync::Mutex::new(db)),
            schedule_reminder: None,
            web_search: None,
        }
    }

    #[test]
    fn registry_contains_time_tool() {
        assert!(builtin_tools().iter().any(|t| t.name == "get_current_time"));
    }

    #[test]
    fn registry_contains_reminder_tool() {
        assert!(builtin_tools().iter().any(|t| t.name == "create_reminder"));
    }

    #[test]
    fn registry_contains_web_search_tool() {
        assert!(builtin_tools().iter().any(|t| t.name == "web_search"));
    }

    #[test]
    fn request_tools_follow_switches() {
        // 全关 → 空列表
        assert!(tools_for_request(false, false).is_empty());
        // 仅工具调用 → 时间 + 提醒，无 web_search
        let names = tools_for_request(true, false)
            .into_iter()
            .map(|t| t.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"create_reminder".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
        // 仅联网 → web_search + get_current_time，无提醒工具
        let names = tools_for_request(false, true)
            .into_iter()
            .map(|t| t.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"web_search".to_string()));
        assert!(names.contains(&"get_current_time".to_string()));
        assert!(!names.contains(&"create_reminder".to_string()));
        // 两者全开 → 时间 + 提醒 + web_search
        let names = tools_for_request(true, true)
            .into_iter()
            .map(|t| t.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"create_reminder".to_string()));
        assert!(names.contains(&"web_search".to_string()));
    }

    #[tokio::test]
    async fn web_search_requires_enabled_context() {
        let ctx = test_ctx(); // web_search: None
        let err = execute_tool(
            "web_search",
            &serde_json::json!({ "query": "今天新闻" }),
            &ctx,
        )
        .await
        .expect_err("未启用时应报错");
        assert!(err.contains("未启用"), "{err}");
    }

    #[tokio::test]
    async fn web_search_rejects_missing_query() {
        let mut ctx = test_ctx();
        ctx.web_search = Some(SearchConfig::default());
        let err = execute_tool("web_search", &serde_json::json!({}), &ctx)
            .await
            .expect_err("缺 query 应报错");
        assert!(err.contains("query"), "{err}");
    }

    #[test]
    fn time_output_contains_date_and_time() {
        let out = get_current_time();
        // "YYYY-MM-DD HH:MM:SS" 至少 19 字符
        assert!(out.len() >= 19, "应含日期时间: {out}");
        assert!(out.contains("星期"), "应含中文星期: {out}");
    }

    #[test]
    fn validates_args_as_object() {
        let spec = builtin_tools()
            .into_iter()
            .find(|t| t.name == "get_current_time")
            .unwrap();
        assert!(validate_args(&serde_json::json!("oops"), &spec).is_err());
        assert!(validate_args(&serde_json::json!({}), &spec).is_ok());
    }

    #[tokio::test]
    async fn executes_time_tool() {
        let ctx = test_ctx();
        let out = execute_tool("get_current_time", &serde_json::json!({}), &ctx)
            .await
            .expect("执行应成功");
        match out {
            ToolOutput::Text(t) => assert!(t.len() >= 19),
        }
    }

    #[tokio::test]
    async fn rejects_unknown_tool() {
        let ctx = test_ctx();
        assert!(execute_tool("nope", &serde_json::json!({}), &ctx)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn reminder_tool_persists_and_returns_summary() {
        let ctx = test_ctx();
        let out = execute_tool(
            "create_reminder",
            &serde_json::json!({ "time_expr": "10分钟", "content": "喝水" }),
            &ctx,
        )
        .await
        .expect("执行应成功");
        match out {
            ToolOutput::Text(t) => assert!(t.contains("喝水"), "应含提醒内容: {t}"),
        }
        let db = ctx.database.lock().await;
        let list = db.list_reminders().expect("应能列出");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "喝水");
    }

    #[tokio::test]
    async fn reminder_tool_rejects_missing_time_expr() {
        let ctx = test_ctx();
        assert!(execute_tool(
            "create_reminder",
            &serde_json::json!({ "content": "x" }),
            &ctx,
        )
        .await
        .is_err());
    }

    #[test]
    fn registry_contains_reminder_management_tools() {
        assert!(builtin_tools().iter().any(|t| t.name == "list_reminders"));
        assert!(builtin_tools().iter().any(|t| t.name == "cancel_reminder"));
    }

    #[tokio::test]
    async fn list_reminders_tool_returns_created_reminders() {
        let ctx = test_ctx();
        execute_tool(
            "create_reminder",
            &serde_json::json!({ "time_expr": "10分钟", "content": "喝水" }),
            &ctx,
        )
        .await
        .expect("创建应成功");
        let out = execute_tool("list_reminders", &serde_json::json!({}), &ctx)
            .await
            .expect("列出应成功");
        match out {
            ToolOutput::Text(t) => {
                assert!(t.contains("喝水"), "应含提醒内容: {t}");
                assert!(t.contains("id:"), "应含 id 供取消使用: {t}");
            }
        }
    }

    #[tokio::test]
    async fn list_reminders_tool_reports_empty() {
        let ctx = test_ctx();
        let out = execute_tool("list_reminders", &serde_json::json!({}), &ctx)
            .await
            .expect("列出应成功");
        match out {
            ToolOutput::Text(t) => assert!(t.contains("没有待触发"), "应为空提示: {t}"),
        }
    }

    #[tokio::test]
    async fn cancel_reminder_tool_deletes_and_reports() {
        let ctx = test_ctx();
        execute_tool(
            "create_reminder",
            &serde_json::json!({ "time_expr": "10分钟", "content": "休息" }),
            &ctx,
        )
        .await
        .expect("创建应成功");
        let id = {
            let db = ctx.database.lock().await;
            db.list_reminders().expect("应能列出").remove(0).id
        };
        let out = execute_tool("cancel_reminder", &serde_json::json!({ "id": id }), &ctx)
            .await
            .expect("取消应成功");
        match out {
            ToolOutput::Text(t) => assert!(t.contains("已取消"), "应含已取消: {t}"),
        }
        let db = ctx.database.lock().await;
        assert!(db.list_reminders().expect("应能列出").is_empty());
    }

    #[tokio::test]
    async fn cancel_reminder_tool_rejects_unknown_id() {
        let ctx = test_ctx();
        assert!(execute_tool(
            "cancel_reminder",
            &serde_json::json!({ "id": "no-such-id" }),
            &ctx,
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn cancel_reminder_tool_rejects_missing_id() {
        let ctx = test_ctx();
        assert!(
            execute_tool("cancel_reminder", &serde_json::json!({}), &ctx)
                .await
                .is_err()
        );
    }
}
