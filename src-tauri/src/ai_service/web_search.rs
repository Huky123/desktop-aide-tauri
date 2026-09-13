//! 联网搜索后端（多级级联，全部免 key 可用）：
//!
//! 顺序：Tavily API（配置 key 时）→ 必应国内版 cn.bing.com（免 key）→ 百度（免 key）
//! → DuckDuckGo HTML（免 key）。某个后端失败（网络不可达/被风控/解析失败）时自动
//! 尝试下一个；全部失败返回汇总原因，便于用户判断是网络问题还是需要 Tavily key。
//!
//! - Tavily 专为 LLM 设计，返回结构化结果（title/url/content），需在 tavily.com 注册免费 key；
//! - 必应/百度对大陆网络直连友好，是免 key 主用兜底；
//! - DuckDuckGo 在部分网络不可达，保留为最后兜底。
//!
//! 说明：三个免 key 后端均为对搜索引擎服务器渲染页面的非官方抓取，可能受页面结构
//! 变化/风控影响；解析失败会记录日志并尝试下一后端，不影响主链路。
//! 本模块只依赖项目已有依赖（reqwest / serde_json / serde / base64），不新增 crate。
//! 网络函数不写单测（离线），解析/编码等纯函数配单测。

use base64::Engine as _;
use serde::Deserialize;

/// 工具默认返回结果条数
pub const DEFAULT_MAX_RESULTS: usize = 5;
/// 单次搜索结果条数上限（防注入过长输出）
pub const MAX_RESULTS_LIMIT: usize = 8;

/// 把模型请求的条数收敛到 `[1, MAX_RESULTS_LIMIT]`。
/// 独立成纯函数以便直接单测（`search()` 本身要走网络）。
fn clamp_result_count(max_results: usize) -> usize {
    max_results.clamp(1, MAX_RESULTS_LIMIT)
}

const TAVILY_URL: &str = "https://api.tavily.com/search";
const BING_URL: &str = "https://cn.bing.com/search";
const BAIDU_URL: &str = "https://www.baidu.com/s";
const DUCKDUCKGO_URL: &str = "https://html.duckduckgo.com/html/";
const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 DesktopAide/0.1"
);
/// 单次请求的超时策略（秒）：
/// - `CONNECT_TIMEOUT`：建连超时；
/// - `READ_TIMEOUT`：**单次读取**超时，避免大响应在慢链路上被"总超时"中途掐断
///   （实测必应 HTML 版约 98 KB，旧的 8 秒总超时会把响应体读断，报
///   `error decoding response body`）；
/// - `TOTAL_TIMEOUT`：兜底总超时，防止请求无限挂住工具循环。
const CONNECT_TIMEOUT_SECS: u64 = 5;
const READ_TIMEOUT_SECS: u64 = 10;
const TOTAL_TIMEOUT_SECS: u64 = 30;

/// 联网搜索运行配置（由命令层按开关从 AppConfig 提取后注入工具上下文）
#[derive(Debug, Clone, Default)]
pub struct SearchConfig {
    /// Tavily API Key；为空时跳过 Tavily，直接走免 key 后端
    pub tavily_api_key: String,
}

/// 单条搜索结果
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    /// 内容摘要（可能为空）
    pub snippet: String,
}

/// 一次搜索的整体结果（backend 标记数据来源，供工具输出与日志使用）
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    pub backend: &'static str,
    pub query: String,
    pub hits: Vec<SearchHit>,
}

/// 执行联网搜索：按 Tavily → 必应 → 百度 → DuckDuckGo 顺序尝试，
/// 返回第一个成功的后端结果；全部失败时给出汇总原因。
pub async fn search(
    query: &str,
    max_results: usize,
    cfg: &SearchConfig,
) -> Result<SearchOutcome, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("搜索关键词不能为空".to_string());
    }
    let n = clamp_result_count(max_results);
    let mut failures: Vec<String> = Vec::new();

    let key = cfg.tavily_api_key.trim();
    if !key.is_empty() {
        match search_tavily(query, n, key).await {
            Ok(outcome) => return Ok(outcome),
            Err(e) => {
                log::warn!("[web_search] Tavily 失败: {e}");
                failures.push(format!("Tavily: {e}"));
            }
        }
    }

    match search_bing(query, n).await {
        Ok(outcome) => return Ok(outcome),
        Err(e) => {
            log::warn!("[web_search] 必应失败: {e}");
            failures.push(format!("必应: {e}"));
        }
    }

    match search_baidu(query, n).await {
        Ok(outcome) => return Ok(outcome),
        Err(e) => {
            log::warn!("[web_search] 百度失败: {e}");
            failures.push(format!("百度: {e}"));
        }
    }

    match search_duckduckgo(query, n).await {
        Ok(outcome) => Ok(outcome),
        Err(e) => {
            log::warn!("[web_search] DuckDuckGo 失败: {e}");
            failures.push(format!("DuckDuckGo: {e}"));
            Err(format!(
                "联网搜索失败：已依次尝试 Tavily/必应/百度/DuckDuckGo 均未成功。{}",
                failures.join("；")
            ))
        }
    }
}

