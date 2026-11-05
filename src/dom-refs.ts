// DOM element references
// Centralized DOM queries to avoid repeated getElementById calls

const $ = <T extends HTMLElement>(id: string) => {
  if (typeof document === "undefined") return null;
  return document.getElementById(id) as T | null;
};

// Bootstrap overlay
export const bootstrapOverlay = $("bootstrap-overlay") as HTMLDivElement | null;
export const bootstrapLabel = $("bootstrap-label") as HTMLSpanElement | null;

// Main tabs
export const tabBtnTranscription = $("tab-btn-transcription") as HTMLButtonElement | null;
export const tabBtnSettings = $("tab-btn-settings") as HTMLButtonElement | null;
export const tabBtnAiRefinement = $("tab-btn-ai-refinement") as HTMLButtonElement | null;
export const tabTranscription = $("tab-transcription") as HTMLDivElement | null;
export const tabSettings = $("tab-settings") as HTMLDivElement | null;
export const tabAiRefinement = $("tab-ai-refinement") as HTMLDivElement | null;
export const expertModeToggle = $("expert-mode-toggle") as HTMLInputElement | null;
export const expertModeLabel = $("expert-mode-label") as HTMLSpanElement | null;

// Status and hero elements
export const statusLabel = $("status-label");
export const statusDot = $("status-dot") as HTMLSpanElement | null;
export const recordingPill = $("recording-pill");
export const refiningStatusDot = $("refining-dot") as HTMLSpanElement | null;
export const ollamaModelDot = $("ollama-model-dot") as HTMLSpanElement | null;
export const gpuStatusDot = $("gpu-dot") as HTMLSpanElement | null;
export const gpuBackendCudaBtn = $("gpu-backend-cuda") as HTMLButtonElement | null;
export const gpuBackendVulkanBtn = $("gpu-backend-vulkan") as HTMLButtonElement | null;
export const gpuPurgeBtn = $("gpu-purge-btn") as HTMLButtonElement | null;
export const statusMessage = $("status-message");
// Engine-row telemetry (header rework)
export const gpuUtil = $("gpu-util") as HTMLSpanElement | null;
export const gpuVramTotal = $("gpu-vram-total") as HTMLSpanElement | null;
export const engineWhisperModel = $("engine-whisper-model") as HTMLSpanElement | null;
export const engineWhisperVram = $("engine-whisper-vram") as HTMLSpanElement | null;
export const engineRefineRow = $("engine-refine") as HTMLDivElement | null;
export const engineRefineLabel = $("engine-refine-label") as HTMLSpanElement | null;
export const engineRefineModel = $("engine-refine-model") as HTMLSpanElement | null;
export const engineRefinePreset = $("engine-refine-preset") as HTMLButtonElement | null;
export const engineRefinePresetMenu = $("engine-refine-preset-menu") as HTMLUListElement | null;
export const engineRefineVram = $("engine-refine-vram") as HTMLSpanElement | null;
export const dictationBadge = $("dictation-badge");
export const appVersion = $("app-version") as HTMLSpanElement | null;
export const whisperInputLanguageSelect = $("whisper-input-language-select") as HTMLSelectElement | null;
export const whisperInputLanguageNote = $("whisper-input-language-note") as HTMLParagraphElement | null;

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
export const asrLanguageField = $("asr-language-field") as HTMLLabelElement | null;
export const languageSelect = $("language-select") as HTMLSelectElement | null;
export const asrLanguageHintNote = $("asr-language-hint-note") as HTMLSpanElement | null;
export const languagePinnedToggle = $("language-pinned-toggle") as HTMLInputElement | null;
export const audioCuesToggle = $("audio-cues-toggle") as HTMLInputElement | null;
export const audioCuesVolume = $("audio-cues-volume") as HTMLInputElement | null;
export const diagnosticLoggingToggle = $("diagnostic-logging-toggle") as HTMLInputElement | null;
export const pttUseVadToggle = $("ptt-use-vad-toggle") as HTMLInputElement | null;
export const pttHotKeepalive = $("ptt-hot-keepalive") as HTMLInputElement | null;
export const pttHotKeepaliveValue = $("ptt-hot-keepalive-value");
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
export const refinementPipelineNote = $("refinement-pipeline-note") as HTMLSpanElement | null;
export const refinementPipelineGraph = $("refinement-pipeline-graph") as HTMLDivElement | null;
export const refinementPipelineLive = $("refinement-pipeline-live") as HTMLDivElement | null;
export const postprocSettings = $("postproc-settings") as HTMLDivElement | null;
export const postprocLanguageDerived = $("postproc-language-derived") as HTMLSpanElement | null;
export const postprocPunctuation = $("postproc-punctuation") as HTMLInputElement | null;
export const postprocCapitalization = $("postproc-capitalization") as HTMLInputElement | null;
export const postprocNumbers = $("postproc-numbers") as HTMLInputElement | null;
export const postprocCustomVocabEnabled = $("postproc-custom-vocab-enabled") as HTMLInputElement | null;
export const postprocCustomVocabConfig = $("postproc-custom-vocab-config") as HTMLDivElement | null;
export const postprocVocabRows = $("postproc-vocab-rows") as HTMLDivElement | null;
export const postprocVocabAdd = $("postproc-vocab-add") as HTMLButtonElement | null;
export const vocabTermsList = $("vocab-terms-list") as HTMLDivElement | null;
export const vocabTermsCount = $("vocab-terms-count") as HTMLSpanElement | null;
export const vocabObservingList = $("vocab-observing-list") as HTMLDivElement | null;
export const aiFallbackEnabled = $("ai-fallback-enabled") as HTMLInputElement | null;
export const aiFallbackSettings = $("ai-fallback-settings") as HTMLDivElement | null;
export const aiFallbackLoadingScrim = $("ai-fallback-loading-scrim") as HTMLDivElement | null;
export const aiFallbackLoadingTitle = $("ai-fallback-loading-title") as HTMLSpanElement | null;
export const aiFallbackLoadingDetail = $("ai-fallback-loading-detail") as HTMLSpanElement | null;
export const aiFallbackLocalLane = $("ai-fallback-local-lane") as HTMLDivElement | null;
export const aiFallbackLocalPrimaryStatus = $("ai-fallback-local-primary-status") as HTMLSpanElement | null;
export const aiFallbackLocalPrimaryAction = $("ai-fallback-local-primary-action") as HTMLButtonElement | null;
export const aiFallbackLocalImportAction = $("ai-fallback-local-import-action") as HTMLButtonElement | null;
export const aiFallbackLocalVerifyAction = $("ai-fallback-local-verify-action") as HTMLButtonElement | null;
export const aiFallbackLocalRefreshAction = $("ai-fallback-local-refresh-action") as HTMLButtonElement | null;
export const aiFallbackRuntimeVersionList = $("ai-fallback-runtime-version-list") as HTMLDivElement | null;
export const aiFallbackFetchVersionsAction = $("ai-fallback-fetch-versions-action") as HTMLButtonElement | null;
export const aiFallbackFetchVersionsStatus = $("ai-fallback-fetch-versions-status") as HTMLSpanElement | null;
export const aiFallbackLocalRuntimeNote = $("ai-fallback-local-runtime-note") as HTMLSpanElement | null;
export const aiFallbackRuntimeProgress = $("ai-fallback-runtime-progress") as HTMLDivElement | null;
export const aiFallbackRuntimeProgressFill = $("ai-fallback-runtime-progress-fill") as HTMLDivElement | null;
export const aiFallbackRuntimeProgressText = $("ai-fallback-runtime-progress-text") as HTMLSpanElement | null;
export const aiFallbackTemperature = $("ai-fallback-temperature") as HTMLInputElement | null;
export const aiFallbackTemperatureValue = $("ai-fallback-temperature-value") as HTMLSpanElement | null;
export const aiFallbackPreserveLanguage = $("ai-fallback-preserve-language") as HTMLInputElement | null;
export const aiFallbackPreserveLanguageNote = $("ai-fallback-preserve-language-note") as HTMLSpanElement | null;
export const aiFallbackLowLatencyMode = $("ai-fallback-low-latency-mode") as HTMLInputElement | null;
export const aiFallbackLowLatencyNote = $("ai-fallback-low-latency-note") as HTMLSpanElement | null;
export const aiFallbackMaxTokens = $("ai-fallback-max-tokens") as HTMLSelectElement | null;
export const promptPresetList = $("prompt-preset-list") as HTMLDivElement | null;
export const aiFallbackPresetNameField = $("ai-fallback-preset-name-field") as HTMLDivElement | null;
export const aiFallbackPresetNameInputWrap = $("ai-fallback-preset-name-input-wrap") as HTMLDivElement | null;
export const aiFallbackPromptPresetName = $("ai-fallback-prompt-preset-name") as HTMLInputElement | null;
export const aiFallbackPromptPresetSave = $("ai-fallback-prompt-preset-save") as HTMLButtonElement | null;
export const aiFallbackPromptPresetReset = $("ai-fallback-prompt-preset-reset") as HTMLButtonElement | null;
export const aiFallbackPromptPresetRevert = $("ai-fallback-prompt-preset-revert") as HTMLButtonElement | null;
export const aiFallbackPromptPresetDiscard = $("ai-fallback-prompt-preset-discard") as HTMLButtonElement | null;
export const aiFallbackPromptPresetDelete = $("ai-fallback-prompt-preset-delete") as HTMLButtonElement | null;
export const aiFallbackPromptPreviewLabel = $("ai-fallback-prompt-preview-label") as HTMLSpanElement | null;
export const aiFallbackPromptPreviewHint = $("ai-fallback-prompt-preview-hint") as HTMLSpanElement | null;
export const aiFallbackCustomPrompt = $("ai-fallback-custom-prompt") as HTMLTextAreaElement | null;

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
export const accentColor = $("accent-color") as HTMLInputElement | null;
export const accentColorReset = $("accent-color-reset") as HTMLButtonElement | null;
export const overlayRefiningIndicatorEnabled = $("overlay-refining-indicator-enabled") as HTMLInputElement | null;
export const overlayRefiningIndicatorPreset = $("overlay-refining-indicator-preset") as HTMLSelectElement | null;
export const overlayRefiningIndicatorColor = $("overlay-refining-indicator-color") as HTMLInputElement | null;
export const overlayRefiningIndicatorSpeed = $("overlay-refining-indicator-speed") as HTMLInputElement | null;
export const overlayRefiningIndicatorSpeedValue = $("overlay-refining-indicator-speed-value");
export const overlayRefiningIndicatorRange = $("overlay-refining-indicator-range") as HTMLInputElement | null;
export const overlayRefiningIndicatorRangeValue = $("overlay-refining-indicator-range-value");
export const overlayDotSettings = $("overlay-dot-settings") as HTMLDivElement | null;
export const overlayKittSettings = $("overlay-kitt-settings") as HTMLDivElement | null;
export const overlayKittMinWidth = $("overlay-kitt-min-width") as HTMLInputElement | null;
export const overlayKittMinWidthValue = $("overlay-kitt-min-width-value");
export const overlayKittMaxWidth = $("overlay-kitt-max-width") as HTMLInputElement | null;
export const overlayKittMaxWidthValue = $("overlay-kitt-max-width-value");
export const overlayKittHeight = $("overlay-kitt-height") as HTMLInputElement | null;
export const overlayKittHeightValue = $("overlay-kitt-height-value");
export const overlayHealthNote = $("overlay-health-note") as HTMLSpanElement | null;

