# Session Runbook: Recovery + Modul-Entkopplung + Assistant-Pivot

Status: Temporary working document for the implementation session on 2026-03-23.
Owner: Core repo maintainers.
Deletion policy: Delete this file after first implementation PR for Block S is merged, or after content is migrated to permanent docs.

## Context & Zielbild

Aktueller Stand:

- Repository ist development-ready, aber nicht release-ready.
- Es gibt Build-/Test-Blocker, die zuerst stabil gelöst werden müssen.
- Multimodal-Module sollen strikt entkoppelt sein (deaktiviert = keine aktive Capability).
- Danach folgt der strukturelle Schritt vom reinen STT-Tool in Richtung Assistant-Modus.

Zielbild für diese Umsetzungswelle:

1. Harte Build-Baseline (grün auf den definierten Gates).
2. Klares Modulverhalten mit deterministischen Enable/Disable-Semantiken.
3. Assistant-Foundation mit Mode-Switch und robuster Degradation.

## Phase A — Build Baseline Recovery (Hard Gates)

### Scope

- Nur Build-/Test-Stabilisierung und unmittelbare Blocker.
- Keine Feature-Erweiterungen außerhalb des Recovery-Bedarfs.

### Mandatory gates (in this order)

1. `npm run build`
2. `npm test`
3. `cargo test --lib`
4. `npm run tauri -- dev --no-watch` (Windows)

### Ziel

- Alle vier Gates sind grün.
- Keine offenen „known blockers“ mehr in Status/Roadmap für diese Baseline.

### Stop-Kriterien

- Wenn Gate 1 oder 2 fehlschlägt: Frontend/TS zuerst stabilisieren, dann neu starten.
- Wenn Gate 3 fehlschlägt: Rust-Test/Logik zuerst fixen, dann neu starten.
- Wenn Gate 4 fehlschlägt: Windows-runtime/compile issue priorisieren, dann neu starten.

## Phase B — Modul-Fertigstellung (harte Entkopplung)

### Leitregel

Modul aus = Capability aus.

Das gilt für alle relevanten Pfade (Commands, UI-Flow, Background-Work, Bridge-Calls).

### Muss-Kriterien

1. Einheitlicher Capability-Gate-Check für Vision/TTS/Agent.
2. Disable-Side-Effects sind deterministisch und sofort:
   - Vision disable stoppt Stream + leert RAM-Buffer.
   - TTS disable stoppt laufende Ausgabe.
   - Agent disable beendet laufende Agent-State-Maschinen.
3. Keine Schattenaktivität über Settings-Drift oder indirekte Aufrufe.
4. UI zeigt konsistenten Disabled-State, wenn Modul nicht aktiv ist.

### Akzeptanz

- Manuelle und automatisierte Checks belegen: keine aktive Capability bei deaktiviertem Modul.

### Stop-Kriterien

- Wenn ein einziger Bypass bleibt (z. B. Snapshot/Speak/Testpfad): Phase B nicht abschließen.

## Phase C — Assistant-Pivot Foundation

### Produktmodus

- `transcribe`: klassischer STT-first Flow.
- `assistant`: orchestrierter Assistenz-Flow.

### Minimaler Foundation-Umfang

1. Persistenter Mode-Switch in Settings/Frontend.
2. Assistant-State-Events für klare Zustandsführung.
3. Graceful Degradation:
   - ohne TTS: Textantwort statt Fehler.
   - ohne Vision: ohne Screen-Kontext weiterarbeiten.
4. Keine Endlosschleifen im Recovery/Restart-Verhalten.

### Akzeptanz

- Moduswechsel ohne Neustart.
- Assistant-Flow bleibt benutzbar, auch wenn einzelne Module deaktiviert sind.

### Stop-Kriterien

- Wenn Moduswechsel inkonsistent ist oder Degradation zu Abbruch führt, Phase C offen lassen.

## Akzeptanzkriterien pro Phase

### Phase A

- Alle vier Build-Gates grün.
- Keine neuen Regressionen in bestehenden Kernpfaden.

### Phase B

- Disable = hard-off für alle betroffenen Capabilities.
- Lifecycle-Side-Effects greifen sofort und stabil.

### Phase C

- `transcribe`/`assistant` funktionieren als klar getrennte Produktmodi.
- Assistant ist auch in degradierter Modul-Lage nutzbar.

## Tagesablauf / Checkliste (A -> B -> C)

1. A1: Build-/Test-Baseline herstellen.
2. A2: Gates vollständig erneut laufen lassen und dokumentieren.
3. B1: Capability-Gates vereinheitlichen.
4. B2: Disable-Lifecycle hart machen.
5. B3: Modul-UI-Konsistenz prüfen.
6. C1: Mode-Switch einführen.
7. C2: Assistant-State + Degradation integrieren.
8. C3: Finaler Regression-Durchlauf (relevante Tests + Windows dev start).

## Cleanup-Regel (verbindlich)

Diese Datei ist temporär.

Delete trigger:

1. Erster Implementierungs-PR für Block S ist gemerged, oder
2. Inhalt wurde in dauerhafte Doku (ROADMAP/STATUS/TASK_SCHEDULE + langfristige docs) überführt.

Vor Löschen prüfen:

- Keine dauerhaften Root-/Wiki-/README-Links zeigen auf diese Datei.
- Die Kernentscheidungen sind in den permanenten Dokumenten vorhanden.
