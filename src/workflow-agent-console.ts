import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { settings } from "./state";
import { showToast } from "./toast";
import { isAmbiguousSelection, isValidTargetLanguage } from "./workflow-agent-policy";
import type {
  AgentBuildExecutionPlanRequest,
  AgentCommandParseResult,
  AgentExecutionPlan,
  AgentExecutionResult,
  AgentReplyResult,
  AssistantAwaitingConfirmationEvent,
  AssistantConfirmationExpiredEvent,
  AssistantIntentDetectedEvent,
  AssistantPlanReadyEvent,
  AssistantStateChangedEvent,
  TranscriptionResultEvent,
  TranscriptionRawResultEvent,
  TranscriptSessionCandidate,
} from "./types";

let initialized = false;
let lastParse: AgentCommandParseResult | null = null;
let lastCandidates: TranscriptSessionCandidate[] = [];
let selectedSessionId = "";
let currentPlan: AgentExecutionPlan | null = null;
let languageExplicitlySet = false;
let latestAssistantState: AssistantStateChangedEvent | null = null;
let latestHeardLine = "No utterance captured yet.";
let latestGateLine = "No gate decisions yet.";
let latestIntentLine = "No intent detected yet.";
let latestReplyLine = "No replies yet.";
let latestConfirmationLine = "No pending confirmation.";
let ttsSpeaking = false;
let pendingConfirmationToken: string | null = null;
let pendingConfirmationExpiresAtMs: number | null = null;
let pendingConfirmationTimer: number | null = null;
let agentPttArmedUntilMs: number | null = null;
let lastHandledTranscriptKey = "";
let lastHandledTranscriptAtMs = 0;
let lastGateToastAtMs = 0;
let transcribeAssistantHoldUntilMs = 0;
let uiAgentStage: AssistantStateChangedEvent["state"] = "idle";
let uiAgentStageNote = "Idle: waiting for wakeword or PTT command.";
let uiAgentStageResetTimer: number | null = null;

const CONFIRM_KEYWORDS = ["confirm", "confirmed", "bestätigen", "bestaetigen", "freigeben", "ok"];
const CANCEL_KEYWORDS = ["cancel", "abbrechen", "stopp", "stop"];
const TRANSCRIBE_DIRECTIVE_PREFIXES = [
  "schreib mal bitte",
  "schreibe mal bitte",
  "schreib bitte",
  "transcribe",
  "transkribiere",
  "parse mal bitte folgendes",
];
const AGENT_PTT_ARM_WINDOW_MS = 12_000;
const AGENT_TRANSCRIPT_DEDUPE_WINDOW_MS = 1_600;
const AGENT_GATE_TOAST_COOLDOWN_MS = 8_000;
const TRANSCRIBE_ASSISTANT_HOLD_MS = 5_000;
const AGENT_STAGE_ORDER: AssistantStateChangedEvent["state"][] = [
  "idle",
  "listening",
  "parsing",
  "planning",
  "awaiting_confirm",
  "executing",
  "recovering",
];
const BUILTIN_WAKEWORD_ALIASES = ["trispa", "trisper", "trispar", "trispur"];

function ensureWorkflowAgentDefaults(): void {
  if (!settings) return;
  settings.workflow_agent ??= {
    enabled: false,
    wakewords: ["trispr", "hey trispr", "trispr agent"],
    wakeword_aliases: [],
    intent_keywords: {
      gdd_generate_publish: [
        "gdd",
        "game design document",
        "designdokument",
        "publish",
        "confluence",
        "generate",
        "draft",
      ],
    },
    model: "qwen3:4b",
    temperature: 0.2,
    max_tokens: 512,
    session_gap_minutes: 20,
    max_candidates: 3,
    hands_free_enabled: false,
    confirm_timeout_sec: 45,
    reply_mode: "rule_only",
    online_enabled: false,
    voice_feedback_enabled: false,
  };
  settings.workflow_agent.hands_free_enabled ??= false;
  settings.workflow_agent.confirm_timeout_sec ??= 45;
  settings.workflow_agent.reply_mode ??= "rule_only";
  settings.workflow_agent.online_enabled ??= false;
  settings.workflow_agent.voice_feedback_enabled ??= false;
  settings.workflow_agent.wakeword_aliases ??= [];
}

export function handleTtsSpeechStarted(): void {
  ttsSpeaking = true;
}

export function handleTtsSpeechFinished(): void {
  ttsSpeaking = false;
}

function isModuleEnabled(moduleId: string): boolean {
  return settings?.module_settings?.enabled_modules?.includes(moduleId) ?? false;
}

function isWorkflowAgentEnabled(): boolean {
  ensureWorkflowAgentDefaults();
  return isModuleEnabled("workflow_agent") && Boolean(settings?.workflow_agent?.enabled);
}

function isAssistantModeEnabled(): boolean {
  return settings?.product_mode === "assistant";
}

function capabilityLabel(capabilityId: string): string {
  switch (capabilityId) {
    case "output_voice_tts":
      return "Voice Output (TTS)";
    case "input_vision":
      return "Vision Input";
    case "workflow_agent":
      return "Workflow Agent";
    case "product_mode_assistant":
      return "Assistant Product Mode";
    default:
      return capabilityId;
  }
}

function formatCapabilityList(capabilityIds: string[]): string {
  const labels = capabilityIds.map((id) => capabilityLabel(id));
  return labels.length > 0 ? labels.join(", ") : "none";
}

function assistantReasonGuidance(reason: string | null | undefined): string {
  switch ((reason ?? "").trim()) {
    case "product_mode_transcribe":
      return "Switch Product mode to Assistant.";
    case "workflow_agent_unavailable":
      return "Enable Workflow Agent module and settings.";
    case "assistant_degraded_capability":
      return "Assistant is running with reduced capabilities.";
    default:
      return "";
  }
}

function requireAssistantInteraction(actionLabel: string): boolean {
  if (!isWorkflowAgentEnabled()) {
    showToast({
      type: "warning",
      title: "Workflow Agent disabled",
      message: `Enable Workflow Agent before ${actionLabel}.`,
      duration: 3200,
    });
    return false;
  }
  return true;
}

function suggestionLevelFromMaxCandidates(maxCandidates: number): "low" | "standard" | "high" {
  if (maxCandidates <= 2) return "low";
  if (maxCandidates >= 5) return "high";
  return "standard";
}

function maxCandidatesFromSuggestionLevel(level: string): number {
  if (level === "low") return 2;
  if (level === "high") return 5;
  return 3;
}

function parseWakewordsInput(value: string): string[] {
  const seen = new Set<string>();
  const parsed: string[] = [];
  for (const token of value.split(/[\n,]+/)) {
    const normalized = token.trim().toLowerCase();
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    parsed.push(normalized);
  }
  return parsed;
}

