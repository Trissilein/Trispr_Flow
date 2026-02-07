# Transcribe (System-Output) – Implementierungsplan

## Ziel
Ein neues Feature **Transcribe**: Per Toggle-Hotkey werden System-Ausgänge (Output-Audio) aufgezeichnet und transkribiert.
Läuft **parallel** zur Mic-Pipeline, hat **eigene History** und **eigene Settings**.

---

## Architektur-Entscheidung
**Getrennte Pipelines mit Priority-System (empfohlen)**
- Mic-Transcription = Priorität 1 (immer sofort)
- Output-Transcription = Priorität 2 (queued wenn Mic aktiv)
- Separate Buffer + Worker + History
- Resource-Management verhindert GPU-Overload

**Entscheidungen (aktuell)**
- Capture: **WASAPI Loopback** (Windows)
- Chunking: **Fixed-Chunk + Overlap (Default)**, VAD optional später
- Language: **geteilt mit Mic**
- History: **separate Histories**
- Hotkey: **eigener Toggle**
- Queue: **Output queued wenn Mic aktiv**
- Späterer Milestone: **Live-Dump + Kapitel‑Zusammenfassung**

---

## Milestones

### **M0 – Settings + Datenmodell**

**Deliverables**
```rust
// Settings struct erweitern (lib.rs)
struct Settings {
  // ... existing fields ...

  // Output Transcription
  transcribe_enabled: bool,
  transcribe_hotkey: String,
  transcribe_output_device: String,
  transcribe_vad_mode: bool,           // false = Fixed chunks (default), true = VAD (optional)
  transcribe_vad_threshold: f32,        // Höher als Mic (z.B. 0.04)
  transcribe_batch_interval_ms: u64,    // Fixed-chunk interval (8000ms default)
  transcribe_chunk_overlap_ms: u64,     // Overlap für Kontext (1000ms default)
  transcribe_audio_cues: bool,          // Separate audio cues?
  transcribe_priority: String,          // "high" | "low" (Queue-Verhalten)
}

// Defaults
transcribe_enabled: false,
transcribe_hotkey: "CommandOrControl+Shift+O".to_string(),
transcribe_output_device: "default".to_string(),
transcribe_vad_mode: false,
transcribe_vad_threshold: 0.04,
transcribe_batch_interval_ms: 8000,
transcribe_chunk_overlap_ms: 1000,
transcribe_audio_cues: true,
transcribe_priority: "low".to_string(),
```

**Neue History**
```rust
// Separate History für Output
history_transcribe: Vec<HistoryEntry>

// HistoryEntry mit Source-Field
struct HistoryEntry {
  id: String,
  text: String,
  timestamp_ms: u64,
  source: String,  // "mic-ptt" | "mic-vad" | "output"
}
```

**API-Stubs**
```rust
#[tauri::command]
fn get_transcribe_history(state: State<AppState>) -> Vec<HistoryEntry>

#[tauri::command]
fn add_transcribe_entry(state: State<AppState>, text: String) -> Vec<HistoryEntry>
```

**Akzeptanz**
- Settings persistiert ✓
- Output-History separiert ✓
- API-Stubs vorhanden ✓

---

### **M1 – UI + Hotkey**

**UI-Platzierung**
- Neues Panel **"Transcribe"** (eigene Kategorie)
- Titel: **"System Audio Transcription"**
- Nur sichtbar wenn `transcribe_enabled` aktiviert (Master-Toggle)

