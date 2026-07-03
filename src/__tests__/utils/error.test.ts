import { describe, it, expect } from "vitest";
import { errorMessage, formatLlmError } from "../../utils/error";

describe("formatLlmError", () => {
  it("parses structured API error with code", () => {
    const input =
      "API error 402 [invalid_request_error]: InsufficientBalance";
    expect(formatLlmError(input)).toBe(
      "请求参数有误 · InsufficientBalance · [invalid_request_error / 402]",
    );
  });

  it("strips the streaming wrapper prefix", () => {
    const input =
      "Model 'deepseek-v4-flash' failed while streaming conversation '00581FF9-C3EF-47A8-9C6A-16F69785284E': API error 402 [invalid_request_error]: InsufficientBalance";
    expect(formatLlmError(input)).toContain("请求参数有误");
    expect(formatLlmError(input)).not.toContain("failed while streaming");
  });

  it("prefers code-specific hint and omits duplicate upstream message", () => {
    const input =
      "API error 429 [rate_limit_exceeded]: rate_limit_exceeded";
    const out = formatLlmError(input);
    expect(out).toBe("请求被限流 · [rate_limit_exceeded / 429]");
  });

  it("falls back to status hint when code is unknown", () => {
    const input = "API error 503: something broke";
    expect(formatLlmError(input)).toBe(
      "服务暂时不可用 · something broke · [503]",
    );
  });

  it("falls back to generic label for unknown status", () => {
    const input = "API error 418: i'm a teapot";
    expect(formatLlmError(input)).toBe(
      "API 错误 (418) · i'm a teapot · [418]",
    );
  });

  it("parses legacy JSON body format", () => {
    const input =
      'API error (402): {"error":{"message":"InsufficientBalance","type":"unknown_error","param":null,"code":"invalid_request_error"}}';
    const out = formatLlmError(input);
    expect(out).toContain("请求参数有误");
    expect(out).toContain("InsufficientBalance");
    expect(out).toContain("[invalid_request_error / 402]");
  });

  it("returns original text when not an API error", () => {
    const input = "NetworkError: failed to fetch";
    expect(formatLlmError(input)).toBe("NetworkError: failed to fetch");
  });

  it("keeps the wrapper-stripped non-API message", () => {
    const input =
      "Model 'm' failed while streaming conversation 'c': something else went wrong";
    expect(formatLlmError(input)).toBe("something else went wrong");
  });
});

describe("errorMessage integration", () => {
  it("formats Error instances", () => {
    const err = new Error(
      "API error 429 [rate_limit_exceeded]: Rate limit hit",
    );
    expect(errorMessage(err)).toBe(
      "请求被限流 · Rate limit hit · [rate_limit_exceeded / 429]",
    );
  });

  it("formats plain string errors", () => {
    expect(errorMessage("API error 401: bad key")).toBe(
      "API Key 无效或已过期 · bad key · [401]",
    );
  });

  it("handles { message } objects", () => {
    expect(errorMessage({ message: "API error 404 [model_not_found]: nope" })).toBe(
      "模型不存在 · nope · [model_not_found / 404]",
    );
  });
});
