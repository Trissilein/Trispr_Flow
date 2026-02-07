# Getting Started

## Prerequisites
- Node.js (npm)
- Rust toolchain (stable)
- Tauri v2 prerequisites for your OS

## Run the app
```bash
npm install
npm run tauri dev
```

## Test workflow
### Unit tests
```bash
npm run test
```

### Smoke test (frontend + Rust)
```bash
npm run test:smoke
```

`test:smoke` runs:
1. `npm run build`
2. `cargo test --manifest-path src-tauri/Cargo.toml`

## Local whisper.cpp (GPU)
1. Build whisper.cpp with CUDA enabled.
2. Place models in the whisper.cpp `models/` directory.
3. Set environment variables (see `.env.example`).

### One-shot setup (Windows)
```bash
.\scripts\setup-whisper.ps1
```

For CPU fallback:
```bash
.\scripts\setup-whisper.ps1 -CpuFallback
```

## WSL/Linux dependencies
Tauri on Linux requires GTK/WebKit and linker dependencies:
```bash
sudo apt install -y \
  pkg-config \
  libgtk-3-dev \
  libglib2.0-dev \
  libcairo2-dev \
  libpango1.0-dev \
  libatk1.0-dev \
  libgdk-pixbuf-2.0-dev \
  libwebkit2gtk-4.1-dev \
  libasound2-dev \
  libxdo-dev
```