function parseWakewordAliasesInput(value: string): string[] {
  const seen = new Set<string>();
  const parsed: string[] = [];
  for (const token of value.split(/[\n,]+/)) {
    const normalized = normalizeWakewordText(token);
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    parsed.push(normalized);
  }
  return parsed;
}

async function persistWorkflowAgentSettings(): Promise<void> {
  if (!settings) return;
  try {
    await Promise.race([
      invoke("save_settings", { settings }),
      new Promise<void>((_, reject) =>
        setTimeout(() => reject(new Error("save_settings timed out")), 3_000)
      ),
    ]);
  } catch (error) {
    console.error("Failed to persist workflow agent settings", error);
  }
}

function clearPendingConfirmationTimer(): void {
  if (pendingConfirmationTimer !== null) {
    window.clearTimeout(pendingConfirmationTimer);
    pendingConfirmationTimer = null;
  }
}

function clearPendingConfirmationState(): void {
  pendingConfirmationToken = null;
  pendingConfirmationExpiresAtMs = null;
  clearPendingConfirmationTimer();
}

async function cancelPendingConfirmation(reason: "timeout" | "cancelled_by_voice"): Promise<void> {
  try {
    await invoke("agent_cancel_pending_confirmation", {
      request: {
        reason,
      },
    });
  } catch (error) {
    console.warn("Failed to cancel pending confirmation", error);
  } finally {
    clearPendingConfirmationState();
    latestConfirmationLine =
      reason === "timeout" ? "Confirmation expired (timeout)." : "Pending confirmation cancelled.";
    renderLiveState();
  }
}

function schedulePendingConfirmationTimeout(expiresAtMs: number): void {
  clearPendingConfirmationTimer();
  const delayMs = Math.max(0, expiresAtMs - Date.now());
  pendingConfirmationTimer = window.setTimeout(() => {
    void cancelPendingConfirmation("timeout");
  }, delayMs);
}

function normalizeToken(value: string): string {
  return value
    .toLowerCase()
    .split("")
    .filter((char) => /[a-z0-9]/.test(char))
    .join("");
}

function normalizeWakewordText(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s]+/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function applyWakewordAliases(value: string): string {
  let normalized = normalizeWakewordText(value);
  const aliases = new Set<string>([
    ...BUILTIN_WAKEWORD_ALIASES.map((alias) => normalizeWakewordText(alias)),
    ...((settings?.workflow_agent?.wakeword_aliases ?? []).map((alias) => normalizeWakewordText(alias))),
  ]);
  aliases.forEach((alias) => {
    if (!alias) return;
    const escaped = alias.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    normalized = normalized.replace(new RegExp(`\\b${escaped}\\b`, "gu"), "trispr");
  });
  return normalized;
}

function isLikelyInputSource(source: string): boolean {
  const normalized = source.trim().toLowerCase();
  if (!normalized) return true;
  if (normalized.includes("system") || normalized.includes("output")) return false;
  return normalized.includes("mic")
    || normalized.includes("input")
    || normalized.includes("local")
    || normalized.includes("unknown");
}

function isTranscribeAssistantHoldActive(timestampMs: number, source: string): boolean {
  if (!isLikelyInputSource(source)) return false;
  return timestampMs <= transcribeAssistantHoldUntilMs;
}

function armTranscribeAssistantHold(reason: string): void {
  if (isAssistantModeEnabled()) return;
  transcribeAssistantHoldUntilMs = Date.now() + TRANSCRIBE_ASSISTANT_HOLD_MS;
  setGateReason("transcribe_followup_hold", reason);
}

function detectTranscribeDirective(commandText: string): boolean {
  const normalized = applyWakewordAliases(commandText);
  if (!normalized) return false;
  for (const prefix of TRANSCRIBE_DIRECTIVE_PREFIXES) {
    if (normalized === prefix || normalized.startsWith(`${prefix} `)) {
      return true;
    }
    const wakewordBound = `trispr ${prefix}`;
    if (normalized === wakewordBound || normalized.startsWith(`${wakewordBound} `)) {
      return true;
    }
  }
  return false;
}

function isAgentPttArmed(nowMs = Date.now()): boolean {
  return agentPttArmedUntilMs !== null && nowMs < agentPttArmedUntilMs;
}

function setAgentPttArmed(armed: boolean): void {
  if (!dom.workflowAgentPttArmBtn) return;
  dom.workflowAgentPttArmBtn.textContent = armed ? "PTT Armed (Agent)" : "PTT (Agent)";
  dom.workflowAgentPttArmBtn.classList.toggle("recording", armed);
}

function armAgentPtt(): void {
  agentPttArmedUntilMs = Date.now() + AGENT_PTT_ARM_WINDOW_MS;
  setAgentPttArmed(true);
  appendLog("Agent PTT armed for next utterance.");
}

function disarmAgentPtt(reason: string): void {
  if (!isAgentPttArmed()) return;
  agentPttArmedUntilMs = null;
  setAgentPttArmed(false);
  appendLog(`Agent PTT disarmed (${reason}).`);
}

function isDuplicateTranscript(spoken: string, nowMs: number): boolean {
  const key = applyWakewordAliases(spoken);
  if (!key) return true;
  if (
    key === lastHandledTranscriptKey
    && nowMs - lastHandledTranscriptAtMs < AGENT_TRANSCRIPT_DEDUPE_WINDOW_MS
  ) {
    return true;
  }
  lastHandledTranscriptKey = key;
  lastHandledTranscriptAtMs = nowMs;
  return false;
}

function containsAnyKeyword(text: string, keywords: string[]): boolean {
  const normalized = applyWakewordAliases(text);
  return keywords.some((keyword) => normalized.includes(keyword));
}

function setGateReason(reasonCode: string, detail: string): void {
  latestGateLine = `${reasonCode}: ${detail}`;
  renderLiveState();
}

function maybeShowGateToast(title: string, message: string): void {
  const now = Date.now();
  if (now - lastGateToastAtMs < AGENT_GATE_TOAST_COOLDOWN_MS) return;
  lastGateToastAtMs = now;
  showToast({
    type: "info",
    title,
    message,
    duration: 3200,
  });
}

function clearAgentStageResetTimer(): void {
  if (uiAgentStageResetTimer !== null) {
    window.clearTimeout(uiAgentStageResetTimer);
    uiAgentStageResetTimer = null;
  }
}

function renderAgentStatePipeline(): void {
  const chips = dom.workflowAgentStatePipeline?.querySelectorAll<HTMLElement>("[data-agent-stage]") ?? [];
  const activeIndex = AGENT_STAGE_ORDER.indexOf(uiAgentStage);
  chips.forEach((chip) => {
    const stage = chip.dataset.agentStage as AssistantStateChangedEvent["state"] | undefined;
    const chipIndex = stage ? AGENT_STAGE_ORDER.indexOf(stage) : -1;
    chip.classList.toggle("is-active", stage === uiAgentStage);
    chip.classList.toggle("is-done", chipIndex >= 0 && chipIndex < activeIndex);
  });
  if (dom.workflowAgentStateNote) {
    dom.workflowAgentStateNote.textContent = uiAgentStageNote;
  }
}