/// 将搜索结果格式化为给模型阅读的文本（工具输出）
pub fn format_search_outcome(outcome: &SearchOutcome) -> String {
    if outcome.hits.is_empty() {
        return format!(
            "联网搜索完成（来源: {}），没有找到与「{}」相关的网页结果。",
            outcome.backend, outcome.query
        );
    }
    let mut text = format!(
        "联网搜索结果（来源: {}，查询: {}）共 {} 条：",
        outcome.backend,
        outcome.query,
        outcome.hits.len()
    );
    for (i, hit) in outcome.hits.iter().enumerate() {
        text.push_str(&format!(
            "\n{}. {}\n   链接: {}\n   摘要: {}",
            i + 1,
            hit.title,
            hit.url,
            hit.snippet
        ));
    }
    text
}

/// 抓取型后端的公共检查：HTTP 非 2xx 时按失败处理（换下一后端）。
async fn fetch_html(url: &str, extra_headers: bool) -> Result<String, String> {
    let client = build_client()?;
    let mut req = client.get(url);
    if extra_headers {
        req = req.header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8");
    }
    let resp = req.send().await.map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(body)
}

/// 抓取文本，**瞬时网络错误自动重试一次**。
///
/// 这条链路上"连接被掐断/响应体读不全"是常态（表现为 `error sending request` /
/// `error decoding response body`），重试一次的收益很高；而 HTTP 4xx/5xx 与
/// "解析为空"（风控页/结构变化）重试无用，直接失败换下一个后端。
async fn fetch_text_with_retry(url: &str, extra_headers: bool) -> Result<String, String> {
    match fetch_html(url, extra_headers).await {
        Ok(body) => Ok(body),
        Err(err) if is_transient_error(&err) => {
            log::info!("[web_search] 瞬时错误，重试一次: {err}");
            fetch_html(url, extra_headers).await
        }
        Err(err) => Err(err),
    }
}

/// 是否属于值得重试的瞬时网络错误
fn is_transient_error(err: &str) -> bool {
    const MARKERS: [&str; 6] = [
        "请求失败",
        "读取响应失败",
        "error decoding response body",
        "error sending request",
        "timed out",
        "connection",
    ];
    MARKERS.iter().any(|marker| err.contains(marker))
}

/// 单行预览：把换行/制表符压平，避免多行 HTML 把日志文件撑乱
fn flatten_preview(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .take(max_chars)
        .collect()
}

/// 解析结果为空的错误（含单行页面片段，便于排查风控/结构变化）
fn empty_parse_error(backend: &str, body: &str) -> String {
    format!(
        "{backend} 未解析到结果，页面片段: {}",
        flatten_preview(body, 300)
    )
}

// ───────────────────────── Tavily（API）─────────────────────────

#[derive(Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

async fn search_tavily(
    query: &str,
    max_results: usize,
    api_key: &str,
) -> Result<SearchOutcome, String> {
    let client = build_client()?;
    let payload = serde_json::json!({
        "api_key": api_key,
        "query": query,
        "max_results": max_results,
        "search_depth": "basic",
        "include_answer": false,
    });
    let resp = client
        .post(TAVILY_URL)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Tavily 请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 Tavily 响应失败: {e}"))?;
    if !status.is_success() {
        let preview = flatten_preview(&body, 300);
        return Err(format!("HTTP {status}: {preview}"));
    }
    let parsed: TavilyResponse =
        serde_json::from_str(&body).map_err(|e| format!("解析 Tavily 响应失败: {e}"))?;
    let hits: Vec<SearchHit> = parsed
        .results
        .into_iter()
        .map(|r| SearchHit {
            title: clamp_chars(html_unescape(&r.title), 200),
            url: r.url.trim().to_string(),
            snippet: clamp_chars(html_unescape(&r.content), 500),
        })
        .filter(|h| !h.url.is_empty())
        .collect();
    Ok(SearchOutcome {
        backend: "Tavily",
        query: query.to_string(),
        hits,
    })
}

// ─────────────────── 必应（cn.bing.com，免 key）───────────────────