// History controls
export const historyList = $("history-list");
export const historyDeleteConversation = $("history-delete-conversation") as HTMLButtonElement | null;
export const historyExport = $("history-export") as HTMLButtonElement | null;
export const historySearch = $("history-search") as HTMLInputElement | null;
export const historySearchClear = $("history-search-clear") as HTMLButtonElement | null;
export const conversationFontControls = $("conversation-font-controls");
export const conversationFontSize = $("conversation-font-size") as HTMLInputElement | null;
export const conversationFontSizeValue = $("conversation-font-size-value");
export const refinementInspector = $("refinement-inspector") as HTMLDetailsElement | null;
export const refinementInspectorEmpty = $("refinement-inspector-empty") as HTMLDivElement | null;
export const refinementInspectorContent = $("refinement-inspector-content") as HTMLDivElement | null;
export const refinementInspectorStatus = $("refinement-inspector-status") as HTMLSpanElement | null;
export const refinementInspectorMeta = $("refinement-inspector-meta") as HTMLSpanElement | null;
export const refinementInspectorRaw = $("refinement-inspector-raw") as HTMLDivElement | null;
export const refinementInspectorRefined = $("refinement-inspector-refined") as HTMLDivElement | null;
export const refinementInspectorDiff = $("refinement-inspector-diff") as HTMLDivElement | null;
export const refinementInspectorError = $("refinement-inspector-error") as HTMLDivElement | null;

