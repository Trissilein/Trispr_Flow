// DOM event listeners setup

import {
  ensureCaptureRuntimeDefaults,
  ensureContinuousDumpDefaults,
  ensureDiagnosticsDefaults,
} from "./settings";
import { wireHistory } from "./wiring/history.wire";
import { wireOverlay } from "./wiring/overlay.wire";
import { wireTranscription } from "./wiring/transcription.wire";
import { wireAiRefinement } from "./wiring/ai-refinement.wire";
import { wireAppChrome } from "./wiring/app-chrome.wire";

// Preserved for re-entrant bootstrap callers. Slice 6 moved the only producer
// to app-chrome.wire.ts behind an idempotency guard.
export function cleanupWindowListeners(): void {
  // no-op
}

export function wireEvents() {
  ensureContinuousDumpDefaults();
  ensureCaptureRuntimeDefaults();
  ensureDiagnosticsDefaults();

  // Transcription + Whisper Backend (R2 slice 4).
  wireTranscription();

  // AI Refinement + Post-Processing + Language controls + Local Runtime (R2 slice 5).
  wireAiRefinement();

  wireAppChrome();

  wireHistory();

  wireOverlay();
}
