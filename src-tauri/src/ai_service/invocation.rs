//! 模型调用的**通路决策**。
//!
//! 背景（为什么要单独一个模块）：改造前，"这个模型该怎么调用"这件事散在四处
//! ——`commands/ai.rs` 算 `use_tools`/`stream`、`Agent::stream_messages` 又算一遍
//! 走流式还是走 `generate_image`、`Agent::request_round` 收一个 `stream: bool`、
//! 最后 `OpenAiCompatibleProvider::generate_image` 再判一次"我到底该不该用出图接口"。
//! 四处用的是**两个不同的谓词**（`is_image_model()` 与
//! `uses_dedicated_image_endpoint()`——后者是 provider×模型名的混合谓词，已删除），
//! 它们必须互相一致却没有任何机制保证。
//!
//! 实际后果：命令层打印「出图通路: true | 非流式: true」，请求却打到了
//! `/chat/completions` —— 网关回 `This model is not supported on the Chat
//! Completions endpoint`，功能不可用且原因难查。
//!
//! 现在改成：**通路在这里算一次，作为一个有类型的值往下传**，
//! 各层只读不改判。

/// 访问某个模型的一条通路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceMode {
    /// `chat/completions` + SSE 流式（可携带 tools）
    ChatStream,
    /// `chat/completions` 非流式（可携带 tools）
    ChatBlocking,
    /// `POST {base}/images/generations`，只接受 `prompt` 字符串（**不可**携带 tools）
    ImageGeneration,
}

impl InferenceMode {
    /// 文本是否以增量（SSE delta）形式返回
    pub fn streams(self) -> bool {
        matches!(self, Self::ChatStream)
    }

    /// 该通路能否携带 `tools`
    pub fn supports_tools(self) -> bool {
        !matches!(self, Self::ImageGeneration)
    }

    /// 该通路是否走 `chat/completions` 端点
    pub fn uses_chat_endpoint(self) -> bool {
        !matches!(self, Self::ImageGeneration)
    }
}

/// 解析某个 (模型用途 × provider 协议形态) 组合应走哪条通路。
///
/// **全项目唯一的通路决策入口**：命令层、`Agent`、provider 都只调用它，
/// 不再各自用 provider 类型 / 模型名重新推断。
///
/// - 出图模型 + 该协议**可能**有独立出图端点 ⇒ `ImageGeneration`
/// - 出图模型 + 不可能有（Anthropic 协议没有该端点） ⇒ `ChatBlocking`
///   （图片以 Markdown 形式出现在对话响应里）
/// - 非出图模型 ⇒ `ChatStream`（无论第二个入参是什么）
///
/// 第二个入参只表达"**可能**有"，不表达"确实有"——后者静态判定不了。
/// 实际是否存在由 `Agent` 发一次请求探测：若回 404（这个网关没有该路由），
/// 就把结论记住并改走 `ChatBlocking`，见 `Agent::stream_messages`。
pub fn resolve(is_image_model: bool, may_have_images_endpoint: bool) -> InferenceMode {
    match (is_image_model, may_have_images_endpoint) {
        (false, _) => InferenceMode::ChatStream,
        (true, true) => InferenceMode::ImageGeneration,
        (true, false) => InferenceMode::ChatBlocking,
    }
}

/// 本轮请求的计划：走哪条通路 + 是否携带工具。
///
/// 由命令层在拿到通路后**一次算好**，避免"工具开关"和"传输方式"各自判断、
/// 结果互相矛盾。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferencePlan {
    pub mode: InferenceMode,
    /// 本轮是否把工具列表发给模型。
    /// 两个条件都要满足：用户开了工具/联网，且该通路本身支持工具
    /// （`/images/generations` 只收 prompt 字符串，永远带不了工具）。
    pub use_tools: bool,
}

impl InferencePlan {
    pub fn new(mode: InferenceMode, enable_tools: bool, enable_web_search: bool) -> Self {
        Self {
            mode,
            use_tools: (enable_tools || enable_web_search) && mode.supports_tools(),
        }
    }
}

