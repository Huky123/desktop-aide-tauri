use crate::ai_service::context::ChatMessage;
use crate::ai_service::error::{classify_http_error, AiError};
use crate::ai_service::preview_chars;
use crate::ai_service::provider::{into_completion_messages, ProviderType};
use crate::ai_service::providers::image_response::{
    detect_and_convert_image_urls, download_image_to_base64, image_url_to_markdown,
};
use crate::ai_service::tools::{ToolCall, ToolSpec};
use eventsource_stream::Eventsource;
use futures::StreamExt;

/// OpenAI 兼容格式的 Provider（OpenAI、DeepSeek、Ollama 等）
pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: String,
    temperature: f64,
    max_tokens: u32,
    provider_type: ProviderType,
}

fn uses_openai_reasoning_parameters(model: &str, provider_type: &ProviderType) -> bool {
    if !matches!(
        provider_type,
        ProviderType::OpenAI | ProviderType::OpenRouter
    ) {
        return false;
    }

    let model = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .to_ascii_lowercase();
    ["o1", "o3", "o4", "gpt-5"]
        .iter()
        .any(|prefix| model == *prefix || model.starts_with(&format!("{prefix}-")))
}

// ── 图片解析辅助函数 ──

/// 从 delta/message Value 中提取内容和图片，返回需要 emit 的文本片段列表
/// - 纯文本 content 字符串 → 直接返回（同时检测内嵌图片 URL）
/// - content 为数组（多模态格式）→ 遍历 parts，文本直接返回，图片转 Markdown
/// - content 缺失 → 检查 venus_multimodal_url 等已知图片字段
fn extract_content_texts(obj: &serde_json::Value) -> Vec<String> {
    let mut results = Vec::new();

    match obj.get("content") {
        // content 为字符串 → 直接返回（同时检测内嵌 URL）
        Some(serde_json::Value::String(text)) => {
            if !text.is_empty() {
                results.push(detect_and_convert_image_urls(text));
            }
        }
        // content 为数组（多模态响应格式）
        Some(serde_json::Value::Array(parts)) => {
            for part in parts {
                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match part_type {
                    "text" | "output_text" => {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                results.push(text.to_string());
                            }
                        }
                    }
                    "image_url" => {
                        // image_url 可能是对象 {"url": "..."} 或直接是字符串 "https://..."
                        let url = part
                            .get("image_url")
                            .and_then(|img_obj| {
                                img_obj
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .or_else(|| img_obj.as_str())
                            })
                            .or_else(|| part.get("url").and_then(|url| url.as_str()));
                        if let Some(url) = url {
                            log::info!(
                                "[SSE] 从 content 数组提取到 image_url: {}...",
                                preview_chars(url, 80)
                            );
                            results.push(image_url_to_markdown(url));
                        }
                    }
                    "image" | "image_base64" => {
                        // 多种图片数据子格式：
                        // 1. {"type":"image", "data":"...", "media_type":"..."}
                        // 2. {"type":"image", "image_url": {"url": "..."}}
                        // 3. {"type":"image", "source": {"data":"...", "media_type":"..."}}
                        // 4. {"type":"image", "url": "https://..."}
                        let mut extracted = false;

                        // 子格式 1: 直接 data 字段
                        if let Some(data) = part.get("data").and_then(|d| d.as_str()) {
                            let mime = part
                                .get("media_type")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            log::info!(
                                "[SSE] 从 content 数组提取到 inline image ({}), {} bytes",
                                mime,
                                data.len()
                            );
                            results.push(format!(
                                "\n\n![生成的图片](data:{};base64,{})\n\n",
                                mime, data
                            ));
                            extracted = true;
                        }

                        // 子格式 2: image_url 内含 URL
                        if !extracted {
                            if let Some(img_obj) = part.get("image_url") {
                                let url = img_obj
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .or_else(|| img_obj.as_str());
                                if let Some(url) = url {
                                    log::info!(
                                        "[SSE] 从 content 数组 image.image_url 提取到图片: {}...",
                                        preview_chars(url, 80)
                                    );
                                    results.push(image_url_to_markdown(url));
                                    extracted = true;
                                }
                            }
                        }

                        // 子格式 3: source 对象（Anthropic/Gemini 原生格式）
                        if !extracted {
                            if let Some(src) = part.get("source") {
                                if let Some(data) = src.get("data").and_then(|d| d.as_str()) {
                                    let mime = src
                                        .get("media_type")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("image/png");
                                    log::info!("[SSE] 从 content 数组 image.source 提取到图片 ({}), {} bytes", mime, data.len());
                                    results.push(format!(
                                        "\n\n![生成的图片](data:{};base64,{})\n\n",
                                        mime, data
                                    ));
                                    extracted = true;
                                }
                            }
                        }

                        // 子格式 4: 直接 url 字段
                        if !extracted {
                            if let Some(url) = part.get("url").and_then(|u| u.as_str()) {
                                log::info!(
                                    "[SSE] 从 content 数组 image.url 提取到图片: {}...",
                                    preview_chars(url, 80)
                                );
                                results.push(image_url_to_markdown(url));
                                extracted = true;
                            }
                        }

                        if !extracted {
                            log::info!(
                                "[SSE] content 数组中有 type=image 但无法解析: {}",
                                serde_json::to_string(part)
                                    .unwrap_or_else(|_| "<序列化失败>".to_string())
                            );
                        }
                    }
                    "inline_data" => {
                        // Gemini 原生格式: {"type":"inline_data","inline_data":{"data":"...","mime_type":"..."}}
                        if let Some(inline) = part.get("inline_data") {
                            if let Some(data) = inline.get("data").and_then(|d| d.as_str()) {
                                let mime = inline
                                    .get("mime_type")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("image/png");
                                log::info!(
                                    "[SSE] 从 content 数组提取到 inline_data ({}), {} bytes",
                                    mime,
                                    data.len()
                                );
                                results.push(format!(
                                    "\n\n![生成的图片](data:{};base64,{})\n\n",
                                    mime, data
                                ));
                            }
                        }
                    }
                    "" => {
                        // 没有 type 字段但可能直接含 url/image_url
                        if let Some(url) = part.get("url").and_then(|u| u.as_str()) {
                            log::info!(
                                "[SSE] 从 content 数组（无 type）提取到 url: {}...",
                                preview_chars(url, 80)
                            );
                            results.push(image_url_to_markdown(url));
                        } else if let Some(img_obj) = part.get("image_url") {
                            let url = img_obj
                                .get("url")
                                .and_then(|u| u.as_str())
                                .or_else(|| img_obj.as_str());
                            if let Some(url) = url {
                                log::info!(
                                    "[SSE] 从 content 数组（无 type）提取到 image_url: {}...",
                                    preview_chars(url, 80)
                                );
                                results.push(image_url_to_markdown(url));
                            }
                        }
                    }
                    _ => {
                        // 未知 type，记录以便排查（如 tool_use、thinking 等）
                        log::info!(
                            "[SSE] content 数组中未知 type=\"{}\": {}",
                            part_type,
                            serde_json::to_string(part)
                                .unwrap_or_else(|_| "<序列化失败>".to_string())
                        );
                    }
                }
            }
        }
        // content 缺失或为 null → 检查其他已知图片字段
        _ => {}
    }

    if results.is_empty() {
        if let Some(text) = obj.get("text").and_then(|text| text.as_str()) {
            if !text.is_empty() {
                results.push(detect_and_convert_image_urls(text));
            }
        }
    }

    // ── 检查 delta/message 层级的图片字段 ──

    // Venus API / 国内代理常见的图片 URL 字段
    if let Some(url) = obj.get("venus_multimodal_url").and_then(|v| v.as_str()) {
        log::info!(
            "[SSE] 从 venus_multimodal_url 提取到图片: {}...",
            preview_chars(url, 80)
        );
        results.push(image_url_to_markdown(url));
    }

    // 通用 image_url 字段（对象或字符串）
    if let Some(img_obj) = obj.get("image_url") {
        if let Some(url) = img_obj.get("url").and_then(|u| u.as_str()) {
            log::info!(
                "[SSE] 从 image_url 字段提取到图片: {}...",
                preview_chars(url, 80)
            );
            results.push(image_url_to_markdown(url));
        } else if let Some(url) = img_obj.as_str() {
            log::info!(
                "[SSE] 从 image_url 字段提取到图片: {}...",
                preview_chars(url, 80)
            );
            results.push(image_url_to_markdown(url));
        }
    }

    // images 数组（某些 API 用 images: [{url: "..."}]）
    if let Some(images) = obj.get("images").and_then(|v| v.as_array()) {
        for img in images {
            if let Some(url) = img.get("url").and_then(|u| u.as_str()) {
                log::info!(
                    "[SSE] 从 images 数组提取到图片: {}...",
                    preview_chars(url, 80)
                );
                results.push(image_url_to_markdown(url));
            } else if let Some(b64) = img
                .get("b64_json")
                .or_else(|| img.get("data"))
                .and_then(|d| d.as_str())
            {
                let mime = img
                    .get("mime_type")
                    .and_then(|m| m.as_str())
                    .unwrap_or("image/png");
                log::info!(
                    "[SSE] 从 images 数组提取到 base64 图片 ({}), {} bytes",
                    mime,
                    b64.len()
                );
                results.push(format!(
                    "\n\n![生成的图片](data:{};base64,{})\n\n",
                    mime, b64
                ));
            }
        }
    }

    // data 字段直接是 base64 图片（某些 API）
    if let Some(data) = obj.get("data").and_then(|d| d.as_str()) {
        if data.len() > 100 {
            let mime = obj
                .get("mime_type")
                .and_then(|m| m.as_str())
                .unwrap_or("image/png");
            log::info!(
                "[SSE] 从 data 字段提取到 base64 图片 ({}), {} bytes",
                mime,
                data.len()
            );
            results.push(format!(
                "\n\n![生成的图片](data:{};base64,{})\n\n",
                mime, data
            ));
        }
    }

    results
}

