import * as dom from "../dom-refs";
import { settings, outputDevices } from "../state";
import { persistSettings } from "../settings-persist";
import { formatBytes, formatHotkeyForDisplay } from "../ui-helpers";
import { invoke } from "@tauri-apps/api/core";
import type {
  PiperVoiceCatalogEntry,
  PiperVoiceDownloadProgress,
  VoiceOutputSettings,
  TtsVoiceInfo,
  TtsProviderInfo,
} from "../types";

// Voice Output settings rendering (R3 slice 4).
//
// Renders provider chain, default + fallback outputs, per-provider voices, Qwen3 cloud parameters,
// and the persistent TTS stop hotkey write in the Settings UI.
//
// Exports:
//   Primary   - renderVoiceOutputSettings()       called by renderSettings() in index.ts
//   Secondary - refreshProviderAvailability()     called by voice-output.wire.ts and internally by render
//               refreshProviderVoices()           called by voice-output.wire.ts and this module
//               handleProviderVoiceSelection()     called by voice-output.wire.ts
//               handlePiperVoiceDownloadProgress() called by main.ts
//
// Per Decision 6 (settings-decomposition.md): all other functions are private.
// refreshVoiceOutputWindowsVoices is removed (it was a one-line wrapper of refreshProviderVoices).
// updateProviderMutualExclusion is now private to this module.

let voiceOutputWindowsVoiceRequestSeq = 0;
let voiceOutputFallbackVoiceRequestSeq = 0;
const DEFAULT_PIPER_VOICE_KEY = "de_DE-thorsten-medium";
const PIPER_OPTION_CUSTOM_PREFIX = "[Custom] ";
const PIPER_OPTION_INSTALLED_MARKER = "✓ ";
const REMOVED_PIPER_VOICE_KEYS = new Set(["de_de-mls-medium"]);
let lastTtsProviders: TtsProviderInfo[] = [];
let piperDownloadInFlight = false;
type TtsProviderId = VoiceOutputSettings["default_provider"];

function isRemovedPiperVoiceKey(value: string): boolean {
  return REMOVED_PIPER_VOICE_KEYS.has(value.trim().toLowerCase());
}

function isWindowsVoiceProvider(
  provider: string | null | undefined
): provider is "windows_native" | "windows_natural" {
  return provider === "windows_native" || provider === "windows_natural";
}

function isPiperVoiceProvider(provider: string | null | undefined): provider is "local_custom" {
  return provider === "local_custom";
}

function normalizePiperGainDb(value: number | null | undefined): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return -12;
  return Math.max(-24, Math.min(6, Math.round(parsed)));
}

function isAnyPiperProviderActive(): boolean {
  if (!settings?.voice_output_settings) return false;
  return isPiperVoiceProvider(settings.voice_output_settings.default_provider)
    || isPiperVoiceProvider(settings.voice_output_settings.fallback_provider);
}

function setPiperDownloadProgressUi(
  percent: number,
  text: string,
  options?: { forceVisible?: boolean }
): void {
  const normalizedPercent = Math.max(0, Math.min(100, Math.round(percent)));
  if (dom.voiceOutputPiperDownloadFill) {
    dom.voiceOutputPiperDownloadFill.style.width = `${normalizedPercent}%`;
    const progressBar = dom.voiceOutputPiperDownloadFill.parentElement;
    if (progressBar) {
      progressBar.setAttribute("aria-valuenow", String(normalizedPercent));
    }
  }
  if (dom.voiceOutputPiperDownloadText) {
    dom.voiceOutputPiperDownloadText.textContent = text;
  }
  if (dom.voiceOutputPiperDownloadStatus) {
    const visible = Boolean(options?.forceVisible || piperDownloadInFlight || isAnyPiperProviderActive());
    dom.voiceOutputPiperDownloadStatus.hidden = !visible;
  }
}

async function isPiperVoiceInstalledByCatalog(voiceKey: string): Promise<boolean> {
  try {
    const catalog = await invoke<PiperVoiceCatalogEntry[]>("list_piper_voice_catalog");
    return catalog.some((entry) => entry.key === voiceKey && entry.installed);
  } catch {
    return false;
  }
}

