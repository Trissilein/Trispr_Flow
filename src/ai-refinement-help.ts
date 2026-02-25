type HelpText = {
  title: string;
  description: string;
  consequence: string;
};

export type HelpKey =
  | "ai_refinement_provider_model_section"
  | "ai_refinement_topic_section"
  | "ai_refinement_local_primary"
  | "ai_refinement_online_fallback"
  | "ai_refinement_execution_mode"
  | "ai_refinement_enable"
  | "ai_refinement_provider"
  | "ai_refinement_model"
  | "ai_refinement_auth_method"
  | "ai_refinement_api_key"
  | "ai_refinement_temperature"
  | "ai_refinement_max_tokens"
  | "ai_refinement_custom_prompt_toggle"
  | "ai_refinement_custom_prompt"
  | "ai_refinement_topic_keywords"
  | "ai_refinement_topic_reset"
  | "ollama_runtime_section"
  | "ollama_runtime_stage"
  | "ollama_runtime_endpoint"
  | "ollama_runtime_strict_local"
  | "ollama_runtime_source"
  | "ollama_runtime_version"
  | "ollama_runtime_health"
  | "ollama_service_section"
  | "ollama_service_endpoint"
  | "ollama_service_strict_local"
  | "ollama_service_remote_expert"
  | "ollama_service_runtime"
  | "ollama_service_tools"
  | "ollama_models_section"
  | "ollama_action_install"
  | "ollama_action_start"
  | "ollama_action_verify"
  | "ollama_action_detect"
  | "ollama_action_use_system"
  | "ollama_action_import"
  | "ollama_action_refresh"
  | "ollama_action_download"
  | "ollama_action_set_active"
  | "ollama_action_delete";

