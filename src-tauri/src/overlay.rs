use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WindowEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayState {
    Idle,
    Recording,
    Transcribing,
}

/// Creates and configures the overlay window for recording status
pub fn create_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    // Check if overlay already exists
    if let Some(existing) = app.get_webview_window("overlay") {
        return Ok(existing);
    }

    let window = tauri::WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("overlay.html".into()),
    )
    .title("Trispr Flow Overlay")
    .inner_size(200.0, 80.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false) // Start hidden
    .build()
    .map_err(|e| format!("Failed to create overlay window: {}", e))?;

    // Position in top-right corner by default
    if let Ok(monitor) = window.current_monitor() {
        if let Some(monitor) = monitor {
            let size = monitor.size();
            let scale = monitor.scale_factor();

            // Position in top-right with 20px margin
            let x = (size.width as f64 / scale) - 220.0;
            let y = 20.0;

            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                x,
                y,
            }));
        }
    }

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
    // Get or create overlay window
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

    // Emit state to overlay
    app.emit("overlay:state", &state)
        .map_err(|e| format!("Failed to emit overlay state: {}", e))?;

    // Show or hide based on state
    match state {
        OverlayState::Idle => {
            window.hide().map_err(|e| format!("Failed to hide overlay: {}", e))?;
        }
        OverlayState::Recording | OverlayState::Transcribing => {
            window.show().map_err(|e| format!("Failed to show overlay: {}", e))?;
            window.set_focus().ok(); // Focus is optional
        }
    }

    Ok(())
}

/// Get current overlay position (for settings persistence)
pub fn get_overlay_position(app: &AppHandle) -> Option<(f64, f64)> {
    app.get_webview_window("overlay")
        .and_then(|w| w.outer_position().ok())
        .map(|pos| (pos.x as f64, pos.y as f64))
}

/// Set overlay position (from saved settings)
pub fn set_overlay_position(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;

    window
        .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
        .map_err(|e| format!("Failed to set overlay position: {}", e))
}
