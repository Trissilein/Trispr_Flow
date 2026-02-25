// DOM element references
// Centralized DOM queries to avoid repeated getElementById calls

const $ = <T extends HTMLElement>(id: string) => {
  if (typeof document === "undefined") return null;
  return document.getElementById(id) as T | null;
};

// Main tabs
export const tabBtnTranscription = $("tab-btn-transcription") as HTMLButtonElement | null;
export const tabBtnSettings = $("tab-btn-settings") as HTMLButtonElement | null;
export const tabBtnAiRefinement = $("tab-btn-ai-refinement") as HTMLButtonElement | null;
export const tabTranscription = $("tab-transcription") as HTMLDivElement | null;
export const tabSettings = $("tab-settings") as HTMLDivElement | null;
export const tabAiRefinement = $("tab-ai-refinement") as HTMLDivElement | null;

// Status and hero elements
export const statusLabel = $("status-label");
export const statusDot = $("status-dot") as HTMLSpanElement | null;
export const recordingPill = $("recording-pill");
export const transcribeStatusDot = $("transcribe-dot") as HTMLSpanElement | null;
export const transcribeStatusLabel = $("transcribe-label");
export const transcribePill = $("transcribe-pill");
export const statusMessage = $("status-message");
export const cloudState = $("cloud-state");
export const cloudDetail = $("cloud-detail");
export const cloudCheck = $("cloud-check");
export const aiModelState = $("ai-model-state");
export const dictationBadge = $("dictation-badge");
export const modeState = $("mode-state");
export const deviceState = $("device-state");
export const modelState = $("model-state");
export const appVersion = $("app-version") as HTMLSpanElement | null;

// Capture input controls
export const captureEnabledToggle = $("capture-enabled-toggle") as HTMLInputElement | null;
export const modeSelect = $("mode-select") as HTMLSelectElement | null;
export const pttHotkey = $("ptt-hotkey") as HTMLInputElement | null;
export const pttHotkeyRecord = $("ptt-hotkey-record") as HTMLButtonElement | null;
export const pttHotkeyStatus = $("ptt-hotkey-status") as HTMLSpanElement | null;
export const toggleHotkey = $("toggle-hotkey") as HTMLInputElement | null;
export const toggleHotkeyRecord = $("toggle-hotkey-record") as HTMLButtonElement | null;
export const toggleHotkeyStatus = $("toggle-hotkey-status") as HTMLSpanElement | null;
export const deviceSelect = $("device-select") as HTMLSelectElement | null;
export const languageSelect = $("language-select") as HTMLSelectElement | null;
export const languagePinnedToggle = $("language-pinned-toggle") as HTMLInputElement | null;
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

// Text filtering controls
export const hallucinationFilterToggle = $("hallucination-filter-toggle") as HTMLInputElement | null;
export const activationWordsToggle = $("activation-words-toggle") as HTMLInputElement | null;
export const activationWordsConfig = $("activation-words-config") as HTMLDivElement | null;
export const activationWordsList = $("activation-words-list") as HTMLTextAreaElement | null;