export const HELP_TEXTS: Record<HelpKey, HelpText> = {
  ai_refinement_provider_model_section: {
    title: "Runtime & Provider",
    description: "Choose where refinement runs: local Ollama by default, or optional verified cloud fallback.",
    consequence: "Local model cards are managed below; cloud model selection appears only in online mode.",
  },
  ai_refinement_topic_section: {
    title: "Topic Detection",
    description: "Configure topic keyword mapping used to annotate conversation entries.",
    consequence: "Keyword changes affect automatic topic badges in transcript history.",
  },
  ai_refinement_local_primary: {
    title: "Local Primary Runtime",
    description: "Ollama runs refinement locally so transcripts stay on this device by default.",
    consequence: "Choose this for privacy-first and offline-capable workflows.",
  },
  ai_refinement_online_fallback: {
    title: "Online Fallback",
    description: "Optional cloud provider path when you explicitly switch execution mode.",
    consequence: "Cloud mode can send transcript text to the selected provider.",
  },
  ai_refinement_execution_mode: {
    title: "Execution Mode",
    description: "Controls whether refinement uses local Ollama or a verified online fallback provider.",
    consequence: "Online mode is blocked until credentials are saved and verified.",
  },
  ai_refinement_enable: {
    title: "Enable AI Refinement",
    description: "Runs an LLM pass after transcription to fix wording, punctuation, and clarity.",
    consequence: "Turning this off uses only base transcription and local post-processing rules.",
  },
  ai_refinement_provider: {
    title: "Refinement Provider",
    description: "Select the service that executes the LLM refinement step.",
    consequence: "Switching provider can change latency, quality, and privacy guarantees.",
  },
  ai_refinement_model: {
    title: "Refinement Model",
    description: "Model used by the selected provider for transcript refinement.",
    consequence: "Different models trade speed, memory usage, and output quality.",
  },
  ai_refinement_auth_method: {
    title: "Authentication Method",
    description: "Choose how cloud credentials are verified for the selected provider.",
    consequence: "API key verification is available now. OAuth is shown for visibility and will be enabled later.",
  },
  ai_refinement_api_key: {
    title: "Cloud API Key",
    description: "Credential for cloud providers to authenticate refinement requests.",
    consequence: "Without a valid key, cloud refinement requests will fail.",
  },
  ai_refinement_temperature: {
    title: "Temperature",
    description: "Controls output randomness; lower values keep corrections conservative.",
    consequence: "Higher values can sound more natural but may alter phrasing more aggressively.",
  },
  ai_refinement_max_tokens: {
    title: "Max Tokens",
    description: "Upper bound for refinement output length.",
    consequence: "Too low may truncate long responses; too high can increase cost and latency.",
  },
  ai_refinement_custom_prompt_toggle: {
    title: "Custom Prompt",
    description: "Override built-in refinement instructions with your own prompt.",
    consequence: "You become responsible for output style and guardrails.",
  },
  ai_refinement_custom_prompt: {
    title: "Prompt Text",
    description: "Instruction sent with each refinement request.",
    consequence: "Prompt quality directly influences correction behavior.",
  },
  ai_refinement_topic_keywords: {
    title: "Topic Keywords",
    description: "Comma-separated terms that map transcript lines to a topic.",
    consequence: "Missing or overly broad keywords reduce topic classification quality.",
  },
  ai_refinement_topic_reset: {
    title: "Reset Topic Keywords",
    description: "Restore built-in topic keyword defaults.",
    consequence: "All custom keyword edits are replaced by defaults.",
  },
  ollama_runtime_section: {
    title: "Runtime Environment",
    description: "Status of the local Ollama runtime used by offline AI refinement.",
    consequence: "If runtime is unavailable, local model refinement cannot execute.",
  },
  ollama_runtime_stage: {
    title: "Runtime Stage",
    description: "Current setup phase: detection, installation, startup, or ready state.",
    consequence: "Use stage info to pick the next required setup action.",
  },
  ollama_runtime_endpoint: {
    title: "Endpoint",
    description: "HTTP address where the local Ollama API is reachable.",
    consequence: "Wrong endpoint prevents model list, pulls, and refinement requests.",
  },
  ollama_runtime_strict_local: {
    title: "Strict Local Mode",
    description: "Allows only localhost and 127.0.0.1 Ollama endpoints.",
    consequence: "Prevents accidental remote routing of transcript data.",
  },
  ollama_runtime_source: {
    title: "Runtime Source",
    description: "Indicates whether runtime comes from per-user install, system PATH, or manual setup.",
    consequence: "Source affects update flow and troubleshooting path.",
  },
  ollama_runtime_version: {
    title: "Runtime Version",
    description: "Detected Ollama runtime build currently used by the app.",
    consequence: "Version drift can change model compatibility and behavior.",
  },
  ollama_runtime_health: {
    title: "Runtime Health",
    description: "Readiness check based on local API reachability and response.",
    consequence: "Unhealthy runtime blocks pull/import and refinement actions.",
  },
  ollama_service_section: {
    title: "Ollama Service",
    description: "Operational service controls for endpoint policy, runtime info, and maintenance actions.",
    consequence: "Misconfiguration here can break local model operations.",
  },
  ollama_service_endpoint: {
    title: "Service Endpoint",
    description: "Current address used for Ollama API requests.",
    consequence: "If the service is not listening on this address, requests fail.",
  },
  ollama_service_strict_local: {
    title: "Service Privacy Guard",
    description: "Enforcement state for localhost-only endpoint policy.",
    consequence: "When enabled, remote endpoints are blocked server-side.",
  },
  ollama_service_remote_expert: {
    title: "Remote Expert Mode",
    description: "Expert-only switch for showing remote endpoint controls.",
    consequence: "Use only if you intentionally run Ollama outside localhost.",
  },
  ollama_service_runtime: {
    title: "Runtime Details",
    description: "Detected source, version, and health for the active runtime.",
    consequence: "Helps diagnose path conflicts and startup issues.",
  },
  ollama_service_tools: {
    title: "Service Tools",
    description: "Advanced runtime and model maintenance actions.",
    consequence: "Some actions can switch runtime source or modify local model state.",
  },
  ollama_models_section: {
    title: "Models",
    description: "Recommended local models for refinement with status and actions.",
    consequence: "Only the selected active model is used for refinement calls.",
  },
  ollama_action_install: {
    title: "Install Runtime",
    description: "Downloads and installs a local per-user Ollama runtime package.",
    consequence: "Requires network access once; afterwards runtime can run locally.",
  },
  ollama_action_start: {
    title: "Start Runtime",
    description: "Starts the Ollama runtime process on the local machine.",
    consequence: "Runtime must be running before verify, pull, or refinement works.",
  },
  ollama_action_verify: {
    title: "Verify Runtime",
    description: "Checks runtime reachability and current model availability.",
    consequence: "Use this to confirm the local stack is healthy.",
  },
  ollama_action_detect: {
    title: "Detect Runtime",
    description: "Scans for local runtime binaries and reports source/version.",
    consequence: "Detection decides whether install or start is the next step.",
  },
  ollama_action_use_system: {
    title: "Use System Runtime",
    description: "Switches to an Ollama executable already available in PATH.",
    consequence: "System runtime configuration may differ from per-user runtime.",
  },
  ollama_action_import: {
    title: "Import Model",
    description: "Imports a local GGUF or Modelfile into Ollama.",
    consequence: "Imported models become selectable without online pull.",
  },
  ollama_action_refresh: {
    title: "Refresh Runtime + Models",
    description: "Reloads runtime health and installed model inventory.",
    consequence: "Use after external changes to synchronize UI state.",
  },
  ollama_action_download: {
    title: "Download Model",
    description: "Pulls the selected model tag from the Ollama registry.",
    consequence: "Model is stored locally and can be activated after download.",
  },
  ollama_action_set_active: {
    title: "Set Active Model",
    description: "Marks this model as the single refinement model.",
    consequence: "New refinements use this tag until changed.",
  },
  ollama_action_delete: {
    title: "Delete Model",
    description: "Removes this local model from Ollama storage.",
    consequence: "Model must be re-pulled or re-imported before reuse.",
  },
};