/// 必应搜索：**优先 RSS 端点**（`format=rss`），失败再回退 HTML 版。
///
/// 为什么优先 RSS：HTML 版约 98 KB，慢链路上会被读取超时截断（报
/// `error decoding response body`）；RSS 版仅约 4 KB、无压缩、`<item>` 结构稳定，
/// 解析也更简单（实测返回 `text/xml; charset=utf-8`）。
async fn search_bing(query: &str, max_results: usize) -> Result<SearchOutcome, String> {
    let rss_url = format!("{BING_URL}?q={}&format=rss", encode_query(query));
    match fetch_text_with_retry(&rss_url, true).await {
        Ok(body) => {
            let hits = parse_bing_rss(&body, max_results);
            if !hits.is_empty() {
                return Ok(SearchOutcome {
                    backend: "必应",
                    query: query.to_string(),
                    hits,
                });
            }
            log::warn!(
                "[web_search] 必应 RSS 未解析到结果，回退 HTML 版: {}",
                empty_parse_error("必应(RSS)", &body)
            );
        }
        Err(e) => log::warn!("[web_search] 必应 RSS 失败，回退 HTML 版: {e}"),
    }

    // 回退：HTML 版（结构变化/风控时会失败，属预期）
    let html_url = format!("{BING_URL}?q={}&ensearch=0", encode_query(query));
    let body = fetch_text_with_retry(&html_url, true).await?;
    let hits = parse_bing_html(&body, max_results);
    if hits.is_empty() {
        return Err(empty_parse_error("必应", &body));
    }
    Ok(SearchOutcome {
        backend: "必应",
        query: query.to_string(),
        hits,
    })
}

/// 解析必应 RSS 结果：`<item>` 内的 title / link / description。
///
/// 样例：
/// ```xml
/// <item><title>标题</title><link>https://example.com/a</link>
/// <description>摘要</description><pubDate>…</pubDate></item>
/// ```
fn parse_bing_rss(xml: &str, max: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut rest = xml;

    while hits.len() < max {
        let Some(start) = rest.find("<item>") else {
            break;
        };
        let after = &rest[start + "<item>".len()..];
        let Some(end) = after.find("</item>") else {
            break;
        };
        let item = &after[..end];
        rest = &after[end + "</item>".len()..];

        let title = extract_xml_tag(item, "title")
            .map(|t| clean_text(&t))
            .unwrap_or_default();
        let link = extract_xml_tag(item, "link")
            .map(|l| html_unescape(l.trim()))
            .unwrap_or_default();
        let snippet = extract_xml_tag(item, "description")
            .map(|d| clean_text(&d))
            .unwrap_or_default();

        if link.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            title: clamp_chars(title, 200),
            url: link,
            snippet: clamp_chars(snippet, 500),
        });
    }
    hits
}

/// 取 `<tag>…</tag>` 之间的文本（支持 CDATA；不处理同名嵌套标签）
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let raw = xml[start..end].trim();
    let inner = raw
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
        .unwrap_or(raw);
    Some(html_unescape(inner))
}

/// 解析必应结果页：`<li class="b_algo">` 中取 h2 内链接 + 首个 <p> 摘要。
fn parse_bing_html(html: &str, max: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut rest = html;

    while hits.len() < max {
        let Some(marker) = rest.find("class=\"b_algo\"") else {
            break;
        };
        // 该条结果的范围：当前 b_algo 到下一个 b_algo
        let after_marker = &rest[marker + "class=\"b_algo\"".len()..];
        let next = after_marker
            .find("class=\"b_algo\"")
            .unwrap_or(after_marker.len());
        let segment = &after_marker[..next];
        rest = &after_marker[next..];

        // 标题锚点：<h2> 内的第一个 <a>
        let Some(h2_rel) = segment.find("<h2") else {
            continue;
        };
        let after_h2 = &segment[h2_rel..];
        let Some(a_rel) = after_h2.find("<a") else {
            continue;
        };
        let a_start = a_rel;
        let Some(open_rel_end) = after_h2[a_start..].find('>') else {
            continue;
        };
        let open_end = a_start + open_rel_end;
        let opening = &after_h2[a_start..=open_end];
        let Some(close_rel) = after_h2[open_end + 1..].find("</a>") else {
            continue;
        };
        let close_end = open_end + 1 + close_rel;
        let title = clean_text(&after_h2[open_end + 1..close_end]);

        let url = decode_link_target(opening);
        // 摘要：标题 </a> 之后的第一个 <p>…</p>
        let snippet = extract_first_p_text(&after_h2[close_end + 4..]);

        if url.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            title: clamp_chars(title, 200),
            url,
            snippet: clamp_chars(snippet, 500),
        });
    }
    hits
}

// ─────────────────── 百度（www.baidu.com，免 key）───────────────────

