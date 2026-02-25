import { describe, expect, it } from "vitest";
import { isExactModelTagMatch, normalizeModelTag } from "../ollama-tag-utils";

describe("ollama-tag-utils", () => {
  it("normalizes model tags with trim + lowercase", () => {
    expect(normalizeModelTag("  QWEN3:8B  ")).toBe("qwen3:8b");
  });

  it("matches exact tags only", () => {
    expect(isExactModelTagMatch("qwen3:8b", "QWEN3:8B")).toBe(true);
    expect(isExactModelTagMatch("qwen3:8b", "qwen3:14b")).toBe(false);
    expect(isExactModelTagMatch("qwen3", "qwen3:8b")).toBe(false);
  });
});
