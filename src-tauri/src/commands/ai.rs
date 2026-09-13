use crate::ai_service::agent::Agent;
use crate::ai_service::context::ImageContent;
use crate::ai_service::prompt;
use crate::ai_service::provider;
use crate::ai_service::tools;
use crate::ai_service::vision;
use crate::config::manager::AppConfig;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::Emitter;
use tauri::State;

#[derive(Clone, Serialize)]
struct AiStreamChunkEvent {
    request_id: String,
    chunk: String,
}

/// `agent_busy` 的 RAII 守卫。
///
/// 背景：`ai_chat` 成功 CAS 后，仅在 `select!` 的两个分支里把 busy 复位。
/// 一旦请求过程中发生 panic（例如日志预览按字节切多字节字符），展开会跳过复位，
/// 此后所有对话都被拒绝，只能重启应用。持有本守卫后，正常返回、提前 `?` 返回、
/// panic 展开三种路径都会复位。
struct BusyGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone, Serialize)]
struct AiStreamDoneEvent {
    request_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigDto {
    pub ai_provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub ollama_endpoint: String,
    #[serde(default)]
    pub ollama_model: String,
    pub bg_color: String,
    pub bg_opacity: f64,
    pub accent_color: String,
    pub msg_user_bg: String,
    pub msg_user_border: String,
    pub panel_width: u32,
    pub panel_height: u32,
    pub theme_mode: String,
    pub theme_preset: String,
    pub bubble_auto_collapse: bool,
    pub bubble_collapse_delay: u32,
    pub enable_tools: bool,
    /// 允许 AI 联网搜索（AI 按需自动调用 web_search 工具）
    #[serde(default)]
    pub enable_web_search: bool,
    /// Tavily 搜索 API Key（可选；为空时自动退回 DuckDuckGo 免 key 搜索）
    #[serde(default)]
    pub tavily_api_key: String,
    /// 图片输入方式：`off`（默认，走 OCR）/ `auto`（按注册表判断）/ `on`（始终按多模态发送）
    #[serde(default = "default_vision_mode_dto")]
    pub vision_mode: String,
    /// 模型用途：`auto`（默认）/ `chat` / `vision`（识图）/ `image`（出图）
    #[serde(default = "default_model_kind_dto")]
    pub model_kind: String,
}

fn default_vision_mode_dto() -> String {
    "off".to_string()
}

fn default_model_kind_dto() -> String {
    "auto".to_string()
}

impl From<&AppConfig> for ConfigDto {
    fn from(c: &AppConfig) -> Self {
        Self {
            ai_provider: c.ai_provider.clone(),
            model: c.model.clone(),
            api_key: c.api_key.clone(),
            api_base: c.api_base.clone(),
            max_tokens: c.max_tokens,
            temperature: c.temperature,
            ollama_endpoint: c.ollama_endpoint.clone(),
            ollama_model: c.ollama_model.clone(),
            bg_color: c.bg_color.clone(),
            bg_opacity: c.bg_opacity,
            accent_color: c.accent_color.clone(),
            msg_user_bg: c.msg_user_bg.clone(),
            msg_user_border: c.msg_user_border.clone(),
            panel_width: c.panel_width,
            panel_height: c.panel_height,
            theme_mode: c.theme_mode.clone(),
            theme_preset: c.theme_preset.clone(),
            bubble_auto_collapse: c.bubble_auto_collapse,
            bubble_collapse_delay: c.bubble_collapse_delay,
            enable_tools: c.enable_tools,
            enable_web_search: c.enable_web_search,
            tavily_api_key: c.tavily_api_key.clone(),
            vision_mode: c.vision_mode.clone(),
            model_kind: c.model_kind.clone(),
        }
    }
}

impl From<ConfigDto> for AppConfig {
    fn from(dto: ConfigDto) -> Self {
        Self {
            ai_provider: dto.ai_provider,
            model: dto.model,
            api_key: dto.api_key,
            api_base: dto.api_base,
            max_tokens: dto.max_tokens,
            temperature: dto.temperature,
            ollama_endpoint: dto.ollama_endpoint,
            ollama_model: dto.ollama_model,
            bg_color: dto.bg_color,
            bg_opacity: dto.bg_opacity,
            accent_color: dto.accent_color,
            msg_user_bg: dto.msg_user_bg,
            msg_user_border: dto.msg_user_border,
            panel_width: dto.panel_width,
            panel_height: dto.panel_height,
            theme_mode: dto.theme_mode,
            theme_preset: dto.theme_preset,
            bubble_auto_collapse: dto.bubble_auto_collapse,
            bubble_collapse_delay: dto.bubble_collapse_delay,
            enable_tools: dto.enable_tools,
            enable_web_search: dto.enable_web_search,
            tavily_api_key: dto.tavily_api_key,
            vision_mode: dto.vision_mode,
            model_kind: dto.model_kind,
            // 后端自有字段，不由前端传入；`save_config` 会从已保存的配置里补回来
            learned_transport: String::new(),
        }
    }
}

/// 获取当前配置
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<ConfigDto, String> {
    let mgr = state.config_manager.lock().await;
    Ok(ConfigDto::from(mgr.get()))
}

/// 判断本次保存是否需要重建 Agent。
///
/// 只有 AI 相关字段变化时才重建——重建会把 Agent 的对话上下文清空后从数据库恢复，
/// 因此纯外观/气泡设置的保存不应该走这条路。
///
/// 独立成纯函数是为了能直接单测（`save_config` 本体依赖 Tauri `State`，无法在单测里构造）。
fn ai_config_changed(old: &AppConfig, new: &AppConfig) -> bool {
    old.ai_provider != new.ai_provider
        || old.model != new.model
        || old.api_key != new.api_key
        || old.api_base != new.api_base
        || old.max_tokens != new.max_tokens
        || old.temperature != new.temperature
        || old.ollama_endpoint != new.ollama_endpoint
        // 模型用途存在 Agent 内部（决定是否走出图通路），变更时需重建
        || old.model_kind != new.model_kind
}

/// 把 Agent 这次探明的通路结论写回配置，跨重启保留。
///
/// 结论是花了一次请求换来的，不写回就等于每次启动重来一遍。写失败只记日志，
/// 不影响本次对话。
async fn persist_learned_transport(state: &AppState, agent: &Agent) {
    let learned = agent.learned_transport();
    let mut mgr = state.config_manager.lock().await;
    if mgr.get().learned_transport == learned {
        return;
    }
    let mut config = mgr.get().clone();
    config.learned_transport = learned.to_string();
    log::info!("[ai_chat] 记录通路探测结论: {learned:?}");
    if let Err(error) = mgr.update(config) {
        log::warn!("[ai_chat] 保存通路探测结论失败: {error}");
    }
}

/// 「测试连接」所需的最小连接信息。
///
/// 刻意不复用 `ConfigDto`：探测只关心地址 / Key / 模型，独立成小结构体可以避免
/// 因为无关字段（配色、面板尺寸）反序列化失败，导致按钮"点了没反应"。
#[derive(Debug, Deserialize)]
pub struct ConnectionDto {
    pub ai_provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_base: String,
    #[serde(default)]
    pub ollama_endpoint: String,
}

impl ConnectionDto {
    /// 只保留连接相关字段，其余走 `Default`（探测不读它们）
    fn into_config(self) -> AppConfig {
        AppConfig {
            ai_provider: self.ai_provider,
            model: self.model,
            api_key: self.api_key,
            api_base: self.api_base,
            ollama_endpoint: self.ollama_endpoint,
            ..AppConfig::default()
        }
    }
}

/// 拉取该连接下可用的模型清单（供设置界面的模型下拉）。
///
/// 返回 `models` + `reason`：**失败原因必须带到前端**。原先统一吞成空数组，
/// 用户点了按钮没有任何反馈，只会以为按钮坏了——而"这个服务商不实现 /models"
/// 恰恰是最常见的情况。
#[tauri::command]
pub async fn list_ai_models(
    connection: ConnectionDto,
) -> Result<crate::ai_service::probe::ModelListResult, String> {
    Ok(crate::ai_service::probe::list_models(&connection.into_config()).await)
}

/// 一次性探测该连接是否可用（Key / 模型清单 / 对话端点 / 出图路由四项）。
///
/// 不产生出图费用，详见 `ai_service::probe`。
#[tauri::command]
pub async fn probe_ai_config(
    connection: ConnectionDto,
) -> Result<crate::ai_service::probe::ProbeReport, String> {
    Ok(crate::ai_service::probe::probe(&connection.into_config()).await)
}

/// 保存配置并重新初始化 AI 适配器
#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config_dto: ConfigDto) -> Result<(), String> {
    let mut new_config: AppConfig = config_dto.into();

