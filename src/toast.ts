// Toast notification system
import type { ToastType, ToastOptions, AppErrorType } from "./types";

let toastCounter = 0;

export function showToast(options: ToastOptions) {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const id = `toast-${++toastCounter}`;
  const type = options.type || "info";
  const duration = options.duration || 5000;

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
  toast.innerHTML = `
    <span class="toast-icon">${icon}</span>
    <div class="toast-content">
      <div class="toast-title">${options.title}</div>
      <div class="toast-message">${options.message}</div>
    </div>
    <button class="toast-close" title="Close">×</button>
  `;

  const closeBtn = toast.querySelector(".toast-close");
  closeBtn?.addEventListener("click", () => removeToast(id));

  container.appendChild(toast);

  // Auto-remove after duration
  if (duration > 0) {
    setTimeout(() => removeToast(id), duration);
  }
}

function removeToast(id: string) {
  const toast = document.getElementById(id);
  if (!toast) return;

  toast.classList.add("removing");

  setTimeout(() => {
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
