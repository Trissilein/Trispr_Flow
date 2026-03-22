import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { settings, updateSettings } from "./state";
import { persistSettings as saveSettings } from "./settings";
import { showToast } from "./toast";
import { ensureLocalRuntimeReady } from "./ollama-models";
import { setupHotkeyRecorder } from "./hotkeys";
import type { HardwareInfo } from "./types";

let currentStep = 1;
const TOTAL_STEPS = 5;
let detectedBackendRecommendation: "auto" | "cuda" | "vulkan" = "auto";

export function initOnboardingWizard(): void {
  if (!dom.onboardingWizard) return;

  // Initial state check
  if (settings?.setup?.local_ai_wizard_completed) {
    dom.onboardingWizard.hidden = true;
    return;
  }

  bindEvents();
  renderStep();
  
  // Setup hotkey recorder for step 3
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

  dom.wizardOllamaEnable?.addEventListener("change", (e) => {
    const enabled = (e.target as HTMLInputElement).checked;
    if (dom.wizardOllamaStatus) {
      dom.wizardOllamaStatus.hidden = !enabled;
    }
  });
}

async function renderStep(): Promise<void> {
  // Hide all steps
  for (let i = 1; i <= TOTAL_STEPS; i++) {
    const stepEl = document.getElementById(`wizard-step-${i}`);
    if (stepEl) stepEl.hidden = true;
  }

  // Show current step
  const currentStepEl = document.getElementById(`wizard-step-${currentStep}`);
  if (currentStepEl) currentStepEl.hidden = false;

  // Update progress
  if (dom.wizardStepCurrent) {
    dom.wizardStepCurrent.textContent = currentStep.toString();
  }

  // Button states
  if (dom.wizardPrevBtn) dom.wizardPrevBtn.disabled = currentStep === 1;
  if (dom.wizardNextBtn) dom.wizardNextBtn.hidden = currentStep === TOTAL_STEPS;
  if (dom.wizardFinishBtn) dom.wizardFinishBtn.hidden = currentStep !== TOTAL_STEPS;

  // Step-specific logic
  if (currentStep === 2) {
    await detectHardware();
  }
}

async function detectHardware(): Promise<void> {
  if (dom.wizardGpuName) dom.wizardGpuName.textContent = "Detecting...";
  if (dom.wizardLoading) dom.wizardLoading.hidden = false;
  if (dom.wizardGpuInfo) dom.wizardGpuInfo.hidden = true;
  
  try {
    // Add a race to avoid hanging the UI if backend command is slow
    const info = await Promise.race([
      invoke<HardwareInfo>("get_hardware_info"),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error("Hardware detection timeout")), 8000))
    ]);
    
    if (dom.wizardGpuName) dom.wizardGpuName.textContent = info.gpu_name;
    if (dom.wizardGpuVram) dom.wizardGpuVram.textContent = info.gpu_vram;
    if (dom.wizardBackendRecommended) {
      dom.wizardBackendRecommended.textContent = info.backend_recommended.toUpperCase();
      dom.wizardBackendRecommended.className = `status-pill status-pill--${info.cuda_available ? "active" : "warning"}`;
    }
    detectedBackendRecommendation =
      info.backend_recommended === "cuda" || info.backend_recommended === "vulkan"
        ? info.backend_recommended
        : "auto";

    if (info.update_url && dom.wizardDriverWarning) {
      dom.wizardDriverWarning.hidden = false;
      if (dom.wizardDriverLink) {
        dom.wizardDriverLink.setAttribute("href", info.update_url);
      }
    }
  } catch (error) {
    console.error("Hardware detection failed:", error);
    if (dom.wizardGpuName) dom.wizardGpuName.textContent = "Detection skipped (Timeout/Error)";
    if (dom.wizardBackendRecommended) {
      dom.wizardBackendRecommended.textContent = "CPU (Fallback)";
      dom.wizardBackendRecommended.className = "status-pill status-pill--warning";
    }
    detectedBackendRecommendation = "auto";
  } finally {
    if (dom.wizardLoading) dom.wizardLoading.hidden = true;
    if (dom.wizardGpuInfo) dom.wizardGpuInfo.hidden = false;
  }
}

async function finishWizard(): Promise<void> {
  if (!settings) return;

  // 1. Update settings based on wizard choices
  const ollamaEnabled = dom.wizardOllamaEnable?.checked ?? false;
  const hotkey = dom.wizardHotkeyInput?.value || "Ctrl+Shift+Space";
  
  const setup = {
    ...settings.setup,
    local_ai_wizard_completed: true,
    local_ai_wizard_pending: false,
  };

  const ai_fallback = {
    ...settings.ai_fallback,
    enabled: ollamaEnabled,
  };

  const moduleSettings = {
    enabled_modules: Array.from(
      new Set(settings.module_settings?.enabled_modules ?? [])
    ).filter((moduleId) => moduleId !== "ai_refinement"),
    consented_permissions: { ...(settings.module_settings?.consented_permissions ?? {}) },
    module_overrides: { ...(settings.module_settings?.module_overrides ?? {}) },
  };
  if (ollamaEnabled) {
    moduleSettings.enabled_modules.push("ai_refinement");
  }
  moduleSettings.module_overrides["ai_refinement.migrated_legacy"] = true;

  updateSettings({ 
    setup, 
    ai_fallback,
    module_settings: moduleSettings,
    hotkey_ptt: hotkey,
    local_backend_preference: detectedBackendRecommendation,
  });
  await saveSettings();

  // 2. Start background tasks if needed
  if (ollamaEnabled) {
    showToast({
      title: "Ollama Setup",
      message: "Starting Ollama download in background...",
      type: "info"
    });
    void ensureLocalRuntimeReady();
  }

  // 3. Close wizard
  if (dom.onboardingWizard) {
    dom.onboardingWizard.hidden = true;
  }

  showToast({
    title: "Setup Complete",
    message: "Welcome to Trispr Flow! Press Ctrl+Shift+Space to start recording.",
    type: "success",
    duration: 5000
  });
}

export function showWizard(): void {
  if (dom.onboardingWizard) {
    currentStep = 1;
    renderStep();
    dom.onboardingWizard.hidden = false;
  }
}