    if !provider::is_supported_provider(&new_config.ai_provider) {
        return Err("不支持的 AI 服务商".to_string());
    }
    if new_config.model.trim().is_empty() {
        return Err("请填写模型名称".to_string());
    }
    if new_config.ai_provider == "custom" && new_config.api_base.trim().is_empty() {
        return Err("自定义兼容服务必须填写 API 地址".to_string());
    }

    // 保护好**后端自有**的字段：它们不由设置表单维护，不能随 save_config 一起被覆盖。
    // 顺带在这里算出是否需要重建 Agent——因为"换了配置"同时意味着旧的通路探测结论失效。
    let ai_config_changed = {
        let mgr = state.config_manager.lock().await;
        let saved = mgr.get();
        // 面板尺寸由前端 resize 事件独立写入
        new_config.panel_width = saved.panel_width;
        new_config.panel_height = saved.panel_height;
        // 通路探测结论完全由后端维护（见 ai_chat 里的回写）
        new_config.learned_transport = saved.learned_transport.clone();
        ai_config_changed(saved, &new_config)
    };
    if ai_config_changed {
        // 换了服务商/模型/Key，上次探明的通路结论对新配置不再适用
        new_config.learned_transport.clear();
    }
    new_config.bubble_collapse_delay = new_config.bubble_collapse_delay.clamp(3, 86_400);