// Modules tab


// Export dialog
export const exportDialog = $("export-dialog") as HTMLDivElement | null;
export const exportDialogBackdrop = $("export-dialog-backdrop") as HTMLDivElement | null;
export const exportDialogClose = $("export-dialog-close") as HTMLButtonElement | null;
export const exportCustomRange = $("export-custom-range") as HTMLDivElement | null;
export const exportCustomFrom = $("export-custom-from") as HTMLInputElement | null;
export const exportCustomTo = $("export-custom-to") as HTMLInputElement | null;
export const exportDialogFormat = $("export-dialog-format") as HTMLSelectElement | null;
export const exportPreviewCount = $("export-preview-count") as HTMLSpanElement | null;
export const exportPreviewSpan = $("export-preview-span") as HTMLSpanElement | null;
export const exportDialogRun = $("export-dialog-run") as HTMLButtonElement | null;

// Archive browser

// Extra hotkeys controls
export const toggleActivationWordsHotkey = $("toggle-activation-words-hotkey") as HTMLInputElement | null;
export const toggleActivationWordsHotkeyRecord = $("toggle-activation-words-hotkey-record") as HTMLButtonElement | null;
export const toggleActivationWordsHotkeyStatus = $("toggle-activation-words-hotkey-status") as HTMLSpanElement | null;

