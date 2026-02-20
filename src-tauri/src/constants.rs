pub const TARGET_SAMPLE_RATE: u32 = 16_000;
#[cfg(target_os = "windows")]
pub const MIN_AUDIO_MS: u64 = 250;

pub const VAD_THRESHOLD_START_DEFAULT: f32 = 0.02;
pub const VAD_THRESHOLD_SUSTAIN_DEFAULT: f32 = 0.01;
pub const VAD_SILENCE_MS_DEFAULT: u64 = 700;
pub const VAD_MIN_VOICE_MS: u64 = 120;
pub const VAD_MIN_CONSECUTIVE_CHUNKS: u64 = 3;

pub const HALLUCINATION_RMS_THRESHOLD: f32 = 0.012; // ~ -38 dB
pub const HALLUCINATION_MAX_WORDS: usize = 2;
pub const HALLUCINATION_MAX_CHARS: usize = 12;
pub const HALLUCINATION_MAX_DURATION_MS: u64 = 1200;

#[cfg(target_os = "windows")]
pub const TRANSCRIBE_IDLE_METER_MS: u64 = 500;
pub const TRANSCRIBE_BACKLOG_TARGET_MS: u64 = 10 * 60 * 1000;
pub const TRANSCRIBE_BACKLOG_MIN_CHUNKS: usize = 6;
#[cfg(any(test, target_os = "windows"))]
pub const TRANSCRIBE_BACKLOG_WARNING_PERCENT: u8 = 80;
pub const TRANSCRIBE_BACKLOG_EXPAND_NUMERATOR: usize = 3;
pub const TRANSCRIBE_BACKLOG_EXPAND_DENOMINATOR: usize = 2;
