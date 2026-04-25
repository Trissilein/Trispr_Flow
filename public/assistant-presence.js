const canvas = document.getElementById("presence-canvas");
const stateDot = document.getElementById("state-dot");
const stateLabel = document.getElementById("state-label");
const lastHeard = document.getElementById("last-heard");
const replyText = document.getElementById("reply-text");
const intentText = document.getElementById("intent-text");
const actionText = document.getElementById("action-text");
const capabilityText = document.getElementById("capability-text");
const reasonText = document.getElementById("reason-text");

const ctx = canvas.getContext("2d");

const viewState = {
  assistantState: "idle",
  lastHeard: "No utterance captured yet.",
  reply: "Waiting for the next assistant event.",
  intent: "No intent detected yet.",
  action: "No pending action.",
  capability: "Assistant baseline unavailable.",
  reason: "No gate decision yet.",
};

const statePalette = {
  idle: "#69d4ff",
  listening: "#6cf0dc",
  parsing: "#9bc9ff",
  planning: "#ffd36d",
  awaiting_confirm: "#ffb869",
  executing: "#91f39f",
  recovering: "#ff8668",
};

const dots = Array.from({ length: 84 }, (_, index) => ({
  seed: index / 84,
  radius: 1.5 + (index % 4) * 0.45,
  offset: Math.random() * Math.PI * 2,
}));

const currentWindow = (() => {
  const api = window.__TAURI__?.window;
  if (!api || typeof api.getCurrentWindow !== "function") {
    return null;
  }
  try {
    return api.getCurrentWindow();
  } catch {
    return null;
  }
})();

function bindDragHandles() {
  if (!currentWindow || typeof currentWindow.startDragging !== "function") {
    return;
  }
  document.querySelectorAll("[data-tauri-drag-region]").forEach((handle) => {
    handle.addEventListener("mousedown", (event) => {
      if (event.button !== 0) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("button, a, input, textarea, select, summary, [data-no-drag]")) {
        return;
      }
      event.preventDefault();
      void currentWindow.startDragging().catch(() => {});
    });
  });
}

function setCanvasSize() {
  const rect = canvas.getBoundingClientRect();
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(rect.width * ratio));
  canvas.height = Math.max(1, Math.round(rect.height * ratio));
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
}

function accentForState(state) {
  return statePalette[state] || statePalette.idle;
}

function renderText() {
  const accent = accentForState(viewState.assistantState);
  stateLabel.textContent = viewState.assistantState.replace(/_/g, " ");
  stateDot.style.color = accent;
  stateDot.style.background = accent;
  lastHeard.textContent = viewState.lastHeard;
  lastHeard.classList.toggle("is-muted", viewState.lastHeard === "No utterance captured yet.");
  replyText.textContent = viewState.reply;
  replyText.classList.toggle("is-muted", viewState.reply === "Waiting for the next assistant event.");
  intentText.textContent = viewState.intent;
  actionText.textContent = viewState.action;
  capabilityText.textContent = viewState.capability;
  reasonText.textContent = viewState.reason;
}

function renderCloud(timestamp) {
  const rect = canvas.getBoundingClientRect();
  const width = rect.width;
  const height = rect.height;
  const accent = accentForState(viewState.assistantState);
  const time = timestamp * 0.0012;
  const baseRadiusByState = {
    idle: 42,
    listening: 56,
    parsing: 48,
    planning: 60,
    awaiting_confirm: 52,
    executing: 66,
    recovering: 58,
  };
  const energyByState = {
    idle: 0.24,
    listening: 0.68,
    parsing: 0.35,
    planning: 0.42,
    awaiting_confirm: 0.16,
    executing: 0.74,
    recovering: 0.52,
  };
  const baseRadius = baseRadiusByState[viewState.assistantState] || 42;
  const energy = energyByState[viewState.assistantState] || 0.3;

  ctx.clearRect(0, 0, width, height);
  ctx.save();
  ctx.translate(width / 2, height / 2 - 6);

  const glow = ctx.createRadialGradient(0, 0, 4, 0, 0, baseRadius * 1.8);
  glow.addColorStop(0, `${accent}55`);
  glow.addColorStop(0.6, `${accent}11`);
  glow.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = glow;
  ctx.beginPath();
  ctx.arc(0, 0, baseRadius * 1.8, 0, Math.PI * 2);
  ctx.fill();

  dots.forEach((dot, index) => {
    const theta = dot.seed * Math.PI * 2 + time * (0.18 + energy * 0.4) + dot.offset;
    const wobble = Math.sin(time * 1.4 + dot.seed * 9.5) * (12 + energy * 16);
    const radial = baseRadius + wobble + Math.cos(time * 0.9 + index) * 5;
    const x = Math.cos(theta) * radial;
    const y = Math.sin(theta) * (radial * 0.72);
    ctx.globalAlpha = 0.22 + energy * 0.38 + ((index % 5) * 0.03);
    ctx.fillStyle = accent;
    ctx.beginPath();
    ctx.arc(x, y, dot.radius + energy * 0.8, 0, Math.PI * 2);
    ctx.fill();
  });

  ctx.restore();
  requestAnimationFrame(renderCloud);
}

