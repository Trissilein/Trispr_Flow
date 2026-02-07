# App Flow — Trispr Flow

> Every screen, route, and user journey in Trispr Flow.

---

## Application Structure

Trispr Flow is a **single-page desktop app** (no routes/navigation). All functionality lives on one screen with collapsible panels.

### Windows
1. **Main Window** (`index.html`) — Primary UI with all panels
2. **Overlay Window** (`overlay.html`) — Always-on-top visual feedback during recording
3. **Conversation Detach Window** — Standalone conversation view

---

## Main Window Layout

```
┌─────────────────────────────────────────────────┐
│ Hero Section                                    │
│ ├── Badge (Offline dictation)                  │
│ ├── Title + Subtitle                           │
│ ├── Status Pills (Recording, Transcribing)     │
│ └── Hero Card (Quick stats)                    │
├─────────────────────────────────────────────────┤
│ Output Panel (History Tabs)                    │
│ ├── Input Tab                                  │
│ ├── Output Tab                                 │
│ ├── Conversation Tab                           │
│ └── History List                               │
├──────────────────┬──────────────────────────────┤
│ Capture Input    │ Capture Output               │
│ - Device         │ - Device                     │
│ - Mode (PTT/VAD) │ - VAD Toggle                 │
│ - Hotkeys        │ - Hotkey                     │
│ - VAD Settings   │ - Threshold                  │
│ - Audio Cues     │ - Meters                     │
├──────────────────┴──────────────────────────────┤
│ Model Manager                                   │
│ ├── Source Selection                           │
│ ├── Active Models                              │
│ ├── Installed Models (expander)                │
│ └── Available Models (expander)                │
├─────────────────────────────────────────────────┤
│ UX / UI Adjustments                            │
│ ├── Overlay Style (Dot / KITT)                 │
│ └── Overlay Settings (expander)                │
└─────────────────────────────────────────────────┘
```

---

## Screens & Panels

### 1. Hero Section
**Purpose**: High-level status + quick stats

**Elements**:
- **Badge**: "Private Mode (Offline)" (or "AI-enhanced Mode (Online)")
- **Title**: Trispr Flow
- **Subtitle**: App description
- **Status Pills**:
  - Recording pill (Recording / Idle)
  - Transcribing pill (Transcribing / Idle)
  - Status dots (animated, color-coded)
- **Hero Card**:
  - AI fallback toggle
  - Input Transcription status
  - Capture Mode (PTT / VAD)
  - Output Transcription status
  - Model name

**User Actions**:
- Toggle AI fallback
- View current state at a glance

---

### 2. Output Panel
**Purpose**: View transcription history across sources

**Tabs**:
1. **Input Tab** — Microphone transcripts only (PTT + VAD)
2. **Output Tab** — System audio transcripts only
3. **Conversation Tab** — Combined timeline of both sources

**Elements**:
- **History Tabs**: Switch between views
- **History Toolbar**:
  - "Copy conversation" button
  - "Detach" button (opens conversation in new window)
  - Font size slider (12-24px)
- **History List**: Scrollable list of entries
  - Each entry shows:
    - Icon (🎤 for mic, 🔊 for system audio)
    - Transcript text
    - Timestamp
    - Source label
    - Copy button (on hover)
- **History Compose**: Test input field + "Add to history" button

**User Journey**:
1. User speaks into mic → transcript appears in Input tab
2. User plays system audio → transcript appears in Output tab
3. User switches to Conversation tab → sees combined timeline
4. User adjusts font size → text size updates live
5. User clicks "Copy conversation" → entire conversation copied to clipboard
6. User clicks "Detach" → conversation opens in new window

---

### 3. Capture Input Panel
**Purpose**: Configure microphone capture settings

**Elements**:
- **Enable toggle**: Enable/disable input capture
- **Input device dropdown**: Select microphone
- **Capture mode dropdown**: PTT or VAD
- **PTT Hotkey**:
  - Hotkey input (read-only)
  - "Record" button to capture hotkey
  - Status label (success/error/recording)
- **Toggle Hotkey**:
  - Same structure as PTT hotkey
- **Use Voice Activation in PTT toggle**: Enable VAD even in PTT mode
- **VAD Settings** (shown when VAD mode active):
  - Voice Activation threshold slider (-60dB to 0dB)
  - Silence grace slider (200ms to 4000ms)
  - Input level meter with dB readout
  - Threshold markers on meter
  - Mic input gain slider (-30dB to +30dB)
- **Audio cues toggle**: Enable/disable audio feedback
- **Audio cue volume slider**: 0-100%

