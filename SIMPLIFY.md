# Simplify Review — Branch `spike/ollama-offline-fallback`

> Scope: **27 Commits**, 62 Files, +13.524 / -1.384 lines vs. `main`
> Base: `git diff main..HEAD`

---

## Aufgabenstellung

Review aller Branch-Änderungen auf **Code Reuse**, **Code Quality** und **Efficiency**.
Drei Perspektiven pro Batch:

1. **Reuse** — Gibt es bestehende Utilities/Helpers, die neu geschriebenen Code ersetzen könnten? Duplikate? Inline-Logik, die eine shared Abstraction nutzen könnte?
2. **Quality** — Redundanter State, Parameter-Sprawl, Copy-Paste mit Variation, Leaky Abstractions, Stringly-typed Code?
3. **Efficiency** — Unnötige Arbeit, verpasste Concurrency, Hot-Path-Bloat, Memory-Leaks, Event-Listener-Leaks, overly broad Operations?

Regel: **Nur konkrete Findings mit konkreten Fixes.** Keine Style-Nits, keine optionalen Refactors.

---

## Batch 1 — Opus: Rust Concurrency + AI Core

**Risiko: HOCH** — Prozess-Lifecycle, Mutex-State, Multi-Provider AI, Security-kritisch

| Datei | Zeilen geändert | Schwerpunkt |
|-------|----------------|-------------|
| `src-tauri/src/ollama_runtime.rs` | +1.096 (neu) | Prozess-Spawn/Kill, Installations-Pipeline, SHA-256 Verify |
| `src-tauri/src/state.rs` | +638 | AppState mit 15+ Mutex-Feldern, Settings Serialization |
| `src-tauri/src/lib.rs` | +1.038 | 58+ Tauri Commands, Path Validation, RunEvent::Exit |
| `src-tauri/src/ai_fallback/provider.rs` | +1.072 | Multi-Provider (Ollama/Claude/OpenAI/Gemini), Auth, SSRF |
| `src-tauri/src/ai_fallback/models.rs` | +197 | Model-Definitionen, Validation |

**Review-Fokus:**
- Mutex-Poisoning: Werden `lock().unwrap()` Panics irgendwo verschluckt?
- Race Conditions: Concurrent access auf `managed_ollama_child`, `is_recording`, etc.
- Ollama Install/Download: TOCTOU, Partial-Download-Cleanup
- Provider Fallback: Fehlerbehandlung bei Provider-Chain, Timeout-Kaskaden
- SSRF: `is_ssrf_target()` Coverage — IPv6, DNS-Rebinding?
- Path Traversal: `validate_path_within()` — Symlink-Edges, UNC-Pfade auf Windows?

---

## Batch 2 — Opus: Rust Peripheral

**Risiko: MITTEL-HOCH** — Audio Pipeline, Transcription State Machine

| Datei | Zeilen geändert | Schwerpunkt |
|-------|----------------|-------------|
| `src-tauri/src/audio.rs` | +441 | FFmpeg Encoding, Opus, VAD |
| `src-tauri/src/transcription.rs` | +439 | Whisper CLI, Segment Parsing, State Machine |
| `src-tauri/src/overlay.rs` | +113 | Overlay Window Management |
| `src-tauri/src/paths.rs` | +34 | Path Resolution, Recordings Dir |
| `src-tauri/src/postprocessing.rs` | +65 | LLM Post-Processing Pipeline |

**Review-Fokus:**
- Subprocess-Buffer: Werden stdout/stderr von FFmpeg/Whisper bounded gelesen?
- Temp-File-Cleanup: Bleiben temporäre .wav/.opus bei Crashes liegen?
- Encoding: Error-Handling bei FFmpeg-Failures (corrupt input, disk full)
- Transcription State: Wird `is_recording` bei Panics korrekt zurückgesetzt?

---

## Batch 3 — Sonnet: TypeScript Core

**Risiko: MITTEL** — Event-Architecture, komplexe State-Transitions

