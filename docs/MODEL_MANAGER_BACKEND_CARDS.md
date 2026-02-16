# Model Manager: Backend-Aware Model Cards

**Purpose**: Enhanced model selection UI with backend information, hardware requirements, and personalized recommendations

**Status**: Design Phase
**Last Updated**: 2026-02-15

---

## Problem Statement

As Trispr Flow adds multiple ASR backends (Whisper, VibeVoice-ASR, Parakeet), users need clear guidance on:

1. **Which backend powers each model**
2. **Hardware requirements** (VRAM, GPU type, CPU fallback)
3. **Trade-offs** (speed vs quality vs features)
4. **Personalized recommendation** based on detected hardware

Current Model Manager shows generic model cards with no backend context.

---

## Solution: Backend-Aware Model Cards

### Enhanced Model Card Structure

Each model card displays:

```
┌─────────────────────────────────────────────────────────┐
│ [Icon] Whisper Large v3 Turbo               [⭐ RECOMMENDED] │
│                                                         │
│ Backend: Whisper (whisper.cpp)                         │
│ VRAM: 2-4 GB  │  Speed: Fast  │  Quality: Excellent    │
│                                                         │
│ ✅ Real-time transcription                              │
│ ✅ 99 languages supported                               │
│ ✅ Works on NVIDIA + AMD + Intel GPUs                   │
│ ❌ No speaker diarization                               │
│                                                         │
│ Why recommended for your hardware:                     │
│ Your RTX 5070 Ti (16GB VRAM) is perfect for this      │
│ model. Fast inference with low memory footprint.       │
│                                                         │
│ [Install] [Documentation]                              │
└─────────────────────────────────────────────────────────┘
```

---

## Backend Comparison Matrix

| Backend | Use Case | VRAM | Speed | Quality | Features | Hardware |
|---------|----------|------|-------|---------|----------|----------|
| **Whisper** | Real-time dictation | 2-4 GB | Fast | Excellent | 99 languages | NVIDIA, AMD, Intel |
| **VibeVoice-ASR** | Meeting transcription | 14-16 GB (FP16)<br>7-8 GB (INT8) | Medium | Excellent | Speaker diarization, 50+ languages | NVIDIA only |
| **Parakeet** | Ultra-fast inference | 4-6 GB | Very Fast | Good | Real-time, low latency | NVIDIA only |

---

## Model Card Data Schema

### Backend Enum

```typescript
type ASRBackend = "whisper" | "vibevoice" | "parakeet";

interface BackendInfo {
  id: ASRBackend;
  name: string;
  description: string;
  vendor: string; // "OpenAI", "Microsoft", "NVIDIA"
  requirements: HardwareRequirements;
  capabilities: Capability[];
  limitations: string[];
}
```

### Hardware Requirements

```typescript
interface HardwareRequirements {
  vram_min_gb: number;
  vram_recommended_gb: number;
  gpu_types: ("nvidia" | "amd" | "intel" | "cpu")[];
  precision_modes: ("fp16" | "int8" | "fp32")[];
  cuda_compute_min?: number; // e.g., 7.5 for RTX 20xx+
}
```

### Capability Flags

```typescript
interface Capability {
  id: string;
  label: string;
  description: string;
}

const CAPABILITIES = {
  speaker_diarization: {
    id: "speaker_diarization",
    label: "Speaker Diarization",
    description: "Identifies who spoke when in multi-speaker recordings"
  },
  real_time: {
    id: "real_time",
    label: "Real-time Transcription",
    description: "Low-latency streaming transcription"
  },
  multilingual: {
    id: "multilingual",
    label: "99+ Languages",
    description: "Supports a wide range of languages"
  },
  hotwords: {
    id: "hotwords",
    label: "Custom Hotwords",
    description: "Domain-specific vocabulary customization"
  },
  timestamps: {
    id: "timestamps",
    label: "Word-level Timestamps",
    description: "Precise timing for each word"
  },
  cross_platform: {
    id: "cross_platform",
    label: "Cross-platform GPU",
    description: "Works on NVIDIA, AMD, and Intel GPUs"
  }
};
```

---

## Enhanced Model Info Structure

