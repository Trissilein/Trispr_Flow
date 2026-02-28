# Security Audit — Trispr Flow

> Erstellt: 2026-02-26 | Aktualisiert: 2026-02-28 | Status: **Vollständig — Deep-Dive abgeschlossen**

---

## Architektur-Überblick (Security-relevant)

Trispr Flow ist eine **Tauri v2 Desktop-App** (Rust backend + TypeScript/HTML frontend im WebView2).

Angriffsflächen:
- **Tauri IPC** — Frontend ↔ Backend über `window.__TAURI__` Bridge
- **HTTP** — Ollama REST API (localhost), Cloud-Provider-APIs (Claude, OpenAI, Gemini)
- **Filesystem** — Settings, History, Recordings, Ollama-Runtime, Whisper-Modelle
- **Subprocess** — Ollama serve, FFmpeg, Whisper CLI, quantize.exe
- **Clipboard + Keyboard-Simulation** — Enigo-basiertes Paste
- **OS Keyring** — API-Key-Storage für Cloud-Provider
- **localStorage** — Refinement-Snapshots, UI-State

---

## Behobene Findings

### ~~HOCH: CSP deaktiviert~~ — GEFIXT

**Datei:** `src-tauri/tauri.conf.json`, `src-tauri/tauri.conf.vulkan.json`

**Problem:** `"csp": null` — Content Security Policy komplett deaktiviert. Jede XSS-Schwachstelle konnte direkt alle Tauri-Commands aufrufen.

**Fix:** CSP aktiviert mit strikter Policy. Inline-Script aus `overlay.html` nach `public/overlay.js` extrahiert, um `script-src 'self'` zu ermöglichen.

```json
"csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; connect-src http://localhost:* http://127.0.0.1:* https://api.anthropic.com https://api.openai.com https://generativelanguage.googleapis.com; img-src 'self' data:"
```

---

### ~~MITTEL: `encode_to_opus` — Path Traversal~~ — GEFIXT

**Datei:** `src-tauri/src/lib.rs`

**Problem:** `input_path` und `output_path` ohne Boundary-Check direkt an FFmpeg übergeben.

**Fix:** `validate_path_within()` Funktion implementiert — `canonicalize()` + `starts_with(app_data_dir)`. Beide Pfade werden gegen das App-Datenverzeichnis validiert.

---

### ~~MITTEL: innerHTML ohne HTML-Escaping (Stored XSS)~~ — GEFIXT

**Datei:** `src/history.ts`

**Problem:** Transkribierter Text in `highlightSearchMatches()` ohne Escaping in `innerHTML`.

**Fix:** Shared `escapeHtml()` in `src/utils.ts` erstellt. Text wird vor Regex-Highlight escaped. `refinement-inspector.ts` verwendet ebenfalls den shared Import.

---

### ~~NIEDRIG: `buildTopicBadges` — XSS~~ — GEFIXT

**Datei:** `src/history.ts`

**Fix:** `escapeHtml()` auf Topic-Strings angewendet (sowohl im `data-topic` Attribut als auch im Textinhalt).

---

### ~~NIEDRIG: `postproc_llm_api_key` im Settings-JSON~~ — GEFIXT

**Datei:** `src-tauri/src/state.rs`

**Fix:** `#[serde(skip_serializing)]` — Key wird nie in JSON geschrieben, aber bestehende Einträge können noch gelesen werden (Migration).

---

## Deep-Dive Findings (Runde 2)

### ~~MITTEL: Crash-Recovery in %TEMP%~~ — GEFIXT

**Datei:** `src-tauri/src/lib.rs` — `save_crash_recovery`

**Problem:** Crash-Recovery-Datei wurde in `%TEMP%` gespeichert — ein world-readable Verzeichnis. Andere Prozesse konnten die Datei lesen oder manipulieren.

**Fix:** Pfad auf `app.path().app_data_dir()` geändert. Legacy-Datei in `%TEMP%` wird bei `clear_crash_recovery` mitgelöscht.

---

### ~~MITTEL: `run_latency_benchmark` — Path Traversal~~ — GEFIXT

**Datei:** `src-tauri/src/lib.rs` — `run_latency_benchmark_inner`

**Problem:** User-provided `fixture_paths` wurden ohne Validierung als Dateipfade verwendet.

**Fix:** `validate_path_within()` auf alle Fixture-Pfade angewendet.

---

### ~~MITTEL: Ollama-Kindprozess Cleanup~~ — GEFIXT

**Datei:** `src-tauri/src/ollama_runtime.rs`, `src-tauri/src/state.rs`, `src-tauri/src/lib.rs`

