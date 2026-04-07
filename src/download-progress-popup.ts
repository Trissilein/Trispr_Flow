import { modelProgress, ollamaPullProgress, quantizeProgress } from "./state";
import { getRuntimeInstallProgress, computeOllamaPercent } from "./ollama-models";
import { formatBytes } from "./ui-helpers";

interface DownloadItem {
  key: string;
  label: string;
  percent: number | null; // null = indeterminate
  statusText: string;
}

function collectItems(): DownloadItem[] {
  const items: DownloadItem[] = [];

  // Whisper model downloads
  for (const [id, p] of modelProgress.entries()) {
    const pct = p.total && p.total > 0 ? Math.round((p.downloaded / p.total) * 100) : null;
    const status = pct !== null
      ? `${formatBytes(p.downloaded)} / ${formatBytes(p.total!)}  •  ${pct}%`
      : `${formatBytes(p.downloaded)} downloaded…`;
    items.push({ key: `model:${id}`, label: id, percent: pct, statusText: status });
  }

  // Ollama model pulls
  for (const [model, p] of ollamaPullProgress.entries()) {
    const pct = p.total && p.total > 0 && p.completed != null
      ? computeOllamaPercent(p)
      : null;
    const status = pct !== null
      ? `${formatBytes(p.completed!)} / ${formatBytes(p.total!)}  •  ${pct}%`
      : (p.status ?? "Downloading…");
    items.push({ key: `pull:${model}`, label: model, percent: pct, statusText: status });
  }

  // Model quantization — skip completed entries
  for (const [fileName, p] of quantizeProgress.entries()) {
    if (p.phase === "done") continue;
    const pct = typeof p.percent === "number" ? p.percent : null;
    const status = p.message?.trim() || (pct !== null ? `${Math.round(pct)}%` : "Quantization in progress…");
    items.push({ key: `quant:${fileName}`, label: fileName, percent: pct, statusText: status });
  }

  // Ollama runtime install/download
  const runtime = getRuntimeInstallProgress();
  if (runtime) {
    const pct = runtime.downloaded != null && runtime.total != null && runtime.total > 0
      ? Math.round((runtime.downloaded / runtime.total) * 100)
      : null;
    const label = runtime.version ? `Runtime ${runtime.version}` : "Ollama Runtime";
    const status = pct !== null
      ? `${formatBytes(runtime.downloaded!)} / ${formatBytes(runtime.total!)}  •  ${pct}%`
      : (runtime.message || "Loading…");
    items.push({ key: `runtime:${runtime.stage}`, label, percent: pct, statusText: status });
  }

  return items;
}

function buildItemHtml(item: DownloadItem): string {
  const isIndeterminate = item.percent === null;
  const fillStyle = isIndeterminate ? "" : `style="width: ${Math.min(100, Math.max(0, item.percent!))}%"`;
  const fillClass = `dpp-item-bar-fill${isIndeterminate ? " dpp-item-bar-fill--indeterminate" : ""}`;
  return `<div class="dpp-item">
      <span class="dpp-item-label" title="${item.label}">${item.label}</span>
      <div class="dpp-item-bar"><div class="${fillClass}" ${fillStyle}></div></div>
      <span class="dpp-item-status">${item.statusText}</span>
    </div>`;
}

let _popupRenderFrame: number | null = null;

export function scheduleDownloadProgressRender(): void {
  if (_popupRenderFrame !== null) return;
  _popupRenderFrame = requestAnimationFrame(() => {
    _popupRenderFrame = null;
    renderDownloadProgressPopup();
  });
}

export function renderDownloadProgressPopup(): void {
  const popup = document.getElementById("download-progress-popup");
  if (!popup) return;

  const items = collectItems();

  if (items.length === 0) {
    popup.hidden = true;
    return;
  }

  const countLabel = items.length === 1 ? "1 Download" : `${items.length} Downloads`;
  popup.innerHTML = `<div class="dpp-header"><span class="dpp-header-title">↓ ${countLabel}</span></div>${items.map(buildItemHtml).join("")}`;
  popup.hidden = false;
}
