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
export const tabBtnModules = $("tab-btn-modules") as HTMLButtonElement | null;
export const tabTranscription = $("tab-transcription") as HTMLDivElement | null;
export const tabSettings = $("tab-settings") as HTMLDivElement | null;
export const tabAiRefinement = $("tab-ai-refinement") as HTMLDivElement | null;
export const tabModules = $("tab-modules") as HTMLDivElement | null;
export const expertModeToggle = $("expert-mode-toggle") as HTMLInputElement | null;
export const expertModeLabel = $("expert-mode-label") as HTMLSpanElement | null;

// Status and hero elements
export const statusLabel = $("status-label");
export const statusDot = $("status-dot") as HTMLSpanElement | null;
export const recordingPill = $("recording-pill");
export const transcribeStatusDot = $("transcribe-dot") as HTMLSpanElement | null;
export const transcribeStatusLabel = $("transcribe-label");
export const transcribePill = $("transcribe-pill");
export const refiningStatusDot = $("refining-dot") as HTMLSpanElement | null;
export const refiningStatusLabel = $("refining-label");
export const refiningPill = $("refining-pill");
export const gpuStatusDot = $("gpu-dot") as HTMLSpanElement | null;
export const gpuStatusLabel = $("gpu-label");
export const gpuVramLabel = $("gpu-vram") as HTMLSpanElement | null;
export const gpuStatusItem = $("gpu-status-item") as HTMLDivElement | null;
export const gpuPill = $("gpu-pill");
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
export const asrLanguageField = $("asr-language-field") as HTMLLabelElement | null;
export const languageSelect = $("language-select") as HTMLSelectElement | null;
export const asrLanguageHintNote = $("asr-language-hint-note") as HTMLSpanElement | null;
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
export const aiFallbackEnabled = $("ai-fallback-enabled") as HTMLInputElement | null;
export const aiFallbackSettings = $("ai-fallback-settings") as HTMLDivElement | null;
export const aiFallbackLoadingScrim = $("ai-fallback-loading-scrim") as HTMLDivElement | null;
export const aiFallbackLoadingTitle = $("ai-fallback-loading-title") as HTMLSpanElement | null;
export const aiFallbackLoadingDetail = $("ai-fallback-loading-detail") as HTMLSpanElement | null;
export const aiFallbackProviderLanes = $("ai-fallback-provider-lanes") as HTMLDivElement | null;
export const aiFallbackLocalLane = $("ai-fallback-local-lane") as HTMLDivElement | null;
export const aiFallbackOnlineLane = $("ai-fallback-online-lane") as HTMLDivElement | null;
export const aiFallbackLocalPrimaryStatus = $("ai-fallback-local-primary-status") as HTMLSpanElement | null;
export const aiFallbackLocalPrimaryAction = $("ai-fallback-local-primary-action") as HTMLButtonElement | null;
export const aiFallbackLocalImportAction = $("ai-fallback-local-import-action") as HTMLButtonElement | null;
export const aiFallbackLocalAdvanced = $("ai-fallback-local-advanced") as HTMLDetailsElement | null;
export const aiFallbackLocalVerifyAction = $("ai-fallback-local-verify-action") as HTMLButtonElement | null;
export const aiFallbackLocalRefreshAction = $("ai-fallback-local-refresh-action") as HTMLButtonElement | null;
export const aiFallbackLocalRuntimeVersion = $("ai-fallback-local-runtime-version") as HTMLSelectElement | null;
export const aiFallbackFetchVersionsAction = $("ai-fallback-fetch-versions-action") as HTMLButtonElement | null;
export const aiFallbackFetchVersionsStatus = $("ai-fallback-fetch-versions-status") as HTMLSpanElement | null;
export const aiFallbackLocalRuntimeSource = $("ai-fallback-local-runtime-source") as HTMLSelectElement | null;
export const aiFallbackLocalRuntimeVersionNote = $("ai-fallback-local-runtime-version-note") as HTMLSpanElement | null;
export const aiFallbackLocalRuntimeNote = $("ai-fallback-local-runtime-note") as HTMLSpanElement | null;
export const aiFallbackLocalFallbackEndpoints = $("ai-fallback-local-fallback-endpoints") as HTMLTextAreaElement | null;
export const aiFallbackLocalBackendSelect = $("ai-fallback-local-backend-select") as HTMLSelectElement | null;
export const aiFallbackCompatConfig = $("ai-fallback-compat-config") as HTMLDivElement | null;
export const aiFallbackCompatGuide = $("ai-fallback-compat-guide") as HTMLParagraphElement | null;
export const aiFallbackCompatEndpoint = $("ai-fallback-compat-endpoint") as HTMLInputElement | null;
export const aiFallbackCompatEndpointHint = $("ai-fallback-compat-endpoint-hint") as HTMLSpanElement | null;
export const aiFallbackCompatApiKey = $("ai-fallback-compat-api-key") as HTMLInputElement | null;
export const aiFallbackCompatModel = $("ai-fallback-compat-model") as HTMLSelectElement | null; // legacy, may be null
export const aiFallbackCompatModelList = $("ai-fallback-compat-model-list") as HTMLDivElement | null;
export const aiFallbackCompatFetchModels = $("ai-fallback-compat-fetch-models") as HTMLButtonElement | null;
export const aiFallbackCompatStatus = $("ai-fallback-compat-status") as HTMLSpanElement | null;
export const aiFallbackCompatVerifyAction = $("ai-fallback-compat-verify-action") as HTMLButtonElement | null;
export const aiFallbackLmStudioInstallAction = $("ai-fallback-lm-studio-install-action") as HTMLButtonElement | null;
export const aiFallbackRuntimeProgress = $("ai-fallback-runtime-progress") as HTMLDivElement | null;
export const aiFallbackRuntimeProgressFill = $("ai-fallback-runtime-progress-fill") as HTMLDivElement | null;
export const aiFallbackRuntimeProgressText = $("ai-fallback-runtime-progress-text") as HTMLSpanElement | null;
export const aiFallbackOnlineStatusBadge = $("ai-fallback-online-status-badge") as HTMLSpanElement | null;
export const aiFallbackCloudProviderList = $("ai-fallback-cloud-provider-list") as HTMLDivElement | null;
export const aiFallbackFallbackStatus = $("ai-fallback-fallback-status") as HTMLSpanElement | null;
export const aiFallbackModelField = $("ai-fallback-model-field") as HTMLLabelElement | null;
export const aiFallbackModel = $("ai-fallback-model") as HTMLSelectElement | null;
export const aiFallbackOllamaManagedNote = $("ai-fallback-ollama-managed-note") as HTMLDivElement | null;
export const aiFallbackTemperature = $("ai-fallback-temperature") as HTMLInputElement | null;
export const aiFallbackTemperatureValue = $("ai-fallback-temperature-value") as HTMLSpanElement | null;
export const aiFallbackPreserveLanguage = $("ai-fallback-preserve-language") as HTMLInputElement | null;
export const aiFallbackPreserveLanguageNote = $("ai-fallback-preserve-language-note") as HTMLSpanElement | null;
export const aiFallbackLowLatencyMode = $("ai-fallback-low-latency-mode") as HTMLInputElement | null;
export const aiFallbackLowLatencyNote = $("ai-fallback-low-latency-note") as HTMLSpanElement | null;
export const aiFallbackMaxTokens = $("ai-fallback-max-tokens") as HTMLSelectElement | null;
export const aiFallbackPromptPreset = $("ai-fallback-prompt-preset") as HTMLSelectElement | null;
export const aiFallbackPromptPresetName = $("ai-fallback-prompt-preset-name") as HTMLInputElement | null;
export const aiFallbackPromptPresetSave = $("ai-fallback-prompt-preset-save") as HTMLButtonElement | null;
export const aiFallbackPromptPresetDelete = $("ai-fallback-prompt-preset-delete") as HTMLButtonElement | null;
export const aiFallbackPromptPreviewField = $("ai-fallback-prompt-preview-field") as HTMLDivElement | null;
export const aiFallbackPromptPreviewLabel = $("ai-fallback-prompt-preview-label") as HTMLSpanElement | null;
export const aiFallbackPromptPreviewHint = $("ai-fallback-prompt-preview-hint") as HTMLSpanElement | null;
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
export const historyTabMic = $("history-tab-mic");
export const historyTabSystem = $("history-tab-system");
export const historyTabConversation = $("history-tab-conversation");
export const historyCopyConversation = $("history-copy-conversation") as HTMLButtonElement | null;
export const historyDeleteConversation = $("history-delete-conversation") as HTMLButtonElement | null;
export const analyseButton = $("analyse-button") as HTMLButtonElement | null;
export const historyExport = $("history-export") as HTMLButtonElement | null;
export const openRecordingsBtn = $("open-recordings-btn") as HTMLButtonElement | null;
export const archiveBrowseBtn = $("archive-browse-btn") as HTMLButtonElement | null;
export const openModulesBtn = $("open-modules-btn") as HTMLButtonElement | null;
export const historySearch = $("history-search") as HTMLInputElement | null;
export const historySearchClear = $("history-search-clear") as HTMLButtonElement | null;
export const conversationFontControls = $("conversation-font-controls");
export const conversationFontSize = $("conversation-font-size") as HTMLInputElement | null;
export const conversationFontSizeValue = $("conversation-font-size-value");
export const historyAliasControls = $("history-alias-controls") as HTMLDivElement | null;
export const historyAliasMicInput = $("history-alias-mic-input") as HTMLInputElement | null;
export const historyAliasSystemInput = $("history-alias-system-input") as HTMLInputElement | null;
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
export const modulesList = $("modules-list") as HTMLDivElement | null;
export const modulesStatus = $("modules-status") as HTMLSpanElement | null;
export const workflowAgentConsole = $("workflow-agent-console") as HTMLDivElement | null;
export const workflowAgentStatus = $("workflow-agent-status") as HTMLSpanElement | null;
export const workflowAgentCommandInput = $("workflow-agent-command-input") as HTMLTextAreaElement | null;
export const workflowAgentParseBtn = $("workflow-agent-parse-btn") as HTMLButtonElement | null;
export const workflowAgentRefreshCandidatesBtn = $("workflow-agent-refresh-candidates-btn") as HTMLButtonElement | null;
export const workflowAgentCandidates = $("workflow-agent-candidates") as HTMLDivElement | null;
export const workflowAgentTargetLanguage = $("workflow-agent-target-language") as HTMLSelectElement | null;
export const workflowAgentBuildPlanBtn = $("workflow-agent-build-plan-btn") as HTMLButtonElement | null;
export const workflowAgentExecuteBtn = $("workflow-agent-execute-btn") as HTMLButtonElement | null;
export const workflowAgentPlanPreview = $("workflow-agent-plan-preview") as HTMLTextAreaElement | null;
export const workflowAgentExecutionLog = $("workflow-agent-execution-log") as HTMLTextAreaElement | null;