// ── 工具调用（tool_calls）流式解析 ──

/// 按 index 累积的分片工具调用（arguments 为分段 JSON 字符串，流结束后整体 parse）
#[derive(Debug, Default)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// 单次流式请求的 tool_calls 累积状态（按 index 合并）
#[derive(Debug, Default)]
struct ToolCallAccumulator {
    by_index: std::collections::HashMap<usize, PendingToolCall>,
}

/// 从已解析的 SSE JSON 中提取 delta.tool_calls 增量（幂等，可重复调用）
fn extract_tool_call_deltas(value: &serde_json::Value, acc: &mut ToolCallAccumulator) {
    let Some(choice) = value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
    else {
        return;
    };
    let Some(delta) = choice.get("delta") else {
        return;
    };
    let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) else {
        return;
    };
    for item in tool_calls {
        let Some(index) = item.get("index").and_then(|i| i.as_u64()) else {
            continue;
        };
        let index = index as usize;
        let entry = acc
            .by_index
            .entry(index)
            .or_insert_with(|| PendingToolCall {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
            });
        // id / name 只出现在该 index 的首个 chunk，非空时不再覆盖
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            if entry.id.is_empty() {
                entry.id = id.to_string();
            }
        }
        if let Some(func) = item.get("function") {
            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                if entry.name.is_empty() {
                    entry.name = name.to_string();
                }
            }
            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                entry.arguments.push_str(args);
            }
        }
    }
}

