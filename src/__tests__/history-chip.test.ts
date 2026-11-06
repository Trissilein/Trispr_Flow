// History refinement chip: reason text from entry.refinement_skip_reason (contract 3).
import { describe, expect, it } from "vitest";
import { buildRefinementChip } from "../history";
import type { HistoryEntry } from "../types";

function entry(extra: Partial<HistoryEntry> = {}): HistoryEntry {
  return { id: "e1", text: "hello world", timestamp_ms: 1000, source: "mic", ...extra };
}

describe("history refinement chip", () => {
  const PAST_TEXT: Record<string, string> = {
    model_not_loaded: "model was not loaded",
    ps_query_failed: "Ollama status could not be read",
    gpu_busy: "GPU was busy",
    cold_start_raw_first: "model was warming up",
    raw_timeout: "refinement timed out",
    queue_full: "refinement queue was full",
    prepare_failed: "refinement setup failed",
    provider_error: "Ollama returned an error",
    disabled: "refinement was off",
  };

  for (const [code, past] of Object.entries(PAST_TEXT)) {
    it(`shows the reason for ${code}`, () => {
      const chip = buildRefinementChip(entry({ refinement_skip_reason: code }));

      expect(chip.textContent).toBe(`Raw — ${past}`);
      expect(chip.title).toContain(`Pasted raw: ${past}.`);
      expect(chip.getAttribute("aria-label")).toBe(`Refinement status: Raw — ${past}`);
      expect(chip.classList.contains("is-raw")).toBe(true);
    });
  }

  it("keeps the plain chip when the field is missing, null or empty", () => {
    for (const extra of [{}, { refinement_skip_reason: null }, { refinement_skip_reason: "  " }]) {
      const chip = buildRefinementChip(entry(extra));

      expect(chip.textContent).toBe("Raw only");
      expect(chip.title).toBe("");
      expect(chip.classList.contains("has-reason")).toBe(false);
    }
  });

  it("maps the legacy gate code and shows unknown codes readable", () => {
    expect(
      buildRefinementChip(entry({ refinement_skip_reason: "ollama_model_not_loaded" })).textContent,
    ).toBe("Raw — model was not loaded");
    expect(
      buildRefinementChip(entry({ refinement_skip_reason: "future_reason" })).textContent,
    ).toBe("Raw — future reason");
  });

  it("keeps the inspector hint on clickable chips and puts the reason first", () => {
    const chip = buildRefinementChip(
      entry({
        refinement_skip_reason: "raw_timeout",
        refinement: {
          job_id: "j1",
          raw: "hello world",
          refined: "",
          status: "idle",
          model: "gemma",
          error: "",
        },
      }),
    );

    expect(chip.tagName).toBe("BUTTON");
    expect(chip.textContent).toBe("Raw — refinement timed out");
    expect(chip.title).toBe(
      "Pasted raw: refinement timed out. Open refinement inspector for this entry",
    );
  });

  it("leaves the label alone when a late refinement completed, but notes the raw paste", () => {
    const chip = buildRefinementChip(
      entry({
        refinement_skip_reason: "raw_timeout",
        refinement: {
          job_id: "j2",
          raw: "hello world",
          refined: "Hello, world.",
          status: "refined",
          model: "gemma",
          error: "",
        },
      }),
    );

    expect(chip.textContent).toBe("Refined");
    expect(chip.title).toContain("Pasted raw: refinement timed out.");
  });
});