async fn search_baidu(query: &str, max_results: usize) -> Result<SearchOutcome, String> {
    let url = format!("{BAIDU_URL}?wd={}", encode_query(query));
    let body = fetch_text_with_retry(&url, true).await?;
    let hits = parse_baidu_html(&body, max_results);
    if hits.is_empty() {
        return Err(empty_parse_error("百度", &body));
    }
    Ok(SearchOutcome {
        backend: "百度",
        query: query.to_string(),
        hits,
    })
}

/// 解析百度结果页：每个自然结果 <h3 …> 内链接，摘要取紧随的 c-abstract / content-right 文本。
/// 百度的结果区结构历史上有多次改版，这里按「连续 h3 结果块」+ 常见摘要容器尽力提取；
/// 摘要拿不到时仅保留标题与链接（模型仍可基于标题作答）。
fn parse_baidu_html(html: &str, max: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut rest = html;

    while hits.len() < max {
        // 找到下一个 h3 块，作为潜在结果起点
        let Some(h3_rel) = rest.find("<h3") else {
            break;
        };
        let seg = &rest[h3_rel..];
        // 跳过明显非结果的 h3（如页脚相关推荐通常无 class="t"），仅看紧跟 <a>
        let Some(a_rel) = seg.find("<a") else { break };
        let Some(open_rel_end) = seg[a_rel..].find('>') else {
            break;
        };
        let open_end = a_rel + open_rel_end;
        let opening = &seg[a_rel..=open_end];
        let Some(close_rel) = seg[open_end + 1..].find("</a>") else {
            break;
        };
        let close_end = open_end + 1 + close_rel;
        let title = clean_text(&seg[open_end + 1..close_end]);
        if title.is_empty() {
            // 非结果 h3（如视频/相关推荐），跳过该 h3 继续找
            rest = &rest[h3_rel + 3..];
            continue;
        }
        let url = decode_link_target(opening);

        // 摘要：优先常见摘要容器；取该 h3 之后到下一个 h3 前的片段
        let block_end = seg[close_end + 4..]
            .find("<h3")
            .map(|d| close_end + 4 + d)
            .unwrap_or(seg.len());
        let block = &seg[close_end + 4..block_end.min(seg.len())];
        let snippet = extract_baidu_snippet(block);

        rest = &rest[h3_rel + 3..];
        if url.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            title: clamp_chars(title, 200),
            url,
            snippet: clamp_chars(snippet, 500),
        });
    }
    hits
}

/// 在结果块中寻找百度摘要容器文本：`c-abstract` / `content-right` 等
fn extract_baidu_snippet(block: &str) -> String {
    for marker in [
        "c-abstract",
        "content-right",
        "c-span-last",
        "cos-text-color",
    ] {
        if let Some(pos) = block.find(marker) {
            let Some(seg_start) = block[..pos].rfind('<') else {
                continue;
            };
            let seg = &block[seg_start..];
            let Some(open_rel_end) = seg.find('>') else {
                continue;
            };
            let open_end = open_rel_end;
            // 容器多为 <div>/<span>，取到第一个闭合标签为止（嵌套 div 时内容可能偏短，
            // 作为摘要够用）
            let mut end = 0usize;
            if let Some(rel) = seg[open_end + 1..].find("</div>") {
                end = open_end + 1 + rel;
            }
            if let Some(rel) = seg[open_end + 1..].find("</span>") {
                let cand = open_end + 1 + rel;
                if end == 0 || cand < end {
                    end = cand;
                }
            }
            if end > 0 {
                let text = clean_text(&seg[open_end + 1..end]);
                if !text.is_empty() {
                    return text;
                }
            }
        }
    }
    String::new()
}

// ─────────────────── DuckDuckGo（免 key，最后兜底）───────────────────

async fn search_duckduckgo(query: &str, max_results: usize) -> Result<SearchOutcome, String> {
    let url = format!("{DUCKDUCKGO_URL}?q={}", encode_query(query));
    let body = fetch_text_with_retry(&url, true).await?;
    let hits = parse_ddg_html(&body, max_results);
    if hits.is_empty() {
        return Err(empty_parse_error("DuckDuckGo", &body));
    }
    Ok(SearchOutcome {
        backend: "DuckDuckGo",
        query: query.to_string(),
        hits,
    })
}

