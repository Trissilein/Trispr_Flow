# Release QA Checklist (Block F)

Last updated: 2026-03-02

## 1. Build and Test Gate

- `npm run build`
- `npm test`
- `cargo build --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`
- `npm run audit:rust`
- `npm run benchmark:latency:strict`

Notes:
- Rust gate runs unit tests with `--no-default-features` and validates default-feature build separately.

### Automated Gate Runner

- `npm run qa:release`
- Writes summary report to `bench/results/release-qa.latest.json`

## 2. Runtime and Startup

- App starts cleanly from tray and normal launch.
- No persistent "Starting runtime..." state after startup.
- Ollama runtime state card converges to a stable status.

## 3. Capture Flows

- PTT input: start/stop/transcribe works.
- VAD input: threshold/silence behavior works.
- System audio capture: start/stop/transcribe works.
- Combined conversation view shows chronologically merged entries.

## 4. History and Storage

- Active month history loads on startup.
- Legacy migration creates monthly partitions once.
- Archive browser lists partitions and loads selected month.

## 5. Export

- Session export uses runtime session window.
- Today/Week/Month/Custom filters produce expected counts.
- TXT/MD/JSON include refined text + speaker snapshots.
- Export succeeds for both active and archived ranges.

## 6. Refinement

- AI refinement triggers and updates entries in-place.
- System-audio clustering flushes after >8s gap and on worker exit.
- Cluster merge replaces chunk entries and preserves chronology.

## 7. Regression and UX

- No console errors during normal operation.
- No major layout regressions desktop/mobile widths.
- Dialog open/close + Escape behavior remains correct.

## Sign-off

- QA date:
- Tester:
- Version/commit:
- Notes:
