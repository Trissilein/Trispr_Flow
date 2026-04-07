// Hotkey recorder system

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ValidationResult } from "./types";
import { settings } from "./state";
import { persistSettings } from "./settings";
import { formatHotkeyForDisplay } from "./ui-helpers";

// Map event.code → tauri-compatible key name for layout-independent recognition
const CODE_TO_KEY: Record<string, string> = {
  // Punctuation / symbol keys
  IntlBackslash: "IntlBackslash", // < > on DE/ISO layout (next to left Shift)
  IntlRo: "IntlRo",               // JIS Ro key
  IntlYen: "IntlYen",             // JIS Yen key
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
  Semicolon: "Semicolon",
  Quote: "Quote",
  Backquote: "Backquote",
  Minus: "Minus",
  Equal: "Equal",
  Backslash: "Backslash",
  Slash: "Slash",
  Comma: "Comma",
  Period: "Period",
  // Navigation / editing
  Space: "Space",
  Enter: "Enter",
  Escape: "Escape",
  Backspace: "Backspace",
  Tab: "Tab",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  // Lock / system keys
  CapsLock: "CapsLock",
  NumLock: "NumLock",
  ScrollLock: "ScrollLock",
  Pause: "Pause",
  PrintScreen: "PrintScreen",
  // Numpad
  Numpad0: "Numpad0",
  Numpad1: "Numpad1",
  Numpad2: "Numpad2",
  Numpad3: "Numpad3",
  Numpad4: "Numpad4",
  Numpad5: "Numpad5",
  Numpad6: "Numpad6",
  Numpad7: "Numpad7",
  Numpad8: "Numpad8",
  Numpad9: "Numpad9",
  NumpadAdd: "NumpadAdd",
  NumpadDecimal: "NumpadDecimal",
  NumpadDivide: "NumpadDivide",
  NumpadEnter: "NumpadEnter",
  NumpadMultiply: "NumpadMultiply",
  NumpadSubtract: "NumpadSubtract",
  // Media keys
  MediaPlayPause: "MediaPlayPause",
  MediaStop: "MediaStop",
  MediaTrackNext: "MediaTrackNext",
  MediaTrackPrevious: "MediaTrackPrevious",
  AudioVolumeUp: "AudioVolumeUp",
  AudioVolumeDown: "AudioVolumeDown",
  AudioVolumeMute: "AudioVolumeMute",
};

// Track registration status per hotkey type
const registrationStatus: Record<string, { registered: boolean; error?: string }> = {};

/** Listen for backend hotkey registration results and update status badges */
export function initHotkeyStatusListener(): void {
  listen<Record<string, { key: string; registered: boolean; error?: string | null }>>(
    "hotkey:registration-status",
    (event) => {
      for (const [type, status] of Object.entries(event.payload)) {
        registrationStatus[type] = {
          registered: status.registered,
          error: status.error ?? undefined,
        };
        // Update badge in DOM if present
        const badge = document.querySelector(`.hotkey-reg-badge[data-hotkey-type="${type}"]`);
        if (badge) {
          if (!status.registered && status.error) {
            badge.textContent = "⚠ Conflict";
            badge.className = "hotkey-reg-badge conflict";
            badge.setAttribute("title", status.error);
          } else {
            badge.textContent = "✓";
            badge.className = "hotkey-reg-badge ok";
            badge.setAttribute("title", "Hotkey registered");
          }
        }
      }
    }
  );
}

export function getHotkeyRegistrationStatus(type: string): { registered: boolean; error?: string } {
  return registrationStatus[type] ?? { registered: true };
}

