import type { AgentIntent } from "./types";

const IMMEDIATE_DIRECT_ACTION_INTENTS = new Set<string>([
  "reminder_capture",
  "web_search",
  "open_module",
  "open_app",
]);

export function isImmediateDirectActionIntent(intent: string | null | undefined): boolean {
  return IMMEDIATE_DIRECT_ACTION_INTENTS.has((intent ?? "").trim());
}

export function immediateDraftForIntent(intent: string | null | undefined): string {
  switch ((intent ?? "").trim()) {
    case "reminder_capture":
      return "Capturing that reminder now.";
    case "web_search":
      return "Checking the best web route for that.";
    case "open_module":
      return "Opening the matching Trispr surface.";
    case "open_app":
      return "Launching that app.";
    case "gdd_generate_publish":
      return "Matching the right transcript session.";
    default:
      return "Working on that now.";
  }
}

export function actionHintForIntent(intent: AgentIntent | string | null | undefined): string {
  switch ((intent ?? "").trim()) {
    case "gdd_generate_publish":
      return "GDD intent detected. Select a session, build plan, then confirm execution.";
    case "confirm_or_cancel":
      return "Confirmation intent detected. It resolves the pending confirmation directly.";
    case "unknown":
      return "Wakeword was recognized, but no side-effect intent was detected. Reply stays safe and read-only.";
    case "reminder_capture":
    case "web_search":
    case "open_module":
    case "open_app":
      return "Immediate action intent detected. It runs directly and does not use the confirm lane.";
    default:
      return "Intent is read-only. Confirm lane is not required.";
  }
}
