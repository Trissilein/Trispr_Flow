// Overlay settings rendering (R3 slice 2).
//
// Renders the "Overlay appearance" settings panel: dot/KITT style visibility,
// shared appearance (colour, rise/fall timing, opacity, position), radius
// sliders, dimension bounds, refining indicator controls, and TTS-stop button.
//
// Exports:
//   Primary   — renderOverlaySettings()         called by renderSettings() in index.ts
//   Secondary — updateOverlayStyleVisibility()  called by overlay.wire.ts
//               applyOverlaySharedUi()          called by overlay.wire.ts
//
// Per Decision 6 (settings-decomposition.md): all other functions are private.
// posX/posY ownership lives entirely in applyOverlaySharedUi().

import * as dom from "../dom-refs";
import { settings } from "../state";
import { DEFAULT_ACCENT_COLOR, normalizeColorHex } from "../utils";
import type { OverlayRefiningIndicatorPreset } from "../types";

function detectOverlayViewport(): { width: number; height: number } {
    const screenWidth = Number(
        (typeof window !== "undefined"
            ? window.screen?.availWidth ?? window.screen?.width
            : 0) ?? 0
    );
    const screenHeight = Number(
        (typeof window !== "undefined"
            ? window.screen?.availHeight ?? window.screen?.height
            : 0) ?? 0
    );
    const width = Number.isFinite(screenWidth) && screenWidth > 0 ? screenWidth : 1920;
    const height = Number.isFinite(screenHeight) && screenHeight > 0 ? screenHeight : 1080;
    return { width, height };
}

function applyOverlayDimensionSliderBounds() {
    const { width, height } = detectOverlayViewport();
    const kittMaxWidthCap = Math.max(50, Math.round(width * 0.5));
    const dotMaxRadiusCap = Math.max(8, Math.round(Math.min(width, height) * 0.25)); // 50% diameter

    if (dom.overlayKittMaxWidth) {
        dom.overlayKittMaxWidth.max = String(kittMaxWidthCap);
        dom.overlayKittMaxWidth.setAttribute("aria-valuemax", String(kittMaxWidthCap));
    }
    if (dom.overlayMaxRadius) {
        dom.overlayMaxRadius.max = String(dotMaxRadiusCap);
        dom.overlayMaxRadius.setAttribute("aria-valuemax", String(dotMaxRadiusCap));
    }
    if (dom.overlayMinRadius) {
        const minRadiusCap = Math.max(4, dotMaxRadiusCap);
        dom.overlayMinRadius.max = String(minRadiusCap);
        dom.overlayMinRadius.setAttribute("aria-valuemax", String(minRadiusCap));
    }
}

function clampToSliderBounds(input: HTMLInputElement, value: number): number {
    const parsedMin = Number(input.min);
    const parsedMax = Number(input.max);
    let out = value;
    if (Number.isFinite(parsedMin)) out = Math.max(parsedMin, out);
    if (Number.isFinite(parsedMax)) out = Math.min(parsedMax, out);
    return out;
}

export function updateOverlayStyleVisibility(style: string) {
    const isKitt = style === "kitt";
    if (dom.overlayDotSettings) dom.overlayDotSettings.style.display = isKitt ? "none" : "block";
    if (dom.overlayKittSettings) dom.overlayKittSettings.style.display = isKitt ? "block" : "none";
}

function getOverlaySharedSettings(style: string, current: typeof settings) {
    if (!current) return null;
    if (style === "kitt") {
        return {
            color: current.overlay_kitt_color,
            rise_ms: current.overlay_kitt_rise_ms,
            fall_ms: current.overlay_kitt_fall_ms,
            opacity_inactive: current.overlay_kitt_opacity_inactive,
            opacity_active: current.overlay_kitt_opacity_active,
        };
    }
    return {
        color: current.overlay_color,
        rise_ms: current.overlay_rise_ms,
        fall_ms: current.overlay_fall_ms,
        opacity_inactive: current.overlay_opacity_inactive,
        opacity_active: current.overlay_opacity_active,
    };
}

