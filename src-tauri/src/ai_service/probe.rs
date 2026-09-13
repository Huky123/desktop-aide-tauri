//! 配置探测：把"这套配置到底能不能用"从"发一条消息试试"变成一次点击。
//!
//! 背景：配置链路原本**没有任何验证环节**——填完 Key 和模型名之后，唯一的反馈方式
//! 是发一条消息，而消息里的错误又常常误导。真实日志：一位用户在 7.5 小时内改了
//! 15 次配置、发了 24 次失败请求才定位到问题。
//!
//! 四项探测，**全部不产生出图费用**：
//! 1. `GET {base}/models` —— Key 是否被接受，顺带拿到可用模型清单；
//! 2. 当前模型是否在该清单里；
//! 3. 当前模型在对话端点是否可用（发一条 `max_tokens: 1` 的极短请求）；
//! 4. 该网关是否有独立出图路由（**不存在的模型名 + 不带 prompt**，任何正常网关
//!    都不会因此生成图片）。

use std::time::Duration;

use serde::Serialize;

use super::preview_chars;
use super::provider;
use crate::config::manager::AppConfig;

/// 单条探测结果
#[derive(Debug, Clone, Serialize)]
pub struct ProbeCheck {
    pub id: String,
    pub label: String,
    /// `ok` | `warn` | `fail`
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub checks: Vec<ProbeCheck>,
    /// `GET /models` 拉到的可用模型（部分网关不实现该端点，此时为空）
    pub models: Vec<String>,
    /// 建议的「模型用途」：`chat` / `image`；无法判断时为 `None`
    pub suggested_model_kind: Option<String>,
}

/// `GET /models` 的结果。
///
/// **必须区分"拿到了空列表"与"根本没拿到"**：大多数网关不实现 `/models`，
/// 只回一个空数组的话，用户点了「获取可用模型」没有任何反馈，只会以为按钮坏了。
///
/// `reason` 取值（成功时为 `None`，由前端翻成一句人话）：
/// `no_base` 没填地址 / `auth` Key 无效 / `no_endpoint` 服务商不提供该接口
/// / `network` 连不上。
#[derive(Debug, Clone, Serialize)]
pub struct ModelListResult {
    pub models: Vec<String>,
    pub reason: Option<String>,
}

impl ModelListResult {
    fn failed(reason: &str) -> Self {
        Self {
            models: Vec::new(),
            reason: Some(reason.to_string()),
        }
    }
}

fn check(id: &str, label: &str, status: &str, detail: impl Into<String>) -> ProbeCheck {
    ProbeCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        detail: detail.into(),
    }
}

/// 探测用的短超时客户端：用户正等着按钮出结果，不能按出图那种 10 分钟等
fn probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .expect("构建 HTTP client 失败")
}

/// 只拉模型清单（供设置界面的模型下拉使用）。
///
/// **不返回 `Result`**：拉不到不是错误，只是"这个服务商不提供"。原因放在
/// `reason` 里，由前端决定怎么措辞——下拉只是辅助，手填永远可用。
pub async fn list_models(config: &AppConfig) -> ModelListResult {
    let base = provider::resolve_api_base(config);
    if base.trim().is_empty() {
        return ModelListResult::failed("no_base");
    }
    let (status, body) = match get(
        &probe_client(),
        &format!("{}/models", base.trim_end_matches('/')),
        &config.api_key,
    )
    .await
    {
        Ok(pair) => pair,
        Err(_) => return ModelListResult::failed("network"),
    };
    classify_models_response(status, &body)
}

/// 把 `/models` 的状态码与 body 翻成 `ModelListResult`。
///
/// 抽成纯函数是为了能直接单测——真实网关在这条路径上返回的东西五花八门，
/// 而"没反馈"正是当初漏掉的那一块。
fn classify_models_response(status: u16, body: &str) -> ModelListResult {
    if (200..300).contains(&status) {
        ModelListResult {
            models: parse_model_ids(body),
            reason: None,
        }
    } else if status == 401 || status == 403 {
        ModelListResult::failed("auth")
    } else {
        // 404 / 405 / 501 等：绝大多数是"该服务商没实现 /models"
        ModelListResult::failed("no_endpoint")
    }
}

