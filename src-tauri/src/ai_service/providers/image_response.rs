use crate::ai_service::preview_chars;
use base64::Engine;

const IMAGE_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"];

const IMAGE_HOST_FRAGMENTS: &[&str] = &[
    "storage.googleapis.com",
    "generativeai",
    "vertexai",
    "googleapis.com/ai",
    "openai.com/api/images",
    "filesystem.iigpt.com",
];

fn url_looks_like_image(url: &str) -> bool {
    let lower = url.to_lowercase();
    IMAGE_EXTENSIONS
        .iter()
        .any(|extension| lower.contains(extension))
        || IMAGE_HOST_FRAGMENTS
            .iter()
            .any(|fragment| lower.contains(fragment))
        || lower.contains("/image")
        || lower.contains("/img")
        || lower.contains("generated")
}

/// Converts a plain image URL in model output to Markdown while leaving normal text untouched.
pub(super) fn detect_and_convert_image_urls(text: &str) -> String {
    if text.contains("![") || text.contains("data:image") {
        return text.to_string();
    }

    let lower = text.to_lowercase();
    let mut search_start = 0;
    while let Some(pos) = lower[search_start..].find("http") {
        let abs_pos = search_start + pos;
        let slice = &text[abs_pos..];
        let (scheme, after_scheme) = if let Some(rest) = slice.strip_prefix("https://") {
            ("https://", rest)
        } else if let Some(rest) = slice.strip_prefix("http://") {
            ("http://", rest)
        } else {
            search_start = abs_pos + 4;
            continue;
        };

        let url_end = after_scheme
            .find(|character: char| {
                character.is_whitespace()
                    || matches!(character, '"' | '\'' | '<' | '>' | ')' | ']')
                    || ('\u{4e00}'..='\u{9fff}').contains(&character)
                    || ('\u{3000}'..='\u{303f}').contains(&character)
            })
            .unwrap_or(after_scheme.len());
        let full_url = format!("{scheme}{}", &after_scheme[..url_end]);

        if url_looks_like_image(&full_url) {
            log::info!(
                "[SSE] 检测到文本中内嵌图片 URL，转换为 Markdown: {}...",
                preview_chars(&full_url, 80)
            );
            let before = &text[..abs_pos];
            let after = &text[abs_pos + full_url.len()..];
            return format!("{}\n\n![生成的图片]({})\n\n{}", before, full_url, after);
        }

        search_start = abs_pos + full_url.len().max(1);
    }

    text.to_string()
}

pub(super) fn image_url_to_markdown(url: &str) -> String {
    format!("\n\n![生成的图片]({url})\n\n")
}

/// Downloads a temporary external image so generated content can be persisted locally.
pub(super) async fn download_image_to_base64(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let response = client.get(url).send().await.ok()?;
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let bytes = response.bytes().await.ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{content_type};base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::detect_and_convert_image_urls;

    #[test]
    fn converts_plain_image_urls() {
        assert_eq!(
            detect_and_convert_image_urls("结果：https://example.com/generated.png"),
            "结果：\n\n![生成的图片](https://example.com/generated.png)\n\n"
        );
    }

    #[test]
    fn leaves_non_image_urls_untouched() {
        let text = "文档：https://example.com/reference";
        assert_eq!(detect_and_convert_image_urls(text), text);
    }
}