| Datei | Zeilen geändert | Schwerpunkt |
|-------|----------------|-------------|
| `src/event-listeners.ts` | +1.018 | Alle UI Event-Handler, Keyboard Shortcuts |
| `src/main.ts` | +395 | App Init, Event Wiring, Lifecycle |
| `src/ui-state.ts` | +172 | UI State Management, Panel Collapse |
| `src/dom-refs.ts` | +78 | DOM Element References |
| `src/types.ts` | +178 | TypeScript Interfaces, Settings Types |

**Review-Fokus:**
- Listener-Leaks: `addEventListener` ohne Cleanup?
- Redundanter State: UI-State der aus Settings abgeleitet werden könnte?
- Event-Ordering: Race Conditions bei `DOMContentLoaded` vs. Tauri `listen()`?
- Type Safety: `as` Casts die zur Laufzeit brechen könnten?

---

## Batch 4 — Sonnet: TypeScript Features

**Risiko: MITTEL** — API Calls, großer State, User-Facing

| Datei | Zeilen geändert | Schwerpunkt |
|-------|----------------|-------------|
| `src/ollama-models.ts` | +1.121 (neu) | Model Manager UI, Pull, Tags, Runtime Wizard |
| `src/settings.ts` | +574 | Settings Rendering, Color Picker, Persist |
| `src/history.ts` | +267 | History List, Search, Topic Badges |
| `src/refinement-inspector.ts` | +531 (neu) | Diff Rendering, Snapshots, localStorage |

**Review-Fokus:**
- API-Error-Handling: Timeout-Handling für Ollama Model Pull (kann Minuten dauern)
- DOM-Performance: Werden große Listen (History, Models) virtualisiert oder komplett gerendert?
- localStorage-Limits: Wie viele Snapshots passen rein? Pruning-Strategie?
- Duplicate Code: `ollama-models.ts` vs. `settings.ts` — gemeinsame API-Call-Patterns?

---

## Batch 5 — Haiku: TypeScript Small + Isolated Modules

**Risiko: NIEDRIG** — Isoliert, klar begrenzt

| Datei | Zeilen geändert | Schwerpunkt |
|-------|----------------|-------------|
| `src/refinement-pipeline-graph.ts` | +276 (neu) | SVG Pipeline Visualization |
| `src/ai-refinement-help.ts` | +283 (neu) | Help System, Tooltip Content |
| `src/custom-tooltips.ts` | +239 (neu) | Custom Tooltip Engine |
| `src/refinement-prompts.ts` | +71 (neu) | Prompt Templates |
| `src/utils.ts` | +104 (neu) | Shared Utilities |
| `src/ollama-tag-utils.ts` | +12 (neu) | Tag Parsing |
| `src/ollama-refresh-policy.ts` | +5 (neu) | Refresh Timer |
| `src/accessibility.ts` | +19 (neu) | A11y Announcements |
| `src/chapters.ts` | +13 | Chapter Navigation |
| `src/state.ts` | +2 | State Constants |

**Review-Fokus:**
- Tooltip-Lifecycle: Event-Listener-Cleanup bei Tooltip-Destroy?
- SVG-Performance: Wird die Pipeline-Graph bei jedem State-Change komplett neu gerendert?
- Prompt-Templates: Hardcoded Strings vs. i18n-Ready?

---

## Batch 6 — Haiku: CSS + HTML + Tests

**Risiko: NIEDRIG** — Visuell, keine Logik-Risiken

| Datei | Zeilen geändert | Schwerpunkt |
|-------|----------------|-------------|
| `src/styles.css` | +1.381 | Main Stylesheet, CSS Variables |
| `src/styles-modern.css` | +318 (neu) | Glassmorphism Layer |
| `index.html` | +753 | HTML Structure, Settings Panel |
| `overlay.html` | +330 (umgebaut) | Overlay mit externem Script |
| `public/overlay.js` | +326 (neu) | Extracted Overlay Script |
| `src/tests/*.test.ts` | +474 | Unit Tests |

**Review-Fokus:**
- CSS-Variable-Konsistenz: Alle hardcoded Farben migriert?
- CSS-Spezifität: `styles-modern.css` Overrides sauber via Layer?
- HTML-Accessibility: Fehlende `aria-*` Attribute?
- Test-Coverage: Welche kritischen Pfade haben keine Tests?

