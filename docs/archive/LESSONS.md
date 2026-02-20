# Design Lessons — Trispr Flow (Archived)

> Archive note: this file captures historical learnings and is no longer the canonical source.
> Use `docs/frontend/DESIGN_SYSTEM.md`, `docs/frontend/FRONTEND_GUIDELINES.md`, and `docs/DECISIONS.md` for active guidance.

> Patterns to follow, mistakes to avoid, and corrections from previous sessions.

---

## ✅ Patterns That Work

### Frontend Architecture
**Lesson**: Modular TypeScript > Monolithic main.ts
- **Why**: Splitting main.ts (~1800 lines) into 14 modules made the codebase maintainable
- **Pattern**: One module per responsibility (state, settings, devices, hotkeys, etc.)
- **Result**: main.ts is now ~220 lines (just initialization)
- **Apply**: Always modularize early. Don't wait for the file to become unmanageable.

**Lesson**: Centralized DOM references in dom-refs.ts
- **Why**: Prevents scattered getElementById calls. Single source of truth.
- **Pattern**: Cache all DOM references at startup. Export as `DOM` object.
- **Result**: No more getElementById in event handlers or loops.
- **Apply**: Create dom-refs.ts on day 1 for any DOM-heavy app.

**Lesson**: Event listeners in event-listeners.ts
- **Why**: Separates event wiring from business logic. Easy to audit.
- **Pattern**: setupEventListeners() called once at startup.
- **Result**: All event handlers in one file. Easy to find and modify.
- **Apply**: Never attach listeners inline. Always centralize.

### UI/UX Design
**Lesson**: Consistent spacing scale creates visual harmony
- **Why**: Random spacing (11px, 13px, 19px) creates visual noise.
- **Pattern**: Use 4px base unit. Scale: 4, 6, 8, 10, 12, 14, 16, 20, 24, 32, 48, 64.
- **Result**: Visual rhythm. Everything feels intentional.
- **Apply**: Define spacing scale in `docs/frontend/DESIGN_SYSTEM.md`. Reference tokens only.

**Lesson**: Toggle switches > Checkboxes for boolean settings
- **Why**: Toggles feel modern. Checkboxes feel like forms.
- **Pattern**: Use toggle-track + toggle-thumb CSS pattern.
- **Result**: Trispr Flow feels premium, not utilitarian.
- **Apply**: Use toggles for settings, checkboxes for multi-select lists.

**Lesson**: Expanders for secondary settings
- **Why**: Keeps primary UI clean. Progressive disclosure.
- **Pattern**: `<details>` + `<summary>` + expander-body.
- **Result**: VAD settings, overlay settings don't clutter the main panel.
- **Apply**: Hide complexity until needed. Defaults visible, advanced hidden.

**Lesson**: Status pills > Text labels for state
- **Why**: Pills are scannable. Color-coded. Visually distinct.
- **Pattern**: Pill with uppercase text + border + background-color.
- **Result**: "Recording / Transcribing" pills stand out instantly.
- **Apply**: Use pills for critical status indicators, not body text.

### Accessibility
**Lesson**: ARIA labels on ALL interactive elements
- **Why**: Screen readers need accessible names.
- **Pattern**: aria-label on icon-only buttons, labels on inputs, aria-expanded on expanders.
- **Result**: VoiceOver/NVDA users can navigate the entire app.
- **Apply**: Test with screen reader during development, not after.

**Lesson**: Focus states are non-negotiable
- **Why**: Keyboard users can't see where they are without focus indicators.
- **Pattern**: 2px solid var(--accent), outline-offset: 2px.
- **Result**: Tab navigation is crystal clear.
- **Apply**: Add :focus-visible to all interactive elements in base CSS.

**Lesson**: ARIA live regions for status updates
- **Why**: Screen readers don't see visual state changes (pills, dots).
- **Pattern**: .sr-only div with aria-live="polite" or "assertive".
- **Result**: "Recording started" announced to screen reader users.
- **Apply**: Add live regions for any critical state change (recording, transcribing, errors).

---

## ❌ Mistakes to Avoid

### Design Anti-Patterns
**Mistake**: Using emoji for UI icons (🎹, 🎤, 🔊)
- **Why wrong**: Inconsistent across platforms. Not scalable. Hard to style.
- **What happened**: Icons look different on Windows vs macOS.
- **Fix**: Migrate to Heroicons or Lucide for consistent, professional icons.
- **Lesson**: Never use emoji for functional UI elements. Only for decoration (if at all).

