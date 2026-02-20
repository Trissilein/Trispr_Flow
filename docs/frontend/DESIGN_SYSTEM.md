# Design System — Trispr Flow

> Visual language for Trispr Flow: offline dictation with a privacy-first GPU-accelerated transcription pipeline.

---

## Color Palette

### Base Colors
```css
--ink: #e8e8ea           /* Primary text */
--ink-soft: #d1d5db      /* Secondary text */
--muted: #9ca3af         /* Tertiary text / subtle labels */
```

### Accent Colors
```css
--accent: #ff6b3d        /* Primary accent (orange-red) */
--accent-2: #1da6a0      /* Secondary accent (teal) - used for toggles, active states */
--accent-3: #f5b342      /* Tertiary accent (gold) - used for transcribing state */
```

### Surface Colors
```css
--card: rgba(30, 35, 42, 0.95)       /* Card backgrounds */
--card-hover: rgba(35, 40, 47, 1)    /* Card hover state */
--panel: rgba(40, 46, 55, 0.96)      /* Panel backgrounds */
```

### Stroke Colors
```css
--stroke: rgba(255, 255, 255, 0.1)   /* Subtle borders */
--stroke-strong: rgba(255, 255, 255, 0.15) /* Prominent borders */
```

### Semantic Colors
```css
/* Success / Enabled */
background: rgba(29, 166, 160, 0.2)
color: #4be0d4
border: rgba(29, 166, 160, 0.35)

/* Error / Disabled / Warning */
background: rgba(239, 68, 68, 0.2)
color: #f87171
border: rgba(239, 68, 68, 0.35)

/* Warning / Active (gold) */
background: rgba(245, 179, 66, 0.2)
color: #f5b342
```

---

## Typography

### Font Families
```css
--font-sans: "IBM Plex Sans", "Segoe UI", sans-serif
--font-display: "Space Grotesk", "IBM Plex Sans", sans-serif
--font-mono: "Space Grotesk", monospace
```

### Type Scale
```css
/* Display */
H1: 1.75rem (28px), weight: 700, line-height: 1.1, font: Space Grotesk

/* Headings */
H2 (Panel titles): 0.95rem (15.2px), weight: 700, font: Space Grotesk
H3 (Model sections): 0.9rem (14.4px), weight: 700, font: Space Grotesk

/* Body */
Body: 1rem (16px), line-height: 1.5
Subtitle: 0.85rem (13.6px), line-height: 1.4, color: muted

/* Small */
Field labels: 0.75rem (12px), weight: 600, uppercase, letter-spacing: 0.04em
Button text: 0.8rem (12.8px), weight: 600
Meta text: 0.72rem (11.5px), color: muted

/* Tiny */
Badge: 0.65rem (10.4px), weight: 600, uppercase
Status pill: 0.6rem (9.6px), weight: 700, uppercase
Hero value: 10px, weight: 400
Model status: 0.7rem (11.2px), weight: 600, uppercase
```

### Font Weights
```css
Regular: 400
Medium: 500
Semibold: 600
Bold: 700
Extra Bold: 800
```

### Line Heights
```css
Display: 1.1
Headings: 1.2-1.3
Body: 1.5
Labels: 1.3-1.4
Tight: 1.2
```

---

## Spacing Scale

### Base Grid
```css
4px base unit
```

### Spacing Values
```css
1px   /* Separator lines */
2px   /* Badges, small gaps */
3px   /* Marker width */
4px   /* Padding micro */
6px   /* Gap small */
8px   /* Gap medium, padding small */
10px  /* Gap standard, padding medium */
12px  /* Gap large, padding large */
14px  /* Padding card */
16px  /* Padding section */
20px  /* Padding hero */
24px  /* Padding expander */
32px  /* Gap extra-large */
48px  /* Section spacing */
64px  /* Hero spacing */
```

### Component-Specific Spacing
```css
/* Panels */
Panel padding: 12px 14px 14px
Panel gap: 12px
Panel-header margin-bottom: 10px
Panel-header padding-bottom: 8px

/* Layout grid */
Grid gap: 12px

/* Cards */
Card padding: 10px 12px
Hero-card padding: 10px 12px
Hero-card gap: 10px 14px

/* Fields */
Field gap: 6px
Panel-grid gap: 10px 12px

/* History */
History-item padding: 10px 12px
History gap: 6px
```

---

## Border Radius

```css
--radius: 10px       /* Primary radius (cards, panels, inputs) */
--radius-sm: 6px     /* Small radius (buttons, expanders) */
--radius-pill: 999px /* Pills, toggles, badges */
```

---

## Shadows

