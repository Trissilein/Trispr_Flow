// Voice Output (TTS) wiring (R2 slice 3).
//
// Owns DOM event listeners for the "Voice Output" tab (index.html line ~1888),
// covering the user-facing surface of the `output_voice_tts` Module:
//   - provider chain (default + fallback + output policy)
//   - output device selection
//   - per-provider voice selection (Windows native, fallback, auto-by-language)
//   - global rate / volume sliders
//   - Piper-specific gain and binary/model paths
//   - Qwen3 cloud TTS endpoint/model/voice/api-key/timeout
//   - the "Test" button that probes the active provider
//
// All listeners only mutate `settings.voice_output_settings.*` and persist,
// except the Test button which dispatches to the `test_tts_provider` Tauri
// command. No modal dispatchers in this slice; no shared closures need
// lifting beyond `formatTtsTestError`.
//
// Slice taxonomy: see OQ-3 clause 7 in
// `project-spec/decisions/2026-05-15/refactoring-plan.md`. This file owns
// the CONTEXT.md term "Output Voice TTS".

import { invoke } from "@tauri-apps/api/core";
import type { TtsSpeakResult } from "../types";
import * as dom from "../dom-refs";
import { settings } from "../state";
import {
  persistSettings,
  refreshProviderAvailability,
  refreshProviderVoices,
  handleProviderVoiceSelection,
} from "../settings";

// Formats a Tauri-side error string from `test_tts_provider` for display in
// the inline status line. Private to this slice; not exported.
function formatTtsTestError(error: unknown): string {
  const raw = String(error ?? "Unknown error");
  const normalized = raw.replace(/^Error:\s*/i, "").trim();
  if (normalized.toLowerCase().includes("[tts_output_device_unavailable]")) {
    return `${normalized} Voice Output device was reset to Default. Please retest.`;
  }
  return normalized;
}

