import * as dom from "./dom-refs";
import { settings, updateSettings } from "./state";
import { persistSettings as saveSettings } from "./settings-persist";
import { showToast } from "./toast";
import { setupHotkeyRecorder } from "./hotkeys";
import { setupRefinementInBackground } from "./ollama-models";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  REFINEMENT_MODEL,
  buildWizardSteps,
  describeProgress,
  describeRecommendation,
  formatMegabytes,
  planAfterSelfTest,
  variantStatus,
  type RuntimeRecommendation,
  type RuntimeVariant,
  type RuntimeVerifyResult,
  type WhisperRuntimeProgress,
  type WhisperRuntimeStatus,
  type WizardStepId,
} from "./onboarding-wizard-logic";

const VARIANT_LABEL: Record<RuntimeVariant, string> = { cuda: "CUDA", vulkan: "Vulkan" };

let steps: WizardStepId[] = [];
let stepIndex = 0;

let runtimeStatus: WhisperRuntimeStatus | null = null;
let runtimeReady = false;
let runtimeBusy = false;

// The model download keeps running while the user moves on to the mode and hotkey steps.
let modelStarted = false;
let modelBusy = false;
let modelProgressText = "";

// Unlisten handles for Tauri events, cleaned up when the wizard completes
const unlisteners: UnlistenFn[] = [];

function byId<T extends HTMLElement = HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}

function currentStep(): WizardStepId | undefined {
  return steps[stepIndex];
}

export async function initOnboardingWizard(): Promise<void> {
  if (!dom.onboardingWizard) return;

  // Await the async checks before the first render to avoid a flash of the wrong step
  const [status, modelInstalled] = await Promise.all([loadRuntimeStatus(), hasInstalledModel()]);
  runtimeStatus = status;
  // Status unknown (command failed): do not block the app on a step that cannot work.
  runtimeReady = status ? status.any_ready : true;
  steps = buildWizardSteps({
    runtimeReady,
    modelInstalled,
    completed: Boolean(settings?.setup?.local_ai_wizard_completed),
  });
  if (steps.length === 0) {
    dom.onboardingWizard.hidden = true;
    return;
  }

  stepIndex = 0;
  bindEvents();
  if (steps.includes("runtime")) void prepareRuntimeStep();
  renderStep();
  dom.onboardingWizard.hidden = false;

  // Setup hotkey recorder for the hotkey step
  if (dom.wizardHotkeyInput && dom.wizardSetupHotkeyBtn && dom.wizardHotkeyStatus) {
    setupHotkeyRecorder("ptt", dom.wizardHotkeyInput, dom.wizardSetupHotkeyBtn, dom.wizardHotkeyStatus);
  }
}

async function loadRuntimeStatus(): Promise<WhisperRuntimeStatus | null> {
  try {
    return await invoke<WhisperRuntimeStatus>("get_whisper_runtime_status");
  } catch (err) {
    console.warn("Could not read the speech engine status", err);
    return null;
  }
}

async function hasInstalledModel(): Promise<boolean> {
  try {
    const models = (await invoke("list_models")) as Array<{ installed: boolean }>;
    return Boolean(models && models.some((m) => m.installed));
  } catch (err) {
    console.warn("Could not list models; assuming none is installed", err);
    return false;
  }
}