**UI-Komponenten**
```html
<details class="expander" id="transcribe-expander">
  <summary>System Audio Transcription</summary>
  <div class="expander-body">
    <!-- Status -->
    <div class="field">
      <span class="field-label">Status</span>
      <span id="transcribe-status" class="status-indicator">Idle</span>
    </div>

    <!-- Hotkey -->
    <div class="field hotkey-field">
      <span class="field-label">Toggle hotkey</span>
      <input id="transcribe-hotkey" type="text" readonly />
      <button id="transcribe-hotkey-record">🎹 Record</button>
    </div>

    <!-- Output Device -->
    <label class="field">
      <span class="field-label">Output device</span>
      <select id="transcribe-device-select"></select>
    </label>

    <!-- VAD vs Batch Mode -->
    <label class="field toggle">
      <span class="field-label">Use voice detection (VAD)</span>
      <input id="transcribe-vad-toggle" type="checkbox" />
      <span class="toggle-hint">Optional. Default is fixed chunks.</span>
    </label>

    <!-- Batch Interval (nur sichtbar wenn VAD disabled) -->
    <label class="field range" id="transcribe-batch-field">
      <span class="field-label">Chunk interval</span>
      <div class="range-row">
        <input id="transcribe-batch-interval" type="range" min="4000" max="15000" step="1000" />
        <span id="transcribe-batch-value">8s</span>
      </div>
    </label>

    <!-- Overlap (nur sichtbar wenn VAD disabled) -->
    <label class="field range" id="transcribe-overlap-field">
      <span class="field-label">Chunk overlap</span>
      <div class="range-row">
        <input id="transcribe-chunk-overlap" type="range" min="0" max="3000" step="250" />
        <span id="transcribe-overlap-value">1s</span>
      </div>
    </label>
  </div>
</details>
```

**Hotkey-Logik**
```typescript
// main.ts
setupHotkeyRecorder("transcribe", transcribeHotkey, transcribeHotkeyRecord, transcribeHotkeyStatus);

// Hotkey-Konflikt-Prüfung
if (transcribeHotkey === pttHotkey || transcribeHotkey === toggleHotkey) {
  showToast({ type: "error", title: "Conflict", message: "Hotkey conflicts with PTT/Toggle" });
}
```

**Akzeptanz**
- UI-Panel vorhanden und verborgen wenn disabled ✓
- Hotkey registriert (nur wenn `transcribe_enabled`) ✓
- Toggle wechselt Status Idle ↔ Monitoring ✓
- Keine Hotkey-Konflikte ✓

---

### **M1.5 – Error Handling (Foundation)**

**Device-Fehler**
```rust
// Device nicht verfügbar
fn resolve_output_device(device_id: &str) -> Result<Device, String> {
  let host = cpal::default_host();

  if device_id == "default" {
    return host.default_output_device()
      .ok_or_else(|| "No default output device available".to_string());
  }

  // Loopback-Device suchen
  // Windows: WASAPI Loopback
  // macOS: ScreenCaptureKit / AVAudioEngine

  Err("Output device not found".to_string())
}
```

**Error-Toast in UI**
```typescript
listen<ErrorEvent>("transcribe:error", (event) => {
  showErrorToast(event.payload.error, "Output Transcription");
});
```

**Akzeptanz**
- Device unavailable → Toast + Status "Error" ✓
- Format nicht unterstützt → Toast + Fallback zu Default ✓
- Permission fehlt (macOS) → Toast mit Anleitung ✓

---

### **M2 – Output Capture (Windows WASAPI Loopback)**

**Device Listing**
```rust
#[tauri::command]
fn list_output_devices() -> Vec<AudioDevice> {
  let mut devices = vec![AudioDevice {
    id: "default".to_string(),
    label: "System Default Output".to_string(),
  }];

  // Windows: WASAPI Loopback-Devices filtern (cpal alleine reicht nicht)
  #[cfg(target_os = "windows")]
  {
    // TODO: implement via wasapi crate or cpal-wasapi support
    // enumerate IAudioEndpoint for loopback-capable devices
  }

  devices
}
```

**Capture-Thread**
```rust
fn start_output_capture(
  app: &AppHandle,
  state: &State<AppState>,
  settings: &Settings,
) -> Result<(), String> {
  // Output loopback capture (Windows) or ScreenCaptureKit (macOS)
  // NOTE: CPAL output devices are NOT loopback capture by default.

  let device = resolve_output_device(&settings.transcribe_output_device)?;
  let config = device.default_output_config()?;

  // VAD oder Fixed-Chunk?
  if settings.transcribe_vad_mode {
    // VAD-Monitor für Output (optional, experimental)
    let vad_handle = VadHandle::new(settings.transcribe_vad_threshold);
    // ... stream mit VAD
  } else {
    // Fixed-Chunk Timer + Overlap
    let batch_interval = settings.transcribe_batch_interval_ms;
    let overlap = settings.transcribe_chunk_overlap_ms;
    // ... stream mit Timer + overlap buffer
  }

  Ok(())
}
```

