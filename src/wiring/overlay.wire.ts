// Overlay-settings-panel wiring (R2 slice 2).
//
// Owns DOM event listeners for the "Overlay appearance" settings cluster:
// colour, radius range, rise/fall timing, opacity (active + inactive),
// position, style (dot/kitt), refining indicator (enable/preset/colour/
// speed/range), the optional TTS-stop button, KITT-mode dimensions, and
// the Apply button.
//
// These listeners only mutate `settings.overlay_*` fields and persist.
// The runtime overlay window itself is controlled from Rust via
// `window.eval()` (see CONTEXT.md — Overlay), which reads the persisted
// settings; nothing in this file talks to the overlay WebView directly.
//
// Per OQ-3 (refactoring-plan.md, 2026-05-15) the contract is:
//   - export function wire<Domain>(): void
//   - imports dom + helpers directly; role-level peer of event-listeners.ts
//   - shared snippets live in ./wire-helpers.ts

import * as dom from "../dom-refs";
import { settings } from "../state";
import { updateOverlayStyleVisibility, applyOverlaySharedUi } from "../settings/overlay.settings";
import { persistSettings } from "../settings-persist";
import { updateRangeAria } from "../accessibility";
import { showToast } from "../toast";
import { onChangePersist } from "./wire-helpers";