```css
--shadow: 0 4px 16px rgba(0, 0, 0, 0.3)  /* Primary shadow for panels/cards */

/* Component-specific shadows */
Toast: 0 8px 24px rgba(0, 0, 0, 0.4)
Range thumb: 0 2px 4px rgba(0, 0, 0, 0.3)
Toggle thumb: 0 1px 3px rgba(0, 0, 0, 0.3)
Status dot (recording): 0 0 0 4px rgba(29, 166, 160, 0.25)
Status dot (transcribing): 0 0 0 4px rgba(245, 179, 66, 0.25)
Threshold marker: 0 0 6px rgba(255, 107, 61, 0.6)
```

---

## Component Patterns

### Buttons

**Primary Button**
```css
background: var(--accent-2)  /* #1da6a0 */
color: #fff
border: 1px solid var(--accent-2)
padding: 8px 14px
border-radius: var(--radius-sm)
font-size: 0.8rem
font-weight: 600

hover:
  background: #1bb8b1
  border-color: #1bb8b1
  transform: translateY(-1px)
```

**Secondary Button (default)**
```css
background: rgba(255, 255, 255, 0.05)
color: var(--ink)
border: 1px solid var(--stroke-strong)
padding: 8px 14px
border-radius: var(--radius-sm)
font-size: 0.8rem
font-weight: 600

hover:
  background: rgba(255, 255, 255, 0.08)
  border-color: rgba(255, 255, 255, 0.25)
  transform: translateY(-1px)
```

**Danger Button**
```css
background: rgba(239, 68, 68, 0.15)
color: #f87171
border: 1px solid rgba(239, 68, 68, 0.3)

hover:
  background: rgba(239, 68, 68, 0.25)
  border-color: rgba(239, 68, 68, 0.5)
```

**Ghost Button**
```css
border: 1px dashed var(--stroke-strong)
background: transparent
color: var(--ink-soft)
padding: 6px 10px
border-radius: 8px
font-size: 0.72rem
font-weight: 600

hover:
  color: #fff
  border-color: rgba(255, 255, 255, 0.3)
  background: rgba(255, 255, 255, 0.04)
```

### Inputs & Selects

```css
padding: 8px 10px
border: 1px solid var(--stroke-strong)
border-radius: var(--radius-sm)
background: rgba(255, 255, 255, 0.03)
color: var(--ink)
font-size: 0.85rem

hover:
  border-color: rgba(255, 255, 255, 0.2)
  background: rgba(255, 255, 255, 0.04)

focus:
  outline: none
  border-color: var(--accent-2)
  background: rgba(255, 255, 255, 0.05)
  box-shadow: 0 0 0 3px rgba(29, 166, 160, 0.15)
```

### Toggle Switch

```css
Track:
  width: 38px
  height: 20px
  background: rgba(255, 255, 255, 0.1)
  border: 1px solid var(--stroke)
  border-radius: 999px

Thumb:
  width: 14px
  height: 14px
  background: #fff
  border-radius: 50%
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3)
  position: absolute, top: 2px, left: 2px

checked:
  background: var(--accent-2)
  border-color: var(--accent-2)
  thumb: translateX(18px)
```

### Range Slider

```css
Track:
  height: 4px
  background: rgba(255, 255, 255, 0.1)
  border-radius: 2px

Thumb:
  width: 14px
  height: 14px
  background: var(--accent-2)
  border: 2px solid #1a1f24
  border-radius: 50%
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3)
```

### Cards & Panels

```css
Panel:
  background: var(--panel)
  border-radius: var(--radius)
  border: 1px solid var(--stroke-strong)
  box-shadow: var(--shadow)
  padding: 12px 14px 14px

Card:
  background: var(--card)
  border-radius: var(--radius)
  border: 1px solid var(--stroke-strong)
  padding: 10px 12px

hover:
  background: var(--card-hover) or rgba(255, 255, 255, 0.05)
  border-color: rgba(255, 255, 255, 0.15)
```

### Status Pills

```css
Enabled:
  background: rgba(29, 166, 160, 0.2)
  color: #4be0d4
  border: 1px solid rgba(29, 166, 160, 0.4)

Disabled:
  background: rgba(239, 68, 68, 0.2)
  color: #f87171
  border: 1px solid rgba(239, 68, 68, 0.4)

Base style:
  padding: 2px 8px
  border-radius: 999px
  font-size: 0.6rem
  font-weight: 700
  uppercase
  letter-spacing: 0.04em
```

### Badges