function setAgentPipelineStage(
  stage: AssistantStateChangedEvent["state"],
  note: string,
  autoResetMs?: number
): void {
  clearAgentStageResetTimer();
  uiAgentStage = stage;
  uiAgentStageNote = note;
  renderAgentStatePipeline();
  if ((autoResetMs ?? 0) > 0) {
    uiAgentStageResetTimer = window.setTimeout(() => {
      uiAgentStageResetTimer = null;
      uiAgentStage = isAssistantModeEnabled() ? "listening" : "idle";
      uiAgentStageNote = isAssistantModeEnabled()
        ? "Listening for wakeword commands."
        : "Idle: waiting for wakeword or PTT command.";
      renderAgentStatePipeline();
    }, autoResetMs);
  }
}

async function maybeSpeakAgentReply(text: string): Promise<void> {
  if (!text.trim()) return;
  if (!settings?.workflow_agent?.voice_feedback_enabled) {
    appendLog("Voice feedback disabled -> reply shown as text only.");
    return;
  }
  try {
    await invoke("speak_tts", {
      request: {
        provider: "",
        text,
        rate: null,
        volume: null,
        context: "agent_reply",
      },
    });
    armTranscribeAssistantHold("reply_spoken");
  } catch (error) {
    appendLog(`Voice feedback failed: ${String(error)}`);
  }
}

function appendLog(line: string): void {
  if (!dom.workflowAgentExecutionLog) return;
  const now = new Date().toLocaleTimeString();
  const next = `[${now}] ${line}`;
  const current = dom.workflowAgentExecutionLog.value.trim();
  dom.workflowAgentExecutionLog.value = current ? `${current}\n${next}` : next;
  dom.workflowAgentExecutionLog.scrollTop = dom.workflowAgentExecutionLog.scrollHeight;
}

function renderLiveState(): void {
  if (dom.workflowAgentLastHeard) dom.workflowAgentLastHeard.textContent = latestHeardLine;
  if (dom.workflowAgentLastGate) dom.workflowAgentLastGate.textContent = latestGateLine;
  if (dom.workflowAgentLastIntent) dom.workflowAgentLastIntent.textContent = latestIntentLine;
  if (dom.workflowAgentLastReply) dom.workflowAgentLastReply.textContent = latestReplyLine;
  if (dom.workflowAgentAwaitingConfirmation) {
    dom.workflowAgentAwaitingConfirmation.textContent = latestConfirmationLine;
  }
}

function isReviewConfirmed(): boolean {
  return Boolean(dom.workflowAgentReviewConfirm?.checked);
}

function setReviewConfirmed(value: boolean): void {
  if (dom.workflowAgentReviewConfirm) {
    dom.workflowAgentReviewConfirm.checked = value;
  }
}

function renderReviewGate(): void {
  const interactionEnabled = isWorkflowAgentEnabled();
  const hasPlan = Boolean(currentPlan);
  const publishLaneActive = (currentPlan?.execution_steps?.length ?? 0) > 0;
  const reviewConfirmed = isReviewConfirmed();
  if (dom.workflowAgentReviewSummary) {
    if (!hasPlan) {
      dom.workflowAgentReviewSummary.textContent =
        "Build a plan first, then review signals, assumptions, and action lanes.";
    } else if (publishLaneActive) {
      dom.workflowAgentReviewSummary.textContent =
        "Side-effect lane detected (publish). Review and confirm is required before execute.";
    } else {
      dom.workflowAgentReviewSummary.textContent =
        "Draft-only lane detected (no publish step). Confirm review to execute.";
    }
  }
  const canExecute = interactionEnabled && hasPlan && reviewConfirmed;
  dom.workflowAgentExecuteBtn?.toggleAttribute("disabled", !canExecute);
}

function shouldShowPlanLane(): boolean {
  return Boolean(
    currentPlan
      || (lastParse?.detected && lastParse.intent === "gdd_generate_publish")
      || pendingConfirmationToken
  );
}

function renderActionLane(): void {
  const showPlanLane = shouldShowPlanLane();
  dom.workflowAgentPlanLane?.toggleAttribute("hidden", !showPlanLane);
  if (!dom.workflowAgentActionHint) return;
  if (!lastParse) {
    dom.workflowAgentActionHint.textContent =
      "Non-side-effect intents reply immediately. GDD intents open a confirmable plan lane.";
    return;
  }
  if (lastParse.intent === "gdd_generate_publish") {
    dom.workflowAgentActionHint.textContent =
      "GDD intent detected. Select a session, build plan, then confirm execution.";
    return;
  }
  if (lastParse.intent === "unknown") {
    dom.workflowAgentActionHint.textContent =
      "Wakeword was recognized, but no side-effect intent was detected. Reply stays safe and read-only.";
    return;
  }
  dom.workflowAgentActionHint.textContent =
    "Intent is read-only (no side effects). Confirm lane is not required.";
}

function syncGlobalOnlineModeHeader(onlineEnabled: boolean): void {
  if (dom.globalOnlineOfflineBtn) {
    dom.globalOnlineOfflineBtn.classList.toggle("is-active", !onlineEnabled);
    dom.globalOnlineOfflineBtn.setAttribute("aria-pressed", onlineEnabled ? "false" : "true");
  }
  if (dom.globalOnlineEnabledBtn) {
    dom.globalOnlineEnabledBtn.classList.toggle("is-active", onlineEnabled);
    dom.globalOnlineEnabledBtn.setAttribute("aria-pressed", onlineEnabled ? "true" : "false");
  }
}

function renderConfiguration(): void {
  ensureWorkflowAgentDefaults();
  const cfg = settings?.workflow_agent;
  if (!cfg) return;

  if (dom.workflowAgentHandsFreeEnabled) {
    dom.workflowAgentHandsFreeEnabled.checked = Boolean(cfg.hands_free_enabled);
  }
  if (dom.workflowAgentWakewords) {
    dom.workflowAgentWakewords.value = (cfg.wakewords ?? []).join("\n");
  }
  if (dom.workflowAgentWakewordAliases) {
    dom.workflowAgentWakewordAliases.value = (cfg.wakeword_aliases ?? []).join("\n");
  }
  if (dom.workflowAgentConfirmTimeoutSec) {
    dom.workflowAgentConfirmTimeoutSec.value = String(cfg.confirm_timeout_sec ?? 45);
  }
  if (dom.workflowAgentSuggestionLevel) {
    dom.workflowAgentSuggestionLevel.value = suggestionLevelFromMaxCandidates(cfg.max_candidates ?? 3);
  }
  if (dom.workflowAgentReplyMode) {
    dom.workflowAgentReplyMode.value =
      cfg.reply_mode === "hybrid_local_llm" ? "hybrid_local_llm" : "rule_only";
  }
  if (dom.workflowAgentOnlineMode) {
    dom.workflowAgentOnlineMode.value = cfg.online_enabled ? "online_enabled" : "local_only";
  }
  syncGlobalOnlineModeHeader(Boolean(cfg.online_enabled));
  if (dom.workflowAgentVoiceFeedbackEnabled) {
    dom.workflowAgentVoiceFeedbackEnabled.checked = Boolean(cfg.voice_feedback_enabled);
  }
}

