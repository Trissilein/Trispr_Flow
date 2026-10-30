import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskCaptureSettings } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("../toast", () => ({ showToast: vi.fn() }));

afterEach(() => {
  document.body.innerHTML = "";
  invokeMock.mockReset();
});

describe("Task Capture visibility metadata", () => {
  it("marks repeated route field roles with shared keys, then announces its render", async () => {
    document.body.innerHTML = `<div id="task-capture-panel-body"></div>`;
    const settings: TaskCaptureSettings = {
      routes: [{
        label: "Agenda",
        keywords: ["agenda"],
        endpoint: "http://127.0.0.1:8177/agenda",
        confluence_page_id: "123",
      }],
      match_mode: "contains",
      ai_refinement_enabled: true,
      refinement_prompt: "Keep tasks concise.",
    };
    invokeMock.mockResolvedValue(settings);
    const rendered = vi.fn();
    window.addEventListener("settings-visibility:dynamic-rendered", rendered, { once: true });

    vi.resetModules();
    const { renderTaskCaptureTab } = await import("../task-capture-config");
    await renderTaskCaptureTab();

    const fields = Array.from(document.querySelectorAll<HTMLElement>('[data-settings-visibility-group="Task Capture"]'));
    expect(fields.map((field) => field.dataset.settingsVisibilityKey)).toEqual(
      expect.arrayContaining([
        "task.routes.label",
        "task.routes.endpoint",
        "task.routes.test",
        "task.add_route",
      ]),
    );
    expect(fields.every((field) => !field.dataset.settingsVisibilityKey?.match(/\d/))).toBe(true);
    expect(rendered).toHaveBeenCalledTimes(1);
  });
});