function setFieldHidden(field: HTMLElement | null, hidden: boolean): void {
  if (!field) return;
  field.hidden = hidden;
  if (hidden) {
    field.style.display = "none";
  } else {
    field.style.removeProperty("display");
  }
}

function voicePickerTitle(provider: string, isDefault: boolean): string {
  if (isWindowsVoiceProvider(provider)) {
    return isDefault
      ? "Select a Windows speaker voice"
      : "Select a fallback Windows speaker voice";
  }
  if (provider === "local_custom") {
    return "Select a Piper voice model";
  }
  if (provider === "qwen3_tts") {
    return "Voice selection is managed in Qwen3-TTS settings";
  }
  return "Auto (provider default)";
}

function toDisplayLanguage(locale: string): string {
  const [languagePart, regionPart] = locale.split("-");
  const language = languagePart?.trim().toLowerCase() ?? "";
  const region = regionPart?.trim().toUpperCase() ?? "";
  if (!language) return locale;
  const languageName = (() => {
    try {
      if (typeof Intl !== "undefined" && typeof Intl.DisplayNames === "function") {
        const names = new Intl.DisplayNames(["en"], { type: "language" });
        return names.of(language) ?? language;
      }
    } catch {
      // ignore and fall back to locale token
    }
    return language;
  })();
  return region ? `${languageName} (${region})` : languageName;
}

function toProfileLabel(profile: string | null | undefined): string | null {
  switch ((profile ?? "").trim().toLowerCase()) {
    case "multilingual":
      return "Multilingual";
    case "natural":
      return "Natural";
    case "online":
      return "Online";
    case "standard":
      return "Standard";
    default:
      return null;
  }
}

function formatWindowsVoiceLabel(voice: TtsVoiceInfo): string {
  const parts: string[] = [];
  const locale = (voice.locale ?? "").trim();
  if (locale.length > 0) {
    parts.push(toDisplayLanguage(locale));
  }
  const profileLabel = toProfileLabel(voice.profile);
  if (profileLabel) {
    parts.push(profileLabel);
  }
  return parts.length > 0 ? `${voice.label} (${parts.join(", ")})` : voice.label;
}