/// 从 DuckDuckGo html 版页面提取结果（不引入 HTML 解析依赖，基于稳定标记扫描）。
///
/// 每条的标记结构（服务器渲染，属性顺序可能变化）：
/// ```html
/// <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=<url>&rut=...">标题</a>
/// <a class="result__snippet" href="...">摘要</a>
/// ```
fn parse_ddg_html(html: &str, max: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut rest = html;

    while hits.len() < max {
        let Some(title_mark) = find_class_marker(rest, "result__a") else {
            break;
        };
        // 捕获该 <a> 开标签（回退到最近的 '<'，避免截到属性中间）
        let Some(tag_start) = rest[..title_mark].rfind('<') else {
            break;
        };
        let Some(tag_relative_end) = rest[title_mark..].find('>') else {
            break;
        };
        let tag_end = title_mark + tag_relative_end;
        let opening = &rest[tag_start..=tag_end];
        // 标题文本：开标签之后到 </a>
        let Some(title_close) = rest[tag_end + 1..].find("</a>") else {
            break;
        };
        let title_end = tag_end + 1 + title_close;
        let title = clean_text(&rest[tag_end + 1..title_end]);

        // 摘要在标题 </a> 之后继续找 result__snippet
        let snippet = extract_snippet(&rest[title_end..]);

        let url = decode_href(opening);
        // 前进：从标题 </a> 之后继续（snippet 位于其间，跳过不影响后续扫描）
        rest = &rest[title_end + 4..];

        if url.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            title: clamp_chars(title, 200),
            url,
            snippet: clamp_chars(snippet, 500),
        });
    }
    hits
}

/// 提取标题锚点之后第一个 `result__snippet` 的文本（无则返回空串）
fn extract_snippet(after_title: &str) -> String {
    let Some(snip_mark) = find_class_marker(after_title, "result__snippet") else {
        return String::new();
    };
    let Some(snip_rel_end) = after_title[snip_mark..].find('>') else {
        return String::new();
    };
    let snip_tag_end = snip_mark + snip_rel_end;
    let Some(snip_close_rel) = after_title[snip_tag_end + 1..].find("</a>") else {
        return String::new();
    };
    let snip_end = snip_tag_end + 1 + snip_close_rel;
    clean_text(&after_title[snip_tag_end + 1..snip_end])
}

// ─────────────────── 链接 / 文本处理公共函数 ───────────────────

/// 从 <a …> 开标签中解析目标 URL。
/// - 常规 href 直接返回（HTML 反转义 + percent 解码）；
/// - DuckDuckGo `//duckduckgo.com/l/?uddg=…&rut=…` 取 uddg 参数解码；
/// - 必应 `bing.com/ck/a?…&u=…` 尝试 base64 解码真实地址（失败则原样返回）。
fn decode_link_target(opening_tag: &str) -> String {
    let Some(start) = opening_tag.find("href=\"") else {
        return String::new();
    };
    let value_start = start + "href=\"".len();
    let Some(rel_end) = opening_tag[value_start..].find('"') else {
        return String::new();
    };
    let value_end = value_start + rel_end;
    let raw = html_unescape(&opening_tag[value_start..value_end]);
    if raw.is_empty() {
        return String::new();
    }

    // DuckDuckGo 跳转
    if raw.contains("duckduckgo.com/l/") {
        if let Some(pos) = raw.find("uddg=") {
            let uddg_start = pos + "uddg=".len();
            let uddg_end = raw[uddg_start..]
                .find('&')
                .map(|d| uddg_start + d)
                .unwrap_or(raw.len());
            return percent_decode(&raw[uddg_start..uddg_end]);
        }
    }
    // 必应 /ck/a 跳转
    if raw.contains("bing.com/ck/a") {
        if let Some(decoded) = decode_bing_target(&raw) {
            return decoded;
        }
        return raw;
    }
    percent_decode(&raw)
}

/// 兼容旧 DDG 解析（保留，供 parse_ddg_html 使用）
fn decode_href(opening_tag: &str) -> String {
    decode_link_target(opening_tag)
}

/// 解码必应跳转参数：`u=a1<base64url(URL)>`（历史上也可能是 `u=a1…` 变体）
fn decode_bing_target(raw: &str) -> Option<String> {
    let u_start = raw.find("u=")? + 2;
    let u_end = raw[u_start..]
        .find('&')
        .map(|d| u_start + d)
        .unwrap_or(raw.len());
    let mut encoded = &raw[u_start..u_end];
    if let Some(stripped) = encoded.strip_prefix("a1") {
        encoded = stripped;
    }
    if encoded.len() > 4096 {
        return None;
    }
    let decoded = [
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded),
        base64::engine::general_purpose::STANDARD_NO_PAD.decode(encoded),
    ]
    .into_iter()
    .find_map(Result::ok)?;
    let text = String::from_utf8(decoded).ok()?;
    if text.starts_with("http://") || text.starts_with("https://") {
        Some(text)
    } else {
        None
    }
}