function renderStatus(): void {
  if (!dom.workflowAgentConsole) return;
  const enabled = isWorkflowAgentEnabled();
  dom.workflowAgentConsole.hidden = !enabled;

  const interactionEnabled = enabled;
  const actionControls: Array<HTMLElement | null> = [
    dom.workflowAgentCommandInput,
    dom.workflowAgentParseBtn,
    dom.workflowAgentRefreshCandidatesBtn,
    dom.workflowAgentPttArmBtn,
    dom.workflowAgentTargetLanguage,
    dom.workflowAgentBuildPlanBtn,
    dom.workflowAgentReviewConfirm,
  ];
  actionControls.forEach((control) => control?.toggleAttribute("disabled", !interactionEnabled));
  if (!interactionEnabled && isAgentPttArmed()) {
    disarmAgentPtt("workflow agent off");
  }

  const configControls: Array<HTMLElement | null> = [
    dom.workflowAgentHandsFreeEnabled,
    dom.workflowAgentWakewords,
    dom.workflowAgentWakewordAliases,
    dom.workflowAgentConfirmTimeoutSec,
    dom.workflowAgentSuggestionLevel,
    dom.workflowAgentReplyMode,
    dom.workflowAgentOnlineMode,
    dom.workflowAgentVoiceFeedbackEnabled,
  ];
  configControls.forEach((control) => control?.toggleAttribute("disabled", !enabled));

  if (!dom.workflowAgentStatus) return;
  if (!enabled) {
    dom.workflowAgentStatus.textContent =
      "Workflow Agent disabled. Enable module + setting to use assistant actions.";
    setAgentPipelineStage("idle", "Workflow Agent disabled.");
    renderReviewGate();
    renderActionLane();
    renderConfiguration();
    renderLiveState();
    renderAgentStatePipeline();
    return;
  }
  if (!isAssistantModeEnabled()) {
    const handsFree = settings?.workflow_agent?.hands_free_enabled ? "on" : "off";
    dom.workflowAgentStatus.textContent =
      `Transcribe mode active. Wakeword auto-route is enabled · 5s follow-up window after spoken replies · hands-free ${handsFree}. Assistant state events are limited in this mode.`;
    renderReviewGate();
    renderActionLane();
    renderConfiguration();
    renderLiveState();
    renderAgentStatePipeline();
    return;
  }

  const stateLabel = latestAssistantState?.state ?? "listening";
  const handsFree = settings?.workflow_agent?.hands_free_enabled ? "on" : "off";
  const base = `Assistant state: ${stateLabel.replace(/_/g, " ")} · hands-free ${handsFree}.`;
  const guidance = assistantReasonGuidance(latestAssistantState?.reason);
  if (!latestAssistantState?.capability?.degraded) {
    dom.workflowAgentStatus.textContent = guidance ? `${base} ${guidance}` : base;
    renderReviewGate();
    renderActionLane();
    renderConfiguration();
    renderLiveState();
    renderAgentStatePipeline();
    return;
  }
  const missing = latestAssistantState.capability.missing_capabilities ?? [];
  const softMissing = missing.filter((id) => id === "output_voice_tts" || id === "input_vision");
  dom.workflowAgentStatus.textContent = softMissing.length > 0
    ? `${base} Degraded capability: ${formatCapabilityList(softMissing)}.`
    : `${base} Degraded capability mode active (${formatCapabilityList(missing)}).`;
  renderReviewGate();
  renderActionLane();
  renderConfiguration();
  renderLiveState();
  renderAgentStatePipeline();
}

function renderCandidates(): void {
  if (!dom.workflowAgentCandidates) return;
  if (!lastCandidates.length) {
    dom.workflowAgentCandidates.innerHTML = `<div class="field-hint">No session candidates yet.</div>`;
    return;
  }
  dom.workflowAgentCandidates.innerHTML = lastCandidates
    .map((candidate) => {
      const selected = candidate.session_id === selectedSessionId;
      return `<button type="button" class="ghost-btn workflow-agent-candidate${
        selected ? " is-active" : ""
      }" data-session-id="${candidate.session_id}">
        <strong>${new Date(candidate.start_ms).toLocaleString()}</strong>
        <span>${candidate.entry_count} entries · score ${(candidate.score * 100).toFixed(0)}%</span>
        <span>${candidate.preview || "No preview"}</span>
      </button>`;
    })
    .join("");
}

function renderPlanPreview(): void {
  if (!dom.workflowAgentPlanPreview) return;
  if (!currentPlan) {
    dom.workflowAgentPlanPreview.value = "";
    renderReviewGate();
    return;
  }
  const steps = currentPlan.steps.map((step) => `- ${step.title}`).join("\n");
  const recognizedSignals = (currentPlan.recognized_signals ?? [])
    .map((item) => `- ${item}`)
    .join("\n");
  const assumptions = (currentPlan.assumptions ?? []).map((item) => `- ${item}`).join("\n");
  const proposedActions = (currentPlan.proposed_actions ?? [])
    .map((item) => `- ${item}`)
    .join("\n");
  const analysisSteps = (currentPlan.analysis_steps ?? []).map((step) => `- ${step.title}`).join("\n");
  const executionSteps = (currentPlan.execution_steps ?? [])
    .map((step) => `- ${step.title}`)
    .join("\n");
  dom.workflowAgentPlanPreview.value = [
    `Intent: ${currentPlan.intent}`,
    `Session: ${currentPlan.session_id}`,
    `Target language: ${currentPlan.target_language}`,
    `Publish: ${currentPlan.publish ? "yes" : "no"}`,
    "",
    "Recognized signals:",
    recognizedSignals || "- none",
    "",
    "Assumptions:",
    assumptions || "- none",
    "",
    "Proposed actions:",
    proposedActions || "- none",
    "",
    "Analysis lane (no side effects):",
    analysisSteps || "- none",
    "",
    "Execution lane (side effects):",
    executionSteps || "- none",
    "",
    "Combined steps:",
    steps || "- none",
  ].join("\n");
  renderReviewGate();
}

function matchesWakeword(text: string): boolean {
  const normalized = applyWakewordAliases(text);
  if (!normalized) return false;
  const wakewords = [
    ...(settings?.workflow_agent?.wakewords ?? []),
    ...((settings?.workflow_agent?.wakeword_aliases ?? [])),
    ...BUILTIN_WAKEWORD_ALIASES,
  ];
  return wakewords.some((wakeword) => {
    const needle = applyWakewordAliases(wakeword);
    return Boolean(needle) && normalized.includes(needle);
  });
}