function bindEvents(): void {
  dom.wizardNextBtn?.addEventListener("click", () => {
    if (stepIndex < steps.length - 1) {
      stepIndex++;
      renderStep();
    }
  });

  dom.wizardPrevBtn?.addEventListener("click", () => {
    if (stepIndex > 0) {
      stepIndex--;
      renderStep();
    }
  });

  dom.wizardFinishBtn?.addEventListener("click", () => {
    void finishWizard();
  });

  byId("wizard-runtime-btn")?.addEventListener("click", () => {
    const picked = document.querySelector<HTMLInputElement>('input[name="wizard-engine"]:checked');
    void runRuntimeSetup(picked?.value === "cuda" ? "cuda" : "vulkan");
  });

  byId("wizard-download-model-btn")?.addEventListener("click", () => {
    void startModelDownload();
  });

  // Card selection visual feedback, per radio group
  document.querySelectorAll<HTMLLabelElement>(".wizard-card").forEach((card) => {
    const radio = card.querySelector<HTMLInputElement>("input[type=radio]");
    if (!radio) return;
    const markGroup = () => {
      document
        .querySelectorAll<HTMLInputElement>(`input[type=radio][name="${radio.name}"]`)
        .forEach((other) => other.closest(".wizard-card")?.classList.toggle("selected", other.checked));
    };
    radio.addEventListener("change", markGroup);
    markGroup();
  });
}

// ---------------------------------------------------------------------------------------
// Step: speech engine
// ---------------------------------------------------------------------------------------

async function prepareRuntimeStep(): Promise<void> {
  let recommendation: RuntimeRecommendation | null = null;
  try {
    recommendation = await invoke<RuntimeRecommendation>("detect_recommended_runtime");
  } catch (err) {
    console.warn("GPU detection failed", err);
  }

  const gpuLine = byId("wizard-gpu-line");
  if (gpuLine) gpuLine.textContent = describeRecommendation(recommendation);

  for (const variant of ["cuda", "vulkan"] as RuntimeVariant[]) {
    const size = variantStatus(runtimeStatus, variant)?.zip_size;
    const sizeEl = byId(`wizard-engine-size-${variant}`);
    if (size && sizeEl) sizeEl.textContent = `~${formatMegabytes(size)}`;
  }

  const wanted: RuntimeVariant = recommendation?.recommended ?? "vulkan";
  const radio = document.querySelector<HTMLInputElement>(`input[name="wizard-engine"][value="${wanted}"]`);
  if (radio) {
    radio.checked = true;
    radio.dispatchEvent(new Event("change"));
    const title = radio.closest(".wizard-card")?.querySelector("strong");
    if (title) title.textContent = `${title.textContent ?? ""} (recommended)`;
  }
}

function setRuntimeUi(state: "idle" | "running" | "error" | "ready", text: string, percent: number | null = null): void {
  const button = byId<HTMLButtonElement>("wizard-runtime-btn");
  const box = byId("wizard-runtime-progress");
  const bar = byId("wizard-runtime-progress-bar");
  const status = byId("wizard-runtime-status");
  if (button) {
    button.disabled = state === "running" || state === "ready";
    button.hidden = state === "ready";
    button.textContent = state === "error" ? "Retry" : "Download speech engine";
  }
  document
    .querySelectorAll<HTMLInputElement>('input[name="wizard-engine"]')
    .forEach((radio) => (radio.disabled = state === "running" || state === "ready"));
  if (box) box.hidden = state !== "running";
  if (bar) bar.style.width = percent === null ? "100%" : `${percent}%`;
  if (bar) bar.classList.toggle("wizard-progress-fill--indeterminate", state === "running" && percent === null);
  if (status) status.textContent = text;
}

/**
 * Downloads and tests one engine. Resolves with the engine to try next when CUDA does not work
 * on this PC; throws with a readable message on any other failure.
 */
