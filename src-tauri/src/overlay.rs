use crate::state::{AppState, Settings};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WindowEvent};
use tracing::warn;

const OVERLAY_RECOVERY_MAX_ATTEMPTS: u32 = 4;
const OVERLAY_RECOVERY_BACKOFF_MS: u64 = 140;
const OVERLAY_CREATE_COOLDOWN_MS: u64 = 1_200;
const OVERLAY_HEARTBEAT_STALE_MS: u64 = 6_000;

/// Throttles repeated create attempts after hard WebView failures.
/// Unlike the legacy lockout, this is a short cooldown and never permanent.
static OVERLAY_CREATE_COOLDOWN_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
static OVERLAY_CREATE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayState {
    Hidden,
    Armed,
    Recording,
    Transcribing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettings {
    pub color: String,
    pub min_radius: f64,
    pub max_radius: f64,
    pub rise_ms: u64,
    pub fall_ms: u64,
    pub opacity_inactive: f64,
    pub opacity_active: f64,
    pub pos_x: f64,
    pub pos_y: f64,
    pub style: String,
    pub refining_indicator_enabled: bool,
    pub refining_indicator_preset: String,
    pub refining_indicator_color: String,
    pub refining_indicator_speed_ms: u64,
    pub refining_indicator_range: f64,
    pub tts_stop_enabled: bool,
    pub tts_stop_shape: String,
    pub tts_stop_color: String,
    pub kitt_min_width: f64,
    pub kitt_max_width: f64,
    pub kitt_height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayHealthEvent {
    pub status: String,
    pub attempt: u32,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct OverlayController {
    pub desired_state: OverlayState,
    pub desired_settings: Option<OverlaySettings>,
    pub refining_active: bool,
    pub tts_stop_visible: bool,
    pub last_level: f64,
    pub last_heartbeat_ms: u64,
    pub recovery_attempt: u32,
}

impl Default for OverlayController {
    fn default() -> Self {
        Self {
            desired_state: OverlayState::Hidden,
            desired_settings: None,
            refining_active: false,
            tts_stop_visible: false,
            last_level: 0.0,
            last_heartbeat_ms: 0,
            recovery_attempt: 0,
        }
    }
}

fn with_overlay_controller<F, T>(app: &AppHandle, f: F) -> T
where
    F: FnOnce(&mut OverlayController) -> T,
{
    let state = app.state::<AppState>();
    let mut guard = state
        .overlay_controller
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

fn overlay_controller_snapshot(app: &AppHandle) -> OverlayController {
    with_overlay_controller(app, |controller| controller.clone())
}

fn emit_overlay_health(app: &AppHandle, status: &str, attempt: u32, reason: impl Into<String>) {
    let _ = app.emit(
        "overlay:health",
        OverlayHealthEvent {
            status: status.to_string(),
            attempt,
            reason: reason.into(),
        },
    );
}

fn now_ms() -> u64 {
    crate::util::now_ms()
}

fn overlay_create_cooldown_active() -> bool {
    let until = OVERLAY_CREATE_COOLDOWN_UNTIL_MS.load(Ordering::Acquire);
    until > now_ms()
}

fn set_overlay_create_cooldown(duration_ms: u64) {
    let until = now_ms().saturating_add(duration_ms);
    OVERLAY_CREATE_COOLDOWN_UNTIL_MS.store(until, Ordering::Release);
}

pub fn mark_overlay_heartbeat(app: &AppHandle) {
    with_overlay_controller(app, |controller| {
        controller.last_heartbeat_ms = now_ms();
    });
}

fn overlay_heartbeat_stale(controller: &OverlayController) -> bool {
    if matches!(controller.desired_state, OverlayState::Hidden) {
        return false;
    }
    if controller.last_heartbeat_ms == 0 {
        return false;
    }
    now_ms().saturating_sub(controller.last_heartbeat_ms) > OVERLAY_HEARTBEAT_STALE_MS
}

pub fn prime_overlay_controller(
    app: &AppHandle,
    desired_settings: Option<OverlaySettings>,
    desired_state: OverlayState,
) {
    with_overlay_controller(app, |controller| {
        controller.desired_settings = desired_settings;
        controller.desired_state = desired_state.clone();
        if matches!(desired_state, OverlayState::Hidden) && !controller.tts_stop_visible {
            controller.last_level = 0.0;
        }
        if !matches!(desired_state, OverlayState::Recording) {
            controller.last_level = 0.0;
        }
    });
}

/// Pre-warms the overlay window at app startup so the WebView2 JS runtime is
/// fully loaded before the first recording begins. Eliminates the race condition
/// where window.eval() calls silently fail because JS hasn't loaded yet.
/// Called once after prime_overlay_controller() during app setup.
pub fn preload_overlay_window(app: &AppHandle) {
    schedule_overlay_window_creation(app, "preload");
}

/// Called when the overlay webview signals readiness.
/// Settings, state and last level are replayed from the cached desired state.
pub fn mark_overlay_ready(app: &AppHandle) {
    mark_overlay_heartbeat(app);
    let _ = ensure_overlay_window(app, "ready");
}

pub fn idle_overlay_state_for_settings(settings: &Settings) -> OverlayState {
    if settings.capture_enabled {
        OverlayState::Armed
    } else {
        OverlayState::Hidden
    }
}

pub fn emit_capture_idle_overlay(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let _ = app.emit("capture:state", "idle");
    update_overlay_state(app, idle_overlay_state_for_settings(settings))
}

/// Creates and configures the overlay window for recording status
pub fn create_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    // Return existing window if already created
    if let Some(existing) = app.get_webview_window("overlay") {
        return Ok(existing);
    }

    // Cooldown after hard create failure to prevent tight retry loops.
    if overlay_create_cooldown_active() {
        return Err("Overlay create retry is cooling down".to_string());
    }

    let window = match tauri::WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("overlay.html".into()),
    )
    .title("Trispr Flow Overlay")
    .inner_size(64.0, 64.0)
    .resizable(false)
    .decorations(false)
    .shadow(false)
    .transparent(true)
    .focusable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    {
        Ok(window) => window,
        Err(primary_error) => {
            // WebView2 can reject transparent/overlay-style windows on some systems.
            // Retry once with a conservative non-transparent config before disabling overlay.
            warn!(
                "Overlay primary window create failed (transparent mode): {}. Retrying safe fallback.",
                primary_error
            );
            match tauri::WebviewWindowBuilder::new(
                app,
                "overlay",
                WebviewUrl::App("overlay.html".into()),
            )
            .title("Trispr Flow Overlay")
            .inner_size(64.0, 64.0)
            .resizable(false)
            .decorations(false)
            .shadow(false)
            .transparent(false)
            .focusable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()
            {
                Ok(window) => {
                    warn!("Overlay created via safe fallback (non-transparent mode).");
                    window
                }
                Err(fallback_error) => {
                    let msg = format!(
                        "Failed to create overlay window. primary='{}' fallback='{}'",
                        primary_error, fallback_error
                    );
                    warn!("{} — scheduling bounded overlay retry cooldown", msg);
                    set_overlay_create_cooldown(OVERLAY_CREATE_COOLDOWN_MS);
                    return Err(msg);
                }
            }
        }
    };

    // Park the window off-screen until apply_overlay_settings repositions it.
    // Previously (12, 12) caused a "ghost overlay" in the upper-left corner when
    // apply_overlay_settings failed or ran late.
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
        x: -9999.0,
        y: -9999.0,
    }));

    let _ = window.set_ignore_cursor_events(true);

    // Prevent closing - hide instead
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window("overlay") {
                let _ = window.hide();
            }
        }
    });

    // Re-anchor overlay when DPI scale changes (monitor switch, display settings change).
    // ScaleFactorChanged fires per-window when it crosses into a different DPI zone.
    let app_handle_dpi = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::ScaleFactorChanged { .. } = event {
            let desired = app_handle_dpi
                .state::<AppState>()
                .overlay_controller
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .desired_settings
                .clone();
            if let Some(settings) = desired {
                let _ = apply_overlay_settings(&app_handle_dpi, &settings);
            }
        }
    });

    Ok(window)
}

