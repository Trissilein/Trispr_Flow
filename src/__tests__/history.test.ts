import { afterEach, describe, expect, it } from "vitest";
import type { HistoryEntry } from "../types";
import { buildConversationHistory, buildConversationText } from "../history";
import { setHistory, setTranscribeHistory } from "../state";

describe("history helpers", () => {
  afterEach(() => {
    setHistory([]);
    setTranscribeHistory([]);
  });

  it("combines and sorts mic + system history by newest first", () => {
    const mic: HistoryEntry = {
      id: "mic-1",
      text: "Mic entry",
      timestamp_ms: 1000,
      source: "mic",
    };
    const system: HistoryEntry = {
      id: "sys-1",
      text: "System entry",
      timestamp_ms: 2000,
      source: "output",
    };

    setHistory([mic]);
    setTranscribeHistory([system]);

    const combined = buildConversationHistory();
    expect(combined).toHaveLength(2);
    expect(combined[0].id).toBe("sys-1");
    expect(combined[1].id).toBe("mic-1");
  });

  it("builds conversation text with speaker labels", () => {
    const entries: HistoryEntry[] = [
      {
        id: "a",
        text: "Hello",
        timestamp_ms: 1000,
        source: "mic",
      },
      {
        id: "b",
        text: "World",
        timestamp_ms: 2000,
        source: "output",
      },
    ];

    const text = buildConversationText(entries);
    expect(text).toContain("Microphone: Hello");
    expect(text).toContain("System Audio: World");
    expect(text).toContain("\n");
    expect(text.trim().startsWith("[")).toBe(true);
  });
});
