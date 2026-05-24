import { beforeEach, describe, expect, it, vi } from "vitest";

vi.hoisted(() => {
    document.body.innerHTML = `
    <input id="overlay-color" type="color" />
    <input id="overlay-min-radius" type="range" min="4" max="32" />
    <span id="overlay-min-radius-value"></span>
    <input id="overlay-max-radius" type="range" min="8" max="1080" />
    <span id="overlay-max-radius-value"></span>
    <input id="overlay-rise" type="range" min="20" max="200" />
    <span id="overlay-rise-value"></span>
    <input id="overlay-fall" type="range" min="20" max="200" />
    <span id="overlay-fall-value"></span>
    <input id="overlay-opacity-inactive" type="range" />
    <span id="overlay-opacity-inactive-value"></span>
    <input id="overlay-opacity-active" type="range" />
    <span id="overlay-opacity-active-value"></span>
    <input id="overlay-pos-x" type="number" />
    <input id="overlay-pos-y" type="number" />
    <select id="overlay-style">
      <option value="dot">dot</option>
      <option value="kitt">kitt</option>
    </select>
    <div id="overlay-dot-settings"></div>
    <div id="overlay-kitt-settings"></div>
    <input id="overlay-refining-indicator-enabled" type="checkbox" />
    <select id="overlay-refining-indicator-preset">
      <option value="standard">standard</option>
      <option value="subtle">subtle</option>
      <option value="intense">intense</option>
    </select>
    <input id="overlay-refining-indicator-color" type="color" />
    <input id="overlay-refining-indicator-speed" type="range" min="450" max="3000" />
    <span id="overlay-refining-indicator-speed-value"></span>
    <input id="overlay-refining-indicator-range" type="range" min="60" max="180" />
    <span id="overlay-refining-indicator-range-value"></span>
    <input id="overlay-tts-stop-enabled" type="checkbox" />
    <select id="overlay-tts-stop-shape">
      <option value="compact">compact</option>
      <option value="round">round</option>
    </select>
    <input id="overlay-tts-stop-color" type="color" />
    <input id="overlay-kitt-min-width" type="range" />
    <span id="overlay-kitt-min-width-value"></span>
    <input id="overlay-kitt-max-width" type="range" min="50" max="3840" />
    <span id="overlay-kitt-max-width-value"></span>
    <input id="overlay-kitt-height" type="range" />
    <span id="overlay-kitt-height-value"></span>
  `;
});

import { applyOverlaySharedUi, renderOverlaySettings, updateOverlayStyleVisibility } from "../settings/overlay.settings";
import { setSettings, settings } from "../state";
import type { Settings } from "../types";

function makeSettings(overrides: Partial<Settings> = {}): Settings {
    return {
        overlay_style: "dot",
        overlay_min_radius: 10,
        overlay_max_radius: 40,
        overlay_color: "#112233",
        overlay_rise_ms: 40,
        overlay_fall_ms: 80,
        overlay_opacity_inactive: 0.5,
        overlay_opacity_active: 0.9,
        overlay_pos_x: 100,
        overlay_pos_y: 200,
        overlay_kitt_color: "#445566",
        overlay_kitt_rise_ms: 60,
        overlay_kitt_fall_ms: 90,
        overlay_kitt_opacity_inactive: 0.4,
        overlay_kitt_opacity_active: 0.8,
        overlay_kitt_pos_x: 300,
        overlay_kitt_pos_y: 400,
        overlay_kitt_min_width: 20,
        overlay_kitt_max_width: 500,
        overlay_kitt_height: 18,
        overlay_refining_indicator_enabled: true,
        overlay_refining_indicator_preset: "standard",
        overlay_refining_indicator_color: "#6ec8ff",
        overlay_refining_indicator_speed_ms: 1150,
        overlay_refining_indicator_range: 100,
        overlay_tts_stop_enabled: true,
        overlay_tts_stop_shape: "compact",
        overlay_tts_stop_color: "#ff00aa",
        ...overrides,
    } as unknown as Settings;
}

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

beforeEach(() => {
    setSettings(makeSettings());
});

describe("updateOverlayStyleVisibility", () => {
    it("shows dot controls and hides KITT controls for dot style", () => {
        updateOverlayStyleVisibility("dot");
        expect(byId<HTMLDivElement>("overlay-dot-settings").style.display).toBe("block");
        expect(byId<HTMLDivElement>("overlay-kitt-settings").style.display).toBe("none");
    });

    it("shows KITT controls and hides dot controls for KITT style", () => {
        updateOverlayStyleVisibility("kitt");
        expect(byId<HTMLDivElement>("overlay-dot-settings").style.display).toBe("none");
        expect(byId<HTMLDivElement>("overlay-kitt-settings").style.display).toBe("block");
    });

    it("treats unknown styles as dot controls", () => {
        updateOverlayStyleVisibility("unknown");
        expect(byId<HTMLDivElement>("overlay-dot-settings").style.display).toBe("block");
        expect(byId<HTMLDivElement>("overlay-kitt-settings").style.display).toBe("none");
    });
});