/// 提取标题锚点之后第一个 `<p>…</p>` 的文本（无则返回空串）
fn extract_first_p_text(after_title: &str) -> String {
    let Some(p_rel) = after_title.find("<p") else {
        return String::new();
    };
    let Some(open_rel_end) = after_title[p_rel..].find('>') else {
        return String::new();
    };
    let open_end = p_rel + open_rel_end;
    let Some(close_rel) = after_title[open_end + 1..].find("</p>") else {
        return String::new();
    };
    let close_end = open_end + 1 + close_rel;
    clean_text(&after_title[open_end + 1..close_end])
}

/// 提取文本内容：剥掉残留标签并做 HTML 反转义、空白整理
fn clean_text(raw: &str) -> String {
    let stripped = strip_tags(raw);
    let unescaped = html_unescape(&stripped);
    unescaped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 简单剥除 <...> 标签（非解析器；对服务器输出的结构足够）
fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// 常见 HTML 实体 + 数字实体反转义（&amp; &lt; &gt; &quot; &#39; &#x27; &nbsp; 等）
fn html_unescape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let Some(semi) = tail.find(';') else {
            // 没有分号的裸 `&`（搜索结果 URL 里的 query string 极常见）：
            // 原样保留剩余内容并直接返回。注意不能 `break` —— 循环外还有一次
            // `out.push_str(rest)`，会把这段内容重复拼接（曾导致链接被拼成两遍）。
            out.push('&');
            out.push_str(tail);
            return out;
        };
        let entity = &tail[..semi];
        let replacement = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" | "#x27" => Some('\''),
            "nbsp" => Some(' '),
            _ => {
                // 数字实体 &#123; / &#x1F;
                if let Some(digits) = entity.strip_prefix('#') {
                    let radix = if let Some(hex) = digits.strip_prefix('x') {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        digits.parse::<u32>().ok()
                    };
                    radix.and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match replacement {
            Some(ch) => {
                out.push(ch);
                rest = &tail[semi + 1..];
            }
            None => {
                // 未识别的实体原样保留
                out.push('&');
                out.push_str(entity);
                out.push(';');
                rest = &tail[semi + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 百分比解码（UTF-8；'+' 原样保留，各搜索引擎的跳转参数中空格以 %20 编码）
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                decoded.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 查询串编码：非保留字符外全部 %XX（UTF-8），空格用 %20
fn encode_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 在片段中定位 `class="xxx"` 标记
fn find_class_marker(haystack: &str, class: &str) -> Option<usize> {
    haystack.find(&format!("class=\"{class}\""))
}

fn clamp_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut clipped: String = s.chars().take(max).collect();
    clipped.push('…');
    clipped
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        // 单次读取超时：大响应在慢链路上不再被"总超时"中途掐断
        .read_timeout(std::time::Duration::from_secs(READ_TIMEOUT_SECS))
        // 兜底总超时：防止请求无限挂住工具循环
        .timeout(std::time::Duration::from_secs(TOTAL_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_with_percent_encoding() {
        assert_eq!(
            encode_query("天气 北京"),
            "%E5%A4%A9%E6%B0%94%20%E5%8C%97%E4%BA%AC"
        );
        assert_eq!(encode_query("a-b_c.d~z1"), "a-b_c.d~z1");
    }

    #[test]
    fn decodes_percent_encoded_utf8() {
        assert_eq!(percent_decode("%E5%A4%A9%E6%B0%94"), "天气");
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a+b"), "a+b"); // '+' 保留
    }

    #[test]
    fn unescapes_common_html_entities() {
        assert_eq!(
            html_unescape("a&amp;b &lt;x&gt; &quot;q&quot; &#39;ap&#39; &#x27;x&#x27;"),
            "a&b <x> \"q\" 'ap' 'x'"
        );
        assert_eq!(html_unescape("&#20013;&#x6587;"), "中文");
        assert_eq!(
            html_unescape("no entity &unknown; here"),
            "no entity &unknown; here"
        );
    }

    #[test]
    fn strips_tags_and_collapses_whitespace() {
        assert_eq!(clean_text("  Hello  <b>World</b> !  "), "Hello World !");
    }

    #[test]
    fn parses_duckduckgo_html_sample() {
        let html = r#"
            <div class="result results_links results_links_deep web-result ">
                <h2 class="result__title">
                    <a rel="nofollow" class="result__a"
                       href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fnews%3Fid%3D1%26x%3Dy&amp;rut=abc123">
                       北京今天 <b>天气</b> 晴朗
                    </a>
                </h2>
                <a class="result__snippet" href="//duckduckgo.com/l/?uddg=...">最高 30 度，适合出行&nbsp;。</a>
            </div>
            <div class="result results_links results_links_deep web-result ">
                <h2 class="result__title">
                    <a rel="nofollow" class="result__a" href="https://plain.example.org/page">
                       第二条结果
                    </a>
                </h2>
                <a class="result__snippet" href="https://plain.example.org/page">没有摘要以外的内容</a>
            </div>
        "#;
        let hits = parse_ddg_html(html, 5);
        assert_eq!(hits.len(), 2, "应解析出两条结果: {hits:?}");
        assert_eq!(hits[0].title, "北京今天 天气 晴朗");
        assert_eq!(hits[0].url, "https://example.com/news?id=1&x=y");
        assert_eq!(hits[0].snippet, "最高 30 度，适合出行 。");
        assert_eq!(hits[1].url, "https://plain.example.org/page");
    }

    #[test]
    fn parses_with_max_limit() {
        let mut html = String::new();
        for i in 0..6 {
            html.push_str(&format!(
                r#"<a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fe{i}.com">t{i}</a>"#
            ));
            html.push_str(r#"<a class="result__snippet" href="x">s</a>"#);
        }
        assert_eq!(parse_ddg_html(&html, 3).len(), 3);
        assert_eq!(parse_ddg_html(&html, 10).len(), 6);
    }

    /// 诊断用（需要网络；默认忽略）。用于区分必应抓取失败的三种原因：
    /// 压缩解码失败 / 字符集问题 / 不可达。
    /// 运行：`cargo test --lib diag_bing_response -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn diag_bing_response() {
        let client = build_client().expect("构建客户端");
        let url = format!("{BING_URL}?q=test&ensearch=0");
        let resp = client.get(&url).send().await.expect("请求应发出");
        println!("[diag] status = {}", resp.status());
        for (name, value) in resp.headers() {
            println!("[diag] header {name}: {value:?}");
        }
        match resp.text().await {
            Ok(text) => println!("[diag] text() ok, len = {}", text.len()),
            Err(e) => println!(
                "[diag] text() err = {e}; debug = {e:?}; source = {:?}",
                std::error::Error::source(&e)
            ),
        }
    }

    /// 诊断用（需要网络；默认忽略）：端到端跑一次必应 RSS 搜索，打印命中条目。
    /// 运行：`cargo test --lib diag_bing_rss_search -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn diag_bing_rss_search() {
        match search_bing("deepseek v4.1 发布", 3).await {
            Ok(outcome) => {
                println!(
                    "[diag] backend = {} hits = {}",
                    outcome.backend,
                    outcome.hits.len()
                );
                for hit in &outcome.hits {
                    println!("[diag] - {} | {}", hit.title, hit.url);
                }
            }
            Err(e) => println!("[diag] 搜索失败: {e}"),
        }
    }

    #[test]
    fn parses_bing_rss_sample() {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?><rss version="2.0"><channel>
            <title>必应：测试</title>
            <item><title>DeepSeek V4.1 Flash 发布</title><link>https://www.deepseek.com/news/v4-1/?a=1&amp;b=2</link><description>原生多模态视觉理解能力，能力更强。</description><pubDate>周四, 10 9月 2026 22:59:00 GMT</pubDate></item>
            <item><title><![CDATA[带 CDATA 的 <b>标题</b>]]></title><link>https://example.com/cdata</link><description><![CDATA[<p>CDATA 摘要 &amp; 实体</p>]]></description></item>
            <item><title>缺少链接的条目</title><description>应被跳过</description></item>
        </channel></rss>"#;
        let hits = parse_bing_rss(xml, 5);
        assert_eq!(hits.len(), 2, "应解析出 2 条（缺 link 的跳过）: {hits:?}");
        assert_eq!(hits[0].title, "DeepSeek V4.1 Flash 发布");
        assert_eq!(hits[0].url, "https://www.deepseek.com/news/v4-1/?a=1&b=2");
        assert!(
            hits[0].snippet.contains("原生多模态"),
            "{}",
            hits[0].snippet
        );
        assert_eq!(hits[1].title, "带 CDATA 的 标题");
        assert_eq!(hits[1].url, "https://example.com/cdata");
        assert!(
            hits[1].snippet.contains("CDATA 摘要 & 实体"),
            "{}",
            hits[1].snippet
        );
    }

    #[test]
    fn bing_rss_respects_max_and_empty_input() {
        let xml = "<rss><channel><item><title>a</title><link>https://a.com</link></item>\
                   <item><title>b</title><link>https://b.com</link></item></channel></rss>";
        assert_eq!(parse_bing_rss(xml, 1).len(), 1);
        assert!(parse_bing_rss("<html>no items</html>", 5).is_empty());
    }

    #[test]
    fn transient_errors_are_retryable() {
        assert!(is_transient_error(
            "请求失败: error sending request for url (x)"
        ));
        assert!(is_transient_error(
            "读取响应失败: error decoding response body"
        ));
        assert!(is_transient_error("请求失败: operation timed out"));
        // 非瞬时错误不重试
        assert!(!is_transient_error("HTTP 403 Forbidden"));
        assert!(!is_transient_error(
            "百度 未解析到结果，页面片段: <!DOCTYPE html>"
        ));
    }

    #[test]
    fn flatten_preview_removes_newlines() {
        let flat = flatten_preview("<html>\n  <head>\r\n    <meta>\t</head>", 100);
        assert!(
            !flat.contains('\n') && !flat.contains('\r') && !flat.contains('\t'),
            "{flat}"
        );
        assert_eq!(flatten_preview("abcdef", 3), "abc");
    }

    #[test]
    fn parses_bing_html_sample() {
        let html = r#"
            <ol id="b_results">
              <li class="b_algo">
                <h2><a href="https://www.example.org/ai-news?from=bing" h="ID=SERP,5000.1">AI 最新进展 报道</a></h2>
                <div class="b_caption"><p><strong>摘要</strong> 中的内容，今天发布。</p></div>
              </li>
              <li class="b_algo">
                <h2><a href="https://www.bing.com/ck/a?!&&p=1&u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9wYWdl&ntb=1">跳转链接标题</a></h2>
                <p>第二条摘要。</p>
              </li>
            </ol>
        "#;
        let hits = parse_bing_html(html, 5);
        assert_eq!(hits.len(), 2, "应解析出两条结果: {hits:?}");
        assert_eq!(hits[0].title, "AI 最新进展 报道");
        assert_eq!(hits[0].url, "https://www.example.org/ai-news?from=bing");
        assert!(
            hits[0].snippet.contains("今天发布"),
            "摘要: {}",
            hits[0].snippet
        );
        // bing ck/a 跳转的 base64 URL 应被解出
        assert_eq!(hits[1].url, "https://example.com/page");
    }

    #[test]
    fn parses_baidu_html_sample() {
        let html = r#"
            <div id="content_left">
              <div class="result c-container new-pmd" id="1">
                <h3 class="t"><a href="https://baike.baidu.com/item/x" target="_blank">百度百科词条标题</a></h3>
                <div class="c-abstract">这是摘要内容，包含相关信息。</div>
              </div>
              <div class="result c-container" id="2">
                <h3 class="t"><a href="https://news.example.com/2">第二条新闻标题</a></h3>
                <div><span class="content-right_8Zs40">第二条摘要文字。</span></div>
              </div>
              <h3>非结果标题（无链接或导航）</h3>
            </div>
        "#;
        let hits = parse_baidu_html(html, 5);
        assert_eq!(hits.len(), 2, "应解析出两条结果: {hits:?}");
        assert_eq!(hits[0].title, "百度百科词条标题");
        assert!(
            hits[0].snippet.contains("摘要内容"),
            "摘要: {}",
            hits[0].snippet
        );
        assert_eq!(hits[1].title, "第二条新闻标题");
        assert!(
            hits[1].snippet.contains("第二条摘要"),
            "摘要: {}",
            hits[1].snippet
        );
    }

    #[test]
    fn decodes_bing_base64_target() {
        // base64("https://example.com/page")
        let raw = "https://www.bing.com/ck/a?!&&p=abc&u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9wYWdl&ntb=1";
        assert_eq!(
            decode_bing_target(raw).as_deref(),
            Some("https://example.com/page")
        );
        assert_eq!(decode_bing_target("https://www.example.org/direct"), None);
    }

    #[test]
    fn rejects_empty_query() {
        let cfg = SearchConfig::default();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let err = rt
            .block_on(search("   ", 5, &cfg))
            .expect_err("空关键词应报错");
        assert!(err.contains("空"), "{err}");
    }

    #[test]
    fn clamps_result_count_range() {
        assert_eq!(clamp_result_count(0), 1, "下界收敛到 1");
        assert_eq!(clamp_result_count(3), 3, "区间内原样返回");
        assert_eq!(
            clamp_result_count(usize::MAX),
            MAX_RESULTS_LIMIT,
            "上界收敛到 MAX_RESULTS_LIMIT"
        );
    }

    #[test]
    fn formats_search_outcome() {
        // 空结果格式化
        let out = SearchOutcome {
            backend: "Tavily",
            query: "q".into(),
            hits: vec![],
        };
        assert!(format_search_outcome(&out).contains("没有找到"));
        // 非空结果格式化
        let out = SearchOutcome {
            backend: "百度",
            query: "q".into(),
            hits: vec![SearchHit {
                title: "标题".into(),
                url: "https://x.com".into(),
                snippet: "摘要".into(),
            }],
        };
        let text = format_search_outcome(&out);
        assert!(text.contains("来源: 百度"));
        assert!(text.contains("https://x.com"));
    }
}