export function setupHotkeyRecorder(
  type: "ptt" | "toggle" | "transcribe" | "toggleActivationWords" | "productModeToggle" | "ttsStop",
  input: HTMLInputElement | null,
  recordBtn: HTMLButtonElement | null,
  statusEl: HTMLSpanElement | null
) {
  if (!input || !recordBtn || !statusEl) return;

  let isRecording = false;
  let recordedKeys: Set<string> = new Set();
  let finalizeTimeout: number | null = null;

  const updateStatus = (message: string, type: "success" | "error" | "info") => {
    statusEl.textContent = message;
    statusEl.className = `hotkey-status ${type}`;
  };

  const validateHotkey = async (hotkey: string) => {
    try {
      const result = await invoke<ValidationResult>("validate_hotkey", { key: hotkey });

      if (result.valid) {
        input.classList.remove("invalid");
        input.classList.add("valid");
        updateStatus("✓ Valid hotkey", "success");
        return true;
      } else {
        input.classList.remove("valid");
        input.classList.add("invalid");
        updateStatus(result.error || "Invalid hotkey", "error");
        return false;
      }
    } catch (error) {
      input.classList.remove("valid");
      input.classList.add("invalid");
      updateStatus(`Error: ${error}`, "error");
      return false;
    }
  };

  const stopRecording = () => {
    isRecording = false;
    recordBtn.textContent = "🎹 Record";
    recordBtn.classList.remove("recording");
    input.classList.remove("recording");
    document.removeEventListener("keydown", handleKeyDown);
    document.removeEventListener("keyup", handleKeyUp);

    // Clear any pending finalization
    if (finalizeTimeout !== null) {
      clearTimeout(finalizeTimeout);
      finalizeTimeout = null;
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    e.preventDefault();

    // Add modifiers
    if (e.ctrlKey) recordedKeys.add("Ctrl");
    if (e.shiftKey) recordedKeys.add("Shift");
    if (e.altKey) recordedKeys.add("Alt");
    if (e.metaKey) recordedKeys.add("Command");

    // Add the actual key — use event.code for layout-independent key detection
    const isModifier = ["Control", "Shift", "Alt", "Meta"].includes(e.key);
    if (!isModifier) {
      let keyName: string;

      // Check code-to-key mapping first (handles special/punctuation keys like IntlBackslash)
      if (CODE_TO_KEY[e.code]) {
        keyName = CODE_TO_KEY[e.code];
      } else if (e.code.startsWith("Key")) {
        // Use event.key for letter keys so DE layout (Z/Y swap) is respected.
        // e.key gives the actual character the user typed, e.g. "z" on DE when
        // the physical key is where US-Y is. parse_key() accepts single letters.
        keyName = e.key.length === 1 ? e.key.toUpperCase() : e.code.slice(3);
      } else if (e.code.startsWith("Digit")) {
        // Digit0 → 0, Digit9 → 9
        keyName = e.code.slice(5);
      } else if (e.code.startsWith("Numpad")) {
        keyName = e.code; // NumpadEnter, Numpad0, etc.
      } else if (e.code.startsWith("F") && /^F\d+$/.test(e.code)) {
        keyName = e.code; // F1-F24
      } else {
        // Fallback to e.key, uppercased for single chars
        keyName = e.key.length === 1 ? e.key.toUpperCase() : e.key;
      }

      recordedKeys.add(keyName);
    }

    // Display current combination with human-readable labels
    const keysArray = Array.from(recordedKeys);
    const hotkeyString = keysArray.join("+");
    input.value = formatHotkeyForDisplay(hotkeyString);
  };

  const finalizeHotkey = async () => {
    if (recordedKeys.size < 2) {
      updateStatus("Need at least modifier + key", "error");
      return;
    }

    stopRecording();

    const hotkeyString = Array.from(recordedKeys).join("+");

    // Validate
    const isValid = await validateHotkey(hotkeyString);

    if (isValid && settings) {
      if (type === "ptt") {
        settings.hotkey_ptt = hotkeyString;
      } else if (type === "toggle") {
        settings.hotkey_toggle = hotkeyString;
      } else if (type === "transcribe") {
        settings.transcribe_hotkey = hotkeyString;
      } else if (type === "toggleActivationWords") {
        settings.hotkey_toggle_activation_words = hotkeyString;
      } else if (type === "productModeToggle") {
        settings.hotkey_product_mode_toggle = hotkeyString;
      } else if (type === "ttsStop") {
        settings.hotkey_tts_stop = hotkeyString;
      }
      await persistSettings();
    }

    recordedKeys.clear();
  };

  const handleKeyUp = async (_e: KeyboardEvent) => {
    // Clear any pending finalization
    if (finalizeTimeout !== null) {
      clearTimeout(finalizeTimeout);
    }

    // Wait 150ms for all keys to be released, then finalize
    // This handles both simultaneous and sequential key releases
    finalizeTimeout = window.setTimeout(async () => {
      // Check if all modifier keys are now released
      // Use a fresh keyboard state check instead of the event
      if (recordedKeys.size > 1) {
        await finalizeHotkey();
      }
      finalizeTimeout = null;
    }, 150);
  };

  // Record button click
  recordBtn.addEventListener("click", () => {
    if (isRecording) {
      stopRecording();
      updateStatus("Recording cancelled", "info");
    } else {
      isRecording = true;
      recordedKeys.clear();
      recordBtn.textContent = "⏺ Recording...";
      recordBtn.classList.add("recording");
      input.classList.add("recording");
      input.value = "";
      updateStatus("Press your key combination...", "info");

      document.addEventListener("keydown", handleKeyDown);
      document.addEventListener("keyup", handleKeyUp);
    }
  });

  // Initial validation
  if (input.value.trim()) {
    validateHotkey(input.value.trim());
  }
}