fn window_position_invalid(window: &WebviewWindow) -> bool {
    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return false;
    };

    let outer_pos = match window.outer_position() {
        Ok(pos) => pos,
        Err(_) => return false,
    };
    let outer_size = match window.outer_size() {
        Ok(size) => size,
        Err(_) => return false,
    };

    let scale = monitor.scale_factor();
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();

    let monitor_x = monitor_pos.x as f64 / scale;
    let monitor_y = monitor_pos.y as f64 / scale;
    let monitor_w = monitor_size.width as f64 / scale;
    let monitor_h = monitor_size.height as f64 / scale;

    let window_x = outer_pos.x as f64;
    let window_y = outer_pos.y as f64;
    let window_w = outer_size.width as f64;
    let window_h = outer_size.height as f64;

    let overlap_w = (window_x + window_w).min(monitor_x + monitor_w) - window_x.max(monitor_x);
    let overlap_h = (window_y + window_h).min(monitor_y + monitor_h) - window_y.max(monitor_y);
    overlap_w < 4.0 || overlap_h < 4.0
}

fn fallback_overlay_to_safe_anchor(window: &WebviewWindow) -> Result<(), String> {
    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return Ok(());
    };

    let scale = monitor.scale_factor();
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();
    let monitor_w = monitor_size.width as f64 / scale;
    let monitor_h = monitor_size.height as f64 / scale;
    let origin_x = monitor_pos.x as f64 / scale;
    let origin_y = monitor_pos.y as f64 / scale;

    let outer_size = window.outer_size().ok();
    let width = outer_size
        .map(|size| size.width as f64)
        .unwrap_or(64.0)
        .max(32.0);
    let height = outer_size
        .map(|size| size.height as f64)
        .unwrap_or(64.0)
        .max(32.0);

    let pos_x = origin_x + monitor_w * 0.5 - width * 0.5;
    let pos_y = origin_y + monitor_h * 0.5 - height * 0.5;
    window
        .set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: pos_x,
            y: pos_y,
        }))
        .map_err(|e| format!("Failed to fallback overlay position to safe anchor: {}", e))
}

