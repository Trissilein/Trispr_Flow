# State Management — UI States

> Dokumentation für Loading, Error, und Empty States in Trispr Flow.

---

## 🔄 Loading States

### Button Loading

**Use Case**: Model-Download, Apply-Settings, Transcription-Start

```html
<!-- Before operation -->
<button id="download-btn" class="button primary">Download Model</button>

<!-- During operation (add .is-loading class via JS) -->
<button id="download-btn" class="button primary is-loading">Download Model</button>
```

**JavaScript Example**:
```typescript
const downloadBtn = document.getElementById("download-btn");

async function downloadModel() {
  downloadBtn?.classList.add("is-loading");
  downloadBtn?.setAttribute("disabled", "true");

  try {
    await invoke("download_model", { url: modelUrl });
  } finally {
    downloadBtn?.classList.remove("is-loading");
    downloadBtn?.removeAttribute("disabled");
  }
}
```

---

### Content Loading (Shimmer)

**Use Case**: Model-List wird geladen, History wird abgerufen

```html
<!-- Container während Laden -->
<div class="model-list is-loading">
  <!-- Existing content wird mit Shimmer überlagert -->
</div>
```

**JavaScript Example**:
```typescript
const modelList = document.getElementById("model-list-active");

async function refreshModels() {
  modelList?.classList.add("is-loading");

  try {
    const models = await invoke<ModelInfo[]>("list_models");
    renderModels(models);
  } finally {
    modelList?.classList.remove("is-loading");
  }
}
```

---

### Model-Specific Loading

**Use Case**: Model wird heruntergeladen, aber andere Interaktionen bleiben möglich

```html
<div class="model-item is-loading">
  <div class="model-info">
    <div class="model-name">tiny.en</div>
    <div class="model-size">75 MB</div>
  </div>
  <div class="model-actions">
    <button>Cancel</button>
  </div>
</div>
```

**CSS-Verhalten**:
- `.is-loading` → opacity: 0.6, shimmer overlay
- `.model-actions` → pointer-events: none (Buttons disabled während Download)

---

## ❌ Error States

### Input Validation Errors

**Use Case**: Ungültige Model-URL, ungültiger Storage-Path

```html
<!-- Normal state -->
<label class="field">
  <span class="field-label">Custom model URL</span>
  <input id="model-custom-url" type="text" placeholder="https://..." />
</label>

<!-- Error state (add .error class + error message) -->
<label class="field error">
  <span class="field-label">Custom model URL</span>
  <input
    id="model-custom-url"
    type="text"
    placeholder="https://..."
    aria-invalid="true"
    aria-describedby="model-url-error" />
  <span id="model-url-error" class="field-error">
    Invalid URL format. Must start with https://
  </span>
</label>
```

**JavaScript Example**:
```typescript
function validateModelUrl(url: string): boolean {
  const isValid = /^https:\/\/.+/.test(url);

  const field = document.querySelector("#model-custom-url")?.closest(".field");
  const input = document.getElementById("model-custom-url") as HTMLInputElement;
  const errorMsg = document.getElementById("model-url-error");

  if (!isValid) {
    field?.classList.add("error");
    input?.setAttribute("aria-invalid", "true");
    if (errorMsg) {
      errorMsg.textContent = "Invalid URL format. Must start with https://";
    }
    return false;
  } else {
    field?.classList.remove("error");
    input?.removeAttribute("aria-invalid");
    if (errorMsg) {
      errorMsg.textContent = "";
    }
    return true;
  }
}

// Usage
customUrlInput?.addEventListener("blur", (e) => {
  const value = (e.target as HTMLInputElement).value;
  if (value) {
    validateModelUrl(value);
  }
});
```

---

### Toggle Error State

**Use Case**: Konflikt-Warnung (z.B. PTT + VAD gleichzeitig nicht erlaubt)

```html
<label class="field toggle error">
  <span class="field-label">Use Voice Activation in PTT</span>
  <input type="checkbox" id="ptt-use-vad-toggle" />
  <span class="toggle-track">
    <span class="toggle-thumb"></span>
  </span>
  <span class="field-error">
    Cannot use VAD in PTT mode. Disable PTT first.
  </span>
</label>
```

---

## 📭 Empty States

### Empty Model List

**Use Case**: Keine Modelle installiert

```html
<div id="model-list-active" class="model-list">
  <!-- Falls leer, JS fügt hinzu: -->
  <div class="empty-state">
    <div class="empty-state-icon">📦</div>
    <div class="empty-state-text">No active model</div>
    <div class="empty-state-hint">Download a model from the "Available" section to start transcribing</div>
  </div>
</div>
```