function basenameFromPath(rawPath: string): string {
  const normalized = rawPath.replace(/\\/g, "/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || rawPath;
}

function formatPiperOptionLabel(entry: PiperVoiceCatalogEntry): string {
  return entry.installed
    ? `${PIPER_OPTION_INSTALLED_MARKER}${entry.label}`
    : entry.label;
}

function applyPiperOptionVisualState(option: HTMLOptionElement, installed: boolean): void {
  option.dataset.piperInstalled = installed ? "1" : "0";
  option.style.backgroundColor = "";
  option.style.backgroundImage = "";
}

function normalizedPiperSelection(
  configuredModelPath: string,
  catalog: PiperVoiceCatalogEntry[]
): string {
  const configured = configuredModelPath.trim();
  if (configured.length === 0) return DEFAULT_PIPER_VOICE_KEY;
  if (isRemovedPiperVoiceKey(configured)) {
    return DEFAULT_PIPER_VOICE_KEY;
  }
  if (catalog.some((entry) => entry.key === configured)) {
    return configured;
  }
  const byPath = catalog.find((entry) => entry.path && entry.path === configured);
  if (byPath) {
    return byPath.key;
  }
  return configured;
}

function availableRuntimeStableProviderIds(providers: TtsProviderInfo[]): TtsProviderId[] {
  return providers
    .filter((provider) => provider.available && provider.surface === "runtime_stable")
    .map((provider) => provider.id) as TtsProviderId[];
}

function normalizeProviderPair(
  providers: TtsProviderInfo[],
  preferredDefault: TtsProviderId,
  preferredFallback: TtsProviderId
): { defaultProvider: TtsProviderId; fallbackProvider: TtsProviderId } {
  const runtimeStable = availableRuntimeStableProviderIds(providers) as TtsProviderId[];
  const available = providers
    .filter((provider) => provider.available)
    .map((provider) => provider.id) as TtsProviderId[];
  const defaultBase: TtsProviderId = runtimeStable[0] ?? available[0] ?? "windows_native";
  const selectPreferred = (preferred: TtsProviderId, disallow: TtsProviderId | null): TtsProviderId => {
    const candidate = preferred;
    const preferredInfo = providers.find((provider) => provider.id === candidate);
    if (preferredInfo?.available) {
      if (disallow && candidate === disallow && runtimeStable.length > 1) {
        // fall through to choose another provider
      } else {
        return candidate;
      }
    }
    const runtimeCandidate = runtimeStable.find((id) => id !== disallow);
    if (runtimeCandidate) return runtimeCandidate;
    const availableCandidate = available.find((id) => id !== disallow);
    if (availableCandidate) return availableCandidate;
    return defaultBase;
  };

  const defaultProvider = selectPreferred(preferredDefault, null);
  const fallbackProvider = selectPreferred(preferredFallback, defaultProvider);
  return { defaultProvider, fallbackProvider };
}

function setProviderOptions(
  select: HTMLSelectElement | null,
  providers: TtsProviderInfo[]
): void {
  if (!select) return;
  select.innerHTML = "";
  providers.forEach((provider) => {
    const option = document.createElement("option");
    option.value = provider.id;
    option.textContent = provider.available ? provider.label : `${provider.label} — nicht verfügbar`;
    option.disabled = !provider.available;
    option.dataset.providerAvailable = provider.available ? "1" : "0";
    select.appendChild(option);
  });
}

function updateProviderMutualExclusion(): void {
  const stableAvailable = availableRuntimeStableProviderIds(lastTtsProviders);
  const enforceDistinctProviders = stableAvailable.length > 1;
  const defaultValue = dom.voiceOutputDefaultProvider?.value ?? "";
  const fallbackValue = dom.voiceOutputFallbackProvider?.value ?? "";
  for (const option of Array.from(dom.voiceOutputDefaultProvider?.options ?? [])) {
    if (!option.value) continue;
    const available = option.dataset.providerAvailable !== "0";
    option.disabled = !available || (enforceDistinctProviders && option.value === fallbackValue);
  }
  for (const option of Array.from(dom.voiceOutputFallbackProvider?.options ?? [])) {
    if (!option.value) continue;
    const available = option.dataset.providerAvailable !== "0";
    option.disabled = !available || (enforceDistinctProviders && option.value === defaultValue);
  }
}

export async function refreshProviderAvailability(): Promise<void> {
  let providers: TtsProviderInfo[];
  try {
    providers = await Promise.race([
      invoke<TtsProviderInfo[]>("list_tts_providers"),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error("timeout")), 3000)),
    ]);
  } catch {
    return;
  }

  lastTtsProviders = providers;
  setProviderOptions(dom.voiceOutputDefaultProvider, providers);
  setProviderOptions(dom.voiceOutputFallbackProvider, providers);

  if (settings?.voice_output_settings) {
    const { defaultProvider, fallbackProvider } = normalizeProviderPair(
      providers,
      settings.voice_output_settings.default_provider,
      settings.voice_output_settings.fallback_provider
    );
    const changed =
      settings.voice_output_settings.default_provider !== defaultProvider
      || settings.voice_output_settings.fallback_provider !== fallbackProvider;
    settings.voice_output_settings.default_provider = defaultProvider;
    settings.voice_output_settings.fallback_provider = fallbackProvider;
    if (dom.voiceOutputDefaultProvider) {
      dom.voiceOutputDefaultProvider.value = defaultProvider;
    }
    if (dom.voiceOutputFallbackProvider) {
      dom.voiceOutputFallbackProvider.value = fallbackProvider;
    }
    if (changed) {
      void persistSettings();
    }
  }
  updateProviderMutualExclusion();

  const setAvailabilityBadge = (
    badge: HTMLElement | null,
    providerId: string | null | undefined
  ): void => {
    if (!badge) return;
    const provider = providers.find((entry) => entry.id === providerId);
    if (!provider) {
      badge.textContent = "Unavailable";
      badge.classList.add("unavailable");
      return;
    }
    badge.textContent = provider.available ? "Available" : "Unavailable";
    badge.classList.toggle("unavailable", !provider.available);
  };

  setAvailabilityBadge(
    dom.voiceOutputDefaultAvailability,
    dom.voiceOutputDefaultProvider?.value
  );
  setAvailabilityBadge(
    dom.voiceOutputFallbackAvailability,
    dom.voiceOutputFallbackProvider?.value
  );

  void refreshProviderVoices("default");
  void refreshProviderVoices("fallback");
}

