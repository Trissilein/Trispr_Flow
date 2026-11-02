# Runbook: Piper TTS Daemon - Latenz-Reduktion von ~15s auf <1s

## Kontext

Piper wird aktuell als Fresh-Process-per-Request gestartet:
`synthesize_piper_to_wav()` -> `Command::new(piper.exe)` -> `.wait()` -> 13-17s Latenz.

Hauptursache: Das ONNX-Modell wird bei jedem Request neu geladen.

Ziel: Persistenter Daemon, Modell nur einmal laden, Folgerequests ohne Kaltstart.

---

## v1 Transport (deterministisch)

Wir nutzen **nicht** `--output_raw`, sondern:

```bash
piper.exe --model <path.onnx> --json_input --length_scale <value>
```

Pro Request senden wir genau eine JSON-Line mit eigener Ausgabe-Datei:

```json
{"text":"...","output_file":"<temp.wav>"}
```

Warum so:
- `output_file` liefert ein klares Ende pro Request (kein Frame-Timeout-Guessing).
- Piper schreibt pro JSON-Line einen `stdout`-Ack (Dateipfad), den wir request-weise lesen.
- Playback bleibt auf der bestehenden WAV/cpal-Pipeline.

---

## Daemon-Key und Restart-Regel

Daemon-Identitat in v1:
- `binary_path`
- `model_path`
- `rate` (normalisiert, 3 Nachkommastellen)

Wenn einer dieser Werte abweicht, wird der Daemon gestoppt und mit neuer Konfiguration gestartet.

Rate-Verhalten:
- `rate` wird beim Spawn via `--length_scale` gesetzt.
- Bei Rate-Wechsel erfolgt Daemon-Restart (kein stilles Ignorieren).

## Runtime-Abhängigkeiten (Windows)

Zusätzlich zu `piper.exe` müssen im selben Verzeichnis vorhanden sein:
- `onnxruntime.dll`
- `onnxruntime_providers_shared.dll`
- `espeak-ng.dll`
- `piper_phonemize.dll`
- `libtashkeel_model.ort`
- `espeak-ng-data/` (Ordner)

Fehlen diese Dateien, bricht der Preflight mit klarer Fehlermeldung ab (statt Prozess-Start mit Windows-DLL-Popup).

---

## Lifecycle (v1)

| Situation | Verhalten |
|---|---|
| Voice-Output Modul aktiviert + Piper ist Primary | Daemon prewarm im Hintergrund |
| Piper nur Fallback | Kein eager Start, lazy beim ersten echten Piper-Request |
| Settings geändert (binary/model/rate) | Reconcile: Daemon neu starten falls Key anders |
| Piper nicht mehr Primary oder Modul deaktiviert | Daemon stoppen |
| App beendet | Daemon im zentralen Cleanup stoppen |

Wichtig: Kein Backend-Start/Stop an `module:state-changed` hängen; direkte Hooks in `enable_module`, `disable_module`, `save_settings` und Exit-Cleanup nutzen.

---

## Request-Flow in `speak_piper`

1. Daemon-Key aus Request berechnen.
2. Daemon unter Mutex prüfen (`ensure_matching_daemon`), bei Bedarf spawnen/restarten.
3. JSON-Line (`text` + `output_file`) an stdin schreiben + flush.
4. Einen `stdout`-Ack mit Timeout lesen.
5. WAV-Datei validieren und wie bisher abspielen.
6. Mutex freigeben vor Playback.

Fehlerpfad v1:
- Daemon-Fehler -> Daemon stoppen, **1x restart+retry**.
- Retry ebenfalls fehlerhaft -> Fallback auf bestehenden Legacy subprocess-WAV-Pfad.

---

## Geplante Codeintegration

- `src-tauri/src/multimodal_io.rs`
  - `PiperDaemon`, `PiperDaemonState`, `daemon_config_from_request`, `ensure_matching_daemon`, `prewarm_piper_daemon`, `shutdown_piper_daemon`.
  - `speak_piper` auf Daemon-Pfad mit Retry+Legacy-Fallback umstellen.
- `src-tauri/src/state.rs`
  - `AppState` um `piper_daemon` erweitern.
- `src-tauri/src/lib.rs`
  - `enable_module(output_voice_tts)`: Primary-Prewarm.
  - `save_settings_inner`: Daemon-Reconcile.
  - `disable_module(output_voice_tts)`: Daemon-Stop.
  - App-Exit-Cleanup: Daemon-Stop.

---

## Out of Scope fur v1

- Kein Multi-Daemon-Setup pro Sprache.
- Kein Benchmark-Umbau auf Daemon (Benchmark bleibt auf bestehendem Legacy-Syntheseweg).
- Kein `--output_raw` Framing mit Inaktivitats-Timeout.

---

## Verifikation nach Implementierung

1. Modul `output_voice_tts` aktivieren, Primary=`local_custom`.
   - Erwartung: Prewarm-Log erscheint (`[piper-daemon] spawned` / `prewarm complete`).
2. Zwei TTS-Requests hintereinander senden.
   - Erwartung: Erster Request kann Kaltstartkosten tragen, danach deutlich schneller.
3. Fallback-Szenario: Primary=`windows_native`, Fallback=`local_custom`.
   - Erwartung: Kein Prewarm, Daemon startet erst beim ersten echten Fallback-Trigger.
4. `piper_model_path` oder `rate` ändern und speichern.
   - Erwartung: Reconcile startet Daemon mit neuem Key.
5. Daemon-Prozess extern beenden wahrend Runtime.
   - Erwartung: Nächster Request startet Daemon neu; bei Fehler greift Retry+Legacy.
6. Modul deaktivieren.
   - Erwartung: Daemon wird gestoppt.
7. App beenden.
   - Erwartung: Daemon wird im globalen Cleanup beendet.