**Akzeptanz**
- Output-Device Listing zeigt Loopback-Devices ✓
- Capture startet/stoppt bei Hotkey-Toggle ✓
- Audio wird in separaten Buffer geschrieben ✓

---

### **M2.5 – Device Selection Logic**

**Default Device Handling**
```rust
// Fallback-Chain
fn get_best_output_device() -> Result<Device, String> {
  let host = cpal::default_host();

  // 1. Try configured device
  // 2. Try default output
  // 3. Try first available output

  host.default_output_device()
    .or_else(|| host.output_devices().ok()?.next())
    .ok_or_else(|| "No output device available".to_string())
}
```

**Virtual Device Support**
- VB-Cable / Virtual Audio Cable erkennen
- Warnung wenn Virtual Device als Output (Loop-Gefahr)

**Akzeptanz**
- "System Default Output" funktioniert ✓
- Fallback zu erstem Device wenn configured fehlt ✓
- Virtual-Device-Warnung angezeigt ✓

---

### **M3 – Transcription Pipeline mit Queue-System (Channel-based)**

**Resource Management**
```rust
enum TranscriptionPriority {
  High,  // Mic (sofort)
  Low,   // Output (queued)
}

struct TranscriptionQueue {
  mic_active: AtomicBool,
  tx: crossbeam_channel::Sender<(TranscriptionPriority, Vec<f32>)>,
}

impl TranscriptionQueue {
  fn request_transcription(&self, priority: TranscriptionPriority, buffer: Vec<f32>) {
    let _ = self.tx.send((priority, buffer));
  }
}
```

**Transcription Worker**
```rust
fn transcribe_output_audio(
  app: AppHandle,
  state: State<AppState>,
  audio_data: Vec<f32>,
) {
  // Status: transcribing
  emit("transcribe:state", "transcribing");

  // Whisper ausführen (eigener Thread)
  let result = run_whisper_inference(audio_data, &settings);

  match result {
    Ok(text) => {
      // In history_transcribe speichern
      add_transcribe_entry(state, text, "output");
      emit("transcribe:result", text);
    }
    Err(err) => {
      emit("transcribe:error", err);
    }
  }

  // Status: idle
  emit("transcribe:state", "idle");
}
```

**Akzeptanz**
- Output-Transcription läuft parallel zu Mic (wenn Mic idle) ✓
- Bei aktivem Mic wird Output gequeued ✓
- Toast informiert über Queue-Status ✓
- Mic bleibt responsiv (keine Blockierung) ✓

---

### **M3.5 – Testing Strategy**

**Unit Tests**
```rust
#[cfg(test)]
mod tests {
  #[test]
  fn test_output_device_listing() {
    let devices = list_output_devices();
    assert!(!devices.is_empty());
    assert_eq!(devices[0].id, "default");
  }

  #[test]
  fn test_transcription_queue() {
    let queue = TranscriptionQueue::new();
    // Mic aktiv → Output wird gequeued
    queue.mic_active.store(true, Ordering::Relaxed);
    queue.request_transcription(TranscriptionPriority::Low, vec![]);
    assert!(queue.output_pending.load(Ordering::Relaxed));
  }
}
```

**Integration Tests**
- Start Output Capture → Stop → Verify Audio captured
- Start Mic + Output parallel → Verify Queue-System
- Change Output Device während Capture → Verify Restart

**Manual Test Checklist**
- [ ] Hotkey funktioniert (Start/Stop)
- [ ] Device-Switch während Recording (sollte restart)
- [ ] VAD vs Fixed-Chunk Modus
- [ ] Queue-System (Mic blockt Output)
- [ ] Virtual-Device-Warnung
- [ ] macOS Permission Flow

