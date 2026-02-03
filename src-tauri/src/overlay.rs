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
    pub pos_x: f64,
    pub pos_y: f64,
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
    .visible(false) // Start hidden
    .build()
    .map_err(|e| format!("Failed to create overlay window: {}", e))?;

    // Default position (will be overridden by overlay settings)
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

    // Show or hide based on state
    match state {
        OverlayState::Idle => {
            info!("Hiding overlay (Idle state)");
            window.hide().map_err(|e| {
                error!("Failed to hide overlay: {}", e);
                format!("Failed to hide overlay: {}", e)
            })?;
        }
        OverlayState::ToggleIdle | OverlayState::Recording | OverlayState::Transcribing => {
            info!("Showing overlay ({:?} state)", state);
            window.show().map_err(|e| {
                error!("Failed to show overlay: {}", e);
                format!("Failed to show overlay: {}", e)
            })?;
            let _ = window.set_always_on_top(true);
            info!("Overlay window.show() succeeded");
        }
    }

    // Re-emit after a short delay to ensure the overlay webview is ready.
    let app_handle = app.clone();
    let state_clone = state.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(120));
        let _ = app_handle.emit("overlay:state", &state_clone);
    });

    Ok(())
}

pub fn apply_overlay_settings(app: &AppHandle, settings: &OverlaySettings) -> Result<(), String> {
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

    let max_radius = settings.max_radius.max(settings.min_radius).max(4.0);
    let size = (max_radius * 2.0 + 32.0).max(32.0);
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: size,
        height: size,
    }));
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
        x: settings.pos_x,
        y: settings.pos_y,
    }));

    let _ = window.emit("overlay:settings", settings);
    let _ = app.emit("overlay:settings", settings);
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