export function applyOverlaySharedUi(style: string) {
    if (!settings) return;
    const shared = getOverlaySharedSettings(style, settings);
    if (!shared) return;

    if (dom.overlayColor) dom.overlayColor.value = shared.color;
    let effectiveRise = shared.rise_ms;
    if (dom.overlayRise) dom.overlayRise.value = shared.rise_ms.toString();
    if (dom.overlayRise) {
        const maxRise = Number(dom.overlayRise.max || "200");
        if (Number.isFinite(maxRise) && maxRise > 0 && shared.rise_ms > maxRise) {
            dom.overlayRise.value = String(maxRise);
            effectiveRise = maxRise;
        }
    }
    if (dom.overlayRiseValue) dom.overlayRiseValue.textContent = `${effectiveRise}`;
    let effectiveFall = shared.fall_ms;
    if (dom.overlayFall) dom.overlayFall.value = shared.fall_ms.toString();
    if (dom.overlayFall) {
        const maxFall = Number(dom.overlayFall.max || "200");
        if (Number.isFinite(maxFall) && maxFall > 0 && shared.fall_ms > maxFall) {
            dom.overlayFall.value = String(maxFall);
            effectiveFall = maxFall;
        }
    }
    if (dom.overlayFallValue) dom.overlayFallValue.textContent = `${effectiveFall}`;
    if (dom.overlayOpacityInactive) {
        dom.overlayOpacityInactive.value = Math.round(shared.opacity_inactive * 100).toString();
    }
    if (dom.overlayOpacityInactiveValue) {
        dom.overlayOpacityInactiveValue.textContent = `${Math.round(shared.opacity_inactive * 100)}%`;
    }
    if (dom.overlayOpacityActive) {
        dom.overlayOpacityActive.value = Math.round(shared.opacity_active * 100).toString();
    }
    if (dom.overlayOpacityActiveValue) {
        dom.overlayOpacityActiveValue.textContent = `${Math.round(shared.opacity_active * 100)}%`;
    }
    if (dom.overlayPosX) {
        dom.overlayPosX.value = Math.round(
            style === "kitt" ? settings.overlay_kitt_pos_x : settings.overlay_pos_x
        ).toString();
    }
    if (dom.overlayPosY) {
        dom.overlayPosY.value = Math.round(
            style === "kitt" ? settings.overlay_kitt_pos_y : settings.overlay_pos_y
        ).toString();
    }
}

function normalizeRefiningIndicatorColor(value: string | undefined): string {
    return normalizeColorHex(value, "#6ec8ff");
}

function normalizeRefiningIndicatorSpeedMs(value: number | undefined): number {
    const numberValue = Number(value);
    if (!Number.isFinite(numberValue)) return 1150;
    return Math.max(450, Math.min(3000, Math.round(numberValue)));
}

function normalizeRefiningIndicatorRange(value: number | undefined): number {
    const numberValue = Number(value);
    if (!Number.isFinite(numberValue)) return 100;
    return Math.max(60, Math.min(180, Math.round(numberValue)));
}

function normalizeOverlayRefiningPreset(
    preset?: string | null
): OverlayRefiningIndicatorPreset {
    if (preset === "subtle" || preset === "intense") return preset;
    return "standard";
}

