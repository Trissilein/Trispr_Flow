import { describe, expect, it } from "vitest";
import {
  normalizeRefinementPromptPreset,
  resolveEffectiveRefinementPrompt,
  resolveRefinementPresetPrompt,
} from "../refinement-prompts";

describe("refinement prompt presets", () => {
  it("normalizes unknown profile to wording", () => {
    expect(normalizeRefinementPromptPreset("unknown")).toBe("wording");
  });

  it("returns german preset text when language is de", () => {
    const prompt = resolveRefinementPresetPrompt("summary", "de");
    expect(prompt).toContain("Stichpunkten");
  });

  it("uses custom prompt when custom profile is active", () => {
    const prompt = resolveEffectiveRefinementPrompt("custom", "en", "Use this custom prompt.", false);
    expect(prompt).toBe("Use this custom prompt.");
  });

  it("does not append language lock for custom profile", () => {
    const prompt = resolveEffectiveRefinementPrompt(
      "custom",
      "en",
      "Custom should stay exactly this.",
      true
    );
    expect(prompt).toBe("Custom should stay exactly this.");
  });

  it("appends language-lock guard when preserve_source_language is enabled", () => {
    const prompt = resolveEffectiveRefinementPrompt("wording", "en", "", true);
    expect(prompt).toContain("Keep the output in the same language as the input. Do not translate.");
  });
});
