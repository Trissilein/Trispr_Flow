# trispr-opus — opus export sidecar

The "code-out" half of Trispr Flow's opus export capability. All FFmpeg/libopus
invocation lives here, in a tiny standalone process (~250 KB exe), instead of in
the core binary. This is what lets the core stay lean: FFmpeg (~84 MB) ships
inside this module package, downloaded on demand, not in the base installer.

This crate is **not** part of the `trispr-flow` core build (no workspace
membership). It is built separately and published as a module package asset:

```
modules/opus/
  module.json            # package manifest (kind = "sidecar", entrypoint below)
  bin/trispr-opus.exe     # this crate
  bin/ffmpeg/ffmpeg.exe   # bundled FFmpeg with libopus
```

`entrypoint = "bin/trispr-opus.exe"`. The sidecar finds FFmpeg next to its own
exe (`./ffmpeg.exe` or `./ffmpeg/ffmpeg.exe`), falling back to `PATH` for dev.

## Build

```powershell
cargo build --release --manifest-path module-sidecars/opus/Cargo.toml
```

## CLI

```
trispr-opus encode --input X.wav --output Y.opus [--bitrate 64] [--vbr on]
                   [--compression 10] [--sample-rate 16000] [--channels 1]
                   [--application voip]
trispr-opus concat --list concat.txt --output session.opus [--cwd DIR]
trispr-opus probe
```

On success: exit 0 and a single JSON line on stdout. On failure: non-zero exit
and a message on stderr. The FFmpeg argument set mirrors the core's previous
inline invocation exactly, so output is byte-comparable.

- `encode` → `{"output_path","input_size_bytes","output_size_bytes","compression_ratio","duration_ms"}`
- `concat` → `{"output_path"}`
- `probe`  → `{"available":bool,"version":string}`

## Why a separate process

Trispr already runs whisper-server, FFmpeg, Piper and Ollama out of process.
Opus export joins that pattern: crash isolation is free, there is no Rust ABI or
plugin-signing contract, and the download is SHA256-verified by the core's module
delivery layer. Core writes the WAV (it keeps `hound` for audio playback anyway)
and hands us a path — no large in-memory IPC payload.
