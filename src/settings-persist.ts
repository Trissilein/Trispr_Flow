import { invoke } from "@tauri-apps/api/core";
import { settings } from "./state";
import {
  normalizePersistedRefinementPromptPresetId,
  normalizeUserRefinementPromptPresets,
} from "./refinement-prompts";

export async function persistSettings() {
  if (!settings) return;
  const aiFallback = settings.ai_fallback;
  const settingsForSave = {
    ...settings,
    ai_fallback: aiFallback ? { ...aiFallback } : aiFallback,
  };
  if (settingsForSave.ai_fallback) {
    settingsForSave.ai_fallback.prompt_presets = normalizeUserRefinementPromptPresets(
      settingsForSave.ai_fallback.prompt_presets
    );
    settingsForSave.ai_fallback.active_prompt_preset_id = normalizePersistedRefinementPromptPresetId(
      settingsForSave.ai_fallback.active_prompt_preset_id,
      settingsForSave.ai_fallback.prompt_profile,
      settingsForSave.ai_fallback.prompt_presets
    );
  }
  try {
    await Promise.race([
      invoke("save_settings", { settings: settingsForSave }),
      new Promise<void>((_, reject) =>
        setTimeout(() => reject(new Error("save_settings timed out")), 3_000)
      ),
    ]);
  } catch (error) {
    console.error("save_settings failed", error);
  }
}