    // 检查 AI 是否正在响应
    if state.agent_busy.load(Ordering::Acquire) {
        return Err("AI 正在响应中，请稍后再保存配置".to_string());
    }

    // 更新配置并保存到文件。`update` 返回**被替换的旧配置**，用于下面判断
    // 是否需要重建 Agent——必须在写入之前拿到旧值：写入之后再读 `mgr.get()`
    // 只能得到新配置，比对恒为相等，Agent 永不重建（改了模型却仍用旧模型）。
    let old_config = {
        let mut mgr = state.config_manager.lock().await;
        mgr.update(new_config.clone())?
    };

    // 仅当 AI 相关配置变化时才重建 Agent：
    // 外观/气泡等非 AI 配置的保存不应清空当前对话上下文。
    if ai_config_changed {
        log::info!(
            "[save_config] AI 配置变化（模型 {} → {}），重建 Agent 并恢复会话上下文",
            old_config.model,
            new_config.model
        );

        // 重新创建 Agent（使用写锁确保独占）
        let adapter = provider::from_config(&new_config);
        let mut agent = Agent::new(adapter);
        agent.set_model_kind(crate::ai_service::agent::ModelKind::from_config(
            &new_config.model_kind,
        ));
        // 上面按需清空过 learned_transport，这里恢复（清空后即 Unknown）
        agent.set_learned_transport(&new_config.learned_transport);
        {
            let mut agent_lock = state.agent.write().await;
            *agent_lock = agent;
        }

        // 从数据库恢复当前会话的记忆（与切换对话的行为一致：
        // 附件路径先还原为 base64，再配对重建上下文）
        let conv_id = state.current_conversation_id.lock().await.clone();
        let pairs = {
            let db = state.database.lock().await;
            let raw = db.load_messages(&conv_id)?;
            let store = state.image_store.lock().await;
            let messages: Vec<crate::db::MessageDto> = raw
                .into_iter()
                .map(|m| crate::db::MessageDto {
                    attachments: m.attachments.map(|a| {
                        crate::commands::history::reconstruct_attachment_images(&a, &store)
                    }),
                    ..m
                })
                .collect();
            crate::commands::history::pair_user_assistant(&messages)
        };
        if !pairs.is_empty() {
            let mut agent_lock = state.agent.write().await;
            agent_lock.rebuild_context_from_pairs(&pairs);
        }
    } else {
        log::info!("[save_config] AI 配置未变，保留当前 Agent 上下文");
    }

