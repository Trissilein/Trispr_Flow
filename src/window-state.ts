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
        console.error("Failed to save window state:", error);
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

    // Only track main and conversation windows
    if (window.label !== "main" && window.label !== "conversation") {
        return;
    }

    // Listen for move and resize events
    window.onMoved(() => debouncedSave());
    window.onResized(() => debouncedSave());
}
