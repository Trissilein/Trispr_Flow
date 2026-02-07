# Troubleshooting

## Smoke test fails in WSL
Install the Linux dependencies listed in `Getting-Started.md`, then re-run:
```bash
npm run test:smoke
```

## Model not found
Check `TRISPR_WHISPER_MODEL_DIR` and verify the model file exists there.

## System audio not transcribing
- Windows only: ensure the output device is selected.
- Toggle transcribe off and on to reinitialize monitoring.

## Overlay not visible
- Ensure overlay runtime is enabled in the tray.
- Verify overlay style and position settings in the UI.