// Post-processing controls
export const postprocEnabled = $("postproc-enabled") as HTMLInputElement | null;
export const postprocSettings = $("postproc-settings") as HTMLDivElement | null;
export const postprocLanguage = $("postproc-language") as HTMLSelectElement | null;
export const postprocPunctuation = $("postproc-punctuation") as HTMLInputElement | null;
export const postprocCapitalization = $("postproc-capitalization") as HTMLInputElement | null;
export const postprocNumbers = $("postproc-numbers") as HTMLInputElement | null;
export const postprocCustomVocabEnabled = $("postproc-custom-vocab-enabled") as HTMLInputElement | null;
export const postprocCustomVocabConfig = $("postproc-custom-vocab-config") as HTMLDivElement | null;
export const postprocVocabRows = $("postproc-vocab-rows") as HTMLDivElement | null;
export const postprocVocabAdd = $("postproc-vocab-add") as HTMLButtonElement | null;
export const aiFallbackEnabled = $("ai-fallback-enabled") as HTMLInputElement | null;
export const aiFallbackSettings = $("ai-fallback-settings") as HTMLDivElement | null;
export const aiFallbackProviderLanes = $("ai-fallback-provider-lanes") as HTMLDivElement | null;
export const aiFallbackLocalLane = $("ai-fallback-local-lane") as HTMLDivElement | null;
export const aiFallbackOnlineLane = $("ai-fallback-online-lane") as HTMLDivElement | null;
export const aiFallbackLocalPrimaryStatus = $("ai-fallback-local-primary-status") as HTMLSpanElement | null;
export const aiFallbackLocalPrimaryAction = $("ai-fallback-local-primary-action") as HTMLButtonElement | null;
export const aiFallbackLocalImportAction = $("ai-fallback-local-import-action") as HTMLButtonElement | null;
export const aiFallbackLocalAdvanced = $("ai-fallback-local-advanced") as HTMLDetailsElement | null;
export const aiFallbackLocalDetectAction = $("ai-fallback-local-detect-action") as HTMLButtonElement | null;
export const aiFallbackLocalUseSystemAction = $("ai-fallback-local-use-system-action") as HTMLButtonElement | null;
export const aiFallbackLocalVerifyAction = $("ai-fallback-local-verify-action") as HTMLButtonElement | null;
export const aiFallbackLocalRefreshAction = $("ai-fallback-local-refresh-action") as HTMLButtonElement | null;
export const aiFallbackLocalRuntimeNote = $("ai-fallback-local-runtime-note") as HTMLSpanElement | null;
export const aiFallbackOnlineStatusBadge = $("ai-fallback-online-status-badge") as HTMLSpanElement | null;
export const aiFallbackCredentialProvider = $("ai-fallback-credential-provider") as HTMLSelectElement | null;
export const aiFallbackFallbackProvider = $("ai-fallback-fallback-provider") as HTMLSelectElement | null;
export const aiFallbackCloudProviderList = $("ai-fallback-cloud-provider-list") as HTMLDivElement | null;
export const aiFallbackFallbackStatus = $("ai-fallback-fallback-status") as HTMLSpanElement | null;
export const aiFallbackModelField = $("ai-fallback-model-field") as HTMLLabelElement | null;
export const aiFallbackModel = $("ai-fallback-model") as HTMLSelectElement | null;
export const aiFallbackApiKeySection = $("ai-fallback-api-key-section") as HTMLDivElement | null;
export const aiFallbackAuthMethod = $("ai-fallback-auth-method") as HTMLSelectElement | null;
export const aiFallbackApiKeyInput = $("ai-fallback-api-key-input") as HTMLInputElement | null;
export const aiFallbackSaveKeyBtn = $("ai-fallback-save-key") as HTMLButtonElement | null;
export const aiFallbackClearKeyBtn = $("ai-fallback-clear-key") as HTMLButtonElement | null;
export const aiFallbackTestKeyBtn = $("ai-fallback-test-key") as HTMLButtonElement | null;
export const aiFallbackKeyStatus = $("ai-fallback-key-status") as HTMLSpanElement | null;
export const aiFallbackOllamaManagedNote = $("ai-fallback-ollama-managed-note") as HTMLDivElement | null;
export const aiFallbackTemperature = $("ai-fallback-temperature") as HTMLInputElement | null;
export const aiFallbackTemperatureValue = $("ai-fallback-temperature-value") as HTMLSpanElement | null;
export const aiFallbackMaxTokens = $("ai-fallback-max-tokens") as HTMLSelectElement | null;
export const aiFallbackCustomPromptEnabled = $("ai-fallback-custom-prompt-enabled") as HTMLInputElement | null;
export const aiFallbackCustomPromptField = $("ai-fallback-custom-prompt-field") as HTMLDivElement | null;
export const aiFallbackCustomPrompt = $("ai-fallback-custom-prompt") as HTMLTextAreaElement | null;
export const aiAuthModal = $("ai-auth-modal") as HTMLDivElement | null;
export const aiAuthModalBackdrop = $("ai-auth-modal-backdrop") as HTMLDivElement | null;
export const aiAuthModalClose = $("ai-auth-modal-close") as HTMLButtonElement | null;
export const aiAuthProviderName = $("ai-auth-provider-name") as HTMLParagraphElement | null;
export const aiAuthMethod = $("ai-auth-method") as HTMLSelectElement | null;
export const aiAuthApiKeyInput = $("ai-auth-api-key-input") as HTMLInputElement | null;
export const aiAuthSaveKey = $("ai-auth-save-key") as HTMLButtonElement | null;
export const aiAuthClearKey = $("ai-auth-clear-key") as HTMLButtonElement | null;
export const aiAuthVerifyKey = $("ai-auth-verify-key") as HTMLButtonElement | null;
export const aiAuthStatus = $("ai-auth-status") as HTMLSpanElement | null;

