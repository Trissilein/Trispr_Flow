# OPUS Recording Pipeline Design

**Purpose**: Convert WAV/PCM audio to OPUS format for efficient storage and transmission to VibeVoice-ASR sidecar.

**Status**: Design Phase (Task C20)
**Last Updated**: 2026-02-15

---

## Why OPUS?

### Size Comparison (60-minute recording)
- **WAV (16-bit, 16kHz, mono)**: ~115 MB
- **OPUS (64 kbps)**: ~28 MB (**75% smaller**)
- **OPUS (128 kbps)**: ~56 MB (**51% smaller**)

### Quality
- Perceptually transparent at 64-128 kbps
- Optimized for speech (VibeVoice-ASR uses 16kHz anyway)
- No quality loss for ASR purposes

### Compatibility
- Native FFmpeg support (widely available)
- Supported by all modern browsers
- WebM container for web playback

---

## Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Recording Flow                              │
└─────────────────────────────────────────────────────────────────┘

[Microphone] ──> [Tauri Audio Capture] ──> [PCM Buffer]
                                                │
                                                ▼
                                        [FFmpeg OPUS Encoder]
                                                │
                                                ▼
                                        [.opus File on Disk]
                                                │
                                                ▼
                                    [HTTP Upload to Sidecar]
                                                │
                                                ▼
                                        [VibeVoice-ASR Model]
```

---

## FFmpeg Integration

### Command Template

```bash
ffmpeg -f s16le -ar 16000 -ac 1 -i pipe:0 \
       -c:a libopus -b:a 64k -vbr on -compression_level 10 \
       -application voip -frame_duration 20 \
       output.opus
```

**Parameters**:
- `-f s16le`: Input format (signed 16-bit little-endian PCM)
- `-ar 16000`: Sample rate (16kHz for ASR)
- `-ac 1`: Audio channels (mono)
- `-i pipe:0`: Read from stdin (Rust will pipe PCM data)
- `-c:a libopus`: OPUS codec
- `-b:a 64k`: Bitrate (64 kbps, balance of size/quality)
- `-vbr on`: Variable bitrate (better quality)
- `-compression_level 10`: Highest quality (0-10 scale)
- `-application voip`: Optimize for speech
- `-frame_duration 20`: 20ms frames (low latency)

### Quality Settings

| Bitrate | Use Case | File Size (60 min) | Quality |
|---------|----------|-------------------|---------|
| 32 kbps | Low bandwidth | 14 MB | Acceptable |
| 64 kbps | **Recommended** | 28 MB | High |
| 96 kbps | High quality | 42 MB | Very high |
| 128 kbps | Maximum | 56 MB | Transparent |

**Recommendation**: **64 kbps** for optimal balance.

---

## Rust Implementation Design

### Module Structure

```
src-tauri/src/
├── audio/
│   ├── mod.rs           # Audio module exports
│   ├── capture.rs       # Existing capture logic
│   └── opus_encoder.rs  # NEW: OPUS encoding
└── lib.rs               # Main app
```

### Rust Dependencies (Cargo.toml)

```toml
[dependencies]
# Existing...

# FFmpeg wrapper
ffmpeg-next = "6.0"  # Safe FFmpeg bindings
# OR alternative: subprocess approach
tokio = { version = "1", features = ["process", "io-util"] }
```

### Two Implementation Approaches

#### **Approach 1: FFmpeg Library Bindings** (Recommended)
- Use `ffmpeg-next` crate (Rust bindings for libav*)
- Direct API calls, no subprocess overhead
- More control over encoding parameters
- **Pro**: Faster, more efficient
- **Con**: Requires FFmpeg dev libraries at compile time

#### **Approach 2: FFmpeg Subprocess** (Fallback)
- Spawn `ffmpeg` process via `tokio::process::Command`
- Pipe PCM data to stdin
- Read OPUS data from stdout or file
- **Pro**: No compile-time dependencies, easier setup
- **Con**: Subprocess overhead, slightly slower

**Decision**: Use **Approach 2 (Subprocess)** for simplicity and cross-platform compatibility.

---

## API Design

### Tauri Command

```rust
#[tauri::command]
async fn encode_to_opus(
    input_wav_path: String,
    output_opus_path: String,
    bitrate_kbps: u32,  // Default: 64
) -> Result<OpusEncodeResult, String> {
    // 1. Validate input file exists
    // 2. Spawn FFmpeg process
    // 3. Pipe WAV data to FFmpeg
    // 4. Wait for completion
    // 5. Return output file path + metadata
}
```

### Return Type

```rust
#[derive(Serialize)]
struct OpusEncodeResult {
    output_path: String,
    input_size_bytes: u64,
    output_size_bytes: u64,
    compression_ratio: f32,  // e.g., 0.25 = 75% reduction
    duration_ms: u64,
}
```

---

## Recording Workflow

### Current Flow (WAV)
```
1. User presses PTT hotkey
2. Tauri starts audio capture → PCM buffer
3. On release: Save PCM to WAV file
4. WAV file stored in temp directory
```

### New Flow (WAV + OPUS)
```
1. User presses PTT hotkey
2. Tauri starts audio capture → PCM buffer
3. On release:
   a. Save PCM to WAV file (for backup/debugging)
   b. Encode WAV → OPUS (async, non-blocking)
   c. OPUS file ready for upload
