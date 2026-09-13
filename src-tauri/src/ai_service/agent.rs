use crate::ai_service::context::{ChatMessage, ContextManager, ImageContent};
use crate::ai_service::error::AiError;
use crate::ai_service::invocation::{self, InferenceMode};
use crate::ai_service::prompt;
use crate::ai_service::provider::AiProvider;
use crate::ai_service::tools::{execute_tool, ToolCall, ToolContext, ToolOutput, ToolSpec};

/// 工具轮次上限（防止模型死循环；联网搜索等多步任务预留更多轮次）
const MAX_TOOL_ROUNDS: usize = 8;
/// 单条工具结果回传的最大长度（超出截断，防上下文膨胀）
const TOOL_RESULT_MAX_CHARS: usize = 20_000;

pub(crate) fn is_image_generation_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("gpt-image")
        || model.contains("dall-e")
        || model.contains("imagen")
        || model.contains("image-generation")
        || model.contains("imagegen")
        || model.contains("-image")
        || model.contains("image-")
}

/// 模型用途（用户设置，默认 `Auto`）
///
/// 背景：`is_image_generation_model()` 的 *名字启发式**会误判——任何名字含 `-image`/`image-`
/// 的模型（例如把视觉理解模型命名为 `xxx-image-vl` 的网关）都会被当成出图模型，
/// 于是跳过流式、还出不了图。因此把"这个模型能不能出图"交给用户显式指定。
///
/// 注意：本枚举**只管"是否出图"这一个维度**。「能否识图」由 `vision::VisionMode`
/// 独立控制（见设置里的「图片输入方式」），两者互不排斥——一个模型完全可以既能识图
/// 又能出图（如 gemini-2.5-flash-image），此时两项能力都开启即可。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    /// 自动：按模型名启发式判断（默认）
    Auto,
    /// 普通对话/识图模型：绝不按出图模型处理（用于破除名字启发式误判）
    Chat,
    /// 出图模型：走非流式通路（图片无法按增量 delta 传输）
    Image,
}

impl ModelKind {
    /// 从配置字符串解析（未知值按 `Auto` 处理）
    ///
    /// `"vision"` 是旧版四选一里的「识图」选项，现已拆分为独立的
    /// `vision_mode = "on"`；这里按 `Auto` 兜底（字段迁移在配置加载层完成）。
    pub fn from_config(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "chat" => Self::Chat,
            "image" => Self::Image,
            _ => Self::Auto,
        }
    }

    /// 该模型是否按"出图模型"处理（用户显式标记优先，其次才按名字猜）
    pub fn is_image_model(self, model: &str) -> bool {
        match self {
            Self::Image => true,
            Self::Chat => false,
            Self::Auto => is_image_generation_model(model),
        }
    }
}

/// 一次工具闭环请求的输入。
///
/// 收敛成结构体而非 6 个位置参数：调用点用字段名表达意图
/// （尤其 `stream` 与 `tools` 顺序相邻，位置传参极易搞错）。
pub struct ToolRunRequest<'a> {
    /// 本轮系统提示（已按需叠加当轮指令）
    pub system_prompt: &'a str,
    /// 用户本轮输入（图片走 OCR 时识别结果已并入）
    pub user_input: &'a str,
    /// 图片附件（已按识图决策处理：要么原图，要么为空）
    pub images: Vec<ImageContent>,
    /// 当轮要提供给模型的工具列表
    pub tools: &'a [ToolSpec],
    /// 工具执行上下文（数据库 / 提醒调度 / 联网配置）
    pub ctx: &'a ToolContext,
    /// 是否走流式传输，以及**走哪条通路**。出图模型必须为 `ChatBlocking`
    /// 或 `ImageGeneration`（图片拿不到增量 delta）。
    pub mode: InferenceMode,
}

/// ReAct 风格的 Agent 循环
///
/// 当前阶段：单步对话 `run()` 覆盖核心场景；
/// `run_with_tools()` 提供工具调用最小闭环（MVP）。
/// 已探明的通路事实（每个 Agent 生命周期最多学到一条）。
///
/// 有两件事静态判定不了，只能发一次请求试出来：
/// 1. 这个网关**有没有**独立出图端点（有的在 `/images/generations` 出图，
///    有的把图片放进对话响应的 Markdown 里）；
/// 2. 这个模型是否**不**在对话端点服务（用户漏标「出图模型」时就会这样）。
///
/// 试出来之后记在这里，后续轮次不再重复探测。记忆范围就是 Agent 的生命周期：
/// Agent 在 AI 配置变更时整体重建（`commands::ai::save_config` 与 `lib::run`），
/// 等于按配置自动失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteFinding {
    /// 还没试过：先按推断的通路走
    Unknown,
    /// 网关没有独立出图端点 → 出图模型改走对话非流式
    NoImagesRoute,
    /// 模型不在对话端点服务 → 改走专用出图接口
    NoChatRoute,
}

impl RouteFinding {
    /// 从配置字符串恢复（未知值按 `Unknown` 处理，绝不 panic）
    fn from_config(value: &str) -> Self {
        match value {
            "no_images_route" => Self::NoImagesRoute,
            "no_chat_route" => Self::NoChatRoute,
            _ => Self::Unknown,
        }
    }

    /// 写入配置用的字符串（`save_config` 原样保留，换配置时由命令层清空）
    fn as_config(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::NoImagesRoute => "no_images_route",
            Self::NoChatRoute => "no_chat_route",
        }
    }
}

pub struct Agent {
    provider: AiProvider,
    context: ContextManager,
    /// 模型用途（由用户配置解析而来，决定是否按出图模型走非流式通路）
    model_kind: ModelKind,
    /// 已探明的通路事实（见 `RouteFinding`）
    route_finding: RouteFinding,
}