describe("applyOverlaySharedUi", () => {
    it("renders dot shared settings", () => {
        applyOverlaySharedUi("dot");
        expect(byId<HTMLInputElement>("overlay-color").value).toBe("#112233");
        expect(byId<HTMLInputElement>("overlay-rise").value).toBe("40");
        expect(byId<HTMLElement>("overlay-rise-value").textContent).toBe("40");
        expect(byId<HTMLInputElement>("overlay-opacity-active").value).toBe("90");
        expect(byId<HTMLElement>("overlay-opacity-active-value").textContent).toBe("90%");
        expect(byId<HTMLInputElement>("overlay-pos-x").value).toBe("100");
    });

    it("renders KITT shared settings", () => {
        applyOverlaySharedUi("kitt");
        expect(byId<HTMLInputElement>("overlay-color").value).toBe("#445566");
        expect(byId<HTMLInputElement>("overlay-rise").value).toBe("60");
        expect(byId<HTMLInputElement>("overlay-opacity-inactive").value).toBe("40");
        expect(byId<HTMLInputElement>("overlay-pos-x").value).toBe("300");
        expect(byId<HTMLInputElement>("overlay-pos-y").value).toBe("400");
    });

    it("is a no-op when settings is null", () => {
        setSettings(null);
        byId<HTMLInputElement>("overlay-color").value = "#000000";
        applyOverlaySharedUi("dot");
        expect(byId<HTMLInputElement>("overlay-color").value).toBe("#000000");
    });

    it("clamps rise and fall labels to control max values", () => {
        setSettings(makeSettings({ overlay_rise_ms: 500, overlay_fall_ms: 600 }));
        applyOverlaySharedUi("dot");
        expect(byId<HTMLInputElement>("overlay-rise").value).toBe("200");
        expect(byId<HTMLElement>("overlay-rise-value").textContent).toBe("200");
        expect(byId<HTMLInputElement>("overlay-fall").value).toBe("200");
        expect(byId<HTMLElement>("overlay-fall-value").textContent).toBe("200");
    });
});

describe("renderOverlaySettings", () => {
    it("renders overlay style, radius, KITT dimensions, and shared settings", () => {
        renderOverlaySettings();
        expect(byId<HTMLSelectElement>("overlay-style").value).toBe("dot");
        expect(byId<HTMLInputElement>("overlay-min-radius").value).toBe("10");
        expect(byId<HTMLElement>("overlay-min-radius-value").textContent).toBe("10");
        expect(byId<HTMLInputElement>("overlay-kitt-min-width").value).toBe("20");
        expect(byId<HTMLInputElement>("overlay-kitt-max-width").value).toBe("500");
        expect(byId<HTMLElement>("overlay-kitt-height-value").textContent).toBe("18");
        expect(byId<HTMLInputElement>("overlay-color").value).toBe("#112233");
    });

    it("is a no-op when settings is null", () => {
        setSettings(null);
        byId<HTMLInputElement>("overlay-min-radius").value = "12";
        renderOverlaySettings();
        expect(byId<HTMLInputElement>("overlay-min-radius").value).toBe("12");
    });

    it("clamps overlay radii and KITT max width to slider bounds", () => {
        setSettings(makeSettings({ overlay_min_radius: 999, overlay_max_radius: 9999, overlay_kitt_max_width: 9999 }));
        renderOverlaySettings();
        expect(settings?.overlay_min_radius).toBe(Number(byId<HTMLInputElement>("overlay-min-radius").max));
        expect(settings?.overlay_max_radius).toBe(Number(byId<HTMLInputElement>("overlay-max-radius").max));
        expect(settings?.overlay_kitt_max_width).toBe(Number(byId<HTMLInputElement>("overlay-kitt-max-width").max));
    });

    it("normalizes overlay-refining indicator values", () => {
        setSettings(makeSettings({
            overlay_refining_indicator_preset: "unknown" as any,
            overlay_refining_indicator_color: "not-a-color",
            overlay_refining_indicator_speed_ms: 12,
            overlay_refining_indicator_range: 999,
            overlay_tts_stop_shape: "square" as any,
            overlay_tts_stop_color: "bad-color",
        }));
        renderOverlaySettings();
        expect(settings?.overlay_refining_indicator_preset).toBe("standard");
        expect(byId<HTMLSelectElement>("overlay-refining-indicator-preset").value).toBe("standard");
        expect(settings?.overlay_refining_indicator_color).toBe("#6ec8ff");
        expect(settings?.overlay_refining_indicator_speed_ms).toBe(450);
        expect(settings?.overlay_refining_indicator_range).toBe(180);
        expect(settings?.overlay_tts_stop_shape).toBe("compact");
        expect(settings?.overlay_tts_stop_color).toBe("#4be0d4");
    });

    it("renders overlay-refining indicator controls", () => {
        renderOverlaySettings();
        expect(byId<HTMLInputElement>("overlay-refining-indicator-enabled").checked).toBe(true);
        expect(byId<HTMLInputElement>("overlay-refining-indicator-color").value).toBe("#6ec8ff");
        expect(byId<HTMLInputElement>("overlay-refining-indicator-speed").value).toBe("1150");
        expect(byId<HTMLElement>("overlay-refining-indicator-speed-value").textContent).toBe("1150 ms");
        expect(byId<HTMLInputElement>("overlay-refining-indicator-range").value).toBe("100");
        expect(byId<HTMLElement>("overlay-refining-indicator-range-value").textContent).toBe("100%");
    });

    it("renders overlay TTS stop controls", () => {
        renderOverlaySettings();
        expect(byId<HTMLInputElement>("overlay-tts-stop-enabled").checked).toBe(true);
        expect(byId<HTMLSelectElement>("overlay-tts-stop-shape").value).toBe("compact");
        expect(byId<HTMLInputElement>("overlay-tts-stop-color").value).toBe("#ff00aa");
    });
});
