import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";

// ---------------------------------------------------------------------------
// Types (mirror Rust SourceItem / VideoJobRequest / VideoJobResult)
// ---------------------------------------------------------------------------

type SourceKind = "content" | "asset" | "hybrid";

interface SourceItem {
  id: string;
  kind: SourceKind;
  display_name: string;
  original_path?: string | null;
  extracted_text?: string | null;
  asset_path?: string | null;
  metadata: unknown;
  order: number;
}

interface VideoJobRequest {
  source_items: SourceItem[];
  style: string;
  resolution: string;
  fps: number;
  brief: string | null;
  tts: boolean;
}

interface VideoJobResult {
  job_id: string;
  output_path: string;
  duration_ms: number;
}

interface ProgressPayload {
  job_id: string;
  phase: string;
  progress: number;
  message?: string | null;
}

// ---------------------------------------------------------------------------
// Module state
// ---------------------------------------------------------------------------

const queue: SourceItem[] = [];
let renderInFlight = false;
let progressUnlisten: UnlistenFn | null = null;
let completeUnlisten: UnlistenFn | null = null;
let tauriDragDropUnlisten: UnlistenFn | null = null;

// ---------------------------------------------------------------------------
// DOM lookup helpers — keep each lookup local so an init-ordering issue in one
// element doesn't take the whole panel down.
// ---------------------------------------------------------------------------

function $(id: string): HTMLElement | null {
  return document.getElementById(id);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

function kindBadge(kind: SourceKind): string {
  const label =
    kind === "content" ? "content" : kind === "asset" ? "asset" : "hybrid";
  return `<span class="video-queue-badge video-queue-badge-${kind}">${label}</span>`;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function renderQueue(): void {
  const list = $("video-queue-list") as HTMLOListElement | null;
  const count = $("video-queue-count");
  if (!list) return;
  if (count) count.textContent = `${queue.length} item${queue.length === 1 ? "" : "s"}`;

  if (queue.length === 0) {
    list.innerHTML =
      '<li class="video-queue-empty field-hint">Queue is empty. Drop files above or pick from history.</li>';
    return;
  }

  list.innerHTML = "";
  queue.forEach((item, idx) => {
    const li = document.createElement("li");
    li.className = "video-queue-item";
    li.dataset.itemId = item.id;
    const preview =
      item.extracted_text && item.extracted_text.trim().length > 0
        ? item.extracted_text.trim().slice(0, 120).replace(/\s+/g, " ")
        : item.original_path || item.asset_path || "";
    li.innerHTML = `
      <span class="video-queue-index">${idx + 1}</span>
      <span class="video-queue-name">${escapeHtml(item.display_name)}</span>
      ${kindBadge(item.kind)}
      <span class="video-queue-preview">${escapeHtml(preview)}</span>
      <button class="btn ghost small video-queue-remove" data-remove-id="${item.id}" title="Remove">&times;</button>
    `;
    list.appendChild(li);
  });

  list.querySelectorAll<HTMLButtonElement>(".video-queue-remove").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      const id = btn.getAttribute("data-remove-id");
      if (!id) return;
      const idx = queue.findIndex((i) => i.id === id);
      if (idx >= 0) {
        queue.splice(idx, 1);
        queue.forEach((item, i) => (item.order = i));
        renderQueue();
      }
    });
  });
}

function renderProgress(phase: string, progress: number, message?: string | null): void {
  const container = $("video-progress");
  const phaseEl = $("video-progress-phase");
  const fillEl = $("video-progress-fill") as HTMLElement | null;
  const logEl = $("video-progress-log") as HTMLPreElement | null;
  if (!container || !phaseEl || !fillEl) return;
  container.hidden = false;
  phaseEl.textContent = phase;
  const pct = Math.max(0, Math.min(1, progress)) * 100;
  fillEl.style.width = `${pct.toFixed(1)}%`;
  if (message && logEl) {
    const now = new Date().toLocaleTimeString();
    logEl.textContent = `[${now}] ${phase}: ${message}\n${logEl.textContent || ""}`.slice(0, 8_000);
  }
}

function renderResult(result: VideoJobResult): void {
  const container = $("video-result");
  const pathEl = $("video-result-path");
  const player = $("video-result-player") as HTMLVideoElement | null;
  if (!container || !pathEl || !player) return;
  container.hidden = false;
  pathEl.textContent = result.output_path;
  try {
    player.src = convertFileSrc(result.output_path);
  } catch (err) {
    console.warn("[video-gen] convertFileSrc failed:", err);
    player.removeAttribute("src");
  }
}

// ---------------------------------------------------------------------------
// Ingest
// ---------------------------------------------------------------------------

async function ingestPaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  try {
    const items = await invoke<SourceItem[]>("video_ingest_sources", { paths });
    items.forEach((item) => {
      item.order = queue.length;
      queue.push(item);
    });
    renderQueue();
  } catch (err) {
    console.error("[video-gen] ingest failed:", err);
    renderProgress("error", 0, `ingest failed: ${err}`);
  }
}

async function ingestHistoryEntry(entryId: string): Promise<void> {
  try {
    const item = await invoke<SourceItem>("video_ingest_history_entry", {
      entryId,
    });
    item.order = queue.length;
    queue.push(item);
    renderQueue();
  } catch (err) {
    console.error("[video-gen] history ingest failed:", err);
    renderProgress("error", 0, `history ingest failed: ${err}`);
  }
}

