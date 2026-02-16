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

    console.log(`[window-state] Initializing for window: ${window.label}`);

    // Only track main window
    if (window.label !== "main") {
        console.log(`[window-state] Skipping - not a tracked window`);
        return;
    }

    console.log(`[window-state] Setting up event listeners for ${window.label}`);

    // Listen for move and resize events
    const unlistenMoved = window.onMoved(() => {
        console.log(`[window-state] Move event detected for ${window.label}`);
        debouncedSave();
    });

    const unlistenResized = window.onResized(() => {
        console.log(`[window-state] Resize event detected for ${window.label}`);
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
                    console.log(`[window-state] Window minimized`);
                    saveWindowVisibility("minimized");
                }
            } catch (_) { /* ignore */ }
        } else {
            // Window regained focus → it's in normal visible state
            saveWindowVisibility("normal");
        }
    });

    console.log(`[window-state] Event listeners registered for ${window.label}`);

    // Store unlisteners for potential cleanup
    return { unlistenMoved, unlistenResized, unlistenFocus };
}
