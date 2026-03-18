import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

let saveTimeout: number | null = null;

async function saveWindowState() {
    const window = getCurrentWindow();
    try {
        const position = await window.outerPosition();
        const size = await window.outerSize();

        await invoke("save_window_state", {
            windowLabel: window.label,
            x: Math.round(position.x),
            y: Math.round(position.y),
            width: Math.round(size.width),
            height: Math.round(size.height),
        });
    } catch (error) {
        console.error(`Failed to save window state:`, error);
    }
}

async function saveWindowVisibility(visibility: "normal" | "minimized") {
    try {
        await invoke("save_window_visibility_state", { visibility });
    } catch (error) {
        console.error(`Failed to save window visibility:`, error);
    }
}

function debouncedSave() {
    if (saveTimeout !== null) {
        clearTimeout(saveTimeout);
    }
    saveTimeout = window.setTimeout(() => {
        saveWindowState();
        saveTimeout = null;
    }, 500);  // Save 500ms after last move/resize
}

export function initWindowStatePersistence() {
    const window = getCurrentWindow();

    // Only track main window
    if (window.label !== "main") {
        return;
    }

    // Listen for move and resize events
    const unlistenMoved = window.onMoved(() => {
        debouncedSave();
    });

    const unlistenResized = window.onResized(() => {
        debouncedSave();
    });

    // Track minimized state: save "minimized" when window is minimized,
    // "normal" when restored (tray state is saved on the Rust side)
    const unlistenFocus = window.onFocusChanged(async ({ payload: focused }) => {
        if (!focused) {
            // Check if window was minimized (not just lost focus)
            try {
                const minimized = await window.isMinimized();
                if (minimized) {
                    saveWindowVisibility("minimized");
                }
            } catch (_) { /* ignore */ }
        } else {
            // Window regained focus → it's in normal visible state
            saveWindowVisibility("normal");
        }
    });

    // Store unlisteners for potential cleanup
    return { unlistenMoved, unlistenResized, unlistenFocus };
}
