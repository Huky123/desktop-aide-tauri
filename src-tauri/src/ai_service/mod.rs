pub mod agent;
pub mod context;
pub mod error;
pub mod invocation;
pub mod probe;
pub mod prompt;
pub mod provider;
pub mod providers;
pub mod tools;
pub mod vision;
pub mod web_search;

/// 按**字符**（而非字节）截断文本，用于日志/预览。
///
/// 直接写 `&s[..s.len().min(n)]` 会在第 n 个字节落在多字节字符（中文/emoji）中间时
/// panic，而 panic 发生在 `ai_chat` 内部会跳过 `agent_busy` 复位，导致应用永久卡在
/// 「AI 正在响应中」。所有日志预览一律走本函数。
pub(crate) fn preview_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::preview_chars;

    #[test]
    fn preview_chars_never_splits_multibyte() {
        // "中" 占 3 字节：按字节截断到 4 会切断第二个字，按字符截断则安全
        let text = "中文测试内容";
        assert_eq!(preview_chars(text, 2), "中文");
        assert_eq!(preview_chars(text, 100), text);
        assert_eq!(preview_chars("", 10), "");
    }

    #[test]
    fn preview_chars_handles_emoji_and_ascii() {
        assert_eq!(preview_chars("ab😀cd", 3), "ab😀");
        assert_eq!(preview_chars("abcdef", 3), "abc");
    }
}
