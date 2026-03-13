import { invoke } from "@tauri-apps/api/core";

type FrontendLogLevel = "info" | "warn" | "error";

function stringifyExtra(extra: unknown): string {
  if (extra === undefined) return "";
  try {
    return JSON.stringify(extra);
  } catch {
    return String(extra);
  }
}

export function traceFrontend(level: FrontendLogLevel, context: string, message: string, extra?: unknown): void {
  const normalizedContext = context.trim() || "frontend";
  const payload = stringifyExtra(extra);
  const fullMessage = payload ? `${message} | ${payload}` : message;
  void invoke("log_frontend_event", {
    level,
    context: normalizedContext,
    message: fullMessage,
  }).catch(() => {});
}

export function traceFrontendInfo(context: string, message: string, extra?: unknown): void {
  traceFrontend("info", context, message, extra);
}

export function traceFrontendWarn(context: string, message: string, extra?: unknown): void {
  traceFrontend("warn", context, message, extra);
}

export function traceFrontendError(context: string, message: string, extra?: unknown): void {
  traceFrontend("error", context, message, extra);
}

export function installGlobalFrontendErrorLogging(): void {
  window.addEventListener("error", (event) => {
    traceFrontendError("window.error", event.message || "Unhandled window error", {
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason instanceof Error
      ? { message: event.reason.message, stack: event.reason.stack }
      : event.reason;
    traceFrontendError("window.unhandledrejection", "Unhandled promise rejection", reason);
  });
}
