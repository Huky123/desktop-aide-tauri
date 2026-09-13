use std::fmt;

#[derive(Debug)]
pub enum AiError {
    /// 网络连接失败（超时、DNS、连接拒绝等）
    Network(String),
    /// API 认证失败（401/403）
    Auth(String),
    /// 请求频率超限（429）
    RateLimit(String),
    /// 模型无效或不存在（404）
    InvalidModel(String),
    /// 流式读取中断
    Stream(String),
    /// 服务端错误（5xx）
    ServerError(String),
    /// 其他未分类错误
    Other(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::Network(msg) => write!(f, "网络连接失败: {}", msg),
            AiError::Auth(msg) => write!(f, "认证失败: {}", msg),
            AiError::RateLimit(msg) => write!(f, "请求频率超限: {}", msg),
            AiError::InvalidModel(msg) => write!(f, "模型无效: {}", msg),
            AiError::Stream(msg) => write!(f, "流式读取错误: {}", msg),
            AiError::ServerError(msg) => write!(f, "服务端错误: {}", msg),
            AiError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<reqwest::Error> for AiError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            AiError::Network("请求超时".to_string())
        } else if e.is_connect() {
            AiError::Network(format!("无法连接到服务器: {}", e))
        } else {
            AiError::Network(e.to_string())
        }
    }
}

/// HTTP 状态码 → AiError（携带模型名用于 InvalidModel 的友好提示）
pub fn classify_http_error(status_code: u16, model_name: &str, error_body: &str) -> AiError {
    let detail = format!(
        "{} (详情: {})",
        http_status_hint(status_code, model_name),
        error_body
    );
    match status_code {
        401 | 403 => AiError::Auth(detail),
        404 => AiError::InvalidModel(detail),
        429 => AiError::RateLimit(detail),
        500..=599 => AiError::ServerError(detail),
        _ => AiError::Other(format!("HTTP {}: {}", status_code, error_body)),
    }
}

/// HTTP 状态码 → 用户可读的中文提示
pub fn http_status_hint(status_code: u16, model_name: &str) -> String {
    match status_code {
        401 => "API Key 无效或已过期，请检查设置".to_string(),
        403 => "无权限访问该模型，请检查 API Key 或模型名称".to_string(),
        404 => format!("模型 '{}' 未找到，请确认模型名称是否正确", model_name),
        429 => "请求频率超限，请稍后重试".to_string(),
        500..=599 => "AI 服务端错误，请稍后重试".to_string(),
        _ => format!("HTTP {}", status_code),
    }
}
