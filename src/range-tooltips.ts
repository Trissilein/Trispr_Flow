export type RangeTooltipDefinition = {
  inputId: string;
  text: string;
};

export const RANGE_TOOLTIP_DEFINITIONS: RangeTooltipDefinition[] = [
  {
    inputId: "conversation-font-size",
    text: "Adjust conversation transcript font size in this view only.",
  },
  {
    inputId: "vad-threshold",
    text: "Minimum voice level required to start or keep capture. Higher values are less sensitive.",
  },
  {
    inputId: "vad-silence",
    text: "Silence grace period before recording stops and current audio is dumped.",
  },
  {
    inputId: "mic-gain",
    text: "Input amplification before VAD/transcription. Increase if microphone level is too low.",
  },
  {
    inputId: "continuous-mic-soft-flush",
    text: "Preferred adaptive flush interval during long continuous mic speech.",
  },
  {
    inputId: "continuous-mic-silence-flush",
    text: "Mic silence grace period. When silence reaches this value, current chunk is dumped.",
  },
  {
    inputId: "continuous-mic-hard-cut",
    text: "Maximum mic chunk duration. After this limit a dump is always created and saved.",
  },
  {
    inputId: "audio-cues-volume",
    text: "Volume of the recording start/stop cue sounds.",
  },
  {
    inputId: "transcribe-vad-threshold",
    text: "Minimum system-audio level treated as active signal.",
  },
  {
    inputId: "transcribe-vad-silence",
    text: "Silence grace period before system-audio chunk is dumped.",
  },
  {
    inputId: "transcribe-gain",
    text: "Input gain for system audio before VAD/transcription.",
  },
  {
    inputId: "transcribe-batch-interval",
    text: "Preferred adaptive flush interval when no clear silence boundary appears.",
  },
  {
    inputId: "transcribe-chunk-overlap",
    text: "Audio context carried into the next chunk to avoid cut-off words.",
  },
  {
    inputId: "continuous-hard-cut",
    text: "Global maximum chunk duration. After this limit a dump is always created and saved.",
  },
  {
    inputId: "continuous-min-chunk",
    text: "Minimum chunk duration required before a dump is allowed.",
  },
  {
    inputId: "continuous-pre-roll",
    text: "Audio kept before detected speech start to preserve leading syllables.",
  },
  {
    inputId: "continuous-post-roll",
    text: "Audio kept after detected speech end to avoid clipped endings.",
  },
  {
    inputId: "continuous-keepalive",
    text: "If idle this long, force a keepalive dump/checkpoint.",
  },
  {
    inputId: "continuous-system-soft-flush",
    text: "System-specific adaptive flush interval during continuous audio.",
  },
  {
    inputId: "continuous-system-silence-flush",
    text: "System-specific silence grace period before dumping.",
  },
  {
    inputId: "continuous-system-hard-cut",
    text: "System-specific maximum chunk duration. Always dumps at this limit.",
  },
  {
    inputId: "ai-fallback-temperature",
    text: "Creativity vs consistency for AI refinement. Lower values produce more deterministic edits.",
  },
  {
    inputId: "overlay-min-radius",
    text: "Minimum dot size at very low input level.",
  },
  {
    inputId: "overlay-max-radius",
    text: "Maximum dot size at high input level.",
  },
  {
    inputId: "overlay-kitt-min-width",
    text: "Minimum KITT bar width at very low input level.",
  },
  {
    inputId: "overlay-kitt-max-width",
    text: "Maximum KITT bar width at high input level.",
  },
  {
    inputId: "overlay-kitt-height",
    text: "Height of the KITT bar.",
  },
  {
    inputId: "overlay-rise",
    text: "How quickly the overlay grows when audio level rises.",
  },
  {
    inputId: "overlay-fall",
    text: "How quickly the overlay shrinks when audio level falls.",
  },
  {
    inputId: "overlay-opacity-inactive",
    text: "Overlay opacity while idle/inactive.",
  },
  {
    inputId: "overlay-opacity-active",
    text: "Overlay opacity while recording or transcribing.",
  },
];

let activeTooltipHost: HTMLElement | null = null;
let globalListenersAttached = false;