export function applyHelpTooltip(target: HTMLElement | null, key: HelpKey): void {
  if (!target) return;
  const text = HELP_TEXTS[key];
  if (!text) return;

  target.dataset.helpKey = key;
  target.dataset.tooltipTitle = text.title;
  target.dataset.tooltipBody = text.description;
  target.dataset.tooltipConsequence = text.consequence;
  target.classList.add("has-help-tooltip");
}

export function renderAIRefinementStaticHelp(): void {
  if (typeof document === "undefined") return;

  applyHelpTooltip(
    document.getElementById("ai-refinement-provider-model-title"),
    "ai_refinement_provider_model_section"
  );
  applyHelpTooltip(document.getElementById("ai-fallback-local-lane-title"), "ai_refinement_local_primary");
  applyHelpTooltip(document.getElementById("ai-fallback-online-lane-title"), "ai_refinement_online_fallback");
  applyHelpTooltip(document.getElementById("ai-fallback-local-primary-action"), "ollama_action_install");
  applyHelpTooltip(document.getElementById("ai-fallback-local-import-action"), "ollama_action_import");
  applyHelpTooltip(document.getElementById("ai-fallback-local-detect-action"), "ollama_action_detect");
  applyHelpTooltip(document.getElementById("ai-fallback-local-use-system-action"), "ollama_action_use_system");
  applyHelpTooltip(document.getElementById("ai-fallback-local-verify-action"), "ollama_action_verify");
  applyHelpTooltip(document.getElementById("ai-fallback-local-refresh-action"), "ollama_action_refresh");
  applyHelpTooltip(document.getElementById("ai-refinement-models-title"), "ollama_models_section");
  applyHelpTooltip(document.getElementById("ai-fallback-cloud-provider-list"), "ai_refinement_auth_method");
  applyHelpTooltip(document.getElementById("ai-auth-method"), "ai_refinement_auth_method");
  applyHelpTooltip(document.getElementById("ai-refinement-topic-title"), "ai_refinement_topic_section");
}