fn apply_overlay_state_to_window(
    app: &AppHandle,
    window: &WebviewWindow,
    state: OverlayState,
) -> Result<(), String> {
    let controller = overlay_controller_snapshot(app);
    let state_clone = state.clone();

    // Emit state event to overlay window
    let _ = window.emit("overlay:state", &state_clone);
    let _ = app.emit("overlay:state", &state_clone);

    let should_show = !matches!(state_clone, OverlayState::Hidden) || controller.tts_stop_visible;
    if should_show {
        // Defensive: if the window is still parked off-screen (apply_overlay_settings
        // failed or hasn't run yet), re-apply cached settings before showing.
        let still_offscreen = window
            .outer_position()
            .map(|pos| pos.x < -5000 || pos.y < -5000)
            .unwrap_or(false);
        if still_offscreen {
            let controller = overlay_controller_snapshot(app);
            if let Some(ref settings) = controller.desired_settings {
                let _ = apply_overlay_settings_to_window(window, settings);
            }
        }
        if window_position_invalid(window) {
            let controller = overlay_controller_snapshot(app);
            if let Some(ref settings) = controller.desired_settings {
                let _ = apply_overlay_settings_to_window(window, settings);
            }
            if window_position_invalid(window) {
                let _ = fallback_overlay_to_safe_anchor(window);
            }
        }
        window
            .show()
            .map_err(|e| format!("Failed to show overlay: {}", e))?;
        let _ = window.set_always_on_top(true);
    } else {
        let _ = window.hide();
    }
    let _ = window.set_ignore_cursor_events(!controller.tts_stop_visible);

    let js = overlay_state_eval_js(&state_clone);
    let _ = window.eval(&js);

    // Re-emit after a short delay to ensure the overlay webview is ready.
    // Uses the managed tokio blocking-thread pool instead of spawning a
    // dedicated OS thread for a trivial 120 ms sleep.
    let app_handle = app.clone();
    let state_clone = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(120));
        let _ = app_handle.emit("overlay:state", &state_clone);
    });

    Ok(())
}

