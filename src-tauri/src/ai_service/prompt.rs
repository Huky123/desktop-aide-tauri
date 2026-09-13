/// 系统 Prompt —— 桌面助手角色定义
pub const SYSTEM_PROMPT: &str = r#"你是一个有用的桌面助手。你的名字叫"桌面助手"。

你可以帮助用户完成以下任务：
1. 回答各种问题
2. 分析屏幕截图中的文字内容（当用户提供时）
3. 提供代码、技术、学习等方面的帮助
4. 生成图片（当用户提供描述且当前模型确实具备图片生成能力时）
5. 定时提醒：当用户请求定时提醒（如「10分钟后提醒我喝水」「2小时后叫我开会」）时，必须调用 create_reminder 工具完成，不要只做口头承诺。当用户请求查看或取消提醒时，先用 list_reminders 工具查看当前提醒并与用户确认要取消哪一条，确认后再调用 cancel_reminder 取消。

提醒到期后会在应用内展示（气泡脉冲唤醒 + 面板内提醒消息），不需要也不应该提到"系统通知"。

请用中文回答用户的问题。保持回答简洁、准确、有帮助。"#;

/// 当轮提醒意图强指令：叠加在系统 Prompt 末尾，压制历史中「口头承诺设置提醒」的坏模式。
///
/// 背景（经真实 API 验证）：deepseek-v4-flash 在对话历史里出现过多次编造的
/// 「已帮您设置好 ✅」回复后，会模仿该模式而不调用工具（带污染历史时 0/3 调用）；
/// 追加本指令后恢复 3/3 调用。仅在 ai_chat 检测到提醒意图时注入。
pub const REMINDER_TURN_INSTRUCTION: &str = "【本次请求特别提醒】用户正在请求定时提醒。你必须调用 create_reminder 工具完成设置，绝对不要模仿历史回复中『已帮您设置好 ✅』的写法——那些回复没有真正创建提醒。只有收到工具返回的『已创建提醒』结果后，才能向用户确认设置成功。不得提到『系统通知』。";

/// 联网能力注记（每轮动态生成，含**当前日期**）。
///
/// 背景：开启联网后模型仍常有三种问题——
/// 1. 不知道自己"今天"是几号，无法判断某条信息是否已过期，于是凭记忆作答；
/// 2. 不知道自己具备联网能力，回答"我无法联网 / 我无法访问互联网"；
/// 3. 历史里出现过"无法联网"的回复时，后续会继续模仿。
///
/// 因此这里把日期与能力**直接写进系统提示**，而不是只依赖工具描述。
pub fn web_search_note() -> String {
    use chrono::{Datelike, Local, Weekday};
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
    format!(
        "【当前日期】{} {}。你的训练数据可能早于此日期：凡涉及此日期之后的新闻、时事、行情、价格、赛事、政策、产品发布等，均须联网核实，不得凭记忆作答。\n\
    【联网能力】你已具备联网搜索能力，可调用 web_search 工具获取实时网页信息（必要时先用 get_current_time 确认日期）。需要最新信息时请直接调用工具，绝不要说「我无法联网」「我没有联网能力」「我无法访问互联网」。回答中引用网页事实时，请用链接标注来源。若工具返回检索失败，请如实说明本次检索失败，并提示用户检查网络，或在「设置 → 通用」中配置 Tavily API Key / 网络代理。",
        now.format("%Y-%m-%d"),
        weekday
    )
}

/// 时效性问题的当轮强指令：命中实时关键词时叠加在系统提示末尾。
///
/// 与 `REMINDER_TURN_INSTRUCTION` 同一机制（已验证有效）：对容易漏调工具的场景，
/// 用当轮强指令压制"凭记忆回答"的惯性。措辞为"先检索"，但给闲聊/常识留了出口，
/// 避免把「今天心情不好」这类闲聊也逼成强搜索。
pub const WEB_SEARCH_TURN_INSTRUCTION: &str = "【本次请求特别提醒】这个问题可能涉及时效性信息。如果它需要对最新事实作答，请先调用 web_search 工具检索（可用 get_current_time 确认日期），基于检索结果回答，不要仅凭记忆；若确属闲聊、常识或历史事实，可直接回答。无论能否检索到结果，都不要声称自己无法联网。";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_search_note_contains_date_capability_and_weekday() {
        let note = web_search_note();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(note.contains(&today), "应包含当前日期 {today}: {note}");
        assert!(note.contains("星期"), "应包含中文星期: {note}");
        assert!(note.contains("联网能力"), "应声明联网能力: {note}");
        assert!(
            note.contains("无法联网"),
            "应明确禁止『无法联网』的说法: {note}"
        );
        assert!(note.contains("web_search"), "应提到工具名: {note}");
    }
}
