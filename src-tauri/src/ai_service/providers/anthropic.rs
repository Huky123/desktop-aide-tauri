use crate::ai_service::context::ChatMessage;
use crate::ai_service::error::{classify_http_error, AiError};
use crate::ai_service::provider::{into_completion_messages, ProviderType};
use crate::ai_service::tools::{ToolCall, ToolSpec};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;

/// Anthropic Messages API Provider
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_base: String,
    model: String,
    api_key: String,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct StreamDelta {
    text: Option<String>,
}

#[derive(Deserialize)]
struct StreamError {
    #[serde(rename = "type")]
    error_type: Option<String>,
    message: Option<String>,
}

/// Anthropic content_block_start 事件中的 content_block
#[derive(Deserialize)]
struct StreamContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    source: Option<StreamImageSource>,
}

/// Anthropic image block 中的 source 子对象
#[derive(Deserialize)]
struct StreamImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: Option<String>,
    data: Option<String>,
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<StreamDelta>,
    error: Option<StreamError>,
    content_block: Option<StreamContentBlock>,
}

fn parse_stream_payload(data: &str) -> Result<Vec<String>, AiError> {
    let event: StreamEvent = serde_json::from_str(data).map_err(|error| {
        let preview: String = data.chars().take(300).collect();
        AiError::Stream(format!(
            "无法解析 Anthropic 流式响应: {error}；数据: {preview}"
        ))
    })?;

    match event.event_type.as_str() {
        "content_block_delta" => Ok(event
            .delta
            .and_then(|delta| delta.text)
            .filter(|text| !text.is_empty())
            .into_iter()
            .collect()),
        "content_block_start" => {
            let Some(block) = event.content_block else {
                return Ok(Vec::new());
            };
            if block.block_type != "image" {
                return Ok(Vec::new());
            }
            let Some(source) = block.source else {
                return Ok(Vec::new());
            };
            if source.source_type != "base64" {
                return Ok(Vec::new());
            }
            let (Some(data), Some(media_type)) = (source.data, source.media_type) else {
                return Ok(Vec::new());
            };
            Ok(vec![format!(
                "\n\n![生成的图片](data:{};base64,{})\n\n",
                media_type, data
            )])
        }
        "error" => {
            let message = event
                .error
                .map(|error| {
                    let error_type = error.error_type.unwrap_or_else(|| "unknown".to_string());
                    format!("{error_type}: {}", error.message.unwrap_or_default())
                })
                .unwrap_or_else(|| "未知 Anthropic 流式错误".to_string());
            Err(AiError::Stream(message))
        }
        _ => Ok(Vec::new()),
    }
}

impl AnthropicProvider {
    pub fn new(
        api_base: String,
        model: String,
        api_key: String,
        temperature: f64,
        max_tokens: u32,
    ) -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("构建 HTTP client 失败"),
            api_base,
            model,
            api_key,
            temperature,
            max_tokens,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/messages", self.api_base)
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    fn build_request(&self, messages: &[ChatMessage], stream: bool) -> serde_json::Value {
        let mut system_parts = Vec::new();
        let non_system: Vec<ChatMessage> = messages
            .iter()
            .filter(|m| {
                if m.role == "system" {
                    system_parts.push(m.content.clone());
                    false
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        let msgs = into_completion_messages(&non_system, &ProviderType::Anthropic);

        serde_json::json!({
            "model": self.model,
            "system": system_parts.join("\n\n"),
            "messages": msgs,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "stream": stream,
        })
    }
}

impl AnthropicProvider {
    pub async fn chat_stream<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        on_chunk: &mut F,
    ) -> Result<(), AiError> {
        let req_body = self.build_request(messages, true);
        let endpoint = self.endpoint();

        log::info!(
            "AI 请求: {} | 模型: {} | 消息数: {}",
            endpoint,
            self.model,
            messages.len()
        );

        let response = self
            .client
            .post(&endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| AiError::Network(format!("{} (端点: {})", e, endpoint)))?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let err_text = response.text().await.unwrap_or_default();
            log::error!("AI API 返回错误 ({status_code}): {err_text}");
            return Err(classify_http_error(status_code, &self.model, &err_text));
        }

        let mut stream = response.bytes_stream().eventsource();
        while let Some(event) = stream.next().await {
            let event = event.map_err(|error| AiError::Stream(format!("SSE 解析失败: {error}")))?;
            for chunk in parse_stream_payload(&event.data)? {
                on_chunk(&chunk);
            }
        }

        Ok(())
    }

    /// 流式对话（可选携带工具）。
    /// MVP：Anthropic 工具协议为 M2 范围，携带 tools 时明确报错（不静默降级）；
    /// 不携带 tools 时与原 chat_stream 行为一致。
    pub async fn chat_stream_with_tools<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        on_chunk: &mut F,
    ) -> Result<Vec<ToolCall>, AiError> {
        if tools.is_some() {
            return Err(AiError::Other(
                "Anthropic 协议的工具调用尚未实现（计划 M2）".to_string(),
            ));
        }
        self.chat_stream(messages, on_chunk).await?;
        Ok(Vec::new())
    }