```typescript
interface ModelInfo {
  id: string;
  label: string;
  file_name: string;
  size_mb: number;
  installed: boolean;
  downloading: boolean;
  path?: string;
  source: string;
  available: boolean;
  download_url?: string;
  removable: boolean;

  // NEW: Backend information
  backend: ASRBackend;
  backend_info: BackendInfo;

  // NEW: Hardware suitability
  hardware_score?: number; // 0-100, calculated based on detected GPU
  hardware_recommendation?: string; // "Perfect fit", "Good", "Marginal", "Incompatible"
  hardware_reason?: string; // Explanation for the recommendation

  // NEW: Performance metrics
  estimated_speed?: string; // "Very Fast", "Fast", "Medium", "Slow"
  quality_tier?: "excellent" | "good" | "fair";

  // NEW: Feature flags
  capabilities: string[]; // IDs from CAPABILITIES
  limitations: string[];
}
```

---

## Hardware Detection

### GPU Detection API

```rust
// src-tauri/src/gpu.rs

#[derive(Debug, Clone, Serialize)]
pub struct GPUInfo {
  pub vendor: String,      // "NVIDIA", "AMD", "Intel"
  pub model: String,       // "GeForce RTX 5070 Ti"
  pub vram_total_mb: u64,  // 16384
  pub vram_free_mb: u64,   // 14200
  pub cuda_compute: Option<f32>, // 8.9 for RTX 50xx
  pub driver_version: String,
}

#[tauri::command]
pub fn detect_gpu() -> Result<GPUInfo, String> {
  // Use nvidia-smi for NVIDIA
  // Use vulkaninfo for AMD/Intel
  // Parse output and return structured data
}
```

### Suitability Scoring Algorithm

```rust
fn calculate_hardware_score(model: &ModelInfo, gpu: &GPUInfo) -> u8 {
  let mut score = 100u8;

  // VRAM check
  if gpu.vram_total_mb < model.backend_info.requirements.vram_min_gb * 1024 {
    return 0; // Incompatible
  }

  if gpu.vram_total_mb < model.backend_info.requirements.vram_recommended_gb * 1024 {
    score -= 30; // Marginal
  }

  // GPU vendor check
  let gpu_vendor = gpu.vendor.to_lowercase();
  if !model.backend_info.requirements.gpu_types.contains(&gpu_vendor.as_str()) {
    return 0; // Incompatible
  }

  // CUDA compute capability check (NVIDIA only)
  if gpu_vendor == "nvidia" {
    if let (Some(min_compute), Some(gpu_compute)) =
      (model.backend_info.requirements.cuda_compute_min, gpu.cuda_compute) {
      if gpu_compute < min_compute {
        return 0; // Incompatible
      }
    }
  }

  // Bonus for perfect fit
  if gpu.vram_total_mb >= model.backend_info.requirements.vram_recommended_gb * 1024 * 2 {
    score = 100; // Perfect fit, plenty of headroom
  }

  score
}

fn get_recommendation_label(score: u8) -> &'static str {
  match score {
    90..=100 => "Perfect fit",
    70..=89  => "Good",
    50..=69  => "Marginal",
    1..=49   => "Not recommended",
    0        => "Incompatible",
  }
}
```

---

## UI Design

### Model Card Layout (HTML)

```html
<div class="model-card" data-backend="whisper" data-score="95">
  <!-- Header -->
  <div class="model-header">
    <div class="model-icon">
      <img src="icons/whisper.svg" alt="Whisper" />
    </div>
    <div class="model-title">
      <h3>Whisper Large v3 Turbo</h3>
      <span class="model-backend">Backend: Whisper (whisper.cpp)</span>
    </div>
    <div class="model-recommendation-badge recommended">
      ⭐ RECOMMENDED
    </div>
  </div>

  <!-- Specs Row -->
  <div class="model-specs">
    <span class="spec">
      <span class="spec-icon">💾</span>
      VRAM: 2-4 GB
    </span>
    <span class="spec">
      <span class="spec-icon">⚡</span>
      Speed: Fast
    </span>
    <span class="spec">
      <span class="spec-icon">✨</span>
      Quality: Excellent
    </span>
  </div>

  <!-- Capabilities -->
  <div class="model-capabilities">
    <span class="capability yes">✅ Real-time transcription</span>
    <span class="capability yes">✅ 99 languages supported</span>
    <span class="capability yes">✅ Works on NVIDIA + AMD + Intel GPUs</span>
    <span class="capability no">❌ No speaker diarization</span>
  </div>

  <!-- Hardware Recommendation -->
  <div class="hardware-recommendation perfect">
    <strong>Why recommended for your hardware:</strong>
    <p>Your RTX 5070 Ti (16GB VRAM) is perfect for this model. Fast inference with low memory footprint.</p>
  </div>

  <!-- Actions -->
  <div class="model-actions">
    <button class="btn-primary">Install</button>
    <button class="btn-secondary">Documentation</button>
  </div>
</div>
```

### CSS Styling

