import * as dom from "./dom-refs";
import { settings } from "./state";

function isModuleEnabled(moduleId: string): boolean {
  return settings?.module_settings?.enabled_modules?.includes(moduleId) ?? false;
}

function isVoiceOutputEnabled(): boolean {
  return isModuleEnabled("output_voice_tts") && (settings?.voice_output_settings?.enabled ?? false);
}

function renderStatus(): void {
  if (!dom.voiceOutputConsole) return;
  const enabled = isVoiceOutputEnabled();
  dom.voiceOutputConsole.hidden = !enabled;
  if (dom.voiceOutputConsoleStatus) {
    dom.voiceOutputConsoleStatus.textContent = enabled
      ? "Voice output active."
      : "Module disabled.";
  }
}

export function focusVoiceOutputConsole(): void {
  renderStatus();
  dom.voiceOutputConsole?.scrollIntoView({ behavior: "smooth", block: "start" });
  dom.voiceOutputDefaultProvider?.focus();
}

export function syncVoiceOutputConsoleState(): void {
  renderStatus();
}