**Mistake**: Hardcoding colors (#hexcode) outside of :root
- **Why wrong**: Creates inconsistency. Hard to update globally.
- **What happened**: Accent colors were hardcoded in 15+ places.
- **Fix**: Extract all colors to CSS variables in :root.
- **Lesson**: Define color tokens once. Reference everywhere.

**Mistake**: Random spacing values (11px, 13px, 19px)
- **Why wrong**: Breaks visual rhythm. Feels sloppy.
- **What happened**: Early panels had inconsistent gaps.
- **Fix**: Replaced all random values with spacing scale (4, 6, 8, 10, 12, etc.).
- **Lesson**: Define spacing scale on day 1. No exceptions.

**Mistake**: Too many font sizes (8 different sizes across 6 screens)
- **Why wrong**: Creates visual noise. Undermines hierarchy.
- **What happened**: Hero values, field labels, button text all competing.
- **Fix**: Reduced to 6 intentional sizes with clear hierarchy.
- **Lesson**: Limit type scale to 4-6 sizes. Use weight and color for hierarchy.

### Code Anti-Patterns
**Mistake**: getElementById in event handlers and loops
- **Why wrong**: Slow. Repeats lookups. Hard to type-check.
- **What happened**: main.ts had 50+ getElementById calls.
- **Fix**: Created dom-refs.ts with cached references.
- **Lesson**: Cache DOM references at startup. Never lookup in handlers.

**Mistake**: No error handling on invoke() calls
- **Why wrong**: Crashes app on backend errors.
- **What happened**: Model download failures crashed frontend.
- **Fix**: Wrapped all invoke() calls in try/catch with toast notifications.
- **Lesson**: Every Tauri command call needs error handling.

**Mistake**: Not using event delegation for dynamic lists
- **Why wrong**: Attaching listeners to every item is slow + memory leak.
- **What happened**: History list with 100+ entries was sluggish.
- **Fix**: Event delegation on .history-list container.
- **Lesson**: Use event delegation for any list that changes dynamically.

---

## 🔄 Corrections from Previous Sessions

### Session 1: Frontend Modularization (Jan 2026)
**Issue**: main.ts was 1800 lines. Impossible to maintain.
**Solution**: Split into 14 modules by responsibility.
**Outcome**: Maintainability ✓, Testability ✓, Readability ✓.

### Session 2: Overlay Animation Fix (Jan 2026)
**Issue**: Overlay dot wasn't growing/shrinking with audio level.
**Root cause**: Audio level updates weren't reaching overlay window.
**Solution**: Use Tauri events to send audio level to overlay window.
**Outcome**: Overlay animation now works correctly ✓.

### Session 3: Monitoring Toggles (Jan 2026)
**Issue**: No way to disable input/output capture from UI.
**Solution**: Added "Enable input capture" and "Enable output transcription" toggles.
**Outcome**: Users can now disable features without closing app ✓.

### Session 4: Tray Menu Sync (Jan 2026)
**Issue**: Tray menu checkmarks didn't sync with UI toggles.
**Solution**: Emit events when UI toggles change. Update tray menu.
**Outcome**: Tray menu now accurately reflects UI state ✓.

### Session 5: Installer Build Robustness (Feb 2026)
**Issue**: Installer rebuild failed with Vite errors when HTML inputs were emitted with `../` paths.
**Root cause**: Vite was invoked with an unexpected CWD, and Rollup treated HTML inputs outside the root.
**Solution**: Anchor the rebuild script to repo root and set Vite `root` + HTML inputs inside repo root.
**Outcome**: `rebuild-installer.bat` completes successfully in a clean environment ✓.

---

## 🎨 Design Patterns to Maintain

### Color Usage
- **Accent-2 (teal)**: Toggles, active states, success, primary CTAs
- **Accent-3 (gold)**: Transcribing state, warnings
- **Accent (orange-red)**: Focus states, errors, danger actions
- **Ink**: Primary text
- **Ink-soft**: Secondary text
- **Muted**: Tertiary text, subtle labels

**Why**: Consistent color semantics. Users learn the visual language.

### Spacing Rhythm
- **Micro (4-6px)**: Badge padding, small gaps
- **Small (8-10px)**: Field gaps, panel-grid gaps
- **Medium (12-14px)**: Panel padding, hero gaps
- **Large (16-20px)**: Section padding, hero card padding
- **XL (24-32px)**: Expander padding, major gaps
- **XXL (48-64px)**: Section spacing (rarely used)

**Why**: Consistent rhythm creates visual harmony.

### Typography Hierarchy
1. **Display (H1)**: 1.75rem, weight 700 → Title
2. **Heading (H2)**: 0.95rem, weight 700 → Panel titles
3. **Subheading (H3)**: 0.9rem, weight 700 → Model sections
4. **Body**: 0.85-1rem → Content
5. **Small**: 0.72-0.75rem → Meta, field labels
6. **Tiny**: 0.6-0.7rem → Badges, pills, status

**Why**: Clear hierarchy. Eye knows where to land.

---

## 🚨 Critical Principles

### Simplicity Over Complexity
- **DO**: Hide advanced settings in expanders
- **DON'T**: Show all 20 settings at once
- **Example**: VAD settings hidden until VAD mode is selected

### Consistency Over Novelty
- **DO**: Use the same button style for all primary actions
- **DON'T**: Invent a new button style for each screen
- **Example**: Primary button = teal background, white text (everywhere)

### Accessibility Is Not Optional
- **DO**: Add ARIA labels, focus states, keyboard navigation
- **DON'T**: Skip accessibility "for now" (it never gets added)
- **Example**: Every release must pass WCAG 2.1 AA

### Performance Matters
- **DO**: Cache DOM references, use event delegation, debounce expensive operations
- **DON'T**: Lookup elements in loops, attach listeners to every item
- **Example**: dom-refs.ts prevents repeated getElementById calls

---

## 📝 Future Reminders

### When Adding New Panels
1. Add to layout-grid in styles.css
2. Add to `docs/APP_FLOW.md`
3. Add DOM references to dom-refs.ts
4. Add event listeners to event-listeners.ts
5. Add panel-collapse button
6. Test keyboard navigation
7. Test screen reader

### When Adding New Settings
1. Add to Settings struct in lib.rs
2. Add to AppState in state.ts
3. Add persistence logic in settings.ts
4. Add UI in index.html
5. Add to `docs/frontend/DESIGN_SYSTEM.md` if new tokens needed
6. Test edge cases (validation, conflicts, errors)

### When Adding New Components
1. Reference `docs/frontend/DESIGN_SYSTEM.md` tokens (no hardcoded values)
2. Add to component patterns section of `docs/frontend/DESIGN_SYSTEM.md`
3. Include all 5 states: default, hover, active, focus, disabled
4. Test keyboard navigation
5. Test screen reader
6. Test mobile responsiveness

---

**Last updated**: 2026-02-06
