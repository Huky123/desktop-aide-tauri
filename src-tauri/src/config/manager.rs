use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_provider")]
    pub ai_provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_base: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_ollama_endpoint")]
    pub ollama_endpoint: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    #[serde(default = "default_bg_color")]
    pub bg_color: String,
    #[serde(default = "default_bg_opacity")]
    pub bg_opacity: f64,
    #[serde(default = "default_accent_color")]
    pub accent_color: String,
    #[serde(default)]
    pub msg_user_bg: String,
    #[serde(default)]
    pub msg_user_border: String,
    #[serde(default = "default_panel_width")]
    pub panel_width: u32,
    #[serde(default = "default_panel_height")]
    pub panel_height: u32,
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    #[serde(default = "default_theme_preset")]
    pub theme_preset: String,
    /// 气泡自动折叠开关
    #[serde(default = "default_bubble_auto_collapse")]
    pub bubble_auto_collapse: bool,
    /// 气泡自动折叠延迟（秒），面板关闭后多久自动收折为边缘细条
    #[serde(default = "default_bubble_collapse_delay")]
    pub bubble_collapse_delay: u32,
    /// 工具调用总开关（MVP：默认关闭，开启后 ai_chat 走工具闭环）
    #[serde(default)]
    pub enable_tools: bool,
    /// 允许 AI 联网搜索（AI 按需自动调用 web_search 工具）
    #[serde(default)]
    pub enable_web_search: bool,
    /// Tavily 搜索 API Key（可选；为空时自动退回 DuckDuckGo 免 key 搜索）
    #[serde(default)]
    pub tavily_api_key: String,
    /// 图片输入方式：`off`（默认，走 OCR 文字识别）/ `auto`（按模型能力注册表判断）/ `on`（始终按多模态发送）
    #[serde(default = "default_vision_mode")]
    pub vision_mode: String,
    /// 模型用途：`auto`（默认，按模型名判断）/ `chat`（对话）/ `vision`（识图，图片直接发给模型）/ `image`（出图，跳过流式并禁用工具）
    #[serde(default = "default_model_kind")]
    pub model_kind: String,
    /// 上次探明的通路事实：`""`（未知）/ `no_images_route` / `no_chat_route`。
    ///
    /// 由后端在探测成功/失败后写回，**前端不参与**（`save_config` 会原样保留，
    /// 换服务商/模型/Key 时清空——旧结论对新配置不再适用）。
    ///
    /// 不写回就得每次启动重新花一次请求试出来。
    #[serde(default)]
    pub learned_transport: String,
}

fn default_panel_width() -> u32 {
    420
}
fn default_panel_height() -> u32 {
    600
}

/// 默认服务商：DeepSeek。
///
/// 面向国内用户，Key 获取门槛最低、价格便宜；原默认值是 Anthropic + 一个写死的
/// Claude 模型名，新用户装完面对的是一个需要海外 Key 的配置，而界面上当时也没有
/// 任何"先去填 Key"的引导。
fn default_provider() -> String {
    "deepseek".to_string()
}
fn default_model() -> String {
    "deepseek-chat".to_string()
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_temperature() -> f64 {
    0.7
}
fn default_ollama_endpoint() -> String {
    "http://localhost:11434".to_string()
}
fn default_ollama_model() -> String {
    "llama3.2".to_string()
}
fn default_bg_color() -> String {
    "#121216".to_string()
}
fn default_bg_opacity() -> f64 {
    0.72
}
fn default_accent_color() -> String {
    "#818cf8".to_string()
}
fn default_theme_mode() -> String {
    "auto".to_string()
}
fn default_theme_preset() -> String {
    "indigo".to_string()
}
fn default_bubble_auto_collapse() -> bool {
    false
}
fn default_bubble_collapse_delay() -> u32 {
    300
}
/// 图片输入方式默认值：`off` —— 交给 OCR 文字识别，最保守（模型识图能力由用户显式开启）
fn default_vision_mode() -> String {
    "off".to_string()
}
/// 模型用途默认值：`chat` —— 不按出图模型处理（设置界面里是二选一，没有"自动"中间态）
fn default_model_kind() -> String {
    "chat".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ai_provider: default_provider(),
            model: default_model(),
            api_key: String::new(),
            api_base: String::new(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            ollama_endpoint: default_ollama_endpoint(),
            ollama_model: default_ollama_model(),
            bg_color: default_bg_color(),
            bg_opacity: default_bg_opacity(),
            accent_color: default_accent_color(),
            msg_user_bg: String::new(),
            msg_user_border: String::new(),
            panel_width: default_panel_width(),
            panel_height: default_panel_height(),
            theme_mode: default_theme_mode(),
            theme_preset: default_theme_preset(),
            bubble_auto_collapse: default_bubble_auto_collapse(),
            bubble_collapse_delay: default_bubble_collapse_delay(),
            enable_tools: false,
            enable_web_search: false,
            tavily_api_key: String::new(),
            vision_mode: default_vision_mode(),
            model_kind: default_model_kind(),
            learned_transport: String::new(),
        }
    }
}