// Topic keywords controls (Settings tab)
export const topicKeywordsList = $("topic-keywords-list") as HTMLDivElement | null;
export const topicKeywordsReset = $("topic-keywords-reset") as HTMLButtonElement | null;

// Quality & Encoding controls

// Model manager controls
export const modelSourceSelect = $("model-source-select") as HTMLSelectElement | null;
export const modelCustomUrl = $("model-custom-url") as HTMLInputElement | null;
export const modelCustomUrlField = $("model-custom-url-field") as HTMLDivElement | null;
export const modelRefresh = $("model-refresh") as HTMLButtonElement | null;
export const modelStoragePath = $("model-storage-path") as HTMLInputElement | null;
export const modelStorageBrowse = $("model-storage-browse") as HTMLButtonElement | null;
export const modelStorageReset = $("model-storage-reset") as HTMLButtonElement | null;
export const modelList = $("model-list");

// Overlay controls (button not reachable via id helper above)
export const applyOverlayBtn = $("apply-overlay-btn") as HTMLButtonElement | null;

// Module config modal

// Model panel (queried by data attribute, not id)
export const modelPanel =
  typeof document !== "undefined"
    ? (document.querySelector('[data-panel="model"]') as HTMLElement | null)
    : null;

// Onboarding Wizard
export const onboardingWizard = $("onboarding-wizard");
export const wizardStepCurrent = $("wizard-step-current");
export const wizardNextBtn = $("wizard-next-btn") as HTMLButtonElement | null;
export const wizardPrevBtn = $("wizard-prev-btn") as HTMLButtonElement | null;
export const wizardFinishBtn = $("wizard-finish-btn") as HTMLButtonElement | null;
export const wizardHotkeyInput = $("wizard-hotkey-input") as HTMLInputElement | null;
export const wizardSetupHotkeyBtn = $("wizard-setup-hotkey-btn") as HTMLButtonElement | null;
export const wizardHotkeyStatus = $("wizard-hotkey-status") as HTMLSpanElement | null;