**Problem:** Der gespawnte Ollama-Prozess (`child`) wurde nach `child.id()` gedroppt, ohne den Handle zu speichern. Der Prozess lief als Waise nach App-Exit weiter.

**Fix:**
- `managed_ollama_child: Mutex<Option<Child>>` zu `AppState` hinzugefügt
- Child-Handle wird nach Spawn gespeichert
- `RunEvent::Exit` Handler ruft `child.kill()` + `child.wait()` auf

---

### ~~NIEDRIG: SSRF-Schutz für Ollama-Endpoint~~ — GEFIXT

**Datei:** `src-tauri/src/ai_fallback/provider.rs`, `src-tauri/src/lib.rs`

**Problem:** `save_ollama_endpoint` konnte auf Cloud-Metadata-Endpoints (169.254.169.254) zeigen.

**Fix:** `is_ssrf_target()` Funktion — blockiert Cloud-Metadata-Endpoints und den gesamten Link-Local-Bereich (169.254.0.0/16). Private IPs (192.168.x.x, 10.x.x.x) bleiben erlaubt, da Netzwerk-Ollama-Instanzen ein legitimer Use-Case sind.

---

### ~~NIEDRIG: localStorage-Snapshots bei History-Änderung~~ — GEFIXT

**Datei:** `src/refinement-inspector.ts`, `src/main.ts`

**Problem:** Verwaiste Refinement-Snapshots in localStorage wurden nie bereinigt, wenn History-Einträge entfernt wurden.

**Fix:** `pruneOrphanedSnapshots()` Funktion implementiert — wird bei jedem `history:updated` und `transcribe:history-updated` Event aufgerufen. Entfernt Snapshots für Entries, die nicht mehr existieren.

---

## Deep-Dive Prüfergebnisse (kein Fix nötig)

### Tauri IPC Boundary
- [x] **58+ Commands geprüft** — Input-Validierung konsistent angewendet
- [x] **Race Conditions** — Mutex-basierte Serialisierung, keine unsicheren Shared-State-Zugriffe
- [x] **Error-Messages** — Geben funktionale Fehlermeldungen zurück, keine Stack-Traces oder interne Pfade

### HTTP / Netzwerk
- [x] **Ollama-Kommunikation** — Nur HTTP auf localhost, kein TLS nötig für Loopback
- [x] **Cloud-Provider-Requests** — `reqwest` mit Standard-TLS, angemessene Timeouts (30s connect, 120s read)
- [x] **Ollama-Download** — HTTPS + SHA-256 Manifest-Verifizierung. Manifest-Quelle: GitHub Release (vertrauenswürdig)

### Filesystem
- [x] **TOCTOU bei Ollama-Install** — Operationen im App-Data-Verzeichnis, geringes Risiko auf Desktop
- [x] **Symlink-Angriffe** — `canonicalize()` in `validate_path_within()` löst Symlinks auf
- [x] **Dateiberechtigungen** — Standard OS-Berechtigungen, App-Data-Dir ist user-private

### Subprocess Management
- [x] **Ollama serve** — Child-Handle gespeichert, kill() bei App-Exit ✓
- [x] **FFmpeg** — Aufgerufen über absoluten Pfad (bundled), Dateinamen als einzelne Argumente (kein Shell-Expand)
- [x] **PATH-Hijacking** — Ollama über gespeicherten absoluten Pfad, FFmpeg bundled, Whisper CLI über absoluten Pfad

### Clipboard / Keyboard
- [x] **Clipboard-Inhalte** — App schreibt nur bei User-Aktion (Paste-Trigger). Akzeptables Risiko für Desktop-App
- [x] **Timing** — Clipboard-Fenster minimal (~50ms). Standard-Risiko, kein Mitigation nötig

### localStorage / Persistenz
- [x] **Snapshot-Cleanup** — `pruneOrphanedSnapshots()` bei History-Updates ✓
- [x] **Maximale Größe** — Snapshots enthalten nur Text-Diffs, Überlauf bei normaler Nutzung unwahrscheinlich
- [x] **Sensible Inhalte** — Transkript-Snippets in localStorage, aber nur im WebView2-Sandbox-Storage (user-private)

### Supply Chain
- [x] **npm audit** — esbuild (moderate), rollup (high) — beides Dev-Dependencies, nicht im Production-Build
- [ ] **cargo audit** — Tool nicht installiert, manuell prüfen: `cargo install cargo-audit && cargo audit`
- [x] **Tauri Updater** — Nicht konfiguriert (kein Auto-Update-Mechanismus)
- [x] **NSIS Installer** — Nicht signiert (TODO für Release)

