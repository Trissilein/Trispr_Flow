import { listen } from "@tauri-apps/api/event";

type OverlayState = "idle" | "toggle-idle" | "recording" | "transcribing";

const root = document.getElementById("overlay-root") as HTMLDivElement | null;

let currentState: OverlayState = "idle";
let currentLevel = 0;

function setLevel(level: number) {
  if (!root) return;
  const clamped = Math.max(0, Math.min(1, level));
  root.style.setProperty("--level", clamped.toFixed(3));
}

function updateOverlay(state: OverlayState) {
  if (!root) return;
  currentState = state;
  root.dataset.state = state;
  if (state !== "recording") {
    currentLevel = 0;
    setLevel(0);
  }
}

listen<OverlayState>("overlay:state", (event) => {
  updateOverlay(event.payload);
});

listen<number>("overlay:level", (event) => {
  if (currentState !== "recording") return;
  const next = Math.max(0, Math.min(1, event.payload ?? 0));
  currentLevel = currentLevel * 0.6 + next * 0.4;
  setLevel(currentLevel);
});

updateOverlay("idle");
