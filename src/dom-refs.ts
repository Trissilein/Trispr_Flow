// DOM element references
// Centralized DOM queries to avoid repeated getElementById calls

const $ = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T | null;

// Status and hero elements
export const statusLabel = $("status-label");
export const statusDot = $("status-dot") as HTMLSpanElement | null;
export const statusMessage = $("status-message");
export const engineLabel = $("engine-label");
export const cloudState = $("cloud-state");
export const cloudCheck = $("cloud-check");
export const dictationBadge = $("dictation-badge");
export const modeState = $("mode-state");
export const deviceState = $("device-state");
export const modelState = $("model-state");

// Capture microphone controls
export const modeSelect = $("mode-select") as HTMLSelectElement | null;
export const pttHotkey = $("ptt-hotkey") as HTMLInputElement | null;
export const pttHotkeyRecord = $("ptt-hotkey-record") as HTMLButtonElement | null;
export const pttHotkeyStatus = $("ptt-hotkey-status") as HTMLSpanElement | null;
export const toggleHotkey = $("toggle-hotkey") as HTMLInputElement | null;
export const toggleHotkeyRecord = $("toggle-hotkey-record") as HTMLButtonElement | null;
export const toggleHotkeyStatus = $("toggle-hotkey-status") as HTMLSpanElement | null;
export const deviceSelect = $("device-select") as HTMLSelectElement | null;
export const languageSelect = $("language-select") as HTMLSelectElement | null;
export const cloudToggle = $("cloud-toggle") as HTMLInputElement | null;
export const audioCuesToggle = $("audio-cues-toggle") as HTMLInputElement | null;
export const audioCuesVolume = $("audio-cues-volume") as HTMLInputElement | null;
export const pttUseVadToggle = $("ptt-use-vad-toggle") as HTMLInputElement | null;
export const audioCuesVolumeValue = $("audio-cues-volume-value");
export const micGain = $("mic-gain") as HTMLInputElement | null;
export const micGainValue = $("mic-gain-value");
export const hotkeysBlock = $("hotkeys-block");
export const vadBlock = $("vad-block");
export const vadThreshold = $("vad-threshold") as HTMLInputElement | null;
export const vadThresholdValue = $("vad-threshold-value");
export const vadSilence = $("vad-silence") as HTMLInputElement | null;
export const vadSilenceValue = $("vad-silence-value");
export const vadMeterFill = $("vad-meter-fill");
export const vadLevelDbm = $("vad-level-dbm");
export const vadMarkerStart = $("vad-marker-start");
export const vadMarkerSustain = $("vad-marker-sustain");

// System audio / transcribe controls
export const transcribeStatus = $("transcribe-status");
export const transcribeHotkey = $("transcribe-hotkey") as HTMLInputElement | null;
export const transcribeHotkeyRecord = $("transcribe-hotkey-record") as HTMLButtonElement | null;
export const transcribeHotkeyStatus = $("transcribe-hotkey-status") as HTMLSpanElement | null;
export const transcribeDeviceSelect = $("transcribe-device-select") as HTMLSelectElement | null;
export const transcribeVadToggle = $("transcribe-vad-toggle") as HTMLInputElement | null;
export const transcribeVadThreshold = $("transcribe-vad-threshold") as HTMLInputElement | null;
export const transcribeVadThresholdValue = $("transcribe-vad-threshold-value");
export const transcribeVadThresholdField = $("transcribe-vad-threshold-field");
export const transcribeVadSilenceField = $("transcribe-vad-silence-field");
export const transcribeVadSilence = $("transcribe-vad-silence") as HTMLInputElement | null;
export const transcribeVadSilenceValue = $("transcribe-vad-silence-value");
export const transcribeMeterFill = $("transcribe-meter-fill");
export const transcribeMeterDb = $("transcribe-meter-db");
export const transcribeMeterThreshold = $("transcribe-meter-threshold");
export const transcribeThresholdDb = $("transcribe-threshold-db");
export const transcribeThresholdLabel = $("transcribe-threshold-label");
export const transcribeBatchField = $("transcribe-batch-field");
export const transcribeBatchInterval = $("transcribe-batch-interval") as HTMLInputElement | null;
export const transcribeBatchValue = $("transcribe-batch-value");
export const transcribeOverlapField = $("transcribe-overlap-field");
export const transcribeChunkOverlap = $("transcribe-chunk-overlap") as HTMLInputElement | null;
export const transcribeOverlapValue = $("transcribe-overlap-value");
export const transcribeGain = $("transcribe-gain") as HTMLInputElement | null;
export const transcribeGainValue = $("transcribe-gain-value");