export function renderOverlaySettings(): void {
    if (!settings) return;

    applyOverlayDimensionSliderBounds();

    if (dom.overlayMinRadius) {
        const clamped = clampToSliderBounds(
            dom.overlayMinRadius,
            Math.round(settings.overlay_min_radius)
        );
        dom.overlayMinRadius.value = clamped.toString();
        settings.overlay_min_radius = clamped;
    }
    if (dom.overlayMinRadiusValue) dom.overlayMinRadiusValue.textContent = `${Math.round(settings.overlay_min_radius)}`;
    if (dom.overlayMaxRadius) {
        const clamped = clampToSliderBounds(
            dom.overlayMaxRadius,
            Math.round(settings.overlay_max_radius)
        );
        dom.overlayMaxRadius.value = clamped.toString();
        settings.overlay_max_radius = clamped;
    }
    if (dom.overlayMaxRadiusValue) dom.overlayMaxRadiusValue.textContent = `${Math.round(settings.overlay_max_radius)}`;
    const overlayStyleValue = settings.overlay_style || "dot";
    if (dom.overlayStyle) dom.overlayStyle.value = overlayStyleValue;
    if (dom.overlayRefiningIndicatorEnabled) {
        dom.overlayRefiningIndicatorEnabled.checked = settings.overlay_refining_indicator_enabled ?? true;
    }
    settings.overlay_refining_indicator_preset = normalizeOverlayRefiningPreset(
        settings.overlay_refining_indicator_preset
    );
    if (dom.overlayRefiningIndicatorPreset) {
        dom.overlayRefiningIndicatorPreset.value = settings.overlay_refining_indicator_preset;
    }

    settings.overlay_refining_indicator_color = normalizeRefiningIndicatorColor(
        settings.overlay_refining_indicator_color
    );
    settings.overlay_refining_indicator_speed_ms = normalizeRefiningIndicatorSpeedMs(
        settings.overlay_refining_indicator_speed_ms
    );
    settings.overlay_refining_indicator_range = normalizeRefiningIndicatorRange(
        settings.overlay_refining_indicator_range
    );
    settings.overlay_tts_stop_shape = settings.overlay_tts_stop_shape === "round" ? "round" : "compact";
    settings.overlay_tts_stop_color = normalizeColorHex(
        settings.overlay_tts_stop_color,
        DEFAULT_ACCENT_COLOR
    );
    if (dom.overlayRefiningIndicatorColor) {
        dom.overlayRefiningIndicatorColor.value = settings.overlay_refining_indicator_color;
    }
    if (dom.overlayRefiningIndicatorSpeed) {
        dom.overlayRefiningIndicatorSpeed.value = String(settings.overlay_refining_indicator_speed_ms);
    }
    if (dom.overlayRefiningIndicatorSpeedValue) {
        dom.overlayRefiningIndicatorSpeedValue.textContent = `${settings.overlay_refining_indicator_speed_ms} ms`;
    }
    if (dom.overlayRefiningIndicatorRange) {
        dom.overlayRefiningIndicatorRange.value = String(settings.overlay_refining_indicator_range);
    }
    if (dom.overlayRefiningIndicatorRangeValue) {
        dom.overlayRefiningIndicatorRangeValue.textContent = `${settings.overlay_refining_indicator_range}%`;
    }
    if (dom.overlayTtsStopEnabled) {
        dom.overlayTtsStopEnabled.checked = Boolean(settings.overlay_tts_stop_enabled);
    }
    if (dom.overlayTtsStopShape) {
        dom.overlayTtsStopShape.value = settings.overlay_tts_stop_shape;
    }
    if (dom.overlayTtsStopColor) {
        dom.overlayTtsStopColor.value = settings.overlay_tts_stop_color;
    }
    updateOverlayStyleVisibility(overlayStyleValue);
    applyOverlaySharedUi(overlayStyleValue);
    if (dom.overlayKittMinWidth) dom.overlayKittMinWidth.value = Math.round(settings.overlay_kitt_min_width).toString();
    if (dom.overlayKittMinWidthValue) dom.overlayKittMinWidthValue.textContent = `${Math.round(settings.overlay_kitt_min_width)}`;
    if (dom.overlayKittMaxWidth) {
        const clamped = clampToSliderBounds(
            dom.overlayKittMaxWidth,
            Math.round(settings.overlay_kitt_max_width)
        );
        dom.overlayKittMaxWidth.value = clamped.toString();
        settings.overlay_kitt_max_width = clamped;
    }
    if (dom.overlayKittMaxWidthValue) dom.overlayKittMaxWidthValue.textContent = `${Math.round(settings.overlay_kitt_max_width)}`;
    if (dom.overlayKittHeight) dom.overlayKittHeight.value = Math.round(settings.overlay_kitt_height).toString();
    if (dom.overlayKittHeightValue) dom.overlayKittHeightValue.textContent = `${Math.round(settings.overlay_kitt_height)}`;
}