export function wireVoiceOutput(): void {
  // ───────── Provider chain + output device ─────────

  dom.voiceOutputDefaultProvider?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.default_provider = dom.voiceOutputDefaultProvider!
      .value as "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
    await refreshProviderAvailability();
    await refreshProviderVoices("default");
    await persistSettings();
  });

  dom.voiceOutputFallbackProvider?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.fallback_provider = dom.voiceOutputFallbackProvider!
      .value as "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
    await refreshProviderAvailability();
    await refreshProviderVoices("fallback");
    await persistSettings();
  });

  dom.voiceOutputPolicy?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings) return;
    settings.voice_output_settings.output_policy = dom.voiceOutputPolicy!
      .value as "agent_replies_only" | "replies_and_events" | "explicit_only";
    await persistSettings();
  });

  dom.voiceOutputDeviceSelect?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputDeviceSelect) return;
    settings.voice_output_settings.output_device = dom.voiceOutputDeviceSelect.value || "default";
    await persistSettings();
  });

  // ───────── Per-provider voice selection ─────────

  dom.voiceOutputWindowsVoiceSelect?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputWindowsVoiceSelect) return;
    await handleProviderVoiceSelection("default");
  });

  dom.voiceOutputFallbackVoiceSelect?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputFallbackVoiceSelect) return;
    await handleProviderVoiceSelection("fallback");
  });

  dom.voiceOutputAutoLanguageVoice?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputAutoLanguageVoice) return;
    settings.voice_output_settings.auto_voice_by_detected_language = dom.voiceOutputAutoLanguageVoice.checked;
    await persistSettings();
  });

  // ───────── Rate / Volume sliders (live preview on input, persist on change) ─────────

  dom.voiceOutputRate?.addEventListener("input", () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputRate) return;
    const rate = parseFloat(dom.voiceOutputRate.value);
    settings.voice_output_settings.rate = rate;
    if (dom.voiceOutputRateValue) {
      dom.voiceOutputRateValue.textContent = rate.toFixed(2);
    }
  });

  dom.voiceOutputRate?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputRate) return;
    settings.voice_output_settings.rate = parseFloat(dom.voiceOutputRate.value);
    await persistSettings();
  });

  dom.voiceOutputVolume?.addEventListener("input", () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputVolume) return;
    const volume = parseFloat(dom.voiceOutputVolume.value);
    settings.voice_output_settings.volume = volume;
    if (dom.voiceOutputVolumeValue) {
      dom.voiceOutputVolumeValue.textContent = volume.toFixed(2);
    }
  });

  dom.voiceOutputVolume?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputVolume) return;
    settings.voice_output_settings.volume = parseFloat(dom.voiceOutputVolume.value);
    await persistSettings();
  });

  // ───────── Piper gain (clamped [-24, 6] dB) ─────────

  dom.voiceOutputPiperGainDb?.addEventListener("input", () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperGainDb) return;
    const parsed = Number.parseInt(dom.voiceOutputPiperGainDb.value, 10);
    settings.voice_output_settings.piper_gain_db = Number.isFinite(parsed)
      ? Math.max(-24, Math.min(6, parsed))
      : -12;
    if (dom.voiceOutputPiperGainDbValue) {
      dom.voiceOutputPiperGainDbValue.textContent = `${settings.voice_output_settings.piper_gain_db} dB`;
    }
  });

  dom.voiceOutputPiperGainDb?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperGainDb) return;
    const parsed = Number.parseInt(dom.voiceOutputPiperGainDb.value, 10);
    settings.voice_output_settings.piper_gain_db = Number.isFinite(parsed)
      ? Math.max(-24, Math.min(6, parsed))
      : -12;
    dom.voiceOutputPiperGainDb.value = String(settings.voice_output_settings.piper_gain_db);
    if (dom.voiceOutputPiperGainDbValue) {
      dom.voiceOutputPiperGainDbValue.textContent = `${settings.voice_output_settings.piper_gain_db} dB`;
    }
    await persistSettings();
  });

  // ───────── Test button (probes the configured provider end-to-end) ─────────

  dom.voiceOutputTestBtn?.addEventListener("click", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputTestBtn || !dom.voiceOutputTestStatus) return;
    const provider = settings.voice_output_settings.default_provider;
    dom.voiceOutputTestStatus.textContent = "Testing…";
    try {
      const result = await invoke<TtsSpeakResult>("test_tts_provider", { provider });
      const message = (result.message ?? "").trim();
      if (result.used_fallback) {
        const preferred = result.preferred_provider || provider;
        dom.voiceOutputTestStatus.textContent = message
          ? `⚠ ${message}`
          : `⚠ Fallback used: ${preferred} -> ${result.provider_used}`;
      } else {
        dom.voiceOutputTestStatus.textContent = message
          ? `✓ ${message}`
          : `✓ ${result.provider_used} responded.`;
      }
      dom.voiceOutputTestStatus.title = message;
    } catch (e) {
      const formatted = formatTtsTestError(e);
      dom.voiceOutputTestStatus.textContent = `Error: ${formatted}`;
      dom.voiceOutputTestStatus.title = formatted;
      console.error("Voice output test failed:", formatted);
    }
  });

  // ───────── Piper paths (binary / model file / model dir) ─────────

  dom.voiceOutputPiperBinary?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperBinary) return;
    settings.voice_output_settings.piper_binary_path = dom.voiceOutputPiperBinary.value;
    await persistSettings();
  });

  dom.voiceOutputPiperModel?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperModel) return;
    settings.voice_output_settings.piper_model_path = dom.voiceOutputPiperModel.value;
    await persistSettings();
    await refreshProviderVoices("default");
    await refreshProviderVoices("fallback");
  });

  dom.voiceOutputPiperModelDir?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputPiperModelDir) return;
    settings.voice_output_settings.piper_model_dir = dom.voiceOutputPiperModelDir.value;
    await persistSettings();
    await refreshProviderVoices("default");
    await refreshProviderVoices("fallback");
  });

  // ───────── Qwen3 cloud TTS configuration ─────────

  dom.voiceOutputQwenEndpoint?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenEndpoint) return;
    settings.voice_output_settings.qwen3_tts_endpoint = dom.voiceOutputQwenEndpoint.value;
    await persistSettings();
  });

  dom.voiceOutputQwenModel?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenModel) return;
    settings.voice_output_settings.qwen3_tts_model = dom.voiceOutputQwenModel.value;
    await persistSettings();
  });

  dom.voiceOutputQwenVoice?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenVoice) return;
    settings.voice_output_settings.qwen3_tts_voice = dom.voiceOutputQwenVoice.value;
    await persistSettings();
  });

  dom.voiceOutputQwenApiKey?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenApiKey) return;
    settings.voice_output_settings.qwen3_tts_api_key = dom.voiceOutputQwenApiKey.value;
    await persistSettings();
  });

  dom.voiceOutputQwenTimeoutSec?.addEventListener("change", async () => {
    if (!settings?.voice_output_settings || !dom.voiceOutputQwenTimeoutSec) return;
    const parsed = Number.parseInt(dom.voiceOutputQwenTimeoutSec.value, 10);
    settings.voice_output_settings.qwen3_tts_timeout_sec = Number.isFinite(parsed)
      ? Math.max(3, Math.min(180, parsed))
      : 45;
    dom.voiceOutputQwenTimeoutSec.value = String(settings.voice_output_settings.qwen3_tts_timeout_sec);
    await persistSettings();
  });
}
