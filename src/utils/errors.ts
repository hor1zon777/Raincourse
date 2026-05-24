/**
 * 后端返回的统一错误结构（来自 Rust `AppError::serialize`）。
 *
 * 形如 `{ code: "SESSION_EXPIRED", message: "会话已过期，请重新扫码登录" }`。
 *
 * Tauri 在 invoke 失败时把 `Result::Err` 序列化后 throw 到前端，
 * 这里把任意未知错误归一化成一致结构，UI 可按 code 分支判断。
 */
export interface AppErrorPayload {
  code: string;
  message: string;
}

const KNOWN_CODES = new Set([
  'REQUEST_FAILED',
  'WEBSOCKET_ERROR',
  'JSON_PARSE_ERROR',
  'IO_ERROR',
  'NOT_LOGGED_IN',
  'SESSION_EXPIRED',
  'API_ERROR',
  'CANCELLED',
  'INVALID_INPUT',
  'CONFIG_ERROR',
  'GENERAL_ERROR',
]);

/** 把 invoke catch 到的任意值归一化成 `AppErrorPayload`。 */
export function normalizeError(err: unknown): AppErrorPayload {
  // 后端返回的结构化错误
  if (
    err !== null &&
    typeof err === 'object' &&
    'code' in err &&
    'message' in err &&
    typeof (err as Record<string, unknown>).code === 'string' &&
    typeof (err as Record<string, unknown>).message === 'string'
  ) {
    const code = (err as AppErrorPayload).code;
    return {
      code: KNOWN_CODES.has(code) ? code : 'GENERAL_ERROR',
      message: (err as AppErrorPayload).message,
    };
  }
  // Error 实例
  if (err instanceof Error) {
    return { code: 'GENERAL_ERROR', message: err.message };
  }
  // 字符串
  if (typeof err === 'string') {
    return { code: 'GENERAL_ERROR', message: err };
  }
  // 兜底
  return { code: 'GENERAL_ERROR', message: String(err) };
}

export function isSessionExpired(err: unknown): boolean {
  return normalizeError(err).code === 'SESSION_EXPIRED';
}