// Output / transcribe controls
export const transcribeEnabledToggle = $("transcribe-enabled-toggle") as HTMLInputElement | null;
export const outputDeviceState = $("output-device-state");
export const transcribeStatusPill = $("transcribe-status-pill");
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
export const continuousDumpEnabledToggle = $("continuous-dump-enabled-toggle") as HTMLInputElement | null;
export const continuousDumpProfile = $("continuous-dump-profile") as HTMLSelectElement | null;
export const continuousHardCut = $("continuous-hard-cut") as HTMLInputElement | null;
export const continuousHardCutValue = $("continuous-hard-cut-value");
export const continuousMinChunk = $("continuous-min-chunk") as HTMLInputElement | null;
export const continuousMinChunkValue = $("continuous-min-chunk-value");
export const continuousPreRoll = $("continuous-pre-roll") as HTMLInputElement | null;
export const continuousPreRollValue = $("continuous-pre-roll-value");
export const continuousPostRoll = $("continuous-post-roll") as HTMLInputElement | null;
export const continuousPostRollValue = $("continuous-post-roll-value");
export const continuousKeepalive = $("continuous-keepalive") as HTMLInputElement | null;
export const continuousKeepaliveValue = $("continuous-keepalive-value");
export const continuousSystemOverrideToggle = $("continuous-system-override-toggle") as HTMLInputElement | null;
export const continuousSystemSoftFlush = $("continuous-system-soft-flush") as HTMLInputElement | null;
export const continuousSystemSoftFlushValue = $("continuous-system-soft-flush-value");
export const continuousSystemSilenceFlush = $("continuous-system-silence-flush") as HTMLInputElement | null;
export const continuousSystemSilenceFlushValue = $("continuous-system-silence-flush-value");
export const continuousSystemHardCut = $("continuous-system-hard-cut") as HTMLInputElement | null;
export const continuousSystemHardCutValue = $("continuous-system-hard-cut-value");
export const continuousMicOverrideToggle = $("continuous-mic-override-toggle") as HTMLInputElement | null;
export const continuousMicSoftFlush = $("continuous-mic-soft-flush") as HTMLInputElement | null;
export const continuousMicSoftFlushValue = $("continuous-mic-soft-flush-value");
export const continuousMicSilenceFlush = $("continuous-mic-silence-flush") as HTMLInputElement | null;
export const continuousMicSilenceFlushValue = $("continuous-mic-silence-flush-value");
export const continuousMicHardCut = $("continuous-mic-hard-cut") as HTMLInputElement | null;
export const continuousMicHardCutValue = $("continuous-mic-hard-cut-value");

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
export const historyTabMic = $("history-tab-mic");
export const historyTabSystem = $("history-tab-system");
export const historyTabConversation = $("history-tab-conversation");
export const historyCopyConversation = $("history-copy-conversation") as HTMLButtonElement | null;
export const analyseButton = $("analyse-button") as HTMLButtonElement | null;
export const historyExport = $("history-export") as HTMLButtonElement | null;
export const exportFormat = $("export-format") as HTMLSelectElement | null;
export const historySearch = $("history-search") as HTMLInputElement | null;
export const historySearchClear = $("history-search-clear") as HTMLButtonElement | null;
export const conversationFontControls = $("conversation-font-controls");
export const conversationFontSize = $("conversation-font-size") as HTMLInputElement | null;
export const conversationFontSizeValue = $("conversation-font-size-value");
export const historyAliasControls = $("history-alias-controls") as HTMLDivElement | null;
export const historyAliasMicInput = $("history-alias-mic-input") as HTMLInputElement | null;
export const historyAliasSystemInput = $("history-alias-system-input") as HTMLInputElement | null;

// Chapter controls (Transcription tab)
export const chaptersContainer = $("chapters-container") as HTMLDivElement | null;
export const chaptersList = $("chapters-list") as HTMLDivElement | null;
export const chapterMethodSelect = $("chapter-method") as HTMLSelectElement | null;
export const chaptersToggle = $("chapters-toggle") as HTMLButtonElement | null;

// Chapter settings (Settings tab)
export const chaptersEnabled = $("chapters-enabled") as HTMLInputElement | null;
export const chaptersSettings = $("chapters-settings") as HTMLDivElement | null;
export const chaptersShowIn = $("chapters-show-in") as HTMLSelectElement | null;
export const chaptersMethod = $("chapters-method") as HTMLSelectElement | null;

// Extra hotkeys controls
export const toggleActivationWordsHotkey = $("toggle-activation-words-hotkey") as HTMLInputElement | null;
export const toggleActivationWordsHotkeyRecord = $("toggle-activation-words-hotkey-record") as HTMLButtonElement | null;
export const toggleActivationWordsHotkeyStatus = $("toggle-activation-words-hotkey-status") as HTMLSpanElement | null;

// Topic keywords controls (Settings tab)
export const topicKeywordsList = $("topic-keywords-list") as HTMLDivElement | null;
export const topicKeywordsReset = $("topic-keywords-reset") as HTMLButtonElement | null;

// Quality & Encoding controls
export const opusEnabledToggle = $("opus-enabled-toggle") as HTMLInputElement | null;
export const opusBitrateSelect = $("opus-bitrate-select") as HTMLSelectElement | null;
export const autoSaveSystemAudioToggle = $("auto-save-system-audio-toggle") as HTMLInputElement | null;
export const autoSaveMicAudioToggle = $("auto-save-mic-audio-toggle") as HTMLInputElement | null;

// Model manager controls
export const modelSourceSelect = $("model-source-select") as HTMLSelectElement | null;
export const modelCustomUrl = $("model-custom-url") as HTMLInputElement | null;
export const modelCustomUrlField = $("model-custom-url-field") as HTMLDivElement | null;
export const modelRefresh = $("model-refresh") as HTMLButtonElement | null;
export const modelStoragePath = $("model-storage-path") as HTMLInputElement | null;
export const modelStorageBrowse = $("model-storage-browse") as HTMLButtonElement | null;
export const modelStorageReset = $("model-storage-reset") as HTMLButtonElement | null;
export const modelList = $("model-list");
