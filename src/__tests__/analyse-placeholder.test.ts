import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

describe("analyse placeholder", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    vi.resetModules();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows placeholder toast when analyse button is clicked", async () => {
    const showToast = vi.fn();
    vi.doMock("../toast", () => ({ showToast }));

    document.body.innerHTML = '<button id="analyse-button">Analyse</button>';
    const { wireEvents } = await import("../event-listeners");

    wireEvents();
    document.getElementById("analyse-button")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(showToast).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "info",
        title: "Analyse module",
      }),
    );
  });
});