/// 流结束后把累积结果固化为 ToolCall 列表（按 index 排序；arguments 整体 parse）
fn finalize_tool_calls(acc: ToolCallAccumulator) -> Result<Vec<ToolCall>, AiError> {
    let mut entries: Vec<(usize, PendingToolCall)> = acc.by_index.into_iter().collect();
    entries.sort_by_key(|(index, _)| *index);

    let mut calls = Vec::new();
    for (index, entry) in entries {
        if entry.id.is_empty() || entry.name.is_empty() {
            log::warn!("[SSE] tool_calls[{index}] 缺少 id/name，跳过");
            continue;
        }
        let arguments: serde_json::Value = if entry.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&entry.arguments).map_err(|error| {
                AiError::Stream(format!(
                    "tool_calls[{index}] arguments 不是合法 JSON: {error}；原始: {}",
                    entry.arguments
                ))
            })?
        };
        calls.push(ToolCall {
            id: entry.id,
            name: entry.name,
            arguments,
        });
    }
    Ok(calls)
}

fn parse_stream_payload(
    data: &str,
    acc: &mut ToolCallAccumulator,
) -> Result<Option<Vec<String>>, AiError> {
    if data.trim() == "[DONE]" {
        return Ok(None);
    }

    let value: serde_json::Value = serde_json::from_str(data).map_err(|error| {
        let preview: String = data.chars().take(300).collect();
        AiError::Stream(format!("无法解析流式响应 JSON: {error}；数据: {preview}"))
    })?;

    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(|message| message.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string());
        return Err(AiError::Stream(message));
    }

    extract_tool_call_deltas(&value, acc);

    let mut results = extract_content_texts(&value);
    let Some(choice) = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
    else {
        return Ok(Some(results));
    };

    results.extend(extract_content_texts(choice));
    let delta_or_message = choice.get("delta").or_else(|| choice.get("message"));

    if let Some(content) = delta_or_message {
        let content_results = extract_content_texts(content);
        if content_results.is_empty() {
            let known_metadata = content.as_object().is_some_and(|object| {
                object.keys().all(|key| {
                    matches!(
                        key.as_str(),
                        "role"
                            | "content"
                            | "finish_reason"
                            | "tool_calls"
                            | "refusal"
                            | "reasoning_content"
                            | "reasoning"
                    )
                })
            });
            if !known_metadata && content.as_object().is_some_and(|object| !object.is_empty()) {
                log::warn!("[SSE] 未识别的 delta/message 格式: {content}");
            }
        }
        results.extend(content_results);
    }

    Ok(Some(results))
}