fn apply_overlay_refining_to_window(
    app: &AppHandle,
    window: &WebviewWindow,
    active: bool,
) -> Result<(), String> {
    let desired_state = overlay_controller_snapshot(app).desired_state;

    if active || !matches!(desired_state, OverlayState::Hidden) {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
    }

    let _ = window.emit("overlay:refining", active);
    let _ = app.emit("overlay:refining", active);

    let active_str = if active { "true" } else { "false" };
    let js = format!(
        "if(window.setOverlayRefining){{window.setOverlayRefining({});}}",
        active_str
    );
    window
        .eval(&js)
        .map_err(|e| format!("Failed to update overlay refining indicator: {}", e))?;
    Ok(())
}

fn apply_overlay_tts_stop_to_window(
    app: &AppHandle,
    window: &WebviewWindow,
    active: bool,
    settings: Option<&OverlaySettings>,
) -> Result<(), String> {
    let effective_active = settings
        .map(|value| active && value.tts_stop_enabled)
        .unwrap_or(active);
    with_overlay_controller(app, |controller| {
        controller.tts_stop_visible = effective_active;
    });

    if let Some(settings) = settings {
        let should_show = effective_active
            || !matches!(
                overlay_controller_snapshot(app).desired_state,
                OverlayState::Hidden
            );
        if should_show {
            let _ = window.show();
            let _ = window.set_always_on_top(true);
        } else {
            let _ = window.hide();
        }
        let _ = window.set_ignore_cursor_events(!effective_active);
        let js = format!(
            "if(window.setOverlayTtsStopVisible){{window.setOverlayTtsStopVisible({}, {}, {});}}",
            if effective_active { "true" } else { "false" },
            if settings.tts_stop_enabled {
                "true"
            } else {
                "false"
            },
            serde_json::to_string(&settings.tts_stop_shape)
                .unwrap_or_else(|_| "\"compact\"".to_string())
        );
        window
            .eval(&js)
            .map_err(|e| format!("Failed to update overlay TTS stop visibility: {}", e))?;
    }
    Ok(())
}

fn apply_overlay_level_to_window(window: &WebviewWindow, level: f64) -> Result<(), String> {
    let clamped = level.clamp(0.0, 1.0);
    let js = format!(
        "if(window.setOverlayLevel){{window.setOverlayLevel({});}}",
        clamped
    );
    window
        .eval(&js)
        .map_err(|e| format!("Failed to update overlay level: {}", e))?;
    Ok(())
}

fn replay_overlay_controller_to_window(
    app: &AppHandle,
    window: &WebviewWindow,
    controller: &OverlayController,
) -> Result<(), String> {
    if let Some(settings) = controller.desired_settings.as_ref() {
        apply_overlay_settings_to_window(window, settings)?;
    }
    apply_overlay_state_to_window(app, window, controller.desired_state.clone())?;
    apply_overlay_refining_to_window(app, window, controller.refining_active)?;
    apply_overlay_tts_stop_to_window(
        app,
        window,
        controller.tts_stop_visible,
        controller.desired_settings.as_ref(),
    )?;
    let replay_level = if matches!(controller.desired_state, OverlayState::Recording) {
        controller.last_level
    } else {
        0.0
    };
    apply_overlay_level_to_window(window, replay_level)?;
    Ok(())
}