    Ok(())
}

/// 提醒意图粗检：命中「提醒/叫我/到点」且含时间线索时，判定用户正在请求定时提醒。
///
/// 用于注入当轮强指令（REMINDER_TURN_INSTRUCTION）。误命中只会多一条无害指令，
/// 漏命中退回默认行为；宁可多触发，不可漏掉真正请求提醒的用户。
/// 「取消/删除」类请求排除在外，避免与「必须调用 create_reminder」指令冲突。
fn has_reminder_intent(text: &str) -> bool {
    const CANCEL_WORDS: [&str; 4] = ["取消", "删除", "删掉", "撤销"];
    if CANCEL_WORDS.iter().any(|k| text.contains(k)) {
        return false;
    }
    const ASK_WORDS: [&str; 3] = ["提醒", "叫我", "到点"];
    const TIME_CUES: [&str; 12] = [
        "分钟", "小时", "秒", "点钟", "上午", "下午", "中午", "晚上", "早上", "明天", "今天", "后",
    ];
    ASK_WORDS.iter().any(|k| text.contains(k)) && TIME_CUES.iter().any(|k| text.contains(k))
}

/// 时效性意图粗检：命中实时/最新类关键词时，判定本轮可能需要联网搜索。
///
/// 与 `has_reminder_intent` 同样的取舍——误命中只多一条"可能需检索"的当轮指令
/// （纯闲聊/常识时模型可自行判断直接回答），漏命中则退回默认行为。
fn has_fresh_info_intent(text: &str) -> bool {
    const FRESH_WORDS: [&str; 26] = [
        "最新", "今天", "今日", "现在", "目前", "当前", "实时", "新闻", "热点", "近期", "本周",
        "这周", "本月", "今年", "刚刚", "股价", "汇率", "天气", "比分", "赛事", "行情", "进展",
        "动态", "政策", "热搜", "发布",
    ];
    FRESH_WORDS.iter().any(|k| text.contains(k))
}

/// 前端传递的图片附件
#[derive(Debug, Deserialize)]
pub struct ImageAttachmentDto {
    pub data: String,
    pub mime_type: String,
    /// 用于 OCR 回退时把识别结果标注到对应图片（`[图片 "name" 的文字识别结果]`）
    pub name: String,
}