impl OpenAiCompatibleProvider {
    pub fn new(
        endpoint: String,
        model: String,
        api_key: String,
        temperature: f64,
        max_tokens: u32,
        provider_type: ProviderType,
    ) -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("构建 HTTP client 失败"),
            endpoint,
            model,
            api_key,
            temperature,
            max_tokens,
            provider_type,
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    fn image_endpoint(&self) -> String {
        self.endpoint
            .strip_suffix("/chat/completions")
            .map(|base| format!("{base}/images/generations"))
            .unwrap_or_else(|| {
                format!("{}/images/generations", self.endpoint.trim_end_matches('/'))
            })
    }

    fn build_request(
        &self,
        messages: &[ChatMessage],
        stream: bool,
        tools: Option<&[ToolSpec]>,
    ) -> serde_json::Value {
        // 所有 OpenAI 兼容 API 使用相同的多模态格式
        let msgs = into_completion_messages(messages, &ProviderType::OpenAI);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": msgs,
            "stream": stream,
        });

        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect::<Vec<_>>());
        }

        if uses_openai_reasoning_parameters(&self.model, &self.provider_type) {
            body["max_completion_tokens"] = serde_json::json!(self.max_tokens);
        } else {
            body["temperature"] = serde_json::json!(self.temperature);
            body["max_tokens"] = serde_json::json!(self.max_tokens);
        }

        body
    }
}