function buildPlanStatusReply(): string {
  if (currentPlan) {
    return `Current plan ready: ${currentPlan.summary}`;
  }
  if (pendingConfirmationToken) {
    return `Awaiting confirmation token ${pendingConfirmationToken}.`;
  }
  if (lastParse?.detected) {
    return `Last intent ${lastParse.intent} parsed; no active execution plan yet.`;
  }
  return "No active plan. Start with a wakeword command to build one.";
}

function buildSessionRecapReply(): string {
  if (!lastCandidates.length) {
    return "No session candidates found yet. Say a command with a topic or time hint first.";
  }
  const top = lastCandidates[0];
  const startedAt = new Date(top.start_ms).toLocaleString();
  const preview = top.preview?.trim() || "No transcript preview available.";
  return `Top session ${startedAt} with ${top.entry_count} entries. Preview: ${preview}`;
}

function buildUnknownRuleReply(commandText: string): string {
  const normalized = applyWakewordAliases(commandText);
  const onlineEnabled = Boolean(settings?.workflow_agent?.online_enabled);
  const englishHint = normalized.includes("please")
    || normalized.includes("what")
    || normalized.includes("session")
    || normalized.includes("status")
    || normalized.includes("weather");
  const weatherLike = normalized.includes("wetter")
    || normalized.includes("weather")
    || normalized.includes("forecast")
    || normalized.includes("temperatur");
  if (weatherLike) {
    if (onlineEnabled) {
      if (englishHint) {
        return "Online weather lookup is currently unavailable. Please try again or include a city (for example: weather in Berlin tomorrow).";
      }
      return "Live-Wetterabfrage ist aktuell nicht verfügbar. Bitte erneut versuchen oder eine Stadt mit angeben (z. B. Wetter in Berlin morgen).";
    }
    if (englishHint) {
      return "I do not have live weather access in local mode. I can still help with plan status, session recaps, or GDD drafts from your transcripts.";
    }
    return "Ich habe lokal keinen Live-Wetterzugriff. Ich kann dir aber einen Plan, Recap oder GDD-Draft aus deinen Transkripten erstellen.";
  }
  if (englishHint) {
    return "I can currently handle GDD drafts, session recaps, and plan status from your local transcripts. Please rephrase your request within that scope.";
  }
  return "Ich kann aktuell GDD-Drafts, Session-Recaps und Plan-Status aus deinen lokalen Transkripten verarbeiten. Formuliere die Anfrage bitte in diesem Scope.";
}

async function composeUnknownReply(commandText: string): Promise<void> {
  try {
    const reply = await invoke<AgentReplyResult>("agent_compose_unknown_reply", {
      request: {
        command_text: commandText,
      },
    });
    latestReplyLine = reply.text?.trim() || buildUnknownRuleReply(commandText);
    setGateReason(reply.reason_code || "unknown_reply", `source=${reply.source || "rule"}`);
    appendLog(`Unknown intent reply -> ${reply.source}/${reply.reason_code}`);
  } catch (error) {
    latestReplyLine = buildUnknownRuleReply(commandText);
    setGateReason("unknown_reply_fallback", "local_llm_unavailable");
    appendLog(`Unknown intent fallback reply -> ${String(error)}`);
  }
  renderLiveState();
  await maybeSpeakAgentReply(latestReplyLine);
}

async function parseCommand(
  commandText: string,
  options?: {
    allowWakewordless?: boolean;
  }
): Promise<void> {
  if (!requireAssistantInteraction("running workflow-agent commands")) return;
  const allowWakewordless = Boolean(options?.allowWakewordless);
  setAgentPipelineStage("parsing", "Parsing command and intent…");
  if (!commandText.trim()) {
    showToast({
      type: "warning",
      title: "No command",
      message: "Enter or speak a command first.",
      duration: 3000,
    });
    return;
  }
  const parsed = await invoke<AgentCommandParseResult>("agent_parse_command", {
    request: {
      command_text: commandText,
      source: "ui_console",
    },
  });
  lastParse = parsed;
  languageExplicitlySet = false;
  latestHeardLine = parsed.command_text?.trim() || commandText.trim();
  latestIntentLine = parsed.detected
    ? `${parsed.intent} (${(parsed.confidence * 100).toFixed(0)}%)`
    : parsed.wakeword_matched
      ? "Wakeword matched, no mapped action intent."
      : "No actionable intent detected.";
  appendLog(
    `Parsed command -> intent=${parsed.intent}, confidence=${(parsed.confidence * 100).toFixed(0)}%, publish=${parsed.publish_requested}`
  );
  setGateReason(
    parsed.wakeword_matched
      ? "wakeword_matched"
      : allowWakewordless
        ? "assistant_handsfree_direct"
        : "wakeword_missing",
    parsed.reasoning || "no reasoning"
  );
  renderActionLane();
  renderLiveState();
  if (!parsed.detected && !parsed.wakeword_matched) {
    if (allowWakewordless) {
      setAgentPipelineStage("listening", "No mapped action intent. Replying in conversational mode.", 1400);
      await composeUnknownReply(parsed.command_text || commandText);
      return;
    }
    setAgentPipelineStage("listening", "No actionable intent detected.", 1200);
    showToast({
      type: "info",
      title: "No actionable intent",
      message: "Wakeword or intent keywords were missing.",
      duration: 3200,
    });
    return;
  }
  if (!parsed.detected && parsed.wakeword_matched) {
    setAgentPipelineStage("listening", "Wakeword recognized; replying in safe read-only mode.", 1400);
    await composeUnknownReply(parsed.command_text);
    return;
  }

  if (parsed.intent === "plan_status") {
    setAgentPipelineStage("listening", "Plan status generated.", 1400);
    const reply = buildPlanStatusReply();
    latestReplyLine = reply;
    appendLog(`Plan status reply -> ${reply}`);
    renderLiveState();
    await maybeSpeakAgentReply(reply);
    return;
  }

  if (parsed.intent === "session_recap") {
    setAgentPipelineStage("listening", "Session recap generated.", 1400);
    if (!lastCandidates.length) {
      await refreshCandidates();
    }
    const reply = buildSessionRecapReply();
    latestReplyLine = reply;
    appendLog(`Session recap reply -> ${reply}`);
    renderLiveState();
    await maybeSpeakAgentReply(reply);
    return;
  }

  if (parsed.intent === "confirm_or_cancel") {
    setAgentPipelineStage("listening", "Confirmation intent processed.", 1400);
    const normalized = parsed.command_text.toLowerCase();
    if (containsAnyKeyword(normalized, CANCEL_KEYWORDS) && pendingConfirmationToken) {
      await cancelPendingConfirmation("cancelled_by_voice");
      latestReplyLine = "Pending confirmation cancelled.";
    } else {
      latestReplyLine = pendingConfirmationToken
        ? `Confirmation pending. Speak wakeword + confirm + token ${pendingConfirmationToken}.`
        : "No pending confirmation to resolve.";
    }
    appendLog(`Confirm/cancel intent reply -> ${latestReplyLine}`);
    renderLiveState();
    await maybeSpeakAgentReply(latestReplyLine);
    return;
  }

  setAgentPipelineStage("planning", "Intent accepted. Looking up matching sessions…");
  await refreshCandidates();
}

