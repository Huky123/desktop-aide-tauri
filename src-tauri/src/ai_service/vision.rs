use crate::ai_service::provider::ProviderType;

/// 图片输入方式（用户设置，默认 `Never` = 走 OCR）
///
/// 背景：模型是否支持识图很难可靠自动判断——能力注册表会滞后（新模型查不到），
/// 也可能误判（例如「moonshot-v1-8k」会被子串匹配到「moonshot-v1-8k-vision-preview」）。
/// 因此把决定权交给用户，默认按最保守处理（OCR 文字识别），
/// 需要直接把图片发给模型的用户显式选择「始终」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionMode {
    /// 自动：查模型能力注册表（`llm_models_spider`）
    Auto,
    /// 始终按多模态发送图片
    Always,
    /// 始终走 OCR 文字识别（默认）
    Never,
}

impl VisionMode {
    /// 从配置字符串解析（未知值一律按最保守的 `Never` 处理）
    pub fn from_config(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Self::Auto,
            "on" | "always" | "vision" => Self::Always,
            _ => Self::Never,
        }
    }
}

/// 按用户设置 + 模型能力，决定是否把图片按多模态发送
pub fn use_multimodal(mode: VisionMode, model: &str, provider_type: &ProviderType) -> bool {
    match mode {
        VisionMode::Always => true,
        VisionMode::Never => false,
        VisionMode::Auto => supports_vision(model, provider_type),
    }
}

/// 检测模型是否支持视觉/多模态输入。
///
/// 使用 `llm_models_spider` 仓库的自动更新模型能力注册表
/// （覆盖 OpenRouter API + LiteLLM + Chatbot Arena 数据源），
/// 搭配 Ollama 常见命名规则作为兜底。
///
/// 注意：仅在 `VisionMode::Auto` 下才会走到这里；默认配置不依赖本函数的判断。
pub fn supports_vision(model: &str, provider_type: &ProviderType) -> bool {
    let m = model.to_lowercase();

    // 优先使用 llm_models_spider 的自动更新注册表
    // （覆盖 Claude 3+/GPT-4o/Gemini/LLaVA/DeepSeek-VL 等数百个模型）
    if llm_models_spider::supports_vision(model) {
        return true;
    }

    // 兜底：Ollama 用户可能使用 llm_models_spider 未收录的自定义模型名
    // （Ollama 的模型名是用户自定义的 tag，不一定在公开注册表中）
    if *provider_type == ProviderType::Ollama {
        return m.contains("llava")
            || m.contains("bakllava")
            || m.contains("minicpm-v")
            || m.contains("llama3.2-vision")
            || m.contains("gemma3")
            || m.contains("multimodal")
            || m.contains("vision");
    }

    // 说明：此前这里对 DeepSeek 直接返回 false（依据是"DeepSeek 无公开视觉模型"）。
    // 该前提已不成立（DeepSeek V4.1 支持识图），故移除硬编码，交由注册表判断；
    // 用户也可在设置中直接选「始终按多模态发送」。
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 诊断用（默认忽略）：打印一批模型在当前注册表下的视觉判定结果。
    /// 运行：`cargo test --lib diag_vision_verdicts -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn diag_vision_verdicts() {
        let models = [
            "gpt-4o",
            "gpt-3.5-turbo",
            "claude-sonnet-4-5-20250929",
            "claude-3-5-sonnet-20241022",
            "gemini-2.5-flash",
            "deepseek-chat",
            "Qwen3.5",
            "qwen2.5-vl-7b",
            "qwen-vl-max",
            "llama3.2-vision",
            "moonshot-v1-8k",
            "glm-4.5-air",
        ];
        println!("{:<32} {:>8} {:>8}", "model", "custom", "ollama");
        for model in models {
            println!(
                "{:<32} {:>8} {:>8}",
                model,
                supports_vision(model, &ProviderType::Custom),
                supports_vision(model, &ProviderType::Ollama)
            );
        }
    }

    /// 明确的多模态模型必须被识别（否则图片会被错误地走 OCR 回退）
    #[test]
    fn well_known_vision_models_are_detected() {
        assert!(supports_vision("gpt-4o", &ProviderType::OpenAI));
        assert!(supports_vision("gemini-2.5-flash", &ProviderType::Gemini));
        assert!(supports_vision("qwen2.5-vl-7b", &ProviderType::Custom));
        // Ollama 自定义 tag 走命名兜底
        assert!(supports_vision("llama3.2-vision", &ProviderType::Ollama));
        assert!(supports_vision(
            "my-custom-llava:latest",
            &ProviderType::Ollama
        ));
    }

    /// 纯文本模型不得被误判为多模态（否则会发出服务端不支持的图片请求）
    #[test]
    fn text_only_models_are_not_vision() {
        assert!(!supports_vision("gpt-3.5-turbo", &ProviderType::OpenAI));
        assert!(!supports_vision("deepseek-chat", &ProviderType::DeepSeek));
        assert!(!supports_vision("glm-4.5-air", &ProviderType::Zhipu));
    }

    /// 用户设置优先级最高：Always 不看模型名，Never 无视注册表
    #[test]
    fn user_setting_overrides_model_detection() {
        // 始终按多模态发送 → 即使是文本模型也返回 true（用户显式要求）
        assert!(use_multimodal(
            VisionMode::Always,
            "gpt-3.5-turbo",
            &ProviderType::OpenAI
        ));
        assert!(use_multimodal(
            VisionMode::Always,
            "deepseek-v4.1",
            &ProviderType::DeepSeek
        ));
        // 默认（Never）→ 即使注册表认为支持识图，也走 OCR 回退
        assert!(!use_multimodal(
            VisionMode::Never,
            "gpt-4o",
            &ProviderType::OpenAI
        ));
        // Auto → 交给注册表
        assert!(use_multimodal(
            VisionMode::Auto,
            "gpt-4o",
            &ProviderType::OpenAI
        ));
        assert!(!use_multimodal(
            VisionMode::Auto,
            "gpt-3.5-turbo",
            &ProviderType::OpenAI
        ));
    }

    /// 配置字符串解析：未知值按最保守的 Never 处理
    #[test]
    fn vision_mode_parses_config_values() {
        assert_eq!(VisionMode::from_config("auto"), VisionMode::Auto);
        assert_eq!(VisionMode::from_config("ON"), VisionMode::Always);
        assert_eq!(VisionMode::from_config(" off "), VisionMode::Never);
        assert_eq!(VisionMode::from_config(""), VisionMode::Never);
        assert_eq!(VisionMode::from_config("whatever"), VisionMode::Never);
    }
}