/// 对 base64 图片运行 OCR，返回识别文字
fn run_ocr_on_base64(base64_data: &str, _media_type: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("base64 解码失败: {e}"))?;

    // 写入临时文件供 OCR 引擎使用
    let temp_dir = std::env::temp_dir().join("desktop-aide-ocr");
    let _ = std::fs::create_dir_all(&temp_dir);
    let temp_path = temp_dir.join(format!("paste-ocr-{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, &bytes).map_err(|e| format!("写临时文件失败: {e}"))?;

    // 尝试 WinRtOcr
    let result = crate::ocr::try_ocr_on_file(&temp_path);

    // 清理临时文件
    let _ = std::fs::remove_file(&temp_path);

    result
        .map(|r| r.full_text)
        .ok_or_else(|| "所有 OCR 引擎均不可用".to_string())
}

/// AI 对话（流式，通过 event 推送每个 chunk）
#[tauri::command]
pub async fn ai_chat(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    message: String,
    images: Option<Vec<ImageAttachmentDto>>,
) -> Result<(), String> {
    if state
        .agent_busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("AI 正在响应中，请稍后再试".to_string());
    }
    // CAS 成功后立刻持有守卫：即使后续 panic，也会在展开时复位 busy 标志
    let _busy_guard = BusyGuard(&state.agent_busy);

    let mut agent = state.agent.write().await;

    // 确定 provider 类型与图片输入方式。
    // 「模型用途」不再在这里解析——通路决策统一由 `Agent::inference_mode()` 负责。
    let (provider_type, vision_mode) = {
        let mgr = state.config_manager.lock().await;
        let cfg = mgr.get();
        (
            provider::resolve_provider_type(&cfg.ai_provider),
            vision::VisionMode::from_config(&cfg.vision_mode),
        )
    };
    let model = {
        let mgr = state.config_manager.lock().await;
        mgr.get().model.clone()
    };
    // 是否把图片按多模态发送：只由「图片输入方式」决定（识图维度）。
    // 「模型用途」只描述"能不能出图"，与识图互不排斥（可同时开启）。
    let send_images_as_multimodal = vision::use_multimodal(vision_mode, &model, &provider_type);

    // 处理图片附件
    let mut final_message = message.clone();
    let mut image_contents: Vec<ImageContent> = Vec::new();

    if let Some(imgs) = &images {
        if !imgs.is_empty() {
            log::info!(
                "[vision] 模型 {model} / 图片输入方式 {vision_mode:?} → {}",
                if send_images_as_multimodal {
                    "按多模态发送图片"
                } else {
                    "OCR 文字识别回退"
                }
            );
            if send_images_as_multimodal {
                // 多模态模型 → 直接发送图片
                for img in imgs {
                    image_contents.push(ImageContent {
                        data: img.data.clone(),
                        media_type: img.mime_type.clone(),
                    });
                }
            } else {
                // 非多模态模型 → OCR 回退
                for img in imgs {
                    match run_ocr_on_base64(&img.data, &img.mime_type) {
                        Ok(ocr_text) => {
                            final_message.push_str(&format!(
                                "\n\n[图片 \"{}\" 的文字识别结果]:\n{}",
                                img.name, ocr_text
                            ));
                        }
                        Err(e) => {
                            log::warn!("OCR 回退失败: {e}");
                        }
                    }
                }
            }
        }
    }

    // 创建取消通道（供"停止生成"中断本次请求）
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    *state.ai_cancel.lock().await = Some(cancel_tx);

    let app_handle = app.clone();
    let chunk_request_id = request_id.clone();
    // 工具执行上下文所需的共享数据库（Arc 克隆，供 async 块使用）
    let tool_db = state.database.clone();

    // 工具/联网开关：只有**专用出图端点**（`/images/generations`，只接受 prompt
    // 字符串、不接受 messages/tools）才必须禁用工具闭环。普通出图模型（走
    // chat/completions 返回图片 Markdown）照样能用工具与联网搜索，只是每轮改走
    // 非流式（见下方传给 run_with_tools 的 `stream` 参数）——例如"先联网搜再出图"。
    let (enable_tools, enable_web_search, tavily_api_key) = {
        let mgr = state.config_manager.lock().await;
        let cfg = mgr.get();
        (
            cfg.enable_tools,
            cfg.enable_web_search,
            cfg.tavily_api_key.clone(),
        )
    };
    // 通路决策只做一次（`Agent::inference_mode()`），工具是否可用由通路能力派生，
    // 不再各处用 provider 类型 / 模型名重新判断——那正是「日志说走 A、实际发到 B」的成因。
    let plan = crate::ai_service::invocation::InferencePlan::new(
        agent.inference_mode(),
        enable_tools,
        enable_web_search,
    );
    let use_tools = plan.use_tools;
    if plan.mode != crate::ai_service::invocation::InferenceMode::ChatStream {
        log::info!(
            "[invocation] 模型 {model} → 通路 {:?} | 工具闭环: {use_tools} | 流式: {}",
            plan.mode,
            plan.mode.streams()
        );
    }

    // 系统 Prompt 按需叠加当轮指令：
    // - 提醒意图：开启工具调用且命中提醒意图时追加强指令，压制「口头承诺」坏模式
    //   （真实 API 验证见 prompt.rs 注释）；
    // - 联网注记：开启联网搜索时告知模型可调用 web_search 并按需引用来源。
    let mut system_prompt = prompt::SYSTEM_PROMPT.to_string();
    if enable_tools && use_tools && has_reminder_intent(&final_message) {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(prompt::REMINDER_TURN_INSTRUCTION);
    }
    if enable_web_search && use_tools {
        // 动态生成：含"当前日期 + 已具备联网能力"的声明，解决模型不知道自己
        // 能联网、也没有日期参照判断信息时效的问题
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&prompt::web_search_note());
        // 时效性问题再叠加当轮强指令，压制"凭记忆回答"的惯性
        if has_fresh_info_intent(&final_message) {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(prompt::WEB_SEARCH_TURN_INSTRUCTION);
        }
    }

    let chat_future = async {
        if use_tools {
            // 按开关组合当轮工具列表（时间/提醒由「工具调用」控制，联网搜索单独控制）
            let tool_list = tools::tools_for_request(enable_tools, enable_web_search);
            // 提醒工具的调度回调：创建提醒后调度应用内提醒（需宿主 AppHandle；
            // 用克隆避免与下方 on_chunk 闭包对 app_handle 的占用冲突）
            let schedule_reminder: Option<Box<dyn Fn(crate::db::Reminder) + Send + Sync>> =
                Some(Box::new({
                    let app = app_handle.clone();
                    move |reminder: crate::db::Reminder| {
                        crate::commands::reminder::schedule_reminder(app.clone(), reminder);
                    }
                }));
            let tool_ctx = tools::ToolContext {
                database: tool_db,
                schedule_reminder,
                web_search: enable_web_search.then(|| {
                    crate::ai_service::web_search::SearchConfig {
                        tavily_api_key: tavily_api_key.clone(),
                    }
                }),
            };
            let request = crate::ai_service::agent::ToolRunRequest {
                system_prompt: &system_prompt,
                user_input: &final_message,
                images: image_contents,
                tools: &tool_list,
                ctx: &tool_ctx,
                // 出图模型拿不到流式增量（图片必须整张返回）→ 通路为非流式
                mode: plan.mode,
            };
            agent
                .run_with_tools(request, move |chunk| {
                    let _ = app_handle.emit(
                        "ai-stream-chunk",
                        AiStreamChunkEvent {
                            request_id: chunk_request_id.clone(),
                            chunk: chunk.to_string(),
                        },
                    );
                })
                .await
        } else if !image_contents.is_empty() {
            // 多模态路径
            agent
                .run_with_images(&final_message, image_contents, move |chunk| {
                    let _ = app_handle.emit(
                        "ai-stream-chunk",
                        AiStreamChunkEvent {
                            request_id: chunk_request_id.clone(),
                            chunk: chunk.to_string(),
                        },
                    );
                })
                .await
        } else {
            agent
                .run(&final_message, move |chunk| {
                    let _ = app_handle.emit(
                        "ai-stream-chunk",
                        AiStreamChunkEvent {
                            request_id: chunk_request_id.clone(),
                            chunk: chunk.to_string(),
                        },
                    );
                })
                .await
        }
    };

    let chat_result = tokio::select! {
        result = chat_future => {
            *state.ai_cancel.lock().await = None;
            state.agent_busy.store(false, Ordering::Release);
            result
                .map(|_| ())
                .map_err(|e: crate::ai_service::error::AiError| e.to_string())
        }
        _ = cancel_rx => {
            // 用户主动停止：保留已收到的内容，不视为错误
            log::info!("[ai_chat] 用户停止生成 (request_id={request_id})");
            state.agent_busy.store(false, Ordering::Release);
            Ok(())
        }
    };

    // 本次请求可能探明了通路事实（网关没有出图端点 / 模型不在对话端点），
    // 写回配置以省掉下次启动的重新探测。
    // 放在 `select!` 之后：让 `chat_future` 对 `agent` 的可变借用先结束。
    persist_learned_transport(&state, &agent).await;

    chat_result?;
    let _ = app.emit("ai-stream-done", AiStreamDoneEvent { request_id });
    Ok(())
}

