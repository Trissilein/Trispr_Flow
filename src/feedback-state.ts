import type { RecordingState, Settings } from "./types";

type RuntimeState = "idle" | "recording" | "transcribing";

type ChannelFeedbackView = {
  activityState: RuntimeState;
  labelState: RecordingState;
  labelText: string;
  enabled: boolean;
};

const feedbackState: {
  captureEnabled: boolean;
  captureRuntime: RuntimeState;
} = {
  captureEnabled: true,
  captureRuntime: "idle",
};

function normalizeRuntimeState(state: string | null | undefined): RuntimeState {
  if (state === "recording" || state === "transcribing") {
    return state;
  }
  return "idle";
}

export function applyFeedbackSettings(nextSettings: Settings | null): void {
  if (!nextSettings) return;

  feedbackState.captureEnabled = nextSettings.capture_enabled;

  if (!feedbackState.captureEnabled) {
    feedbackState.captureRuntime = "idle";
  }
}

export function transitionCaptureRuntime(state: RecordingState | string): void {
  const next = normalizeRuntimeState(state);
  if (!feedbackState.captureEnabled) {
    feedbackState.captureRuntime = "idle";
    return;
  }

  // Capture channel only has active/not-active semantics in UI.
  feedbackState.captureRuntime = next === "idle" ? "idle" : "recording";
}

function getCaptureView(): ChannelFeedbackView {
  if (!feedbackState.captureEnabled) {
    return {
      activityState: "idle",
      labelState: "disabled",
      labelText: "Deactivated",
      enabled: false,
    };
  }

  if (feedbackState.captureRuntime === "recording") {
    return {
      activityState: "recording",
      labelState: "recording",
      labelText: "Active",
      enabled: true,
    };
  }

  return {
    activityState: "idle",
    labelState: "idle",
    labelText: "Idle",
    enabled: true,
  };
}

export function getFeedbackView(): { capture: ChannelFeedbackView } {
  return {
    capture: getCaptureView(),
  };
}