// GDD flow modal
export const gddFlowModal = $("gdd-flow-modal") as HTMLDivElement | null;
export const gddFlowBackdrop = $("gdd-flow-backdrop") as HTMLDivElement | null;
export const gddFlowClose = $("gdd-flow-close") as HTMLButtonElement | null;
export const gddFlowModeStandard = $("gdd-flow-mode-standard") as HTMLButtonElement | null;
export const gddFlowModeAdvanced = $("gdd-flow-mode-advanced") as HTMLButtonElement | null;
export const gddFlowRuntimeSummary = $("gdd-flow-runtime-summary") as HTMLSpanElement | null;
export const gddFlowPreset = $("gdd-flow-preset") as HTMLSelectElement | null;
export const gddFlowDetectPreset = $("gdd-flow-detect-preset") as HTMLButtonElement | null;
export const gddFlowTitle = $("gdd-flow-title") as HTMLInputElement | null;
export const gddFlowMaxChunk = $("gdd-flow-max-chunk") as HTMLSelectElement | null;
export const gddFlowTemplateSource = $("gdd-flow-template-source") as HTMLSelectElement | null;
export const gddFlowTemplateConfluenceGroup = $("gdd-flow-template-confluence-group") as HTMLDivElement | null;
export const gddFlowTemplateConfluenceUrl = $("gdd-flow-template-confluence-url") as HTMLInputElement | null;
export const gddFlowTemplateConfluenceLoad = $("gdd-flow-template-confluence-load") as HTMLButtonElement | null;
export const gddFlowTemplateFileGroup = $("gdd-flow-template-file-group") as HTMLDivElement | null;
export const gddFlowTemplateFilePath = $("gdd-flow-template-file-path") as HTMLInputElement | null;
export const gddFlowTemplateFilePick = $("gdd-flow-template-file-pick") as HTMLButtonElement | null;
export const gddFlowTemplateFileLoad = $("gdd-flow-template-file-load") as HTMLButtonElement | null;
export const gddFlowTemplateMeta = $("gdd-flow-template-meta") as HTMLSpanElement | null;
export const gddFlowTemplatePreview = $("gdd-flow-template-preview") as HTMLTextAreaElement | null;
export const gddFlowGenerate = $("gdd-flow-generate") as HTMLButtonElement | null;
export const gddFlowValidate = $("gdd-flow-validate") as HTMLButtonElement | null;
export const gddFlowStatus = $("gdd-flow-status") as HTMLSpanElement | null;
export const gddFlowSpaceKey = $("gdd-flow-space-key") as HTMLInputElement | null;
export const gddFlowParentPageId = $("gdd-flow-parent-page-id") as HTMLInputElement | null;
export const gddFlowTargetPageId = $("gdd-flow-target-page-id") as HTMLInputElement | null;
export const gddFlowSuggestTarget = $("gdd-flow-suggest-target") as HTMLButtonElement | null;
export const gddFlowOneClickPublish = $("gdd-flow-one-click-publish") as HTMLButtonElement | null;
export const gddFlowPublish = $("gdd-flow-publish") as HTMLButtonElement | null;
export const gddFlowPublishLink = $("gdd-flow-publish-link") as HTMLAnchorElement | null;
export const gddFlowOutput = $("gdd-flow-output") as HTMLTextAreaElement | null;
export const gddFlowQueueRefresh = $("gdd-flow-queue-refresh") as HTMLButtonElement | null;
export const gddFlowQueueList = $("gdd-flow-queue-list") as HTMLDivElement | null;