// Overlay controls
export const overlayColor = $("overlay-color") as HTMLInputElement | null;
export const overlayMinRadius = $("overlay-min-radius") as HTMLInputElement | null;
export const overlayMinRadiusValue = $("overlay-min-radius-value");
export const overlayMaxRadius = $("overlay-max-radius") as HTMLInputElement | null;
export const overlayMaxRadiusValue = $("overlay-max-radius-value");
export const overlayRise = $("overlay-rise") as HTMLInputElement | null;
export const overlayRiseValue = $("overlay-rise-value");
export const overlayFall = $("overlay-fall") as HTMLInputElement | null;
export const overlayFallValue = $("overlay-fall-value");
export const overlayOpacityInactive = $("overlay-opacity-inactive") as HTMLInputElement | null;
export const overlayOpacityInactiveValue = $("overlay-opacity-inactive-value");
export const overlayOpacityActive = $("overlay-opacity-active") as HTMLInputElement | null;
export const overlayOpacityActiveValue = $("overlay-opacity-active-value");
export const overlayPosX = $("overlay-pos-x") as HTMLInputElement | null;
export const overlayPosY = $("overlay-pos-y") as HTMLInputElement | null;
export const overlayStyle = $("overlay-style") as HTMLSelectElement | null;
export const overlayDotSettings = $("overlay-dot-settings") as HTMLDivElement | null;
export const overlayKittSettings = $("overlay-kitt-settings") as HTMLDivElement | null;
export const overlayKittMinWidth = $("overlay-kitt-min-width") as HTMLInputElement | null;
export const overlayKittMinWidthValue = $("overlay-kitt-min-width-value");
export const overlayKittMaxWidth = $("overlay-kitt-max-width") as HTMLInputElement | null;
export const overlayKittMaxWidthValue = $("overlay-kitt-max-width-value");
export const overlayKittHeight = $("overlay-kitt-height") as HTMLInputElement | null;
export const overlayKittHeightValue = $("overlay-kitt-height-value");

// History controls
export const historyList = $("history-list");
export const historyInput = $("history-input") as HTMLInputElement | null;
export const historyAdd = $("history-add");
export const historyCompose = document.querySelector(".history-compose") as HTMLDivElement | null;
export const historyTabMic = $("history-tab-mic");
export const historyTabSystem = $("history-tab-system");
export const historyTabConversation = $("history-tab-conversation");
export const historyCopyConversation = $("history-copy-conversation") as HTMLButtonElement | null;
export const historyDetachConversation = $("history-detach-conversation") as HTMLButtonElement | null;
export const conversationFontControls = $("conversation-font-controls");
export const conversationFontSize = $("conversation-font-size") as HTMLInputElement | null;
export const conversationFontSizeValue = $("conversation-font-size-value");

// Model manager controls
export const modelSourceSelect = $("model-source-select") as HTMLSelectElement | null;
export const modelCustomUrl = $("model-custom-url") as HTMLInputElement | null;
export const modelCustomUrlField = $("model-custom-url-field") as HTMLDivElement | null;
export const modelRefresh = $("model-refresh") as HTMLButtonElement | null;
export const modelStoragePath = $("model-storage-path") as HTMLInputElement | null;
export const modelStorageBrowse = $("model-storage-browse") as HTMLButtonElement | null;
export const modelStorageReset = $("model-storage-reset") as HTMLButtonElement | null;
export const modelListActive = $("model-list-active");
export const modelListInstalled = $("model-list-installed");
export const modelListAvailable = $("model-list-available");