function setTooltipOpen(host: HTMLElement, open: boolean) {
  host.classList.toggle("tooltip-open", open);
  const trigger = host.querySelector<HTMLButtonElement>(".tooltip-trigger");
  if (trigger) {
    trigger.setAttribute("aria-expanded", open ? "true" : "false");
  }
}

function closeActiveTooltip() {
  if (!activeTooltipHost) return;
  setTooltipOpen(activeTooltipHost, false);
  activeTooltipHost = null;
}

function openTooltip(host: HTMLElement) {
  if (activeTooltipHost && activeTooltipHost !== host) {
    setTooltipOpen(activeTooltipHost, false);
  }
  setTooltipOpen(host, true);
  activeTooltipHost = host;
}

function resolveTooltipAnchor(input: HTMLInputElement): HTMLElement | null {
  if (input.id === "conversation-font-size") {
    const fontLabel = document.querySelector("#conversation-font-controls .font-label");
    return fontLabel instanceof HTMLElement ? fontLabel : null;
  }

  const field = input.closest(".field");
  if (!field) return null;

  const label = field.querySelector(".field-label");
  return label instanceof HTMLElement ? label : null;
}

function buildTooltipId(inputId: string): string {
  return `tooltip-${inputId}`;
}

function sanitizeLabel(label: string): string {
  return label.replace(/\s+/g, " ").trim();
}

function ensureTooltipNode(
  anchor: HTMLElement,
  input: HTMLInputElement,
  definition: RangeTooltipDefinition
): HTMLElement {
  const selector = `.tooltip-host[data-tooltip-for="${definition.inputId}"]`;
  const existing = anchor.querySelector(selector);
  if (existing instanceof HTMLElement) {
    const bubble = existing.querySelector<HTMLElement>(".tooltip-bubble");
    if (bubble) {
      bubble.textContent = definition.text;
    }
    return existing;
  }

  const host = document.createElement("span");
  host.className = "tooltip-host";
  host.dataset.tooltipFor = definition.inputId;

  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.className = "tooltip-trigger";
  const inputName = sanitizeLabel(input.getAttribute("aria-label") || input.id.replace(/-/g, " "));
  trigger.setAttribute("aria-label", `Show help for ${inputName}`);
  trigger.setAttribute("aria-expanded", "false");

  const bubble = document.createElement("span");
  bubble.className = "tooltip-bubble";
  bubble.setAttribute("role", "tooltip");
  bubble.id = buildTooltipId(definition.inputId);
  bubble.textContent = definition.text;

  trigger.setAttribute("aria-controls", bubble.id);
  trigger.setAttribute("aria-describedby", bubble.id);
  trigger.textContent = "i";

  trigger.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();

    if (host.classList.contains("tooltip-open")) {
      closeActiveTooltip();
      return;
    }
    openTooltip(host);
  });

  host.append(trigger, bubble);
  anchor.appendChild(host);
  return host;
}

function onDocumentClick(event: MouseEvent) {
  if (!activeTooltipHost) return;
  if (!(event.target instanceof Node)) {
    closeActiveTooltip();
    return;
  }
  if (activeTooltipHost.contains(event.target)) return;
  closeActiveTooltip();
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeActiveTooltip();
  }
}

function ensureGlobalListeners() {
  if (globalListenersAttached) return;
  document.addEventListener("click", onDocumentClick, true);
  document.addEventListener("keydown", onDocumentKeydown);
  globalListenersAttached = true;
}

export function initRangeTooltips() {
  if (typeof document === "undefined") return;
  if (activeTooltipHost && !document.body.contains(activeTooltipHost)) {
    activeTooltipHost = null;
  }

  ensureGlobalListeners();

  for (const definition of RANGE_TOOLTIP_DEFINITIONS) {
    const input = document.getElementById(definition.inputId);
    if (!(input instanceof HTMLInputElement) || input.type !== "range") {
      continue;
    }

    input.title = definition.text;

    const anchor = resolveTooltipAnchor(input);
    if (!anchor) {
      continue;
    }

    ensureTooltipNode(anchor, input, definition);
  }
}
