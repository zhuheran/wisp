/**
 * Strip the noisy `Model 'X' failed while streaming conversation 'Y':` prefix
 * that the Rust backend wraps around streaming errors.
 */
function stripStreamWrapper(raw: string): string {
  const re =
    /^Model '[^']*' failed while streaming conversation '[^']*':\s*/;
  return raw.replace(re, "");
}

interface ParsedApiError {
  status: number;
  code?: string;
  message: string;
}

/**
 * Parse the structured backend format: `API error <status> [<code>]: <message>`
 * or the legacy `API error (<status>): <body>`. Returns undefined if it does
 * not match.
 */
function parseApiError(text: string): ParsedApiError | undefined {
  // New structured format: "API error 402 [invalid_request_error]: InsufficientBalance"
  const structured = /^API error (\d+)(?:\s*\[([^\]]*)\])?\s*:\s*(.+)$/s.exec(
    text,
  );
  if (structured) {
    const status = Number(structured[1]);
    const code = structured[2]?.trim() || undefined;
    const message = structured[3].trim();
    return { status, code, message };
  }

  // Legacy format: "API error (402): {raw body}" — try to recover message/code from JSON body.
  const legacy = /^API error \((\d+)\):\s*(.+)$/s.exec(text);
  if (legacy) {
    const status = Number(legacy[1]);
    const body = legacy[2].trim();
    try {
      const parsed = JSON.parse(body)?.error;
      if (parsed && typeof parsed === "object") {
        const message =
          typeof parsed.message === "string" ? parsed.message : body;
        const code =
          typeof parsed.code === "string"
            ? parsed.code
            : typeof parsed.type === "string" && parsed.type !== "unknown_error"
              ? parsed.type
              : undefined;
        return { status, code, message };
      }
    } catch {
      // fallthrough
    }
    return { status, message: body };
  }

  return undefined;
}

const STATUS_HINT: Record<number, string> = {
  400: "请求格式不正确",
  401: "API Key 无效或已过期",
  402: "账户余额不足",
  403: "无权访问该资源",
  404: "模型或端点不存在",
  408: "请求超时",
  413: "请求内容过大",
  422: "请求参数校验失败",
  429: "请求过于频繁，已触发限流",
  500: "服务端内部错误",
  502: "网关错误",
  503: "服务暂时不可用",
  504: "网关超时",
};

const CODE_HINT: Record<string, string> = {
  invalid_request_error: "请求参数有误",
  invalid_api_key: "API Key 无效",
  insufficient_quota: "额度已用尽",
  insufficient_balance: "账户余额不足",
  rate_limit_exceeded: "请求被限流",
  model_not_found: "模型不存在",
  context_length_exceeded: "上下文长度超限",
};

/**
 * Format a backend LLM error string into a friendly, human-readable message.
 * Returns the original text when nothing recognised.
 */
export function formatLlmError(raw: string): string {
  const text = stripStreamWrapper(raw).trim();
  const parsed = parseApiError(text);

  if (!parsed) return text;

  const segments: string[] = [];

  const codeHint = parsed.code ? CODE_HINT[parsed.code] : undefined;
  const statusHint = STATUS_HINT[parsed.status];

  // Prefer the most specific hint as the headline.
  if (codeHint) {
    segments.push(codeHint);
  } else if (statusHint) {
    segments.push(statusHint);
  } else {
    segments.push(`API 错误 (${parsed.status})`);
  }

  // Surface upstream's own message verbatim if it adds detail.
  if (parsed.message && !isMessageRedundant(codeHint, parsed.code, parsed.message)) {
    segments.push(parsed.message);
  }

  // Append code + status as a compact tail for debugging.
  const meta: string[] = [];
  if (parsed.code) meta.push(parsed.code);
  meta.push(String(parsed.status));
  segments.push(`[${meta.join(" / ")}]`);

  return segments.join(" · ");
}

function isMessageRedundant(
  hint: string | undefined,
  code: string | undefined,
  message: string,
): boolean {
  const msg = message.toLowerCase();
  if (hint && msg.includes(hint.toLowerCase())) return true;
  if (code && msg === code.toLowerCase()) return true;
  return false;
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return formatLlmError(error.message || String(error));
  }

  if (typeof error === "string") {
    return formatLlmError(error);
  }

  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.length > 0) {
      return formatLlmError(message);
    }
  }

  return String(error);
}
