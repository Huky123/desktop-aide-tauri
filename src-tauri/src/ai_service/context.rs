use std::collections::VecDeque;

use crate::ai_service::tools::ToolCall;

/// 图片附件数据（用于多模态请求）
#[derive(Debug, Clone)]
pub struct ImageContent {
    pub data: String,       // base64 编码
    pub media_type: String, // "image/png", "image/jpeg"
}

/// 聊天消息
#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    pub role: String, // "user", "assistant", "system", "tool"
    pub content: String,
    /// 图片附件（仅 user 消息携带，用于多模态 API）
    pub images: Vec<ImageContent>,
    /// 工具调用（仅 assistant 消息携带，工具协议用）
    pub tool_calls: Vec<ToolCall>,
    /// 工具结果关联 id（仅 role == "tool" 消息携带，OpenAI 协议用）
    pub tool_call_id: Option<String>,
}

/// 上下文管理器 —— 管理对话历史
pub struct ContextManager {
    /// 对话历史（最近 N 轮）
    history: VecDeque<ChatMessage>,
    /// 最大历史消息数
    max_history: usize,
    /// 最大 Token 估算（按字符数÷2 粗略估算）
    max_tokens: usize,
    /// 当前历史的累计 token 数（避免每次都重新计算）
    total_tokens: usize,
}

impl ContextManager {
    pub fn new(max_history: usize, max_tokens: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            max_tokens,
            total_tokens: 0,
        }
    }

    /// 估算文本的 token 数
    ///
    /// 采用更精确的估算方法：
    /// - 英文/ASCII: ~4 字符 = 1 token
    /// - 中文/CJK: ~1.5 字符 = 1 token
    /// - `data:image/...;base64,` 段：按 1/8 折算。视觉模型按图像尺寸计费，
    ///   base64 字符数会严重高估 token，导致含图历史被过快裁剪。
    fn estimate_tokens(text: &str) -> usize {
        let mut ascii_count = 0;
        let mut cjk_count = 0;
        let mut rest = text;

        loop {
            let Some(pos) = rest.find("data:image/") else {
                break;
            };
            count_chars(&rest[..pos], &mut ascii_count, &mut cjk_count);

            let after = &rest[pos..];
            let Some(comma_rel) = after.find(";base64,") else {
                count_chars(after, &mut ascii_count, &mut cjk_count);
                break;
            };
            let b64_start = comma_rel + ";base64,".len();
            let data_end = after[b64_start..]
                .find(|c| [')', ' ', '\n', '\r', '\t'].contains(&c))
                .map(|idx| b64_start + idx)
                .unwrap_or(after.len());
            // data:image/png;base64, 前缀按普通字符，base64 数据按 1/8 折算
            ascii_count += b64_start / 4;
            ascii_count += (data_end - b64_start) / 8;
            rest = &after[data_end..];
        }
        count_chars(rest, &mut ascii_count, &mut cjk_count);

        (ascii_count / 4) + (cjk_count * 2 / 3)
    }

    /// 添加一轮对话
    pub fn add_turn(&mut self, user_msg: &str, assistant_msg: &str) {
        let user_tokens = Self::estimate_tokens(user_msg);
        let assistant_tokens = Self::estimate_tokens(assistant_msg);

        self.history.push_back(ChatMessage {
            role: "user".to_string(),
            content: user_msg.to_string(),
            images: vec![],
            tool_calls: vec![],
            tool_call_id: None,
        });
        self.history.push_back(ChatMessage {
            role: "assistant".to_string(),
            content: assistant_msg.to_string(),
            images: vec![],
            tool_calls: vec![],
            tool_call_id: None,
        });

        self.total_tokens += user_tokens + assistant_tokens;

        // 裁剪历史，保持 token 限制
        self.trim_history();
    }

    /// 裁剪历史消息，保持在 token 限制内
    fn trim_history(&mut self) {
        // 先按消息数量裁剪
        while self.history.len() > self.max_history * 2 {
            if let Some(msg) = self.history.pop_front() {
                self.total_tokens = self
                    .total_tokens
                    .saturating_sub(Self::estimate_tokens(&msg.content));
            }
            if let Some(msg) = self.history.pop_front() {
                self.total_tokens = self
                    .total_tokens
                    .saturating_sub(Self::estimate_tokens(&msg.content));
            }
        }

        // 再按 token 数量裁剪
        while self.total_tokens > self.max_tokens && self.history.len() >= 2 {
            if let Some(msg) = self.history.pop_front() {
                self.total_tokens = self
                    .total_tokens
                    .saturating_sub(Self::estimate_tokens(&msg.content));
            }
            if let Some(msg) = self.history.pop_front() {
                self.total_tokens = self
                    .total_tokens
                    .saturating_sub(Self::estimate_tokens(&msg.content));
            }
        }
    }

    /// 清除所有历史
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.total_tokens = 0;
    }

    /// 从存储的 (user, assistant) 对话对重建完整历史。
    /// 会先清空现有历史，再逐对调用 add_turn（自带 token 计数和裁剪）。
    pub fn rebuild_history(&mut self, pairs: &[(String, String)]) {
        self.history.clear();
        self.total_tokens = 0;
        for (user_msg, assistant_msg) in pairs {
            self.add_turn(user_msg, assistant_msg);
        }
    }

    /// 构建发送给 LLM 的完整消息列表（纯文本，无图片）
    /// 注意：确保只有一条 system 消息（部分 API 如 Qwen 不允许多条）
    pub fn build_messages(&self, system_prompt: &str, user_input: &str) -> Vec<ChatMessage> {
        self.build_messages_inner(system_prompt, user_input, vec![])
    }

    /// 构建带图片的消息列表 — 图片附加到最后一条 user 消息
    pub fn build_messages_with_images(
        &self,
        system_prompt: &str,
        user_input: &str,
        images: Vec<ImageContent>,
    ) -> Vec<ChatMessage> {
        self.build_messages_inner(system_prompt, user_input, images)
    }

    fn build_messages_inner(
        &self,
        system_prompt: &str,
        user_input: &str,
        images: Vec<ImageContent>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // 1. 系统 Prompt
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
            images: vec![],
            tool_calls: vec![],
            tool_call_id: None,
        });

        // 2. 对话历史
        for msg in self.history.iter() {
            messages.push(msg.clone());
        }

        // 3. 当前用户输入
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: user_input.to_string(),
            images,
            tool_calls: vec![],
            tool_call_id: None,
        });

        messages
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(20, 16000)
    }
}

/// 统计文本中的 ASCII 与 CJK 字符数（供 token 估算使用）
fn count_chars(text: &str, ascii_count: &mut usize, cjk_count: &mut usize) {
    for ch in text.chars() {
        if ch.is_ascii() {
            *ascii_count += 1;
        } else if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
            || ('\u{3400}'..='\u{4DBF}').contains(&ch) // CJK 扩展 A
            || ('\u{20000}'..='\u{2A6DF}').contains(&ch) // CJK 扩展 B
            || ('\u{F900}'..='\u{FAFF}').contains(&ch) // CJK 兼容
            || ('\u{3040}'..='\u{309F}').contains(&ch) // 日文平假名
            || ('\u{30A0}'..='\u{30FF}').contains(&ch) // 日文片假名
            || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
        // 韩文
        {
            *cjk_count += 1;
        } else {
            *ascii_count += 1; // 其他字符按 ASCII 估算
        }
    }
}