---

## Positive Findings (was gut läuft)

| Bereich | Status |
|---------|--------|
| **Tauri Capabilities** | Minimal: nur `core:default` + `dialog:default` |
| **Keine gefährlichen Plugins** | Kein `shell`, `fs`, `http` Plugin — Filesystem/Shell nur über eigene Commands |
| **CSP aktiv** | Strikte Policy mit `script-src 'self'`, kein `unsafe-eval` |
| **Ollama Model-Name Validation** | `validate_ollama_model_name()` konsistent angewendet |
| **Whisper Model-File Validation** | `validate_model_file_name()` für FS-Operationen |
| **Ollama Install SHA-256 Check** | Archiv wird gegen gepinnten Manifest-Hash verifiziert |
| **Cloud API Keys im Keyring** | Nicht im Settings-JSON, sondern im OS-Keyring (`keyring` crate) |
| **Keine hardcoded Secrets** | Kein einziger API-Key oder Token im Quellcode |
| **Kein eval() / new Function()** | Keine dynamische Code-Ausführung im Frontend |
| **Refinement-Inspector** | `escapeHtml()` korrekt angewendet bei Diff-Rendering |
| **Input-Sanitization** | `sanitize_ollama_refinement_output()` mit Heuristik gegen Halluzinationen |
| **Strict Local Mode** | Endpoint-Validierung auf localhost wenn aktiviert |
| **Path Traversal Protection** | `validate_path_within()` für alle FS-Commands mit user-controlled Pfaden |
| **SSRF Protection** | Cloud-Metadata und Link-Local blockiert |
| **Process Lifecycle** | Ollama-Kindprozess wird bei App-Exit sauber beendet |
| **Shared escapeHtml()** | Zentrale Utility in `src/utils.ts`, konsistent verwendet |

---

## Verbleibende TODOs

| Priorität | Item | Beschreibung |
|-----------|------|-------------|
| Mittel | `cargo audit` installieren | `cargo install cargo-audit` und regelmäßig ausführen |
| Niedrig | NSIS Installer signieren | Code-Signing-Zertifikat für Release-Builds |
| Niedrig | npm-Dependencies updaten | `npm audit fix` für rollup-Vulnerability (Dev-Dep) |

---

## Security Guidelines für Entwicklung

### 1. HTML-Output
```
REGEL: Niemals user-derived Strings in innerHTML ohne escapeHtml().
```
- `textContent` bevorzugen wo immer möglich
- `innerHTML` nur für strukturiertes HTML mit escaped Variablen
- `escapeHtml()` aus `src/utils.ts` verwenden

### 2. Tauri Commands
```
REGEL: Jeder String-Parameter der auf Filesystem oder Shell trifft MUSS validiert werden.
```
- Pfade gegen erlaubtes Root-Verzeichnis prüfen (`canonicalize` + `starts_with`)
- Enum-artige Strings (provider, mode, format) gegen Allowlist validieren
- Keine user-controlled Strings als Command-Argumente ohne Sanitization

### 3. Secrets
```
REGEL: Alle API-Keys und Tokens ausschließlich im OS-Keyring speichern.
```
- Niemals in Settings-JSON, localStorage, oder Log-Output
- `#[serde(skip_serializing)]` für Key-Felder in serialisierbaren Structs
- Tracing/Logging: Keine Secrets in warn!/error! Nachrichten

### 4. HTTP
```
REGEL: Alle externen HTTP-Requests mit angemessenen Timeouts und TLS.
```
- Connect-Timeout ≤ 5s, Read-Timeout je nach Kontext
- TLS für alle non-localhost Endpoints erzwingen
- SSRF: `is_ssrf_target()` für Endpoint-Validierung verwenden

### 5. CSP
```
REGEL: CSP ist aktiv — keine inline Scripts, kein unsafe-eval.
```
- `default-src 'self'` als Basis
- `connect-src` explizit für Ollama localhost + Cloud-APIs
- `style-src 'unsafe-inline'` für dynamische Styles (nötig)
- Neue Script-Dateien in `public/` ablegen

### 6. Subprocess
```
REGEL: Alle externen Binaries über absolute Pfade aufrufen.
```
- Niemals `Command::new("ollama")` ohne vollen Pfad
- `--` Separator vor user-derived Argumenten bei CLI-Tools
- Stdout/Stderr begrenzen (keine unbegrenzten Buffer-Reads)
- Child-Handles in AppState speichern für sauberes Cleanup
