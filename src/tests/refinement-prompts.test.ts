import { describe, expect, it } from "vitest";
import {
  findUserRefinementPromptPresetByOptionId,
  NEW_REFINEMENT_PROMPT_OPTION_ID,
  normalizeActiveRefinementPromptPresetId,
  normalizePersistedRefinementPromptPresetId,
  normalizeRefinementPromptPreset,
  normalizeUserRefinementPromptPresets,
  resolveEffectiveRefinementPrompt,
  resolveRefinementPresetPrompt,
  toUserRefinementPromptOptionId,
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
    expect(prompt).toContain("Keep the output in the same language as the input");
    expect(prompt).toContain("Do not translate");
  });

  it("uses auto language-lock guard when language hint is auto", () => {
    const prompt = resolveEffectiveRefinementPrompt("wording", "auto", "", true);
    expect(prompt).toContain("Detect the input language and keep it unchanged.");
    expect(prompt).toContain("preserve each segment in its original language");
  });

  it("does not append language-lock guard for llm_prompt profile", () => {
    const promptEn = resolveEffectiveRefinementPrompt("llm_prompt", "en", "", true);
    expect(promptEn).toContain("You are an expert prompt engineer");
    expect(promptEn).toContain("Always write the resulting prompt in English");
    expect(promptEn).not.toContain("Keep the output in the same language");

    const promptDe = resolveEffectiveRefinementPrompt("llm_prompt", "de", "", true);
    expect(promptDe).toContain("Du bist ein erfahrener Prompt-Engineer");
    expect(promptDe).toContain("Schreibe den fertigen Prompt immer auf Englisch");
    expect(promptDe).not.toContain("Behalte die Ausgabe in derselben Sprache");
  });

  it("normalizes user presets and drops invalid entries", () => {
    const presets = normalizeUserRefinementPromptPresets([
      { id: "  Team Prompt  ", name: " Team ", prompt: "  Prompt body  " },
      { id: "", name: "No", prompt: "x" },
      { id: "team-prompt", name: "Duplicate", prompt: "x" },
    ]);
    expect(presets).toHaveLength(1);
    expect(presets[0]).toEqual({
      id: "team-prompt",
      name: "Team",
      prompt: "Prompt body",
    });
  });

  it("keeps active user preset id only when preset exists", () => {
    const presets = normalizeUserRefinementPromptPresets([
      { id: "qa", name: "QA", prompt: "Prompt QA" },
    ]);
    expect(
      normalizeActiveRefinementPromptPresetId("user:qa", "wording", presets)
    ).toBe("user:qa");
    expect(
      normalizeActiveRefinementPromptPresetId("user:missing", "custom", presets)
    ).toBe("custom");
  });

  it("keeps new preset option id stable", () => {
    expect(
      normalizeActiveRefinementPromptPresetId(NEW_REFINEMENT_PROMPT_OPTION_ID, "custom", [])
    ).toBe(NEW_REFINEMENT_PROMPT_OPTION_ID);
  });

  it("never persists new preset option id", () => {
    expect(
      normalizePersistedRefinementPromptPresetId(NEW_REFINEMENT_PROMPT_OPTION_ID, "custom", [])
    ).toBe("custom");
    expect(
      normalizePersistedRefinementPromptPresetId(NEW_REFINEMENT_PROMPT_OPTION_ID, "wording", [])
    ).toBe("wording");
  });

  it("finds user preset by option id", () => {
    const presets = normalizeUserRefinementPromptPresets([
      { id: "ops", name: "Ops", prompt: "Prompt Ops" },
    ]);
    const selected = findUserRefinementPromptPresetByOptionId(
      presets,
      toUserRefinementPromptOptionId("ops")
    );
    expect(selected?.name).toBe("Ops");
  });
});