---

---

## Batch 1+2 Findings — Rust (Opus Review abgeschlossen)

### HOCH — Security-kritisch

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| H1 | `is_ssrf_target()` Fail-Open bei Parse-Fehlern (`Err(_) => return false`) | provider.rs:99 | `Err(_) => return true` |
| H2 | SSRF deckt IPv6 nicht ab (`[::ffff:169.254.169.254]`, `fe80::`) | provider.rs:97-118 | IPv6-Parsing + mapped-IPv4 Check |
| H3 | `validate_path_within` anfällig für UNC-Pfade (`\\attacker\share`) → NTLM-Leak | lib.rs:1700-1731 | UNC-Prefix ablehnen + Filename-Sanitize |

### MITTEL — Robustheit + Duplikation

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| M1 | Exit-Handler `lock().unwrap()` panikt bei poisoned Mutex | lib.rs:3094-3106 | `if let Ok(mut guard) = ...lock()` |
| M2 | Pull-Cleanup Race: Panic → Model permanent "in progress" | lib.rs:1143-1188 | Drop-Guard für Pull-Cleanup |
| M3 | `save_settings_file` nicht atomar → Crash korrumpiert JSON | state.rs:950-958 | Write-to-tmp + rename |
| M4 | Settings-Save+Emit Boilerplate 15x copy-pasted | lib.rs diverse | `update_and_persist_settings()` Helper |
| M5 | Strict-Local-Mode Guard 12x identisch | lib.rs + ollama_runtime.rs | `enforce_strict_local()` Funktion |
| M6 | Refinement-Setup 3x dupliziert (50+ Zeilen/Kopie) | lib.rs:639+1011, audio.rs | `prepare_refinement_context()` |
| M7 | Provider-ID-Normalisierung 5 separate Funktionen | provider.rs, models.rs, state.rs, keyring.rs | Eine kanonische Funktion |
| M8 | Full History Clone+Serialize+Write pro Eintrag (HOT) | state.rs:1023-1078 | Debounced/async persist |
| M9 | Sync HTTP in sync Tauri-Commands blockiert Thread-Pool | lib.rs diverse | `async` + `spawn_blocking` |
| M10 | `validate_ollama_model_name` blockiert `/` für Namespaces | provider.rs:977-991 | `/` zur Allowlist |
| M11 | `save_settings_file` bei Window-Drag ohne Debounce | lib.rs:1412-1464 | Debounce (500ms) |
| M12 | `Vec::insert(0)` statt `VecDeque::push_front` | state.rs:1038,1066 | `VecDeque` |

### NIEDRIG

| ID | Finding | Datei |
|----|---------|-------|
| L1 | `ureq::Agent` bei jedem Call neu (kein Connection-Reuse) | provider.rs |
| L2 | `now_iso()` dupliziert | ollama_runtime.rs:415 vs lib.rs:579 |
| L3 | Doppelter SHA-256-Hash bei Download | ollama_runtime.rs:606+656 |
| L4 | Window-Geometry-Restore 2x copy-pasted | lib.rs:2511+2950 |
| L5 | `Settings::default()` bei jedem normalize-Aufruf | state.rs:869 |
| L6 | `sanitize_model_name` / `sanitize_session_name` fast identisch | ollama_runtime.rs:324, lib.rs:1916 |
| L7 | Redundanter State: `transcribe_active` vs `transcribe_enabled` | state.rs:379 |
| L8 | Prompt-Profile-Normalisierung 2x | provider.rs:322, models.rs:389 |
| L9 | `resolve_runtime_root` umgeht paths.rs | ollama_runtime.rs:143 |

## Batch 2 Findings — Rust Peripheral (Opus Review abgeschlossen)

### HOCH

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| P-H1 | Temp-File-Leak: WAV-Datei bleibt bei Early-Return-Fehlerpfaden liegen | transcription.rs:1495-1657 | `Drop`-Guard (`TempFileGuard`) |
| P-H2 | `command.output()` ohne Timeout — hängender Whisper blockiert Worker-Thread | transcription.rs:1575-1577 | `spawn()` + 120s Timeout + `child.kill()` |
| P-H3 | Transcription-Result-Handling 3x kopiert (40 Zeilen/Kopie) | audio.rs:1215,1773,1936 | `handle_transcription_result()` extrahieren |

