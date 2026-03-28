# Agent Evolution Roadmap - Transkript-Tool -> vollwertiger Trispr-Agent

Last updated: 2026-03-28
Status: Accepted roadmap (execution-ready)

## Summary

Trispr entwickelt sich in klaren, messbaren Stufen:
- von Transkript + Archiv + TTS + Workflow-Agent
- hin zum produktionsnahen **GDD-Copilot**
- und danach zum vollwertigen, sprachgeführten Co-Worker.

Fixe Leitplanken:
- Aktivierung: **Hybrid** (Mode-Switch + Wakeword im Assistant-Modus)
- Autonomie: **Plan+Confirm** für alle Side-Effects
- Priorität: **GDD Copilot zuerst**
- LLM-Strategie: **Local-first**, Cloud nur optionaler Fallback

## Phasen und Exit-Kriterien

### Phase 0 - Stabiler Unterbau (jetzt, 1 Woche)

Ziel:
- `S13.5` schließen (TTS-Provider-Matrix, Device-Routing, Forced-Failure-Pfade)
- Runtime-Diagnostik vereinheitlichen (ASR/TTS/Agent)

Exit-Kriterium:
- deterministisches Verhalten bei deaktivierten/fehlenden Modulen
- klar verständliche Fehlergründe statt stiller Degradation

### Phase 1 - Assistant Pivot Foundation (2-4 Wochen, Block T)

Ziel:
- Assistant-Orchestrator-State-Machine fertigstellen
  - `Idle`, `Listening`, `Parsing`, `Planning`, `AwaitingConfirm`, `Executing`, `Recovering`
- Frontend-Modusführung `transcribe` vs `assistant` finalisieren
- Graceful-Degradation-Policy verbindlich machen

Exit-Kriterium:
- Assistant-Modus stabil nutzbar ohne Regression des Transcribe-Modus

### Phase 2 - GDD-Copilot Loop (2-3 Wochen, Block V)

Ziel:
- Workflow-Agent von Command-Trigger zu Copilot-Loop erweitern
  - Gespräch mitschreiben
  - Sessions clustern
  - Kontext aus Archiv einziehen
  - Vorschläge erzeugen
  - GDD-Draft bauen
- Vorschläge transparent darstellen (Erkannt, Annahmen, nächste Schritte)
- Plan/Execute strikt trennen (Draft ohne Side-Effect, Publish nur via Confirm)

Exit-Kriterium:
- reproduzierbares E2E: `Gespräch -> Vorschläge -> GDD-Draft -> Review`

### Phase 3 - Voice Confirmation Loop (3-4 Wochen, Block O)

Ziel:
- `awaiting_confirmation` + TTL + eindeutige Tokens
- Wakeword-basierte Confirm/Cancel-Intents (`bestätigen`/`abbrechen` + EN Synonyme)
- TTS-Rückfragen mit Timeout-Cancel

Exit-Kriterium:
- sichere sprachgeführte Freigabe ohne unbeabsichtigte Aktionen

### Phase 4 - Hands-free Aktionen (4-5 Wochen, Block P)

Ziel:
- Command-Surface für Text-Injektion/Fokuswechsel
- Agent-Step-Typ `inject_text` + aktiver Fensterkontext
- E2E: `Voice -> Plan -> Confirm -> Window Action -> TTS Feedback`

Exit-Kriterium:
- verlässlicher, tastaturfreier Workflow für definierte Zielapps

### Phase 5 - Vollwertiger Mitarbeiter-Modus (laufend)

Ziel:
- proaktive Vorschläge aus Gesprächs- und Archivkontext
- Rollen/Skill-Profile pro Aufgabenklasse
- harte Safety-Grenzen (policy-gated, confirm-pflichtig für Side-Effects)

Exit-Kriterium:
- täglicher Co-Work-Betrieb mit messbarer Zeitersparnis und niedriger Korrekturlast

## Public / Interface Changes

Settings-Erweiterungen:
- `product_mode` bleibt zentral (`transcribe` / `assistant`)
- assistant-spezifische Subsettings ergänzen:
  - Wakeword-Policy
  - Confirm-Timeout
  - Suggestion-Level

API/Event-Surface:
- konsistente Assistant-Ereignisse:
  - `assistant:state-changed`
  - `assistant:plan-ready`
  - `assistant:awaiting-confirmation`
  - `assistant:action-result`

Commands:
- bestehende Workflow-Agent-Commands bleiben stabil
- ergänzt um Confirmation- und Assistant-State-Commands

Kompatibilität:
- kein Breaking Change für Transcribe-Flow
- Assistant bleibt opt-in über Modus

## Test Plan (Roadmap-Gates)

1. Mode Safety:
- `transcribe` und `assistant` sind isoliert, kein Cross-Mode-Leak.

2. Copilot E2E:
- Gespräch -> Session-Scoring -> Plan -> Draft -> optional Publish.

3. Confirm Safety:
- False-positive/False-negative für Confirm/Cancel-Tokens + TTL.

4. Degradation:
- Vision/TTS/LLM einzeln ausfallend, Assistant bleibt bedienbar.

5. Soak:
- 8h + 24h Assistant-Betrieb ohne Restart-Zwang.

6. Quality Gates:
- Vorschlagsqualität, Latenz pro State, Erfolgsquote pro Aktion.

## Assumptions / Defaults

- Transkript + Archiv bleiben v1-Primärwissensbasis (kein schwerer Memory-Stack).
- Wakeword wird nur im Assistant-Modus aktiv ausgewertet.
- Plan+Confirm bleibt Standard für Side-Effects.
- Local-first bleibt Produktstandard; Cloud bleibt optional.
- GDD Copilot ist der erste Agent-Product-Fit-Meilenstein vor General-Assistant-Ausbau.