export async function refreshProviderVoices(target: "default" | "fallback"): Promise<void> {
  if (!settings?.voice_output_settings) return;

  const isDefault = target === "default";
  const provider = isDefault
    ? settings.voice_output_settings.default_provider
    : settings.voice_output_settings.fallback_provider;

  const field = isDefault ? dom.voiceOutputWindowsVoiceField : dom.voiceOutputFallbackVoiceField;
  const select = isDefault ? dom.voiceOutputWindowsVoiceSelect : dom.voiceOutputFallbackVoiceSelect;
  const hint = isDefault ? dom.voiceOutputWindowsVoiceHint : dom.voiceOutputFallbackVoiceHint;
  const autoField = isDefault ? dom.voiceOutputAutoLanguageVoiceField : null;

  if (!select) return;
  select.title = voicePickerTitle(provider, isDefault);
  if (!piperDownloadInFlight) {
    setPiperDownloadProgressUi(0, "Bereit.");
  }

  if (!isWindowsVoiceProvider(provider) && !isPiperVoiceProvider(provider)) {
    select.classList.remove("piper-voice-select");
    setFieldHidden(field, true);
    setFieldHidden(autoField, true);
    if (hint) {
      hint.textContent = provider === "qwen3_tts"
        ? "Stimme wird in den Qwen3-TTS-Einstellungen gesteuert."
        : "Stimme-Auswahl nur für Windows-Provider verfügbar.";
    }
    select.innerHTML = "";
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "Auto (provider default)";
    select.appendChild(option);
    select.value = "";
    select.disabled = true;
    return;
  }

  if (isPiperVoiceProvider(provider)) {
    select.classList.add("piper-voice-select");
    setFieldHidden(field, false);
    setFieldHidden(autoField, true);
    select.disabled = true;
    select.innerHTML = "";
    const loadingOption = document.createElement("option");
    loadingOption.value = "";
    loadingOption.textContent = "Lade Piper-Stimmen...";
    select.appendChild(loadingOption);
    if (hint) hint.textContent = "Lade kuratierte und installierte Piper-Stimmen...";

    const seqRef = isDefault ? ++voiceOutputWindowsVoiceRequestSeq : ++voiceOutputFallbackVoiceRequestSeq;
    try {
      const catalog = await invoke<PiperVoiceCatalogEntry[]>("list_piper_voice_catalog");
      const currentSeq = isDefault ? voiceOutputWindowsVoiceRequestSeq : voiceOutputFallbackVoiceRequestSeq;
      if (seqRef !== currentSeq) return;

      select.innerHTML = "";
      const configured = (settings.voice_output_settings.piper_model_path ?? "").trim();
      const normalizedSelection = normalizedPiperSelection(configured, catalog);
      if (configured !== normalizedSelection) {
        settings.voice_output_settings.piper_model_path = normalizedSelection;
        if (dom.voiceOutputPiperModel) {
          dom.voiceOutputPiperModel.value = normalizedSelection;
        }
      }
      const installedEntries = catalog.filter((entry) => entry.installed);
      const downloadableEntries = catalog.filter((entry) => !entry.installed);
      [...installedEntries, ...downloadableEntries].forEach((entry) => {
        const option = document.createElement("option");
        option.value = entry.key;
        option.textContent = formatPiperOptionLabel(entry);
        applyPiperOptionVisualState(option, entry.installed);
        option.dataset.piperPath = entry.path ?? "";
        option.dataset.piperCurated = entry.curated ? "1" : "0";
        option.dataset.piperBaseLabel = entry.label;
        select.appendChild(option);
      });

      if (
        normalizedSelection.length > 0
        && !catalog.some((entry) => entry.key === normalizedSelection)
      ) {
        const customOption = document.createElement("option");
        customOption.value = normalizedSelection;
        customOption.textContent =
          `${PIPER_OPTION_INSTALLED_MARKER}${PIPER_OPTION_CUSTOM_PREFIX}${basenameFromPath(normalizedSelection)}`;
        applyPiperOptionVisualState(customOption, true);
        customOption.dataset.piperPath = normalizedSelection;
        customOption.dataset.piperCurated = "0";
        customOption.dataset.piperBaseLabel = basenameFromPath(normalizedSelection);
        select.appendChild(customOption);
      }

      select.value = normalizedSelection;
      select.disabled = false;
      const installedCount = catalog.filter((entry) => entry.installed).length;
      const downloadableCount = Math.max(0, catalog.length - installedCount);
      if (hint) {
        hint.textContent =
          `${installedCount}/${catalog.length} installiert · ${downloadableCount} per Download verfügbar.`;
      }
      return;
    } catch (error) {
      const currentSeq = isDefault ? voiceOutputWindowsVoiceRequestSeq : voiceOutputFallbackVoiceRequestSeq;
      if (seqRef !== currentSeq) return;
      select.innerHTML = "";
      const fallbackOption = document.createElement("option");
      fallbackOption.value = settings.voice_output_settings.piper_model_path || DEFAULT_PIPER_VOICE_KEY;
      fallbackOption.textContent = `${PIPER_OPTION_INSTALLED_MARKER}${PIPER_OPTION_CUSTOM_PREFIX}${fallbackOption.value}`;
      applyPiperOptionVisualState(fallbackOption, true);
      fallbackOption.dataset.piperPath = fallbackOption.value;
      fallbackOption.dataset.piperBaseLabel = fallbackOption.value;
      select.appendChild(fallbackOption);
      select.value = fallbackOption.value;
      select.disabled = false;
      if (hint) {
        hint.textContent = `Piper-Stimmliste nicht verfügbar: ${String(error).replace(/^Error:\s*/i, "").trim()}`;
      }
      return;
    }
  }

  select.classList.remove("piper-voice-select");
  setFieldHidden(field, false);
  setFieldHidden(autoField, false);
  select.disabled = true;
  select.innerHTML = "";
  const loadingOption = document.createElement("option");
  loadingOption.value = "";
  loadingOption.textContent = "Lade Stimmen...";
  select.appendChild(loadingOption);
  if (hint) hint.textContent = "Lade installierte Windows-Stimmen...";

  const seqRef = isDefault ? ++voiceOutputWindowsVoiceRequestSeq : ++voiceOutputFallbackVoiceRequestSeq;
  try {
    const voices = await invoke<TtsVoiceInfo[]>("list_tts_voices", { provider });
    const currentSeq = isDefault ? voiceOutputWindowsVoiceRequestSeq : voiceOutputFallbackVoiceRequestSeq;
    if (seqRef !== currentSeq) return;

    const filteredVoices = voices.filter((voice) => voice.provider === provider);
    const voiceIdKey = isDefault ? "voice_id_windows" : "voice_id_windows_fallback";
    const selectedVoiceId = ((settings.voice_output_settings[voiceIdKey] as string) ?? "").trim();
    const availableIds = new Set(filteredVoices.map((voice) => voice.id));
    const effectiveVoiceId = selectedVoiceId.length > 0 && availableIds.has(selectedVoiceId)
      ? selectedVoiceId
      : "";
    (settings.voice_output_settings[voiceIdKey] as string) = effectiveVoiceId;

    select.innerHTML = "";
    const autoOption = document.createElement("option");
    autoOption.value = "";
    autoOption.textContent = "Auto (provider default)";
    select.appendChild(autoOption);

    filteredVoices.forEach((voice) => {
      const option = document.createElement("option");
      option.value = voice.id;
      option.textContent = formatWindowsVoiceLabel(voice);
      select.appendChild(option);
    });

    select.value = effectiveVoiceId;
    select.disabled = filteredVoices.length === 0;
    if (hint) {
      hint.textContent = filteredVoices.length > 0
        ? `${filteredVoices.length} Windows-Stimme(n) gefunden.`
        : "Keine Windows-Stimmen für diesen Provider gefunden.";
    }
  } catch (error) {
    const currentSeq = isDefault ? voiceOutputWindowsVoiceRequestSeq : voiceOutputFallbackVoiceRequestSeq;
    if (seqRef !== currentSeq) return;
    select.innerHTML = "";
    const errorOption = document.createElement("option");
    errorOption.value = "";
    errorOption.textContent = "Auto (provider default)";
    select.appendChild(errorOption);
    select.value = "";
    select.disabled = false;
    if (hint) {
      hint.textContent = `Stimmliste nicht verfügbar: ${String(error).replace(/^Error:\s*/i, "").trim()}`;
    }
  }
}

