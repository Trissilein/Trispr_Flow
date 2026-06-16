import * as dom from "./dom-refs";
import { settings, updateSettings } from "./state";
import { persistSettings as saveSettings } from "./settings-persist";
import { showToast } from "./toast";
import { setupHotkeyRecorder } from "./hotkeys";
import { invoke } from "@tauri-apps/api/core";

let currentStep = 1;
const TOTAL_STEPS = 3;

export function initOnboardingWizard(): void {
  if (!dom.onboardingWizard) return;

  if (settings?.setup?.local_ai_wizard_completed) {
    dom.onboardingWizard.hidden = true;
    return;
  }

  bindEvents();
  checkAndSkipModelStep();
  renderStep();

  // Setup hotkey recorder for step 3
  if (dom.wizardHotkeyInput && dom.wizardSetupHotkeyBtn && dom.wizardHotkeyStatus) {
    setupHotkeyRecorder("ptt", dom.wizardHotkeyInput, dom.wizardSetupHotkeyBtn, dom.wizardHotkeyStatus);
  }
}

// Check if a model is already present; if so, skip step 1
async function checkAndSkipModelStep(): Promise<void> {
  try {
    const models = (await invoke("list_models")) as string[];
    if (models && models.length > 0) {
      currentStep = 2; // Skip model download step, go to mode selection
    }
  } catch (err) {
    console.warn("Could not list models; showing model setup step", err);
    // If check fails, show model step anyway
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
      void downloadModelStep();
    });
  }

  // Listen for model download progress/completion events
  window.addEventListener("model:download-progress", (evt: any) => {
    const percent = evt.detail?.percent || 0;
    const progressBar = document.getElementById("wizard-download-progress-bar");
    const statusText = document.getElementById("wizard-download-status");
    if (progressBar) {
      progressBar.style.width = `${percent}%`;
    }
    if (statusText && evt.detail?.size) {
      statusText.textContent = `${evt.detail.downloaded}MB / ${evt.detail.size}MB`;
    }
  });

  window.addEventListener("model:download-complete", (evt: any) => {
    const progressDiv = document.getElementById("wizard-download-progress");
    const statusText = document.getElementById("wizard-download-status");
    const downloadBtn = document.getElementById("wizard-download-model-btn");
    if (progressDiv) progressDiv.style.display = "none";
    if (statusText) statusText.textContent = "Modell erfolgreich heruntergeladen!";
    if (downloadBtn) {
      downloadBtn.disabled = false;
      downloadBtn.textContent = "Modell erneut herunterladen";
      dom.wizardNextBtn!.disabled = false; // Enable next button
    }
  });

  // Mode card selection visual feedback (steps 2-3)
  document.querySelectorAll<HTMLLabelElement>(".wizard-card").forEach((card) => {
    const radio = card.querySelector<HTMLInputElement>("input[type=radio]");
    if (radio) {
      radio.addEventListener("change", () => {
        document.querySelectorAll(".wizard-card").forEach((c) => c.classList.remove("selected"));
        if (radio.checked) card.classList.add("selected");
      });
      // Mark initial selection
      if (radio.checked) card.classList.add("selected");
    }
  });
}

async function downloadModelStep(): Promise<void> {
  const modelRadio = document.querySelector<HTMLInputElement>(
    'input[name="wizard-model"]:checked'
  );
  const modelName = modelRadio?.value || "whisper-large-v3-turbo-q8_0";
  const downloadBtn = document.getElementById("wizard-download-model-btn") as HTMLButtonElement;
  const progressDiv = document.getElementById("wizard-download-progress");
  const statusText = document.getElementById("wizard-download-status");

  if (downloadBtn) {
    downloadBtn.disabled = true;
    downloadBtn.textContent = "Wird heruntergeladen...";
  }
  if (progressDiv) progressDiv.style.display = "block";
  if (statusText) statusText.textContent = "Wird heruntergeladen...";

  try {
    await invoke("download_model", { model: modelName });
    // Model download event will handle UI update
    // Update settings with downloaded model
    updateSettings({ model: modelName });
    await saveSettings();
  } catch (err) {
    console.error("Model download failed:", err);
    if (statusText) statusText.textContent = `Download fehlgeschlagen: ${err}`;
    if (downloadBtn) {
      downloadBtn.disabled = false;
      downloadBtn.textContent = "Erneut versuchen";
    }
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
    // Step 1 (model download) requires model to be present before next is enabled
    if (currentStep === 1) {
      dom.wizardNextBtn.disabled = true; // Enabled only after model download complete
    } else {
      dom.wizardNextBtn.disabled = false;
    }
  }
  if (dom.wizardFinishBtn) dom.wizardFinishBtn.hidden = currentStep !== TOTAL_STEPS;
}

async function finishWizard(): Promise<void> {
  if (!settings) return;

  // Read wizard choices from step 2 & 3
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
