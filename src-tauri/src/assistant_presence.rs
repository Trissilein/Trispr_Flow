use crate::modules::{
    canonicalize_module_id, ASSISTANT_CORE_MODULE_ID, ASSISTANT_PRESENCE_MODULE_ID,
};
use crate::state::{save_settings_file, AppState, Settings};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WindowEvent};

const ASSISTANT_PRESENCE_LABEL: &str = "assistant_presence";
const ASSISTANT_PRESENCE_MIN_WIDTH: f64 = 400.0;
const ASSISTANT_PRESENCE_MIN_HEIGHT: f64 = 280.0;
const ASSISTANT_PRESENCE_DEFAULT_WIDTH: f64 = 420.0;
const ASSISTANT_PRESENCE_DEFAULT_HEIGHT: f64 = 280.0;
const ASSISTANT_PRESENCE_GEOMETRY_SAVE_DEBOUNCE_MS: u64 = 500;

static LAST_GEOMETRY_SAVE_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    crate::util::now_ms()
}

fn module_enabled(settings: &Settings, module_id: &str) -> bool {
    let module_id = canonicalize_module_id(module_id);
    settings
        .module_settings
        .enabled_modules
        .iter()
        .any(|enabled| canonicalize_module_id(enabled) == module_id)
}

fn presence_should_be_visible(settings: &Settings) -> bool {
    settings.assistant_presence_enabled
        && settings.workflow_agent.enabled
        && settings
            .product_mode
            .trim()
            .eq_ignore_ascii_case("assistant")
        && module_enabled(settings, ASSISTANT_CORE_MODULE_ID)
        && module_enabled(settings, ASSISTANT_PRESENCE_MODULE_ID)
}

fn restore_presence_geometry(window: &WebviewWindow, settings: &Settings) {
    if let (Some(width), Some(height)) = (
        settings.assistant_presence_window_width,
        settings.assistant_presence_window_height,
    ) {
        let width = width.max(ASSISTANT_PRESENCE_MIN_WIDTH as u32);
        let height = height.max(ASSISTANT_PRESENCE_MIN_HEIGHT as u32);
        let _ = window.set_size(tauri::PhysicalSize::new(width, height));
    } else {
        let _ = window.set_size(tauri::LogicalSize::new(
            ASSISTANT_PRESENCE_DEFAULT_WIDTH,
            ASSISTANT_PRESENCE_DEFAULT_HEIGHT,
        ));
    }

    if let (Some(x), Some(y)) = (
        settings.assistant_presence_window_x,
        settings.assistant_presence_window_y,
    ) {
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        return;
    }

    if let Ok(Some(monitor)) = window
        .current_monitor()
        .or_else(|_| window.primary_monitor())
    {
        let size = monitor.size();
        let pos = monitor.position();
        let width = settings
            .assistant_presence_window_width
            .unwrap_or(ASSISTANT_PRESENCE_DEFAULT_WIDTH as u32) as i32;
        let x = pos.x + size.width as i32 - width - 48;
        let y = pos.y + 72;
        let _ = window.set_position(tauri::PhysicalPosition::new(x.max(pos.x), y));
    }
}

fn persist_presence_geometry(app: &AppHandle, window: &WebviewWindow) {
    let now = now_ms();
    let last = LAST_GEOMETRY_SAVE_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < ASSISTANT_PRESENCE_GEOMETRY_SAVE_DEBOUNCE_MS {
        return;
    }
    LAST_GEOMETRY_SAVE_MS.store(now, Ordering::Relaxed);

    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let monitor_name = window
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|monitor| monitor.name().map(|name| name.clone()));

    let state = app.state::<AppState>();
    let snapshot = {
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings.assistant_presence_window_x = Some(position.x);
        settings.assistant_presence_window_y = Some(position.y);
        settings.assistant_presence_window_width = Some(size.width);
        settings.assistant_presence_window_height = Some(size.height);
        settings.assistant_presence_window_monitor = monitor_name;
        settings.clone()
    };
    let _ = save_settings_file(app, &snapshot);
}

fn create_assistant_presence_window(
    app: &AppHandle,
    settings: &Settings,
) -> Result<WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(ASSISTANT_PRESENCE_LABEL) {
        return Ok(existing);
    }

    let window = tauri::WebviewWindowBuilder::new(
        app,
        ASSISTANT_PRESENCE_LABEL,
        WebviewUrl::App("assistant-presence.html".into()),
    )
    .title("Trispr Assistant")
    .inner_size(
        ASSISTANT_PRESENCE_DEFAULT_WIDTH,
        ASSISTANT_PRESENCE_DEFAULT_HEIGHT,
    )
    .min_inner_size(ASSISTANT_PRESENCE_MIN_WIDTH, ASSISTANT_PRESENCE_MIN_HEIGHT)
    .decorations(false)
    .transparent(false)
    .resizable(true)
    .always_on_top(settings.assistant_presence_pinned)
    .visible(false)
    .build()
    .map_err(|err| format!("Failed to create assistant presence window: {err}"))?;

    restore_presence_geometry(&window, settings);

    let app_handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window(ASSISTANT_PRESENCE_LABEL) {
                let _ = window.hide();
            }
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if let Some(window) = app_handle.get_webview_window(ASSISTANT_PRESENCE_LABEL) {
                persist_presence_geometry(&app_handle, &window);
            }
        }
        _ => {}
    });

    Ok(window)
}

pub fn show_assistant_presence_window(app: &AppHandle) -> Result<(), String> {
    let settings = {
        let state = app.state::<AppState>();
        let snapshot = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        snapshot
    };
    let window = create_assistant_presence_window(app, &settings)?;
    let _ = window.set_always_on_top(settings.assistant_presence_pinned);
    window
        .show()
        .map_err(|err| format!("Failed to show assistant presence window: {err}"))?;
    Ok(())
}

pub fn hide_assistant_presence_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(ASSISTANT_PRESENCE_LABEL) {
        let _ = window.hide();
    }
}

pub fn reconcile_assistant_presence_window(app: &AppHandle, settings: &Settings) {
    if !presence_should_be_visible(settings) {
        hide_assistant_presence_window(app);
        return;
    }

    let _ = show_assistant_presence_window(app);
}
