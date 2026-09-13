use crate::ai_service::context::{ChatMessage, ImageContent};
use crate::ai_service::error::AiError;
use crate::ai_service::providers;
use crate::ai_service::tools::{ToolCall, ToolSpec};
use crate::config::manager::AppConfig;
use serde::Serialize;

/// 跨 Provider 共享的 Chat Completion 消息格式。
/// content 为 `serde_json::Value`：纯文本时为 String，多模态时为 Array。
/// tool_calls / tool_call_id 为 OpenAI 工具协议字段，仅携带时序列化（`skip_serializing_if`）。
#[derive(Serialize)]
pub(crate) struct ChatCompletionMessage {
    pub role: String,
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 从 context::ChatMessage 转换为 ChatCompletionMessage
/// 纯文本消息 → content 为 String；带图片消息 → content 为多模态数组；
/// assistant 携带 tool_calls 时（OpenAI 协议）content 置 null 并附加 tool_calls 数组；
/// role == "tool" 消息携带 tool_call_id。
pub(crate) fn into_completion_messages(
    messages: &[ChatMessage],
    provider_type: &ProviderType,
) -> Vec<ChatCompletionMessage> {
    messages
        .iter()
        .map(|m| {
            let mut msg = ChatCompletionMessage {
                role: m.role.clone(),
                content: if m.images.is_empty() {
                    serde_json::Value::String(m.content.clone())
                } else {
                    build_multimodal_content(&m.content, &m.images, provider_type)
                },
                tool_calls: None,
                tool_call_id: m.tool_call_id.clone(),
            };
            if m.role == "assistant" && !m.tool_calls.is_empty() {
                msg.content = serde_json::Value::Null;
                msg.tool_calls = Some(
                    m.tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": serde_json::to_string(&tc.arguments)
                                        .unwrap_or_else(|_| "{}".to_string()),
                                }
                            })
                        })
                        .collect(),
                );
            }
            msg
        })
        .collect()
}

/// 构建多模态 content 数组
fn build_multimodal_content(
    text: &str,
    images: &[ImageContent],
    provider_type: &ProviderType,
) -> serde_json::Value {
    let mut parts: Vec<serde_json::Value> = Vec::new();

    // 文本部分
    if !text.is_empty() {
        parts.push(serde_json::json!({
            "type": "text",
            "text": text,
        }));
    }

    // 图片部分
    for img in images {
        match provider_type {
            ProviderType::Anthropic => {
                parts.push(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": img.media_type,
                        "data": img.data,
                    }
                }));
            }
            _ => {
                // OpenAI 兼容格式
                let data_url = format!("data:{};base64,{}", img.media_type, img.data);
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": data_url,
                    }
                }));
            }
        }
    }

    serde_json::Value::Array(parts)
}

/// LLM Provider 类型
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderType {
    OpenAI,
    Xai,
    Anthropic,
    Gemini,
    Ollama,
    DeepSeek,
    Qwen,
    Kimi,
    Zhipu,
    OpenRouter,
    Custom,
}

fn normalize_api_base(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/chat/completions") || base.ends_with("/v1") {
        base.to_string()
    } else {
        format!("{base}/v1")
    }
}

/// AI Provider — 使用 enum 分发而非 trait object（async fn 不支持 dyn）
pub enum AiProvider {
    Anthropic(providers::AnthropicProvider),
    OpenAiCompatible(providers::OpenAiCompatibleProvider),
}

impl AiProvider {
    /// 流式对话（可选携带工具）。文本逐 chunk 经 on_chunk 回调；
    /// 返回本轮模型请求的工具调用列表（空 = 本轮无工具调用）。
    /// tools 为 None 时等价于原 chat_stream 行为。
    pub async fn chat_stream_with_tools<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        on_chunk: &mut F,
    ) -> Result<Vec<ToolCall>, AiError> {
        match self {
            AiProvider::Anthropic(p) => p.chat_stream_with_tools(messages, tools, on_chunk).await,
            AiProvider::OpenAiCompatible(p) => {
                p.chat_stream_with_tools(messages, tools, on_chunk).await
            }
        }
    }

    /// 流式对话，通过回调逐 chunk 返回文本增量（无工具，原行为）
    pub async fn chat_stream<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        on_chunk: &mut F,
    ) -> Result<(), AiError> {
        self.chat_stream_with_tools(messages, None, on_chunk)
            .await
            .map(|_| ())
    }

    /// 非流式对话（可选携带工具）。
    ///
    /// 用于不支持流式输出的模型（图片生成必须整张返回），但这类模型仍可能支持
    /// 工具调用，因此 `tools` 与流式路径一样是独立参数。
    /// 返回 (完整响应文本, 本轮工具调用列表)。
    pub async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<(String, Vec<ToolCall>), AiError> {
        match self {
            AiProvider::Anthropic(p) => p.chat_with_tools(messages, tools).await,
            AiProvider::OpenAiCompatible(p) => p.chat_with_tools(messages, tools).await,
        }
    }

    /// 非流式对话（无工具）— 用于图片生成等不支持流式输出的模型。
    /// 返回完整的响应文本（含图片 Markdown）。
    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<String, AiError> {
        self.chat_with_tools(messages, None)
            .await
            .map(|(text, _)| text)
    }

    /// 返回当前使用的模型名称
    pub fn model_name(&self) -> &str {
        match self {
            AiProvider::Anthropic(p) => p.model_name(),
            AiProvider::OpenAiCompatible(p) => p.model_name(),
        }
    }

    /// 该 provider 是否**可能**有独立的图片生成端点。
    ///
    /// 只按**协议形态**回答：OpenAI 兼容协议（含所有第三方网关）都可能自带
    /// `POST {base}/images/generations`；Anthropic 协议没有这个端点。
    ///
    /// 为什么不再看 `provider_type`：它只是**默认 base_url 的别名**（见 `from_config`），
    /// 不是能力声明。而 `Custom` 恰恰是最可能自带任意端点的类型，却因这条判定被判死
    /// ——于是"出图模型 + custom 网关"永远走 `/chat/completions`，网关回
    /// `This model is not supported on the Chat Completions endpoint`。
    ///
    /// 这里只回答"**可能**有"：到底有没有，静态判定不了。由 `Agent` 发一次请求探测
    /// （见 `Agent::stream_messages` 中仅对 404 生效的回退），并在 Agent 生命周期内记住。
    pub fn may_have_images_endpoint(&self) -> bool {
        matches!(self, AiProvider::OpenAiCompatible(_))
    }

    pub async fn generate_image(&self, prompt: &str) -> Result<String, AiError> {
        match self {
            AiProvider::OpenAiCompatible(provider) => provider.generate_image(prompt).await,
            AiProvider::Anthropic(_) => Err(AiError::Other(
                "当前服务商不支持独立图片生成接口".to_string(),
            )),
        }
    }
}

