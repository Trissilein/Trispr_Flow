import { describe, expect, it } from "vitest";
import {
  actionHintForIntent,
  immediateDraftForIntent,
  isImmediateDirectActionIntent,
} from "../workflow-agent-intents";

describe("workflow-agent immediate intents", () => {
  it("treats reminder_capture as immediate direct action", () => {
    expect(isImmediateDirectActionIntent("reminder_capture")).toBe(true);
    expect(immediateDraftForIntent("reminder_capture")).toBe("Capturing that reminder now.");
  });

  it("keeps gdd_generate_publish out of direct action lane", () => {
    expect(isImmediateDirectActionIntent("gdd_generate_publish")).toBe(false);
  });

  it("uses immediate-action hint for reminder_capture", () => {
    expect(actionHintForIntent("reminder_capture")).toBe(
      "Immediate action intent detected. It runs directly and does not use the confirm lane."
    );
  });

  it("keeps unknown intent hint read-only", () => {
    expect(actionHintForIntent("unknown")).toBe(
      "Wakeword was recognized, but no side-effect intent was detected. Reply stays safe and read-only."
    );
  });

  it("describes confirm_or_cancel as a confirmation action", () => {
    expect(actionHintForIntent("confirm_or_cancel")).toBe(
      "Confirmation intent detected. It resolves the pending confirmation directly."
    );
  });
});
