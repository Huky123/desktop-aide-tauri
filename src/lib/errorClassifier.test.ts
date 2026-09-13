import { describe, it, expect } from "vitest";
import { classifyAiError } from "./errorClassifier";

/**
 * 构造后端 `classify_http_error` 的真实输出格式：
 *   detail = "{hint} (详情: {body})"
 *   409 的 Display 前缀见各用例；`Other` 分支是 "HTTP {status}: {body}"。
 */
function serverError(body: string) {
  return `服务端错误: AI 服务端错误，请稍后重试 (详情: ${body})`;
}
function authError(body: string) {
  return `认证失败: API Key 无效或已过期，请检查设置 (详情: ${body})`;
}
function modelError(model: string, body: string) {
  return `模型无效: 模型 '${model}' 未找到，请确认模型名称是否正确 (详情: ${body})`;
}
function rateLimitError(body: string) {
  return `请求频率超限: 请求频率超限，请稍后重试 (详情: ${body})`;
}
function otherError(status: number, body: string) {
  return `HTTP ${status}: ${body}`;
}

describe("classifyAiError — 服务端回显不得劫持分类", () => {
  /**
   * 真实故障回归：网关回 502 时 body 里是 `"type":"upstream_error"`，
   * 其中 `upstream_error` 字面包含 `stream_error`，曾被匹配成"响应中断"。
   */
  it("502 的 body 含 upstream_error 时判为服务端故障，而不是响应中断", () => {
    const result = classifyAiError(
      serverError(
        '{"error":{"message":"Upstream service temporarily unavailable","type":"upstream_error"}}'
      )
    );
    expect(result.category).toBe("server");
    expect(result.title).toBe("AI 服务端故障");
    expect(result.title).not.toBe("响应中断");
  });

  it("body 里塞满各类关键词也一律不影响分类", () => {
    // 这几个词分别对应 auth / model / rate_limit / stream 四条规则
    const result = classifyAiError(
      serverError(
        '{"error":{"message":"stream error: 404 not found, unauthorized, invalid api key, rate limit exceeded"}}'
      )
    );
    expect(result.category).toBe("server");
  });

  it("工具参数解析失败时，`；原始: ` 之后的原始请求也不参与匹配", () => {
    const result = classifyAiError(
      "流式读取错误: tool_calls[0] arguments 不是合法 JSON: expected value；原始: {not json, stream error, 404}"
    );
    expect(result.category).toBe("format");
  });
});

describe("classifyAiError — 响应格式异常不再被误报为响应中断", () => {
  it("出图响应解析失败", () => {
    const result = classifyAiError("流式读取错误: 图片生成响应解析失败: error decoding response body");
    expect(result.category).toBe("format");
    expect(result.title).toBe("响应格式异常");
  });

  it("非流式响应 JSON 解析失败", () => {
    const result = classifyAiError(
      "流式读取错误: 非流式响应 JSON 解析失败: expected value at line 1 column 1"
    );
    expect(result.category).toBe("format");
  });

  it("流式响应不是合法 JSON", () => {
    const result = classifyAiError(
      "流式读取错误: 无法解析流式响应 JSON: expected ident at line 1 column 2；数据: <html>"
    );
    expect(result.category).toBe("format");
  });
});

describe("classifyAiError — 真正的中断仍报响应中断", () => {
  it("SSE 解析失败", () => {
    const result = classifyAiError("流式读取错误: SSE 解析失败: error decoding response body");
    expect(result.category).toBe("stream");
    expect(result.title).toBe("响应中断");
  });

  it("传输层中断", () => {
    const result = classifyAiError("流式读取错误: stream closed unexpectedly");
    expect(result.category).toBe("stream");
  });
});

describe("classifyAiError — 常见状态码", () => {
  it("401 认证失败", () => {
    expect(classifyAiError(authError('{"error":{"message":"invalid api key"}}')).category).toBe("auth");
  });

  it("404 模型不存在", () => {
    expect(
      classifyAiError(
        modelError(
          "grok-4.5",
          '{"error":{"message":"Model \\"grok-4.5\\" is not supported by any configured account in this group","type":"model_not_found"}}'
        )
      ).category
    ).toBe("model");
  });

  it("429 请求太频繁", () => {
    expect(classifyAiError(rateLimitError('{"error":{"message":"rate limit exceeded"}}')).category).toBe(
      "rate_limit"
    );
  });

  it("502 服务端故障", () => {
    expect(classifyAiError(serverError("{}")).category).toBe("server");
  });

  it("400 走 Other 分支时按 body 判定", () => {
    const result = classifyAiError(
      otherError(400, '{"error":{"message":"prompt is required","type":"invalid_request_error"}}')
    );
    // 没有可识别的关键词 → 兜底，而不是硬猜成某一类
    expect(result.category).toBe("unknown");
  });

  /**
   * 真实日志里这句话出现了 16 次。它含 "model ... not supported"，曾被
   * "模型不存在 / 请检查模型名称是否正确" 抢走——而模型名是对的，用户照做只会白折腾。
   */
  it("模型不在对话端点服务时，不再误报为「模型不存在」", () => {
    const result = classifyAiError(
      otherError(
        400,
        '{"error":{"message":"This model is not supported on the Chat Completions endpoint","type":"invalid_request_error"}}'
      )
    );
    expect(result.title).toBe("模型不在对话接口服务");
    expect(result.title).not.toBe("模型不存在");
  });
});

describe("classifyAiError — 兜底", () => {
  it("空响应", () => {
    expect(classifyAiError("AI 服务返回了空响应，请重试或切换模型").title).toBe("未收到回复");
  });

  it("网络连接失败", () => {
    expect(classifyAiError("网络连接失败: 无法连接到服务器: error sending request").category).toBe(
      "network"
    );
  });

  it("完全无法识别时给兜底文案", () => {
    expect(classifyAiError("某种没见过的错误").category).toBe("unknown");
    expect(classifyAiError("某种没见过的错误").title).toBe("请求失败");
  });

  it("非字符串输入不抛异常", () => {
    expect(() => classifyAiError(undefined)).not.toThrow();
    expect(() => classifyAiError(null)).not.toThrow();
    expect(classifyAiError(undefined).category).toBe("unknown");
  });
});
