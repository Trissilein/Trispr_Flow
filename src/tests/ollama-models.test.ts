import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

import {
  buildInstalledOllamaInventory,
  resolveCuratedModelState,
} from "../ollama-models";

describe("ollama-models helpers", () => {
  describe("resolveCuratedModelState", () => {
    it("marks a selected but missing model as missing instead of active", () => {
      expect(resolveCuratedModelState("qwen3.5:4b", "qwen3.5:4b", false, false)).toBe("missing");
    });

    it("marks a selected installed model as active", () => {
      expect(resolveCuratedModelState("qwen3.5:4b", "qwen3.5:4b", true, false)).toBe("active");
    });

    it("prefers downloading state while a pull is in progress", () => {
      expect(resolveCuratedModelState("qwen3.5:4b", "qwen3.5:4b", false, true)).toBe("downloading");
    });
  });

  describe("buildInstalledOllamaInventory", () => {
    it("sorts active first, then curated, then custom models", () => {
      const inventory = buildInstalledOllamaInventory(
        [
          { name: "custom-import:latest", size_bytes: 3 * 1024 * 1024 * 1024 },
          { name: "qwen3.5:2b", size_bytes: 2 * 1024 * 1024 * 1024 },
          { name: "gemma4:e4b", size_bytes: 10 * 1024 * 1024 * 1024 },
        ],
        "gemma4:e4b"
      );

      expect(inventory.map((item) => item.name)).toEqual([
        "gemma4:e4b",
        "qwen3.5:2b",
        "custom-import:latest",
      ]);
      expect(inventory[0].is_active).toBe(true);
      expect(inventory[0].can_activate).toBe(false);
      expect(inventory[0].can_uninstall).toBe(false);
      expect(inventory[1].is_curated).toBe(true);
      expect(inventory[2].is_curated).toBe(false);
    });

    it("deduplicates installed tags case-insensitively", () => {
      const inventory = buildInstalledOllamaInventory(
        [
          { name: "QWEN3.5:4B", size_bytes: 2 * 1024 * 1024 * 1024 },
          { name: "qwen3.5:4b", size_bytes: 2 * 1024 * 1024 * 1024 },
        ],
        ""
      );

      expect(inventory).toHaveLength(1);
      expect(inventory[0].size_label).toContain("GB");
    });
  });
});
