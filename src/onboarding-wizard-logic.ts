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

// ---------------------------------------------------------------------------------------
// Troubleshooting: what the wizard shows when the engine setup fails
// ---------------------------------------------------------------------------------------

/** Structured failure of the download or the manual import (`code` is stable, see Rust `RuntimeError`). */
export interface RuntimeSetupError {
  code: string;
  /** What went wrong. */
  message: string;
  /** What the user can do. */
  hint: string;
  /** Technical text, for the diagnostics. */
  detail: string;
}

export interface ManualPackage {
  variant: RuntimeVariant;
  label: string;
  asset: string;
  url: string;
  size_mb: number;
  size_bytes: number;
  sha256: string;
  which_gpu_hint: string;
}

export interface ManualInfo {
  release_page_url: string;
  whisper_cpp_version: string;
  variants: ManualPackage[];
}

export interface SetupDiagnostics {
  app_version: string;
  os: string;
  arch: string;
  runtime_dir: string;
  whisper_cpp_version: string;
  adapters: Array<{ vendor_id: string; name: string }>;
  variants: Array<{ variant: string; bundled: boolean; downloaded: boolean }>;
}

export const GENERIC_HINT =
  'Retry. If it keeps failing, use "Download it yourself" below and copy the diagnostics when you ask for help.';

export const WHICH_ONE_DO_I_NEED =
  "NVIDIA RTX/GTX 16xx or newer with a current driver: the CUDA package. Everything else, including Intel and AMD graphics: the Vulkan package.";

/**
 * Tauri hands a failed command's `Err` over as the serialized value: our structured error for
 * the speech engine commands, a plain string for older ones. Anything else (an `Error`, an
 * unexpected object) is wrapped so the panel always has a cause and a hint.
 */
export function toRuntimeError(err: unknown): RuntimeSetupError {
  if (err && typeof err === "object" && !(err instanceof Error)) {
    const record = err as Record<string, unknown>;
    if (typeof record.message === "string" && record.message) {
      return {
        code: typeof record.code === "string" && record.code ? record.code : "unknown",
        message: record.message,
        hint: typeof record.hint === "string" && record.hint ? record.hint : GENERIC_HINT,
        detail: typeof record.detail === "string" ? record.detail : "",
      };
    }
  }
  const text = err instanceof Error ? err.message : typeof err === "string" ? err : safeJson(err);
  return { code: "unknown", message: text || "Unknown error.", hint: GENERIC_HINT, detail: text };
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

/** The preferred package first; without a preference Vulkan, the one that runs everywhere. */
export function orderPackages(packages: ManualPackage[], preferred: RuntimeVariant | null): ManualPackage[] {
  const first = preferred ?? "vulkan";
  return [...packages].sort((a, b) => Number(b.variant === first) - Number(a.variant === first));
}

/** `C:\Users\<name>\...` -> `%USERPROFILE%\...`: a support text should not carry the user name. */
export function maskUserPaths(text: string): string {
  return text.replace(/[A-Za-z]:[\\/]+Users[\\/]+[^\\/\s'"]+/gi, "%USERPROFILE%");
}

/** Plain text for a support request. No settings, no history, no tokens, no user name. */
export function buildDiagnosticsText(input: {
  diagnostics: SetupDiagnostics | null;
  variant: RuntimeVariant | null;
  error: RuntimeSetupError | null;
  recommendation: RuntimeRecommendation | null;
}): string {
  const { diagnostics, variant, error, recommendation } = input;
  const lines = ["Trispr Flow speech engine diagnostics"];
  if (diagnostics) {
    const installed = diagnostics.variants
      .map((v) => `${v.variant}=${v.bundled ? "bundled" : v.downloaded ? "downloaded" : "missing"}`)
      .join(", ");
    lines.push(
      `App version: ${diagnostics.app_version}`,
      `OS: ${diagnostics.os} (${diagnostics.arch})`,
      `Engine folder: ${diagnostics.runtime_dir}`,
      `whisper.cpp: ${diagnostics.whisper_cpp_version}`,
      `Installed: ${installed || "unknown"}`
    );
  } else {
    lines.push("App version / OS: could not be read");
  }
  lines.push(
    recommendation
      ? `GPU detection: recommends ${recommendation.recommended}; ${recommendation.gpu_name ?? "no GPU name"}; ${recommendation.reason}`
      : "GPU detection: failed"
  );
  if (diagnostics?.adapters.length) {
    lines.push(`Adapters: ${diagnostics.adapters.map((a) => `${a.name} (${a.vendor_id})`).join("; ")}`);
  }
  lines.push(`Engine in question: ${variant ?? "none"}`);
  if (error) {
    lines.push(`Error code: ${error.code}`, `Error: ${error.message}`);
    if (error.detail && error.detail !== error.message) lines.push(`Technical detail: ${error.detail}`);
  } else {
    lines.push("Error: none");
  }
  return maskUserPaths(lines.join("\n"));
}

/** Backend texts that mean "no usable speech engine on disk" (`transcription.rs`, startup preflight). */
export function isRuntimeMissingMessage(text: string): boolean {
  return /whisper (?:cuda |vulkan )?runtime (?:executable )?is (?:missing|incomplete)|whisper-cli not found/i.test(text);
}