4. Delete WAV after successful OPUS encoding (optional)
```

---

## Error Handling

### FFmpeg Not Found
- **Detection**: Check if `ffmpeg` exists in PATH or bundle
- **Fallback**: Show error, provide download link
- **Windows**: Bundle `ffmpeg.exe` in `resources/ffmpeg/`

### Encoding Failure
- **Detection**: FFmpeg process exits with non-zero code
- **Fallback**: Keep WAV file, show error
- **Logging**: Capture stderr for debugging

### Corrupt Audio
- **Detection**: FFmpeg reports invalid input
- **Fallback**: Discard file, notify user

---

## FFmpeg Bundling Strategy

### Windows
```
resources/
└── ffmpeg/
    └── ffmpeg.exe  (~100 MB, statically linked)
```

- Include in Tauri bundle via `tauri.conf.json`:
```json
{
  "tauri": {
    "bundle": {
      "resources": ["resources/ffmpeg/*"]
    }
  }
}
```

### macOS
- Use Homebrew FFmpeg: `brew install ffmpeg`
- OR bundle `ffmpeg` binary in app bundle
- Detect in PATH first, fallback to bundled

### Linux
- Expect system FFmpeg (`apt install ffmpeg`)
- Provide install instructions if missing

---

## Testing Strategy

### Unit Tests

1. **encode_to_opus()**: WAV → OPUS conversion
   - Input: 5-second test WAV
   - Output: Valid OPUS file, 75% smaller
   - Verify: File plays correctly, no corruption

2. **Error Handling**: FFmpeg not found
   - Mock: Remove FFmpeg from PATH
   - Expect: Graceful error, user-friendly message

3. **Large Files**: 60-minute WAV
   - Input: Long recording
   - Verify: No memory issues, completes successfully

### Integration Tests

1. **Recording → OPUS Pipeline**:
   - Record 10 seconds of mic audio
   - Encode to OPUS
   - Upload to FastAPI sidecar
   - Verify: Sidecar receives valid audio

2. **Quality Verification**:
   - Compare 64 kbps vs 128 kbps
   - Use VibeVoice transcription accuracy as metric
   - Verify: No degradation at 64 kbps

---

## Performance Benchmarks

### Expected Encoding Speed
- **Real-time Factor**: ~0.1-0.2x (10-20% of audio duration)
- **Example**: 60-second audio → 6-12 seconds to encode
- **Acceptable**: Non-blocking, user doesn't notice

### VRAM Impact
- OPUS encoding is CPU-only
- No GPU/VRAM usage
- Safe to run during AI inference

---

## Configuration (Settings)

### New Settings Fields

```rust
pub struct Settings {
    // Existing...

    // OPUS encoding
    pub opus_enabled: bool,          // Default: true
    pub opus_bitrate_kbps: u32,      // Default: 64
    pub delete_wav_after_opus: bool, // Default: true
}
```

### UI (Settings Panel)

```html
<details class="expander">
  <summary>Recording Format</summary>
  <div class="expander-body">
    <label class="field toggle">
      <input id="opus-enabled" type="checkbox" checked />
      <span>Enable OPUS compression</span>
    </label>

    <label class="field">
      <span class="field-label">OPUS bitrate (kbps)</span>
      <select id="opus-bitrate">
        <option value="32">32 kbps (smallest)</option>
        <option value="64" selected>64 kbps (recommended)</option>
        <option value="96">96 kbps (high quality)</option>
        <option value="128">128 kbps (maximum)</option>
      </select>
    </label>

    <label class="field toggle">
      <input id="delete-wav-after-opus" type="checkbox" checked />
      <span>Delete WAV after OPUS encoding</span>
    </label>

    <span class="field-hint">
      OPUS reduces file size by 75% with no quality loss for speech recognition.
    </span>
  </div>
</details>
```

---

## Open Questions

1. **Container Format**:
   - `.opus` (Ogg Opus) vs `.webm` (WebM Opus)?
   - **Decision**: Use `.opus` (simpler, widely supported)

2. **Streaming Encoding**:
   - Encode during recording (live) vs after (batch)?
   - **Decision**: Batch (simpler, less complex)

3. **Metadata**:
   - Embed speaker labels in OPUS file?
   - **Decision**: No, keep metadata separate (JSON sidecar)

---

## Next Steps

1. ✅ **Task C20 Complete**: Pipeline designed
2. 🔄 **Task C22**: Implement Rust FFmpeg wrapper
   - Create `opus_encoder.rs` module
   - Implement `encode_to_opus()` function
   - Add Tauri command
   - Test with sample audio

---

## References

- **OPUS Codec**: RFC 6716, https://opus-codec.org/
- **FFmpeg Documentation**: https://ffmpeg.org/ffmpeg.html
- **ffmpeg-next Crate**: https://crates.io/crates/ffmpeg-next
- **Tauri Resource Bundling**: https://tauri.app/v1/guides/building/resources
