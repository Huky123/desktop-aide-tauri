/**
 * AI 错误分类工具
 *
 * 后端 AiError 枚举以中文字符串通过 Tauri 传递到前端。本模块根据错误消息中的
 * 关键词匹配，输出用户可操作的提示。
 *
 * 注意：后端 Display 格式：
 *   Network → "网络连接失败: {msg}"
 *   Auth → "认证失败: {msg}"
 *   RateLimit → "请求频率超限: {msg}"
 *   InvalidModel → "模型无效: {msg}"
 *   Stream → "流式读取错误: {msg}"
 *   ServerError → "服务端错误: {msg}"
 *   Other → "{msg}"（通常是 reqwest 原始错误文本）
 */

interface ClassifiedError {
  /** 用户可读的简短描述 */
  title: string;
  /** 更详细的指引文案 */
  detail: string;
  /** 错误类别，用于日志/遥测 */
  category: "auth" | "network" | "rate_limit" | "model" | "server" | "format" | "stream" | "unknown";
}

/**
 * 后端把上游回显拼进错误文本的分隔符。
 *
 * 来源：`ai_service::error::classify_http_error`（`"{} (详情: {})"`）
 * 与工具参数解析失败（`"…；原始: {}"`）。
 */
const BODY_PREVIEW_MARKERS = [" (详情: ", "；原始: "] as const;

/**
 * 匹配关键词前先切掉服务端回显的 body —— 它只能作为详情展示，不能参与分类。
 *
 * 必要性（真实故障）：网关回 502 时 body 是
 * `{"error":{"message":"Upstream service temporarily unavailable","type":"upstream_error"}}`，
 * 其中 `upstream_error` **字面包含** `stream_error`，正好命中下面那条
 * `/stream.*(error|fail|broken)/`，于是"上游服务不可用"被显示成
 * **"响应中断 / 如频繁出现可尝试切换其他模型"**——把用户引向换模型、查配置，
 * 而真正该做的是等对方服务端恢复或换服务商。
 *
 * 回显是不可控文本，任何关键词匹配都可能被它劫持，所以统一先剥离再匹配。
 */
function stripBodyPreview(message: string): string {
  let cut = message.length;
  for (const marker of BODY_PREVIEW_MARKERS) {
    const index = message.indexOf(marker);
    if (index !== -1 && index < cut) {
      cut = index;
    }
  }
  return message.slice(0, cut);
}

const PATTERNS: Array<{ regex: RegExp; classify: (match: RegExpMatchArray) => ClassifiedError }> = [
  {
    regex: /返回了?空响应|empty response|no content/i,
    classify: () => ({
      title: "未收到回复",
      detail: "AI 服务没有返回内容，请重试或切换模型。",
      category: "unknown",
    }),
  },
  {
    // 认证失败 / 401 / Unauthorized
    regex: /认证失败|401|unauthorized|invalid.*(api.?key|key|token|x-api-key)|auth.*fail|auth.*invalid/i,
    classify: () => ({
      title: "API Key 无效",
      detail: "请检查 API Key 是否正确，或前往对应平台重新生成。",
      category: "auth",
    }),
  },
  {
    // 请求频率超限 / 429
    regex: /请求频率超限|429|rate.?limit|too many requests|quota/i,
    classify: () => ({
      title: "请求太频繁",
      detail: "API 请求频率超限，请稍等片刻后再试。",
      category: "rate_limit",
    }),
  },
  {
    // 模型不在**对话端点**服务（出图模型被当成对话模型时的典型症状）。
    //
    // 必须排在「模型不存在」之前：那句话里含 "model ... not supported"，
    // 会被那一条的 `/model.*(not|invalid|...)/` 抢先命中，于是提示用户
    // "请检查模型名称是否正确"——**而模型名其实是完全正确的**，
    // 用户照做只会白折腾（真实日志里这句话出现了 16 次）。
    regex: /not supported on the chat completions|chat completions endpoint is not|不在对话端点/i,
    classify: () => ({
      title: "模型不在对话接口服务",
      detail:
        "这个模型不通过对话接口提供（出图模型通常如此）。应用会自动改走专用出图接口；若仍失败，说明服务商并未提供该模型。",
      category: "model",
    }),
  },
  {
    // 模型无效 / 404 Not Found
    regex: /模型无效|404|not.?found|model.*(not|invalid|unknown|exist)|no.*model/i,
    classify: () => ({
      title: "模型不存在",
      detail: "请检查模型名称是否正确，或确认你在该平台有对应模型的访问权限。",
      category: "model",
    }),
  },
  {
    // 服务端 5xx（含网关自身的上游故障）。必须排在 stream 之前：
    // 剥离回显后 "服务端错误" 不会再被 body 里的字样带偏。
    regex: /服务端错误|\b50[0-9]\b|bad gateway|service unavailable/i,
    classify: () => ({
      title: "AI 服务端故障",
      detail: "对方服务端（或其上游）暂时不可用，与本机配置和 API Key 无关。请稍后重试，或换用其他服务商。",
      category: "server",
    }),
  },
  {
    // 响应体不是预期格式。后端因历史原因把"非流式解析失败"也归入 Stream 变体，
    // 文案里带 "流式读取错误:" 前缀，所以必须排在 stream 之前，否则会被误报成响应中断。
    regex: /响应解析失败|响应 JSON 解析失败|无法解析流式响应 JSON|不是合法 JSON|expected value|unexpected end of/i,
    classify: () => ({
      title: "响应格式异常",
      detail: "服务端返回的内容不是预期的 JSON，通常是网关或其上游异常。请重试，或换用其他服务商。",
      category: "format",
    }),
  },
  {
    // 网络连接失败（超时、DNS、连接拒绝等）
    regex: /网络连接失败|请求超时|timeout|timed.?out|connect.*(fail|refus|reset|error)|dns|resolve|network|econnrefused|econnreset/i,
    classify: () => ({
      title: "网络连接失败",
      detail: "无法连接到 AI 服务器，请检查网络连接或 API 地址配置。",
      category: "network",
    }),
  },
  {
    // 流式读取错误（真正的中断；SSE 帧解析失败也落在这里）
    regex: /流式读取错误|stream.*(error|fail|broken)|sse.*(error|fail)/i,
    classify: () => ({
      title: "响应中断",
      detail: "AI 响应流意外中断，请重试。如频繁出现，可尝试切换其他模型。",
      category: "stream",
    }),
  },
];

const FALLBACK: ClassifiedError = {
  title: "请求失败",
  detail: "请检查 API Key 和网络配置，或稍后重试。",
  category: "unknown",
};

/** 将后端错误字符串分类为用户可读的错误信息 */
export function classifyAiError(err: unknown): ClassifiedError {
  const raw = typeof err === "string" ? err : String(err ?? "");
  // 先剥离服务端回显，避免不可控文本劫持关键词匹配（见 stripBodyPreview）
  const message = stripBodyPreview(raw);

  for (const pattern of PATTERNS) {
    const match = message.match(pattern.regex);
    if (match) {
      return pattern.classify(match);
    }
  }

  return FALLBACK;
}