async function attemptRuntime(variant: RuntimeVariant): Promise<{ fallback: RuntimeVariant } | null> {
  const label = VARIANT_LABEL[variant];
  setRuntimeUi("running", `Downloading the ${label} speech engine...`);
  const unlisten = await listen<WhisperRuntimeProgress>("whisper-runtime:progress", (event) => {
    if (event.payload.variant !== variant) return;
    const { percent, text } = describeProgress(
      event.payload.downloaded,
      event.payload.total,
      event.payload.message
    );
    setRuntimeUi("running", `${event.payload.message} ${percent === null ? "" : text}`.trim(), percent);
  });
  try {
    runtimeStatus = await invoke<WhisperRuntimeStatus>("download_whisper_runtime", { variant });
  } finally {
    unlisten();
  }

  setRuntimeUi("running", `Testing the ${label} speech engine...`);
  const result = await invoke<RuntimeVerifyResult>("verify_whisper_runtime", { variant });
  const next = planAfterSelfTest(result, runtimeStatus);
  if (next.action === "failed") throw new Error(next.message);
  if (next.action === "ready") return null;

  // CUDA does not run here (old driver, no usable GPU): use Vulkan, and keep it that way.
  updateSettings({ local_backend_preference: "vulkan" });
  await saveSettings();
  setRuntimeUi(
    "running",
    `CUDA does not work on this PC: ${result.detail} Using Vulkan instead.`
  );
  return next.action === "switch" ? null : { fallback: next.variant };
}

