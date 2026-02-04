# Trispr Flow – Web Accessibility Audit (WCAG 2.1 AA)

**Date:** 2026-02-04
**Scope:** Frontend UI (index.html, main.ts, styles.css)
**Standard:** WCAG 2.1 Level AA
**Status:** 🟡 Partial Compliance

---

## Executive Summary

**Current Compliance:** ~65% (Estimated)
**Critical Issues:** 8 findings
**High Priority:** 12 findings
**Medium Priority:** 6 findings

**Overall Assessment:**
- ✅ **Good:** Semantic HTML structure, proper `<label>` usage
- ⚠️ **Needs Work:** ARIA labels, live regions, keyboard navigation, color contrast
- ❌ **Missing:** Screen reader announcements, focus management

---

## Critical Issues (Must Fix)

### 1. Status Indicators Lack ARIA Labels
**Severity:** CRITICAL
**WCAG:** 1.3.1 Info and Relationships, 4.1.2 Name, Role, Value
**Location:** [index.html:26-32](index.html#L26-L32)

**Issue:**
```html
<div class="status">
  <span class="status-dot" id="status-dot" data-state="idle"></span>
  <span id="status-label">Idle</span>
  <span class="status-divider">•</span>
  <span id="engine-label">whisper.cpp (GPU)</span>
  <span id="status-message" class="status-message"></span>
</div>
```

Recording state is **visual only** (color dot) without screen reader support.

**Fix:**
```html
<div class="status" role="status" aria-live="polite">
  <span class="status-dot" id="status-dot" data-state="idle"
        aria-label="Recording status: idle"></span>
  <span id="status-label">Idle</span>
  <span class="status-divider" aria-hidden="true">•</span>
  <span id="engine-label">Engine: whisper.cpp (GPU)</span>
  <span id="status-message" class="status-message" role="alert"></span>
</div>
```

**JavaScript Update (main.ts):**
```typescript
function updateStatus(state: string) {
  const dot = document.getElementById('status-dot');
  const label = document.getElementById('status-label');
  dot?.setAttribute('data-state', state);
  dot?.setAttribute('aria-label', `Recording status: ${state}`);
  label.textContent = state.charAt(0).toUpperCase() + state.slice(1);
}
```

---

### 2. Toast Notifications Missing ARIA Live Region
**Severity:** CRITICAL
**WCAG:** 4.1.3 Status Messages
**Location:** [index.html:16](index.html#L16)

**Issue:**
```html
<div id="toast-container" class="toast-container"></div>
```

Toasts appear without screen reader announcement.

**Fix:**
```html
<div id="toast-container" class="toast-container"
     role="region"
     aria-live="polite"
     aria-label="Notifications"></div>
```

**JavaScript Update (main.ts):**
```typescript
function showToast(message: string, type: 'error' | 'success' | 'info') {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  toast.setAttribute('role', 'alert'); // Each toast is an alert
  toast.textContent = message;
  toastContainer.appendChild(toast);
  // ... rest of logic
}
```

---

### 3. Range Sliders Missing ARIA Attributes
**Severity:** CRITICAL
**WCAG:** 4.1.2 Name, Role, Value
**Location:** [index.html:146](index.html#L146), [index.html:153](index.html#L153), etc.

**Issue:**
```html
<input id="vad-threshold" type="range" min="0" max="100" step="1" />
<span id="vad-threshold-value" class="range-value">0%</span>
```

Screen readers can't announce current value or range.

**Fix:**
```html
<input id="vad-threshold" type="range"
       min="0" max="100" step="1"
       aria-valuemin="0"
       aria-valuemax="100"
       aria-valuenow="50"
       aria-label="Voice activation threshold"
       aria-describedby="vad-threshold-value" />
<span id="vad-threshold-value" class="range-value" aria-live="polite">50%</span>
```

**Apply to all range inputs:**
- `#vad-silence` (line 153)
- `#mic-gain` (line 176)
- `#audio-cues-volume` (line 191)
- `#conversation-font-size` (line 84)

---

### 4. Hotkey Status Messages Not Announced
**Severity:** HIGH
**WCAG:** 4.1.3 Status Messages
**Location:** [index.html:126](index.html#L126), [index.html:134](index.html#L134)

**Issue:**
```html
<span id="ptt-hotkey-status" class="hotkey-status"></span>
```

Hotkey validation errors/success not announced to screen readers.

**Fix:**
```html
<span id="ptt-hotkey-status" class="hotkey-status"
      role="status"
      aria-live="polite"></span>
```

---

### 5. VAD Meter Lacks Accessible Description
**Severity:** HIGH
**WCAG:** 1.1.1 Non-text Content
**Location:** [index.html:159-171](index.html#L159-L171)

**Issue:**
Visual-only meter with no text alternative.

**Fix:**
```html
<div class="field">
  <span class="field-label">
    Input level
    <span id="vad-level-dbm" class="dbm-value" aria-live="polite">-∞ dB</span>
  </span>
  <div class="vad-meter-container"
       role="img"
       aria-label="Audio input level meter"
       aria-describedby="vad-level-dbm">
    <div id="vad-meter" class="vad-meter">
      <div id="vad-meter-fill" class="vad-meter-fill"
           role="progressbar"
           aria-valuemin="-60"
           aria-valuemax="0"
           aria-valuenow="-60"
           aria-label="Current audio level"></div>
      <!-- markers remain visual only -->
    </div>
    <!-- scale remains visual only, described by aria-describedby -->
  </div>
</div>
```

---

### 6. History Tabs Missing ARIA Tablist Pattern
**Severity:** HIGH
**WCAG:** 4.1.2 Name, Role, Value
**Location:** [index.html:74-78](index.html#L74-L78)

**Issue:**
```html
<div class="history-tabs">
  <button id="history-tab-mic" class="history-tab active">Microphone</button>
  <button id="history-tab-system" class="history-tab">System Audio</button>
  <button id="history-tab-conversation" class="history-tab">Conversation</button>
</div>
```

Tabs don't follow WAI-ARIA tabs pattern.

**Fix:**
```html
<div class="history-tabs" role="tablist" aria-label="History view selector">
  <button id="history-tab-mic" class="history-tab"
          role="tab"
          aria-selected="true"
          aria-controls="history-panel-mic">Microphone</button>
  <button id="history-tab-system" class="history-tab"
          role="tab"
          aria-selected="false"
          aria-controls="history-panel-system">System Audio</button>
  <button id="history-tab-conversation" class="history-tab"
          role="tab"
          aria-selected="false"
          aria-controls="history-panel-conversation">Conversation</button>
</div>
<div id="history-panel-mic" role="tabpanel" aria-labelledby="history-tab-mic">
  <!-- history content -->
</div>
```

**Keyboard Navigation (main.ts):**
- Arrow Left/Right: Switch tabs
- Home/End: First/Last tab
- Tab: Move focus out of tablist

---

### 7. Panel Collapse Buttons Need Better Labels
**Severity:** MEDIUM
**WCAG:** 2.4.6 Headings and Labels
**Location:** [index.html:70](index.html#L70), [index.html:103](index.html#L103), etc.

**Issue:**
```html
<button class="panel-collapse-btn" data-panel-collapse="output" title="Collapse">▾</button>
```

`title` attribute alone is insufficient for screen readers.

**Fix:**
```html
<button class="panel-collapse-btn"
        data-panel-collapse="output"
        aria-label="Collapse Output panel"
        aria-expanded="true">
  <span aria-hidden="true">▾</span>
</button>
```

**JavaScript Update (main.ts):**
```typescript
function togglePanel(panelId: string) {
  const button = document.querySelector(`[data-panel-collapse="${panelId}"]`);
  const isExpanded = button?.getAttribute('aria-expanded') === 'true';
  button?.setAttribute('aria-expanded', String(!isExpanded));
  button?.setAttribute('aria-label',
    isExpanded ? `Expand ${panelId} panel` : `Collapse ${panelId} panel`);
  // ... rest of collapse logic
}
```

---

### 8. Overlay Window Lacks Accessible Alternative
**Severity:** HIGH
**WCAG:** 1.1.1 Non-text Content
**Location:** overlay.html (entire file)

**Issue:**
Recording indicator is **visual only** (floating dot/KITT bar) with no screen reader equivalent.

**Fix:**
Add hidden status announcement in main app:

```html
<!-- In index.html, near status area -->
<div id="recording-announcement"
     class="sr-only"
     role="status"
     aria-live="assertive"
     aria-atomic="true"></div>
```

```typescript
// In main.ts
function announceRecordingState(state: 'idle' | 'recording' | 'transcribing') {
  const announcer = document.getElementById('recording-announcement');
  const messages = {
    idle: 'Recording stopped',
    recording: 'Recording started',
    transcribing: 'Transcribing audio'
  };
  announcer.textContent = messages[state];
}
```

---

## High Priority Issues

### 9. Language Attribute Missing on Some Dynamic Content
**WCAG:** 3.1.1 Language of Page
**Fix:** Add `lang` attributes to German/English transcripts if mixed

### 10. Focus Indicators Not Visible on All Elements
**WCAG:** 2.4.7 Focus Visible
**Location:** CSS (styles.css)

**Current Issue:** Some interactive elements (hotkey record buttons, panel collapse) may not have visible focus.

**Fix (styles.css):**
```css
/* Ensure all interactive elements have visible focus */
button:focus-visible,
input:focus-visible,
select:focus-visible {
  outline: 2px solid var(--accent-primary);
  outline-offset: 2px;
}

/* Special focus for hotkey record buttons */
.hotkey-record-btn:focus-visible {
  outline: 2px solid var(--accent-primary);
  outline-offset: 2px;
  background: rgba(255, 61, 46, 0.1);
}
```

### 11. Insufficient Color Contrast on Status Messages
**WCAG:** 1.4.3 Contrast (Minimum)
**Severity:** HIGH
**Location:** Check with color picker

**Action Required:**
1. Use browser DevTools or WebAIM Contrast Checker
2. Check status-dot colors against background
3. Check all text against backgrounds (minimum 4.5:1 for normal text, 3:1 for large)

**Likely Issues:**
- Gray text on gray backgrounds
- Status colors (red, green, yellow) may not meet contrast

**Fix Example:**
```css
/* Ensure sufficient contrast */
.status-message {
  color: #ffffff; /* White on dark bg */
}

.status-dot[data-state="idle"] {
  background: #888888; /* Ensure 3:1 against bg */
}

.status-dot[data-state="recording"] {
  background: #ff3d2e; /* Bright enough for visibility */
}
```

### 12. Keyboard Navigation Incomplete
**WCAG:** 2.1.1 Keyboard
**Severity:** HIGH

**Issues:**
- History tabs don't support arrow key navigation
- Panel collapse buttons work but could have keyboard shortcuts
- Hotkey recording doesn't announce capturing state

**Fixes:**
See Tab pattern implementation in Issue #6

---

## Medium Priority Issues

### 13. Missing Skip Links
**WCAG:** 2.4.1 Bypass Blocks
**Recommendation:** Add skip-to-content links

```html
<body>
  <a href="#main-content" class="skip-link">Skip to main content</a>
  <a href="#capture-panel" class="skip-link">Skip to microphone settings</a>
  <!-- ... -->
  <main id="main-content" class="app">
```

```css
.skip-link {
  position: absolute;
  top: -40px;
  left: 0;
  background: var(--bg-primary);
  color: var(--text-primary);
  padding: 8px;
  z-index: 100;
}

.skip-link:focus {
  top: 0;
}
```

### 14. Form Validation Errors Not Associated with Inputs
**WCAG:** 3.3.1 Error Identification
**Location:** Hotkey validation (main.ts)

**Fix:**
Use `aria-describedby` to link errors to inputs:

```html
<input id="ptt-hotkey"
       type="text"
       aria-describedby="ptt-hotkey-status"
       aria-invalid="false" />
<span id="ptt-hotkey-status" class="hotkey-status" role="alert"></span>
```

```typescript
function validateHotkey(input: HTMLInputElement, status: HTMLElement) {
  if (invalid) {
    input.setAttribute('aria-invalid', 'true');
    status.textContent = 'Error: Hotkey already in use';
    status.className = 'hotkey-status error';
  } else {
    input.setAttribute('aria-invalid', 'false');
    status.textContent = 'Valid hotkey';
    status.className = 'hotkey-status success';
  }
}
```

### 15. Heading Hierarchy May Skip Levels
**WCAG:** 1.3.1 Info and Relationships
**Check:** Ensure no h1 → h3 jumps (should be h1 → h2 → h3)

Currently:
- h1: "Trispr Flow" (line 21)
- h2: Panel titles ("Output", "Capture Microphone", etc.)
- Appears correct, but verify no h4/h5 without h3

### 16. Dynamic Content Changes Not Announced
**WCAG:** 4.1.3 Status Messages
**Examples:**
- Model download progress
- Transcription completion
- Device connection status

**Fix:** Use `aria-live="polite"` regions for status updates

### 17. Audio Cue Volume Slider Needs Better Description
**WCAG:** 2.4.6 Headings and Labels
**Current:** "Audio cue volume"
**Better:** "Audio cue volume (0-100%)"

### 18. Icon-Only Buttons Need Text Alternatives
**WCAG:** 1.1.1 Non-text Content
**Example:** Emoji buttons (🎹 Record)

**Fix:**
```html
<button id="ptt-hotkey-record"
        class="hotkey-record-btn"
        aria-label="Record PTT hotkey">
  <span aria-hidden="true">🎹</span> Record
</button>
```

---

## Low Priority / Enhancements

### 19. Add Landmark Roles (Enhancement)
While semantic HTML provides implicit roles, explicit ARIA landmarks improve navigation:

```html
<header role="banner">
<nav role="navigation" aria-label="Main navigation">
<main role="main">
<footer role="contentinfo">
```

### 20. Add Loading States
When downloading models or transcribing, add:
```html
<div role="status" aria-live="polite" aria-busy="true">
  Downloading model... 50%
</div>
```

---

## Testing Recommendations

### Automated Testing
1. **axe DevTools** (browser extension)
   - Install: https://www.deque.com/axe/devtools/
   - Run on index.html
   - Check for WCAG AA violations

2. **WAVE** (WebAIM)
   - https://wave.webaim.org/
   - Paste index.html or use extension

3. **Lighthouse** (Chrome DevTools)
   ```bash
   npm run build
   # Open app in Chrome
   # DevTools → Lighthouse → Accessibility audit
   ```

### Manual Testing Checklist
- [ ] **Keyboard Only:** Navigate entire app with Tab, Enter, Escape, Arrows
- [ ] **Screen Reader:** Test with NVDA (Windows) or JAWS
  - Can you understand recording state?
  - Are form errors announced?
  - Can you navigate history tabs?
- [ ] **Color Contrast:** Use DevTools to check all text
- [ ] **Zoom:** Test at 200% zoom (WCAG 1.4.4 Resize Text)
- [ ] **Focus Indicators:** Verify visible on all interactive elements

---

## Implementation Priority

### Week 1 (Critical)
- [ ] Issue #1: Status indicators ARIA labels
- [ ] Issue #2: Toast notifications aria-live
- [ ] Issue #3: Range sliders ARIA attributes
- [ ] Issue #4: Hotkey status announcements
- [ ] Issue #8: Overlay accessible alternative

### Week 2 (High)
- [ ] Issue #6: History tabs ARIA pattern
- [ ] Issue #7: Panel collapse button labels
- [ ] Issue #10: Focus indicators CSS
- [ ] Issue #11: Color contrast audit
- [ ] Issue #12: Keyboard navigation

### Week 3 (Medium + Polish)
- [ ] Issue #13-18: Form validation, skip links, dynamic content
- [ ] Automated testing with axe + WAVE
- [ ] Screen reader testing
- [ ] Documentation update

---

## Code Examples Summary

### CSS Additions (styles.css)
```css
/* Screen reader only (for announcements) */
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border-width: 0;
}

/* Skip links */
.skip-link {
  position: absolute;
  top: -40px;
  left: 0;
  background: var(--bg-primary);
  color: var(--text-primary);
  padding: 8px;
  z-index: 100;
  transition: top 0.2s;
}

.skip-link:focus {
  top: 0;
}

/* Enhanced focus indicators */
button:focus-visible,
input:focus-visible,
select:focus-visible {
  outline: 2px solid var(--accent-primary);
  outline-offset: 2px;
}
```

### TypeScript Helper (main.ts)
```typescript
// Announce status changes to screen readers
function announceToScreenReader(message: string, priority: 'polite' | 'assertive' = 'polite') {
  const announcer = document.getElementById('recording-announcement');
  if (!announcer) return;

  announcer.setAttribute('aria-live', priority);
  announcer.textContent = message;

  // Clear after announcement
  setTimeout(() => {
    announcer.textContent = '';
  }, 1000);
}

// Update range slider ARIA attributes
function updateRangeAria(input: HTMLInputElement, value: number) {
  input.setAttribute('aria-valuenow', String(value));
  const valueDisplay = document.getElementById(`${input.id}-value`);
  if (valueDisplay) {
    valueDisplay.textContent = formatValue(value, input.id);
  }
}

// Tab keyboard navigation
function handleTabKeyboard(event: KeyboardEvent) {
  const tabs = Array.from(document.querySelectorAll('[role="tab"]'));
  const currentIndex = tabs.findIndex(tab => tab === event.target);

  switch (event.key) {
    case 'ArrowLeft':
      const prevIndex = currentIndex === 0 ? tabs.length - 1 : currentIndex - 1;
      (tabs[prevIndex] as HTMLElement).focus();
      event.preventDefault();
      break;
    case 'ArrowRight':
      const nextIndex = currentIndex === tabs.length - 1 ? 0 : currentIndex + 1;
      (tabs[nextIndex] as HTMLElement).focus();
      event.preventDefault();
      break;
    case 'Home':
      (tabs[0] as HTMLElement).focus();
      event.preventDefault();
      break;
    case 'End':
      (tabs[tabs.length - 1] as HTMLElement).focus();
      event.preventDefault();
      break;
  }
}
```

---

## Success Metrics

**Goal:** WCAG 2.1 AA Compliance ≥ 95%

### Current (Estimated):
- **Perceivable:** 60% (missing ARIA labels, text alternatives)
- **Operable:** 70% (keyboard nav exists but incomplete)
- **Understandable:** 75% (clear labels, but missing error associations)
- **Robust:** 60% (missing ARIA roles/states)

### Target (After Fixes):
- **Perceivable:** 95%+ (all content has text alternative)
- **Operable:** 98%+ (full keyboard navigation, focus management)
- **Understandable:** 95%+ (clear labels, error messages linked to inputs)
- **Robust:** 95%+ (proper ARIA roles, states, properties)

---

## References

- **WCAG 2.1 Quick Reference:** https://www.w3.org/WAI/WCAG21/quickref/
- **WAI-ARIA Authoring Practices:** https://www.w3.org/WAI/ARIA/apg/
- **WebAIM Contrast Checker:** https://webaim.org/resources/contrastchecker/
- **axe DevTools:** https://www.deque.com/axe/devtools/
- **NVDA Screen Reader:** https://www.nvaccess.org/download/

---

**Report Generated:** 2026-02-04
**Next Review:** After Week 1 critical fixes implemented
**Auditor:** Claude Code Skills & Agents Framework