// Export dialog
export const exportDialog = $("export-dialog") as HTMLDivElement | null;
export const exportDialogBackdrop = $("export-dialog-backdrop") as HTMLDivElement | null;
export const exportDialogClose = $("export-dialog-close") as HTMLButtonElement | null;
export const exportCustomRange = $("export-custom-range") as HTMLDivElement | null;
export const exportCustomFrom = $("export-custom-from") as HTMLInputElement | null;
export const exportCustomTo = $("export-custom-to") as HTMLInputElement | null;
export const exportIncludeMic = $("export-include-mic") as HTMLInputElement | null;
export const exportIncludeSystem = $("export-include-system") as HTMLInputElement | null;
export const exportDialogFormat = $("export-dialog-format") as HTMLSelectElement | null;
export const exportPreviewCount = $("export-preview-count") as HTMLSpanElement | null;
export const exportPreviewSpan = $("export-preview-span") as HTMLSpanElement | null;
export const exportDialogRun = $("export-dialog-run") as HTMLButtonElement | null;

// Archive browser
export const archiveBrowser = $("archive-browser") as HTMLDivElement | null;
export const archiveBrowserBackdrop = $("archive-browser-backdrop") as HTMLDivElement | null;
export const archiveBrowserClose = $("archive-browser-close") as HTMLButtonElement | null;
export const archiveMicPartitions = $("archive-mic-partitions") as HTMLDivElement | null;
export const archiveSystemPartitions = $("archive-system-partitions") as HTMLDivElement | null;
export const archiveSelectionMeta = $("archive-selection-meta") as HTMLSpanElement | null;
export const archiveEntries = $("archive-entries") as HTMLDivElement | null;
export const archiveExportFormat = $("archive-export-format") as HTMLSelectElement | null;
export const archiveExportBtn = $("archive-export-btn") as HTMLButtonElement | null;

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

