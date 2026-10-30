import * as dom from "./dom-refs";
import { settings, updateSettings } from "./state";
import { persistSettings as saveSettings } from "./settings-persist";
import { showToast } from "./toast";
import { setupHotkeyRecorder } from "./hotkeys";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

let currentStep = 1;
const TOTAL_STEPS = 3;

// Unlisten handles for Tauri events, cleaned up when wizard completes
let unlistenDownloadProgress: (() => void) | null = null;
let unlistenDownloadComplete: (() => void) | null = null;

export async function initOnboardingWizard(): Promise<void> {
  if (!dom.onboardingWizard) return;

  if (settings?.setup?.local_ai_wizard_completed) {
    dom.onboardingWizard.hidden = true;
    return;
  }

  // Await async model check before first render to avoid Step 1 flash
  await checkAndSkipModelStep();

  bindEvents();
  renderStep();

  // Setup hotkey recorder for step 3
  if (dom.wizardHotkeyInput && dom.wizardSetupHotkeyBtn && dom.wizardHotkeyStatus) {
    setupHotkeyRecorder("ptt", dom.wizardHotkeyInput, dom.wizardSetupHotkeyBtn, dom.wizardHotkeyStatus);
  }
}

// Check if a model is already installed; if so, skip step 1
async function checkAndSkipModelStep(): Promise<void> {
  try {
    const models = (await invoke("list_models")) as Array<{ installed: boolean }>;
    if (models && models.some((m) => m.installed)) {
      currentStep = 2;
    }
  } catch (err) {
    console.warn("Could not list models; showing model setup step", err);
  }
}

function bindEvents(): void {
  dom.wizardNextBtn?.addEventListener("click", () => {
    if (currentStep < TOTAL_STEPS) {
      currentStep++;
      renderStep();
    }
  });

  dom.wizardPrevBtn?.addEventListener("click", () => {
    if (currentStep > 1) {
      currentStep--;
      renderStep();
    }
  });

  dom.wizardFinishBtn?.addEventListener("click", () => {
    void finishWizard();
  });

  // Model download button (step 1)
  const downloadBtn = document.getElementById("wizard-download-model-btn");
  if (downloadBtn) {
    downloadBtn.addEventListener("click", () => {
      void startModelDownload();
    });
  }

  // Mode card selection visual feedback (steps 2-3)
  document.querySelectorAll<HTMLLabelElement>(".wizard-card").forEach((card) => {
    const radio = card.querySelector<HTMLInputElement>("input[type=radio]");
    if (radio) {
      radio.addEventListener("change", () => {
        document.querySelectorAll(".wizard-card").forEach((c) => c.classList.remove("selected"));
        if (radio.checked) card.classList.add("selected");
      });
      if (radio.checked) card.classList.add("selected");
    }
  });
}

