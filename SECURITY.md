# Security Audit — Trispr Flow

> Erstellt: 2026-02-26 | Status: **Checkliste + bekannte Findings**
> Vollständiges Deep-Dive Review steht noch aus (geplant: nächste Session mit Opus).

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

## Bekannte Findings

### HOCH: CSP deaktiviert

**Datei:** `src-tauri/tauri.conf.json`
```json
"security": { "csp": null }
```

**Problem:** Content Security Policy ist komplett deaktiviert. In Kombination mit `withGlobalTauri: true` kann jede XSS-Schwachstelle im WebView direkt alle Tauri-Commands aufrufen — inklusive Filesystem-Zugriff und Prozess-Spawning.

**Fix:** CSP aktivieren mit mindestens:
```json
"csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src http://localhost:* http://127.0.0.1:* https://api.anthropic.com https://api.openai.com https://generativelanguage.googleapis.com; img-src 'self' data:"
```

**Priorität:** Hoch — aber vor Aktivierung testen, ob Vite-Injects und Ollama-Connects weiterhin funktionieren.

---

### MITTEL: `encode_to_opus` — beliebige Pfade ohne Validierung

**Datei:** `src-tauri/src/lib.rs` — `encode_to_opus` Command

**Problem:** `input_path` und `output_path` sind user-controlled Strings, die direkt an FFmpeg übergeben werden. Keine Prüfung, ob die Pfade innerhalb des App-Datenverzeichnisses liegen.

```rust
fn encode_to_opus(input_path: String, output_path: String, bitrate_kbps: Option<u32>)
```

**Risiko:** Ein kompromittiertes Frontend könnte beliebige Dateien lesen (als FFmpeg-Input) oder überschreiben (als Output).

**Fix:** Pfade gegen `app.path().app_data_dir()` oder ein explizites Recordings-Verzeichnis validieren:
```rust
fn validate_path_within(path: &Path, allowed_root: &Path) -> Result<PathBuf, String> {
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(allowed_root) {
        return Err("Path outside allowed directory".into());
    }
    Ok(canonical)
}
```

---

### MITTEL: innerHTML ohne HTML-Escaping (Stored XSS Pattern)

**Datei:** `src/history.ts` — `highlightSearchMatches()`

**Problem:** Transkribierter Text wird ohne HTML-Escaping in `innerHTML` geschrieben:
```typescript
node.innerHTML = highlightSearchMatches(text);
// text kommt aus History-State (Spracheingabe des Users)
```

**Risiko:** Wenn ein Transkript zufällig oder absichtlich HTML enthält (z.B. ein diktierter HTML-Tag), wird es als HTML interpretiert. In einem lokalen Tauri-App-Kontext ist die Ausnutzbarkeit gering, aber strukturell ist es ein XSS-Vektor.