**User Journey**:
1. User enables input capture
2. User selects microphone from dropdown
3. User chooses PTT mode
4. User clicks "Record" next to PTT hotkey
5. User presses desired hotkey (e.g., Ctrl+Shift+Space)
6. Hotkey is registered and saved
7. User enables audio cues
8. User holds PTT hotkey → recording starts → audio cue plays
9. User releases PTT hotkey → recording stops → transcription runs
10. OR: User switches to VAD mode → adjusts threshold → speaks → auto-records

---

### 4. Capture Output Panel
**Purpose**: Configure system audio transcription

**Elements**:
- **Enable toggle**: Enable/disable output transcription
- **Output device dropdown**: Select system audio output
- **Use Voice Activation toggle**: Enable VAD for output
- **Transcribe hotkey**:
  - Hotkey input (read-only)
  - "Record" button
  - Status label
- **VAD Settings** (when VAD enabled):
  - Voice Activation threshold slider
  - Grace silence slider (200ms to 5000ms)
- **Output level meter**: Real-time dB visualization
  - Meter fill
  - Threshold marker
  - dB readout
- **Input gain slider**: -30dB to +30dB
- **Chunk interval slider**: 4s to 15s (when VAD disabled)
- **Chunk overlap slider**: 0s to 3s (when VAD disabled)

**User Journey**:
1. User enables output transcription
2. User selects output device (e.g., "Speakers (Loopback)")
3. User clicks "Record" next to transcribe hotkey
4. User presses desired hotkey (e.g., Ctrl+Shift+O)
5. Hotkey is registered
6. User presses transcribe hotkey → output monitoring starts
7. System audio plays (e.g., YouTube video)
8. Audio chunks are transcribed every 8s (or on VAD trigger)
9. Transcripts appear in Output tab
10. User presses transcribe hotkey again → monitoring stops

---

### 5. Model Manager Panel
**Purpose**: Download and manage whisper.cpp models

**Elements**:
- **Source dropdown**: Default (whisper.cpp) or Custom URL
- **Custom URL field** (when Custom selected):
  - URL input
  - "Refresh" button
  - Hint: "Supports JSON index or direct model file URL"
- **Model storage path**:
  - Path input
  - "Browse" button
  - "Reset" button
  - Hint: "Choose where downloaded models are stored"
- **Active section**: Currently loaded model
  - Model name
  - Model size
  - "Active" badge
  - Model description
- **Installed expander**: Downloaded models
  - List of installed models
  - Each shows: name, size, description
  - "Select" button (to activate)
  - "Delete" button
- **Available expander**: Models to download
  - List of remote models
  - Each shows: name, size, description
  - "Download" button
  - Progress bar (during download)

**User Journey**:
1. User opens Model Manager panel
2. User sees active model (e.g., "ggml-tiny.en.bin")
3. User expands "Available"
4. User clicks "Download" on "ggml-base.en.bin"
5. Progress bar shows download progress
6. Model moves to "Installed" section
7. User clicks "Select" → model becomes active
8. Hero card updates to show new model name

---

### 6. UX / UI Adjustments Panel
**Purpose**: Customize overlay appearance

**Elements**:
- **Overlay style dropdown**: Circle (Dot) or KITT by Dox®
- **Overlay settings expander**:
  - **Color picker**: Overlay color
  - **Dot mode settings** (when Dot selected):
    - Min radius slider (4-32px)
    - Max radius slider (8-64px)
  - **KITT mode settings** (when KITT selected):
    - Min width slider (4-40px)
    - Max width slider (50-800px)
    - Height slider (8-40px)
  - **Shared settings**:
    - Rise smoothing slider (20-600ms)
    - Fall smoothing slider (20-800ms)
    - Inactive opacity slider (5-100%)
    - Active opacity slider (5-100%)
    - Position X input (pixels)
    - Position Y input (pixels)
  - **Apply button**: "Apply Overlay Settings"

**User Journey**:
1. User selects "Circle (Dot)" overlay
2. User expands "Overlay settings"
3. User adjusts max radius to 32px
4. User sets color to #ff6b3d (accent orange)
5. User clicks "Apply Overlay Settings"
6. Overlay window updates immediately
7. User starts recording → overlay dot grows/shrinks with audio level

---

## Overlay Window

**Purpose**: Always-on-top visual feedback during recording

**Modes**:
1. **Dot Mode**: Circular dot that grows/shrinks with audio level
2. **KITT Mode**: Horizontal bar that expands/contracts

