import * as dom from "./dom-refs";
import { settings, updateSettings } from "./state";
import { persistSettings as saveSettings } from "./settings";
import { showToast } from "./toast";
import { setupHotkeyRecorder } from "./hotkeys";

let currentStep = 1;
const TOTAL_STEPS = 2;

export function initOnboardingWizard(): void {
  if (!dom.onboardingWizard) return;

  if (settings?.setup?.local_ai_wizard_completed) {
    dom.onboardingWizard.hidden = true;
    return;
  }

  bindEvents();
  renderStep();

  // Setup hotkey recorder for step 2
  if (dom.wizardHotkeyInput && dom.wizardSetupHotkeyBtn && dom.wizardHotkeyStatus) {
    setupHotkeyRecorder("ptt", dom.wizardHotkeyInput, dom.wizardSetupHotkeyBtn, dom.wizardHotkeyStatus);
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

  // Mode card selection visual feedback
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

function renderStep(): void {
  for (let i = 1; i <= TOTAL_STEPS; i++) {
    const stepEl = document.getElementById(`wizard-step-${i}`);
    if (stepEl) stepEl.hidden = i !== currentStep;
  }

  if (dom.wizardStepCurrent) {
    dom.wizardStepCurrent.textContent = currentStep.toString();
  }

  if (dom.wizardPrevBtn) dom.wizardPrevBtn.disabled = currentStep === 1;
  if (dom.wizardNextBtn) dom.wizardNextBtn.hidden = currentStep === TOTAL_STEPS;
  if (dom.wizardFinishBtn) dom.wizardFinishBtn.hidden = currentStep !== TOTAL_STEPS;
}

async function finishWizard(): Promise<void> {
  if (!settings) return;

  // Read wizard choices
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
