// Backend event `refinement:health`: the model dropped out of memory and the backend heals it.
// Episode = `recovering`, then `recovered` or `failed`. One toast per transition, never a repeat.

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { describeRefinementReason } from "./refinement-reasons";
import { dismissToast, showToast } from "./toast";
import { setOllamaModelState } from "./ui-state";
import type { RefinementHealthEvent } from "./types";

// Safety net: the closing event dismisses the toast; this only matters if it never arrives.
const RECOVERING_TOAST_MS = 15_000;

let lastState: RefinementHealthEvent["state"] | null = null;
let recoveringToastId: string | null = null;

export function handleRefinementHealth(payload: RefinementHealthEvent | null | undefined): void {
  const state = payload?.state;
  if (state !== "recovering" && state !== "recovered" && state !== "failed") return;

  // Status light follows every event (gray/yellow while healing, green when back, gray if given up).
  setOllamaModelState(state === "recovering" ? "loading" : state === "recovered" ? "warm" : "cold");

  // Repeated state (duplicate or replayed event) stays silent.
  if (state === lastState) return;
  lastState = state;

  dismissToast(recoveringToastId);
  recoveringToastId = null;

  if (state === "recovering") {
    const label = describeRefinementReason(payload?.reason)?.label ?? "model not loaded";
    recoveringToastId = showToast({
      type: "warning",
      title: "Refinement paused",
      message: `${label.charAt(0).toUpperCase()}${label.slice(1)}. Reloading…`,
      duration: RECOVERING_TOAST_MS,
    });
  } else if (state === "recovered") {
    showToast({
      type: "success",
      title: "Refinement restored",
      message: "The model is loaded again. Dictation is refined as usual.",
      duration: 4000,
    });
  } else {
    showToast({
      type: "warning",
      title: "Refinement unavailable",
      message: "Dictation continues as raw text.",
      duration: 9000,
    });
  }
}

/** Registers the backend listener; the caller keeps the returned unlisten (see main.ts). */
export function listenRefinementHealth(): Promise<UnlistenFn> {
  return listen<RefinementHealthEvent>("refinement:health", (event) => {
    handleRefinementHealth(event.payload);
  });
}

export function resetRefinementHealth(): void {
  dismissToast(recoveringToastId);
  recoveringToastId = null;
  lastState = null;
}