async function refreshCandidates(): Promise<void> {
  if (!requireAssistantInteraction("searching transcript sessions")) return;
  setAgentPipelineStage("planning", "Searching transcript sessions…");
  if (!lastParse) {
    showToast({
      type: "info",
      title: "Parse first",
      message: "Parse a command before searching sessions.",
      duration: 3000,
    });
    return;
  }
  const candidates = await invoke<TranscriptSessionCandidate[]>("search_transcript_sessions", {
    request: {
      temporal_hint: lastParse.temporal_hint ?? null,
      topic_hint: lastParse.topic_hint ?? null,
      session_gap_minutes: settings?.workflow_agent?.session_gap_minutes ?? 20,
      max_candidates: settings?.workflow_agent?.max_candidates ?? 3,
    },
  });
  lastCandidates = candidates;
  selectedSessionId = "";
  currentPlan = null;
  setReviewConfirmed(false);
  renderCandidates();
  renderPlanPreview();
  renderActionLane();
  appendLog(`Session search -> ${candidates.length} candidate(s) found.`);
  setAgentPipelineStage("planning", candidates.length > 0
    ? "Session candidates ready. Select one and build plan."
    : "No matching sessions found.");
  if (candidates.length === 0) {
    appendLog("No matching sessions found. Try different keywords or check transcript history.");
    return;
  }
  if (lastParse?.topic_hint) {
    appendLog(`Detected topic: "${lastParse.topic_hint}"`);
  }
  if (lastParse?.temporal_hint) {
    appendLog(`Detected time hint: "${lastParse.temporal_hint}"`);
  }
  if (isAmbiguousSelection(candidates)) {
    appendLog(
      `⚠ Top sessions have similar scores (${(candidates[0].score * 100).toFixed(0)}% vs ${(candidates[1].score * 100).toFixed(0)}%). Please review and select manually.`
    );
  }
  appendLog("Select a session above before building the plan.");
}

async function buildPlan(): Promise<void> {
  if (!requireAssistantInteraction("building an execution plan")) return;
  setAgentPipelineStage("planning", "Building execution plan…");
  if (!lastParse?.detected) {
    showToast({
      type: "warning",
      title: "No command parsed",
      message: "Parse a command first.",
      duration: 3200,
    });
    return;
  }
  if (!selectedSessionId) {
    showToast({
      type: "warning",
      title: "No session selected",
      message: "Click a session candidate above to select it before building the plan.",
      duration: 3500,
    });
    return;
  }
  if (!languageExplicitlySet) {
    showToast({
      type: "warning",
      title: "Language required",
      message: "Please select the target language before building the plan.",
      duration: 3500,
    });
    return;
  }
  const targetLanguage = dom.workflowAgentTargetLanguage?.value ?? "";
  if (!isValidTargetLanguage(targetLanguage)) {
    showToast({
      type: "warning",
      title: "Invalid language",
      message: `Language "${targetLanguage}" is not supported. Please select a valid option.`,
      duration: 3500,
    });
    return;
  }
  const req: AgentBuildExecutionPlanRequest = {
    intent: lastParse.intent,
    session_id: selectedSessionId,
    target_language: targetLanguage,
    publish: Boolean(lastParse.publish_requested),
    command_text: lastParse.command_text,
    temporal_hint: lastParse.temporal_hint ?? null,
    topic_hint: lastParse.topic_hint ?? null,
    parse_confidence: lastParse.confidence,
  };
  currentPlan = await invoke<AgentExecutionPlan>("agent_build_execution_plan", { request: req });
  setReviewConfirmed(false);
  renderPlanPreview();
  renderActionLane();
  appendLog("Execution plan ready. Review the plan details and confirm review before execution.");
  setAgentPipelineStage("planning", "Plan ready. Review and confirm before execution.");
}

async function executePlan(confirmationToken?: string): Promise<void> {
  if (!requireAssistantInteraction("executing the plan")) return;
  setAgentPipelineStage("executing", "Executing plan…");
  if (!currentPlan) {
    showToast({
      type: "warning",
      title: "No plan",
      message: "Build a plan before execution.",
      duration: 3000,
    });
    return;
  }
  if (!isReviewConfirmed()) {
    showToast({
      type: "warning",
      title: "Review required",
      message: "Review the plan and toggle review confirmation before execution.",
      duration: 3500,
    });
    return;
  }
  appendLog("Executing plan...");
  const result = await invoke<AgentExecutionResult>("agent_execute_gdd_plan", {
    request: {
      plan: currentPlan,
      title: null,
      preset_id: null,
      max_chunk_chars: null,
      space_key: settings?.confluence_settings?.default_space_key || null,
      parent_page_id: settings?.confluence_settings?.default_parent_page_id || null,
      target_page_id: null,
      confirmation_token: confirmationToken ?? null,
    },
  });
  if (result.status !== "failed") {
    clearPendingConfirmationState();
  }
  latestReplyLine = result.message;
  latestConfirmationLine =
    result.status === "failed" ? latestConfirmationLine : "No pending confirmation.";
  renderActionLane();
  renderLiveState();
  appendLog(`Execution result -> ${result.status}: ${result.message}`);
  showToast({
    type: result.status === "failed" ? "error" : result.status === "queued" ? "warning" : "success",
    title: "Workflow Agent",
    message: result.message,
    duration: 4200,
  });
  setAgentPipelineStage(
    result.status === "failed" ? "recovering" : "listening",
    result.status === "failed" ? "Execution failed. Recovery path active." : "Execution finished.",
    result.status === "failed" ? 0 : 2000
  );
}