impl OpenAiCompatibleProvider {
    /// 调用独立的图片生成接口（`POST {base}/images/generations`）。
    ///
    /// **该不该走这条通路由 `Agent::inference_mode()` 决定**，provider 不再自己
    /// 判断——原先这里有一句 `if !self.uses_dedicated_image_endpoint() { return
    /// Err("没有配置独立图片生成接口") }`，是"provider 越权做 policy"的产物，
    /// 也是"命令层以为走 A、实际发到 B"这类不一致的来源之一。
    pub async fn generate_image(&self, prompt: &str) -> Result<String, AiError> {
        let endpoint = self.image_endpoint();
        let response = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": prompt,
                "n": 1,
            }))
            .send()
            .await
            .map_err(|error| AiError::Network(format!("{} (端点: {})", error, endpoint)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(classify_http_error(
                status.as_u16(),
                &self.model,
                &error_text,
            ));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| AiError::Stream(format!("图片生成响应解析失败: {error}")))?;
        if let Some(error) = body.get("error") {
            return Err(AiError::Other(format!("图片生成失败: {error}")));
        }

        let mut images = Vec::new();
        if let Some(items) = body.get("data").and_then(|data| data.as_array()) {
            for item in items {
                if let Some(base64) = item.get("b64_json").and_then(|data| data.as_str()) {
                    images.push(format!(
                        "\n\n![生成的图片](data:image/png;base64,{base64})\n\n"
                    ));
                    continue;
                }
                if let Some(url) = item.get("url").and_then(|url| url.as_str()) {
                    let durable_url = download_image_to_base64(url)
                        .await
                        .unwrap_or_else(|| url.to_string());
                    images.push(image_url_to_markdown(&durable_url));
                }
            }
        }

        if images.is_empty() {
            return Err(AiError::Other(format!(
                "图片生成接口未返回图片数据: {}",
                body.to_string().chars().take(500).collect::<String>()
            )));
        }

        Ok(images.join(""))
    }

    /// 流式对话（可选携带工具）。文本逐 chunk 经 on_chunk 回调；
    /// 返回本轮模型请求的工具调用列表（无工具调用时为空列表）。
    pub async fn chat_stream_with_tools<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        on_chunk: &mut F,
    ) -> Result<Vec<ToolCall>, AiError> {
        let req_body = self.build_request(messages, true, tools);

        log::info!(
            "AI 请求: {} | 模型: {} | 消息数: {} | tools: {}",
            self.endpoint,
            self.model,
            messages.len(),
            tools.map(|t| t.len()).unwrap_or(0)
        );

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| AiError::Network(format!("{} (端点: {})", e, self.endpoint)))?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let err_text = response.text().await.unwrap_or_default();
            log::error!("AI API 返回错误 ({status_code}): {err_text}");
            return Err(classify_http_error(status_code, &self.model, &err_text));
        }

        let mut acc = ToolCallAccumulator::default();
        let mut stream = response.bytes_stream().eventsource();
        while let Some(event) = stream.next().await {
            let event = event.map_err(|error| AiError::Stream(format!("SSE 解析失败: {error}")))?;
            let Some(chunks) = parse_stream_payload(&event.data, &mut acc)? else {
                break;
            };
            for chunk in chunks {
                on_chunk(&chunk);
            }
        }

        finalize_tool_calls(acc)
    }

    /// 非流式对话（可选携带工具）——用于图片生成等不支持流式输出的模型。
    ///
    /// 出图模型用不了流式端点（图片必须整张生成完才有像素数据），但仍然可能支持
    /// 工具调用（例如"先联网搜、再出图"）。因此把 `tools` 做成独立参数：
    /// 请求体复用 `build_request`，响应同时解析文本（含图片 Markdown）
    /// 与 `choices[0].message.tool_calls`。
    pub async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<(String, Vec<ToolCall>), AiError> {
        let req_body = self.build_request(messages, false, tools);

        log::info!(
            "[非流式] AI 请求: {} | 模型: {} | 消息数: {} | tools: {}",
            self.endpoint,
            self.model,
            messages.len(),
            tools.map(|t| t.len()).unwrap_or(0)
        );

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| AiError::Network(format!("{} (端点: {})", e, self.endpoint)))?;

        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("未知")
            .to_string();
        log::info!(
            "[非流式] 响应状态: {}, Content-Type: {}",
            status.as_u16(),
            content_type
        );

        if !status.is_success() {
            let err_text = response.text().await.unwrap_or_default();
            log::error!("[非流式] AI API 返回错误 ({status}): {err_text}");
            return Err(classify_http_error(status.as_u16(), &self.model, &err_text));
        }

        // 先读取原始文本，记录日志以便诊断格式问题
        let raw_text = response
            .text()
            .await
            .map_err(|e| AiError::Stream(format!("非流式响应读取失败: {e}")))?;

        log::debug!(
            "[非流式] 原始响应(前800): {}",
            preview_chars(&raw_text, 800)
        );

        // 解析 JSON
        let body: serde_json::Value = serde_json::from_str(&raw_text).map_err(|e| {
            log::error!(
                "[非流式] JSON 解析失败: {e} — 原始响应(前300): {}",
                preview_chars(&raw_text, 300)
            );
            AiError::Stream(format!("非流式响应 JSON 解析失败: {e}"))
        })?;

        let tool_calls = parse_completion_tool_calls(&body)?;
        let full_text = extract_completion_text(&body);

        log::info!(
            "[非流式] 最终提取结果: {} chars, 包含图片: {}, tool_calls: {}",
            full_text.len(),
            full_text.contains("data:image") || full_text.contains("!["),
            tool_calls.len()
        );

        Ok((full_text, tool_calls))
    }
}

