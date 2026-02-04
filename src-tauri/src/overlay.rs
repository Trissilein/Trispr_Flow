use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WindowEvent};
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};

static OVERLAY_JS_READY: AtomicBool = AtomicBool::new(false);
static OVERLAY_RETRY_PENDING: AtomicBool = AtomicBool::new(false);

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
    pub kitt_min_width: f64,
    pub kitt_max_width: f64,
    pub kitt_height: f64,
}

pub fn mark_overlay_ready() {
    OVERLAY_JS_READY.store(true, Ordering::Relaxed);
}

/// Creates and configures the overlay window for recording status
pub fn create_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    use tracing::info;

    info!("create_overlay_window called");

    // Check if overlay already exists
    if let Some(existing) = app.get_webview_window("overlay") {
        info!("Overlay window already exists, returning existing");
        return Ok(existing);
    }

    info!("Creating new overlay window");

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

    // Default position (may be overridden by overlay settings)
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
        x: 12.0,
        y: 12.0,
    }));

    let _ = window.set_ignore_cursor_events(true);

    // Handle window events
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            // Prevent closing, just hide instead
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
    use tracing::{info, error, warn};

    info!("update_overlay_state called with state: {:?}", state);

    // Get or create overlay window
    let window = match app.get_webview_window("overlay") {
        Some(w) => {
            info!("Overlay window found");
            w
        }
        None => {
            warn!("Overlay window not found, creating new one");
            create_overlay_window(app)?
        }
    };

    info!("Emitting overlay state: {:?}", state);
    // Emit state directly to overlay window (broadcast as fallback)
    let _ = window.emit("overlay:state", &state);
    let _ = app.emit("overlay:state", &state);

    // Keep overlay visible; visual state is handled by CSS opacity.
    info!("Showing overlay ({:?} state)", state);
    window.show().map_err(|e| {
        error!("Failed to show overlay: {}", e);
        format!("Failed to show overlay: {}", e)
    })?;
    let _ = window.set_always_on_top(true);
    info!("Overlay window.show() succeeded");

    let state_str = match state {
        OverlayState::Idle => "idle",
        OverlayState::ToggleIdle => "idle",
        OverlayState::Recording => "recording",
        OverlayState::Transcribing => "transcribing",
    };
    // Call setOverlayState JS function (new simple overlay)
    let js = format!("if(window.setOverlayState){{window.setOverlayState('{}');}}", state_str);
    let _ = window.eval(&js);

    // Re-emit after a short delay to ensure the overlay webview is ready.
    let app_handle = app.clone();
    let state_clone = state.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        let _ = app_handle.emit("overlay:state", &state_clone);
    });

    Ok(())
}

fn resolve_overlay_position(window: &WebviewWindow, settings: &OverlaySettings, width: f64, height: f64) -> (f64, f64) {
    let mut anchor_x = settings.pos_x;
    let mut anchor_y = settings.pos_y;

    let is_default = anchor_x <= 16.0 && anchor_y <= 16.0;
    if is_default {
        if let Some(monitor) = window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten()) {
            let scale = monitor.scale_factor();
            let size_px = monitor.size();
            let pos_px = monitor.position();
            let width = size_px.width as f64 / scale;
            let height = size_px.height as f64 / scale;
            let origin_x = pos_px.x as f64 / scale;
            let origin_y = pos_px.y as f64 / scale;
            anchor_x = origin_x + width * 0.5;
            anchor_y = origin_y + height - 30.0;
        }
    }

    let pos_x = anchor_x - width * 0.5;
    let pos_y = anchor_y - height * 0.5;
    (pos_x, pos_y)
}

pub fn resolve_overlay_position_for_settings(app: &AppHandle, settings: &OverlaySettings) -> Option<(f64, f64)> {
    let window = app.get_webview_window("overlay")?;

    // Calculate size based on style
    let (width, height) = if settings.style == "kitt" {
        let w = settings.kitt_max_width.max(settings.kitt_min_width).max(50.0) + 32.0;
        let h = settings.kitt_height.max(8.0) + 32.0;
        (w, h)
    } else {
        let max_radius = settings.max_radius.max(settings.min_radius).max(4.0);
        let size = (max_radius * 2.0 + 96.0).max(64.0);
        (size, size)
    };

    let (pos_x, pos_y) = resolve_overlay_position(&window, settings, width, height);
    let center_x = pos_x + width * 0.5;
    let center_y = pos_y + height * 0.5;
    let changed = (center_x - settings.pos_x).abs() > 0.5 || (center_y - settings.pos_y).abs() > 0.5;
    if changed {
        Some((center_x, center_y))
    } else {
        None
    }
}

pub fn apply_overlay_settings(app: &AppHandle, settings: &OverlaySettings) -> Result<(), String> {
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

    // Calculate window size based on style
    let (width, height) = if settings.style == "kitt" {
        // KITT mode: rectangular window
        let w = settings.kitt_max_width.max(settings.kitt_min_width).max(50.0) + 32.0;
        let h = settings.kitt_height.max(8.0) + 32.0;
        (w, h)
    } else {
        // Dot mode: square window
        let max_radius = settings.max_radius.max(settings.min_radius).max(4.0);
        let size = (max_radius * 2.0 + 96.0).max(64.0);
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

    if !OVERLAY_JS_READY.load(Ordering::Relaxed)
        && !OVERLAY_RETRY_PENDING.swap(true, Ordering::Relaxed)
    {
        let app_handle = app.clone();
        let settings_clone = settings.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));
            OVERLAY_RETRY_PENDING.store(false, Ordering::Relaxed);
            let _ = apply_overlay_settings(&app_handle, &settings_clone);
        });
    }

    // Update overlay via JS functions
    let js = format!(
        "if(window.setOverlayColor){{window.setOverlayColor('{}');}}if(window.setOverlayOpacity){{window.setOverlayOpacity({},{});}}if(window.setOverlayStyle){{window.setOverlayStyle('{}');}}if(window.setKittDimensions){{window.setKittDimensions({},{},{});}}",
        settings.color,
        settings.opacity_active,
        settings.opacity_inactive,
        settings.style,
        settings.kitt_min_width,
        settings.kitt_max_width,
        settings.kitt_height
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