```css
Default (teal):
  background: rgba(29, 166, 160, 0.2)
  color: #4be0d4
  border: 1px solid rgba(29, 166, 160, 0.35)

Online (red):
  background: rgba(239, 68, 68, 0.2)
  color: #f87171
  border: 1px solid rgba(239, 68, 68, 0.35)

Base style:
  padding: 3px 10px
  border-radius: 999px
  font-size: 0.65rem
  font-weight: 600
  uppercase
  letter-spacing: 0.03em
```

---

## Transitions & Animations

### Timing Functions
```css
ease: default easing
ease-in-out: smooth
linear: meters, progress bars
```

### Durations
```css
80ms: VAD meter fill
150ms: hover states, color changes, borders
200ms: toggle switch, panel collapse, button transform
300ms: toasts, model progress bar
600ms: rise animation (page load)
1500ms: pulse animation
```

### Keyframes

**Rise (page load)**
```css
from: opacity 0, translateY(20px)
to: opacity 1, translateY(0)
duration: 0.6s ease
```

**Pulse (recording status)**
```css
0%, 100%: opacity 1
50%: opacity 0.5
duration: 1.5s ease-in-out infinite
```

**Slide-in (toast)**
```css
from: translateX(100px), opacity 0
to: translateX(0), opacity 1
duration: 0.3s ease
```

**Slide-out (toast)**
```css
from: translateX(0) scale(1), opacity 1
to: translateX(400px) scale(0.95), opacity 0
duration: 0.3s ease
```

---

## Accessibility Standards

### Focus States
All interactive elements have visible focus indicators:
```css
outline: 2px solid var(--accent)  /* #ff6b3d */
outline-offset: 2px
```

Applied to: buttons, inputs, selects, textareas, links, range sliders, tabs

### Contrast Ratios (WCAG 2.1 AA)
- Primary text (#e8e8ea on #1a1f24): ~12:1 ✓
- Secondary text (#d1d5db on #1a1f24): ~10:1 ✓
- Muted text (#9ca3af on #1a1f24): ~5.5:1 ✓
- Accent teal (#1da6a0): Sufficient for UI elements ✓

### ARIA Labels
- All buttons have aria-label or visible text
- All inputs have labels (not just placeholders)
- Status regions use aria-live
- Expandable panels use aria-expanded
- All range inputs have aria-valuemin, aria-valuemax, aria-valuenow

---

## Grid System

### Main Layout Grid
```css
Desktop (>920px):
  columns: repeat(2, minmax(0, 1fr))
  gap: 12px
  max-width: 1200px
  min-width: 1020px

Mobile (≤920px):
  columns: 1fr
  gap: 12px
```

### Panel Grid
```css
Desktop:
  columns: repeat(2, minmax(0, 1fr))
  gap: 10px 12px

Mobile:
  columns: 1fr
```

### Hero Grid
```css
Desktop:
  columns: minmax(0, 1fr) minmax(280px, 320px)
  gap: 12px

Mobile:
  columns: 1fr
```

---

## Responsive Breakpoints

```css
Desktop: >920px (2-column layout)
Mobile: ≤920px (1-column layout)
```

---

## Background Effects

### Noise Texture
```css
SVG noise overlay
opacity: 0.3
Turbulence baseFrequency: 0.9
```

### Gradients
```css
VAD meter fill:
  linear-gradient(90deg, rgba(29, 166, 160, 0.75), rgba(245, 179, 66, 0.9))
```

---

## Z-Index Scale

```css
1: Default stacking
2: VAD markers (above meter)
100: Skip links
9999: Toasts
```

---

## Icon System

Currently using emoji/unicode:
- 🎹 - Hotkey record
- ▾ - Chevron down (panel collapse)
- ✓ - Checkmark (cloud enabled)
- • - Status divider

**Recommendation**: Migrate to a consistent icon library (Heroicons, Lucide, etc.)

---

## Usage Guidelines

### DO
- Reference tokens from this design system
- Use spacing scale values (4, 6, 8, 10, 12, 14, 16, 20, 24, 32, 48, 64)
- Use defined color variables (--ink, --accent-2, etc.)
- Use standard border-radius (--radius, --radius-sm, 999px)
- Maintain 2px minimum touch target size (44px minimum recommended)

### DON'T
- Use hardcoded colors (#hexcode) outside of this file
- Use random spacing values (11px, 13px, 19px, 27px)
- Use random font sizes outside the type scale
- Mix border-radius values (8px, 9px, 11px)
- Skip focus states on interactive elements

---

## Updates & Versioning

**Last updated**: 2026-02-06
**Version**: 1.0 (extracted from existing styles.css)

**Change log**:
- 2026-02-06: Initial design system documentation extracted from styles.css