/// 从非流式响应中提取文本与图片 Markdown。
///
/// 覆盖各家兼容实现的多种形态：顶层图片字段、`choices[0].message`、
/// `message.images[]`、`message.venus_multimodal_url`、`data[]` 等。
fn extract_completion_text(body: &serde_json::Value) -> String {
    let mut results: Vec<String> = Vec::new();

    // 1. 检查 body 顶层图片字段
    results.extend(extract_content_texts(body));

    // 2. 解析 choices[0].message
    if let Some(choices) = body["choices"].as_array() {
        if let Some(choice) = choices.first() {
            // choice 层级
            results.extend(extract_content_texts(choice));

            // message 层级（核心）
            if let Some(message) = choice.get("message") {
                log::debug!(
                    "[非流式] message keys: {:?}",
                    message.as_object().map(|o| o.keys().collect::<Vec<_>>())
                );

                // 特殊处理：部分代理 API 把图片放在 message.images 数组中
                if let Some(images) = message.get("images").and_then(|v| v.as_array()) {
                    for img in images {
                        if let Some(url) = img.get("url").and_then(|u| u.as_str()) {
                            log::info!(
                                "[非流式] 从 message.images[].url 提取到图片: {}...",
                                preview_chars(url, 80)
                            );
                            results.push(image_url_to_markdown(url));
                        }
                        if let Some(b64) = img
                            .get("b64_json")
                            .or_else(|| img.get("data"))
                            .and_then(|d| d.as_str())
                        {
                            log::info!(
                                "[非流式] 从 message.images[].data 提取到图片, {} bytes",
                                b64.len()
                            );
                            let mime = img
                                .get("mime_type")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            results.push(format!(
                                "\n\n![生成的图片](data:{};base64,{})\n\n",
                                mime, b64
                            ));
                        }
                    }
                }

                // 特殊处理：部分代理 API 把图片放在 message.venus_multimodal_url 字段
                if let Some(url) = message.get("venus_multimodal_url").and_then(|v| v.as_str()) {
                    log::info!(
                        "[非流式] 从 message.venus_multimodal_url 提取到图片: {}...",
                        preview_chars(url, 80)
                    );
                    results.push(image_url_to_markdown(url));
                }

                // 通用 content 提取
                let msg_texts = extract_content_texts(message);
                if msg_texts.is_empty() && results.is_empty() {
                    log::info!(
                            "[非流式] extract_content_texts 未从 message 提取到内容，message 完整内容(前500): {}",
                            serde_json::to_string_pretty(message)
                                .unwrap_or_default()
                                .chars()
                                .take(500)
                                .collect::<String>()
                        );
                }
                results.extend(msg_texts);

                // 如果 message.content 不是数组而是字符串但含有已渲染图片
                if let Some(content_str) = message.get("content").and_then(|c| c.as_str()) {
                    if !content_str.is_empty() && results.is_empty() {
                        // 纯字符串 content，检测是否有内嵌图片 URL
                        let converted = detect_and_convert_image_urls(content_str);
                        results.push(converted);
                    }
                }
            }
        }
    }

    // 3. 检查 data/images 等顶层字段（DALL-E 风格）
    if let Some(data_arr) = body["data"].as_array() {
        for item in data_arr {
            if let Some(url) = item.get("url").and_then(|u| u.as_str()) {
                log::info!(
                    "[非流式] 从 data[].url 提取到图片: {}...",
                    preview_chars(url, 80)
                );
                results.push(image_url_to_markdown(url));
            }
            if let Some(b64) = item.get("b64_json").and_then(|d| d.as_str()) {
                log::info!(
                    "[非流式] 从 data[].b64_json 提取到图片, {} bytes",
                    b64.len()
                );
                results.push(format!(
                    "\n\n![生成的图片](data:image/png;base64,{})\n\n",
                    b64
                ));
            }
        }
    }

    results.join("")
}

