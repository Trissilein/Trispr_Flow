# Frontend Guidelines — Trispr Flow

> Component engineering, state management, and file structure for the Trispr Flow frontend.

---

## Tech Stack

### Core
- **Framework**: Vanilla TypeScript + Tauri 2
- **Build Tool**: Vite 6.0.3
- **Type System**: TypeScript 5.6.2
- **Testing**: Vitest 1.6.0

### No Framework Philosophy
Trispr Flow intentionally avoids React/Vue/Svelte to:
- Keep bundle size minimal
- Reduce startup latency
- Maintain full control over DOM updates
- Simplify the architecture for a desktop app

---

## File Structure

```
src/
├── main.ts              # App initialization (~220 lines, modular entry point)
├── state.ts             # Global application state
├── types.ts             # TypeScript type definitions
├── settings.ts          # Settings persistence & UI rendering
├── devices.ts           # Audio device management
├── hotkeys.ts           # Hotkey configuration
├── models.ts            # Model management
├── history.ts           # Transcript history logic
├── dom-refs.ts          # Centralized DOM references
├── event-listeners.ts   # Event handler setup
├── ui-state.ts          # UI state management
├── ui-helpers.ts        # UI utility functions
├── toast.ts             # Toast notifications
├── accessibility.ts     # Accessibility helpers
├── audio-cues.ts        # Audio feedback system
├── overlay.ts           # Overlay state + animation
└── styles.css           # App styling

index.html               # Main window UI
overlay.html             # Overlay UI (separate window)
```

---

## Architecture Principles

### 1. Modular by Purpose
Each module has a single responsibility:
- `state.ts` → single source of truth for app state
- `settings.ts` → persistence + UI rendering
- `devices.ts` → audio device enumeration + selection
- `hotkeys.ts` → hotkey recording + registration
- etc.

### 2. Centralized DOM References
All DOM element lookups happen in `dom-refs.ts`:
```typescript
export const DOM = {
  deviceSelect: document.getElementById('device-select') as HTMLSelectElement,
  modeSelect: document.getElementById('mode-select') as HTMLSelectElement,
  // ... 50+ references
}
```

**Why**: Prevents scattered `getElementById` calls. Single source of truth. Type-safe.

### 3. Event-Driven Architecture
Event listeners are registered in `event-listeners.ts`:
```typescript
export function setupEventListeners() {
  DOM.deviceSelect.addEventListener('change', handleDeviceChange);
  DOM.modeSelect.addEventListener('change', handleModeChange);
  // ...
}
```

**Why**: Separates event wiring from business logic. Easy to audit.

### 4. State Management
Global state lives in `state.ts`:
```typescript
export const AppState = {
  recordingState: 'idle' as RecordingState,
  transcribeState: 'idle' as TranscribeState,
  currentDevice: 'default',
  currentMode: 'ptt' as CaptureMode,
  cloudEnabled: false,
  // ...
}
```

**Why**: Single source of truth. Predictable updates. Easy to debug.

---

## State Management Patterns

### Read State
```typescript
import { AppState } from './state';

if (AppState.recordingState === 'recording') {
  // ...
}
```

### Update State
```typescript
import { AppState } from './state';

AppState.recordingState = 'recording';
updateRecordingUI(); // Manually trigger UI update
```

### UI Updates
State changes do NOT automatically trigger UI updates. You must manually call update functions:
```typescript
function updateRecordingUI() {
  DOM.statusDot.dataset.state = AppState.recordingState;
  DOM.statusLabel.textContent = `Recording: ${AppState.recordingState}`;
}
```

**Why**: Explicit > Implicit. No hidden reactivity. Full control over when DOM updates happen.

---

## Component Patterns

### Panel Collapse
```html
<button class="panel-collapse-btn"
        data-panel-collapse="output"
        aria-expanded="true"
        aria-controls="output-panel-body">▾</button>
```

```typescript
// Handled in event-listeners.ts
document.querySelectorAll('[data-panel-collapse]').forEach(btn => {
  btn.addEventListener('click', (e) => {
    const panelId = btn.dataset.panelCollapse;
    const panel = document.querySelector(`[data-panel="${panelId}"]`);
    panel?.classList.toggle('panel-collapsed');
    btn.setAttribute('aria-expanded', !panel?.classList.contains('panel-collapsed'));
  });
});
```

### Toggle Switch
```html
<label class="toggle-row">
  <span class="field-label">Enable input capture</span>
  <input id="capture-enabled-toggle" type="checkbox" />
  <span class="toggle-track">
    <span class="toggle-thumb"></span>
  </span>
</label>
```

