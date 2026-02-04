# Trispr Flow Development Roadmap

**Status:** Infrastructure Setup Complete ✅
**Phase:** Security Hardening + Code Optimization
**Last Updated:** 2026-02-04

---

## 🎯 Overview

This roadmap guides the implementation of findings from Phase 3 validation:
- **Security vulnerabilities** identified in lib.rs
- **Accessibility gaps** in UI/frontend
- **Architecture refactoring** for code maintainability
- **MCP server integration** for enhanced capabilities

All items are prioritized by impact and effort.

---

## 📊 Priority Levels

| Level | Impact | Timeline | Examples |
|-------|--------|----------|----------|
| 🔴 **CRITICAL** | Security/Data Risk | This week | SSRF prevention, model integrity |
| 🟠 **HIGH** | Functionality/Quality | This sprint | Accessibility, refactoring |
| 🟡 **MEDIUM** | Optimization/Polish | Next sprint | Performance, testing |
| 🟢 **LOW** | Nice-to-have | Backlog | Documentation, advanced features |

---

## 🔴 CRITICAL – Security Hardening (This Week)

### Block 1: SSRF Prevention
**Purpose:** Prevent unauthorized model downloads from arbitrary URLs
**Effort:** 2-3 hours
**Files:** `lib.rs:1604-1620`, `lib.rs:866-949`

#### Tasks
- [ ] **1.1** Create URL whitelist function
  - [ ] Define allowed domains: `huggingface.co`, `distil-whisper`, custom base URLs (env var)
  - [ ] Reject dangerous schemes: `file://`, `ftp://`, `localhost`, `127.0.0.1`, `0.0.0.0`
  - [ ] Add scheme validation (must be `https://`)
  - [ ] Location: Add `fn is_url_safe(url: &str) -> Result<(), String>` in lib.rs

- [ ] **1.2** Update `download_model()` function
  - [ ] Call `is_url_safe()` on user-provided download URLs
  - [ ] Reject custom URLs that don't pass validation
  - [ ] Return error: `"Download URL not allowed (not in whitelist)"`
  - [ ] Location: lib.rs:1604-1620

- [ ] **1.3** Update `load_custom_source_models()` function
  - [ ] Validate `custom_url` before fetching JSON index
  - [ ] Check if custom_url is in whitelist (or is environment variable override)
  - [ ] Return error: `"Custom model source URL not allowed"`
  - [ ] Location: lib.rs:866-949

