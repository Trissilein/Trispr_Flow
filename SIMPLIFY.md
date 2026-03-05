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

## Findings-Status — Rust + TypeScript (alle 3 Wellen abgeschlossen)

### Rust HOCH — Security

| ID | Finding | Status |
|----|---------|--------|
| H1 | `is_ssrf_target()` Fail-Open | ✅ |
| H2 | SSRF IPv6 nicht abgedeckt | ✅ |
| H3 | UNC-Pfade → NTLM-Leak | ✅ |
| P-H1 | WAV Temp-File-Leak | ✅ |
| P-H2 | Whisper `output()` ohne Timeout | ✅ |
| P-H3 | Transcription-Result-Handling 3× | ✅ |

### Rust MITTEL — Robustheit + Duplikation

| ID | Finding | Status |
|----|---------|--------|
| M1 | Exit-Handler Mutex-Poisoning | ✅ |
| M2 | Pull-Cleanup Drop-Guard | ✅ |
| M3 | `save_settings_file` nicht atomar | ✅ |
| M4 | Settings-Save+Emit Boilerplate 15× | ✅ (7 Stellen) |
| M5 | Strict-Local-Mode Guard 12× | ✅ (10 Stellen) |
| M6 | Refinement-Setup 3× dupliziert | ✅ (`prepare_refinement()`) |
| M7 | Provider-ID-Normalisierung 5 Funktionen | ✅ (models.rs kanonisch) |
| M8 | Full History Write pro Eintrag (HOT) | ✅ (200ms Debounce) |
| M9 | Sync HTTP blockiert Thread-Pool | ⏭ Übersprungen (zu riskant) |
| M10 | `validate_ollama_model_name` blockiert `/` | ✅ |
| M11 | Window-Drag ohne Debounce | ✅ (500ms AtomicU64) |
| M12 | `Vec::insert(0)` statt `VecDeque` | ✅ |
| P-M1 | `ContinuousDumpEvent` 2× definiert | ✅ |
| P-M2 | `lock().unwrap()` 36 Stellen | ⏭ Übersprungen (zu riskant) |
| P-M3 | Regex pro Wort neu kompiliert | ✅ (OnceLock Cache) |
| P-M4 | `build_input_stream` 3× Callbacks | ✅ (Makro) |
| P-M5 | `update_overlay_state` OS-Thread | ✅ (spawn_blocking) |
| P-M6 | Whisper Seiteneffekte nicht aufgeräumt | ✅ |
| P-M7 | `paths.rs` Fallback ohne Warnung | ✅ |
| P-M8 | Question-Detection 3× dupliziert | ✅ |

### Rust NIEDRIG

| ID | Finding | Status |
|----|---------|--------|
| L1 | `ureq::Agent` kein Connection-Reuse | ✅ (OnceLock shared_agent) |
| L2 | `now_iso()` dupliziert | ✅ |
| L3 | SHA-256 Doppel-Hash | ⏭ Übersprungen (absichtlich doppelt — Security) |
| L4 | Window-Geometry-Restore 2× | ✅ |
| L5 | `Settings::default()` bei normalize | ✅ (const-Werte) |
| L6 | `sanitize_model_name` / `sanitize_session_name` | ⏭ Übersprungen (zu unterschiedlich) |
| L7 | `transcribe_active` vs `transcribe_enabled` | ⏭ Übersprungen (State-Semantik unklar) |
| L8 | Prompt-Profile-Normalisierung 2× | ✅ |
| L9 | `resolve_runtime_root` umgeht paths.rs | ✅ (Kommentar + Begründung) |
| P-L1 | `mark_overlay_ready()` leerer Body | ✅ (Kommentar) |
| P-L2 | `whisper_cli_supports_gpu_layers` --help jedes Mal | ✅ (OnceLock Cache) |
| P-L3 | `refine_with_llm` toter Stub | ✅ (entfernt) |
| P-L4 | `eprintln!` statt `tracing::error!` | ✅ |

### TypeScript HOCH

| ID | Finding | Status |
|----|---------|--------|
| TS-H1 | `window.addEventListener` 3× ohne Cleanup | ✅ |
| TS-H2 | AI-Provider-Normalisierung 6 Funktionen dupliziert | ✅ (`ai-provider-utils.ts`) |
| TS-H3 | `ensureContinuousDumpDefaults` dupliziert | ✅ |
| TS-H4 | Render-Tripel 5× | ✅ (`refreshAIUi()`) |
| TS-H5 | `handleOllamaPull` fehlendes `finally` | ✅ |
| TS-H6 | `detectTopics` 18 RegExp pro Render | ✅ (Map-Cache) |
| TS-H7 | Event-Listener-Leak bei `renderHistory()` | ✅ (Event-Delegation) |
| TS-H8 | Tooltip Memory Leak | ✅ (`cleanupUnifiedTooltips()`) |

### TypeScript MITTEL