pub struct ConfigManager {
    path: PathBuf,
    config: AppConfig,
}

impl ConfigManager {
    /// 从数据目录加载配置（数据目录不存在或文件缺失时使用默认配置）
    pub fn new(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("config.json");
        let config = Self::load_from(&path).unwrap_or_default();
        Self { path, config }
    }

    fn load_from(path: &PathBuf) -> Option<AppConfig> {
        if let Ok(content) = fs::read_to_string(path) {
            serde_json::from_str(&content).ok().map(Self::migrate)
        } else {
            None
        }
    }

    /// 字段迁移：把旧版本配置文件升级到当前语义。
    ///
    /// 设置界面里的「识图」「出图」现在是两个**二选一**开关，不会再产生 `auto`
    /// 这种中间态；但旧配置里可能存着 `auto`（以及更早四选一的 `vision`）。
    /// 这里把它们**一次性解析成具体取值**，原因有二：
    /// 1. 二选一开关无法显示中间态，不解析就会出现"两个选项都没选中"；
    /// 2. 解析用的是与运行时同一套启发式，因此升级**不会静默改变**用户已有的效果。
    fn migrate(mut config: AppConfig) -> AppConfig {
        let raw_model_kind = config.model_kind.trim().to_ascii_lowercase();
        let raw_vision_mode = config.vision_mode.trim().to_ascii_lowercase();

        // 旧版四选一里的「识图」：识图维度已独立出去
        if raw_model_kind == "vision" {
            log::info!(
                "[config] 旧版 model_kind=\"vision\" → vision_mode=\"on\"（识图维度已独立）"
            );
            config.vision_mode = "on".to_string();
        }

        // 出图维度：`auto`（及未知值）按模型名一次性解析为 image / chat
        if raw_model_kind == "vision" || raw_model_kind == "auto" || raw_model_kind.is_empty() {
            let is_image = crate::ai_service::agent::is_image_generation_model(&config.model);
            log::info!(
                "[config] 模型用途 {:?} 按模型名解析为 {:?}（模型 {}）",
                config.model_kind,
                if is_image { "image" } else { "chat" },
                config.model
            );
            config.model_kind = if is_image { "image" } else { "chat" }.to_string();
        }

        // 识图维度：`auto` 按模型能力注册表一次性解析为 on / off
        if raw_vision_mode == "auto" {
            let provider_type =
                crate::ai_service::provider::resolve_provider_type(&config.ai_provider);
            let supported =
                crate::ai_service::vision::supports_vision(&config.model, &provider_type);
            log::info!(
                "[config] 图片输入方式 \"auto\" 按模型能力解析为 {:?}（模型 {}）",
                if supported { "on" } else { "off" },
                config.model
            );
            config.vision_mode = if supported { "on" } else { "off" }.to_string();
        }

        config
    }

    pub fn get(&self) -> &AppConfig {
        &self.config
    }

    /// 保存配置到文件并替换内存中的配置。
    ///
    /// **返回被替换掉的旧配置**。调用方通常需要拿它与新配置做比对（例如判断
    /// 是否需要重建 AI 适配器）。把旧值直接交出来是刻意的 API 形状：若让调用方
    /// 在 `update()` 之后再 `get()`，拿到的已经是新值，比对必然全部相等——
    /// 「改了模型却仍用旧模型」正是这么来的。有了返回值，正确用法是自然的。
    pub fn update(&mut self, new_config: AppConfig) -> Result<AppConfig, String> {
        // 确保目录存在
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {}", e))?;
        }
        let content = serde_json::to_string_pretty(&new_config)
            .map_err(|e| format!("序列化配置失败: {}", e))?;