function bindUi(): void {
  dom.workflowAgentParseBtn?.addEventListener("click", () => {
    void parseCommand(dom.workflowAgentCommandInput?.value || "", { allowWakewordless: true });
  });
  dom.workflowAgentRefreshCandidatesBtn?.addEventListener("click", () => {
    void refreshCandidates();
  });
  dom.workflowAgentPttArmBtn?.addEventListener("click", () => {
    if (!isWorkflowAgentEnabled()) {
      showToast({
        type: "info",
        title: "Workflow Agent disabled",
        message: "Enable Workflow Agent before arming Agent PTT.",
        duration: 3200,
      });
      return;
    }
    armAgentPtt();
  });
  dom.workflowAgentBuildPlanBtn?.addEventListener("click", () => {
    void buildPlan();
  });
  dom.workflowAgentExecuteBtn?.addEventListener("click", () => {
    void executePlan();
  });
  dom.workflowAgentCandidates?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest<HTMLButtonElement>("[data-session-id]");
    if (!button) return;
    selectedSessionId = button.dataset.sessionId || "";
    currentPlan = null;
    setReviewConfirmed(false);
    renderCandidates();
    renderPlanPreview();
    appendLog(`Selected session ${selectedSessionId}`);
  });
  dom.workflowAgentTargetLanguage?.addEventListener("change", () => {
    languageExplicitlySet = true;
    if (!currentPlan) {
      return;
    }
    currentPlan = null;
    setReviewConfirmed(false);
    renderPlanPreview();
    appendLog("Target language changed. Rebuild the execution plan.");
  });
  dom.workflowAgentReviewConfirm?.addEventListener("change", () => {
    renderReviewGate();
  });

  dom.workflowAgentHandsFreeEnabled?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentHandsFreeEnabled) return;
    settings.workflow_agent.hands_free_enabled = dom.workflowAgentHandsFreeEnabled.checked;
    void persistWorkflowAgentSettings();
    renderStatus();
  });

  dom.workflowAgentWakewords?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentWakewords) return;
    const wakewords = parseWakewordsInput(dom.workflowAgentWakewords.value);
    if (wakewords.length > 0) {
      settings.workflow_agent.wakewords = wakewords;
    }
    renderConfiguration();
    void persistWorkflowAgentSettings();
  });

  dom.workflowAgentWakewordAliases?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentWakewordAliases) return;
    settings.workflow_agent.wakeword_aliases = parseWakewordAliasesInput(
      dom.workflowAgentWakewordAliases.value
    );
    renderConfiguration();
    void persistWorkflowAgentSettings();
  });

  dom.workflowAgentConfirmTimeoutSec?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentConfirmTimeoutSec) return;
    const parsed = Number.parseInt(dom.workflowAgentConfirmTimeoutSec.value, 10);
    const clamped = Number.isFinite(parsed) ? Math.min(300, Math.max(10, parsed)) : 45;
    settings.workflow_agent.confirm_timeout_sec = clamped;
    dom.workflowAgentConfirmTimeoutSec.value = String(clamped);
    void persistWorkflowAgentSettings();
  });

  dom.workflowAgentSuggestionLevel?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentSuggestionLevel) return;
    settings.workflow_agent.max_candidates = maxCandidatesFromSuggestionLevel(
      dom.workflowAgentSuggestionLevel.value
    );
    void persistWorkflowAgentSettings();
  });

  dom.workflowAgentReplyMode?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentReplyMode) return;
    settings.workflow_agent.reply_mode =
      dom.workflowAgentReplyMode.value === "hybrid_local_llm" ? "hybrid_local_llm" : "rule_only";
    void persistWorkflowAgentSettings();
  });

  dom.workflowAgentOnlineMode?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentOnlineMode) return;
    settings.workflow_agent.online_enabled = dom.workflowAgentOnlineMode.value === "online_enabled";
    syncGlobalOnlineModeHeader(Boolean(settings.workflow_agent.online_enabled));
    void persistWorkflowAgentSettings();
  });

  dom.workflowAgentVoiceFeedbackEnabled?.addEventListener("change", () => {
    if (!settings?.workflow_agent || !dom.workflowAgentVoiceFeedbackEnabled) return;
    settings.workflow_agent.voice_feedback_enabled = dom.workflowAgentVoiceFeedbackEnabled.checked;
    void persistWorkflowAgentSettings();
  });
}

export function focusWorkflowAgentConsole(): void {
  renderStatus();
  dom.workflowAgentConsole?.scrollIntoView({ behavior: "smooth", block: "start" });
  dom.workflowAgentCommandInput?.focus();
}

export function initWorkflowAgentConsole(): void {
  if (initialized) return;
  initialized = true;
  bindUi();
  renderStatus();
  renderCandidates();
  renderPlanPreview();
  renderReviewGate();
  renderActionLane();
  renderConfiguration();
  renderLiveState();
  setAgentPipelineStage("idle", "Idle: waiting for wakeword or PTT command.");
  setAgentPttArmed(false);
}

export function syncWorkflowAgentConsoleState(): void {
  renderStatus();
  renderReviewGate();
  renderActionLane();
  renderConfiguration();
  renderLiveState();
  renderAgentStatePipeline();
  setAgentPttArmed(isAgentPttArmed());
}

