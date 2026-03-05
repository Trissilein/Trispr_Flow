import { describe, expect, it } from "vitest";
import {
  ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD,
  requiresOneClickPublishConfirmation,
} from "../gdd-policy";

describe("gdd one-click publish policy", () => {
  it("requires confirmation when confidence is below threshold", () => {
    expect(
      requiresOneClickPublishConfirmation(ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD - 0.01)
    ).toBe(true);
  });

  it("does not require confirmation when confidence meets threshold", () => {
    expect(
      requiresOneClickPublishConfirmation(ONE_CLICK_ROUTE_CONFIDENCE_THRESHOLD)
    ).toBe(false);
  });

  it("requires confirmation when confidence is missing", () => {
    expect(requiresOneClickPublishConfirmation(undefined)).toBe(true);
    expect(requiresOneClickPublishConfirmation(null)).toBe(true);
    expect(requiresOneClickPublishConfirmation(Number.NaN)).toBe(true);
  });
});
