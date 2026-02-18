# Clipboard Restore - QA Scenarios

## Goal
Validate that auto-paste temporarily uses clipboard content `B` but reliably restores prior content `A`.

## Manual Matrix
| ID | Initial Clipboard | Action | Expected |
|---|---|---|---|
| CLP-1 | Text `A` | Trigger dictation, transcript `B` is auto-pasted, then manually paste again | First paste inserts `B`, next manual paste inserts original `A` |
| CLP-2 | Image in clipboard | Trigger dictation and auto-paste text `B` | Target app receives `B`, image clipboard content remains available afterward |
| CLP-3 | Text `A` | Trigger two transcriptions quickly (`B1`, `B2`) | No crash/hang; restore follows last operation policy |
| CLP-4 | Text `A` | Trigger auto-paste while target app is under high load | Restore completes within retry window, or warning appears in logs without user-facing error dialog |
| CLP-5 | Empty clipboard | Trigger dictation and auto-paste text `B` | Auto-paste works; no crash/hang during restore phase |

## Notes
- Restore mode is `robust still`: retries + verification for text restores, no UI error popup.
- Burst policy is `last-write-wins`.
