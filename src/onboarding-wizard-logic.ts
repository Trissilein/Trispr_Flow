// Pure decisions of the setup wizard (no DOM, no Tauri): which steps exist, what a finished
// self-test means, how progress reads. `onboarding-wizard.ts` wires them to the page.

export type WizardStepId = "runtime" | "model" | "mode" | "hotkey" | "refinement";
export type RuntimeVariant = "cuda" | "vulkan";

export const REFINEMENT_MODEL = "gemma4:e4b";

export interface WhisperRuntimeVariantStatus {
  variant: RuntimeVariant;
  bundled: boolean;
  downloaded: boolean;
  ready: boolean;
  zip_size: number;
}

export interface WhisperRuntimeStatus {
  variants: WhisperRuntimeVariantStatus[];
  any_ready: boolean;
  whisper_cpp_version: string;
  runtime_dir: string;
}

export interface RuntimeRecommendation {
  recommended: RuntimeVariant;
  gpu_name: string | null;
  nvidia: boolean;
  reason: string;
}

export interface RuntimeVerifyResult {
  variant: RuntimeVariant;
  ok: boolean;
  detail: string;
  fallback_variant: RuntimeVariant | null;
}

export interface WhisperRuntimeProgress {
  variant: RuntimeVariant;
  stage: "download" | "verify" | "extract" | "install" | "done";
  message: string;
  downloaded: number | null;
  total: number | null;
}

const ALL_STEPS: readonly WizardStepId[] = ["runtime", "model", "mode", "hotkey", "refinement"];

/**
 * The steps to show. A first run (no finished wizard, no model yet) gets all of them, minus
 * the speech engine step when a runtime is bundled or already downloaded. Someone who already
 * has a model or finished the wizard is only bothered when the speech engine is gone
 * (deleted, never downloaded): then the wizard reopens with that one step.
 */
export function buildWizardSteps(opts: {
  runtimeReady: boolean;
  modelInstalled: boolean;
  completed: boolean;
}): WizardStepId[] {
  if (opts.completed || opts.modelInstalled) return opts.runtimeReady ? [] : ["runtime"];
  return ALL_STEPS.filter((id) => id !== "runtime" || !opts.runtimeReady);
}

export function variantStatus(
  status: WhisperRuntimeStatus | null,
  variant: RuntimeVariant
): WhisperRuntimeVariantStatus | undefined {
  return status?.variants.find((v) => v.variant === variant);
}

export type AfterSelfTest =
  | { action: "ready" }
  | { action: "switch"; variant: RuntimeVariant }
  | { action: "download"; variant: RuntimeVariant }
  | { action: "failed"; message: string };

/**
 * What to do with a finished self-test. A CUDA engine that does not run here (old driver, no
 * usable GPU) is replaced by Vulkan: switched to when it is already on disk, downloaded when
 * not. Any other failure is shown with a retry.
 */
export function planAfterSelfTest(
  result: RuntimeVerifyResult,
  status: WhisperRuntimeStatus | null
): AfterSelfTest {
  if (result.ok) return { action: "ready" };
  const fallback = result.fallback_variant;
  if (fallback) {
    return variantStatus(status, fallback)?.ready
      ? { action: "switch", variant: fallback }
      : { action: "download", variant: fallback };
  }
  return { action: "failed", message: result.detail };
}

export function formatMegabytes(bytes: number): string {
  return `${Math.round(bytes / 1024 / 1024)} MB`;
}

/** `{ percent, text }` of a running download; `percent` is null while the total is unknown. */
export function describeProgress(
  downloaded: number | null,
  total: number | null,
  message: string
): { percent: number | null; text: string } {
  if (downloaded === null || downloaded === undefined) return { percent: null, text: message };
  if (!total || total <= 0) return { percent: null, text: `${formatMegabytes(downloaded)} downloaded` };
  const percent = Math.min(100, Math.max(0, Math.round((downloaded / total) * 100)));
  return { percent, text: `${formatMegabytes(downloaded)} / ${formatMegabytes(total)} (${percent}%)` };
}

export function describeRecommendation(rec: RuntimeRecommendation | null): string {
  if (!rec) return "Could not detect the graphics card. Vulkan runs on any modern GPU.";
  return rec.reason;
}