async function handleWorkflowAgentTranscriptInput(
  spokenRaw: string,
  source: string,
  timestampMs: number,
  streamKind: "raw" | "final"
): Promise<void> {
  const spoken = spokenRaw.trim();
  if (!spoken) return;
  if (detectTranscribeDirective(spoken)) {
    setGateReason("transcribe_directive_passthrough", `${source}/${streamKind}`);
    appendLog(`Transcribe directive passthrough from ${source} (${streamKind}).`);
    setAgentPipelineStage("idle", "Transcribe directive recognized. Agent routing skipped.", 1200);
    return;
  }
  const normalizedSpoken = normalizeWakewordText(spoken);
  if (ttsSpeaking && isLikelyInputSource(source) && containsAnyKeyword(normalizedSpoken, CANCEL_KEYWORDS)) {
    appendLog(`Emergency TTS stop detected from ${source} (${streamKind}).`);
    setGateReason("tts_stop_requested", `${source}/${streamKind}`);
    await invoke("stop_tts");
    handleTtsSpeechFinished();
    setAgentPipelineStage("idle", "TTS stop requested.", 1000);
    return;
  }
  const wakewordPreview = matchesWakeword(spoken);

  if (!isWorkflowAgentEnabled()) {
    setGateReason("module_disabled", "workflow_agent not enabled");
    setAgentPipelineStage("idle", "Workflow Agent disabled.");
    if (wakewordPreview) {
      maybeShowGateToast("Workflow Agent disabled", "Enable the Workflow Agent module to receive replies.");
    }
    return;
  }
  const assistantMode = isAssistantModeEnabled();
  const handsFree = Boolean(settings?.workflow_agent?.hands_free_enabled);
  const pttArmed = isAgentPttArmed(timestampMs);
  const assistantHandsFreeDirectRoute = assistantMode && handsFree && !pttArmed;
  const transcribeHoldActive = !assistantMode
    && isTranscribeAssistantHoldActive(timestampMs, source);
  if (assistantMode) {
    if (!handsFree && !pttArmed) {
      setGateReason("hands_free_off", "awaiting ptt arm");
      if (wakewordPreview) {
        maybeShowGateToast("Hands-free is off", "Enable Hands-free mode or use PTT (Agent) for the next utterance.");
      }
      return;
    }
  } else if (!pttArmed && !wakewordPreview && !transcribeHoldActive) {
    setGateReason("transcribe_passthrough", "no wakeword detected");
    setAgentPipelineStage("idle", "No wakeword detected in transcribe mode.");
    return;
  } else if (wakewordPreview) {
    setGateReason("auto_route_transcribe", `${source}/${streamKind}`);
    if (!handsFree) {
      maybeShowGateToast("Wakeword routed", "Wakeword command is routed in Transcribe mode (auto-route active).");
    }
  } else if (transcribeHoldActive) {
    setGateReason("transcribe_followup", `${source}/${streamKind}`);
  }

  latestHeardLine = `${source}/${streamKind}: ${spoken}`;
  if (isDuplicateTranscript(spoken, timestampMs)) {
    setGateReason("duplicate_suppressed", `${source}/${streamKind}`);
    return;
  }

  const bypassWakeword = pttArmed;
  const wakewordMatched = matchesWakeword(spoken);
  if (!bypassWakeword && !wakewordMatched && !transcribeHoldActive && !assistantHandsFreeDirectRoute) {
    setGateReason("no_wakeword", `${source}/${streamKind}`);
    setAgentPipelineStage("idle", "Wakeword required for agent routing.");
    return;
  }

  if (pendingConfirmationToken) {
    if (containsAnyKeyword(normalizedSpoken, CANCEL_KEYWORDS)) {
      appendLog(`Voice cancel detected from ${source} (${streamKind}).`);
      await cancelPendingConfirmation("cancelled_by_voice");
      if (pttArmed) disarmAgentPtt("voice cancel");
      return;
    }

    const pendingTokenNormalized = normalizeToken(pendingConfirmationToken);
    const spokenTokenNormalized = normalizeToken(spoken);
    const confirms = containsAnyKeyword(normalizedSpoken, CONFIRM_KEYWORDS);
    const tokenMatched = pendingTokenNormalized.length > 0
      && spokenTokenNormalized.includes(pendingTokenNormalized);
    if (confirms && tokenMatched) {
      appendLog(`Voice confirmation token accepted from ${source} (${streamKind}).`);
      await executePlan(pendingConfirmationToken);
      if (pttArmed) disarmAgentPtt("confirmation accepted");
      return;
    }
  }

  appendLog(
    `${
      bypassWakeword
        ? "PTT command"
        : wakewordMatched
          ? "Wakeword command"
          : assistantHandsFreeDirectRoute
            ? "Assistant hands-free command"
            : "Follow-up command"
    } detected from ${source} (${streamKind}).`
  );
  setGateReason(
    bypassWakeword
      ? "ptt_bypass"
      : wakewordMatched
        ? "wakeword_command"
        : assistantHandsFreeDirectRoute
          ? "assistant_handsfree_direct"
          : "followup_command",
    `${source}/${streamKind}`
  );
  setAgentPipelineStage("parsing", "Command routed to parser.");
  if (dom.workflowAgentCommandInput) {
    dom.workflowAgentCommandInput.value = spoken;
  }
  await parseCommand(spoken, { allowWakewordless: assistantHandsFreeDirectRoute });
  if (pttArmed) {
    disarmAgentPtt("command consumed");
  }
}

export async function handleWorkflowAgentRawResult(
  payload: TranscriptionRawResultEvent
): Promise<void> {
  if (!payload?.text) return;
  await handleWorkflowAgentTranscriptInput(
    payload.text,
    payload.source || "unknown",
    Number(payload.timestamp_ms || Date.now()),
    "raw"
  );
}

export async function handleWorkflowAgentFinalResult(
  payload: TranscriptionResultEvent
): Promise<void> {
  if (!payload?.text) return;
  await handleWorkflowAgentTranscriptInput(
    payload.text,
    payload.source || "unknown",
    Date.now(),
    "final"
  );
}

export function appendWorkflowAgentLog(line: string): void {
  appendLog(line);
}

export function handleAssistantStateChanged(payload: AssistantStateChangedEvent): void {
  latestAssistantState = payload;
  if (payload.state !== "awaiting_confirm" && pendingConfirmationToken) {
    clearPendingConfirmationState();
    latestConfirmationLine = "No pending confirmation.";
    renderLiveState();
  }
  setGateReason("assistant_state", `${payload.state}:${payload.reason}`);
  setAgentPipelineStage(payload.state, `Assistant state: ${payload.state.replace(/_/g, " ")}.`);
  renderStatus();
}

export function handleAssistantIntentDetected(payload: AssistantIntentDetectedEvent): void {
  lastParse = payload.parse;
  setAgentPipelineStage("parsing", `Intent detected: ${payload.parse.intent}.`);
  latestIntentLine = payload.parse.detected
    ? `${payload.parse.intent} (${(payload.parse.confidence * 100).toFixed(0)}%)`
    : payload.parse.wakeword_matched
      ? "Wakeword matched, no mapped action intent."
      : "No actionable intent detected.";
  setGateReason(
    payload.parse.wakeword_matched ? "assistant_parse_wakeword" : "assistant_parse_ignored",
    payload.parse.reasoning || payload.reason
  );
  renderActionLane();
  renderLiveState();
}

export function handleAssistantPlanReady(payload: AssistantPlanReadyEvent): void {
  currentPlan = payload.plan;
  setReviewConfirmed(false);
  setAgentPipelineStage("planning", "Plan ready for review.");
  latestReplyLine = `Plan ready: ${payload.plan.summary}`;
  renderPlanPreview();
  renderActionLane();
  renderLiveState();
}

export function handleAssistantAwaitingConfirmation(payload: AssistantAwaitingConfirmationEvent): void {
  setAgentPipelineStage("awaiting_confirm", "Awaiting voice or manual confirmation.");
  pendingConfirmationToken = payload.confirm_token?.trim() || null;
  pendingConfirmationExpiresAtMs = payload.expires_at_ms;
  if (pendingConfirmationExpiresAtMs) {
    schedulePendingConfirmationTimeout(pendingConfirmationExpiresAtMs);
  }
  latestConfirmationLine = pendingConfirmationToken
    ? `Awaiting confirm (${payload.confirm_timeout_sec}s), token ${pendingConfirmationToken}: ${payload.plan.summary}`
    : `Awaiting confirm (${payload.confirm_timeout_sec}s): ${payload.plan.summary}`;
  renderActionLane();
  renderLiveState();
}

export function handleAssistantConfirmationExpired(payload: AssistantConfirmationExpiredEvent): void {
  clearPendingConfirmationState();
  setAgentPipelineStage("listening", "Confirmation expired. Waiting for next command.", 2000);
  latestConfirmationLine = `Confirmation expired (${new Date(payload.expired_at_ms).toLocaleTimeString()}).`;
  renderActionLane();
  renderLiveState();
}

export function handleAssistantActionResult(
  payload: { result: AgentExecutionResult }
): void {
  setAgentPipelineStage(
    payload.result.status === "failed" ? "recovering" : "listening",
    payload.result.status === "failed" ? "Action failed. Recovery required." : "Action completed.",
    payload.result.status === "failed" ? 0 : 2000
  );
  if (payload.result.status === "completed" || payload.result.status === "queued" || payload.result.status === "cancelled") {
    clearPendingConfirmationState();
    latestConfirmationLine = "No pending confirmation.";
  }
  renderActionLane();
  renderLiveState();
}