/// 截断过长的工具结果（保留开头，追加截断标记）
/// 取出最后一条用户消息作为出图 prompt（`/images/generations` 只接受这个字段）
fn last_user_prompt(messages: &[ChatMessage]) -> &str {
    messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.as_str())
        .unwrap_or_default()
}

/// 截断过长的工具结果（保留开头，追加截断标记）
fn truncate_tool_result(text: &str) -> String {
    if text.len() <= TOOL_RESULT_MAX_CHARS {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(TOOL_RESULT_MAX_CHARS).collect();
    clipped.push_str("\n…[结果已截断]");
    clipped
}

/// 判断一次失败是否**由"服务端不接受 tools 参数"引起**——只有这种失败才值得
/// "摘掉工具重试一次"。
///
/// 真实反例（网关日志）：出图模型被要求走 chat/completions 时返回
/// `This model is not supported on the Chat Completions endpoint`，
/// 摘掉 tools 重试一次照样 400，只是让日志多出一条一模一样的报错。
///
/// 取舍：宁可漏掉极少数"服务端用完全不含 tool 字样的措辞拒绝工具"的网关
/// （那种情况下用户会看到明确报错，可在设置里关掉工具调用））
/// 也不想每次真失败都白发一次请求。
fn looks_like_tools_rejection(error: &AiError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("tool") || text.contains("function") || text.contains("工具")
}

/// 模型级 404 的典型措辞：**端点存在**，只是不服务这个模型。
///
/// 命中任何一个就不算"没有出图端点"，因为那会把结论记成
/// "这个网关没有该路由"并写进配置（`learned_transport`），从此不再尝试出图端点。
const MODEL_LEVEL_404_MARKERS: [&str; 6] = [
    // Google：models/gemini-3.1-flash-image is not found for API version v1main,
    //         or is not supported for predict
    "not found for api version",
    "is not supported for predict",
    // 通用网关
    "model_not_found",
    "model not found",
    "model does not exist",
    // Google 的路径式模型名（真实路由 404 的 body 里不会出现）
    "models/",
];

/// 判断出图接口的失败是否说明"**这个网关没有该路由**"——只有这种失败才值得
/// 回退到对话非流式端点。
///
/// 信号收窄到「404（`AiError::InvalidModel`）**且不是模型级 404**」：
/// - 路由级 404（`404 page not found` 之类）= 路径不存在 ⇒ 换个端点确实有意义；
/// - **模型级 404**（如 Google 的 `is not supported for predict`）= 端点存在，
///   只是这个模型不在上面 ⇒ 回退救不了，而且会**学错**：日志打「出图端点不存在」、
///   并把 `no_images_route` 写进配置；
/// - 400 参数错、401 鉴权、429 限流、5xx 服务端故障，都是"路由存在、只是这次没成功"。
///   回退只会把清晰的错误换成另一个更含糊的错误——典型是 `n` 参数不被接受时，
///   回退后报的是 chat 端点的
///   `This model is not supported on the Chat Completions endpoint`，真正原因被埋掉。
fn images_route_missing(error: &AiError) -> bool {
    let AiError::InvalidModel(detail) = error else {
        return false;
    };
    let text = detail.to_ascii_lowercase();
    !MODEL_LEVEL_404_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

/// 判断失败是否说明"**这个模型不在对话端点上服务**"——只有这种失败才值得
/// 改走专用出图接口。
///
/// 信号是服务端的**确定陈述**（真实日志逐字）：
/// `This model is not supported on the Chat Completions endpoint`。
///
/// 收窄到 `AiError::Other`（即 `classify_http_error` 里未归类的 4xx，典型是 400）：
/// 401/403/404/429/5xx 各有自己的变体，不会走到这里；网络错误与参数错误
/// 也不会这样措辞。所以命中即"这个模型不该走对话端点"。
fn looks_like_chat_endpoint_rejection(error: &AiError) -> bool {
    if !matches!(error, AiError::Other(_)) {
        return false;
    }
    let text = error.to_string().to_ascii_lowercase();
    text.contains("chat completion") && (text.contains("not supported") || text.contains("不支持"))
}

impl Agent {
    pub fn new(provider: AiProvider) -> Self {
        Self {
            provider,
            context: ContextManager::default(),
            model_kind: ModelKind::Auto,
            route_finding: RouteFinding::Unknown,
        }
    }

    /// 设置模型用途（用户配置变更时由命令层调用；不需要重建 Agent）
    pub fn set_model_kind(&mut self, kind: ModelKind) {
        self.model_kind = kind;
    }

    /// 本次会话探明的通路事实（写回配置用；没探明时是空串）
    pub fn learned_transport(&self) -> &'static str {
        self.route_finding.as_config()
    }

    /// 从配置恢复上次探明的通路事实（Agent 重建时调用，省掉一次重新探测）
    pub fn set_learned_transport(&mut self, value: &str) {
        self.route_finding = RouteFinding::from_config(value);
    }

    /// 当前模型是否按"出图模型"处理
    fn treats_as_image_model(&self) -> bool {
        self.model_kind.is_image_model(self.provider.model_name())
    }

    /// **全项目唯一的通路决策点。**
    ///
    /// 命令层、`stream_messages`、工具闭环都只读这个值，不再各自用
    /// provider 类型 / 模型名重新推断——改造前四处各判一次、用两个不同的谓词，
    /// 是"日志说走 A、实际发到 B"的根源。
    ///
    /// 判定依据只有两件事：**用户声明的模型用途** × **协议形态是否可能有出图端点**
    /// （不再看 `provider_type` 这个名字）。再叠加已探明的通路事实：
    ///
    /// - 探明没有出图端点 → 降级 `ChatBlocking`（注意 `use_tools` 会由 false 变 true，
    ///   正是"图片在对话响应里返回"那类网关应有的行为）；
    /// - 探明模型不在对话端点 → 直接 `ImageGeneration`，即使用户漏标了「出图模型」。
    pub fn inference_mode(&self) -> InferenceMode {
        let mode = invocation::resolve(
            self.treats_as_image_model(),
            self.provider.may_have_images_endpoint(),
        );
        match (mode, self.route_finding) {
            (InferenceMode::ImageGeneration, RouteFinding::NoImagesRoute) => {
                InferenceMode::ChatBlocking
            }
            (InferenceMode::ChatStream | InferenceMode::ChatBlocking, RouteFinding::NoChatRoute) => {
                InferenceMode::ImageGeneration
            }
            _ => mode,
        }
    }

    /// 改走专用出图接口，并记住这个结论。
    ///
    /// 用于"用户漏标「出图模型」"的自愈：此时推断出的通路是对话端点，而服务端会
    /// 明确回绝（`This model is not supported on the Chat Completions endpoint`）。
    /// 与其把这个看不懂的报错丢给用户，不如直接换到出图接口重试一次。
    async fn generate_image_fallback(
        &mut self,
        messages: &[ChatMessage],
    ) -> Result<String, AiError> {
        let prompt = last_user_prompt(messages);
        if prompt.trim().is_empty() {
            return Err(AiError::Other("图片描述不能为空".to_string()));
        }
        let text = self.provider.generate_image(prompt).await?;
        self.route_finding = RouteFinding::NoChatRoute;
        Ok(text)
    }

    /// 单轮请求：按 `mode` 选择传输方式，文本统一经 `on_chunk` 输出。
    ///
    /// 拆成独立方法是为了让工具闭环在两种传输方式下复用同一套逻辑：
    /// 出图模型必须走非流式（图片拿不到增量 delta），普通模型走流式。
    /// 返回 (本轮文本, 本轮工具调用)。
    async fn request_round<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        mode: InferenceMode,
        on_chunk: &mut F,
    ) -> Result<(String, Vec<ToolCall>), AiError> {
        if mode.streams() {
            let mut text = String::new();
            let tool_calls = self
                .provider
                .chat_stream_with_tools(messages, tools, &mut |chunk| {
                    text.push_str(chunk);
                    on_chunk(chunk);
                })
                .await?;
            Ok((text, tool_calls))
        } else {
            let (text, tool_calls) = self.provider.chat_with_tools(messages, tools).await?;
            if !text.is_empty() {
                on_chunk(&text);
            }
            Ok((text, tool_calls))
        }
    }

    /// 简单单步对话
    pub async fn run(
        &mut self,
        user_input: &str,
        mut on_chunk: impl FnMut(&str),
    ) -> Result<String, AiError> {
        let messages = self
            .context
            .build_messages(prompt::SYSTEM_PROMPT, user_input);
        let full_response = self.stream_messages(&messages, &mut on_chunk).await?;
        self.context.add_turn(user_input, &full_response);
        Ok(full_response)
    }

    /// 带图片的单步对话（多模态）
    pub async fn run_with_images(
        &mut self,
        user_input: &str,
        images: Vec<ImageContent>,
        mut on_chunk: impl FnMut(&str),
    ) -> Result<String, AiError> {
        let messages =
            self.context
                .build_messages_with_images(prompt::SYSTEM_PROMPT, user_input, images);
        let full_response = self.stream_messages(&messages, &mut on_chunk).await?;
        self.context.add_turn(user_input, &full_response);
        Ok(full_response)
    }

    /// 工具调用最小闭环（MVP）：ReAct 循环。
    ///
    /// - 每轮携带 tools 请求模型，文本逐 chunk 回调；
    /// - 模型返回 tool_calls → 逐个执行 → 结果以 tool 消息回传 → 继续下一轮；
    /// - 无 tool_calls → 结束；轮次达到上限 → 强制结束（不执行该轮工具）；
    /// - 工具执行失败不终止对话：错误文本作为 tool 结果回传，模型可修正重试；
    /// - 工具中间消息只存在于循环内局部 messages，不进持久化历史；
    /// - `mode` 由 `Agent::inference_mode()` 决定；出图模型必须走非流式，
    ///   文本仍在拿到后一次性回调（见 `request_round`）；
    /// - 非流式首轮若因附带 tools 被服务端拒绝（部分出图模型不接受 tools 参数），
    ///   自动摘掉工具降级重试一次，保证"出图"这个主目标不被工具开关拖垮；
    /// - 首轮若服务端明确回绝"该模型不在 Chat Completions 端点服务"（用户漏标
    ///   「出图模型」的典型症状），放弃工具闭环、改走专用出图接口——出图接口本来
    ///   也带不了工具，见 `generate_image_fallback`。
    ///
    /// `mode` 不会是 `ImageGeneration`：那条通路只接受 prompt 字符串、带不了工具，
    /// 因此命令层在 `use_tools` 判定时就已经把它排除（见 `InferencePlan`）。
    pub async fn run_with_tools(
        &mut self,
        request: ToolRunRequest<'_>,
        mut on_chunk: impl FnMut(&str),
    ) -> Result<String, AiError> {
        let ToolRunRequest {
            system_prompt,
            user_input,
            images,
            tools,
            ctx,
            mode,
        } = request;
        let mut messages =
            self.context
                .build_messages_with_images(system_prompt, user_input, images);
        let mut final_text = String::new();

        for round in 0..=MAX_TOOL_ROUNDS {
            let outcome = self
                .request_round(&messages, Some(tools), mode, &mut on_chunk)
                .await;
            // 降级重试只在非流式首轮做：非流式响应是原子的，重试不会产生重复文本；
            // 流式中途失败则可能已输出过内容，重试会造成两份互相矛盾的回答。
            // 还要确认失败**确实与 tools 有关**——模型不存在、端点不支持该模型这类
            // 错误重试一次也一样失败，只会让日志出现两条完全相同的报错。
            let (round_text, tool_calls) = match outcome {
                Ok(pair) => pair,
                // 用户漏标「出图模型」时服务端会明确回绝对话端点；出图接口本来也
                // 带不了工具，所以直接放弃工具闭环、改走出图接口并结束本轮。
                Err(error) if round == 0 && looks_like_chat_endpoint_rejection(&error) => {
                    log::warn!("[Agent] 模型不在对话端点服务（{error}），改走专用出图接口");
                    let text = self.generate_image_fallback(&messages).await?;
                    on_chunk(&text);
                    self.context.add_turn(user_input, &text);
                    return Ok(text);
                }
                Err(error)
                    if round == 0 && !mode.streams() && looks_like_tools_rejection(&error) =>
                {
                    log::warn!(
                        "[Agent] 首轮携带工具的请求失败（{error}），摘掉工具重试一次（该模型可能不支持 tools）"
                    );
                    self.request_round(&messages, None, mode, &mut on_chunk)
                        .await?
                }
                Err(error) => return Err(error),
            };
            final_text.push_str(&round_text);

            if tool_calls.is_empty() {
                self.context.add_turn(user_input, &final_text);
                return Ok(final_text);
            }
            if round == MAX_TOOL_ROUNDS {
                log::warn!("[Agent] 工具调用达到轮次上限 ({MAX_TOOL_ROUNDS})，强制结束");
                self.context.add_turn(user_input, &final_text);
                return Ok(final_text);
            }

            // 追加 assistant 消息（含 tool_calls，序列化见 into_completion_messages）
            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                images: vec![],
                tool_calls: tool_calls.clone(),
                tool_call_id: None,
            });

            // 逐个执行工具（MVP 串行）
            for call in &tool_calls {
                log::info!("[Agent] 执行工具: {} args={}", call.name, call.arguments);
                let output = match execute_tool(&call.name, &call.arguments, ctx).await {
                    Ok(ToolOutput::Text(text)) => truncate_tool_result(&text),
                    Err(error) => format!("工具执行失败: {error}"),
                };
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: output,
                    images: vec![],
                    tool_calls: vec![],
                    tool_call_id: Some(call.id.clone()),
                });
            }
        }

        unreachable!("工具循环必然在轮次上限内返回")
    }

    async fn stream_messages(
        &mut self,
        messages: &[crate::ai_service::context::ChatMessage],
        on_chunk: &mut impl FnMut(&str),
    ) -> Result<String, AiError> {
        // 复制而不是借 `self.provider` 的 &str：下面 `generate_image_fallback(&mut self)`
        // 需要独占借用，`model` 若仍借着自己就会冲突
        let model = self.provider.model_name().to_string();
        // 通路由「模型用途 × provider 能力」唯一决定（见 invocation::resolve）
        let mode = self.inference_mode();

        if !mode.streams() {
            // 图片生成模型（如 gemini-3.1-flash-image）不支持流式输出图片数据：
            // 流式 SSE 只能传输增量文本 delta，图片必须完整生成后才有一有效像素数据，
            // 因此图片仅存在于 stream: false 的非流式响应中。
            // 对图片模型直接跳过流式，减少一次无效 API 调用。
            log::info!(
                "[Agent] 模型 {} 的通路为 {mode:?}，跳过流式，直接使用非流式请求",
                model
            );
            let full_response = if mode.uses_chat_endpoint() {
                match self.provider.chat(messages).await {
                    Ok(text) => text,
                    // 用户漏标「出图模型」时，服务端会在这里明确回绝 → 改走出图接口
                    Err(error) if looks_like_chat_endpoint_rejection(&error) => {
                        log::warn!(
                            "[Agent] 模型 {model} 不在对话端点服务（{error}），改走专用出图接口"
                        );
                        self.generate_image_fallback(messages).await?
                    }
                    Err(error) => return Err(error),
                }
            } else {
                let prompt = last_user_prompt(messages);
                if prompt.trim().is_empty() {
                    return Err(AiError::Other("图片描述不能为空".to_string()));
                }
                match self.provider.generate_image(prompt).await {
                    Ok(text) => text,
                    // 这个网关没有独立出图端点：记住结论并改走对话非流式
                    // （图片会以 Markdown 形式出现在对话响应里）。
                    Err(error) if images_route_missing(&error) => {
                        log::warn!(
                            "[Agent] 模型 {model} 的专用出图端点不存在（{error}），回退到对话非流式；本 Agent 后续轮次不再探测"
                        );
                        self.route_finding = RouteFinding::NoImagesRoute;
                        self.provider.chat(messages).await?
                    }
                    Err(error) => return Err(error),
                }
            };
            if full_response.trim().is_empty() {
                return Err(AiError::Other(
                    "AI 服务返回了空响应，请重试或切换模型".to_string(),
                ));
            }
            on_chunk(&full_response);
            if !full_response.contains("![") && !full_response.contains("data:image") {
                log::warn!(
                    "[Agent] 图片生成模型 ({}) 未返回可展示的图片，保留服务端文本响应",
                    model
                );
            }
            return Ok(full_response);
        }

        let mut full_response = String::new();
        let stream_result = self
            .provider
            .chat_stream(messages, &mut |chunk| {
                full_response.push_str(chunk);
                on_chunk(chunk);
            })
            .await;

        if let Err(error) = stream_result {
            // 已经吐过内容就不能改道——那会给出两份互相矛盾的半截回答。
            // 只在"一个字都没收到 + 服务端明确说该模型不在对话端点服务"时自愈。
            if !full_response.is_empty() || !looks_like_chat_endpoint_rejection(&error) {
                return Err(error);
            }
            log::warn!("[Agent] 模型 {model} 不在对话端点服务（{error}），改走专用出图接口");
            let text = self.generate_image_fallback(messages).await?;
            on_chunk(&text);
            return Ok(text);
        }

        // 部分兼容服务的流式端点会返回空流，但非流式端点工作正常。
        // 仅在完全没有收到内容时重试，避免产生两份互相矛盾的回答。
        if full_response.trim().is_empty() {
            log::info!("[Agent] 流式响应为空，使用非流式请求重试一次");
            full_response = match self.provider.chat(messages).await {
                Ok(text) => text,
                Err(error) if looks_like_chat_endpoint_rejection(&error) => {
                    log::warn!("[Agent] 模型 {model} 不在对话端点服务（{error}），改走专用出图接口");
                    self.generate_image_fallback(messages).await?
                }
                Err(error) => return Err(error),
            };
            if full_response.trim().is_empty() {
                return Err(AiError::Other(
                    "AI 服务返回了空响应，请重试或切换模型".to_string(),
                ));
            }
            on_chunk(&full_response);
        }

        Ok(full_response)
    }

    /// 从存储的 (user, assistant) 对话对重建上下文。
    /// 用于切换对话时恢复 Agent 的对话记忆。
    pub fn rebuild_context_from_pairs(&mut self, pairs: &[(String, String)]) {
        self.context.rebuild_history(pairs);
    }

    /// 重置对话上下文
    pub fn reset(&mut self) {
        self.context.clear_history();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_service::provider::ProviderType;
    use crate::ai_service::providers::OpenAiCompatibleProvider;
    use crate::ai_service::tools::ToolContext;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn identifies_explicit_image_generation_models() {
        assert!(is_image_generation_model("gpt-image-1"));
        assert!(is_image_generation_model("dall-e-3"));
        assert!(is_image_generation_model("gemini-2.5-flash-image-preview"));
        assert!(is_image_generation_model("imagen-4.0-generate"));
        assert!(is_image_generation_model("grok-2-image-1212"));
    }

    #[test]
    fn does_not_treat_regular_chat_models_as_image_models() {
        assert!(!is_image_generation_model("gemini-3.5-flash"));
        assert!(!is_image_generation_model("gpt-4o"));
        assert!(!is_image_generation_model("deepseek-chat"));
    }

    #[test]
    fn model_kind_overrides_name_heuristic() {
        // 显式标记"出图"→ 即使名字不像出图模型也按出图处理
        assert!(ModelKind::Image.is_image_model("gpt-4o"));
        // 显式标记"不是出图"→ 即使名字命中启发式也不按出图处理
        // （例如网关把视觉理解模型命名为 xxx-image-vl）
        assert!(!ModelKind::Chat.is_image_model("gpt-image-1"));
        assert!(!ModelKind::Chat.is_image_model("xxx-image-vl"));
        // Auto → 回落名字启发式
        assert!(ModelKind::Auto.is_image_model("dall-e-3"));
        assert!(!ModelKind::Auto.is_image_model("deepseek-chat"));
    }

    #[test]
    fn model_kind_parses_config_values() {
        assert_eq!(ModelKind::from_config("chat"), ModelKind::Chat);
        assert_eq!(ModelKind::from_config(" image "), ModelKind::Image);
        assert_eq!(ModelKind::from_config(""), ModelKind::Auto);
        assert_eq!(ModelKind::from_config("nonsense"), ModelKind::Auto);
        // 旧版四选一里的 "vision" 已拆分到独立的 vision_mode，这里必须兜底为 Auto，
        // 否则升级后"识图"标记会被误当成出图通路（跳过流式）
        assert_eq!(ModelKind::from_config("vision"), ModelKind::Auto);
    }

    /// 通路结论要能跨重启往返（写进配置再读回来），未知/损坏的值一律回落 Unknown
    #[test]
    fn route_finding_round_trips_through_config() {
        for finding in [
            RouteFinding::Unknown,
            RouteFinding::NoImagesRoute,
            RouteFinding::NoChatRoute,
        ] {
            assert_eq!(RouteFinding::from_config(finding.as_config()), finding);
        }
        assert_eq!(RouteFinding::from_config(""), RouteFinding::Unknown);
        assert_eq!(RouteFinding::from_config("nonsense"), RouteFinding::Unknown);
    }

    /// 从配置恢复的结论必须真的改变通路决策，否则"写回"就白做了
    #[test]
    fn restores_learned_transport_from_config() {
        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            "https://example.com/v1/chat/completions".to_string(),
            "gpt-image-2.5-flare".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);
        assert_eq!(agent.learned_transport(), "");
        assert_eq!(agent.inference_mode(), InferenceMode::ImageGeneration);

        // 模拟重启后从配置恢复"这个网关没有出图端点"
        agent.set_learned_transport("no_images_route");
        assert_eq!(agent.learned_transport(), "no_images_route");
        assert_eq!(agent.inference_mode(), InferenceMode::ChatBlocking);
    }

    #[test]
    fn only_tool_related_errors_trigger_the_tool_less_retry() {
        // 真正的"不支持 tools"：应当降级重试
        assert!(looks_like_tools_rejection(&AiError::Other(
            r#"{"error":{"message":"tools is not supported for this model"}}"#.to_string()
        )));
        assert!(looks_like_tools_rejection(&AiError::Other(
            "tool_choice is not supported".to_string()
        )));
        assert!(looks_like_tools_rejection(&AiError::Other(
            "该模型不支持工具调用".to_string()
        )));
        // 真实案例：出图模型不能走 chat/completions——摘掉 tools 也一样失败，
        // 不能重试（否则每次失败都白发一次请求 + 多一条相同报错）
        assert!(!looks_like_tools_rejection(&AiError::Other(
            r#"HTTP 400: {"error":{"message":"This model is not supported on the Chat Completions endpoint"}}"#
                .to_string()
        )));
        // 模型不存在 / 认证失败同样与 tools 无关
        assert!(!looks_like_tools_rejection(&AiError::InvalidModel(
            "模型 'grok-4.5' 未找到".to_string()
        )));
        assert!(!looks_like_tools_rejection(&AiError::Auth(
            "invalid api key".to_string()
        )));
        assert!(!looks_like_tools_rejection(&AiError::RateLimit(
            "rate limit exceeded".to_string()
        )));
    }

    /// 回退信号必须收窄到「404 **且不是模型级 404**」。
    ///
    /// 放宽到任意 4xx 会把"路由存在、只是这次没成功"当成"没这个端点"；而不区分
    /// 模型级 404 则会**学错**——把"这个模型不在此端点"记成"网关没有此端点"。
    #[test]
    fn only_a_route_level_404_counts_as_a_missing_images_route() {
        // ① 路由级 404（真实：Go 默认处理器）→ 换端点有意义
        assert!(images_route_missing(&AiError::InvalidModel(
            "模型 'x' 未找到 (详情: 404 page not found)".to_string()
        )));
        assert!(images_route_missing(&AiError::InvalidModel(
            "模型 'x' 未找到 (详情: {\"error\":{\"message\":\"Route not found\"}})".to_string()
        )));

        // ② 模型级 404（真实：Google 兼容层）→ 端点存在，只是不服务这个模型。
        //    回退救不了，而且会把它错记成"网关没有出图端点"
        assert!(!images_route_missing(&AiError::InvalidModel(
            "模型 'gemini-3.1-flash-image' 未找到 (详情: {\"error\":{\"code\":404,\"message\":\"models/gemini-3.1-flash-image is not found for API version v1main, or is not supported for predict. Call ModelService.ListModels to see the list of available models and their supported methods.\",\"status\":\"NOT_FOUND\"}})".to_string()
        )));
        assert!(!images_route_missing(&AiError::InvalidModel(
            "模型 'x' 未找到 (详情: {\"error\":{\"message\":\"Model not found\",\"type\":\"model_not_found\"}})".to_string()
        )));

        // ③ "路由存在、只是这次没成功" → 一律不回退，原样上报
        assert!(!images_route_missing(&AiError::Other(
            r#"HTTP 400: {"error":{"message":"prompt is required"}}"#.to_string()
        )));
        assert!(!images_route_missing(&AiError::Other(
            r#"HTTP 400: {"error":{"message":"images endpoint requires an image model"}}"#
                .to_string()
        )));
        assert!(!images_route_missing(&AiError::Auth("401".to_string())));
        assert!(!images_route_missing(&AiError::RateLimit("429".to_string())));
        assert!(!images_route_missing(&AiError::ServerError(
            "502 Upstream service temporarily unavailable".to_string()
        )));
        assert!(!images_route_missing(&AiError::Network(
            "请求超时".to_string()
        )));
    }

    /// 反向自愈的信号同样要窄：只有服务端**明确说该模型不在对话端点服务**才改道。
    /// 放宽到任意 400 会把"参数写错"也当成"模型不对端点"，白白多打一次出图请求。
    #[test]
    fn only_the_chat_endpoint_rejection_triggers_the_image_fallback() {
        // 真实日志逐字
        assert!(looks_like_chat_endpoint_rejection(&AiError::Other(
            r#"HTTP 400: {"error":{"message":"This model is not supported on the Chat Completions endpoint","type":"invalid_request_error"}}"#
                .to_string()
        )));

        // 其余变体连变体本身都不匹配（只有 Other 才可能命中）
        assert!(!looks_like_chat_endpoint_rejection(&AiError::Auth(
            "invalid api key".to_string()
        )));
        assert!(!looks_like_chat_endpoint_rejection(&AiError::RateLimit(
            "429".to_string()
        )));
        assert!(!looks_like_chat_endpoint_rejection(&AiError::ServerError(
            "502".to_string()
        )));
        assert!(!looks_like_chat_endpoint_rejection(&AiError::InvalidModel(
            "模型 'x' 未找到".to_string()
        )));
        assert!(!looks_like_chat_endpoint_rejection(&AiError::Network(
            "请求超时".to_string()
        )));
        assert!(!looks_like_chat_endpoint_rejection(&AiError::Stream(
            "SSE 解析失败".to_string()
        )));

        // Other 里但措辞不符：不触发
        assert!(!looks_like_chat_endpoint_rejection(&AiError::Other(
            r#"HTTP 400: {"error":{"message":"prompt is required"}}"#.to_string()
        )));
    }

    // ── 工具调用闭环测试（本地 mock OpenAI SSE 服务，不依赖外部网络）──

    /// 构造一次工具闭环请求（测试默认不带图片；`mode` 由用例指定）
    fn tool_request<'a>(
        user_input: &'a str,
        tools: &'a [ToolSpec],
        ctx: &'a ToolContext,
        mode: InferenceMode,
    ) -> ToolRunRequest<'a> {
        ToolRunRequest {
            system_prompt: prompt::SYSTEM_PROMPT,
            user_input,
            images: vec![],
            tools,
            ctx,
            mode,
        }
    }

    /// 构造测试用 ToolContext：临时数据库 + 无调度回调（工具不依赖宿主）
    fn test_ctx() -> ToolContext {
        let dir = std::env::temp_dir().join(format!(
            "desktop-aide-agent-tool-test-{}",
            uuid::Uuid::new_v4()
        ));
        let db = crate::db::Database::open(&dir).expect("打开测试数据库失败");
        ToolContext {
            database: std::sync::Arc::new(tokio::sync::Mutex::new(db)),
            schedule_reminder: None,
            web_search: None,
        }
    }

    /// 简易 OpenAI 兼容 mock：每个连接按请求计数返回脚本化 SSE 响应体。
    /// 返回 (endpoint_base, server_handle)；测试结束 runtime 销毁时服务自动停止。
    async fn mock_openai_server(responses: Vec<String>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let mut counter = 0usize;
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                // 读请求直到头部结束（body 不需要，直接丢弃）
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    match socket.read(&mut tmp).await {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&tmp[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let idx = counter;
                counter += 1;
                let body = responses.get(idx).cloned().unwrap_or_default();
                let resp =
                    format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n{body}");
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        (format!("http://{addr}"), handle)
    }

    /// 闭环：第一轮模型返回 get_current_time 的 tool_call（arguments 分片），
    /// 第二轮返回最终文本。验证：工具被调用一次、结果回传、最终回答返回。
    #[tokio::test]
    async fn closed_loop_invokes_tool_and_returns_final_answer() {
        let responses = vec![
            // 第一轮：tool_call 分片 + 结束标记
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_current_time\",\"arguments\":\"\"}}]}}]}\n\n".to_string()
                + "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{}\"}}]}}]}\n\n"
                + "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n"
                + "data: [DONE]\n\n",
            // 第二轮：最终回答
            "data: {\"choices\":[{\"delta\":{\"content\":\"现在是 2026-02-13 14:30:00 星期五。\"}}]}\n\n".to_string()
                + "data: [DONE]\n\n",
        ];
        let (endpoint, _server) = mock_openai_server(responses).await;

        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            format!("{endpoint}/v1/chat/completions"),
            "test-model".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);
        let tools = crate::ai_service::tools::builtin_tools();
        let ctx = test_ctx();

        let mut streamed = String::new();
        let result = agent
            .run_with_tools(
                tool_request("现在几点了？", &tools, &ctx, InferenceMode::ChatStream),
                |chunk| streamed.push_str(chunk),
            )
            .await
            .expect("工具闭环应成功");

        assert!(result.contains("现在是"), "应返回模型最终回答: {result}");
        assert!(streamed.contains("现在是"), "最终文本应流式输出");
    }

    /// 工具执行失败不终止对话：mock 返回不存在的工具名，
    /// Agent 应把错误作为 tool 结果回传，并继续第二轮拿到最终回答。
    #[tokio::test]
    async fn tool_execution_error_does_not_abort_loop() {
        let responses = vec![
            // 第一轮：调用不存在的工具
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_x\",\"function\":{\"name\":\"no_such_tool\",\"arguments\":\"{}\"}}]}}]}\n\n".to_string()
                + "data: [DONE]\n\n",
            // 第二轮：模型根据错误结果给出回答
            "data: {\"choices\":[{\"delta\":{\"content\":\"抱歉，该操作不可用。\"}}]}\n\n".to_string()
                + "data: [DONE]\n\n",
        ];
        let (endpoint, _server) = mock_openai_server(responses).await;

        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            format!("{endpoint}/v1/chat/completions"),
            "test-model".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);
        let tools = crate::ai_service::tools::builtin_tools();
        let ctx = test_ctx();

        let result = agent
            .run_with_tools(
                tool_request("帮我做点什么", &tools, &ctx, InferenceMode::ChatStream),
                |_| {},
            )
            .await
            .expect("工具失败不应中断循环");
        assert!(result.contains("抱歉"), "应拿到第二轮回答: {result}");
    }

    // ── 非流式工具闭环（出图模型路径）──

    /// 非流式工具闭环：每轮返回 JSON（而非 SSE），第一轮 tool_call、第二轮最终文本。
    /// 出图模型拿不到流式增量，但仍应能"先调工具、再出结果"。
    #[tokio::test]
    async fn non_streaming_tool_loop_invokes_tool_and_returns_final_answer() {
        let responses = vec![
            r#"{"choices":[{"message":{"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_current_time","arguments":"{}"}}]}}]}"#.to_string(),
            r#"{"choices":[{"message":{"role":"assistant","content":"现在是 2026-02-13 14:30:00 星期五。"}}]}"#.to_string(),
        ];
        let (endpoint, _server) = mock_openai_server(responses).await;

        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            format!("{endpoint}/v1/chat/completions"),
            "gemini-2.5-flash-image".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);
        let tools = crate::ai_service::tools::builtin_tools();
        let ctx = test_ctx();

        let mut emitted = String::new();
        // 出图模型的通路是 ChatBlocking → 每轮非流式
        let result = agent
            .run_with_tools(
                tool_request("现在几点了？", &tools, &ctx, InferenceMode::ChatBlocking),
                |chunk| emitted.push_str(chunk),
            )
            .await
            .expect("非流式工具闭环应成功");

        assert!(result.contains("现在是"), "应返回模型最终回答: {result}");
        assert!(emitted.contains("现在是"), "非流式文本也应回调给前端");
    }

    /// 可指定 HTTP 状态码的 mock：读取并记录每次请求的 body）
    /// 用于验证"携带 tools 被拒绝 → 摘掉工具重试"的降级行为。
    async fn mock_openai_server_with_status(
        statuses: Vec<u16>,
        responses: Vec<String>,
        seen_bodies: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let mut counter = 0usize;
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let header_end;
                loop {
                    match socket.read(&mut tmp).await {
                        Ok(0) => {
                            header_end = buf.len();
                            break;
                        }
                        Ok(n) => {
                            buf.extend_from_slice(&tmp[..n]);
                            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                header_end = pos + 4;
                                break;
                            }
                        }
                        Err(_) => {
                            header_end = buf.len();
                            break;
                        }
                    }
                }
                // 按 Content-Length 读完 body（用于检查这次请求是否携带 tools）
                let headers = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
                let content_length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0);
                while buf.len() < header_end + content_length {
                    match socket.read(&mut tmp).await {
                        Ok(0) => break,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                        Err(_) => break,
                    }
                }
                seen_bodies
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[header_end..]).to_string());

                let idx = counter;
                counter += 1;
                let status = statuses.get(idx).copied().unwrap_or(200);
                let payload = responses.get(idx).cloned().unwrap_or_default();
                let reason = if status == 200 { "OK" } else { "Bad Request" };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        (format!("http://{addr}"), handle)
    }

    /// 安全网：出图模型不接受 `tools` 参数时，首轮请求会失败；
    /// Agent 应摘掉工具重试一次，保证"出图"这个主目标不被工具开关拖垮。
    #[tokio::test]
    async fn falls_back_to_tool_less_request_when_tools_are_rejected() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (endpoint, _server) = mock_openai_server_with_status(
            vec![400, 200],
            vec![
                r#"{"error":{"message":"tools is not supported for this model"}}"#.to_string(),
                r#"{"choices":[{"message":{"role":"assistant","content":"![生成的图片](data:image/png;base64,AAA)"}}]}"#.to_string(),
            ],
            seen.clone(),
        )
        .await;

        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            format!("{endpoint}/v1/chat/completions"),
            "dall-e-3".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);
        let tools = crate::ai_service::tools::builtin_tools();
        let ctx = test_ctx();

        let result = agent
            .run_with_tools(
                tool_request("画一只猫", &tools, &ctx, InferenceMode::ChatBlocking),
                |_| {},
            )
            .await
            .expect("降级重试后应成功返回图片");

        assert!(
            result.contains("data:image/png"),
            "应拿到图片结果: {result}"
        );
        let bodies = seen.lock().unwrap();
        assert_eq!(bodies.len(), 2, "应发生一次降级重试: {bodies:?}");
        assert!(bodies[0].contains("\"tools\""), "首次请求应携带 tools");
        assert!(!bodies[1].contains("\"tools\""), "重试请求不应携带 tools");
    }

    /// 回归（真实故障现场）：**custom 网关 + 出图模型**。原先"有没有出图端点"
    /// 只看 `provider_type == Xai`，于是这类配置一律走 `/chat/completions`，
    /// 网关回 `This model is not supported on the Chat Completions endpoint`，
    /// 出图功能完全不可用且原因难查。
    ///
    /// 现在的行为：先按专用出图接口探测（本用例网关回 404），随后回退到对话
    /// 非流式拿到图片，并把"没有该端点"记住，后续轮次不再重复探测。
    #[tokio::test]
    async fn probes_images_endpoint_then_falls_back_and_remembers() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (endpoint, _server) = mock_openai_server_with_status(
            vec![404, 200],
            vec![
                "404 page not found".to_string(),
                r#"{"choices":[{"message":{"role":"assistant","content":"![生成的图片](data:image/png;base64,AAA)"}}]}"#.to_string(),
            ],
            seen.clone(),
        )
        .await;

        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            format!("{endpoint}/v1/chat/completions"),
            "gpt-image-2.5-flare".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);

        // 探测前：名字命中启发式 + OpenAI 兼容协议 → 先按专用出图接口试
        assert_eq!(
            agent.inference_mode(),
            InferenceMode::ImageGeneration,
            "custom 网关的出图模型应先按专用出图接口探测"
        );

        let result = agent
            .run("画一只猫", |_| {})
            .await
            .expect("应回退到对话端点并成功返回图片");

        assert!(result.contains("data:image/png"), "应拿到图片结果: {result}");

        {
            let bodies = seen.lock().unwrap();
            assert_eq!(bodies.len(), 2, "应先在出图端点探测失败、再回退: {bodies:?}");
            assert!(
                bodies[0].contains("\"prompt\""),
                "首个请求应是出图请求: {}",
                bodies[0]
            );
            assert!(
                bodies[1].contains("\"messages\""),
                "回退请求应走对话端点: {}",
                bodies[1]
            );
        }

        // 结论已记住 → 不再重复探测
        assert_eq!(
            agent.inference_mode(),
            InferenceMode::ChatBlocking,
            "探明没有出图端点后应直接走对话非流式"
        );
    }

    /// 反向自愈（用户漏标「出图模型」）：模型被当成对话模型走流式，服务端明确回绝
    /// `This model is not supported on the Chat Completions endpoint`。
    /// Agent 应改走专用出图接口并记住结论，而不是把这个看不懂的报错丢给用户。
    #[tokio::test]
    async fn heals_when_the_model_is_not_served_on_the_chat_endpoint() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (endpoint, _server) = mock_openai_server_with_status(
            vec![400, 200],
            vec![
                r#"{"error":{"message":"This model is not supported on the Chat Completions endpoint","type":"invalid_request_error"}}"#.to_string(),
                r#"{"data":[{"b64_json":"AAA"}]}"#.to_string(),
            ],
            seen.clone(),
        )
        .await;

        let provider = AiProvider::OpenAiCompatible(OpenAiCompatibleProvider::new(
            format!("{endpoint}/v1/chat/completions"),
            "gpt-image-2.5-flare".to_string(),
            "key".to_string(),
            0.7,
            1024,
            ProviderType::Custom,
        ));
        let mut agent = Agent::new(provider);
        // 漏标：模型其实是出图模型，却被标记成"不是出图模型"
        agent.set_model_kind(ModelKind::Chat);

        assert_eq!(
            agent.inference_mode(),
            InferenceMode::ChatStream,
            "漏标时先按普通对话走流式"
        );

        let result = agent
            .run("画一只猫", |_| {})
            .await
            .expect("应改走出图接口并成功返回图片");

        assert!(result.contains("data:image/png"), "应拿到图片: {result}");

        {
            let bodies = seen.lock().unwrap();
            assert_eq!(bodies.len(), 2, "应先被对话端点回绝、再改走出图接口: {bodies:?}");
            assert!(
                bodies[0].contains("\"messages\""),
                "首个请求应走对话端点: {}",
                bodies[0]
            );
            assert!(
                bodies[1].contains("\"prompt\""),
                "改道请求应走出图接口: {}",
                bodies[1]
            );
        }

        assert_eq!(
            agent.inference_mode(),
            InferenceMode::ImageGeneration,
            "结论应被记住，后续直接走出图接口"
        );
    }
}