---

### **M4 – UI: Output History mit Combined View**

**History-Tabs**
```html
<div class="history-tabs">
  <button id="history-tab-all" class="tab active">All</button>
  <button id="history-tab-mic" class="tab">Microphone</button>
  <button id="history-tab-output" class="tab">System Audio</button>
</div>

<div id="history-list" class="history-list"></div>
```

**Entry-Rendering mit Icons**
```typescript
function renderHistoryEntry(entry: HistoryEntry) {
  const icon = entry.source === "output"
    ? "🔊" // System audio
    : "🎤"; // Microphone

  const sourceLabel = entry.source === "output"
    ? "System Audio"
    : entry.source === "mic-vad" ? "Voice (Auto)" : "Voice (PTT)";

  return `
    <div class="history-item" data-source="${entry.source}">
      <div class="history-icon">${icon}</div>
      <div class="history-content">
        <div class="history-text">${entry.text}</div>
        <div class="history-meta">${formatTime(entry.timestamp_ms)} · ${sourceLabel}</div>
      </div>
      <div class="history-actions">
        <button class="history-copy">Copy</button>
      </div>
    </div>
  `;
}
```

**Filter-Logik**
```typescript
let currentFilter: "all" | "mic" | "output" = "all";

function filterHistory(filter: string) {
  currentFilter = filter;
  const items = document.querySelectorAll(".history-item");

  items.forEach(item => {
    const source = item.dataset.source;
    const visible = filter === "all"
      || (filter === "mic" && source.startsWith("mic"))
      || (filter === "output" && source === "output");

    item.style.display = visible ? "flex" : "none";
  });
}
```

**Akzeptanz**
- Tab "All" zeigt beide Historien gemischt (timestamp-sorted) ✓
- Tab "Microphone" nur Mic-Entries ✓
- Tab "System Audio" nur Output-Entries ✓
- Icons unterscheiden visuell ✓
- Copy-Button funktioniert für beide ✓

---

### **M5 – macOS Support + Permissions**

**ScreenCaptureKit Integration**
```rust
#[cfg(target_os = "macos")]
mod macos_audio {
  use screencapturekit::*;

  pub fn capture_system_audio() -> Result<AudioStream, String> {
    // Check permission first
    if !has_screen_recording_permission() {
      return Err("Screen Recording permission required. Go to System Preferences → Privacy → Screen Recording.".to_string());
    }

    // Create audio capture session
    let session = SCStreamConfiguration::new();
    session.set_capture_audio(true);

    // ...
  }

  fn has_screen_recording_permission() -> bool {
    // Check macOS Screen Recording permission
    // Required for system audio capture
    CGPreflightScreenCaptureAccess()
  }
}
```

**Permission UI Flow**
```typescript
// Frontend: Permission-Check
if (isMac && !hasPermission) {
  showToast({
    type: "warning",
    title: "Permission Required",
    message: "System Audio requires Screen Recording permission. Click to open System Preferences.",
    duration: 0, // Persistent
  });

  // Button öffnet System Preferences
  openSystemPreferences("privacy_screen_recording");
}
```

**Graceful Degradation**
```rust
// Wenn Permission fehlt
fn start_output_capture_with_fallback(settings: &Settings) -> Result<(), String> {
  #[cfg(target_os = "macos")]
  {
    if !has_screen_recording_permission() {
      // Disabled State + UI-Hinweis
      emit("transcribe:permission-required", "screen_recording");
      return Err("Permission denied".to_string());
    }
  }

  // Normal flow
  start_output_capture(settings)
}
```

**Akzeptanz**
- macOS Permission-Check beim Start ✓
- UI zeigt Hinweis + Link zu System Preferences ✓
- Graceful Degradation wenn Permission fehlt ✓
- System Audio Capture funktioniert auf macOS ✓

---

## Open Decisions – Empfehlungen

### 1. Output-Pipeline mit eigener VAD?
**Entscheidung: Dual-Mode (Default = Fixed chunks)**
- **Fixed-Chunk (Default)**: 8–12s Chunks + 1s overlap  
  - Stabil für Musik/Videos, kein VAD-Flattern