**States**:
- **Idle**: Min size, inactive opacity
- **Recording**: Grows to max size based on audio level, active opacity
- **Transcribing**: Returns to min size

**Customization**:
- Color
- Size range (min/max)
- Smoothing (rise/fall timing)
- Opacity (inactive/active)
- Position (x, y)

---

## User Journeys

### Journey 1: First-Time Setup
1. User launches app
2. User sees default settings:
   - Capture mode: PTT
   - PTT hotkey: Ctrl+Shift+Space
   - Model: (not loaded yet)
3. User goes to Model Manager
4. User downloads "ggml-base.en.bin"
5. User selects model → becomes active
6. User returns to Capture Input
7. User holds PTT hotkey → speaks → releases
8. Transcript appears in Input tab
9. Success!

### Journey 2: System Audio Transcription
1. User enables output transcription
2. User selects "Speakers (Loopback)" output device
3. User sets transcribe hotkey to Ctrl+Shift+O
4. User opens YouTube video
5. User presses Ctrl+Shift+O → output monitoring starts
6. YouTube audio plays → transcribed in 8s chunks
7. Transcripts appear in Output tab
8. User switches to Conversation tab → sees mic + system audio combined
9. User presses Ctrl+Shift+O → monitoring stops

### Journey 3: VAD Mode for Hands-Free Dictation
1. User switches Capture mode to "Voice Activation"
2. VAD settings appear
3. User adjusts threshold to -34dB
4. User adjusts silence grace to 700ms
5. User speaks → recording starts automatically
6. User stops speaking → grace period → recording stops
7. Transcript appears in Input tab
8. No hotkey needed!

### Journey 4: Claude Fallback
1. User enables Claude fallback toggle
2. Badge changes to "Claude fallback"
3. User speaks into mic
4. Local whisper.cpp fails (model not loaded)
5. App falls back to Claude API
6. Transcript appears in Input tab
7. Toast notification: "Used Claude fallback"

### Journey 5: Conversation Detach
1. User has long conversation history (50+ entries)
2. User clicks "Detach" button
3. New window opens with Conversation tab only
4. User can adjust font size, copy conversation
5. Main window remains open for settings

---

## Edge Cases & Error States

### No Microphone
- Device dropdown shows "No devices found"
- Enable toggle is disabled
- Toast: "No input devices detected"

### No Output Device
- Device dropdown shows "No output devices found"
- Enable toggle is disabled
- Toast: "No output devices detected"

### Model Not Loaded
- Hero card shows "Model: —"
- Transcription attempts fall back to Claude (if enabled)
- Toast: "No model loaded. Please download a model."

### Hotkey Conflict
- User tries to set PTT hotkey to Ctrl+Shift+M (same as Toggle hotkey)
- Status label: "Conflict with Toggle hotkey"
- Hotkey not saved

### Download Failed
- Model download fails (network error)
- Progress bar stops
- Toast: "Download failed: [error]"
- "Download" button reappears

### Transcription Failed
- Whisper.cpp fails
- Claude fallback also fails
- Toast: "Transcription failed: [error]"
- Recording is lost (no retry)

---

## Responsive Behavior

### Desktop (>920px)
- 2-column layout
- Panels side-by-side

### Mobile (≤920px)
- 1-column layout
- Panels stack vertically
- All functionality intact

---

## Keyboard Navigation

### Tab Order
1. Hero Card (Cloud toggle)
2. Output Panel (Tabs → Toolbar → History → Compose)
3. Capture Input Panel (Toggle → Device → Mode → Hotkeys → VAD → Audio Cues)
4. Capture Output Panel (Toggle → Device → Hotkey → VAD → Meters → Gain → Chunk)
5. Model Manager (Source → URL → Storage → Active → Installed → Available)
6. UX / UI Adjustments (Overlay → Settings)

### Shortcuts
- **Tab**: Move focus forward
- **Shift+Tab**: Move focus backward
- **Enter/Space**: Activate focused element
- **Escape**: Close expanders, cancel hotkey recording
- **Arrow Keys**: Adjust range sliders

---

## Future Screens

### Planned
- **Settings Panel**: General app settings (language, theme, updates)
- **Conversation Detach Window**: Standalone conversation view
- **Post-Processing Panel**: Punctuation, formatting, normalization

### Ideas
- **Live Transcript Dump**: Export conversation to Markdown/TXT
- **Chapter Summarization**: AI-generated summaries of conversation sections
- **Hotkey Conflict Manager**: Visual conflict resolution

---

**Last updated**: 2026-02-06
**Version**: 1.0

