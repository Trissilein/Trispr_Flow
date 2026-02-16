// Toast notification system
import type { ToastType, ToastOptions, AppErrorType } from "./types";

let toastCounter = 0;

export function showToast(options: ToastOptions): string | null {
  const container = document.getElementById("toast-container");
  if (!container) return null;

  const id = `toast-${++toastCounter}`;
  const type = options.type || "info";
  const duration = options.duration ?? 5000;

  const icons: Record<ToastType, string> = {
    error: "❌",
    success: "✅",
    warning: "⚠️",
    info: "ℹ️",
  };

  const icon = options.icon || icons[type];

  const toast = document.createElement("div");
  toast.id = id;
  toast.className = `toast ${type}`;

  const iconEl = document.createElement("span");
  iconEl.className = "toast-icon";
  iconEl.textContent = icon;

  const contentEl = document.createElement("div");
  contentEl.className = "toast-content";

  const titleEl = document.createElement("div");
  titleEl.className = "toast-title";
  titleEl.textContent = options.title;

  const messageEl = document.createElement("div");
  messageEl.className = "toast-message";
  messageEl.textContent = options.message;

  contentEl.append(titleEl, messageEl);

  if (options.actionLabel && options.onAction) {
    const actionsEl = document.createElement("div");
    actionsEl.className = "toast-actions";

    const actionBtn = document.createElement("button");
    actionBtn.type = "button";
    actionBtn.className = "toast-action";
    actionBtn.textContent = options.actionLabel;
    actionBtn.addEventListener("click", async () => {
      actionBtn.disabled = true;
      try {
        await options.onAction?.();
        if (options.actionDismiss !== false) {
          removeToast(id);
        }
      } finally {
        actionBtn.disabled = false;
      }
    });

    actionsEl.appendChild(actionBtn);
    contentEl.appendChild(actionsEl);
  }

  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "toast-close";
  closeBtn.title = "Close";
  closeBtn.textContent = "x";
  closeBtn.addEventListener("click", () => removeToast(id));

  toast.append(iconEl, contentEl, closeBtn);

  container.appendChild(toast);

  // Auto-remove after duration
  if (duration > 0) {
    window.setTimeout(() => removeToast(id), duration);
  }

  return id;
}

export function dismissToast(id: string | null | undefined) {
  if (!id) return;
  removeToast(id);
}

function removeToast(id: string) {
  const toast = document.getElementById(id);
  if (!toast || toast.classList.contains("removing")) return;

  toast.classList.add("removing");

  window.setTimeout(() => {
    toast.remove();
  }, 200);
}

export function showErrorToast(error: AppErrorType, context?: string) {
  const typeMapping: Record<string, string> = {
    AudioDevice: "Audio Device Issue",
    Transcription: "Transcription Failed",
    Hotkey: "Hotkey Problem",
    Storage: "Storage Error",
    Network: "Network Problem",
    Window: "Window Error",
    Other: "Error",
  };

  showToast({
    type: "error",
    title: typeMapping[error.type] || "Error",
    message: context ? `${context}: ${error.message}` : error.message,
    duration: 7000,
  });
}
