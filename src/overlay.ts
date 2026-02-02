import { listen } from "@tauri-apps/api/event";

type OverlayState = "idle" | "recording" | "transcribing";

const container = document.getElementById("overlay-container");
const icon = document.getElementById("overlay-icon");
const text = document.getElementById("overlay-text");

// Listen for state changes from Rust backend
listen<OverlayState>("overlay:state", (event) => {
  updateOverlay(event.payload);
});

function updateOverlay(state: OverlayState) {
  if (!container || !icon || !text) return;

  // Remove all state classes
  container.classList.remove("idle", "recording", "transcribing");

  // Add current state class
  container.classList.add(state);

  // Update text and icon
  switch (state) {
    case "recording":
      text.textContent = "Recording...";
      icon.innerHTML = "🎤";
      break;
    case "transcribing":
      text.textContent = "Transcribing...";
      icon.innerHTML = "⚡";
      break;
    case "idle":
    default:
      text.textContent = "Idle";
      icon.innerHTML = "💤";
      break;
  }
}

// Initialize with idle state
updateOverlay("idle");
