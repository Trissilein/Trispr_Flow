import * as dom from "../dom-refs";
import { settings } from "../state";
import { VAD_DB_FLOOR, formatHotkeyForDisplay, thresholdToDb } from "../ui-helpers";

// Transcription settings rendering (R3 slice 3).
//
// Renders capture + ASR-language + VAD/transcribe controls in the Settings UI.
//
// Exports:
//   Primary   - renderTranscriptionSettings()      called by renderSettings() in index.ts
//   Secondary - updateTranscribeVadVisibility()    called by transcription.wire.ts
//               updateTranscribeThreshold()        called by transcription.wire.ts
//               syncCaptureModeVisibility()        called by transcription.wire.ts
//               resolveEffectiveAsrLanguageHint()  called by ai-refinement.wire.ts
//               derivePostprocLanguageFromAsr()    called by ai-refinement.wire.ts and settings-persist.ts
//
// Per Decision 6 (settings-decomposition.md): all other functions are private.
// syncDerivedLanguageSettings() moved to settings-persist.ts during S5 to avoid index.ts dependency cycles.
// productModeHotkey stays owned by this slice (capture/hotkey UI block cohesion).

export function updateTranscribeVadVisibility(enabled: boolean): void {
    if (dom.transcribeMeterThreshold) {
        dom.transcribeMeterThreshold.style.display = enabled ? "block" : "none";
    }
    if (dom.transcribeThresholdLabel) {
        dom.transcribeThresholdLabel.style.display = enabled ? "block" : "none";
    }
}

export function updateTranscribeThreshold(threshold: number): void {
    const db = thresholdToDb(threshold, VAD_DB_FLOOR);
    if (dom.transcribeThresholdDb) {
        dom.transcribeThresholdDb.textContent = `${db.toFixed(1)} dB`;
    }
    if (dom.transcribeMeterThreshold) {
        const pos = (db - VAD_DB_FLOOR) / (0 - VAD_DB_FLOOR);
        dom.transcribeMeterThreshold.style.left = `${Math.round(pos * 100)}%`;
    }
}

function normalizeLanguageModeValue(languageMode: string | null | undefined): string {
    const normalized = (languageMode || "auto").trim().toLowerCase();
    if (!normalized) return "auto";
    return normalized;
}

export function resolveEffectiveAsrLanguageHint(
    languageMode: string | null | undefined,
    languagePinned: boolean | null | undefined
): string {
    const normalized = normalizeLanguageModeValue(languageMode);
    return languagePinned ? normalized : "auto";
}

export function derivePostprocLanguageFromAsr(
    languageMode: string | null | undefined,
    languagePinned: boolean | null | undefined
): "en" | "de" | "multi" {
    if (!languagePinned) return "multi";
    const normalized = normalizeLanguageModeValue(languageMode);
    if (normalized === "en") return "en";
    if (normalized === "de") return "de";
    return "multi";
}

export function syncCaptureModeVisibility(mode: string, pttUseVad = false): void {
    const hotkeysEnabled = mode === "ptt";
    const vadEnabled = mode === "vad" || (mode === "ptt" && pttUseVad);
    if (dom.hotkeysBlock) dom.hotkeysBlock.classList.toggle("hidden", !hotkeysEnabled);
    if (dom.vadBlock) dom.vadBlock.classList.toggle("hidden", !vadEnabled);
    // In PTT+VAD mode we only use threshold gating while the key is held.
    // Silence grace is VAD-mode specific and should not appear for PTT.
    const vadSilenceField = dom.vadSilence?.closest(".field");
    if (vadSilenceField) {
        vadSilenceField.classList.toggle("hidden", mode === "ptt");
    }
}

function syncAsrLanguageHintUi(): void {
    if (!settings) return;
    const pinned = Boolean(settings.language_pinned);
    if (dom.languageSelect) {
        dom.languageSelect.disabled = !pinned;
        dom.languageSelect.setAttribute("aria-disabled", String(!pinned));
    }
    if (dom.asrLanguageField) {
        dom.asrLanguageField.classList.toggle("is-disabled", !pinned);
    }
    if (dom.asrLanguageHintNote) {
        dom.asrLanguageHintNote.textContent = pinned
            ? "Pinned: ASR is locked to the selected language."
            : "Auto-detect is active. Enable pinning to lock a specific ASR language.";
    }
    if (dom.whisperInputLanguageSelect) {
        dom.whisperInputLanguageSelect.value = pinned ? settings.language_mode : "auto";
    }
    if (dom.whisperInputLanguageNote) {
        dom.whisperInputLanguageNote.textContent = pinned
            ? "Language is pinned. Short clips skip auto-detect for lower Whisper latency."
            : "Multi uses Whisper auto-detect. Pin a language for lower latency on short clips.";
    }
}