export function wireOverlay(): void {
  // ───────── Core appearance (colour, radius, rise/fall, opacity) ─────────

  dom.overlayColor?.addEventListener("input", () => {
    if (!settings || !dom.overlayColor) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_color = dom.overlayColor.value;
    } else {
      settings.overlay_color = dom.overlayColor.value;
    }
  });

  onChangePersist(dom.overlayColor);

  dom.overlayMinRadius?.addEventListener("input", () => {
    if (!settings || !dom.overlayMinRadius || !dom.overlayMaxRadius) return;
    settings.overlay_min_radius = Number(dom.overlayMinRadius.value);
    if (settings.overlay_min_radius > settings.overlay_max_radius) {
      settings.overlay_max_radius = settings.overlay_min_radius;
      dom.overlayMaxRadius.value = Math.round(settings.overlay_max_radius).toString();
    }
    if (dom.overlayMinRadiusValue) {
      dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    }
    if (dom.overlayMaxRadiusValue) {
      dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    }
    updateRangeAria("overlay-min-radius", settings.overlay_min_radius);
  });

  onChangePersist(dom.overlayMinRadius);

  dom.overlayMaxRadius?.addEventListener("input", () => {
    if (!settings || !dom.overlayMaxRadius || !dom.overlayMinRadius) return;
    settings.overlay_max_radius = Number(dom.overlayMaxRadius.value);
    if (settings.overlay_max_radius < settings.overlay_min_radius) {
      settings.overlay_min_radius = settings.overlay_max_radius;
      dom.overlayMinRadius.value = Math.round(settings.overlay_min_radius).toString();
    }
    if (dom.overlayMinRadiusValue) {
      dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    }
    if (dom.overlayMaxRadiusValue) {
      dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    }
    updateRangeAria("overlay-max-radius", settings.overlay_max_radius);
  });

  onChangePersist(dom.overlayMaxRadius);

  dom.overlayRise?.addEventListener("input", () => {
    if (!settings || !dom.overlayRise) return;
    const value = Number(dom.overlayRise.value);
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_rise_ms = value;
    } else {
      settings.overlay_rise_ms = value;
    }
    if (dom.overlayRiseValue) dom.overlayRiseValue.textContent = `${value}`;
    updateRangeAria("overlay-rise", value);
  });

  onChangePersist(dom.overlayRise);

  dom.overlayFall?.addEventListener("input", () => {
    if (!settings || !dom.overlayFall) return;
    const value = Number(dom.overlayFall.value);
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_fall_ms = value;
    } else {
      settings.overlay_fall_ms = value;
    }
    if (dom.overlayFallValue) dom.overlayFallValue.textContent = `${value}`;
    updateRangeAria("overlay-fall", value);
  });

  onChangePersist(dom.overlayFall);

  dom.overlayOpacityInactive?.addEventListener("input", () => {
    if (!settings || !dom.overlayOpacityInactive || !dom.overlayOpacityActive) return;
    const value = Math.min(1, Math.max(0.05, Number(dom.overlayOpacityInactive.value) / 100));
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_opacity_inactive = value;
      if (settings.overlay_kitt_opacity_active < settings.overlay_kitt_opacity_inactive) {
        settings.overlay_kitt_opacity_active = settings.overlay_kitt_opacity_inactive;
        dom.overlayOpacityActive.value = Math.round(settings.overlay_kitt_opacity_active * 100).toString();
      }
      if (dom.overlayOpacityInactiveValue) {
        dom.overlayOpacityInactiveValue.textContent = `${Math.round(settings.overlay_kitt_opacity_inactive * 100)}%`;
      }
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_kitt_opacity_active * 100)}%`;
      }
    } else {
      settings.overlay_opacity_inactive = value;
      if (settings.overlay_opacity_active < settings.overlay_opacity_inactive) {
        settings.overlay_opacity_active = settings.overlay_opacity_inactive;
        dom.overlayOpacityActive.value = Math.round(settings.overlay_opacity_active * 100).toString();
      }
      if (dom.overlayOpacityInactiveValue) {
        dom.overlayOpacityInactiveValue.textContent = `${Math.round(settings.overlay_opacity_inactive * 100)}%`;
      }
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
      }
    }
    updateRangeAria("overlay-opacity-inactive", Number(dom.overlayOpacityInactive.value));
  });

  onChangePersist(dom.overlayOpacityInactive);

  dom.overlayOpacityActive?.addEventListener("input", () => {
    if (!settings || !dom.overlayOpacityActive || !dom.overlayOpacityInactive) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      const value = Math.min(
        1,
        Math.max(settings.overlay_kitt_opacity_inactive, Number(dom.overlayOpacityActive.value) / 100)
      );
      settings.overlay_kitt_opacity_active = value;
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_kitt_opacity_active * 100)}%`;
      }
    } else {
      const value = Math.min(
        1,
        Math.max(settings.overlay_opacity_inactive, Number(dom.overlayOpacityActive.value) / 100)
      );
      settings.overlay_opacity_active = value;
      if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(settings.overlay_opacity_active * 100)}%`;
      }
    }
    updateRangeAria("overlay-opacity-active", Number(dom.overlayOpacityActive.value));
  });

  onChangePersist(dom.overlayOpacityActive);

  // ───────── Position and style ─────────

  dom.overlayPosX?.addEventListener("change", async () => {
    if (!settings || !dom.overlayPosX) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_pos_x = Number(dom.overlayPosX.value);
    } else {
      settings.overlay_pos_x = Number(dom.overlayPosX.value);
    }
    await persistSettings();
  });

  dom.overlayPosY?.addEventListener("change", async () => {
    if (!settings || !dom.overlayPosY) return;
    if ((settings.overlay_style || "dot") === "kitt") {
      settings.overlay_kitt_pos_y = Number(dom.overlayPosY.value);
    } else {
      settings.overlay_pos_y = Number(dom.overlayPosY.value);
    }
    await persistSettings();
  });

  dom.overlayStyle?.addEventListener("change", async () => {
    if (!settings || !dom.overlayStyle) return;
    settings.overlay_style = dom.overlayStyle.value;
    updateOverlayStyleVisibility(dom.overlayStyle.value);
    applyOverlaySharedUi(dom.overlayStyle.value);
    await persistSettings();
  });

  // ───────── Refining indicator ─────────

  dom.overlayRefiningIndicatorEnabled?.addEventListener("change", async () => {
    if (!settings || !dom.overlayRefiningIndicatorEnabled) return;
    settings.overlay_refining_indicator_enabled = dom.overlayRefiningIndicatorEnabled.checked;
    await persistSettings();
  });

  dom.overlayRefiningIndicatorPreset?.addEventListener("change", async () => {
    if (!settings || !dom.overlayRefiningIndicatorPreset) return;
    const value = dom.overlayRefiningIndicatorPreset.value;
    settings.overlay_refining_indicator_preset =
      value === "subtle" || value === "intense" ? value : "standard";
    await persistSettings();
  });

  dom.overlayRefiningIndicatorColor?.addEventListener("input", () => {
    if (!settings || !dom.overlayRefiningIndicatorColor) return;
    settings.overlay_refining_indicator_color = dom.overlayRefiningIndicatorColor.value;
  });

  onChangePersist(dom.overlayRefiningIndicatorColor);

  dom.overlayRefiningIndicatorSpeed?.addEventListener("input", () => {
    if (!settings || !dom.overlayRefiningIndicatorSpeed) return;
    const value = Math.max(450, Math.min(3000, Number(dom.overlayRefiningIndicatorSpeed.value)));
    settings.overlay_refining_indicator_speed_ms = value;
    if (dom.overlayRefiningIndicatorSpeedValue) {
      dom.overlayRefiningIndicatorSpeedValue.textContent = `${value} ms`;
    }
    updateRangeAria("overlay-refining-indicator-speed", value);
  });

  onChangePersist(dom.overlayRefiningIndicatorSpeed);

  dom.overlayRefiningIndicatorRange?.addEventListener("input", () => {
    if (!settings || !dom.overlayRefiningIndicatorRange) return;
    const value = Math.max(60, Math.min(180, Number(dom.overlayRefiningIndicatorRange.value)));
    settings.overlay_refining_indicator_range = value;
    if (dom.overlayRefiningIndicatorRangeValue) {
      dom.overlayRefiningIndicatorRangeValue.textContent = `${value}%`;
    }
    updateRangeAria("overlay-refining-indicator-range", value);
  });

  onChangePersist(dom.overlayRefiningIndicatorRange);

  // ───────── TTS-stop button on the overlay ─────────

  dom.overlayTtsStopEnabled?.addEventListener("change", async () => {
    if (!settings) return;
    settings.overlay_tts_stop_enabled = Boolean(dom.overlayTtsStopEnabled?.checked);
    await persistSettings();
  });

  dom.overlayTtsStopShape?.addEventListener("change", async () => {
    if (!settings || !dom.overlayTtsStopShape) return;
    settings.overlay_tts_stop_shape = dom.overlayTtsStopShape.value === "round" ? "round" : "compact";
    await persistSettings();
  });

  dom.overlayTtsStopColor?.addEventListener("input", () => {
    if (!settings || !dom.overlayTtsStopColor) return;
    settings.overlay_tts_stop_color = dom.overlayTtsStopColor.value;
  });

  onChangePersist(dom.overlayTtsStopColor);

  // ───────── KITT-mode dimensions ─────────

  dom.overlayKittMinWidth?.addEventListener("input", () => {
    if (!settings || !dom.overlayKittMinWidth) return;
    settings.overlay_kitt_min_width = Number(dom.overlayKittMinWidth.value);
    if (dom.overlayKittMinWidthValue) dom.overlayKittMinWidthValue.textContent = `${Math.round(settings.overlay_kitt_min_width)}`;
    updateRangeAria("overlay-kitt-min-width", settings.overlay_kitt_min_width);
  });

  onChangePersist(dom.overlayKittMinWidth);

  dom.overlayKittMaxWidth?.addEventListener("input", () => {
    if (!settings || !dom.overlayKittMaxWidth) return;
    settings.overlay_kitt_max_width = Number(dom.overlayKittMaxWidth.value);
    if (dom.overlayKittMaxWidthValue) dom.overlayKittMaxWidthValue.textContent = `${Math.round(settings.overlay_kitt_max_width)}`;
    updateRangeAria("overlay-kitt-max-width", settings.overlay_kitt_max_width);
  });

  onChangePersist(dom.overlayKittMaxWidth);

  dom.overlayKittHeight?.addEventListener("input", () => {
    if (!settings || !dom.overlayKittHeight) return;
    settings.overlay_kitt_height = Number(dom.overlayKittHeight.value);
    if (dom.overlayKittHeightValue) dom.overlayKittHeightValue.textContent = `${Math.round(settings.overlay_kitt_height)}`;
    updateRangeAria("overlay-kitt-height", settings.overlay_kitt_height);
  });

  onChangePersist(dom.overlayKittHeight);

  // ───────── Apply Overlay Settings button ─────────

  dom.applyOverlayBtn?.addEventListener("click", async () => {
    if (!settings) return;
    await persistSettings();
    showToast({ title: "Applied", message: "Overlay settings applied", type: "success" });
  });
}