- [ ] **1.4** Test & Document
  - [ ] Test with valid URLs (huggingface.co) ✅
  - [ ] Test with dangerous URLs (file://, localhost) ❌
  - [ ] Test with env var custom base URL ✅
  - [ ] Add comments documenting whitelist logic

**Verification:**
```bash
# After implementation, verify:
1. download_model() rejects file:// URLs
2. Custom source accepts only whitelisted domains
3. Environment variables for custom base URLs still work
```

---

### Block 2: Model Integrity Verification
**Purpose:** Ensure downloaded models haven't been tampered with
**Effort:** 2 hours
**Files:** `lib.rs:2979-3048`

#### Tasks
- [ ] **2.1** Create checksum validation
  - [ ] Add `fn verify_model_checksum(path: &Path, expected: &str) -> Result<(), String>`
  - [ ] Use SHA256 hashing (crypto crate or similar)
  - [ ] Compare downloaded file hash to expected hash
  - [ ] Return clear error on mismatch: `"Model checksum mismatch: possible corruption or tampering"`

- [ ] **2.2** Store model checksums
  - [ ] Create `MODEL_CHECKSUMS` constant or load from file:
    ```rust
    const MODEL_CHECKSUMS: &[(&str, &str)] = &[
        ("ggml-large-v3.bin", "sha256_hash_here"),
        ("ggml-large-v3-turbo.bin", "sha256_hash_here"),
    ];
    ```
  - [ ] For custom models, require checksum in JSON index (optional field)
  - [ ] Location: Near MODEL_SPECS in lib.rs

- [ ] **2.3** Integrate checksum into download
  - [ ] After `fs::rename(&tmp_path, &dest_path)` in `download_model_file()`:
    - Call `verify_model_checksum()` on final file
    - If checksum fails: delete file and return error
    - If checksum passes: emit success event
  - [ ] Location: lib.rs:3029

- [ ] **2.4** Handle missing checksums gracefully
  - [ ] For built-in models: Always verify (checksum must exist)
  - [ ] For custom models: Make checksum optional (warn if missing)
  - [ ] For user-provided URLs: Require checksum if available, warn if not

- [ ] **2.5** Test & Document
  - [ ] Test with correct checksum ✅
  - [ ] Test with corrupted file (wrong checksum) ❌
  - [ ] Test custom model without checksum (should warn)
  - [ ] Document how to generate SHA256 for custom models

**Verification:**
```bash
# After implementation:
1. Download model → verify checksum passes
2. Corrupt file → checksum validation fails, file deleted
3. Custom model without checksum → download succeeds with warning
```

---

### Block 3: Download Size Limits
**Purpose:** Prevent disk space exhaustion / DoS
**Effort:** 30 minutes
**Files:** `lib.rs:2993-3026`

#### Tasks
- [ ] **3.1** Define max download size
  - [ ] Add constant: `const MAX_MODEL_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024;` (5 GB)
  - [ ] Location: Top of lib.rs

- [ ] **3.2** Check Content-Length header
  - [ ] In `download_model_file()`, after `ureq::get()` response:
    ```rust
    let total = response.header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());

    if let Some(size) = total {
        if size > MAX_MODEL_SIZE_BYTES {
            return Err(format!("Model too large: {} MB (max 5000 MB)", size / 1024 / 1024));
        }
    }
    ```
  - [ ] Location: lib.rs:2994-2996

- [ ] **3.3** Timeout stalled downloads
  - [ ] Track last read timestamp in loop
  - [ ] If no data read for 30+ seconds: cancel download
  - [ ] Return error: `"Download stalled: no data for 30 seconds"`
  - [ ] Location: lib.rs:3005-3026

- [ ] **3.4** Test
  - [ ] Download normal model (< 5GB) ✅
  - [ ] Test with fake huge Content-Length header ❌
  - [ ] Document timeout behavior

**Verification:**
```bash
# After implementation:
1. Normal model download succeeds (2-3 GB)
2. Attempting 10GB model → rejected with "Model too large"
3. Stalled download → killed after 30 seconds
```

---

## 🟠 HIGH – Accessibility & UX (This Sprint, ~2 weeks)

### Block 4: Frontend Accessibility Audit
**Purpose:** WCAG 2.1 AA compliance for UI
**Effort:** 4-6 hours
**Files:** `index.html`, `src/main.ts`, `src/overlay.ts`

#### Tasks
- [ ] **4.1** Semantic HTML review (index.html)
  - [ ] Check all form inputs have associated `<label>` tags
  - [ ] Verify form groups use `<fieldset>` + `<legend>` (for hotkey inputs)
  - [ ] Ensure buttons have descriptive text (no icon-only buttons)
  - [ ] Review heading hierarchy (h1, h2, h3 sequence)
  - [ ] Location: index.html lines 1-475

- [ ] **4.2** Color contrast audit
  - [ ] Check all text meets 4.5:1 contrast ratio (normal text) or 3:1 (large text)
  - [ ] Test with dark theme enabled
  - [ ] Review overlay colors (#ff3d2e on dark background)
  - [ ] Use: https://webaim.org/resources/contrastchecker/ or similar
  - [ ] Location: src/styles.css, src/overlay.css

- [ ] **4.3** ARIA labels & live regions
  - [ ] Add `aria-label` to overlay indicator (currently generic div)
  - [ ] Add `aria-live="polite"` to status messages (recording/idle)
  - [ ] Add `aria-describedby` for complex settings (dB sliders)
  - [ ] Add `role="alert"` to error toasts
  - [ ] Location: index.html, src/main.ts

- [ ] **4.4** Keyboard navigation
  - [ ] Verify all controls reachable via Tab key
  - [ ] Check focus indicators are visible (ring, underline, etc.)
  - [ ] Test Escape key closes modals/panels
  - [ ] Test Enter on buttons/links works correctly
  - [ ] Location: index.html, src/main.ts, CSS focus styles

- [ ] **4.5** Recording state indicator
  - [ ] Overlay state should have accessible alternative (not visual only)
  - [ ] Add aria-label: `"Recording status: [idle|recording|transcribing]"`
  - [ ] Consider audio cue for state changes (already implemented ✅)
  - [ ] Location: src/overlay.ts, overlay.html

- [ ] **4.6** Screen reader testing (optional)
  - [ ] Test with NVDA (Windows screen reader)
  - [ ] Verify main UI structure readable (headings, form labels)
  - [ ] Check overlay doesn't interfere with screen reader

- [ ] **4.7** Update accessibility documentation
  - [ ] Add WCAG compliance notes to README
  - [ ] Document keyboard shortcuts (PTT hotkey, transcribe hotkey)
  - [ ] Accessibility checklist for future features

**Verification:**
```bash
# Automated check:
npm run a11y-audit  # (if you add this to scripts)

# Manual check:
1. Tab through all UI controls (should be logical order)
2. Verify focus indicator visible at each step
3. Test with browser's accessibility inspector
4. Check color contrast with WebAIM tool
```

---

### Block 5: Frontend Code Organization
**Purpose:** Improve main.ts maintainability (~1800 lines)
**Effort:** 3-4 hours
**Files:** `src/main.ts`

#### Tasks
- [ ] **5.1** Split by domain
  ```
  src/
  ├── main.ts (keep small: init, event listeners)
  ├── settings/
  │   ├── settings.ts (load, save, validation)
  │   └── defaults.ts (default values)
  ├── devices/
  │   ├── devices.ts (enumeration, selection)
  │   └── audio.ts (audio device helpers)
  ├── hotkeys/
  │   └── hotkeys.ts (configuration, validation)
  ├── models/
  │   ├── manager.ts (download, install, remove)
  │   └── list.ts (list, info)
  ├── history/
  │   └── history.ts (get, clear)
  └── ui/
      └── components.ts (toast, notifications)
  ```

- [ ] **5.2** Extract Settings logic
  - [ ] Create `src/settings/settings.ts` with functions:
    - `loadSettings(): Promise<Settings>`
    - `saveSettings(s: Settings): Promise<void>`
    - `validateSettings(s: Settings): ValidationResult`
  - [ ] Move localStorage persistence logic
  - [ ] Effort: 1 hour

- [ ] **5.3** Extract Device management
  - [ ] Create `src/devices/devices.ts` with functions:
    - `listInputDevices(): Promise<Device[]>`
    - `listOutputDevices(): Promise<Device[]>`
    - `selectInputDevice(id: string): Promise<void>`
  - [ ] Effort: 45 minutes

- [ ] **5.4** Extract Hotkey management
  - [ ] Create `src/hotkeys/hotkeys.ts` with functions:
    - `registerHotkey(key: string, handler: () => void): Promise<void>`
    - `unregisterHotkey(key: string): Promise<void>`
    - `validateHotkey(key: string): ValidationResult`
  - [ ] Effort: 45 minutes

- [ ] **5.5** Extract Model management
  - [ ] Create `src/models/manager.ts` with functions:
    - `listModels(): Promise<ModelInfo[]>`
    - `downloadModel(id: string): Promise<void>`
    - `removeModel(id: string): Promise<void>`
  - [ ] Effort: 1 hour

- [ ] **5.6** Update main.ts
  - [ ] Import from new modules
  - [ ] Keep event listeners and top-level logic
  - [ ] Effort: 30 minutes

**Verification:**
```bash
npm run build   # Should still compile
npm run dev     # Should still work
# File structure should match outline above
```

---

## 🟡 MEDIUM – Code Quality & Testing (Next Sprint)

### Block 6: lib.rs Monolith Refactoring
**Purpose:** Split 3000+ line file into focused modules
**Effort:** 8-12 hours
**Files:** `src-tauri/src/lib.rs`

#### Tasks
- [ ] **6.1** Extract audio module
  - [ ] Create `src-tauri/src/audio.rs`
  - [ ] Move functions:
    - `resolve_input_device()`
    - `resolve_output_device()`
    - `list_audio_devices()`
    - `list_output_devices()`
    - CPAL initialization & device enumeration
  - [ ] Effort: 2 hours

- [ ] **6.2** Extract transcription module
  - [ ] Create `src-tauri/src/transcription.rs`
  - [ ] Move functions:
    - `run_whisper_cli()` (subprocess spawning)
    - `parse_whisper_output()`
    - `transcribe_audio()` (Tauri command wrapper)
    - Whisper CLI integration
  - [ ] Effort: 2 hours

- [ ] **6.3** Extract models module
  - [ ] Create `src-tauri/src/models.rs`
  - [ ] Move functions:
    - `download_model()` (with SSRF/integrity fixes)
    - `download_model_file()`
    - `remove_model()`
    - `list_models()`
    - Model specs & management
  - [ ] Effort: 2.5 hours

- [ ] **6.4** Extract state/settings module
  - [ ] Create `src-tauri/src/state.rs`
  - [ ] Move:
    - `Settings` struct & defaults
    - Recording state machine
    - VAD configuration
    - `AppState` struct
  - [ ] Effort: 2 hours

- [ ] **6.5** Consolidate utility functions
  - [ ] Create `src-tauri/src/paths.rs`
  - [ ] Move:
    - `resolve_models_dir()`
    - `resolve_cache_dir()`
    - All path resolution logic
  - [ ] Effort: 1 hour

- [ ] **6.6** Update lib.rs main file
  - [ ] Add `mod audio;`, `mod transcription;`, etc.
  - [ ] Re-export public APIs
  - [ ] Keep only top-level logic (run function, event handlers)
  - [ ] Should reduce to < 500 lines
  - [ ] Effort: 1 hour

- [ ] **6.7** Update Cargo.toml if needed
  - [ ] No dependency changes needed (same crate)
  - [ ] Just module declarations

- [ ] **6.8** Testing & verification
  - [ ] `cargo build` should succeed
  - [ ] `cargo test` should pass (if tests exist)
  - [ ] `npm run tauri dev` should run without errors
  - [ ] Effort: 1 hour

**Verification:**
```bash
cargo build --release   # Must compile
cargo check             # No warnings
ls src-tauri/src/*.rs   # Should see: audio.rs, transcription.rs, models.rs, state.rs, etc.
wc -l src-tauri/src/lib.rs   # Should be < 500 lines
```

---

### Block 7: Automated Testing
**Purpose:** Catch regressions in security + core logic
**Effort:** 4-6 hours
**Files:** `src-tauri/src/tests.rs` (new), `src-tauri/tests/` (new)

#### Tasks
- [ ] **7.1** Unit tests for URL validation
  ```rust
  #[test]
  fn test_is_url_safe_accepts_huggingface() {
      assert!(is_url_safe("https://huggingface.co/model.bin").is_ok());
  }

  #[test]
  fn test_is_url_safe_rejects_file_scheme() {
      assert!(is_url_safe("file:///etc/passwd").is_err());
  }
  ```
  - [ ] Effort: 1 hour

- [ ] **7.2** Unit tests for checksum validation
  ```rust
  #[test]
  fn test_checksum_match() {
      // Create temp file with known content
      // Calculate SHA256
      // Verify match returns Ok
  }
  ```
  - [ ] Effort: 1 hour

- [ ] **7.3** Integration tests for recording state machine
  - [ ] Test transitions: idle → recording → transcribing → idle
  - [ ] Test VAD detection (threshold changes)
  - [ ] Effort: 2 hours

- [ ] **7.4** Test coverage reporting
  - [ ] Add `cargo tarpaulin` for coverage
  - [ ] Target: 70%+ coverage for critical modules
  - [ ] Effort: 1 hour

**Verification:**
```bash
cargo test                  # All tests pass
cargo tarpaulin            # Coverage report
```

---

## 🟢 LOW – Documentation & Polish

### Block 8: Documentation Updates
**Effort:** 2 hours

- [ ] **8.1** Update README.md
  - [ ] Add "Security Hardening" section (SSRF prevention, checksums)
  - [ ] Add "Accessibility" section (WCAG AA compliance)
  - [ ] Add "Skills Integration" section (link to SKILLS_AND_AGENTS_GUIDE.md)
  - [ ] Add "Architecture" section (module breakdown: audio, transcription, models, state)

- [ ] **8.2** Add inline code comments
  - [ ] Document URL validation logic
  - [ ] Document recording state machine
  - [ ] Document model download flow

- [ ] **8.3** Create CONTRIBUTING.md
  - [ ] Module overview & responsibilities
  - [ ] Testing requirements
  - [ ] Security checklist for PRs

---

### Block 9: Performance Optimization (Optional)
**Effort:** 4+ hours

- [ ] **9.1** Profile MCP server integration
  - [ ] Benchmark local-stt-mcp vs. current whisper-cli subprocess
  - [ ] Measure latency, GPU memory usage
  - [ ] Decide: Replace subprocess or keep both?

- [ ] **9.2** Optimize VAD detection
  - [ ] Profile audio processing pipeline
  - [ ] Consider GPU acceleration if CPU bottleneck

- [ ] **9.3** Cache model metadata
  - [ ] Reduce network calls to model index
  - [ ] Add local cache with TTL (1 day)

---

## 📈 Progress Tracking

### Checklist Summary

**This Week (CRITICAL):**
- [ ] Block 1: SSRF Prevention (2-3h)
- [ ] Block 2: Model Integrity (2h)
- [ ] Block 3: Size Limits (30min)
- **Total: ~5 hours**

**This Sprint (HIGH):**
- [ ] Block 4: Accessibility (4-6h)
- [ ] Block 5: Frontend Org (3-4h)
- **Total: ~8-10 hours**

**Next Sprint (MEDIUM):**
- [ ] Block 6: lib.rs Refactoring (8-12h)
- [ ] Block 7: Testing (4-6h)
- [ ] Block 8: Documentation (2h)
- **Total: ~14-20 hours**

**Backlog (LOW):**
- [ ] Block 9: Performance (4+h)

---

## 🚀 Getting Started

### Day 1 (2 hours)
1. ✅ Read SKILLS_AND_AGENTS_GUIDE.md (15 min)
2. ⏭️ **Start Block 1 (SSRF Prevention)**
   - Create `is_url_safe()` function
   - Update `download_model()`
   - Add whitelist constants

### Day 2 (2 hours)
3. ⏭️ **Continue Block 2 (Checksums)**
   - Add MODEL_CHECKSUMS constant
   - Implement checksum verification
   - Test with real download

### Day 3 (1 hour)
4. ⏭️ **Complete Block 3 (Size Limits)**
   - Add MAX_MODEL_SIZE constant
   - Check Content-Length in download

### End of Week
- ✅ All CRITICAL items done
- Security vulnerabilities patched
- Ready for HIGH priority items next sprint

---

## 📝 Notes & Tips

1. **Test after each block:** Don't wait until everything is done
2. **Git commits:** One commit per block (helps with review)
3. **Use SKILLS_AND_AGENTS_GUIDE:** Reference specific line numbers for context
4. **Ask Claude Code:** Use `/code-review` skill on PRs
5. **Document decisions:** Why you chose certain approaches

---

## 🔗 Related Documents

- [SKILLS_AND_AGENTS_GUIDE.md](.claude/SKILLS_AND_AGENTS_GUIDE.md) – Detailed findings & recommendations
- [INSTALLATION_SUMMARY.md](.claude/INSTALLATION_SUMMARY.md) – Infrastructure status
- [README.md](../README.md) – Main project documentation

---

**Created:** 2026-02-04
**Status:** Ready for Implementation
**Next Review:** After Block 1 completion