async function runRuntimeSetup(first: RuntimeVariant): Promise<void> {
  if (runtimeBusy) return;
  runtimeBusy = true;
  let variant = first;
  try {
    for (;;) {
      const outcome = await attemptRuntime(variant);
      if (!outcome) break;
      variant = outcome.fallback;
    }
    runtimeReady = true;
    setRuntimeUi("ready", `Speech engine ready (${VARIANT_LABEL[variant]}).`);
  } catch (err) {
    console.error("Speech engine setup failed", err);
    setRuntimeUi("error", `Speech engine setup failed: ${errorText(err)}`);
  } finally {
    runtimeBusy = false;
    renderStep();
  }
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// ---------------------------------------------------------------------------------------
// Step: speech model
// ---------------------------------------------------------------------------------------

function selectedModelId(): string {
  const modelRadio = document.querySelector<HTMLInputElement>('input[name="wizard-model"]:checked');
  return modelRadio?.value || "whisper-large-v3-turbo";
}

function setModelUi(opts: { button?: string; disabled?: boolean; status: string; percent?: number | null }): void {
  const button = byId<HTMLButtonElement>("wizard-download-model-btn");
  const box = byId("wizard-download-progress");
  const bar = byId("wizard-download-progress-bar");
  const status = byId("wizard-download-status");
  if (button) {
    if (opts.button) button.textContent = opts.button;
    button.disabled = Boolean(opts.disabled);
  }
  if (box) box.hidden = opts.percent === undefined;
  if (bar && opts.percent !== undefined) {
    bar.style.width = opts.percent === null ? "100%" : `${opts.percent}%`;
    bar.classList.toggle("wizard-progress-fill--indeterminate", opts.percent === null);
  }
  if (status) status.textContent = opts.status;
}

function releaseModelListeners(): void {
  while (unlisteners.length) unlisteners.pop()?.();
}

async function startModelDownload(): Promise<void> {
  if (modelBusy) return;
  const modelId = selectedModelId();
  modelBusy = true;
  setModelUi({ button: "Downloading...", disabled: true, status: "Connecting...", percent: null });

  // Attach Tauri event listeners before invoking to avoid missing early events
  unlisteners.push(
    await listen<{ id: string; downloaded: number; total: number | null }>("model:download-progress", (event) => {
      if (event.payload.id !== modelId) return;
      const { percent, text } = describeProgress(event.payload.downloaded, event.payload.total, "Downloading...");
      modelProgressText = percent === null ? text : `${percent}%`;
      setModelUi({ button: "Downloading...", disabled: true, status: text, percent });
      renderBackgroundDownloads();
    }),
    await listen<{ id: string; path: string }>("model:download-complete", async (event) => {
      if (event.payload.id !== modelId) return;
      modelBusy = false;
      modelProgressText = "";
      setModelUi({ button: "Download again", status: "Model downloaded." });
      renderBackgroundDownloads();

      // Save the chosen model to settings now that download succeeded
      updateSettings({ model: modelId });
      await saveSettings();
    }),
    await listen<{ id: string; error: string }>("model:download-error", (event) => {
      if (event.payload.id !== modelId) return;
      modelBusy = false;
      modelStarted = false;
      modelProgressText = "";
      setModelUi({ button: "Retry", status: `Download failed: ${event.payload.error}` });
      renderStep();
    })
  );

  try {
    // download_model returns immediately — progress arrives via events above
    await invoke("download_model", { model_id: modelId });
    modelStarted = true;
    renderStep();
  } catch (err) {
    releaseModelListeners();
    modelBusy = false;
    console.error("Model download failed:", err);
    setModelUi({ button: "Retry", status: `Download failed: ${errorText(err)}` });
  }
}

/** Download status shown on the other steps while the model keeps loading. */
function renderBackgroundDownloads(): void {
  const strip = byId("wizard-downloads");
  if (!strip) return;
  const text = currentStep() !== "model" && modelBusy ? `Speech model downloading: ${modelProgressText || "starting..."}` : "";
  strip.textContent = text;
  strip.hidden = text === "";
}

// ---------------------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------------------

function renderStep(): void {
  const active = currentStep();
  for (const id of ["runtime", "model", "mode", "hotkey", "refinement"] as WizardStepId[]) {
    const stepEl = byId(`wizard-step-${id}`);
    if (stepEl) stepEl.hidden = id !== active;
  }

  if (dom.wizardStepCurrent) dom.wizardStepCurrent.textContent = String(stepIndex + 1);
  const total = byId("wizard-step-total");
  if (total) total.textContent = String(steps.length);

  const isLast = stepIndex === steps.length - 1;
  if (dom.wizardPrevBtn) dom.wizardPrevBtn.disabled = stepIndex === 0;
  // The engine must be installed to go on; the model only has to be on its way.
  const blocked = (active === "runtime" && !runtimeReady) || (active === "model" && !modelStarted);
  if (dom.wizardNextBtn) {
    dom.wizardNextBtn.hidden = isLast;
    dom.wizardNextBtn.disabled = blocked;
  }
  if (dom.wizardFinishBtn) {
    dom.wizardFinishBtn.hidden = !isLast;
    dom.wizardFinishBtn.disabled = blocked;
  }
  renderBackgroundDownloads();
}

async function finishWizard(): Promise<void> {
  if (!settings) return;

  const patch: Parameters<typeof updateSettings>[0] = {
    setup: {
      ...settings.setup,
      local_ai_wizard_completed: true,
      local_ai_wizard_pending: false,
    },
  };
  // Only what the user was asked about; a reopened wizard (speech engine repair) keeps the rest.
  if (steps.includes("model")) patch.model = selectedModelId();
  if (steps.includes("mode")) {
    const modeRadio = document.querySelector<HTMLInputElement>('input[name="wizard-mode"]:checked');
    patch.mode = modeRadio?.value === "vad" ? "vad" : "ptt";
  }
  if (steps.includes("hotkey")) patch.hotkey_ptt = dom.wizardHotkeyInput?.value || "Ctrl+Shift+Space";
  const setupRefinement =
    steps.includes("refinement") && Boolean(byId<HTMLInputElement>("wizard-refinement-check")?.checked);
  if (steps.includes("refinement")) patch.ai_fallback = { ...settings.ai_fallback, enabled: setupRefinement };

  updateSettings(patch);
  await saveSettings();

  // Downloads in flight (model) continue without the wizard; its own listeners are done.
  releaseModelListeners();
  if (dom.onboardingWizard) {
    dom.onboardingWizard.hidden = true;
  }

  if (steps.includes("hotkey")) {
    const hotkeyDisplay = String(patch.hotkey_ptt).replace(/\+/g, " + ");
    showToast({
      title: "Ready!",
      message: `Press ${hotkeyDisplay} to record. Turn on Expert mode (top right) for advanced features.`,
      type: "success",
      duration: 6000,
    });
  }

  // Ollama + gemma4:e4b: install, start, pull. Progress shows in the download popup.
  if (setupRefinement) void setupRefinementInBackground(REFINEMENT_MODEL);
}