pub fn ensure_overlay_window(app: &AppHandle, reason: &str) -> Result<WebviewWindow, String> {
    let mut last_error = None;
    for attempt in 1..=OVERLAY_RECOVERY_MAX_ATTEMPTS {
        let controller = overlay_controller_snapshot(app);
        let stale_heartbeat = overlay_heartbeat_stale(&controller);
        if stale_heartbeat {
            let next_attempt = controller.recovery_attempt.saturating_add(1);
            emit_overlay_health(
                app,
                "recovering",
                next_attempt,
                "Overlay heartbeat stale; replaying state",
            );
        }
        let window = match app.get_webview_window("overlay") {
            Some(existing) => existing,
            None => match create_overlay_window(app) {
                Ok(window) => window,
                Err(err) => {
                    warn!(
                        "Overlay create attempt {} failed during {}: {}",
                        attempt, reason, err
                    );
                    last_error = Some(err.clone());
                    let recovery_attempt = with_overlay_controller(app, |cached| {
                        cached.recovery_attempt = cached.recovery_attempt.saturating_add(1);
                        cached.recovery_attempt
                    });
                    if recovery_attempt >= OVERLAY_RECOVERY_MAX_ATTEMPTS {
                        emit_overlay_health(app, "failed", recovery_attempt, err.clone());
                        set_overlay_create_cooldown(OVERLAY_CREATE_COOLDOWN_MS);
                    } else {
                        emit_overlay_health(app, "recovering", recovery_attempt, err.clone());
                        std::thread::sleep(Duration::from_millis(
                            OVERLAY_RECOVERY_BACKOFF_MS * attempt as u64,
                        ));
                    }
                    continue;
                }
            },
        };
        match replay_overlay_controller_to_window(app, &window, &controller) {
            Ok(()) => {
                if controller.recovery_attempt > 0 {
                    let recovered_attempt = with_overlay_controller(app, |cached| {
                        let previous = cached.recovery_attempt;
                        cached.recovery_attempt = 0;
                        previous
                    });
                    emit_overlay_health(
                        app,
                        "recovered",
                        recovered_attempt,
                        "Overlay recovered and synchronized",
                    );
                } else if stale_heartbeat {
                    emit_overlay_health(app, "recovered", 0, "Overlay synchronized");
                }
                return Ok(window);
            }
            Err(err) => {
                warn!(
                    "Overlay replay attempt {} failed during {}: {}",
                    attempt, reason, err
                );
                last_error = Some(err.clone());
                let recovery_attempt = with_overlay_controller(app, |cached| {
                    cached.recovery_attempt = cached.recovery_attempt.saturating_add(1);
                    cached.recovery_attempt
                });
                if recovery_attempt >= OVERLAY_RECOVERY_MAX_ATTEMPTS {
                    emit_overlay_health(app, "failed", recovery_attempt, err.clone());
                    set_overlay_create_cooldown(OVERLAY_CREATE_COOLDOWN_MS);
                } else {
                    emit_overlay_health(app, "recovering", recovery_attempt, err.clone());
                    std::thread::sleep(Duration::from_millis(
                        OVERLAY_RECOVERY_BACKOFF_MS * attempt as u64,
                    ));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "Overlay recovery failed".to_string()))
}

fn schedule_overlay_window_creation(app: &AppHandle, reason: &str) {
    if overlay_create_cooldown_active() {
        return;
    }
    if app.get_webview_window("overlay").is_some() {
        return;
    }
    if OVERLAY_CREATE_IN_FLIGHT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let app_handle = app.clone();
    let reason_owned = reason.to_string();
    crate::util::spawn_guarded("overlay_create_async", move || {
        struct InFlightReset;
        impl Drop for InFlightReset {
            fn drop(&mut self) {
                OVERLAY_CREATE_IN_FLIGHT.store(false, Ordering::Release);
            }
        }
        let _in_flight_reset = InFlightReset;

        if let Err(err) = ensure_overlay_window(&app_handle, &reason_owned) {
            warn!(
                "Async overlay creation failed during {}: {}",
                reason_owned, err
            );
        }
    });
}

/// Updates the overlay state and shows/hides it accordingly
pub fn update_overlay_state(app: &AppHandle, state: OverlayState) -> Result<(), String> {
    with_overlay_controller(app, |controller| {
        controller.desired_state = state.clone();
        if !matches!(state, OverlayState::Recording) {
            controller.last_level = 0.0;
        }
    });
    let Some(window) = app.get_webview_window("overlay") else {
        if matches!(state, OverlayState::Recording | OverlayState::Transcribing) {
            schedule_overlay_window_creation(app, "state_update");
        }
        return Ok(());
    };
    apply_overlay_state_to_window(app, &window, state)
}

pub fn update_overlay_tts_stop_visibility(app: &AppHandle, active: bool) -> Result<(), String> {
    let controller = overlay_controller_snapshot(app);
    let effective_active = active
        && controller
            .desired_settings
            .as_ref()
            .map(|settings| settings.tts_stop_enabled)
            .unwrap_or(true);
    with_overlay_controller(app, |controller| {
        controller.tts_stop_visible = effective_active;
    });
    let Some(window) = app.get_webview_window("overlay") else {
        if effective_active {
            schedule_overlay_window_creation(app, "tts_stop_update");
        }
        return Ok(());
    };
    let controller = overlay_controller_snapshot(app);
    let should_show = effective_active || !matches!(controller.desired_state, OverlayState::Hidden);
    if should_show {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
    } else {
        let _ = window.hide();
    }
    let _ = window.set_ignore_cursor_events(!effective_active);
    let js = format!(
        "if(window.setOverlayTtsStopVisible){{window.setOverlayTtsStopVisible({}, {}, {});}}",
        if effective_active { "true" } else { "false" },
        if let Some(settings) = controller.desired_settings.as_ref() {
            if settings.tts_stop_enabled {
                "true"
            } else {
                "false"
            }
        } else {
            "false"
        },
        if let Some(settings) = controller.desired_settings.as_ref() {
            serde_json::to_string(&settings.tts_stop_shape)
                .unwrap_or_else(|_| "\"compact\"".to_string())
        } else {
            "\"compact\"".to_string()
        }
    );
    window
        .eval(&js)
        .map_err(|e| format!("Failed to update overlay TTS stop visibility: {}", e))
}

pub fn update_overlay_refining_indicator(app: &AppHandle, active: bool) -> Result<(), String> {
    with_overlay_controller(app, |controller| {
        controller.refining_active = active;
    });
    let Some(window) = app.get_webview_window("overlay") else {
        if active {
            schedule_overlay_window_creation(app, "refining_update");
        }
        return Ok(());
    };
    apply_overlay_refining_to_window(app, &window, active)
}

pub fn sync_overlay_level(app: &AppHandle, level: f64) -> Result<(), String> {
    let desired_state = with_overlay_controller(app, |controller| {
        if matches!(controller.desired_state, OverlayState::Recording) {
            controller.last_level = level.clamp(0.0, 1.0);
        } else {
            controller.last_level = 0.0;
        }
        controller.desired_state.clone()
    });

    let Some(window) = app.get_webview_window("overlay") else {
        if matches!(desired_state, OverlayState::Recording) {
            schedule_overlay_window_creation(app, "level_update");
        }
        return Ok(());
    };

    if !matches!(desired_state, OverlayState::Recording) {
        if matches!(desired_state, OverlayState::Hidden) {
            return Ok(());
        }
        return apply_overlay_level_to_window(&window, 0.0);
    }
    if matches!(desired_state, OverlayState::Hidden) {
        return Ok(());
    }
    apply_overlay_level_to_window(&window, level)
}

fn resolve_overlay_position(
    window: &WebviewWindow,
    settings: &OverlaySettings,
    width: f64,
    height: f64,
) -> (f64, f64) {
    // pos_x and pos_y are stored as percentages (0-100)
    // Convert to absolute monitor coordinates, then to window position

    if let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    {
        let scale = monitor.scale_factor();
        let size_px = monitor.size();
        let pos_px = monitor.position();

        // Monitor dimensions in logical pixels
        let monitor_width = size_px.width as f64 / scale;
        let monitor_height = size_px.height as f64 / scale;
        let origin_x = pos_px.x as f64 / scale;
        let origin_y = pos_px.y as f64 / scale;

        // Convert percentage (0-100) to absolute screen coordinate
        let percent_x = settings.pos_x.max(0.0).min(100.0);
        let percent_y = settings.pos_y.max(0.0).min(100.0);

        let anchor_x = origin_x + (monitor_width * percent_x / 100.0);
        let anchor_y = origin_y + (monitor_height * percent_y / 100.0);

        // Position window so its center is at the anchor point
        let pos_x = anchor_x - width * 0.5;
        let pos_y = anchor_y - height * 0.5;
        (pos_x, pos_y)
    } else {
        // Fallback if monitor info unavailable (shouldn't happen)
        (0.0, 0.0)
    }
}

fn current_monitor_logical_size(window: &WebviewWindow) -> Option<(f64, f64)> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let size_px = monitor.size();
    Some((size_px.width as f64 / scale, size_px.height as f64 / scale))
}

fn resolve_effective_dimensions(
    window: &WebviewWindow,
    settings: &OverlaySettings,
) -> (f64, f64, f64, f64, f64) {
    // Hard cap: overlays may consume at most 50% of the display.
    let (monitor_width, monitor_height) =
        current_monitor_logical_size(window).unwrap_or((1920.0, 1080.0));
    let kitt_width_cap = (monitor_width * 0.5).max(50.0);
    let dot_radius_cap = (monitor_width.min(monitor_height) * 0.25).max(8.0); // 50% diameter

    let mut kitt_min_width = settings.kitt_min_width.max(4.0);
    let mut kitt_max_width = settings.kitt_max_width.max(50.0).min(kitt_width_cap);
    if kitt_max_width < 50.0 {
        kitt_max_width = 50.0;
    }
    if kitt_min_width > kitt_max_width {
        kitt_min_width = kitt_max_width;
    }

    let mut min_radius = settings.min_radius.max(4.0);
    let mut max_radius = settings.max_radius.max(8.0).min(dot_radius_cap);
    if max_radius < 8.0 {
        max_radius = 8.0;
    }
    if min_radius > max_radius {
        min_radius = max_radius;
    }

    let kitt_height = settings.kitt_height.max(8.0).min(400.0);

    (
        min_radius,
        max_radius,
        kitt_min_width,
        kitt_max_width,
        kitt_height,
    )
}

fn apply_overlay_settings_to_window(
    window: &WebviewWindow,
    settings: &OverlaySettings,
) -> Result<(), String> {
    let (
        effective_min_radius,
        effective_max_radius,
        effective_kitt_min_width,
        effective_kitt_max_width,
        effective_kitt_height,
    ) = resolve_effective_dimensions(&window, settings);

    // Calculate window size based on style
    // Add extra height for transcribe indicator positioned above the main element
    let (width, height) = if settings.style == "kitt" {
        let w = effective_kitt_max_width
            .max(effective_kitt_min_width)
            .max(50.0)
            + 32.0;
        let h = effective_kitt_height.max(8.0) + 32.0 + 18.0; // +18px for transcribe indicator
        (w, h)
    } else {
        let max_radius = effective_max_radius.max(effective_min_radius).max(4.0);
        let size = (max_radius * 2.0 + 96.0 + 20.0).max(64.0); // +20px for transcribe indicator
        (size, size)
    };

    window
        .set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }))
        .map_err(|e| format!("Failed to set overlay size: {}", e))?;
    let (pos_x, pos_y) = resolve_overlay_position(&window, settings, width, height);
    window
        .set_position(tauri::Position::Logical(tauri::LogicalPosition {
            x: pos_x,
            y: pos_y,
        }))
        .map_err(|e| format!("Failed to set overlay position: {}", e))?;

    // Update overlay via JS functions
    let js = format!(
        "if(window.setOverlayColor){{window.setOverlayColor('{}');}}if(window.setOverlayOpacity){{window.setOverlayOpacity({},{});}}if(window.setOverlayStyle){{window.setOverlayStyle('{}');}}if(window.setOverlayRefiningEnabled){{window.setOverlayRefiningEnabled({});}}if(window.setOverlayRefiningPreset){{window.setOverlayRefiningPreset('{}');}}if(window.setOverlayRefiningAppearance){{window.setOverlayRefiningAppearance('{}',{},{});}}if(window.setKittDimensions){{window.setKittDimensions({},{},{});}}if(window.setDotDimensions){{window.setDotDimensions({},{});}}",
        settings.color,
        settings.opacity_active,
        settings.opacity_inactive,
        settings.style,
        if settings.refining_indicator_enabled { "true" } else { "false" },
        settings.refining_indicator_preset,
        settings.refining_indicator_color,
        settings.refining_indicator_speed_ms,
        settings.refining_indicator_range,
        effective_kitt_min_width,
        effective_kitt_max_width,
        effective_kitt_height,
        effective_min_radius,
        effective_max_radius
    );
    window
        .eval(&js)
        .map_err(|e| format!("Failed to apply overlay settings: {}", e))?;
    let tts_shape = serde_json::to_string(&settings.tts_stop_shape)
        .unwrap_or_else(|_| "\"compact\"".to_string());
    let tts_color = serde_json::to_string(&settings.tts_stop_color)
        .unwrap_or_else(|_| "\"#4be0d4\"".to_string());
    let tts_js = format!(
        "if(window.setOverlayTtsStopConfig){{window.setOverlayTtsStopConfig({},{},{});}}",
        if settings.tts_stop_enabled {
            "true"
        } else {
            "false"
        },
        tts_shape,
        tts_color
    );
    window
        .eval(&tts_js)
        .map_err(|e| format!("Failed to apply overlay TTS stop config: {}", e))?;

    Ok(())
}