/// 停止当前 AI 生成：中断流式请求，前端保留已收到的内容。
#[tauri::command]
pub async fn stop_generation(state: State<'_, AppState>) -> Result<(), String> {
    let sender = state.ai_cancel.lock().await.take();
    if let Some(sender) = sender {
        let _ = sender.send(());
        log::info!("[stop_generation] 已请求停止当前生成");
    } else {
        log::info!("[stop_generation] 当前没有进行中的生成");
    }
    Ok(())
}

/// 重置对话上下文
#[tauri::command]
pub async fn reset_conversation(state: State<'_, AppState>) -> Result<(), String> {
    let mut agent = state.agent.write().await;
    agent.reset();
    Ok(())
}

/// 清除所有对话数据（消息 + 对话元数据），重置 Agent，创建新的默认对话。
/// 需要二次确认：前端应在调用前展示确认对话框。
#[tauri::command]
pub async fn clear_all_data(state: State<'_, AppState>) -> Result<(), String> {
    // AI 响应中不允许清除
    if state.agent_busy.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("AI 正在响应中，请稍后再试".to_string());
    }

    // 清除所有图片文件
    {
        let store = state.image_store.lock().await;
        store.delete_all_images();
    }

    // 清除数据库所有数据（含 images 表）
    {
        let db = state.database.lock().await;
        db.clear_all_data()?;
    }

    // 重置 Agent 上下文
    {
        let mut agent = state.agent.write().await;
        agent.reset();
    }

    // 更新当前对话 ID 为 default
    {
        let mut current = state.current_conversation_id.lock().await;
        *current = "default".to_string();
    }

    log::info!("[clear_all_data] 所有对话数据已清除");
    Ok(())
}