```css
/* CSS handles visual state */
.toggle-row input:checked + .toggle-track {
  background: var(--accent-2);
}
```

```typescript
// Event listener updates AppState + backend
DOM.captureEnabledToggle.addEventListener('change', async () => {
  AppState.captureEnabled = DOM.captureEnabledToggle.checked;
  await invoke('set_capture_enabled', { enabled: AppState.captureEnabled });
});
```

### Range Slider with Live Value
```html
<label class="field range">
  <span class="field-label">Voice Activation threshold</span>
  <div class="range-row">
    <input id="vad-threshold" type="range" min="-60" max="0" step="1"
           aria-valuenow="-34"
           aria-label="Voice activation threshold" />
    <span id="vad-threshold-value" class="range-value">-34 dB</span>
  </div>
</label>
```

```typescript
DOM.vadThreshold.addEventListener('input', () => {
  const value = parseInt(DOM.vadThreshold.value, 10);
  DOM.vadThresholdValue.textContent = `${value} dB`;
  DOM.vadThreshold.setAttribute('aria-valuenow', value.toString());
});
```

---

## Tauri Integration

### Invoking Rust Commands
```typescript
import { invoke } from '@tauri-apps/api/core';

// Get devices
const devices = await invoke<AudioDevice[]>('list_devices');

// Update setting
await invoke('update_setting', {
  key: 'capture_mode',
  value: 'vad'
});
```

### Listening to Rust Events
```typescript
import { listen } from '@tauri-apps/api/event';

await listen<TranscriptionResult>('transcription-result', (event) => {
  const { text, source } = event.payload;
  addHistoryEntry(text, source);
});
```

### Window Management
```typescript
import { Window } from '@tauri-apps/api/window';

// Create overlay window
const overlayWindow = new Window('overlay', {
  url: 'overlay.html',
  alwaysOnTop: true,
  decorations: false,
  transparent: true,
});
```

---

## Accessibility Patterns

### ARIA Labels
All interactive elements have accessible names:
```html
<!-- Button with text -->
<button>Apply Overlay Settings</button>

<!-- Button with aria-label (icon-only) -->
<button aria-label="Collapse output panel">▾</button>

<!-- Input with label -->
<label>
  <span class="field-label">Input device</span>
  <select id="device-select"></select>
</label>
```

### ARIA Live Regions
Status updates announce to screen readers:
```html
<!-- Polite updates (toasts) -->
<div id="toast-container" role="region" aria-live="polite"></div>

<!-- Assertive updates (recording state) -->
<div id="recording-announcement" class="sr-only" aria-live="assertive"></div>
```

```typescript
// Update live region
DOM.recordingAnnouncement.textContent = 'Recording started';
```

### Keyboard Navigation
All interactive elements are keyboard-accessible:
- Tab order follows visual order
- Focus states are visible (2px solid var(--accent))
- Escape closes modals/expanders
- Enter/Space activates buttons

---

## Performance Guidelines

### DOM Lookups
- ✅ Cache DOM references in `dom-refs.ts`
- ❌ Don't call `getElementById` in loops or handlers

```typescript
// Good
const btn = DOM.applyOverlayBtn;
btn.addEventListener('click', () => { ... });

// Bad
document.getElementById('apply-overlay-btn')!.addEventListener('click', () => { ... });
```

### Event Listeners
- ✅ Use event delegation for dynamic lists
- ✅ Remove listeners when elements are destroyed
- ❌ Don't attach listeners in tight loops

```typescript
// Good: Event delegation
DOM.historyList.addEventListener('click', (e) => {
  const target = e.target as HTMLElement;
  if (target.classList.contains('history-copy')) {
    copyHistoryItem(target.closest('.history-item'));
  }
});

// Bad: Listener per item
historyItems.forEach(item => {
  item.querySelector('.history-copy').addEventListener('click', () => { ... });
});
```

### Animations
- ✅ Use `transform` for animations (GPU-accelerated)
- ✅ Use `will-change` sparingly (only during animation)
- ❌ Don't animate `width`, `height`, `top`, `left` (triggers layout)

```css
/* Good */
.history-item:hover {
  transform: translateX(2px);
}

/* Bad */
.history-item:hover {
  margin-left: 2px;
}
```

---

## Styling Guidelines

### CSS Organization
1. **Variables** (`:root`) — Design tokens
2. **Reset** (`*`, `body`) — Base styles
3. **Layout** (`.app`, `.layout-grid`) — Structure
4. **Components** (`.panel`, `.field`, `.button`) — UI blocks
5. **Utilities** (`.sr-only`, `.hidden`) — Helpers
6. **Responsive** (`@media`) — Breakpoints
7. **Animations** (`@keyframes`) — Motion