export function renderTranscriptionSettings(): void {
    if (!settings) return;

    if (dom.captureEnabledToggle) dom.captureEnabledToggle.checked = settings.capture_enabled;
    if (dom.transcribeEnabledToggle) dom.transcribeEnabledToggle.checked = settings.transcribe_enabled;
    if (dom.modeSelect) dom.modeSelect.value = settings.mode;
    if (dom.pttHotkey) dom.pttHotkey.value = formatHotkeyForDisplay(settings.hotkey_ptt);
    if (dom.toggleHotkey) dom.toggleHotkey.value = formatHotkeyForDisplay(settings.hotkey_toggle);
    syncCaptureModeVisibility(settings.mode, settings.ptt_use_vad);
    if (dom.deviceSelect) dom.deviceSelect.value = settings.input_device;
    if (dom.languageSelect) dom.languageSelect.value = settings.language_mode;
    if (dom.languagePinnedToggle) dom.languagePinnedToggle.checked = settings.language_pinned;
    syncAsrLanguageHintUi();
    if (dom.modelSourceSelect) dom.modelSourceSelect.value = settings.model_source;
    if (dom.modelCustomUrl) dom.modelCustomUrl.value = settings.model_custom_url ?? "";
    if (dom.modelStoragePath && settings.model_storage_dir) {
        dom.modelStoragePath.value = settings.model_storage_dir;
    }
    if (dom.modelCustomUrlField) {
        dom.modelCustomUrlField.classList.toggle("hidden", settings.model_source !== "custom");
    }
    if (dom.audioCuesToggle) dom.audioCuesToggle.checked = settings.audio_cues;
    if (dom.pttUseVadToggle) dom.pttUseVadToggle.checked = settings.ptt_use_vad;
    if (dom.diagnosticLoggingToggle) {
        dom.diagnosticLoggingToggle.checked = settings.diagnostic_logging_enabled === true;
    }
    if (dom.pttHotKeepalive) {
        dom.pttHotKeepalive.value = String(settings.ptt_hot_keepalive_ms ?? 30000);
    }
    if (dom.pttHotKeepaliveValue) {
        dom.pttHotKeepaliveValue.textContent = `${Math.round((settings.ptt_hot_keepalive_ms ?? 30000) / 1000)}s`;
    }
    if (dom.audioCuesVolume) dom.audioCuesVolume.value = Math.round(settings.audio_cues_volume * 100).toString();
    if (dom.audioCuesVolumeValue) {
        dom.audioCuesVolumeValue.textContent = `${Math.round(settings.audio_cues_volume * 100)}%`;
    }
    if (dom.hallucinationFilterToggle) {
        dom.hallucinationFilterToggle.checked = settings.hallucination_filter_enabled;
    }
    if (dom.activationWordsToggle) {
        dom.activationWordsToggle.checked = settings.activation_words_enabled;
    }
    if (dom.activationWordsList) {
        dom.activationWordsList.value = settings.activation_words.join("\n");
    }
    if (dom.activationWordsConfig) {
        dom.activationWordsConfig.classList.toggle("hidden", !settings.activation_words_enabled);
    }
    if (dom.micGain) dom.micGain.value = Math.round(settings.mic_input_gain_db).toString();
    if (dom.micGainValue) {
        const gain = Math.round(settings.mic_input_gain_db);
        dom.micGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
    }
    // Display start threshold in dB (main user-facing threshold)
    const vadThresholdDb = thresholdToDb(settings.vad_threshold_start, VAD_DB_FLOOR);
    if (dom.vadThreshold) dom.vadThreshold.value = Math.round(vadThresholdDb).toString();
    if (dom.vadThresholdValue) dom.vadThresholdValue.textContent = `${Math.round(vadThresholdDb)} dB`;
    if (dom.vadSilence) dom.vadSilence.value = settings.vad_silence_ms.toString();
    if (dom.vadSilenceValue) dom.vadSilenceValue.textContent = `${settings.vad_silence_ms} ms`;
    if (dom.transcribeHotkey) dom.transcribeHotkey.value = formatHotkeyForDisplay(settings.transcribe_hotkey);
    if (dom.toggleActivationWordsHotkey) {
        dom.toggleActivationWordsHotkey.value = formatHotkeyForDisplay(settings.hotkey_toggle_activation_words);
    }
    if (dom.productModeHotkey) {
        dom.productModeHotkey.value = formatHotkeyForDisplay(
            settings.hotkey_product_mode_toggle || "CommandOrControl+Shift+P"
        );
    }
    if (dom.transcribeDeviceSelect) {
        dom.transcribeDeviceSelect.value = settings.transcribe_output_device;
        // If the stored device ID is not present in the current option list, the browser
        // silently leaves the dropdown on "Default (System)" (value = "default").
        // Sync the settings object so the next persistSettings() sends the actual value.
        if (dom.transcribeDeviceSelect.value !== settings.transcribe_output_device) {
            settings.transcribe_output_device = dom.transcribeDeviceSelect.value;
        }
    }
    if (dom.transcribeVadToggle) dom.transcribeVadToggle.checked = settings.transcribe_vad_mode;
    const transcribeThresholdDb = thresholdToDb(settings.transcribe_vad_threshold, VAD_DB_FLOOR);
    if (dom.transcribeVadThreshold) {
        dom.transcribeVadThreshold.value = Math.round(transcribeThresholdDb).toString();
    }
    if (dom.transcribeVadThresholdValue) {
        dom.transcribeVadThresholdValue.textContent = `${Math.round(transcribeThresholdDb)} dB`;
    }
    if (dom.transcribeVadSilence) {
        dom.transcribeVadSilence.value = settings.transcribe_vad_silence_ms.toString();
    }
    if (dom.transcribeVadSilenceValue) {
        dom.transcribeVadSilenceValue.textContent = `${Math.round(settings.transcribe_vad_silence_ms / 100) / 10}s`;
    }
    updateTranscribeThreshold(settings.transcribe_vad_threshold);
    updateTranscribeVadVisibility(settings.transcribe_vad_mode);
    if (dom.transcribeBatchInterval) {
        dom.transcribeBatchInterval.value = settings.transcribe_batch_interval_ms.toString();
    }
    if (dom.transcribeBatchValue) {
        dom.transcribeBatchValue.textContent = `${Math.round(settings.transcribe_batch_interval_ms / 1000)}s`;
    }
    if (dom.transcribeChunkOverlap) {
        dom.transcribeChunkOverlap.value = settings.transcribe_chunk_overlap_ms.toString();
    }
    if (dom.transcribeOverlapValue) {
        dom.transcribeOverlapValue.textContent = `${(settings.transcribe_chunk_overlap_ms / 1000).toFixed(1)}s`;
    }
    if (dom.transcribeGain) {
        dom.transcribeGain.value = Math.round(settings.transcribe_input_gain_db).toString();
    }
    if (dom.transcribeGainValue) {
        const gain = Math.round(settings.transcribe_input_gain_db);
        dom.transcribeGainValue.textContent = `${gain >= 0 ? "+" : ""}${gain} dB`;
    }
    if (dom.transcribeBatchField) {
        const disabled = settings.transcribe_vad_mode;
        dom.transcribeBatchField.classList.toggle("is-disabled", disabled);
        dom.transcribeBatchInterval?.toggleAttribute("disabled", disabled);
    }
    if (dom.transcribeOverlapField) {
        const disabled = settings.transcribe_vad_mode;
        dom.transcribeOverlapField.classList.toggle("is-disabled", disabled);
        dom.transcribeChunkOverlap?.toggleAttribute("disabled", disabled);
    }
    if (dom.transcribeVadThresholdField) {
        const disabled = !settings.transcribe_vad_mode;
        dom.transcribeVadThresholdField.classList.toggle("is-disabled", disabled);
        dom.transcribeVadThreshold?.toggleAttribute("disabled", disabled);
    }
    if (dom.transcribeVadSilenceField) {
        const disabled = !settings.transcribe_vad_mode;
        dom.transcribeVadSilenceField.classList.toggle("is-disabled", disabled);
        dom.transcribeVadSilence?.toggleAttribute("disabled", disabled);
    }
}