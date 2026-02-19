# Trispr Analysis Tool

Standalone Voice Analysis app for Trispr Flow.

## Features
- CLI entrypoint (`open`, `status`, `doctor`)
- Local HTML UI (demo-style layout inspired by VibeVoice ASR Demo)
- Own native window via WebView
- File handoff from Trispr (`--audio`) with startup choice:
  - Analyze now
  - Load only

## Quick Start

```powershell
cd analysis-tool
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
.\.venv\Scripts\python.exe main.py open --audio "C:\path\to\audio.opus" --source trispr-flow
```

## CLI

```powershell
python main.py open --audio "C:\path\to\audio.opus" --source trispr-flow
python main.py status --json
python main.py doctor --json
```

## Runtime Note
The UI server delegates transcription to `sidecar/vibevoice-asr/worker_once.py` when available.
For UI-only testing you can enable mock mode:

```powershell
$env:TRISPR_ANALYSIS_USE_MOCK = \"1\"
python main.py open --audio \"C:\\path\\to\\audio.opus\" --source trispr-flow
```