export function handlePiperVoiceDownloadProgress(progress: PiperVoiceDownloadProgress): void {
  const key = (progress.voice_key ?? "").trim();
  const stage = (progress.stage ?? "").trim().toLowerCase();
  const downloaded = Number(progress.downloaded_bytes ?? 0);
  const total = Number(progress.total_bytes ?? 0);
  const explicitPercent = Number(progress.percent ?? Number.NaN);
  const computedPercent = Number.isFinite(explicitPercent)
    ? explicitPercent
    : (Number.isFinite(total) && total > 0
      ? (downloaded / total) * 100
      : 0);
  const readableDownloaded = formatBytes(Math.max(0, downloaded));
  const readableTotal = total > 0 ? formatBytes(total) : null;
  const stageLabel = key.length > 0 ? key : "Piper";

  if (stage === "started") {
    piperDownloadInFlight = true;
    const message = progress.message?.trim() || `${stageLabel}: Download gestartet...`;
    setPiperDownloadProgressUi(0, message, { forceVisible: true });
    return;
  }

  if (stage === "downloading") {
    piperDownloadInFlight = true;
    const suffix = readableTotal
      ? `${readableDownloaded} / ${readableTotal}`
      : readableDownloaded;
    const message = progress.message?.trim()
      || `${stageLabel}: ${Math.round(Math.max(0, Math.min(100, computedPercent)))}% · ${suffix}`;
    setPiperDownloadProgressUi(computedPercent, message, { forceVisible: true });
    return;
  }

  if (stage === "completed") {
    piperDownloadInFlight = false;
    const message = progress.message?.trim() || `${stageLabel}: Download abgeschlossen.`;
    setPiperDownloadProgressUi(100, message, { forceVisible: true });
    return;
  }

  if (stage === "error") {
    piperDownloadInFlight = false;
    const message = progress.message?.trim() || `${stageLabel}: Download fehlgeschlagen.`;
    setPiperDownloadProgressUi(computedPercent, message, { forceVisible: true });
    return;
  }

  const fallbackMessage = progress.message?.trim() || `${stageLabel}: ${stage || "Status-Update"}`;
  setPiperDownloadProgressUi(computedPercent, fallbackMessage, { forceVisible: true });
}