/// Applies overlay settings by resizing/repositioning the window and updating
/// the frontend via window.eval(). This is the primary settings application path.
///
/// Win32 safety: window.set_size(), set_position(), and eval() dispatch via the
/// Win32 message queue. Calling them from a background thread uses SendMessage
/// which re-enters the event loop and triggers tao warnings/freeze. We queue all
/// window operations onto the main thread via run_on_main_thread() instead.
pub fn apply_overlay_settings(app: &AppHandle, settings: &OverlaySettings) -> Result<(), String> {
    with_overlay_controller(app, |controller| {
        controller.desired_settings = Some(settings.clone());
    });
    if app.get_webview_window("overlay").is_none() {
        return Ok(());
    }
    let window = ensure_overlay_window(app, "settings_update")?;
    let settings_clone = settings.clone();
    app.run_on_main_thread(move || {
        let _ = apply_overlay_settings_to_window(&window, &settings_clone);
    })
    .map_err(|e| format!("apply_overlay_settings: run_on_main_thread failed: {:?}", e))?;

    // Backup path for webview-load races: deliver settings via events too.
    let _ = app.emit("overlay:settings", settings.clone());
    let app_handle = app.clone();
    let delayed_settings = settings.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(180));
        let _ = app_handle.emit("overlay:settings", delayed_settings);
    });

    Ok(())
}

fn overlay_state_eval_js(state: &OverlayState) -> String {
    let state_str = match state {
        OverlayState::Hidden => "hidden",
        OverlayState::Armed => "armed",
        OverlayState::Recording => "recording",
        OverlayState::Transcribing => "transcribing",
    };
    if matches!(state, OverlayState::Recording) {
        format!(
            "if(window.setOverlayState){{window.setOverlayState('{}');}}",
            state_str
        )
    } else {
        format!(
            "if(window.setOverlayState){{window.setOverlayState('{}');}}if(window.setOverlayLevel){{window.setOverlayLevel(0);}}",
            state_str
        )
    }
}

/// Get current overlay position (for settings persistence)
#[allow(dead_code)]
pub fn get_overlay_position(app: &AppHandle) -> Option<(f64, f64)> {
    app.get_webview_window("overlay")
        .and_then(|w| w.outer_position().ok())
        .map(|pos| (pos.x as f64, pos.y as f64))
}

/// Set overlay position (from saved settings)
#[allow(dead_code)]
pub fn set_overlay_position(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;

    window
        .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
        .map_err(|e| format!("Failed to set overlay position: {}", e))
}