**JavaScript Example**:
```typescript
function renderActiveModels(models: ModelInfo[]) {
  const container = document.getElementById("model-list-active");
  if (!container) return;

  if (models.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <div class="empty-state-icon">📦</div>
        <div class="empty-state-text">No active model</div>
        <div class="empty-state-hint">Download a model from the "Available" section to start transcribing</div>
      </div>
    `;
    return;
  }

  // Render models...
  container.innerHTML = models.map(model => `...`).join("");
}
```

---

### Empty History

**Use Case**: Keine Transkripte vorhanden

```html
<div id="history-list" class="history-list">
  <!-- Falls leer (aktuell inline in history.ts implementiert): -->
  <div class="empty-state compact">
    <div class="empty-state-icon">🎤</div>
    <div class="empty-state-text">No transcripts yet</div>
    <div class="empty-state-hint">Start dictating to build your input history</div>
  </div>
</div>
```

**Compact Variant**: Für kleinere Panels (History, Capture-Logs)

---

### Empty Conversation View

**Use Case**: Keine Input/Output-Entries vorhanden für Conversation

```html
<div id="history-list" class="history-list">
  <div class="empty-state">
    <div class="empty-state-icon">💬</div>
    <div class="empty-state-text">No conversation yet</div>
    <div class="empty-state-hint">Build input or output entries to generate the conversation view</div>
  </div>
</div>
```

---

## 🎨 Visual Reference

### Loading States
```
┌─────────────────────────────────┐
│  [⟳ Spinner]                    │  ← button.is-loading
└─────────────────────────────────┘

┌─────────────────────────────────┐
│  ╱╱╱╱ Shimmer animation ╲╲╲╲    │  ← .is-loading (content)
└─────────────────────────────────┘
```

### Error States
```
┌─────────────────────────────────┐
│  Custom model URL               │
│  ┌───────────────────────────┐  │
│  │ https://invalid          │  │  ← Red border
│  └───────────────────────────┘  │
│  ⚠ Invalid URL format           │  ← .field-error
└─────────────────────────────────┘
```

### Empty States
```
┌─────────────────────────────────┐
│            📦                    │
│      No active model             │
│  Download a model to start...    │
└─────────────────────────────────┘
```

---

## 📍 Implementation Checklist

### Phase 1: Core Components ✅
- [x] CSS für Loading States
- [x] CSS für Error States
- [x] CSS für Empty States

### Phase 2: Integration (To-Do)
- [ ] Model Manager:
  - [ ] Download-Button → `.is-loading` während Download
  - [ ] Model-List → `.is-loading` während Refresh
  - [ ] Empty-State für leere Available-Liste
  - [ ] Empty-State für leere Active-Liste
  - [ ] URL-Validation → `.field.error` für Custom-URL

- [ ] History:
  - [ ] Empty-State für Input-Tab (aktuell text-only)
  - [ ] Empty-State für Output-Tab
  - [ ] Empty-State für Conversation-Tab
  - [ ] Loading-State während History-Load

- [ ] Settings:
  - [ ] Apply-Button → `.is-loading` während Save
  - [ ] Storage-Path-Validation → `.field.error`
  - [ ] Hotkey-Conflict-Detection → `.field.error`

- [ ] Audio Capture:
  - [ ] Device-Selection → `.is-loading` während Device-Enumeration
  - [ ] Error-State für fehlende Permissions

---

## 🧪 Testing Checklist

- [ ] **Loading States**:
  - [ ] Button-Spinner animiert smooth (0.6s rotation)
  - [ ] Shimmer-Effect läuft flüssig (2s loop)
  - [ ] Disabled während Loading (pointer-events: none)
  - [ ] Loading-State wird entfernt nach Completion/Error

- [ ] **Error States**:
  - [ ] Error-Border ist sichtbar (#f87171)
  - [ ] Error-Message erscheint mit Icon (⚠)
  - [ ] aria-invalid="true" gesetzt
  - [ ] Error wird gelöscht nach Korrektur

- [ ] **Empty States**:
  - [ ] Icon, Text, Hint zentriert
  - [ ] Dashed-Border sichtbar
  - [ ] Compact-Variant funktioniert in kleinen Panels
  - [ ] Text ist hilfreich (nicht nur "No data")

---

**Last updated**: 2026-02-06