- **VAD-Mode (Optional/Experimental)**: für echte Pausen

**Implementierung:**
```rust
if settings.transcribe_vad_mode {
  // VAD mit höherem Threshold
  let vad = VadRuntime::new(settings.transcribe_vad_threshold);
} else {
  // Fixed-Chunk Timer
  let batch_timer = Timer::new(settings.transcribe_batch_interval_ms);
}
```

### 2. Output-Language fest vs. auto?
**Entscheidung: Teilen mit Mic-Settings (initial)**
- Nutze `settings.language_mode` auch für Output
- Reduziert Komplexität
- User erwartet konsistentes Verhalten
- Falls später nötig: Separates `transcribe_language_mode` hinzufügen

### 3. Delay/Batching Strategy?
**Entscheidung: Hybrid mit User-Choice**
- Default: Fixed-Chunk (8–12s) + Overlap (1s)
- Optional: VAD-Mode (Experimental)
- Setting: `transcribe_batch_interval_ms: 8000`, `transcribe_chunk_overlap_ms: 1000`

---

## Risiken & Mitigation

| Risiko | Impact | Mitigation |
|--------|--------|-----------|
| **2x Whisper = GPU-Overload** | Hoch | Queue-System (M3) + Priority + optional GPU throttle |
| **Audio Format Mismatch** (48kHz Output → 16kHz Whisper) | Mittel | Resampling in Capture-Thread (cpal built-in) |
| **Latency** (Output-Audio ist gepuffert) | Niedrig | Akzeptabel für Post-Processing (nicht Echtzeit) |
| **Virtual Devices** (Loop-Gefahr) | Mittel | Warning in UI + Detection-Logic |
| **macOS Permissions** (Screen Recording) | Hoch | M5: Permission-Check + UI-Flow |
| **Linux Support** (kein WASAPI) | Niedrig | Später: PulseAudio / PipeWire Integration |

---

## Kritische Dateien

**Backend (Rust)**
- `src-tauri/src/lib.rs` – Settings, Commands, Capture-Logic
- `src-tauri/src/transcribe.rs` (neu) – Output-Transcription Module
- `src-tauri/Cargo.toml` – Dependencies (screencapturekit für macOS)

**Frontend (TypeScript)**
- `src/main.ts` – UI-Logik, Event-Listener, History-Rendering
- `index.html` – UI-Komponenten (Expander, Tabs)
- `src/styles.css` – History-Icons, Tab-Styles

---

## Verifizierung (End-to-End)

Nach M5-Abschluss:

### Windows
1. `npm run tauri dev` starten
2. Output-Device auswählen (z.B. "Speakers (Loopback)")
3. Hotkey drücken → Status "Monitoring"
4. YouTube-Video abspielen (mit Audio)
5. Hotkey drücken → Status "Transcribing"
6. **Prüfen:** Transkript erscheint in History mit 🔊 Icon
7. **Prüfen:** Tab "System Audio" zeigt nur Output-Entries

### macOS
1. Permission-Check: System Preferences → Privacy → Screen Recording ✓
2. Gleicher Flow wie Windows
3. **Prüfen:** Permission-Toast erscheint falls nicht granted

### Parallel-Test
1. VAD-Modus aktivieren (Mic)
2. Output-Transcription starten
3. **In Mic sprechen** → Sofort transkribiert
4. **System-Audio spielt** → Gequeued + Toast angezeigt
5. **Mic stoppt** → Output-Queue wird abgearbeitet
6. **Prüfen:** Beide Entries in "All" Tab, richtig sortiert

---

## Nächste Schritte nach M5

**Nice-to-Have (Later)**
- Linux-Support (PulseAudio/PipeWire)
- Output-Overlay (separater Dot für System Audio?)
- Export-Funktion (Output-History → CSV)
- Auto-Copy zu Clipboard (Setting: `transcribe_auto_copy`)
- Separate Cloud-Fallback für Output