### MITTEL

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| P-M1 | `ContinuousDumpEvent` struct 2x definiert | audio.rs:105 + transcription.rs:60 | Gemeinsames Modul |
| P-M2 | `lock().unwrap()` 36 Stellen in audio.rs + transcription.rs | audio.rs:588,1889 + transcription.rs:234 | `unwrap_or_else(\|e\| e.into_inner())` |
| P-M3 | Regex wird pro Wort bei jedem Aufruf neu kompiliert | postprocessing.rs:392 | `OnceLock<HashMap<String, Regex>>` Cache |
| P-M4 | `build_input_stream_f32/i16/u16` — 3x identische Callbacks (~50 Zeilen/Kopie) | audio.rs:597-746 | Generische Funktion + Conversion-Trait |
| P-M5 | `update_overlay_state` spawnt OS-Thread nur für 120ms Sleep | overlay.rs:121-124 | Channel + Debounce |
| P-M6 | Whisper-Seiteneffekte (.srt/.vtt/.json) nicht aufgeräumt | transcription.rs:1651-1657 | Glob-Cleanup oder dediziertes Temp-Dir |
| P-M7 | `paths.rs` Fallback auf `"."` ohne Warnung | paths.rs:6-20 | `Result` zurückgeben + Logging |
| P-M8 | Question-Detection im `multi`-Modus 3x dupliziert | postprocessing.rs:69-196 | `const` Arrays + einmalige Prüfung |

### NIEDRIG

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| P-L1 | `mark_overlay_ready()` leerer Funktionskörper (Dead Code) | overlay.rs:39 | Entfernen oder `todo!()` |
| P-L2 | `whisper_cli_supports_gpu_layers` ruft `--help` bei jeder Transkription auf | transcription.rs:1317-1341 | `OnceLock<bool>` |
| P-L3 | `refine_with_llm` toter Stub mit `#[allow(dead_code)]` | postprocessing.rs:420-434 | Entfernen |
| P-L4 | `eprintln!` in Audio-Stream-Callbacks statt `tracing::error!` | audio.rs:607,657,707 | `tracing::error!()` |

---

## Batch 3+4 Findings — TypeScript Core + Features (Sonnet Review abgeschlossen)

### HOCH

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| TS-H1 | `window.addEventListener` 3x ohne Cleanup → akkumuliert bei Re-Bootstrap | main.ts:221, event-listeners.ts:1020,1473 | In `eventUnlisteners` aufnehmen |
| TS-H2 | AI-Provider-Normalisierung 6 Funktionen + 2 Konstanten dupliziert | event-listeners.ts:46-76, settings.ts:197-239 | `src/ai-provider-utils.ts` extrahieren |
| TS-H3 | `ensureContinuousDumpDefaults` vollständig dupliziert | event-listeners.ts:299, settings.ts:25 | Export aus settings.ts, Import in event-listeners |
| TS-H4 | Render-Tripel (`renderAIFallbackSettingsUi/OllamaModelManager/Hero`) 5x copy-pasted | event-listeners.ts:1511,1541,1591,1615,1635 | `refreshAIUi()` + `commitLocalPrimary()` extrahieren |
| TS-H5 | `handleOllamaPull` fehlendes `finally` → UI bleibt bei Erfolg in "Downloading"-State | ollama-models.ts:1028-1043 | `finally { activeOllamaPulls.delete(modelName) }` |
| TS-H6 | `detectTopics` erstellt 18 neue RegExp-Instanzen pro History-Entry bei jedem Render | history.ts:408-423,858 | Regex-Cache als `Map<string, RegExp[]>` |
| TS-H7 | Memory Leak: Event-Listener auf weggeworfenen DOM-Nodes bei `renderHistory()` | history.ts:860-904 | Event-Delegation auf `historyList`-Container |
| TS-H8 | Tooltip Memory Leak: `initUnifiedTooltips` ohne Cleanup-Funktion | custom-tooltips.ts:210-238 | `cleanupUnifiedTooltips()` + in `cleanupEventListeners()` aufrufen |

