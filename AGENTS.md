<!-- AIHUB AGENTS SYNC: MANAGED -->
<!-- Source: C:\Users\trist\OneDrive\AIHub\AGENTS.md -->
<!-- Re-run setup\sync-project-agents.ps1 to refresh the global block. -->

<!-- BEGIN AIHUB GLOBAL BLOCK -->
# AGENTS.md — AIHub

Hinweise für AI-Agents (Codex, Cursor, Antigravity, Windsurf, etc.), die in diesem Cloud-Hub oder in Projekten arbeiten, die den Hub nutzen.

## Die wichtigste Information

Alle Pfade sind transparent. Du musst nichts umlernen.

- `D:\GIT\mempalace` (Windows) bzw. `~/mempalace` (macOS) existieren weiterhin und zeigen via Junction/Symlink auf den Hub-Ordner `AIHub/mempalace/`.
- Der Mem Palace ist **geräteübergreifend geteilt** — wenn du etwas schreibst, siehst du es auch auf anderen Geräten (nach OneDrive-Sync).

## Multi-Agent-Kollaboration

Wenn mehrere AI-Agents (Claude, Codex, Cursor, Antigravity, ...) parallel an demselben Problem arbeiten sollen, nutzen sie das **Orchestration-Protokoll** — ein gemeinsames Schema im Mem Palace für Plans, Tasks und Agent-States.

**Quelle der Wahrheit: [`docs/ORCHESTRATION.md`](docs/ORCHESTRATION.md) im Hub-Root.** Lies die Datei vollständig, wenn der User dich bittet, an einem laufenden Plan mitzuarbeiten oder einen neuen strukturierten Plan anzulegen.

Kurzfassung:
- **Plans** sind KG-Entities `plan_<slug>` mit `status=active`.
- **Tasks** sind `task_<slug>_<nn>` mit Lifecycle `pending → claimed → in_progress → done/blocked/failed`.
- **Agent-States** sind `agent_<name>` mit `current_focus`, `current_task`, `last_heartbeat`.

**Einstieg in laufenden Plan:**
1. `mempalace_kg_query entity=<plan-name>` — Plan-Meta holen
2. `mempalace_kg_query entity=agent_claude` und `agent_codex` — schauen was andere gerade machen
3. Offene Tasks identifizieren (status=pending, assigned_to=null), Dependencies prüfen
4. Task claimen: `task_X assigned_to <self>`, `task_X status claimed`, dann `in_progress`
5. `agent_<self>` State schreiben (`current_focus`, `last_heartbeat`)
6. Arbeiten, alle ~5-10 Min Heartbeat aktualisieren
7. Bei done: `status=done`, `result=<was-wurde-gemacht>`

**Plan neu anlegen:** siehe ORCHESTRATION.md "Workflow A".

**Agent-Name-Konvention:** Codex = `codex`, Cursor = `cursor`, Antigravity = `antigravity`, Claude = `claude`.

## Setup-Status prüfen

Falls du unsicher bist, was zu welchem Pfad gehört, frag den Mem Palace:

```
mempalace_kg_query entity="AIHub"
mempalace_kg_query entity="D:\GIT\mempalace"
mempalace_kg_query entity="mempalace_sqlite"
```

Gibt dir die aktuellen Facts zur Cloud-Struktur.

Oder lies:
- `C:\Users\trist\OneDrive\AIHub\SESSION-BRIEF.md` — aktueller Kontext
- `C:\Users\trist\OneDrive\AIHub\docs\ARCHITECTURE.md` — Gesamt-Layout

## Kritische Regel

**Nie parallele Schreibzugriffe auf `chroma.sqlite3` aus mehreren Geräten.** Die DB liegt in OneDrive-Sync und hat keinen verteilten Lock. Wenn Claude auf Windows und du (z.B. Codex auf Mac) gleichzeitig schreiben → WAL-Korruption möglich.

Reads sind unkritisch (SQLite teilt Read-Locks).

Praktisch: Wenn du siehst, dass der User gerade mit einem anderen Agent aktiv am Schreiben ist, frag kurz nach, bevor du selbst `mempalace_kg_add` oder `mempalace_diary_write` aufrufst.

## Skills-Ordner

Wenn du den Skills-Ordner liest (`~/.claude/skills/`), bekommst du automatisch die Hub-Version. Wenn du dort schreibst, landen Änderungen geräteübergreifend — überlege, ob du das wirklich willst, oder ob's eine Project-Local-Copy sein sollte.

## Secrets

- Bitwarden-Vault `AIHub/env` (EU-Server: `https://vault.bitwarden.eu`)
- Lokaler Fallback: `~/.claude/.env.local`

Secrets NIE in den Hub schreiben — `.env.local` ist explizit nicht gesynct.

## Bei Verwirrung

1. `mempalace_status` — zeigt aktuellen Palace-Status
2. `mempalace_kg_query entity="AIHub"` — Migration-Facts
3. `cat $HUB/docs/TROUBLESHOOTING.md` — häufige Probleme

Bei unerwartetem Verhalten (Datei fehlt, DB-Sperre, falsche Settings) lieber einmal nachfragen als raten.
<!-- END AIHUB GLOBAL BLOCK -->

<!-- BEGIN PROJECT LOCAL BLOCK -->
# Trispr_Flow Project Rules

## Global Memory

If the `mempalace` MCP server is available, treat it as the first source of truth for prior-session context.

At the start of a new session:
- Call `mempalace_status` once.
- Call `mempalace_diary_read` with `agent_name="codex"` and review recent entries.
- If the repo, issue, person, or topic may have prior context, call `mempalace_search` before making claims from memory.

During work:
- Use `mempalace_search` or `mempalace_kg_query` before stating facts about prior decisions, people, timelines, or past sessions.
- Never invent prior-session context when `mempalace` is unavailable or returns nothing.

At the end of a substantial session:
- Call `mempalace_diary_write` with `agent_name="codex"` and record a compact AAAK-style summary of what changed, what mattered, and what should be resumed next.
<!-- END PROJECT LOCAL BLOCK -->