/// 从非流式响应中解析 `choices[0].message.tool_calls`（OpenAI 协议）。
///
/// 与流式解析器 `finalize_tool_calls` 的差别：非流式响应一次性给出完整的
/// `arguments` 字符串，无需按 index 累积分片；其余校验保持一致
/// （id/name 缺失则跳过该条，arguments 非法 JSON 视为协议错误）。
fn parse_completion_tool_calls(body: &serde_json::Value) -> Result<Vec<ToolCall>, AiError> {
    let Some(raw_calls) = body
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("tool_calls"))
        .and_then(|calls| calls.as_array())
    else {
        return Ok(Vec::new());
    };

    let mut calls = Vec::new();
    for (index, item) in raw_calls.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let name = item
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if id.is_empty() || name.is_empty() {
            log::warn!("[非流式] tool_calls[{index}] 缺少 id/name，跳过");
            continue;
        }
        let raw_arguments = item
            .get("function")
            .and_then(|function| function.get("arguments"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let arguments: serde_json::Value = if raw_arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(raw_arguments).map_err(|error| {
                AiError::Stream(format!(
                    "tool_calls[{index}] arguments 不是合法 JSON: {error}；原始: {}",
                    preview_chars(raw_arguments, 500)
                ))
            })?
        };
        calls.push(ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        });
    }
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::{
        extract_completion_text, finalize_tool_calls, parse_completion_tool_calls,
        parse_stream_payload, uses_openai_reasoning_parameters, ToolCallAccumulator,
    };
    use crate::ai_service::provider::ProviderType;

    #[test]
    fn parses_openai_text_delta() {
        let chunks = parse_stream_payload(
            r#"{"choices":[{"delta":{"content":"你好"}}]}"#,
            &mut ToolCallAccumulator::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(chunks, vec!["你好"]);
    }

    #[test]
    fn accepts_reasoning_metadata_without_exposing_it_as_answer() {
        let chunks = parse_stream_payload(
            r#"{"choices":[{"delta":{"reasoning_content":"internal"}}]}"#,
            &mut ToolCallAccumulator::default(),
        )
        .unwrap()
        .unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn parses_message_style_stream_event() {
        let chunks = parse_stream_payload(
            r#"{"choices":[{"message":{"content":[{"type":"text","text":"完成"}]}}]}"#,
            &mut ToolCallAccumulator::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(chunks, vec!["完成"]);
    }

    #[test]
    fn recognizes_done_marker() {
        assert!(
            parse_stream_payload("[DONE]", &mut ToolCallAccumulator::default())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_malformed_json_instead_of_dropping_it() {
        assert!(parse_stream_payload("{broken", &mut ToolCallAccumulator::default()).is_err());
    }

    #[test]
    fn accumulates_tool_calls_across_chunks() {
        let mut acc = ToolCallAccumulator::default();
        // 首 chunk：id + name + 空 arguments
        parse_stream_payload(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_current_time","arguments":""}}]}}]}"#,
            &mut acc,
        )
        .unwrap();
        // 后续 chunk：arguments 分片
        parse_stream_payload(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]}"#,
            &mut acc,
        )
        .unwrap();
        let calls = finalize_tool_calls(acc).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "get_current_time");
        assert_eq!(calls[0].arguments, serde_json::json!({}));
    }

    #[test]
    fn rejects_malformed_tool_call_arguments() {
        let mut acc = ToolCallAccumulator::default();
        parse_stream_payload(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_current_time","arguments":"{"}}]}}]}"#,
            &mut acc,
        )
        .unwrap();
        assert!(finalize_tool_calls(acc).is_err());
    }

    #[test]
    fn skips_tool_calls_without_id_or_name() {
        let mut acc = ToolCallAccumulator::default();
        parse_stream_payload(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]}"#,
            &mut acc,
        )
        .unwrap();
        assert!(finalize_tool_calls(acc).unwrap().is_empty());
    }

    #[test]
    fn builds_request_with_tools() {
        let provider = super::OpenAiCompatibleProvider::new(
            "https://api.example.com/v1/chat/completions".to_string(),
            "test-model".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        );
        let tools = crate::ai_service::tools::builtin_tools();
        let body = provider.build_request(&[], true, Some(&tools));
        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["function"]["name"], "get_current_time");
        // 不带 tools 时不应出现 tools 字段
        let body = provider.build_request(&[], true, None);
        assert!(body.get("tools").is_none());
    }

    // ── 非流式 tool_calls 解析（出图模型带工具的路径）──

    #[test]
    fn parses_non_streaming_tool_calls() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "我先查一下时间。",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "get_current_time", "arguments": "{}" }
                    }]
                }
            }]
        });
        let calls = parse_completion_tool_calls(&body).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "get_current_time");
        assert_eq!(calls[0].arguments, serde_json::json!({}));
        // 文本与工具调用必须同时可用（ReAct 循环两者都要）
        assert!(extract_completion_text(&body).contains("我先查一下时间"));
    }

    #[test]
    fn non_streaming_message_without_tool_calls_yields_empty_list() {
        let body = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "![生成的图片](data:image/png;base64,AAA)" } }]
        });
        assert!(parse_completion_tool_calls(&body).unwrap().is_empty());
        assert!(extract_completion_text(&body).contains("data:image/png"));
    }

    #[test]
    fn rejects_malformed_non_streaming_tool_call_arguments() {
        let body = serde_json::json!({
            "choices": [{ "message": { "tool_calls": [{
                "id": "call_1",
                "function": { "name": "web_search", "arguments": "{" }
            }] } }]
        });
        assert!(parse_completion_tool_calls(&body).is_err());
    }

    #[test]
    fn skips_non_streaming_tool_calls_without_id_or_name() {
        let body = serde_json::json!({
            "choices": [{ "message": { "tool_calls": [{
                "function": { "arguments": "{}" }
            }] } }]
        });
        assert!(parse_completion_tool_calls(&body).unwrap().is_empty());
    }

    #[test]
    fn identifies_openai_reasoning_parameter_models() {
        assert!(uses_openai_reasoning_parameters(
            "gpt-5-mini",
            &ProviderType::OpenAI
        ));
        assert!(uses_openai_reasoning_parameters(
            "openai/o3-mini",
            &ProviderType::OpenRouter
        ));
        assert!(!uses_openai_reasoning_parameters(
            "deepseek-reasoner",
            &ProviderType::DeepSeek
        ));
        assert!(!uses_openai_reasoning_parameters(
            "gpt-4o",
            &ProviderType::OpenAI
        ));
    }

    /// `image_endpoint()` 从 chat 端点反推。第二例是真实故障现场的网关形态
    /// （custom provider + 出图模型），用于钉死 URL 推导不再依赖 provider 类型。
    #[test]
    fn derives_image_endpoint_from_chat_endpoint() {
        let cases = [
            (
                "https://api.x.ai/v1/chat/completions",
                "https://api.x.ai/v1/images/generations",
            ),
            (
                "https://cn-api.ooapi.cc/v1/chat/completions",
                "https://cn-api.ooapi.cc/v1/images/generations",
            ),
            // 没带 /chat/completions 后缀时按原样拼接
            (
                "https://example.com/v1",
                "https://example.com/v1/images/generations",
            ),
        ];
        for (endpoint, expected) in cases {
            let provider = super::OpenAiCompatibleProvider::new(
                endpoint.to_string(),
                "gpt-image-2.5-flare".to_string(),
                "key".to_string(),
                0.7,
                1024,
                ProviderType::Custom,
            );
            assert_eq!(provider.image_endpoint(), expected, "endpoint={endpoint}");
        }
    }
}