| ID | Finding | Status |
|----|---------|--------|
| TS-M1 | `refreshModelsDir()` kein `.catch()` | ✅ |
| TS-M2 | Identische `forcedLocal`-Branches | ✅ |
| TS-M3 | 24 identische change-Persist-Handler | ✅ (`onChangePersist()`) |
| TS-M4 | History-Update-Handler 2× | ✅ (Factory) |
| TS-M5 | Inline DOM-Queries | ✅ (dom-refs.ts) |
| TS-M6 | `postproc_llm` sync 7× | ✅ (`syncLegacyProviderFields()`) |
| TS-M7 | `persistCurrentSettings` Duplikat | ⏭ Offen (Circular Import Blocker) |
| TS-M8 | Circular Import settings.ts ↔ event-listeners.ts | ⏭ Übersprungen (Architektur-Risiko) |
| TS-M9 | `renderHistory/Settings` ohne RAF-Batching | ✅ (`scheduleHistoryRender/scheduleSettingsRender`) |
| TS-M10 | localStorage ohne Größenlimit | ✅ (Cap 100 + console.warn) |
| TS-M11 | `clipboard.writeText` ohne try/catch | ✅ |
| TS-M12 | Dead ternary in Pipeline-Graph | ✅ |
| TS-M13 | CSS backdrop-filter inkonsistent | ✅ (16px vereinheitlicht) |
| TS-M14 | Hardcoded Farbe `#1da6a0` | ✅ (`var(--accent-2)`) |

### TypeScript NIEDRIG

| ID | Finding | Status |
|----|---------|--------|
| TS-L1 | `checkModelOnStartup` ruft `get_settings` 2× auf | ✅ |
| TS-L2 | `pasteQueue` bei Re-Bootstrap nicht reset | ✅ |
| TS-L3 | Ungetypte Tauri-Event-Payloads | ✅ (benannte Types) |
| TS-L4 | `wrapper.className` zweifach gesetzt | ✅ |
| TS-L5 | `syncAIRefinementExpanders` liest localStorage immer | ✅ (In-Memory-Cache) |
| TS-L6 | accessibility.ts nicht exhaustive | ✅ (Record-Annotation) |
| TS-L7 | chapters.ts scrollt zur falschen Entry | ✅ (Timestamp-Nähe) |

---

## Bewusst offengelassene Findings

Diese 6 Findings wurden nach Abwägung **absichtlich nicht gefixt**. Sie sind kein Versehen.

| ID | Finding | Warum offen gelassen |
| -- | ------- | -------------------- |
| **M9** | Sync HTTP blockiert Tauri Thread-Pool | Alle `ureq`-Aufrufe sitzen in `#[tauri::command]`-Funktionen, die Tauri bereits auf einem Blocking-Thread ausführt. Eine Migration auf `reqwest` async würde alle Provider-Aufrufe, State-Guards und Error-Propagation neu schneiden — zu hohes Regressionsrisiko ohne End-to-End-Testabdeckung für Cloud-Provider (Block I noch nicht implementiert). |
| **P-M2** | `lock().unwrap()` an 36 Stellen | Mutex-Poisoning tritt nur auf, wenn ein Thread mit gehaltener Lock panict. Das passiert in diesem Codebase nur bei echten Bugs — die man dann lieber als Panic sieht als als Silent-Corruption. Eine flächendeckende Migration auf `lock().unwrap_or_else(PoisonError::into_inner)` würde Silent-Recovery einführen, ohne das Root-Problem zu fixen. Offen bis konkreter Anlass (reproduzierbarer Poison-Fall) auftritt. |
| **L6** | `sanitize_model_name` vs. `sanitize_session_name` zusammenführen | Die Funktionen haben unterschiedliche erlaubte Zeichen (`.` und `-` vs. `-` und `_`), unterschiedliche Case-Behandlung (preserve vs. lowercase), unterschiedliche Ersetzungszeichen und unterschiedliche Max-Längen. Eine gemeinsame Abstraktion würde Parameter-Sprawl erzeugen, der schwerer lesbar ist als zwei klare Einzelfunktionen. |
| **L7** | `transcribe_active` vs. `transcribe_enabled` konsolidieren | Die State-Semantik ist unklar: `transcribe_enabled` ist eine persistierte User-Einstellung (Settings-JSON), `transcribe_active` ist transienter Laufzeit-State (läuft gerade eine Aufnahme?). Beide zu einem Feld zusammenzuführen würde diese Unterscheidung verwischen und ist ein Architektur-Entscheid, der Diskussion braucht. |
| **TS-M7** | `persistCurrentSettings` in `ollama-models.ts` dupliziert | Blockiert durch TS-M8: Um `persistSettings` aus `settings.ts` importieren zu können, müsste der Circular Import aufgebrochen werden. Das Duplikat ist minimal (3 Zeilen) und hat korrekte Error-Behandlung — vertretbar bis zum Architektur-Refactor. |
| **TS-M8** | Circular Import `settings.ts` ↔ `event-listeners.ts` | Beide Module importieren gegenseitig Funktionen. Auflösung erfordert Extraktion eines `render-core.ts` o.ä. Moduls — ein größerer Architektur-Eingriff der am besten in einem dedizierten Refactor-Branch landet, nicht als Teil dieses Feature-Branches. |

---

## Verification nach allen Fixes

```bash
cargo build                   # Rust kompiliert
npm run build                 # TypeScript/Vite kompiliert
npm test                      # Unit Tests bestehen
cargo tauri dev               # App startet und funktioniert
```
