import type {
  TranscriptionRefinedEvent,
  TranscriptionRefinementFailedEvent,
  TranscriptionRefinementStartedEvent,
  TranscriptionResultEvent,
} from "./types";
import { settings } from "./state";
import { getOllamaRuntimeCardState } from "./ollama-models";
import * as dom from "./dom-refs";

type NodeState = "idle" | "active" | "success" | "bypassed" | "blocked" | "error" | "timeout";
type EdgeState = "idle" | "active" | "muted";
type PipelinePhase = "idle" | "raw_emitted" | "refining" | "refined" | "failed" | "timed_out";

type PipelineJobState = {
  jobId: string;
  source: string;
  phase: PipelinePhase;
  deferred: boolean;
  model: string;
  error: string;
};

const pipelineJobState: PipelineJobState = {
  jobId: "",
  source: "",
  phase: "idle",
  deferred: false,
  model: "",
  error: "",
};

function setNodeState(nodeId: string, state: NodeState): void {
  const element = document.getElementById(nodeId);
  if (!element) return;
  element.dataset.state = state;
}

function setEdgeState(edgeId: string, state: EdgeState): void {
  const element = document.getElementById(edgeId);
  if (!element) return;
  element.dataset.state = state;
}

function isLocalAiPathEnabled(): boolean {
  if (!settings?.ai_fallback?.enabled) return false;
  return (
    settings.ai_fallback.provider === "ollama"
    && settings.ai_fallback.execution_mode === "local_primary"
  );
}

function describeIdleState(aiEnabled: boolean, rulesEnabled: boolean): string {
  if (aiEnabled && rulesEnabled) {
    return "Idle: AI refinement is primary, rule-based refinement remains available.";
  }
  if (aiEnabled) {
    return "Idle: AI-only path is active (rule-based fallback disabled).";
  }
  if (rulesEnabled) {
    return "Idle: rule-based path is active (AI disabled).";
  }
  return "Idle: raw transcription output (no refinement active).";
}

function formatJobToken(jobId: string): string {
  if (!jobId) return "—";
  if (jobId.length <= 28) return jobId;
  return `${jobId.slice(0, 16)}…${jobId.slice(-8)}`;
}