/// 解析该配置实际的 API base（含 `/v1` 归一化与按服务商回落到默认地址）。
///
/// **探测与真实请求必须共用这一个函数**——否则会出现"测试连接通过、实际请求打偏"
/// 这种最难查的不一致。本项目已经因为"两套判定各写一遍"吃过一次亏
/// （见 `invocation` 模块头注释）。
pub(crate) fn resolve_api_base(config: &AppConfig) -> String {
    match resolve_provider_type(&config.ai_provider) {
        ProviderType::Ollama => {
            let endpoint = if config.ollama_endpoint.is_empty() {
                "http://localhost:11434"
            } else {
                &config.ollama_endpoint
            };
            normalize_api_base(endpoint)
        }
        _ if !config.api_base.is_empty() => normalize_api_base(&config.api_base),
        ProviderType::OpenAI => "https://api.openai.com/v1".to_string(),
        ProviderType::Xai => "https://api.x.ai/v1".to_string(),
        ProviderType::Anthropic => "https://api.anthropic.com/v1".to_string(),
        ProviderType::Gemini => {
            "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
        }
        ProviderType::DeepSeek => "https://api.deepseek.com/v1".to_string(),
        ProviderType::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
        ProviderType::Kimi => "https://api.moonshot.cn/v1".to_string(),
        ProviderType::Zhipu => "https://open.bigmodel.cn/api/paas/v4".to_string(),
        ProviderType::OpenRouter => "https://openrouter.ai/api/v1".to_string(),
        ProviderType::Custom => "".to_string(),
    }
}

/// 由 base 推出 `chat/completions` 端点（用户直接填了完整端点时原样使用）
fn chat_endpoint_from_base(api_base: &str) -> String {
    let trimmed = api_base.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

/// 该配置的 `chat/completions` 端点（探测用）
pub(crate) fn resolve_chat_endpoint(config: &AppConfig) -> String {
    chat_endpoint_from_base(&resolve_api_base(config))
}

/// 从配置创建对应的 Provider 实例
pub fn from_config(config: &AppConfig) -> AiProvider {
    let provider_type = resolve_provider_type(&config.ai_provider);
    let api_base = resolve_api_base(config);

    let model = config.model.clone();
    let api_key = config.api_key.clone();
    let temperature = config.temperature;
    let max_tokens = config.max_tokens;

    match provider_type {
        ProviderType::Anthropic => AiProvider::Anthropic(providers::AnthropicProvider::new(
            api_base,
            model,
            api_key,
            temperature,
            max_tokens,
        )),
        _ => AiProvider::OpenAiCompatible(providers::OpenAiCompatibleProvider::new(
            chat_endpoint_from_base(&api_base),
            model,
            api_key,
            temperature,
            max_tokens,
            provider_type,
        )),
    }
}

/// 将 provider 字符串解析为 ProviderType（公开，供 vision 检测等使用）
pub fn resolve_provider_type(provider_str: &str) -> ProviderType {
    match provider_str {
        "openai" => ProviderType::OpenAI,
        "xai" => ProviderType::Xai,
        "anthropic" => ProviderType::Anthropic,
        "gemini" => ProviderType::Gemini,
        "ollama" => ProviderType::Ollama,
        "deepseek" => ProviderType::DeepSeek,
        "qwen" => ProviderType::Qwen,
        "kimi" => ProviderType::Kimi,
        "zhipu" => ProviderType::Zhipu,
        "openrouter" => ProviderType::OpenRouter,
        "custom" => ProviderType::Custom,
        _ => {
            log::warn!(
                "未知的 AI Provider: '{}'，将使用 Anthropic 作为回退。",
                provider_str
            );
            ProviderType::Anthropic
        }
    }
}

pub fn is_supported_provider(provider_str: &str) -> bool {
    matches!(
        provider_str,
        "openai"
            | "xai"
            | "anthropic"
            | "gemini"
            | "ollama"
            | "deepseek"
            | "qwen"
            | "kimi"
            | "zhipu"
            | "openrouter"
            | "custom"
    )
}

#[cfg(test)]
mod tests {
    use super::{normalize_api_base, resolve_provider_type, ProviderType};

    #[test]
    fn normalizes_provider_api_bases() {
        assert_eq!(
            normalize_api_base("https://example.com"),
            "https://example.com/v1"
        );
        assert_eq!(
            normalize_api_base("https://example.com/v1/"),
            "https://example.com/v1"
        );
        assert_eq!(
            normalize_api_base("https://example.com/v1/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn keeps_xai_config_value_compatible() {
        assert_eq!(resolve_provider_type("xai"), ProviderType::Xai);
    }
}
