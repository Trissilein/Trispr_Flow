// Hotkey recorder system

import { invoke } from "@tauri-apps/api/core";
import type { ValidationResult } from "./types";
import { settings } from "./state";
import { persistSettings } from "./settings";

export function setupHotkeyRecorder(
  type: "ptt" | "toggle" | "transcribe",
  input: HTMLInputElement | null,
  recordBtn: HTMLButtonElement | null,
  statusEl: HTMLSpanElement | null
) {
  if (!input || !recordBtn || !statusEl) return;

  let isRecording = false;
  let recordedKeys: Set<string> = new Set();

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
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    e.preventDefault();

    // Add modifiers
    if (e.ctrlKey) recordedKeys.add("Ctrl");
    if (e.shiftKey) recordedKeys.add("Shift");
    if (e.altKey) recordedKeys.add("Alt");
    if (e.metaKey) recordedKeys.add("Command");

    // Add the actual key - use e.code for better reliability with special characters
    const isModifier = ["Control", "Shift", "Alt", "Meta"].includes(e.key);
    if (!isModifier) {
      // Use e.key for display (shows actual character like "^")
      // But handle special cases
      let keyName = e.key;

      // For single character keys, uppercase them
      if (keyName.length === 1) {
        keyName = keyName.toUpperCase();
      }

      recordedKeys.add(keyName);
    }

    // Display current combination
    const keysArray = Array.from(recordedKeys);
    const hotkeyString = keysArray.join("+");
    input.value = hotkeyString;
  };

  const handleKeyUp = async (e: KeyboardEvent) => {
    // When all keys are released, finalize the hotkey
    if (!e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey && recordedKeys.size > 1) {
      stopRecording();

      const hotkeyString = Array.from(recordedKeys).join("+");

      // Validate
      const isValid = await validateHotkey(hotkeyString);

      if (isValid && settings) {
        if (type === "ptt") {
          settings.hotkey_ptt = hotkeyString;
        } else if (type === "toggle") {
          settings.hotkey_toggle = hotkeyString;
        } else {
          settings.transcribe_hotkey = hotkeyString;
        }
        await persistSettings();
      }

      recordedKeys.clear();
    }
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