function updateLiveSummary(
  hasJob: boolean,
  aiEnabled: boolean,
  rulesEnabled: boolean,
  localAiPath: boolean,
): void {
  if (!dom.refinementPipelineLive) return;

  if (!hasJob) {
    dom.refinementPipelineLive.textContent = describeIdleState(aiEnabled, rulesEnabled);
    return;
  }

  const jobToken = formatJobToken(pipelineJobState.jobId);
  switch (pipelineJobState.phase) {
    case "raw_emitted":
      dom.refinementPipelineLive.textContent = localAiPath && pipelineJobState.deferred
        ? `Job ${jobToken}: raw transcript ready, waiting for AI refinement.`
        : `Job ${jobToken}: raw/rule output path selected.`;
      break;
    case "refining":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: AI refinement running in background.`;
      break;
    case "refined":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: refined output ready${pipelineJobState.model ? ` (${pipelineJobState.model})` : ""}.`;
      break;
    case "failed":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: AI failed, raw fallback used${pipelineJobState.error ? ` (${pipelineJobState.error})` : ""}.`;
      break;
    case "timed_out":
      dom.refinementPipelineLive.textContent = `Job ${jobToken}: AI timeout, raw fallback pasted.`;
      break;
    default:
      dom.refinementPipelineLive.textContent = describeIdleState(aiEnabled, rulesEnabled);
      break;
  }
}

export function renderRefinementPipelineGraph(): void {
  if (!dom.refinementPipelineGraph) return;

  const aiEnabled = Boolean(settings?.ai_fallback?.enabled);
  const rulesEnabled = Boolean(settings?.postproc_enabled);
  const localAiPath = isLocalAiPathEnabled();
  const runtime = getOllamaRuntimeCardState();
  const aiBlocked = localAiPath
    && !runtime.healthy
    && !runtime.busy
    && !runtime.backgroundStarting;
  const hasJob = pipelineJobState.phase !== "idle" && pipelineJobState.jobId.trim().length > 0;

  const transcribeState: NodeState = !hasJob
    ? "idle"
    : pipelineJobState.phase === "raw_emitted" || pipelineJobState.phase === "refining"
      ? "active"
      : "success";
  const rulesState: NodeState = !rulesEnabled
    ? "bypassed"
    : !hasJob
      ? "idle"
      : pipelineJobState.phase === "raw_emitted" || pipelineJobState.phase === "refining"
        ? "active"
        : "success";

  let aiState: NodeState = "idle";
  if (!localAiPath) {
    aiState = aiEnabled ? "bypassed" : "bypassed";
  } else if (aiBlocked) {
    aiState = "blocked";
  } else if (!hasJob) {
    aiState = "idle";
  } else if (pipelineJobState.phase === "refining") {
    aiState = "active";
  } else if (pipelineJobState.phase === "refined") {
    aiState = "success";
  } else if (pipelineJobState.phase === "failed") {
    aiState = "error";
  } else if (pipelineJobState.phase === "timed_out") {
    aiState = "timeout";
  } else if (localAiPath && pipelineJobState.deferred) {
    aiState = "active";
  }

  const gateEnabled = localAiPath;
  const gateState: NodeState = !gateEnabled
    ? "bypassed"
    : !hasJob
      ? "idle"
      : pipelineJobState.phase === "raw_emitted" || pipelineJobState.phase === "refining"
        ? "active"
        : pipelineJobState.phase === "refined"
          ? "success"
          : pipelineJobState.phase === "failed"
            ? "error"
            : pipelineJobState.phase === "timed_out"
              ? "timeout"
              : "idle";

  const refinedOutputState: NodeState = !hasJob
    ? "idle"
    : pipelineJobState.phase === "refined"
      ? "active"
      : "idle";

  const rawOutputState: NodeState = !hasJob
    ? "idle"
    : pipelineJobState.phase === "failed"
      || pipelineJobState.phase === "timed_out"
      || (!gateEnabled && pipelineJobState.phase !== "refined")
      ? "active"
      : "idle";

  setNodeState("pipeline-node-transcribe", transcribeState);
  setNodeState("pipeline-node-rules", rulesState);
  setNodeState("pipeline-node-ai", aiState);
  setNodeState("pipeline-node-gate", gateState);
  setNodeState("pipeline-node-output-refined", refinedOutputState);
  setNodeState("pipeline-node-output-raw", rawOutputState);

  const useRulesBypassToAi = hasJob && localAiPath && !rulesEnabled;
  const useDirectRawBypass = hasJob && !localAiPath && !rulesEnabled;
  const useRulesOnlyRawPath =
    hasJob && !localAiPath && rulesEnabled && pipelineJobState.phase !== "refined";
  const useGateRawFallback =
    hasJob
    && gateEnabled
    && (pipelineJobState.phase === "failed" || pipelineJobState.phase === "timed_out");

  setEdgeState("pipeline-edge-transcribe-rules", hasJob && rulesEnabled ? "active" : "muted");
  setEdgeState("pipeline-edge-transcribe-ai", useRulesBypassToAi ? "active" : "muted");
  setEdgeState("pipeline-edge-transcribe-raw", useDirectRawBypass ? "active" : "muted");
  setEdgeState(
    "pipeline-edge-rules-ai",
    hasJob && localAiPath && rulesEnabled ? "active" : "muted",
  );
  setEdgeState("pipeline-edge-ai-gate", hasJob && gateEnabled ? "active" : "muted");
  setEdgeState("pipeline-edge-gate-refined", pipelineJobState.phase === "refined" ? "active" : "muted");
  setEdgeState("pipeline-edge-gate-raw", useGateRawFallback ? "active" : "muted");
  setEdgeState("pipeline-edge-rules-raw", useRulesOnlyRawPath ? "active" : "muted");

  updateLiveSummary(hasJob, aiEnabled, rulesEnabled, localAiPath);
}

export function syncRefinementPipelineGraphFromSettings(): void {
  renderRefinementPipelineGraph();
}

export function handlePipelineTranscriptionResult(payload: TranscriptionResultEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) {
    return;
  }
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || "";
  pipelineJobState.phase = "raw_emitted";
  pipelineJobState.deferred = Boolean(payload.paste_deferred);
  pipelineJobState.model = "";
  pipelineJobState.error = "";
  renderRefinementPipelineGraph();
}

export function handlePipelineRefinementStarted(payload: TranscriptionRefinementStartedEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) return;
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || pipelineJobState.source;
  pipelineJobState.phase = "refining";
  renderRefinementPipelineGraph();
}

export function handlePipelineRefined(payload: TranscriptionRefinedEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) return;
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || pipelineJobState.source;
  pipelineJobState.phase = "refined";
  pipelineJobState.model = payload.model || "";
  pipelineJobState.error = "";
  renderRefinementPipelineGraph();
}

export function handlePipelineRefinementFailed(payload: TranscriptionRefinementFailedEvent): void {
  const jobId = (payload?.job_id || "").trim();
  if (!jobId) return;
  pipelineJobState.jobId = jobId;
  pipelineJobState.source = payload.source || pipelineJobState.source;
  pipelineJobState.phase = "failed";
  pipelineJobState.error = payload.error || "";
  renderRefinementPipelineGraph();
}

export function handlePipelineRefinementTimeout(jobId: string): void {
  const normalized = (jobId || "").trim();
  if (!normalized) return;
  pipelineJobState.jobId = normalized;
  pipelineJobState.phase = "timed_out";
  renderRefinementPipelineGraph();
}

export function handlePipelineRefinementReset(reason: string): void {
  if (!pipelineJobState.jobId) return;
  pipelineJobState.phase = "failed";
  pipelineJobState.error = reason || "refinement reset";
  renderRefinementPipelineGraph();
}
