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
    .inner_size(32.0, 32.0)
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

    // Position in top-left corner by default
    if let Ok(monitor) = window.current_monitor() {
        if let Some(_monitor) = monitor {
            // Position in top-left with 12px margin
            let x = 12.0;
            let y = 12.0;

            let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition {
                x,
                y,
            }));
        }
    }

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
    // Get or create overlay window
    let window = match app.get_webview_window("overlay") {
        Some(w) => w,
        None => create_overlay_window(app)?,
    };

    // Emit state directly to overlay window (broadcast as fallback)
    let _ = window.emit("overlay:state", &state);
    let _ = app.emit("overlay:state", &state);

    // Show or hide based on state
    match state {
        OverlayState::Idle => {
            window.hide().map_err(|e| format!("Failed to hide overlay: {}", e))?;
        }
        OverlayState::ToggleIdle | OverlayState::Recording | OverlayState::Transcribing => {
            window.show().map_err(|e| format!("Failed to show overlay: {}", e))?;
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
