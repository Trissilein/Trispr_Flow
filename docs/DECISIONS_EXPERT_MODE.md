# DEC-026: Expert Mode Settings Classification

**Date:** 2026-03-16
**Status:** APPROVED
**Scope:** Block K Expert Mode UX (K1-K5)

## Decision

Trispr Flow Settings will be divided into two categories for display:

- **STANDARD Mode (37 fields):** Essential controls for everyday users
- **EXPERT Mode (100 fields):** Advanced timing, tuning, and configuration controls

The mode toggle persists in localStorage (`trispr-expert-mode`) and applies CSS classes to the document root.

## Standard Settings (Essential)

**Always visible to all users:**

### Capture & Hotkeys (5)
- `mode` (ptt/vad toggle)
- `hotkey_ptt`, `hotkey_toggle`, `transcribe_hotkey`
- `ptt_use_vad`

### Audio Devices (3)
- `input_device`, `transcribe_output_device`, `mic_input_gain_db`

### Language & Model (5)
- `language_mode`, `language_pinned`, `model`, `model_source`, `model_custom_url`

### Audio Cues (2)
- `audio_cues`, `audio_cues_volume`

### Activation Words & Filtering (4)
- `activation_words_enabled`, `activation_words`, `hotkey_toggle_activation_words`
- `hallucination_filter_enabled`

### Overlay – Basic (12)
- `overlay_style` (dot/kitt)
- `overlay_color`, `overlay_opacity_inactive`, `overlay_opacity_active`
- `overlay_pos_x`, `overlay_pos_y`
- `overlay_rise_ms`, `overlay_fall_ms`
- `overlay_min_radius`, `overlay_max_radius`
- `accent_color`

### Capture & Export (5)
- `capture_enabled`, `opus_enabled`, `opus_bitrate_kbps`
- `auto_save_system_audio`, `auto_save_mic_audio`

### AI Refinement – Toggle (1)
- `ai_fallback.enabled` (turn on/off local refinement)

---

## Expert Settings (Advanced — 100 fields)

**Hidden by default, visible when Expert Mode enabled:**

### VAD Thresholds (4)
- `vad_threshold`, `vad_threshold_start`, `vad_threshold_sustain`, `vad_silence_ms`

### Transcription – VAD & Buffering (8)
- `transcribe_vad_mode`, `transcribe_vad_threshold`, `transcribe_vad_silence_ms`
- `transcribe_batch_interval_ms`, `transcribe_chunk_overlap_ms`
- `transcribe_input_gain_db`, `transcribe_backend`, `local_backend_preference`

### Overlay – KITT Mode (11)
- `overlay_kitt_*` (color, timing, opacity, position, dimensions)

### Overlay – Refining Indicator (5)
- `overlay_refining_indicator_enabled`, `overlay_refining_indicator_preset`
- `overlay_refining_indicator_color`, `overlay_refining_indicator_speed_ms`, `overlay_refining_indicator_range`

### Continuous Dump (13)
- `continuous_dump_enabled`, `continuous_dump_profile`
- Global: `continuous_soft_flush_ms`, `continuous_silence_flush_ms`, `continuous_hard_cut_ms`
- Global: `continuous_min_chunk_ms`, `continuous_pre_roll_ms`, `continuous_post_roll_ms`, `continuous_idle_keepalive_ms`
- Mic override: `continuous_mic_override_enabled`, `continuous_mic_soft_flush_ms`, `continuous_mic_silence_flush_ms`, `continuous_mic_hard_cut_ms`
- System override: `continuous_system_override_enabled`, `continuous_system_soft_flush_ms`, `continuous_system_silence_flush_ms`, `continuous_system_hard_cut_ms`

### Post-Processing (14)
- `postproc_enabled`, `postproc_language`
- `postproc_punctuation_enabled`, `postproc_capitalization_enabled`, `postproc_numbers_enabled`
- `postproc_custom_vocab_enabled`, `postproc_custom_vocab`
- `postproc_llm_enabled`, `postproc_llm_provider`, `postproc_llm_api_key`, `postproc_llm_model`, `postproc_llm_prompt`

### AI Fallback – Provider & Model Selection (12)
- `ai_fallback.provider`, `ai_fallback.fallback_provider`, `ai_fallback.execution_mode`
- `ai_fallback.model`, `ai_fallback.temperature`, `ai_fallback.max_tokens`
- `ai_fallback.prompt_profile`, `ai_fallback.custom_prompt_enabled`, `ai_fallback.custom_prompt`
- `ai_fallback.low_latency_mode`, `ai_fallback.strict_local_mode`, `ai_fallback.preserve_source_language`

### Topic Keywords & Context (1)
- `topic_keywords`

### Voice Output / TTS (11)
- `voice_output_settings.*` (provider, voice selection, rate, volume, policy, paths)

### Window Persistence (10)
- `main_window_*`, `conv_window_*` (position, size, monitor, state)

### AI Provider Authentication (11)
- `providers.*` (API keys, auth methods, model selection, Ollama configuration)

### Setup & Misc (4)
- `setup.local_ai_wizard_*`, `setup.ollama_remote_expert_opt_in`

---

## Implementation Rules

### CSS Strategy
```css
/* Hide expert-only elements in standard mode */
.standard-mode [data-expert-only] {
  display: none;
}

/* Show all in expert mode (default) */
.expert-mode [data-expert-only] {
  display: block; /* or flex/grid as needed */
}
```

### HTML Markup
```html
<!-- Always visible -->
<label for="language">Language</label>
<select id="language">...</select>

<!-- Expert-only, hidden in standard mode -->
<label for="vad_threshold" data-expert-only>VAD Threshold</label>
<input id="vad_threshold" data-expert-only>
```

### DOM Classes
- `document.documentElement.classList.add('expert-mode')` — Expert mode active
- `document.documentElement.classList.add('standard-mode')` — Standard mode active

### Persistence
```typescript
localStorage.setItem('trispr-expert-mode', 'true' | 'false')
```

---

## Visual Hierarchy

**Standard Mode:**
1. Capture device + mode (ptt/vad)
2. Hotkey setup
3. Language & model selection
4. Overlay appearance (color, position, opacity)
5. Export settings
6. AI Refinement on/off toggle

**Expert Mode:**
All standard + timing parameters, VAD thresholds, continuous dump tuning, post-processing rules, provider configuration, window persistence.

**Reordering:** Expert items will sink to the bottom of each panel, separated by a subtle divider.

---

## Rationale

1. **Reduced cognitive load:** Standard users see only essential controls.
2. **Power-user access:** Expert users can fine-tune latency, accuracy, and memory without cluttering the UI.
3. **No feature loss:** All functionality remains accessible; just hidden by default.
4. **Discoverable toggle:** Mode toggle is visible in the Settings tab header for easy switching.

---

## Affected Components

- `index.html` — Settings panels (add `data-expert-only` attributes)
- `src/settings.ts` — renderSettings() function
- `src/styles.css` or `src/styles-modern.css` — CSS hide/show rules
- `src/expert-mode.ts` — Mode toggle and localStorage management

---

## Verification

- [ ] All 37 Standard fields render in Standard mode
- [ ] All 100 Expert fields render in Expert mode
- [ ] Expert fields hidden in Standard mode
- [ ] Mode toggle persists across page reload
- [ ] CSS transitions smooth (no jarring show/hide)
- [ ] Regression tests pass (K5)