// Overlay controls (button not reachable via id helper above)
export const applyOverlayBtn = $("apply-overlay-btn") as HTMLButtonElement | null;

// Voice Output Console (Module config modal)
export const voiceOutputConsole = $("voice-output-console") as HTMLElement | null;
export const voiceOutputConsoleStatus = $("voice-output-console-status") as HTMLSpanElement | null;

// Module config modal
export const moduleConfigModal = $("module-config-modal") as HTMLDivElement | null;
export const moduleConfigModalBackdrop = $("module-config-modal-backdrop") as HTMLDivElement | null;
export const moduleConfigModalClose = $("module-config-modal-close") as HTMLButtonElement | null;
export const moduleConfigModalName = $("module-config-modal-name") as HTMLElement | null;
export const moduleConfigModalMeta = $("module-config-modal-meta") as HTMLElement | null;
export const moduleConfigModalDesc = $("module-config-modal-desc") as HTMLElement | null;
export const moduleConfigModalUsage = $("module-config-modal-usage") as HTMLElement | null;
export const moduleConfigModalDeps = $("module-config-modal-deps") as HTMLDivElement | null;
export const moduleConfigModalFeedback = $("module-config-modal-feedback") as HTMLDivElement | null;

// Voice Output Settings
export const voiceOutputDefaultProvider = $("voice-output-default-provider") as HTMLSelectElement | null;
export const voiceOutputFallbackProvider = $("voice-output-fallback-provider") as HTMLSelectElement | null;
export const voiceOutputPolicy = $("voice-output-policy") as HTMLSelectElement | null;
export const voiceOutputRate = $("voice-output-rate") as HTMLInputElement | null;
export const voiceOutputRateValue = $("voice-output-rate-value") as HTMLSpanElement | null;
export const voiceOutputVolume = $("voice-output-volume") as HTMLInputElement | null;
export const voiceOutputVolumeValue = $("voice-output-volume-value") as HTMLSpanElement | null;
export const voiceOutputTestBtn = $("voice-output-test-btn") as HTMLButtonElement | null;
export const voiceOutputTestStatus = $("voice-output-test-status") as HTMLSpanElement | null;
export const voiceOutputPiperBinary = $("voice-output-piper-binary") as HTMLInputElement | null;
export const voiceOutputPiperModel = $("voice-output-piper-model") as HTMLInputElement | null;
export const voiceOutputPiperModelDir = $("voice-output-piper-model-dir") as HTMLInputElement | null;

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
export const wizardGpuName = $("wizard-gpu-name");
export const wizardGpuVram = $("wizard-gpu-vram");
export const wizardLoading = $("wizard-loading");
export const wizardGpuInfo = $("wizard-gpu-info");
export const wizardBackendRecommended = $("wizard-backend-recommended");
export const wizardDriverWarning = $("wizard-driver-warning");
export const wizardDriverLink = $("wizard-driver-link");
export const wizardHotkeyInput = $("wizard-hotkey-input") as HTMLInputElement | null;
export const wizardSetupHotkeyBtn = $("wizard-setup-hotkey-btn") as HTMLButtonElement | null;
export const wizardHotkeyStatus = $("wizard-hotkey-status") as HTMLSpanElement | null;
export const wizardOllamaEnable = $("wizard-ollama-enable") as HTMLInputElement | null;
export const wizardOllamaStatus = $("wizard-ollama-status");