function summarizeCapability(payload) {
  const capability = payload?.capability;
  if (!capability) {
    return "Assistant baseline unavailable.";
  }
  const core = capability.assistant_core_available ?? capability.workflow_agent_available;
  if (!core) {
    return "Assistant Core unavailable.";
  }
  if (capability.degraded) {
    const missing = Array.isArray(capability.missing_capabilities)
      ? capability.missing_capabilities.join(", ")
      : "reduced capability";
    return `Degraded: ${missing}`;
  }
  return "Assistant core ready.";
}

function setState(payload) {
  if (payload?.state) {
    viewState.assistantState = payload.state;
  }
  if (payload?.reason) {
    viewState.reason = payload.reason;
  }
  const capabilitySummary = summarizeCapability(payload);
  if (capabilitySummary) {
    viewState.capability = capabilitySummary;
  }
  renderText();
}

function listen(eventName, handler) {
  const api = window.__TAURI__?.event?.listen;
  if (typeof api !== "function") return;
  api(eventName, (event) => handler(event?.payload)).catch(() => {});
}

window.addEventListener("resize", setCanvasSize);
setCanvasSize();
renderText();
bindDragHandles();
requestAnimationFrame(renderCloud);

listen("assistant:state-changed", (payload) => {
  setState(payload);
});

listen("assistant:intent-detected", (payload) => {
  if (payload?.parse?.command_text) {
    viewState.lastHeard = payload.parse.command_text;
  }
  if (payload?.parse?.intent) {
    viewState.intent = `${payload.parse.intent} · ${Math.round((payload.parse.confidence || 0) * 100)}%`;
  }
  setState(payload);
});

listen("assistant:plan-ready", (payload) => {
  if (payload?.plan?.summary) {
    viewState.reply = `Plan ready: ${payload.plan.summary}`;
    viewState.action = "Plan prepared. Awaiting confirmation lane.";
  }
  setState(payload);
});

listen("assistant:awaiting-confirmation", (payload) => {
  if (payload?.plan?.summary) {
    viewState.action = payload.confirm_token
      ? `Confirm ${payload.confirm_token} within ${payload.confirm_timeout_sec}s`
      : `Confirm within ${payload.confirm_timeout_sec}s`;
    viewState.reply = payload.plan.summary;
  }
  setState(payload);
});

listen("assistant:confirmation-expired", (payload) => {
  viewState.action = "Confirmation expired.";
  setState(payload);
});

listen("assistant:action-result", (payload) => {
  if (payload?.result?.message) {
    viewState.reply = payload.result.message;
    viewState.action = `Action ${payload.result.status || "completed"}`;
  }
  setState(payload);
});

listen("assistant:reply-draft", (payload) => {
  if (payload?.text) {
    viewState.reply = payload.text;
  }
  if (payload?.intent) {
    viewState.intent = String(payload.intent);
  }
  if (payload?.reason) {
    viewState.reason = payload.reason;
  }
  renderText();
});

listen("assistant:reply-final", (payload) => {
  if (payload?.text) {
    viewState.reply = payload.text;
  }
  if (payload?.intent) {
    viewState.intent = String(payload.intent);
  }
  if (payload?.reason) {
    viewState.reason = payload.reason;
  }
  renderText();
});

listen("transcription:raw-result", (payload) => {
  if (payload?.text) {
    viewState.lastHeard = `${payload.source || "unknown"}: ${payload.text}`;
    renderText();
  }
});

const invoke = window.__TAURI__?.core?.invoke;
if (typeof invoke === "function") {
  invoke("get_settings")
    .then((settings) => {
      if (settings?.product_mode !== "assistant") {
        viewState.action = "Assistant mode inactive.";
        renderText();
      }
    })
    .catch(() => {});
}