/// 完整探测一次配置。**任何单项失败都不中断其余探测**——用户需要的是一张完整的
/// 体检表，而不是第一个错误。
pub async fn probe(config: &AppConfig) -> ProbeReport {
    let mut checks = Vec::new();
    let base = provider::resolve_api_base(config);

    if base.trim().is_empty() {
        checks.push(check("base", "API 地址", "fail", "未填写 API 地址"));
        return ProbeReport {
            checks,
            models: Vec::new(),
            suggested_model_kind: None,
        };
    }

    let client = probe_client();

    // ── 1/2. Key 与模型清单 ──
    let mut models = Vec::new();
    match get(&client, &format!("{}/models", base.trim_end_matches('/')), &config.api_key).await {
        Ok((status, body)) if (200..300).contains(&status) => {
            models = parse_model_ids(&body);
            checks.push(check("key", "API Key", "ok", "已被服务端接受"));
            checks.push(check(
                "models",
                "模型清单",
                if models.is_empty() { "warn" } else { "ok" },
                if models.is_empty() {
                    "服务端未返回模型列表（该端点可能未实现，不影响使用）".to_string()
                } else {
                    format!("服务端提供 {} 个模型", models.len())
                },
            ));
        }
        Ok((status, body)) if status == 401 || status == 403 => {
            checks.push(check(
                "key",
                "API Key",
                "fail",
                format!("认证失败（HTTP {status}）：{}", preview_chars(&body, 160)),
            ));
        }
        Ok((status, _)) => {
            checks.push(check(
                "key",
                "API Key",
                "warn",
                format!("拉取模型列表返回 HTTP {status}（该端点可能未实现，不代表 Key 无效）"),
            ));
        }
        Err(error) => {
            checks.push(check("key", "API Key", "warn", format!("无法连接：{error}")));
        }
    }

    if !models.is_empty() {
        if models.iter().any(|model| model == &config.model) {
            checks.push(check(
                "model_listed",
                "模型名称",
                "ok",
                format!("「{}」在服务端清单里", config.model),
            ));
        } else {
            let sample = models.iter().take(5).cloned().collect::<Vec<_>>().join("、");
            checks.push(check(
                "model_listed",
                "模型名称",
                "fail",
                format!(
                    "「{}」不在服务端清单里，可能名字写错或该账号无权限。清单前几个：{sample}",
                    config.model
                ),
            ));
        }
    }

    // ── 3. 对话端点是否接受当前模型 ──
    let mut suggested = None;
    let chat_url = provider::resolve_chat_endpoint(config);
    match post_chat(&client, &chat_url, &config.api_key, &config.model).await {
        Ok((status, _)) if (200..300).contains(&status) => {
            suggested = Some("chat".to_string());
            checks.push(check("chat", "对话端点", "ok", "该模型可用于对话"));
        }
        Ok((status, body)) => {
            if body
                .to_ascii_lowercase()
                .contains("not supported on the chat completions")
            {
                suggested = Some("image".to_string());
                checks.push(check(
                    "chat",
                    "对话端点",
                    "warn",
                    "该模型不在对话端点服务——看起来是出图模型，建议把「模型用途」设为「是出图模型」",
                ));
            } else {
                checks.push(check(
                    "chat",
                    "对话端点",
                    "fail",
                    format!("HTTP {status}：{}", preview_chars(&body, 160)),
                ));
            }
        }
        Err(error) => {
            checks.push(check("chat", "对话端点", "warn", format!("请求失败：{error}")));
        }
    }

    // ── 4. 该网关有没有独立出图路由 ──
    let images_url = format!("{}/images/generations", base.trim_end_matches('/'));
    match post_images_probe(&client, &images_url, &config.api_key).await {
        Ok((status, _)) => {
            // 只有 404 才说明路由不存在；400 之类的"路由存在但这次没成功"也算存在
            let exists = status != 404;
            checks.push(check(
                "images_route",
                "出图接口",
                if exists { "ok" } else { "warn" },
                if exists {
                    "该网关提供 /images/generations，出图模型会走这里"
                } else {
                    "该网关没有独立出图路由，出图模型会改走对话端点返回图片"
                },
            ));
        }
        Err(error) => {
            checks.push(check(
                "images_route",
                "出图接口",
                "warn",
                format!("探测失败：{error}"),
            ));
        }
    }

    ProbeReport {
        checks,
        models,
        suggested_model_kind: suggested,
    }
}

