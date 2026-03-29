// Pipeline status bar — shows current transcription stage in the header

import { listen } from "@tauri-apps/api/event";

const stages = ["rec", "whisper", "postproc", "agent", "paste"] as const;
type Stage = (typeof stages)[number];

let activeStage: Stage | null = null;
let bar: HTMLElement | null = null;
let clearTimer: ReturnType<typeof setTimeout> | null = null;

function getBar(): HTMLElement | null {
  if (!bar) bar = document.getElementById("pipeline-status-bar");
  return bar;
}

function setStage(
  stage: Stage | null,
  state: "active" | "done" | "error" = "active",
  force = false
): void {
  const b = getBar();
  if (!b) return;

  // Only advance forward through the pipeline
  if (!force && stage !== null && activeStage !== null) {
    if (stages.indexOf(stage) < stages.indexOf(activeStage)) return;
  }

  if (clearTimer !== null) { clearTimeout(clearTimer); clearTimer = null; }

  if (stage === null) {
    b.hidden = true;
    activeStage = null;
    stages.forEach(s => b.querySelector(`[data-stage="${s}"]`)?.classList.remove("active", "done", "error"));
    return;
  }

  b.hidden = false;
  const stageIdx = stages.indexOf(stage);
  stages.forEach((s, i) => {
    const el = b.querySelector(`[data-stage="${s}"]`);
    if (!el) return;
    el.classList.remove("active", "done", "error");
    if (i < stageIdx) el.classList.add("done");
    else if (i === stageIdx) el.classList.add(state);
  });
  activeStage = stage;
}

function clearAfterDelay(ms: number): void {
  if (clearTimer !== null) clearTimeout(clearTimer);
  clearTimer = setTimeout(() => {
    clearTimer = null;
    setStage(null);
  }, ms);
}

export function initPipelineStatus(): void {
  // capture:state fires "recording" at start AND at end of transcription cycle.
  // Only start the pipeline on the first "recording" (when bar is idle).
  listen<string>("capture:state", (e) => {
    if (e.payload === "recording" && activeStage === null) setStage("rec");
    else if (e.payload === "transcribing") setStage("whisper");
  });

  // Raw transcription text ready → post-processing begins
  listen("transcription:raw-result", () => setStage("postproc"));

  // Final result without AI refinement → go to paste
  listen<{ paste_deferred?: boolean }>("transcription:result", (e) => {
    if (!e.payload?.paste_deferred) {
      setStage("paste");
      clearAfterDelay(1500);
    }
  });

  // AI refinement complete → paste refined text
  listen("transcription:refined", () => { setStage("paste"); clearAfterDelay(1500); });

  // AI refinement failed → fallback paste of raw text
  listen("transcription:refinement-failed", () => { setStage("paste"); clearAfterDelay(1500); });

  // Transcription error → show error state then hide
  listen("transcription:error", () => { setStage("whisper", "error"); clearAfterDelay(3000); });

  listen("assistant:intent-detected", () => {
    setStage("agent", "active", true);
  });

  listen("assistant:action-result", () => {
    setStage("agent", "done", true);
    clearAfterDelay(1500);
  });

  listen<{ state?: string }>("assistant:state-changed", (event) => {
    if (event.payload?.state === "idle" || event.payload?.state === "recovering") {
      clearAfterDelay(400);
    }
  });
}