/// 用 AI 为会话生成标题（stateless：临时 provider，不污染主 Agent 上下文）。
///
/// 仅在以下情况生成：主请求空闲、会话标题仍为默认"新对话"。
/// 生成失败或主请求进行中时静默返回 None，不阻塞发送流程。
#[tauri::command]
pub async fn generate_conversation_title(
    state: State<'_, AppState>,
    conv_id: String,
) -> Result<Option<String>, String> {
    use crate::ai_service::context::ChatMessage;

    // 主请求进行中时跳过（避免与主请求竞争）
    if state.agent_busy.load(Ordering::Acquire) {
        return Ok(None);
    }

    // 取会话首条用户消息
    let first_message = {
        let db = state.database.lock().await;
        let msgs = db.load_messages(&conv_id)?;
        msgs.iter()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
    };
    let Some(first_message) = first_message else {
        return Ok(None);
    };

    // 仅当标题仍是默认"新对话"时才生成
    let title_is_default = {
        let db = state.database.lock().await;
        db.list_conversations()?
            .iter()
            .find(|c| c.id == conv_id)
            .map(|c| c.title == "新对话")
            .unwrap_or(false)
    };
    if !title_is_default {
        return Ok(None);
    }

    // 临时 provider 生成标题（非流式）
    let config = {
        let mgr = state.config_manager.lock().await;
        mgr.get().clone()
    };
    let provider = provider::from_config(&config);
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "你是一个对话标题生成器。根据用户的第一条消息，用不超过 15 个字概括对话主题。只输出标题本身，不要引号、标点或解释。".to_string(),
            images: vec![],
            tool_calls: vec![],
            tool_call_id: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: first_message.chars().take(500).collect(),
            images: vec![],
            tool_calls: vec![],
            tool_call_id: None,
        },
    ];

    let raw_title = provider.chat(&messages).await.map_err(|e| e.to_string())?;
    let title = raw_title
        .trim()
        .trim_matches('"')
        .trim_matches('“')
        .trim_matches('”')
        .trim()
        .chars()
        .take(30)
        .collect::<String>();
    if title.is_empty() {
        return Ok(None);
    }

    // 更新数据库标题
    {
        let db = state.database.lock().await;
        db.rename_conversation(&conv_id, &title)?;
    }
    log::info!("[ai_title] 会话 {conv_id} 标题已生成: {title}");
    Ok(Some(title))
}

#[cfg(test)]
mod tests {
    use super::{ai_config_changed, has_fresh_info_intent, has_reminder_intent};
    use crate::config::manager::AppConfig;

    #[test]
    fn detects_fresh_info_intents() {
        assert!(has_fresh_info_intent("今天有什么新闻"));
        assert!(has_fresh_info_intent("最新的 iPhone 发布了吗"));
        assert!(has_fresh_info_intent("现在股价多少"));
        assert!(has_fresh_info_intent("长沙天气怎么样"));
        assert!(has_fresh_info_intent("最近有什么进展"));
    }