async function startModelDownload(): Promise<void> {
  const modelRadio = document.querySelector<HTMLInputElement>(
    'input[name="wizard-model"]:checked'
  );
  const modelId = modelRadio?.value || "whisper-large-v3-turbo-q8_0";
  const downloadBtn = document.getElementById("wizard-download-model-btn") as HTMLButtonElement | null;
  const progressDiv = document.getElementById("wizard-download-progress");
  const progressBar = document.getElementById("wizard-download-progress-bar");
  const statusText = document.getElementById("wizard-download-status");

  if (downloadBtn) {
    downloadBtn.disabled = true;
    downloadBtn.textContent = "Wird heruntergeladen...";
  }
  if (progressDiv) progressDiv.style.display = "block";
  if (statusText) statusText.textContent = "Verbinde...";

  // Attach Tauri event listeners before invoking to avoid missing early events
  unlistenDownloadProgress = await listen<{ id: string; downloaded: number; total: number | null }>(
    "model:download-progress",
    (event) => {
      if (event.payload.id !== modelId) return;
      const { downloaded, total } = event.payload;
      if (progressBar && total && total > 0) {
        const pct = Math.round((downloaded / total) * 100);
        progressBar.style.width = `${pct}%`;
      }
      if (statusText) {
        const dlMb = (downloaded / 1024 / 1024).toFixed(0);
        const totalMb = total ? `/ ${(total / 1024 / 1024).toFixed(0)} MB` : "";
        statusText.textContent = `${dlMb} MB ${totalMb}`;
      }
    }
  );

  unlistenDownloadComplete = await listen<{ id: string; path: string }>(
    "model:download-complete",
    async (event) => {
      if (event.payload.id !== modelId) return;

      // Clean up listeners
      unlistenDownloadProgress?.();
      unlistenDownloadComplete?.();
      unlistenDownloadProgress = null;
      unlistenDownloadComplete = null;

      if (progressDiv) progressDiv.style.display = "none";
      if (statusText) statusText.textContent = "✓ Modell heruntergeladen!";
      if (downloadBtn) {
        downloadBtn.disabled = false;
        downloadBtn.textContent = "Erneut herunterladen";
      }

      // Save the chosen model to settings now that download succeeded
      updateSettings({ model: modelId });
      await saveSettings();

      // Enable the Next button
      if (dom.wizardNextBtn) dom.wizardNextBtn.disabled = false;
    }
  );

  try {
    // download_model returns immediately — progress arrives via events above
    await invoke("download_model", { model_id: modelId });
  } catch (err) {
    // Clean up listeners on error
    unlistenDownloadProgress?.();
    unlistenDownloadComplete?.();
    unlistenDownloadProgress = null;
    unlistenDownloadComplete = null;

    console.error("Model download failed:", err);
    if (statusText) statusText.textContent = `Download fehlgeschlagen: ${String(err)}`;
    if (downloadBtn) {
      downloadBtn.disabled = false;
      downloadBtn.textContent = "Erneut versuchen";
    }
    if (progressDiv) progressDiv.style.display = "none";
  }
}

function renderStep(): void {
  for (let i = 1; i <= TOTAL_STEPS; i++) {
    const stepEl = document.getElementById(`wizard-step-${i}`);
    if (stepEl) stepEl.hidden = i !== currentStep;
  }

  if (dom.wizardStepCurrent) {
    dom.wizardStepCurrent.textContent = currentStep.toString();
  }

  if (dom.wizardPrevBtn) dom.wizardPrevBtn.disabled = currentStep === 1;
  if (dom.wizardNextBtn) {
    dom.wizardNextBtn.hidden = currentStep === TOTAL_STEPS;
    // Step 1 requires model download before advancing
    dom.wizardNextBtn.disabled = currentStep === 1;
  }
  if (dom.wizardFinishBtn) dom.wizardFinishBtn.hidden = currentStep !== TOTAL_STEPS;
}

async function finishWizard(): Promise<void> {
  if (!settings) return;

  const modelRadio = document.querySelector<HTMLInputElement>(
    'input[name="wizard-model"]:checked'
  );
  const modelName = modelRadio?.value || "whisper-large-v3-turbo-q8_0";

  const modeRadio = document.querySelector<HTMLInputElement>(
    'input[name="wizard-mode"]:checked'
  );
  const mode = modeRadio?.value === "vad" ? "vad" : "ptt";
  const hotkey = dom.wizardHotkeyInput?.value || "Ctrl+Shift+Space";

  updateSettings({
    setup: {
      ...settings.setup,
      local_ai_wizard_completed: true,
      local_ai_wizard_pending: false,
    },
    model: modelName,
    mode,
    hotkey_ptt: hotkey,
  });
  await saveSettings();

  // Clean up any dangling event listeners
  unlistenDownloadProgress?.();
  unlistenDownloadComplete?.();

  if (dom.onboardingWizard) {
    dom.onboardingWizard.hidden = true;
  }

  const hotkeyDisplay = hotkey.replace(/\+/g, " + ");
  showToast({
    title: "Bereit!",
    message: `Druecke ${hotkeyDisplay} zum Aufnehmen. Expert-Modus oben rechts fuer erweiterte Features.`,
    type: "success",
    duration: 6000,
  });
}

export function showWizard(): void {
  if (dom.onboardingWizard) {
    currentStep = 1;
    renderStep();
    dom.onboardingWizard.hidden = false;
  }
}