        // 原子写入：先写临时文件，再 rename 替换目标
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, &content).map_err(|e| format!("写入临时配置文件失败: {}", e))?;
        fs::rename(&tmp_path, &self.path).map_err(|e| format!("替换配置文件失败: {}", e))?;

        // 文件写入成功后才替换内存值，并把旧值返回给调用方
        Ok(std::mem::replace(&mut self.config, new_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_bubble_behavior_settings() {
        let root =
            std::env::temp_dir().join(format!("desktop-aide-config-{}", uuid::Uuid::new_v4()));
        let path = root.join("config.json");
        let mut manager = ConfigManager {
            path: path.clone(),
            config: AppConfig::default(),
        };
        let config = AppConfig {
            bubble_auto_collapse: true,
            bubble_collapse_delay: 3,
            ..AppConfig::default()
        };

        manager.update(config).expect("保存配置应成功");
        let restored = ConfigManager::load_from(&path).expect("应能重新读取配置");

        assert!(restored.bubble_auto_collapse);
        assert_eq!(restored.bubble_collapse_delay, 3);
        let _ = std::fs::remove_dir_all(root);
    }

    /// 写一份临时配置并读回，用于验证迁移结果
    fn load_with(json: &str) -> AppConfig {
        let root =
            std::env::temp_dir().join(format!("desktop-aide-config-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("创建临时目录应成功");
        let path = root.join("config.json");
        std::fs::write(&path, json).expect("写入配置应成功");
        let restored = ConfigManager::load_from(&path).expect("应能读取配置");
        let _ = std::fs::remove_dir_all(root);
        restored
    }

    /// 旧版四选一的「识图」必须迁移到独立的 vision_mode，
    /// 否则升级后用户选好的识图能力会静默丢失（图片改走 OCR）。
    #[test]
    fn migrates_legacy_vision_model_kind_to_vision_mode() {
        let restored = load_with(r#"{"model_kind":"vision","vision_mode":"off","model":"gpt-4o"}"#);
        assert_eq!(
            restored.vision_mode, "on",
            "旧的识图标记应迁移为 vision_mode=on"
        );
        assert_eq!(
            restored.model_kind, "chat",
            "识图维度独立后，出图维度按模型名解析（gpt-4o 不是出图模型）"
        );
    }

    /// 现行取值不得被迁移逻辑误改（尤其 image 仍表示"能出图"）。
    #[test]
    fn keeps_current_model_kind_values_untouched() {
        let restored = load_with(r#"{"model_kind":"image","vision_mode":"on"}"#);
        assert_eq!(restored.model_kind, "image");
        assert_eq!(restored.vision_mode, "on", "识图与出图可同时开启");
    }

    /// 旧的 `auto` 必须解析成具体取值（二选一开关显示不了中间态），
    /// 且解析结果要与升级前的运行时行为一致。
    #[test]
    fn resolves_legacy_auto_into_concrete_values() {
        // 出图维度：名字像出图模型 → image
        let restored = load_with(r#"{"model_kind":"auto","vision_mode":"off","model":"dall-e-3"}"#);
        assert_eq!(restored.model_kind, "image", "dall-e-3 应解析为出图模型");

        // 出图维度：普通对话模型 → chat
        let restored =
            load_with(r#"{"model_kind":"auto","vision_mode":"off","model":"deepseek-chat"}"#);
        assert_eq!(restored.model_kind, "chat");

        // 识图维度：注册表认为支持 → on
        let restored = load_with(
            r#"{"model_kind":"chat","vision_mode":"auto","model":"gpt-4o","ai_provider":"openai"}"#,
        );
        assert_eq!(restored.vision_mode, "on", "gpt-4o 应解析为支持识图");

        // 识图维度：纯文本模型 → off
        let restored = load_with(
            r#"{"model_kind":"chat","vision_mode":"auto","model":"gpt-3.5-turbo","ai_provider":"openai"}"#,
        );
        assert_eq!(restored.vision_mode, "off");
    }

    /// 缺字段的新配置使用保守默认：不按出图模型、识图走 OCR。
    #[test]
    fn defaults_are_conservative_binary_choices() {
        let restored = load_with(r#"{"model":"gpt-4o"}"#);
        assert_eq!(restored.model_kind, "chat");
        assert_eq!(restored.vision_mode, "off");
    }

    /// 回归护栏：`update` 必须把**被替换掉的旧配置**交出来。
    ///
    /// 曾经的 bug：调用方在 `update()` 之后才 `get()` 旧值，拿到的已经是新配置，
    /// 「AI 配置是否变化」的比对恒为相等 → Agent 永不重建 → 改了模型仍用旧模型。
    /// 把旧配置作为返回值交出去，才让正确用法成为自然写法。
    #[test]
    fn update_returns_replaced_config() {
        let root =
            std::env::temp_dir().join(format!("desktop-aide-config-{}", uuid::Uuid::new_v4()));
        let mut manager = ConfigManager {
            path: root.join("config.json"),
            config: AppConfig {
                model: "old-model".to_string(),
                ..AppConfig::default()
            },
        };

        let new = AppConfig {
            model: "new-model".to_string(),
            ..AppConfig::default()
        };
        let replaced = manager.update(new).expect("保存配置应成功");

        assert_eq!(replaced.model, "old-model", "返回值必须是旧配置");
        assert_eq!(manager.get().model, "new-model", "内存配置应已替换为新值");
        // 关键：两者必须不同——若返回值等于当前值，调用方的比对就会恒等
        assert_ne!(replaced.model, manager.get().model);
        let _ = std::fs::remove_dir_all(root);
    }
}