// ---------------------------------------------------------------------------
// Drop zone wiring — HTML5 drag events for in-webview + Tauri drag-drop event
// for full-window drops from the OS.
// ---------------------------------------------------------------------------

function wireDropZone(): void {
  const zone = $("video-drop-zone");
  if (!zone) return;

  zone.addEventListener("dragover", (e) => {
    e.preventDefault();
    zone.classList.add("is-dragover");
  });
  zone.addEventListener("dragleave", () => {
    zone.classList.remove("is-dragover");
  });
  zone.addEventListener("drop", (e) => {
    e.preventDefault();
    zone.classList.remove("is-dragover");
    // HTML5 `drop` gives File objects without path on Tauri; Tauri's
    // window-level drag-drop event below is the reliable path.
  });
}

async function wireTauriDragDrop(): Promise<void> {
  try {
    const win = getCurrentWindow();
    tauriDragDropUnlisten = await win.onDragDropEvent(async (event) => {
      if (event.payload.type === "drop") {
        const paths = (event.payload.paths ?? []) as string[];
        if (paths.length > 0) {
          await ingestPaths(paths);
        }
      }
    });
  } catch (err) {
    console.warn("[video-gen] tauri drag-drop unavailable:", err);
  }
}

// ---------------------------------------------------------------------------
// Generate / render
// ---------------------------------------------------------------------------

async function generateVideo(): Promise<void> {
  if (renderInFlight) {
    console.warn("[video-gen] render already in flight");
    return;
  }
  if (queue.length === 0) {
    renderProgress("error", 0, "Queue is empty. Add sources first.");
    return;
  }

  const style = ($("video-style-select") as HTMLSelectElement | null)?.value ?? "slideshow";
  const resolution =
    ($("video-resolution-select") as HTMLSelectElement | null)?.value ?? "1920x1080";
  const fps = Number(($("video-fps-select") as HTMLSelectElement | null)?.value ?? 30);
  const tts =
    (($("video-tts-select") as HTMLSelectElement | null)?.value ?? "off") !== "off";
  const brief =
    ($("video-brief-textarea") as HTMLTextAreaElement | null)?.value?.trim() ?? "";

  const request: VideoJobRequest = {
    source_items: queue.slice(),
    style,
    resolution,
    fps,
    brief: brief.length > 0 ? brief : null,
    tts,
  };

  renderInFlight = true;
  renderProgress("starting", 0);
  const btn = $("video-generate-btn") as HTMLButtonElement | null;
  if (btn) btn.disabled = true;

  try {
    const result = await invoke<VideoJobResult>("video_generate", { request });
    renderProgress("done", 1, `wrote ${result.output_path} in ${result.duration_ms} ms`);
    renderResult(result);
  } catch (err) {
    console.error("[video-gen] render failed:", err);
    renderProgress("error", 0, String(err));
  } finally {
    renderInFlight = false;
    if (btn) btn.disabled = false;
  }
}

// ---------------------------------------------------------------------------
// Event-listener wiring (tauri backend events + button clicks)
// ---------------------------------------------------------------------------

async function wireBackendEvents(): Promise<void> {
  progressUnlisten = await listen<ProgressPayload>("video:progress", (e) => {
    renderProgress(e.payload.phase, e.payload.progress, e.payload.message ?? null);
  });
  completeUnlisten = await listen<VideoJobResult>("video:complete", (e) => {
    renderResult(e.payload);
  });
}

function wireButtons(): void {
  $("video-pick-files-btn")?.addEventListener("click", async () => {
    try {
      const selected = await openDialog({
        multiple: true,
        filters: [
          {
            name: "Supported sources",
            extensions: [
              "md", "txt", "json", "yaml", "yml", "html", "htm", "srt", "vtt",
              "png", "jpg", "jpeg", "webp", "gif", "svg",
              "mp3", "wav", "m4a", "ogg",
              "mp4", "mov", "webm", "mkv",
            ],
          },
        ],
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      await ingestPaths(paths);
    } catch (err) {
      console.error("[video-gen] pick files failed:", err);
    }
  });

  $("video-add-history-btn")?.addEventListener("click", async () => {
    const entryId = window.prompt("Transcript entry id (Phase 1a stub — UI picker comes later):");
    if (entryId && entryId.trim().length > 0) {
      await ingestHistoryEntry(entryId.trim());
    }
  });

  $("video-queue-clear-btn")?.addEventListener("click", () => {
    queue.splice(0, queue.length);
    renderQueue();
  });

  $("video-generate-btn")?.addEventListener("click", () => {
    void generateVideo();
  });

  $("video-open-folder-btn")?.addEventListener("click", async () => {
    try {
      await invoke("video_open_output_dir");
    } catch (err) {
      console.error("[video-gen] open folder failed:", err);
    }
  });
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

let initialized = false;

export async function initVideoGenerationPanel(): Promise<void> {
  if (initialized) return;
  initialized = true;
  wireDropZone();
  wireButtons();
  await wireTauriDragDrop();
  await wireBackendEvents();
  renderQueue();
}

export function teardownVideoGenerationPanel(): void {
  if (progressUnlisten) {
    progressUnlisten();
    progressUnlisten = null;
  }
  if (completeUnlisten) {
    completeUnlisten();
    completeUnlisten = null;
  }
  if (tauriDragDropUnlisten) {
    tauriDragDropUnlisten();
    tauriDragDropUnlisten = null;
  }
  initialized = false;
}