    #[test]
    fn plain_questions_do_not_trigger_fresh_intent() {
        assert!(!has_fresh_info_intent("解释一下快速排序"));
        assert!(!has_fresh_info_intent("帮我写个 Python 脚本"));
        assert!(!has_fresh_info_intent("1+1 等于几"));
    }

    #[test]
    fn reminder_intent_still_works() {
        assert!(has_reminder_intent("10分钟后提醒我喝水"));
        assert!(has_reminder_intent("2小时后叫我开会"));
        // 取消类请求不触发"必须创建"的强指令
        assert!(!has_reminder_intent("取消提醒"));
        assert!(!has_reminder_intent("提醒功能怎么用"));
    }

    // ── 保存配置后是否需要重建 Agent ──
    //
    // 回归护栏：曾经这里是在 `mgr.update()` **之后** 读 `mgr.get()` 做比对，
    // 拿到的已是新配置 → 比对恒为相等 → Agent 永不重建 →「改了模型却仍用旧模型」。
    // 判断逻辑本身必须对"模型变了"返回 true，否则即使顺序修对了也不会生效。

    #[test]
    fn model_change_requires_agent_rebuild() {
        let old = AppConfig {
            model: "qwen3.5".to_string(),
            ..AppConfig::default()
        };
        let new = AppConfig {
            model: "deepseek-v4".to_string(),
            ..AppConfig::default()
        };
        assert!(
            ai_config_changed(&old, &new),
            "换模型必须重建 Agent，否则请求仍打到旧模型"
        );
    }

    #[test]
    fn all_ai_fields_trigger_rebuild() {
        let base = AppConfig::default();
        let cases: [(&str, AppConfig); 7] = [
            (
                "ai_provider",
                AppConfig {
                    ai_provider: "openai".to_string(),
                    ..base.clone()
                },
            ),
            (
                "api_key",
                AppConfig {
                    api_key: "sk-new".to_string(),
                    ..base.clone()
                },
            ),
            (
                "api_base",
                AppConfig {
                    api_base: "https://gw.example/v1".to_string(),
                    ..base.clone()
                },
            ),
            (
                "max_tokens",
                AppConfig {
                    max_tokens: 8192,
                    ..base.clone()
                },
            ),
            (
                "temperature",
                AppConfig {
                    temperature: 1.5,
                    ..base.clone()
                },
            ),
            (
                "ollama_endpoint",
                AppConfig {
                    ollama_endpoint: "http://127.0.0.1:11434".to_string(),
                    ..base.clone()
                },
            ),
            (
                "model_kind",
                AppConfig {
                    model_kind: "image".to_string(),
                    ..base.clone()
                },
            ),
        ];
        for (field, new) in cases {
            assert!(
                ai_config_changed(&base, &new),
                "{field} 变化后必须重建 Agent"
            );
        }
    }

    /// 纯外观 / 行为设置不应该触发重建——重建会清空 Agent 上下文，
    /// 一次改配色的保存不该把对话记忆重置掉。
    #[test]
    fn appearance_only_change_keeps_agent() {
        let old = AppConfig::default();
        let new = AppConfig {
            bg_color: "#000000".to_string(),
            bg_opacity: 0.5,
            accent_color: "#ff0000".to_string(),
            theme_mode: "dark".to_string(),
            theme_preset: "midnight".to_string(),
            bubble_auto_collapse: true,
            bubble_collapse_delay: 60,
            panel_width: 900,
            vision_mode: "on".to_string(),
            ..AppConfig::default()
        };
        assert!(
            !ai_config_changed(&old, &new),
            "外观/气泡/识图方式变化不应重建 Agent（识图方式每轮现读，不入 Agent 状态）"
        );
    }

    /// 内容完全相同（例如只点了保存）时必须判定为"未变化"，避免无谓重建。
    #[test]
    fn identical_config_is_not_a_change() {
        let cfg = AppConfig::default();
        assert!(!ai_config_changed(&cfg, &cfg.clone()));
    }
}