/// "这个网关到底有没有独立出图端点"静态判定不了：同一个 OpenAI 兼容网关，
/// 有的在 `/images/generations` 出图，有的把图片放进对话响应的 Markdown 里。
/// 所以本模块只处理"**可能**有"，实际存在性由 `Agent` 探测；协议形态的判定留在
/// `AiProvider::may_have_images_endpoint()`（只有 provider 层知道协议形态）。
#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动：把 (模型用途 × 协议形态) 的**全部组合**钉死在这里。
    ///
    /// 任何一行结果变化都意味着行为变更，必须是有意为之并同步更新预期。
    #[test]
    fn resolve_covers_every_combination() {
        // (is_image_model, may_have_images_endpoint, 期望通路)
        let cases = [
            (false, false, InferenceMode::ChatStream), // 普通对话
            (false, true, InferenceMode::ChatStream),  // 显式标"不是出图"：仍走对话流式
            // OpenAI 兼容（含自定义网关）：先按专用出图接口试；
            // 探测到 404 才由 `Agent` 回退到 `ChatBlocking`
            (true, true, InferenceMode::ImageGeneration),
            // Anthropic 协议没有该端点：图片只能在对话响应里
            (true, false, InferenceMode::ChatBlocking),
        ];
        for (is_image, may_have, expected) in cases {
            assert_eq!(
                resolve(is_image, may_have),
                expected,
                "resolve(is_image={is_image}, may_have_images_endpoint={may_have})"
            );
        }
    }

    #[test]
    fn mode_capabilities() {
        assert!(InferenceMode::ChatStream.streams());
        assert!(!InferenceMode::ChatBlocking.streams());
        assert!(!InferenceMode::ImageGeneration.streams());

        assert!(InferenceMode::ChatStream.supports_tools());
        assert!(InferenceMode::ChatBlocking.supports_tools());
        // 专用出图接口只接受 prompt，永远带不了工具
        assert!(!InferenceMode::ImageGeneration.supports_tools());

        assert!(InferenceMode::ChatStream.uses_chat_endpoint());
        assert!(InferenceMode::ChatBlocking.uses_chat_endpoint());
        assert!(!InferenceMode::ImageGeneration.uses_chat_endpoint());
    }

    /// 表驱动：用户开关 × 通路 ⇒ 本轮是否带工具。
    /// 这就是改造前 `ai.rs` 里 `use_tools` 那一行的全部组合。
    #[test]
    fn plan_derives_tools_from_mode_and_switches() {
        // (通路, enable_tools, enable_web_search, 期望 use_tools)
        let cases = [
            (InferenceMode::ChatStream, false, false, false),
            (InferenceMode::ChatStream, true, false, true),
            (InferenceMode::ChatStream, false, true, true),
            (InferenceMode::ChatBlocking, false, false, false),
            (InferenceMode::ChatBlocking, true, false, true),
            (InferenceMode::ChatBlocking, false, true, true),
            // 专用出图接口：无论开关如何都不带工具
            (InferenceMode::ImageGeneration, false, false, false),
            (InferenceMode::ImageGeneration, true, false, false),
            (InferenceMode::ImageGeneration, false, true, false),
            (InferenceMode::ImageGeneration, true, true, false),
        ];
        for (mode, enable_tools, enable_web, expected) in cases {
            let plan = InferencePlan::new(mode, enable_tools, enable_web);
            assert_eq!(
                plan.use_tools, expected,
                "plan({mode:?}, tools={enable_tools}, web={enable_web})"
            );
            assert_eq!(plan.mode, mode, "通路本身不应被开关改变");
        }
    }

    /// 传输方式只由通路决定，与工具开关无关。
    #[test]
    fn streaming_is_decided_by_mode_only() {
        for mode in [
            InferenceMode::ChatStream,
            InferenceMode::ChatBlocking,
            InferenceMode::ImageGeneration,
        ] {
            for enable_tools in [false, true] {
                for enable_web in [false, true] {
                    assert_eq!(
                        InferencePlan::new(mode, enable_tools, enable_web)
                            .mode
                            .streams(),
                        mode.streams()
                    );
                }
            }
        }
    }

    /// 专用出图接口只收 `prompt` 字符串，无论用户开关如何都不带工具。
    /// （"这个网关到底有没有该接口"由 `Agent` 探测，与本模块无关。）
    #[test]
    fn image_modes_never_allow_tools() {
        assert!(!InferencePlan::new(InferenceMode::ImageGeneration, true, true).use_tools);
    }
}
