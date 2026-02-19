# Voice Analysis UX - QA Matrix (Block C)

Last updated: 2026-02-19

This matrix validates Block C UX resilience: first-use setup guidance, retry/reset behavior, and stale-run protection.

| ID | Area | Preconditions | Action | Expected |
| --- | --- | --- | --- | --- |
| QC-1 | First-use preflight | No `vibevoice-venv` exists | Click `Analyse`, select audio | Preflight returns `setup_required`; confirm dialog shows size hint + detected free disk; setup can be started directly |
| QC-2 | First-use cancel | Same as QC-1 | Cancel setup in confirm dialog | Analysis stops cleanly; dialog shows actionable remediation text; no app crash/hang |
| QC-3 | In-app install progress | Runtime missing/invalid | Use `Install Voice Analysis` in error dialog | Progress status updates as setup runs; on success sidecar is reset and analysis retries |
| QC-4 | Reset after runtime failure | Force sidecar runtime error once | Run analysis twice | Second run does not replay stale previous error; sidecar reset path allows fresh attempt |
| QC-5 | Burst/cancel safety | Start analysis, then close dialog mid-run, start another | Perform two runs quickly | Old async updates are ignored (run-id gated); only current run updates UI state |
| QC-6 | Blocking preflight errors | Python missing / unsupported | Click `Analyse` and select file | Clear guided error includes Python remediation and manual setup command; no misleading auto-install loop |
| QC-7 | External worker queue | `analysis_external_worker_enabled=true` | Start analysis | Job row appears in queue (`queued` → `running` → terminal status) while UI remains responsive |
| QC-8 | External worker cancel | Same as QC-7 | Close analysis dialog while job is running | Running job transitions to `canceled`; next analysis can start immediately |
| QC-9 | External worker timeout | Force worker stall beyond timeout | Run analysis | Job transitions to `timeout` with readable error text; app does not hang |
