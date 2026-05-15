/**
 * Smoke tests for wireOverlay() — R2 slice 2.
 *
 * Integration-style per OQ-3: build the DOM, call wireOverlay(), dispatch
 * real events, assert observable state. No mocking of settings.ts (the
 * primary helper module being wired); only the Tauri boundary (invoke —
 * globally mocked via tauri.setup) is mocked.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

// Inject DOM nodes before module imports so dom-refs.ts captures them.
vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>

    <input id="overlay-color" type="color" value="#ffffff" />

    <input id="overlay-min-radius" type="range" min="4" max="32" value="10" />
    <span id="overlay-min-radius-value"></span>
    <input id="overlay-max-radius" type="range" min="8" max="1080" value="15" />
    <span id="overlay-max-radius-value"></span>

    <input id="overlay-rise" type="range" min="20" max="200" value="40" />
    <span id="overlay-rise-value"></span>
    <input id="overlay-fall" type="range" min="20" max="200" value="80" />
    <span id="overlay-fall-value"></span>

    <input id="overlay-opacity-inactive" type="range" min="5" max="100" value="50" />
    <span id="overlay-opacity-inactive-value"></span>
    <input id="overlay-opacity-active" type="range" min="5" max="100" value="90" />
    <span id="overlay-opacity-active-value"></span>

    <input id="overlay-pos-x" type="number" value="100" />
    <input id="overlay-pos-y" type="number" value="200" />

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
    <input id="overlay-refining-indicator-color" type="color" value="#ff0000" />
    <input id="overlay-refining-indicator-speed" type="range" min="450" max="3000" value="1000" />
    <span id="overlay-refining-indicator-speed-value"></span>
    <input id="overlay-refining-indicator-range" type="range" min="60" max="180" value="100" />
    <span id="overlay-refining-indicator-range-value"></span>

    <input id="overlay-tts-stop-enabled" type="checkbox" />
    <select id="overlay-tts-stop-shape">
      <option value="compact">compact</option>
      <option value="round">round</option>
    </select>
    <input id="overlay-tts-stop-color" type="color" value="#00ff00" />

    <input id="overlay-kitt-min-width" type="range" min="4" max="40" value="10" />
    <span id="overlay-kitt-min-width-value"></span>
    <input id="overlay-kitt-max-width" type="range" min="50" max="3840" value="400" />
    <span id="overlay-kitt-max-width-value"></span>
    <input id="overlay-kitt-height" type="range" min="8" max="40" value="20" />
    <span id="overlay-kitt-height-value"></span>

    <button id="apply-overlay-btn"></button>
  `;
});

import { wireOverlay } from "../wiring/overlay.wire";
import * as dom from "../dom-refs";
import { settings, setSettings } from "../state";
import type { Settings } from "../types";

const mockedInvoke = vi.mocked(invoke);

function freshSettings(): Settings {
  return {
    overlay_color: "#ffffff",
    overlay_kitt_color: "#00ffff",
    overlay_min_radius: 10,
    overlay_max_radius: 15,
    overlay_rise_ms: 40,
    overlay_fall_ms: 80,
    overlay_kitt_rise_ms: 30,
    overlay_kitt_fall_ms: 70,
    overlay_opacity_inactive: 0.5,
    overlay_opacity_active: 0.9,
    overlay_kitt_opacity_inactive: 0.4,
    overlay_kitt_opacity_active: 0.85,
    overlay_pos_x: 100,
    overlay_pos_y: 200,
    overlay_kitt_pos_x: 150,
    overlay_kitt_pos_y: 250,
    overlay_style: "dot",
    overlay_refining_indicator_enabled: false,
    overlay_refining_indicator_preset: "standard",
    overlay_refining_indicator_color: "#ff0000",
    overlay_refining_indicator_speed_ms: 1000,
    overlay_refining_indicator_range: 100,
    overlay_tts_stop_enabled: false,
    overlay_tts_stop_shape: "compact",
    overlay_tts_stop_color: "#00ff00",
    overlay_kitt_min_width: 10,
    overlay_kitt_max_width: 400,
    overlay_kitt_height: 20,
  } as unknown as Settings;
}

let wired = false;
function wireOnce() {
  if (wired) return;
  wireOverlay();
  wired = true;
}

beforeEach(() => {
  setSettings(freshSettings());
  mockedInvoke.mockReset();
  mockedInvoke.mockResolvedValue(undefined);
  // Reset DOM control values
  if (dom.overlayColor) dom.overlayColor.value = "#ffffff";
  if (dom.overlayMinRadius) dom.overlayMinRadius.value = "10";
  if (dom.overlayMaxRadius) dom.overlayMaxRadius.value = "15";
  if (dom.overlayRise) dom.overlayRise.value = "40";
  if (dom.overlayFall) dom.overlayFall.value = "80";
  if (dom.overlayOpacityInactive) dom.overlayOpacityInactive.value = "50";
  if (dom.overlayOpacityActive) dom.overlayOpacityActive.value = "90";
  if (dom.overlayPosX) dom.overlayPosX.value = "100";
  if (dom.overlayPosY) dom.overlayPosY.value = "200";
  if (dom.overlayStyle) dom.overlayStyle.value = "dot";
  if (dom.overlayRefiningIndicatorEnabled) dom.overlayRefiningIndicatorEnabled.checked = false;
  if (dom.overlayRefiningIndicatorPreset) dom.overlayRefiningIndicatorPreset.value = "standard";
  if (dom.overlayRefiningIndicatorColor) dom.overlayRefiningIndicatorColor.value = "#ff0000";
  if (dom.overlayRefiningIndicatorSpeed) dom.overlayRefiningIndicatorSpeed.value = "1000";
  if (dom.overlayRefiningIndicatorRange) dom.overlayRefiningIndicatorRange.value = "100";
  if (dom.overlayTtsStopEnabled) dom.overlayTtsStopEnabled.checked = false;
  if (dom.overlayTtsStopShape) dom.overlayTtsStopShape.value = "compact";
  if (dom.overlayTtsStopColor) dom.overlayTtsStopColor.value = "#00ff00";
  if (dom.overlayKittMinWidth) dom.overlayKittMinWidth.value = "10";
  if (dom.overlayKittMaxWidth) dom.overlayKittMaxWidth.value = "400";
  if (dom.overlayKittHeight) dom.overlayKittHeight.value = "20";
  wireOnce();
});

afterEach(() => {
  vi.restoreAllMocks();
});

function fire(el: HTMLElement | null, type: string) {
  el?.dispatchEvent(new Event(type, { bubbles: true }));
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

describe("wireOverlay — colour", () => {
  it("input writes overlay_color in dot style", () => {
    if (dom.overlayColor) dom.overlayColor.value = "#abcdef";
    fire(dom.overlayColor, "input");
    expect(settings!.overlay_color).toBe("#abcdef");
    expect(settings!.overlay_kitt_color).toBe("#00ffff");
  });

  it("input writes overlay_kitt_color in kitt style", () => {
    settings!.overlay_style = "kitt";
    if (dom.overlayColor) dom.overlayColor.value = "#123456";
    fire(dom.overlayColor, "input");
    expect(settings!.overlay_kitt_color).toBe("#123456");
    expect(settings!.overlay_color).toBe("#ffffff");
  });

  it("change persists settings", async () => {
    fire(dom.overlayColor, "change");
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireOverlay — radius range", () => {
  it("min input updates value label and aria", () => {
    if (dom.overlayMinRadius) dom.overlayMinRadius.value = "12";
    fire(dom.overlayMinRadius, "input");
    expect(settings!.overlay_min_radius).toBe(12);
    expect(dom.overlayMinRadiusValue?.textContent).toBe("12");
  });

  it("min > max pulls max up to min", () => {
    // Start min=10, max=15. Set min=25, should pull max to 25.
    if (dom.overlayMinRadius) dom.overlayMinRadius.value = "25";
    fire(dom.overlayMinRadius, "input");
    expect(settings!.overlay_min_radius).toBe(25);
    expect(settings!.overlay_max_radius).toBe(25);
    expect(dom.overlayMaxRadius?.value).toBe("25");
  });

  it("max < min pulls min down to max", () => {
    // Start min=10, max=15. Drop max to 8, should pull min to 8.
    if (dom.overlayMaxRadius) dom.overlayMaxRadius.value = "8";
    fire(dom.overlayMaxRadius, "input");
    expect(settings!.overlay_max_radius).toBe(8);
    expect(settings!.overlay_min_radius).toBe(8);
    expect(dom.overlayMinRadius?.value).toBe("8");
  });
});

describe("wireOverlay — rise/fall timing", () => {
  it("rise input updates overlay_rise_ms in dot style", () => {
    if (dom.overlayRise) dom.overlayRise.value = "60";
    fire(dom.overlayRise, "input");
    expect(settings!.overlay_rise_ms).toBe(60);
    expect(settings!.overlay_kitt_rise_ms).toBe(30);
    expect(dom.overlayRiseValue?.textContent).toBe("60");
  });

  it("rise input updates overlay_kitt_rise_ms in kitt style", () => {
    settings!.overlay_style = "kitt";
    if (dom.overlayRise) dom.overlayRise.value = "55";
    fire(dom.overlayRise, "input");
    expect(settings!.overlay_kitt_rise_ms).toBe(55);
    expect(settings!.overlay_rise_ms).toBe(40);
  });

  it("fall input updates overlay_fall_ms in dot style", () => {
    if (dom.overlayFall) dom.overlayFall.value = "120";
    fire(dom.overlayFall, "input");
    expect(settings!.overlay_fall_ms).toBe(120);
  });
});

describe("wireOverlay — opacity", () => {
  it("inactive input writes overlay_opacity_inactive in dot style", () => {
    if (dom.overlayOpacityInactive) dom.overlayOpacityInactive.value = "30";
    fire(dom.overlayOpacityInactive, "input");
    expect(settings!.overlay_opacity_inactive).toBeCloseTo(0.3);
  });

  it("inactive opacity pulls active up when inactive > active (dot)", () => {
    // active starts at 0.9, set inactive to 95 → 0.95, should push active to 0.95
    if (dom.overlayOpacityInactive) dom.overlayOpacityInactive.value = "95";
    fire(dom.overlayOpacityInactive, "input");
    expect(settings!.overlay_opacity_inactive).toBeCloseTo(0.95);
    expect(settings!.overlay_opacity_active).toBeCloseTo(0.95);
  });

  it("inactive input writes overlay_kitt_opacity_inactive in kitt style", () => {
    settings!.overlay_style = "kitt";
    if (dom.overlayOpacityInactive) dom.overlayOpacityInactive.value = "20";
    fire(dom.overlayOpacityInactive, "input");
    expect(settings!.overlay_kitt_opacity_inactive).toBeCloseTo(0.2);
  });

  it("active opacity floored at inactive value in dot style", () => {
    // inactive is 0.5; try setting active to 30 → 0.3, should be floored to 0.5
    if (dom.overlayOpacityActive) dom.overlayOpacityActive.value = "30";
    fire(dom.overlayOpacityActive, "input");
    expect(settings!.overlay_opacity_active).toBeCloseTo(0.5);
  });

  it("active opacity floored at kitt inactive value in kitt style", () => {
    // kitt inactive is 0.4 (from freshSettings); try setting active to 20 → 0.2, should be floored to 0.4
    settings!.overlay_style = "kitt";
    if (dom.overlayOpacityActive) dom.overlayOpacityActive.value = "20";
    fire(dom.overlayOpacityActive, "input");
    expect(settings!.overlay_kitt_opacity_active).toBeCloseTo(0.4);
    expect(settings!.overlay_opacity_active).toBeCloseTo(0.9);
  });
});

describe("wireOverlay — position", () => {
  it("posX change writes overlay_pos_x and persists (dot)", async () => {
    if (dom.overlayPosX) dom.overlayPosX.value = "250";
    fire(dom.overlayPosX, "change");
    await flush();
    expect(settings!.overlay_pos_x).toBe(250);
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("posY change writes overlay_kitt_pos_y in kitt style", async () => {
    settings!.overlay_style = "kitt";
    if (dom.overlayPosY) dom.overlayPosY.value = "333";
    fire(dom.overlayPosY, "change");
    await flush();
    expect(settings!.overlay_kitt_pos_y).toBe(333);
    expect(settings!.overlay_pos_y).toBe(200);
  });
});

describe("wireOverlay — style", () => {
  it("change updates overlay_style, toggles panels, and persists", async () => {
    if (dom.overlayStyle) dom.overlayStyle.value = "kitt";
    fire(dom.overlayStyle, "change");
    await flush();
    expect(settings!.overlay_style).toBe("kitt");
    expect(dom.overlayDotSettings?.style.display).toBe("none");
    expect(dom.overlayKittSettings?.style.display).toBe("block");
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });
});

describe("wireOverlay — refining indicator", () => {
  it("enabled checkbox change persists", async () => {
    if (dom.overlayRefiningIndicatorEnabled) dom.overlayRefiningIndicatorEnabled.checked = true;
    fire(dom.overlayRefiningIndicatorEnabled, "change");
    await flush();
    expect(settings!.overlay_refining_indicator_enabled).toBe(true);
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("preset accepts 'subtle'", async () => {
    if (dom.overlayRefiningIndicatorPreset) dom.overlayRefiningIndicatorPreset.value = "subtle";
    fire(dom.overlayRefiningIndicatorPreset, "change");
    await flush();
    expect(settings!.overlay_refining_indicator_preset).toBe("subtle");
  });

  it("preset accepts 'intense'", async () => {
    if (dom.overlayRefiningIndicatorPreset) dom.overlayRefiningIndicatorPreset.value = "intense";
    fire(dom.overlayRefiningIndicatorPreset, "change");
    await flush();
    expect(settings!.overlay_refining_indicator_preset).toBe("intense");
  });

  it("preset falls back to 'standard' for unknown values", async () => {
    if (dom.overlayRefiningIndicatorPreset) {
      // Force an unknown value via setAttribute since the <option> set is closed
      const opt = document.createElement("option");
      opt.value = "garbage";
      opt.textContent = "garbage";
      dom.overlayRefiningIndicatorPreset.appendChild(opt);
      dom.overlayRefiningIndicatorPreset.value = "garbage";
    }
    fire(dom.overlayRefiningIndicatorPreset, "change");
    await flush();
    expect(settings!.overlay_refining_indicator_preset).toBe("standard");
  });

  it("color input writes overlay_refining_indicator_color", () => {
    if (dom.overlayRefiningIndicatorColor) dom.overlayRefiningIndicatorColor.value = "#deadbe";
    fire(dom.overlayRefiningIndicatorColor, "input");
    expect(settings!.overlay_refining_indicator_color).toBe("#deadbe");
  });

  it("speed input clamps below 450", () => {
    if (dom.overlayRefiningIndicatorSpeed) dom.overlayRefiningIndicatorSpeed.value = "100";
    fire(dom.overlayRefiningIndicatorSpeed, "input");
    expect(settings!.overlay_refining_indicator_speed_ms).toBe(450);
  });

  it("speed input clamps above 3000", () => {
    if (dom.overlayRefiningIndicatorSpeed) dom.overlayRefiningIndicatorSpeed.value = "9999";
    fire(dom.overlayRefiningIndicatorSpeed, "input");
    expect(settings!.overlay_refining_indicator_speed_ms).toBe(3000);
    expect(dom.overlayRefiningIndicatorSpeedValue?.textContent).toBe("3000 ms");
  });

  it("range input clamps to [60, 180]", () => {
    if (dom.overlayRefiningIndicatorRange) dom.overlayRefiningIndicatorRange.value = "10";
    fire(dom.overlayRefiningIndicatorRange, "input");
    expect(settings!.overlay_refining_indicator_range).toBe(60);
    if (dom.overlayRefiningIndicatorRange) dom.overlayRefiningIndicatorRange.value = "999";
    fire(dom.overlayRefiningIndicatorRange, "input");
    expect(settings!.overlay_refining_indicator_range).toBe(180);
    expect(dom.overlayRefiningIndicatorRangeValue?.textContent).toBe("180%");
  });
});

describe("wireOverlay — TTS stop button", () => {
  it("enabled checkbox change persists", async () => {
    if (dom.overlayTtsStopEnabled) dom.overlayTtsStopEnabled.checked = true;
    fire(dom.overlayTtsStopEnabled, "change");
    await flush();
    expect(settings!.overlay_tts_stop_enabled).toBe(true);
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
  });

  it("shape change normalizes to 'round' or 'compact'", async () => {
    if (dom.overlayTtsStopShape) dom.overlayTtsStopShape.value = "round";
    fire(dom.overlayTtsStopShape, "change");
    await flush();
    expect(settings!.overlay_tts_stop_shape).toBe("round");

    // Falls back to compact for any non-"round" value
    if (dom.overlayTtsStopShape) {
      const opt = document.createElement("option");
      opt.value = "weird";
      dom.overlayTtsStopShape.appendChild(opt);
      dom.overlayTtsStopShape.value = "weird";
    }
    fire(dom.overlayTtsStopShape, "change");
    await flush();
    expect(settings!.overlay_tts_stop_shape).toBe("compact");
  });

  it("color input writes overlay_tts_stop_color", () => {
    if (dom.overlayTtsStopColor) dom.overlayTtsStopColor.value = "#0099ff";
    fire(dom.overlayTtsStopColor, "input");
    expect(settings!.overlay_tts_stop_color).toBe("#0099ff");
  });
});

describe("wireOverlay — KITT dimensions", () => {
  it("min width input updates overlay_kitt_min_width and label", () => {
    if (dom.overlayKittMinWidth) dom.overlayKittMinWidth.value = "25";
    fire(dom.overlayKittMinWidth, "input");
    expect(settings!.overlay_kitt_min_width).toBe(25);
    expect(dom.overlayKittMinWidthValue?.textContent).toBe("25");
  });

  it("max width input updates overlay_kitt_max_width and label", () => {
    if (dom.overlayKittMaxWidth) dom.overlayKittMaxWidth.value = "800";
    fire(dom.overlayKittMaxWidth, "input");
    expect(settings!.overlay_kitt_max_width).toBe(800);
    expect(dom.overlayKittMaxWidthValue?.textContent).toBe("800");
  });

  it("height input updates overlay_kitt_height and label", () => {
    if (dom.overlayKittHeight) dom.overlayKittHeight.value = "35";
    fire(dom.overlayKittHeight, "input");
    expect(settings!.overlay_kitt_height).toBe(35);
    expect(dom.overlayKittHeightValue?.textContent).toBe("35");
  });
});

describe("wireOverlay — apply button", () => {
  it("click persists and shows a success toast", async () => {
    dom.applyOverlayBtn?.click();
    await flush();
    expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.any(Object));
    const container = document.getElementById("toast-container");
    expect(container?.querySelector(".toast.success")).not.toBeNull();
  });
});