### MITTEL

| ID | Finding | Datei | Fix |
|----|---------|-------|-----|
| TS-M1 | `refreshModelsDir()` fire-and-forget ohne `.catch()` | main.ts:316 | `void refreshModelsDir().catch(...)` |
| TS-M2 | `if (forcedLocal) { persist } else { persist }` → identische Branches | event-listeners.ts:535-540,596-601 | if/else entfernen, einmal `persistSettings()` |
| TS-M3 | 24 identische `change`-only-Persist-Handler | event-listeners.ts diverse | `onChangePersist(el)` Helper |
| TS-M4 | History-Update-Handler für 2 Events: identischer Body bis auf Setter | main.ts:359-379 | `makeHistoryUpdateHandler(setter)` Factory |
| TS-M5 | Inline DOM-Queries außerhalb dom-refs.ts | event-listeners.ts:2192, main.ts:687 | `applyOverlayBtn` in dom-refs.ts |
| TS-M6 | `postproc_llm_provider/model` sync 7x ohne Helper-Funktion | event-listeners.ts+settings.ts+ollama-models.ts | `syncLegacyProviderFields(settings)` |
| TS-M7 | `persistCurrentSettings` in ollama-models.ts: Duplikat ohne Error-Handling | ollama-models.ts:433-436 | Löschen, `persistSettings` aus settings.ts importieren |
| TS-M8 | Circular Import: settings.ts ↔ event-listeners.ts | settings.ts:7, event-listeners.ts:13 | `renderVocabulary` auslagern |
| TS-M9 | `renderHistory()` + `renderSettings()` ohne RAF-Batching (werden 8x sync aufgerufen) | main.ts:359-378, event-listeners.ts:diverse | RAF-Guard analog `scheduleRender()` |
| TS-M10 | localStorage Snapshot-Store ohne Größenlimit → `QuotaExceededError` still geschluckt | refinement-inspector.ts:79-97 | Max 100 Snapshots + Quota-Fehler loggen |
| TS-M11 | `navigator.clipboard.writeText` ohne try/catch | event-listeners.ts:910-915 | `try/catch` + `showToast` |
| TS-M12 | Dead logic: `aiEnabled ? "bypassed" : "bypassed"` in Pipeline-Graph | refinement-pipeline-graph.ts:137-138 | `aiState = "bypassed"` direkt |
| TS-M13 | CSS: `.panel` vs `.hero-card` backdrop-filter inkonsistent (16px vs 20px) | styles-modern.css:101-172 | Vereinheitlichen |
| TS-M14 | CSS: Fallback-Farben `#1da6a0` hardcoded statt CSS-Variable | styles.css:3705 | `var(--accent-2)` ohne Fallback |

### NIEDRIG

| ID | Finding | Datei |
|----|---------|-------|
| TS-L1 | `checkModelOnStartup` ruft `get_settings` 2x auf, shadowt Modul-Level-Variable | main.ts:670-709 |
| TS-L2 | `pasteQueue` wird bei Re-Bootstrap nicht zurückgesetzt | main.ts:103 |
| TS-L3 | Ungetypte Tauri-Event-Payloads (`event.payload as "idle" | "recording"`) | main.ts:331,336,636 |
| TS-L4 | `wrapper.className` zweifach gesetzt (redundante Zuweisung) | history.ts:846-848 |
| TS-L5 | `syncAIRefinementExpanders` liest localStorage bei jedem Render | settings.ts:314-329 |
| TS-L6 | `accessibility.ts` Dataset-Mapping nicht exhaustive type-safe | accessibility.ts:31-55 |
| TS-L7 | `chapters.ts` TODO: scrollt immer zur ersten statt nächsten Entry | chapters.ts:191 |

---

## Verification nach allen Fixes

```bash
cargo build                   # Rust kompiliert
npm run build                 # TypeScript/Vite kompiliert
npm test                      # Unit Tests bestehen
cargo tauri dev               # App startet und funktioniert
```