    /// 非流式对话（可选携带工具）。Anthropic 协议的工具调用尚未实现（计划 M2），
    /// 携带工具时与流式路径 `chat_stream_with_tools` 保持一致的明确报错，
    /// 不走静默降级（否则模型会以为自己调用了工具）。
    pub async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<(String, Vec<ToolCall>), AiError> {
        if tools.is_some() {
            return Err(AiError::Other(
                "Anthropic 协议的工具调用尚未实现（计划 M2）".to_string(),
            ));
        }
        Ok((self.chat(messages).await?, Vec::new()))
    }

    /// 非流式对话 — 用于图片生成等不支持流式输出的模型。
    /// 解析 Anthropic Messages API 非流式响应（`content` 数组）。
    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<String, AiError> {
        let req_body = self.build_request(messages, false);
        let endpoint = self.endpoint();

        log::info!(
            "[非流式 Anthropic] AI 请求: {} | 模型: {} | 消息数: {}",
            endpoint,
            self.model,
            messages.len()
        );

        let response = self
            .client
            .post(&endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| AiError::Network(format!("{} (端点: {})", e, endpoint)))?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let err_text = response.text().await.unwrap_or_default();
            log::error!("[非流式 Anthropic] AI API 返回错误 ({status_code}): {err_text}");
            return Err(classify_http_error(status_code, &self.model, &err_text));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AiError::Stream(format!("非流式响应解析失败: {e}")))?;

        log::debug!(
            "[非流式 Anthropic] 响应 body 结构(前500): {}",
            serde_json::to_string_pretty(&body)
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect::<String>()
        );

        let mut results: Vec<String> = Vec::new();

        // Anthropic 非流式响应格式: { "content": [...] }
        if let Some(content_arr) = body["content"].as_array() {
            for block in content_arr {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                results.push(text.to_string());
                            }
                        }
                    }
                    "image" => {
                        if let Some(source) = block.get("source") {
                            let source_type =
                                source.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if source_type == "base64" {
                                if let (Some(data), Some(media_type)) = (
                                    source.get("data").and_then(|d| d.as_str()),
                                    source.get("media_type").and_then(|m| m.as_str()),
                                ) {
                                    log::info!(
                                        "[非流式 Anthropic] 提取到图片 ({}), {} bytes",
                                        media_type,
                                        data.len()
                                    );
                                    results.push(format!(
                                        "\n\n![生成的图片](data:{};base64,{})\n\n",
                                        media_type, data
                                    ));
                                }
                            }
                        }
                    }
                    "tool_use" => {
                        log::info!("[非流式 Anthropic] 跳过 tool_use block");
                    }
                    _ => {
                        log::info!(
                            "[非流式 Anthropic] 未知 content block type=\"{}\": {}",
                            block_type,
                            serde_json::to_string(block).unwrap_or_default()
                        );
                    }
                }
            }
        }

        let full_text = results.join("");
        log::info!(
            "[非流式 Anthropic] 最终提取结果: {} chars, 包含图片: {}",
            full_text.len(),
            full_text.contains("data:image") || full_text.contains("![")
        );

        Ok(full_text)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_stream_payload;

    #[test]
    fn parses_text_delta() {
        let chunks = parse_stream_payload(
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"你好"}}"#,
        )
        .unwrap();
        assert_eq!(chunks, vec!["你好"]);
    }

    #[test]
    fn ignores_non_text_delta() {
        let chunks = parse_stream_payload(
            r#"{"type":"content_block_delta","delta":{"type":"thinking_delta"}}"#,
        )
        .unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn returns_stream_error_event() {
        let result = parse_stream_payload(
            r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn rejects_malformed_event() {
        assert!(parse_stream_payload("not-json").is_err());
    }
}