```css
.model-card {
  border: 2px solid var(--border-color);
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 16px;
  transition: all 0.2s;
}

.model-card[data-score="0"] {
  opacity: 0.5;
  border-color: var(--red);
}

.model-card[data-score="90"],
.model-card[data-score="95"],
.model-card[data-score="100"] {
  border-color: var(--green);
  background: rgba(0, 255, 0, 0.05);
}

.model-recommendation-badge {
  display: inline-block;
  padding: 4px 12px;
  border-radius: 16px;
  font-size: 12px;
  font-weight: bold;
  text-transform: uppercase;
}

.model-recommendation-badge.recommended {
  background: var(--green);
  color: white;
}

.model-specs {
  display: flex;
  gap: 16px;
  margin: 12px 0;
  font-size: 14px;
}

.spec {
  display: flex;
  align-items: center;
  gap: 4px;
}

.model-capabilities {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 12px 0;
  font-size: 14px;
}

.capability.yes { color: var(--green); }
.capability.no { color: var(--gray); }

.hardware-recommendation {
  margin: 12px 0;
  padding: 12px;
  border-radius: 6px;
  font-size: 13px;
}

.hardware-recommendation.perfect {
  background: rgba(0, 255, 0, 0.1);
  border-left: 3px solid var(--green);
}

.hardware-recommendation.good {
  background: rgba(255, 200, 0, 0.1);
  border-left: 3px solid var(--yellow);
}

.hardware-recommendation.marginal {
  background: rgba(255, 100, 0, 0.1);
  border-left: 3px solid var(--orange);
}

.hardware-recommendation.incompatible {
  background: rgba(255, 0, 0, 0.1);
  border-left: 3px solid var(--red);
}
```

---

## Backend Definitions

### Whisper Backend

```typescript
const WHISPER_BACKEND: BackendInfo = {
  id: "whisper",
  name: "Whisper (whisper.cpp)",
  description: "OpenAI's multilingual speech recognition model, optimized via whisper.cpp",
  vendor: "OpenAI",
  requirements: {
    vram_min_gb: 2,
    vram_recommended_gb: 4,
    gpu_types: ["nvidia", "amd", "intel", "cpu"],
    precision_modes: ["fp16", "fp32"],
  },
  capabilities: [
    "real_time",
    "multilingual",
    "timestamps",
    "cross_platform"
  ],
  limitations: [
    "No speaker diarization",
    "No hotword customization"
  ]
};
```

### VibeVoice-ASR Backend

```typescript
const VIBEVOICE_BACKEND: BackendInfo = {
  id: "vibevoice",
  name: "VibeVoice-ASR 7B",
  description: "Microsoft's speaker-diarized ASR model for meeting transcription",
  vendor: "Microsoft",
  requirements: {
    vram_min_gb: 7,
    vram_recommended_gb: 16,
    gpu_types: ["nvidia"],
    precision_modes: ["fp16", "int8"],
    cuda_compute_min: 7.0,
  },
  capabilities: [
    "speaker_diarization",
    "multilingual",
    "hotwords",
    "timestamps"
  ],
  limitations: [
    "NVIDIA GPUs only",
    "Higher VRAM requirements",
    "Not real-time (post-processing)"
  ]
};
```

### Parakeet Backend

```typescript
const PARAKEET_BACKEND: BackendInfo = {
  id: "parakeet",
  name: "Parakeet RNNT",
  description: "NVIDIA's ultra-fast ASR model optimized for NVIDIA hardware",
  vendor: "NVIDIA",
  requirements: {
    vram_min_gb: 4,
    vram_recommended_gb: 6,
    gpu_types: ["nvidia"],
    precision_modes: ["fp16", "int8"],
    cuda_compute_min: 7.5,
  },
  capabilities: [
    "real_time",
    "multilingual",
    "timestamps"
  ],
  limitations: [
    "NVIDIA GPUs only",
    "No speaker diarization",
    "Fewer languages than Whisper (50 vs 99)"
  ]
};
```

---

## Implementation Tasks

### Phase 1: Backend Infrastructure (v0.6.0 Block E)

- [ ] **E35**: Define backend data schema (BackendInfo, HardwareRequirements, Capability)
- [ ] **E36**: Implement GPU detection API (NVIDIA-smi, vulkaninfo)
- [ ] **E37**: Implement hardware scoring algorithm
- [ ] **E38**: Add backend field to ModelInfo struct (Rust + TypeScript)

### Phase 2: UI Implementation (v0.6.0 Block E)

- [ ] **E39**: Design enhanced model card layout (HTML + CSS)
- [ ] **E40**: Implement capability badges rendering
- [ ] **E41**: Implement hardware recommendation display
- [ ] **E42**: Add backend filter/sort options

