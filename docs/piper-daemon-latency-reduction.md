# Runbook: Piper TTS Daemon — Latenz-Reduktion von ~15s auf <1s

## Kontext

Piper wird aktuell als **Fresh-Process-per-Request** gestartet:
`synthesize_piper_to_wav()` → `Command::new(piper.exe)` → `.wait()` → 13-17s Latenz

**Hauptursache:** ONNX-Modell wird bei jedem Aufruf neu von Disk geladen (8-12s).

**Ziel:** Daemon-Modus → Modell einmal laden → alle weiteren Requests in <1s.

---

## Piper Daemon-Modus (CLI-Feature)

Piper unterstützt persistente Nutzung via Flags:
```
piper.exe --model <path.onnx> --json_input --output_raw
```
- `--json_input`: liest JSON-Lines von stdin (`{"text": "...", "speaker_id": 0}`)
- `--output_raw`: schreibt rohe 16-bit PCM auf stdout (kein WAV-Header)
- Prozess bleibt am Leben und wartet auf weitere stdin-Zeilen
- Modell bleibt im RAM zwischen Requests

---

## Startup-Logik (wie User besprochen)

| Situation | Wann starten |
|-----------|-------------|
| Piper = **Primary Provider** | Beim Aktivieren des Voice-Output-Moduls (Tauri `module:state-changed`) |
| Piper = **Fallback Provider** | Beim ersten TTS-Request der den Fallback triggert (lazy) |
| Provider wechselt zu Piper | Beim nächsten Request (lazy, on-demand) |
| Modell-Pfad ändert sich | Daemon killen + neu starten mit neuem Modell |

---

## Architektur: `PiperDaemon` Struct

**Datei:** `src-tauri/src/multimodal_io.rs` (neue Struct + Methoden)

```rust
struct PiperDaemon {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    model_path: String,        // um Modell-Wechsel zu erkennen
}

impl PiperDaemon {
    fn spawn(binary: &Path, model: &Path, rate: f32) -> Result<Self, String>
    fn synthesize(&mut self, text: &str) -> Result<Vec<i16>, String>
    fn is_alive(&mut self) -> bool
}
```

**Globaler State** (via `AppState` oder separates `Mutex<Option<PiperDaemon>>`):
```rust
pub struct PiperDaemonState {
    daemon: Mutex<Option<PiperDaemon>>,
}
```

---

## Synthesize-Flow im Daemon-Modus

1. Mutex locken
2. Falls kein Daemon oder Daemon dead → spawnen
3. `{"text": "..."}` als JSON-Line auf stdin schreiben
4. Rohes PCM von stdout lesen (bis Stille-Marker oder feste Byte-Länge)
5. PCM direkt an cpal weitergeben (kein Temp-WAV!)
6. Mutex freigeben

**PCM-Parameter:** Piper output raw = 16-bit signed, mono, 22050 Hz (modell-abhängig)
→ Sample-Rate aus der `.onnx.json` Begleitdatei lesen (immer vorhanden neben .onnx)

---

## Integration in bestehenden Code

| Wo | Was ändern |
|----|-----------|
| `speak_piper()` in `multimodal_io.rs` | `synthesize_piper_to_wav()` ersetzen durch `daemon.synthesize()` |
| `speak_tts_internal()` in `lib.rs` | Bei Primary-Provider-Init: `PiperDaemon::spawn()` aufrufen |
| `AppState` in `lib.rs` | `piper_daemon: PiperDaemonState` hinzufügen |
| `module:state-changed` handler | Daemon starten wenn Piper Primary + Modul aktiv |
| `save_settings` handler | Daemon neustarten wenn piper_model_path geändert |

---

## Modelle: Deutsch + Englisch

**Kein Multilingual-Modell verfügbar** — separate Modelle pro Sprache nötig.

Empfohlene Modelle:

| Sprache | Modell | Qualität |
|---------|--------|----------|
| Deutsch | `de_DE-thorsten-medium` | Bereits im Einsatz (auto-download) |
| Englisch US | `en_US-lessac-medium` | Gute Balance |
| Englisch GB | `en_GB-cori-high` | Höchste EN-GB Qualität |

**Für den ersten Daemon-Schritt:** Ein Daemon pro Sprache ist möglich, aber komplex.
**Pragmatisch für v1:** Ein Daemon (DE), englische Texte via Windows-Native TTS (hat native EN-Stimmen).

---

## Fehlerbehandlung

| Fehlerfall | Verhalten |
|-----------|-----------|
| Daemon stirbt während Request | Neustart + Request wiederholen (1x) |
| Neustart fehlgeschlagen | Fallback: alter `synthesize_piper_to_wav()` subprocess |
| Modell nicht gefunden | Fehler an Frontend (kein Daemon-Start) |
| Timeout auf PCM-Read | Nach 10s → Daemon killen + Fehler |

---

## PCM-Frame-Ende erkennen (wichtig!)

Piper im `--output_raw` Modus sendet PCM bis das Ende der Synthese — kein expliziter Frame-End-Marker.

**Lösung:** Piper sendet genau ein Audio-Segment pro JSON-Input-Line. Nach dem Senden der JSON-Line: stdout lesen bis Piper **keine weiteren Bytes** sendet (kurzes Read-Timeout ~50ms nach letztem Byte).

Alternativ: `--output_file /dev/stdout` mit WAV-Header nutzen → Header enthält Länge → definiertes Ende.

---

## Dateien die geändert werden müssen

```
src-tauri/src/multimodal_io.rs   — PiperDaemon struct + spawn/synthesize
src-tauri/src/lib.rs             — AppState + Daemon lifecycle hooks
src-tauri/src/state.rs           — PiperDaemonState als Default
```

---

## Verifikation nach Implementierung

```
1. Voice Output Modul aktivieren, Piper als Primary
   → Daemon wird beim Modul-Start gestartet (log: "[piper-daemon] spawned")
2. TTS-Request absetzen
   → Erste Response: ~3-5s (Model-Load)
   → Weitere Responses: <1s
3. Fallback-Pfad: Primary = Windows, Fallback = Piper
   → Daemon startet erst beim ersten Fallback-Trigger
4. Modell-Pfad ändern in Settings
   → Daemon neustart (log: "[piper-daemon] restarting — model changed")
5. App neu starten
   → Daemon läuft nicht → startet lazy beim ersten Request
```
