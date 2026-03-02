import * as dom from "./dom-refs";

const EXPERT_MODE_KEY = "trispr-expert-mode";
let initialized = false;

function readExpertModePreference(): boolean {
  try {
    return localStorage.getItem(EXPERT_MODE_KEY) === "1";
  } catch {
    return false;
  }
}

function persistExpertModePreference(enabled: boolean): void {
  try {
    localStorage.setItem(EXPERT_MODE_KEY, enabled ? "1" : "0");
  } catch {
    // Ignore localStorage write failures.
  }
}

function applyExpertModeClass(enabled: boolean): void {
  document.documentElement.classList.toggle("expert-mode", enabled);
  document.documentElement.classList.toggle("standard-mode", !enabled);
  if (dom.expertModeLabel) {
    dom.expertModeLabel.textContent = enabled
      ? "Expert mode active: advanced controls visible."
      : "Standard mode active.";
  }
}

export function initExpertMode(): void {
  if (initialized) return;
  initialized = true;

  const initial = readExpertModePreference();
  applyExpertModeClass(initial);

  if (dom.expertModeToggle) {
    dom.expertModeToggle.checked = initial;
    dom.expertModeToggle.addEventListener("change", () => {
      const enabled = Boolean(dom.expertModeToggle?.checked);
      applyExpertModeClass(enabled);
      persistExpertModePreference(enabled);
    });
  }
}
