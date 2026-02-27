use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WindowEvent};
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayState {
    Idle,
    ToggleIdle,
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
    pub kitt_min_width: f64,
    pub kitt_max_width: f64,
    pub kitt_height: f64,
}

/// Called when the overlay webview signals readiness.
/// Settings are applied via the overlay:ready listener in lib.rs.
pub fn mark_overlay_ready() {}

/// Creates and configures the overlay window for recording status
pub fn create_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    // Return existing window if already created
    if let Some(existing) = app.get_webview_window("overlay") {
        return Ok(existing);
    }

    let window = tauri::WebviewWindowBuilder::new(
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
    .visible(true)
    .build()
    .map_err(|e| format!("Failed to create overlay window: {}", e))?;

    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
        x: 12.0,
        y: 12.0,
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

    Ok(window)
}

/// Updates the overlay state and shows/hides it accordingly
pub fn update_overlay_state(app: &AppHandle, state: OverlayState) -> Result<(), String> {
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

    // Emit state event to overlay window
    let _ = window.emit("overlay:state", &state);
    let _ = app.emit("overlay:state", &state);

    // Keep overlay visible; visual state is handled by CSS opacity
    window.show().map_err(|e| format!("Failed to show overlay: {}", e))?;
    let _ = window.set_always_on_top(true);

    let state_str = match state {
        OverlayState::Idle => "idle",
        OverlayState::ToggleIdle => "idle",
        OverlayState::Recording => "recording",
        OverlayState::Transcribing => "transcribing",
    };
    let js = if matches!(state, OverlayState::Recording) {
        format!("if(window.setOverlayState){{window.setOverlayState('{}');}}", state_str)
    } else {
        format!(
            "if(window.setOverlayState){{window.setOverlayState('{}');}}if(window.setOverlayLevel){{window.setOverlayLevel(0);}}",
            state_str
        )
    };
    let _ = window.eval(&js);

    // Re-emit after a short delay to ensure the overlay webview is ready
    let app_handle = app.clone();
    let state_clone = state.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        let _ = app_handle.emit("overlay:state", &state_clone);
    });

    Ok(())
}

pub fn update_overlay_refining_indicator(app: &AppHandle, active: bool) -> Result<(), String> {
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

    let _ = window.emit("overlay:refining", active);
    let _ = app.emit("overlay:refining", active);

    let active_str = if active { "true" } else { "false" };
    let js = format!(
        "if(window.setOverlayRefining){{window.setOverlayRefining({});}}",
        active_str
    );
    let _ = window.eval(&js);
    Ok(())
}

fn resolve_overlay_position(window: &WebviewWindow, settings: &OverlaySettings, width: f64, height: f64) -> (f64, f64) {
    // pos_x and pos_y are stored as percentages (0-100)
    // Convert to absolute monitor coordinates, then to window position

    if let Some(monitor) = window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten()) {
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

/// Applies overlay settings by resizing/repositioning the window and updating
/// the frontend via window.eval(). This is the primary settings application path.
pub fn apply_overlay_settings(app: &AppHandle, settings: &OverlaySettings) -> Result<(), String> {
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

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
        let w = effective_kitt_max_width.max(effective_kitt_min_width).max(50.0) + 32.0;
        let h = effective_kitt_height.max(8.0) + 32.0 + 18.0;  // +18px for transcribe indicator
        (w, h)
    } else {
        let max_radius = effective_max_radius.max(effective_min_radius).max(4.0);
        let size = (max_radius * 2.0 + 96.0 + 20.0).max(64.0);  // +20px for transcribe indicator
        (size, size)
    };

    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width,
        height,
    }));
    let (pos_x, pos_y) = resolve_overlay_position(&window, settings, width, height);
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
        x: pos_x,
        y: pos_y,
    }));

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
    let _ = window.eval(&js);

    Ok(())
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
