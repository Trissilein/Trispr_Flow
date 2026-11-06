// Readable texts for the backend's refinement reason codes (history, gate, health toasts).
// One table so every surface says the same thing.

const REASONS = {
  model_not_loaded: { label: "model not loaded", past: "model was not loaded" },
  ps_query_failed: { label: "Ollama status unavailable", past: "Ollama status could not be read" },
  gpu_busy: { label: "GPU busy", past: "GPU was busy" },
  cold_start_raw_first: { label: "model warming up", past: "model was warming up" },
  raw_timeout: { label: "refinement timed out", past: "refinement timed out" },
  queue_full: { label: "refinement queue full", past: "refinement queue was full" },
  prepare_failed: { label: "refinement setup failed", past: "refinement setup failed" },
  provider_error: { label: "Ollama error", past: "Ollama returned an error" },
  disabled: { label: "refinement off", past: "refinement was off" },
} as const;

export type RefinementReasonCode = keyof typeof REASONS;

// Gate code used before the unified set existed.
const LEGACY_ALIASES = new Map<string, RefinementReasonCode>([
  ["ollama_model_not_loaded", "model_not_loaded"],
]);

/** Trimmed, legacy-mapped code; null for empty/missing input. */
export function normalizeRefinementReason(code: string | null | undefined): string | null {
  const trimmed = code?.trim();
  if (!trimmed) return null;
  return LEGACY_ALIASES.get(trimmed) ?? trimmed;
}

/** Codes under which the model could not be used right now (raw pasted, model warming/reloading). */
export function isModelUnavailableReason(code: string | null | undefined): boolean {
  const normalized = normalizeRefinementReason(code);
  return normalized === "model_not_loaded"
    || normalized === "ps_query_failed"
    || normalized === "cold_start_raw_first";
}

/**
 * `label`: present tense ("model not loaded"), `past`: for history ("model was not loaded").
 * Unknown codes fall back to the code with spaces; empty input returns null.
 */
export function describeRefinementReason(
  code: string | null | undefined,
): { label: string; past: string } | null {
  const normalized = normalizeRefinementReason(code);
  if (!normalized) return null;
  if (Object.prototype.hasOwnProperty.call(REASONS, normalized)) {
    return REASONS[normalized as RefinementReasonCode];
  }
  const fallback = normalized.replace(/_/g, " ");
  return { label: fallback, past: fallback };
}