### Phase 3: Backend Definitions (v0.6.0 Block E)

- [ ] **E43**: Define Whisper backend metadata
- [ ] **E44**: Define VibeVoice backend metadata
- [ ] **E45**: Define Parakeet backend metadata (when integrated)

### Phase 4: Integration & Testing (v0.6.0 Block E)

- [ ] **E46**: Wire GPU detection to model list rendering
- [ ] **E47**: Test on NVIDIA, AMD, Intel GPUs
- [ ] **E48**: Add fallback for GPU detection failure
- [ ] **E49**: Documentation update

---

## User Flow Examples

### Example 1: RTX 5070 Ti (16GB VRAM) User

**Detected Hardware:**
- GPU: NVIDIA GeForce RTX 5070 Ti
- VRAM: 16384 MB
- CUDA Compute: 8.9
- Driver: 570.47

**Model Card Recommendations:**

1. **Whisper Large v3 Turbo** — ⭐ RECOMMENDED (Score: 95)
   - "Perfect fit: Your RTX 5070 Ti has plenty of VRAM (16GB) for this 4GB model. Fast inference with low memory footprint."

2. **VibeVoice-ASR 7B (FP16)** — ⭐ RECOMMENDED (Score: 90)
   - "Perfect fit: Your RTX 5070 Ti (16GB VRAM) is ideal for this model. Enables speaker diarization for meetings."

3. **Parakeet RNNT** — Good (Score: 85)
   - "Good fit: Ultra-fast inference on your NVIDIA GPU. Consider this for real-time low-latency use cases."

### Example 2: AMD Radeon RX 7600 (8GB VRAM) User

**Detected Hardware:**
- GPU: AMD Radeon RX 7600
- VRAM: 8192 MB
- Vulkan: 1.3

**Model Card Recommendations:**

1. **Whisper Large v3 Turbo** — ⭐ RECOMMENDED (Score: 90)
   - "Good fit: Your AMD Radeon RX 7600 (8GB VRAM) works well with this model via Vulkan backend."

2. **VibeVoice-ASR 7B (FP16)** — Incompatible (Score: 0)
   - "Incompatible: This model requires NVIDIA GPU. Your AMD Radeon RX 7600 is not supported."

3. **Parakeet RNNT** — Incompatible (Score: 0)
   - "Incompatible: This model requires NVIDIA GPU with CUDA support."

### Example 3: Intel Arc A770 (16GB VRAM) User

**Detected Hardware:**
- GPU: Intel Arc A770
- VRAM: 16384 MB
- Vulkan: 1.3

**Model Card Recommendations:**

1. **Whisper Large v3 Turbo** — ⭐ RECOMMENDED (Score: 95)
   - "Perfect fit: Your Intel Arc A770 (16GB VRAM) is excellent for this model. Cross-platform GPU support via Vulkan."

2. **VibeVoice-ASR 7B (FP16)** — Incompatible (Score: 0)
   - "Incompatible: This model requires NVIDIA GPU. Your Intel Arc A770 is not supported."

3. **Parakeet RNNT** — Incompatible (Score: 0)
   - "Incompatible: This model requires NVIDIA GPU with CUDA support."

---

## Open Questions

1. **GPU Detection Fallback**: What to show if GPU detection fails?
   - Default to showing all models with "Unable to detect GPU" message
   - Allow manual hardware specification in settings

2. **Multi-GPU Systems**: How to handle systems with multiple GPUs?
   - Detect all GPUs, recommend best fit
   - Allow user to select preferred GPU in settings

3. **CPU-Only Fallback**: How to represent CPU-only mode?
   - Show Whisper models with "CPU mode (slower)" badge
   - Disable NVIDIA-only models

4. **Backend Version Updates**: How to handle backend version changes?
   - Include backend version in BackendInfo
   - Check for backend updates separately from model updates

---

## Success Metrics

- **User Clarity**: 90%+ of users understand which model to choose based on card information
- **Correct Choices**: 80%+ of users select recommended models
- **Reduced Support Queries**: 50% reduction in "which model should I use?" questions
- **Hardware Compatibility**: 95%+ accurate GPU detection across NVIDIA, AMD, Intel

---

## References

- [COMPETITOR_ANALYSIS_HANDY.md](COMPETITOR_ANALYSIS_HANDY.md)
- [VIBEVOICE_RESEARCH.md](VIBEVOICE_RESEARCH.md)
- [OPUS_PIPELINE_DESIGN.md](OPUS_PIPELINE_DESIGN.md)