export function renderVoiceOutputSettings(): void {
  if (!settings?.voice_output_settings) return;

  if (dom.ttsStopHotkey) {
    dom.ttsStopHotkey.value = formatHotkeyForDisplay(
      settings.hotkey_tts_stop || "CommandOrControl+Shift+F12"
    );
  }

  const vo = settings.voice_output_settings;
  vo.auto_voice_by_detected_language = vo.auto_voice_by_detected_language === true;
  vo.piper_gain_db = normalizePiperGainDb(vo.piper_gain_db);
  const normalizedOutputDevice = typeof vo.output_device === "string" && vo.output_device.trim().length > 0
    ? vo.output_device.trim()
    : "default";
  vo.output_device = normalizedOutputDevice;

  if (dom.voiceOutputDeviceSelect) {
    dom.voiceOutputDeviceSelect.innerHTML = "";
    const defaultOption = document.createElement("option");
    defaultOption.value = "default";
    defaultOption.textContent = "Default (System)";
    dom.voiceOutputDeviceSelect.appendChild(defaultOption);

    outputDevices
      .filter((device) => device.id !== "default")
      .forEach((device) => {
        const option = document.createElement("option");
        option.value = device.id;
        option.textContent = device.label;
        dom.voiceOutputDeviceSelect?.appendChild(option);
      });

    dom.voiceOutputDeviceSelect.value = normalizedOutputDevice;
    if (dom.voiceOutputDeviceSelect.value !== normalizedOutputDevice) {
      vo.output_device = "default";
      dom.voiceOutputDeviceSelect.value = "default";
    }
  }

  const normalizeProvider = (
    select: HTMLSelectElement | null,
    preferred: string | undefined,
    fallback: "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts"
  ): "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts" => {
    if (!select) return fallback;
    const candidate = (preferred ?? "").trim();
    const optionExists = candidate.length > 0
      && Array.from(select.options).some((option) => option.value === candidate && !option.disabled);
    return optionExists ? (candidate as "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts") : fallback;
  };
  const normalizedDefault = normalizeProvider(
    dom.voiceOutputDefaultProvider,
    vo.default_provider as string | undefined,
    "windows_native"
  );
  const normalizedFallback = normalizeProvider(
    dom.voiceOutputFallbackProvider,
    vo.fallback_provider as string | undefined,
    "windows_native"
  );
  vo.default_provider = normalizedDefault;
  vo.fallback_provider = normalizedFallback;

  if (dom.voiceOutputDefaultProvider) {
    dom.voiceOutputDefaultProvider.value = normalizedDefault;
  }
  if (dom.voiceOutputFallbackProvider) {
    dom.voiceOutputFallbackProvider.value = normalizedFallback;
  }
  if (dom.voiceOutputPolicy) {
    dom.voiceOutputPolicy.value = vo.output_policy ?? "agent_replies_only";
  }

  // Rate slider
  if (dom.voiceOutputRate) {
    const rate = vo.rate ?? 1.0;
    dom.voiceOutputRate.value = String(rate);
    if (dom.voiceOutputRateValue) {
      dom.voiceOutputRateValue.textContent = rate.toFixed(2);
    }
  }

  // Volume slider
  if (dom.voiceOutputVolume) {
    const volume = vo.volume ?? 1.0;
    dom.voiceOutputVolume.value = String(volume);
    if (dom.voiceOutputVolumeValue) {
      dom.voiceOutputVolumeValue.textContent = volume.toFixed(2);
    }
  }

  if (dom.voiceOutputPiperGainDb) {
    dom.voiceOutputPiperGainDb.value = String(vo.piper_gain_db);
    if (dom.voiceOutputPiperGainDbValue) {
      dom.voiceOutputPiperGainDbValue.textContent = `${vo.piper_gain_db} dB`;
    }
  }

  // Piper paths
  if (dom.voiceOutputPiperBinary) {
    dom.voiceOutputPiperBinary.value = vo.piper_binary_path ?? "";
  }
  if (dom.voiceOutputPiperModel) {
    dom.voiceOutputPiperModel.value = vo.piper_model_path ?? "";
  }
  if (dom.voiceOutputPiperModelDir) {
    dom.voiceOutputPiperModelDir.value = vo.piper_model_dir ?? "";
  }
  if (dom.voiceOutputQwenEndpoint) {
    dom.voiceOutputQwenEndpoint.value = vo.qwen3_tts_endpoint ?? "http://127.0.0.1:8000/v1/audio/speech";
  }
  if (dom.voiceOutputQwenModel) {
    dom.voiceOutputQwenModel.value = vo.qwen3_tts_model ?? "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice";
  }
  if (dom.voiceOutputQwenVoice) {
    dom.voiceOutputQwenVoice.value = vo.qwen3_tts_voice ?? "vivian";
  }
  if (dom.voiceOutputQwenApiKey) {
    dom.voiceOutputQwenApiKey.value = vo.qwen3_tts_api_key ?? "";
  }
  if (dom.voiceOutputQwenTimeoutSec) {
    const timeout = Number.isFinite(vo.qwen3_tts_timeout_sec as number)
      ? Math.max(3, Math.min(180, Number(vo.qwen3_tts_timeout_sec)))
      : 45;
    vo.qwen3_tts_timeout_sec = timeout;
    dom.voiceOutputQwenTimeoutSec.value = String(timeout);
  }
  if (dom.voiceOutputAutoLanguageVoice) {
    dom.voiceOutputAutoLanguageVoice.checked = vo.auto_voice_by_detected_language;
  }

  // Gate qwen3-TTS UI section based on enabled flag
  const qwen3Section = document.getElementById("voice-output-qwen3-section");
  if (qwen3Section) {
    qwen3Section.style.display = vo.qwen3_tts_enabled ? "block" : "none";
  }

  if (!piperDownloadInFlight) {
    setPiperDownloadProgressUi(0, "Bereit.");
  } else {
    setPiperDownloadProgressUi(0, dom.voiceOutputPiperDownloadText?.textContent?.trim() || "Lade Stimme...");
  }

  void refreshProviderAvailability();
}