**Fix:** Text erst escapen, dann Highlights anwenden:
```typescript
function highlightSearchMatches(text: string): string {
  const escaped = escapeHtml(text);
  const regex = new RegExp(`(${currentSearchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  return escaped.replace(regex, "<mark>$1</mark>");
}
```

---

### NIEDRIG: `buildTopicBadges` — Topics ohne Escaping

**Datei:** `src/history.ts` — `buildTopicBadges()`

**Problem:** Topic-Strings (aus Transkript-Analyse) werden unescaped in innerHTML und `data-topic` Attribut geschrieben:
```typescript
`<span class="topic-badge" data-topic="${topic}">${topic}</span>`
```

**Fix:** `escapeHtml()` auf `topic` anwenden (Funktion existiert bereits in `refinement-inspector.ts`, nach `src/utils.ts` extrahieren und teilen).

---

### NIEDRIG: `postproc_llm_api_key` im Settings-JSON

**Datei:** `src-tauri/src/ai_fallback/models.rs`

**Problem:** Das Feld `postproc_llm_api_key` ist ein Placeholder in der serialisierten Settings-Struktur. Falls es jemals befüllt wird, landet der Key im Klartext in der JSON-Datei statt im OS-Keyring.

**Fix:** Feld entfernen oder mit `#[serde(skip)]` markieren. Alle API-Keys ausschließlich über den Keyring-Pfad speichern.

---

## Positive Findings (was gut läuft)

| Bereich | Status |
|---------|--------|
| **Tauri Capabilities** | Minimal: nur `core:default` + `dialog:default` |
| **Keine gefährlichen Plugins** | Kein `shell`, `fs`, `http` Plugin — Filesystem/Shell nur über eigene Commands |
| **Ollama Model-Name Validation** | `validate_ollama_model_name()` konsistent angewendet |
| **Whisper Model-File Validation** | `validate_model_file_name()` für FS-Operationen |
| **Ollama Install SHA-256 Check** | Archiv wird gegen gepinnten Manifest-Hash verifiziert |
| **Cloud API Keys im Keyring** | Nicht im Settings-JSON, sondern im OS-Keyring (`keyring` crate) |
| **Keine hardcoded Secrets** | Kein einziger API-Key oder Token im Quellcode |
| **Kein eval() / new Function()** | Keine dynamische Code-Ausführung im Frontend |
| **Refinement-Inspector** | `escapeHtml()` korrekt angewendet bei Diff-Rendering |
| **Input-Sanitization** | `sanitize_ollama_refinement_output()` mit Heuristik gegen Halluzinationen |
| **Strict Local Mode** | Endpoint-Validierung auf localhost wenn aktiviert |

---

## Offene Prüfpunkte (Deep-Dive Review)

Diese Punkte müssen im vollständigen Security-Review geprüft werden:

### Tauri IPC Boundary
- [ ] Alle `#[tauri::command]` Funktionen auf Input-Validierung prüfen
- [ ] Race Conditions bei concurrent Command-Aufrufen (z.B. gleichzeitig `save_settings` + `refine_transcript`)
- [ ] Error-Messages auf Information Disclosure prüfen (Pfade, Stack-Traces)

### HTTP / Netzwerk
- [ ] Ollama-Kommunikation: TLS-Validierung bei non-localhost Endpoints
- [ ] SSRF-Potential: Kann `save_ollama_endpoint` auf interne Netzwerk-Adressen zeigen?
- [ ] Cloud-Provider-Requests: Certificate Pinning? Timeout-Handling?
- [ ] Ollama-Download: HTTPS + SHA-256 — ist die Manifest-Quelle vertrauenswürdig?

### Filesystem
- [ ] TOCTOU-Races bei Ollama-Install (check → extract → rename)
- [ ] Symlink-Angriffe auf Recordings-Verzeichnis oder Temp-Dateien
- [ ] Berechtigungen der erstellten Dateien (world-readable?)
- [ ] Crash-Recovery-Datei in TEMP — andere Prozesse könnten sie manipulieren

### Subprocess Management
- [ ] Ollama serve: Wird der Prozess sauber beendet? Zombie-Prozesse?
- [ ] FFmpeg: Argument-Injection über Dateinamen? (z.B. `--` Separator)
- [ ] PATH-Hijacking: Werden Binaries über absolute Pfade aufgerufen?

### Clipboard / Keyboard
- [ ] Clipboard-Inhalte werden nicht gesäubert — was wenn vorher sensitiver Inhalt im Clipboard war?
- [ ] Timing-Angriffe auf Paste-Operation (andere Apps könnten Clipboard auslesen)

### localStorage / Persistenz
- [ ] Refinement-Snapshots: Werden sie bei "Clear History" auch gelöscht?
- [ ] Maximale Größe? Kann localStorage voll laufen?
- [ ] Werden Transkript-Inhalte in localStorage gespeichert, die sensibel sein könnten?

### Updates / Supply Chain
- [ ] Tauri Updater konfiguriert? Signaturprüfung?
- [ ] NSIS Installer: Signiert? Elevated privileges?
- [ ] Dependency-Audit: `cargo audit`, `npm audit`

---

## Security Guidelines für Entwicklung

### 1. HTML-Output
```
REGEL: Niemals user-derived Strings in innerHTML ohne escapeHtml().
```
- `textContent` bevorzugen wo immer möglich
- `innerHTML` nur für strukturiertes HTML mit escaped Variablen
- `escapeHtml()` aus `refinement-inspector.ts` in shared Utility extrahieren

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
- `#[serde(skip)]` für Key-Felder in serialisierbaren Structs
- Tracing/Logging: Keine Secrets in warn!/error! Nachrichten

### 4. HTTP
```
REGEL: Alle externen HTTP-Requests mit angemessenen Timeouts und TLS.
```
- Connect-Timeout ≤ 5s, Read-Timeout je nach Kontext
- TLS für alle non-localhost Endpoints erzwingen
- SSRF: Endpoint-Validierung auf erlaubte Hosts/Ports

### 5. CSP
```
REGEL: CSP aktivieren sobald die App stabil läuft.
```
- `default-src 'self'` als Basis
- `connect-src` explizit für Ollama localhost + Cloud-APIs
- `style-src 'unsafe-inline'` nur falls nötig (Vite Dev-Mode)
- Kein `unsafe-eval`, kein `unsafe-inline` für Scripts

### 6. Subprocess
```
REGEL: Alle externen Binaries über absolute Pfade aufrufen.
```
- Niemals `Command::new("ollama")` ohne vollen Pfad
- `--` Separator vor user-derived Argumenten bei CLI-Tools
- Stdout/Stderr begrenzen (keine unbegrenzten Buffer-Reads)