### Class Naming
- **Component**: `.panel`, `.button`, `.field`
- **Modifier**: `.panel-collapsed`, `.button.primary`, `.field.toggle`
- **State**: `[data-state="recording"]`, `.is-active`, `.is-disabled`
- **Utility**: `.sr-only`, `.hidden`, `.span-2`

### BEM-ish (not strict)
```css
.history-item { }
.history-content { }
.history-text { }
.history-meta { }
.history-actions { }
```

---

## TypeScript Patterns

### Type Definitions
All shared types live in `types.ts`:
```typescript
export type RecordingState = 'idle' | 'recording' | 'transcribing';
export type CaptureMode = 'ptt' | 'vad';
export type HistorySource = 'mic-ptt' | 'mic-vad' | 'output';

export interface AudioDevice {
  id: string;
  label: string;
}

export interface HistoryEntry {
  id: string;
  text: string;
  timestamp_ms: number;
  source: HistorySource;
}
```

### Async/Await
All Tauri calls are async:
```typescript
async function loadDevices() {
  try {
    const devices = await invoke<AudioDevice[]>('list_devices');
    renderDevices(devices);
  } catch (error) {
    showToast({
      type: 'error',
      title: 'Device Error',
      message: String(error)
    });
  }
}
```

### Error Handling
Catch Rust errors and show user-friendly toasts:
```typescript
try {
  await invoke('start_recording');
} catch (error) {
  showToast({
    type: 'error',
    title: 'Recording Failed',
    message: error instanceof Error ? error.message : String(error)
  });
}
```

---

## Testing Strategy

### Unit Tests
Test pure functions (no DOM, no Tauri):
```typescript
// src/__tests__/ui-helpers.test.ts
import { describe, it, expect } from 'vitest';
import { formatTimestamp } from '../ui-helpers';

describe('formatTimestamp', () => {
  it('formats milliseconds to HH:MM:SS', () => {
    expect(formatTimestamp(90061000)).toBe('25:01:01');
  });
});
```

### Integration Tests
Test DOM interactions + event handlers:
```typescript
// src/__tests__/history.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { addHistoryEntry, renderHistory } from '../history';

beforeEach(() => {
  document.body.innerHTML = '<div id="history-list"></div>';
});

it('renders history entry', () => {
  addHistoryEntry('Test entry', 'mic-ptt');
  const list = document.getElementById('history-list')!;
  expect(list.children.length).toBe(1);
  expect(list.textContent).toContain('Test entry');
});
```

### Manual Testing
- Test on Windows (WASAPI)
- Test on macOS (ScreenCaptureKit permissions)
- Test keyboard navigation (Tab, Enter, Escape)
- Test screen reader (VoiceOver, NVDA)
- Test with different audio devices
- Test hotkey conflicts

---

## Build & Development

### Dev Server
```bash
npm run tauri dev
```
- Vite dev server with HMR
- Tauri auto-reload on Rust changes

### Production Build
```bash
npm run tauri build
```
- TypeScript compilation
- Vite bundling
- Rust release build
- Platform-specific installers

### Type Checking
```bash
npx tsc --noEmit
```

### Tests
```bash
npm run test
```

---

## Code Review Checklist

### Before Committing
- [ ] No hardcoded colors (use DESIGN_SYSTEM tokens)
- [ ] No hardcoded spacing (use spacing scale)
- [ ] All interactive elements have focus states
- [ ] All inputs have labels (not just placeholders)
- [ ] All buttons have accessible names
- [ ] ARIA labels on icon-only buttons
- [ ] TypeScript types for all function params
- [ ] Error handling for all `invoke` calls
- [ ] DOM references cached in `dom-refs.ts`
- [ ] Event listeners in `event-listeners.ts`
- [ ] No console.log in production code

### Performance
- [ ] No DOM lookups in loops
- [ ] Animations use `transform` not `width/height`
- [ ] Event delegation for dynamic lists
- [ ] Async operations use try/catch

---

## Future Improvements

### Potential Refactors
- Migrate to a typed state management library (Zustand, Valtio)
- Add E2E tests (Playwright, Tauri webdriver)
- Migrate emoji icons to Heroicons/Lucide
- Add CSS-in-TS for component co-location
- Implement virtual scrolling for long history lists

### Optimization Opportunities
- Lazy-load model manager UI (only when panel is opened)
- Debounce range slider updates (avoid spamming Rust backend)
- Use IntersectionObserver for history list virtualization
- Web Worker for heavy audio processing (if needed)

---

**Last updated**: 2026-02-06
**Version**: 1.0