async fn get(client: &reqwest::Client, url: &str, api_key: &str) -> Result<(u16, String), String> {
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

/// 极短对话请求：只为确认"这个模型在对话端点能不能用"，`max_tokens: 1` 把开销压到最低
async fn post_chat(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    model: &str,
) -> Result<(u16, String), String> {
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": "ping" }],
            "max_tokens": 1,
            "stream": false,
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

/// 出图路由探测。**刻意不传 `prompt` 且用一个不存在的模型名**：无论服务端怎么实现，
/// 都不可能因此真的生成一张图片（真实网关会回 `prompt is required` 或模型校验错），
/// 于是"非 404"就等价于"这条路由存在"。
async fn post_images_probe(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<(u16, String), String> {
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "model": "__desktop_aide_probe__" }))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

/// 从 `GET /models` 的响应里取模型 id 列表（兼容 `{"data":[{"id":…}]}` 与裸数组）
fn parse_model_ids(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let items = value
        .get("data")
        .and_then(|data| data.as_array())
        .or_else(|| value.as_array());
    let Some(items) = items else {
        return Vec::new();
    };
    let mut ids: Vec<String> = items
        .iter()
        .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
        .map(str::to_owned)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_style_model_list() {
        let body = r#"{"object":"list","data":[{"id":"b-model"},{"id":"a-model"},{"id":"a-model"}]}"#;
        // 去重 + 排序，避免下拉里出现重复项
        assert_eq!(parse_model_ids(body), vec!["a-model", "b-model"]);
    }

    #[test]
    fn parses_bare_array_model_list() {
        assert_eq!(
            parse_model_ids(r#"[{"id":"x"},{"name":"没有 id 的项"}]"#),
            vec!["x"]
        );
    }

    #[test]
    fn tolerates_non_json_model_list() {
        // 有的网关在 /models 上返回 HTML 或空体，不能让探测崩掉
        assert!(parse_model_ids("<html>404</html>").is_empty());
        assert!(parse_model_ids("").is_empty());
        assert!(parse_model_ids("{}").is_empty());
    }

    /// 失败原因必须能带到前端——"点了按钮没反应"就是因为这里当初全被吞掉了
    #[test]
    fn keeps_the_reason_when_the_model_list_cannot_be_read() {
        // 401 / 403 → Key 问题
        assert_eq!(
            classify_models_response(401, r#"{"error":{"message":"invalid api key"}}"#).reason,
            Some("auth".to_string())
        );
        assert_eq!(classify_models_response(403, "").reason, Some("auth".to_string()));

        // 未实现 /models（最常见）与 5xx → 都归到"服务商不提供该接口"
        assert_eq!(
            classify_models_response(404, "404 page not found").reason,
            Some("no_endpoint".to_string())
        );
        assert_eq!(
            classify_models_response(405, "").reason,
            Some("no_endpoint".to_string())
        );
        assert_eq!(
            classify_models_response(500, "oops").reason,
            Some("no_endpoint".to_string())
        );

        // 失败时模型列表一定为空，免得前端拿到半截数据
        assert!(classify_models_response(401, "{}").models.is_empty());
    }

    #[test]
    fn reports_success_without_a_reason() {
        let result = classify_models_response(200, r#"{"data":[{"id":"m1"}]}"#);
        assert_eq!(result.reason, None);
        assert_eq!(result.models, vec!["m1"]);

        // 2xx 但 body 解析不出模型：也算成功（reason 为空），前端按"没有返回列表"处理
        let empty = classify_models_response(200, "<html>hi</html>");
        assert_eq!(empty.reason, None);
        assert!(empty.models.is_empty());
    }
}