export async function handleProviderVoiceSelection(target: "default" | "fallback"): Promise<void> {
  if (!settings?.voice_output_settings) return;
  const isDefault = target === "default";
  const provider = isDefault
    ? settings.voice_output_settings.default_provider
    : settings.voice_output_settings.fallback_provider;
  const select = isDefault ? dom.voiceOutputWindowsVoiceSelect : dom.voiceOutputFallbackVoiceSelect;
  const hint = isDefault ? dom.voiceOutputWindowsVoiceHint : dom.voiceOutputFallbackVoiceHint;
  if (!select) return;

  if (isWindowsVoiceProvider(provider)) {
    if (isDefault) {
      settings.voice_output_settings.voice_id_windows = select.value.trim();
    } else {
      settings.voice_output_settings.voice_id_windows_fallback = select.value.trim();
    }
    await persistSettings();
    return;
  }

  if (!isPiperVoiceProvider(provider)) {
    return;
  }

  const selected = select.value.trim();
  const previous = (settings.voice_output_settings.piper_model_path ?? DEFAULT_PIPER_VOICE_KEY).trim();
  const nextKey = selected || DEFAULT_PIPER_VOICE_KEY;
  if (isRemovedPiperVoiceKey(nextKey)) {
    select.value = DEFAULT_PIPER_VOICE_KEY;
    settings.voice_output_settings.piper_model_path = DEFAULT_PIPER_VOICE_KEY;
    if (dom.voiceOutputPiperModel) {
      dom.voiceOutputPiperModel.value = DEFAULT_PIPER_VOICE_KEY;
    }
    if (hint) {
      hint.textContent = "Diese Piper-Stimme wurde entfernt. Default wurde wiederhergestellt.";
    }
    await persistSettings();
    await refreshProviderVoices("default");
    await refreshProviderVoices("fallback");
    return;
  }
  const selectedOption = Array.from(select.options).find((option) => option.value === nextKey);
  let installed = selectedOption?.dataset.piperInstalled === "1";
  if (!installed) {
    installed = await isPiperVoiceInstalledByCatalog(nextKey);
  }

  if (!installed) {
    const confirmed = window.confirm(
      `Die Stimme '${nextKey}' ist nicht installiert. Jetzt herunterladen und aktivieren?`
    );
    if (!confirmed) {
      select.value = previous;
      if (hint) hint.textContent = `Auswahl verworfen. Aktiv bleibt: ${previous}.`;
      return;
    }
    if (hint) hint.textContent = `Lade Piper-Stimme '${nextKey}'...`;
    piperDownloadInFlight = true;
    setPiperDownloadProgressUi(0, `${nextKey}: Download gestartet...`, { forceVisible: true });
    try {
      await invoke<string>("download_piper_voice_key", { voiceKey: nextKey });
      piperDownloadInFlight = false;
      setPiperDownloadProgressUi(100, `${nextKey}: Download abgeschlossen.`, { forceVisible: true });
    } catch (error) {
      piperDownloadInFlight = false;
      select.value = previous;
      setPiperDownloadProgressUi(
        0,
        `${nextKey}: Download fehlgeschlagen (${String(error).replace(/^Error:\s*/i, "").trim()}).`,
        { forceVisible: true }
      );
      if (hint) {
        hint.textContent = `Download fehlgeschlagen (${nextKey}): ${String(error).replace(/^Error:\s*/i, "").trim()}`;
      }
      return;
    }
  }

  settings.voice_output_settings.piper_model_path = nextKey;
  if (dom.voiceOutputPiperModel) {
    dom.voiceOutputPiperModel.value = nextKey;
  }
  await persistSettings();
  await refreshProviderVoices("default");
  await refreshProviderVoices("fallback");
  const activeSelect = target === "default" ? dom.voiceOutputWindowsVoiceSelect : dom.voiceOutputFallbackVoiceSelect;
  if (activeSelect) {
    activeSelect.value = nextKey;
  }
  if (hint) {
    hint.textContent = `Aktive Piper-Stimme: ${nextKey}.`;
  }
}
