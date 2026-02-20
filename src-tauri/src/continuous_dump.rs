use crate::constants::TARGET_SAMPLE_RATE;
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentFlushReason {
    Silence,
    SoftInterval,
    HardCut,
    Stop,
    Backpressure,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentOutput {
    pub samples: Vec<i16>,
    pub reason: SegmentFlushReason,
    pub duration_ms: u64,
    pub rms: f32,
}

#[derive(Debug, Clone)]
pub struct AdaptiveSegmenterConfig {
    pub soft_flush_ms: u64,
    pub silence_flush_ms: u64,
    pub hard_cut_ms: u64,
    pub min_chunk_ms: u64,
    pub pre_roll_ms: u64,
    pub post_roll_ms: u64,
    pub idle_keepalive_ms: u64,
    pub threshold_start: f32,
    pub threshold_sustain: f32,
}

impl AdaptiveSegmenterConfig {
    pub fn balanced_default() -> Self {
        Self {
            soft_flush_ms: 10_000,
            silence_flush_ms: 1_200,
            hard_cut_ms: 45_000,
            min_chunk_ms: 1_000,
            pre_roll_ms: 300,
            post_roll_ms: 200,
            idle_keepalive_ms: 60_000,
            threshold_start: 0.02,
            threshold_sustain: 0.01,
        }
    }

    pub fn low_latency_default() -> Self {
        Self {
            soft_flush_ms: 8_000,
            silence_flush_ms: 900,
            hard_cut_ms: 30_000,
            min_chunk_ms: 800,
            pre_roll_ms: 200,
            post_roll_ms: 150,
            idle_keepalive_ms: 45_000,
            threshold_start: 0.02,
            threshold_sustain: 0.01,
        }
    }

    pub fn high_quality_default() -> Self {
        Self {
            soft_flush_ms: 12_000,
            silence_flush_ms: 1_600,
            hard_cut_ms: 60_000,
            min_chunk_ms: 1_500,
            pre_roll_ms: 450,
            post_roll_ms: 300,
            idle_keepalive_ms: 75_000,
            threshold_start: 0.02,
            threshold_sustain: 0.01,
        }
    }

    pub fn from_profile(profile: &str) -> Self {
        match profile {
            "low_latency" => Self::low_latency_default(),
            "high_quality" => Self::high_quality_default(),
            _ => Self::balanced_default(),
        }
    }

    pub fn clamp(&mut self) {
        self.soft_flush_ms = self.soft_flush_ms.clamp(4_000, 30_000);
        self.silence_flush_ms = self.silence_flush_ms.clamp(300, 5_000);
        self.hard_cut_ms = self.hard_cut_ms.clamp(15_000, 120_000);
        self.min_chunk_ms = self.min_chunk_ms.clamp(250, 5_000);
        self.pre_roll_ms = self.pre_roll_ms.clamp(0, 1_500);
        self.post_roll_ms = self.post_roll_ms.clamp(0, 1_500);
        self.idle_keepalive_ms = self.idle_keepalive_ms.clamp(10_000, 120_000);
        self.threshold_start = self.threshold_start.clamp(0.001, 1.0);
        self.threshold_sustain = self.threshold_sustain.clamp(0.001, self.threshold_start);
    }
}

impl Default for AdaptiveSegmenterConfig {
    fn default() -> Self {
        Self::balanced_default()
    }
}

#[derive(Debug)]
pub struct AdaptiveSegmenter {
    cfg: AdaptiveSegmenterConfig,
    active: Vec<i16>,
    pre_roll: VecDeque<i16>,
    pending_short: Vec<i16>,
    in_voice: bool,
    silence_since_voice_samples: usize,
    samples_since_flush: usize,
    backpressure_scale: f32,
}

impl AdaptiveSegmenter {
    pub fn new(mut cfg: AdaptiveSegmenterConfig) -> Self {
        cfg.clamp();
        Self {
            cfg,
            active: Vec::new(),
            pre_roll: VecDeque::new(),
            pending_short: Vec::new(),
            in_voice: false,
            silence_since_voice_samples: 0,
            samples_since_flush: 0,
            backpressure_scale: 1.0,
        }
    }

    pub fn update_config(&mut self, mut cfg: AdaptiveSegmenterConfig) {
        cfg.clamp();
        self.cfg = cfg;
    }

    #[cfg(target_os = "windows")]
    pub fn set_backpressure_percent(&mut self, percent_used: u8) {
        self.backpressure_scale = if percent_used >= 90 {
            1.35
        } else if percent_used >= 80 {
            1.2
        } else {
            1.0
        };
    }

    pub fn push_samples(&mut self, samples: &[i16], level: f32) -> Vec<SegmentOutput> {
        if samples.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();
        self.extend_pre_roll(samples);

        let threshold = if self.in_voice {
            self.cfg.threshold_sustain
        } else {
            self.cfg.threshold_start
        };
        let is_voice = level >= threshold;

        if self.active.is_empty() && is_voice {
            self.seed_active_with_pre_roll();
        }
        self.active.extend_from_slice(samples);
        self.samples_since_flush = self.samples_since_flush.saturating_add(samples.len());

        if is_voice {
            self.in_voice = true;
            self.silence_since_voice_samples = 0;
        } else {
            self.silence_since_voice_samples = self
                .silence_since_voice_samples
                .saturating_add(samples.len());
        }

        self.flush_hard_cut(&mut out);
        self.flush_silence_if_needed(&mut out, is_voice);
        self.flush_soft_if_needed(&mut out, is_voice);
        self.flush_keepalive_if_needed(&mut out);

        out
    }

    pub fn finalize(&mut self) -> Vec<SegmentOutput> {
        let mut out = Vec::new();
        if !self.active.is_empty() {
            self.flush_active(SegmentFlushReason::Stop, &mut out);
        }
        if !self.pending_short.is_empty() {
            let chunk = std::mem::take(&mut self.pending_short);
            self.emit_chunk(chunk, SegmentFlushReason::Stop, &mut out, true);
        }
        out
    }

    fn effective_soft_flush_samples(&self) -> usize {
        let scaled = (self.cfg.soft_flush_ms as f32 * self.backpressure_scale) as u64;
        ms_to_samples(scaled.max(1))
    }

    fn effective_pre_roll_samples(&self) -> usize {
        let ms = if self.backpressure_scale > 1.0 {
            ((self.cfg.pre_roll_ms as f32) * 0.75) as u64
        } else {
            self.cfg.pre_roll_ms
        };
        ms_to_samples(ms)
    }

    fn flush_hard_cut(&mut self, out: &mut Vec<SegmentOutput>) {
        let hard_cut_samples = ms_to_samples(self.cfg.hard_cut_ms.max(1));
        while self.active.len() >= hard_cut_samples && hard_cut_samples > 0 {
            let chunk: Vec<i16> = self.active.drain(..hard_cut_samples).collect();
            let reason = if self.backpressure_scale > 1.0 {
                SegmentFlushReason::Backpressure
            } else {
                SegmentFlushReason::HardCut
            };
            self.emit_chunk(chunk, reason, out, false);
            self.samples_since_flush = 0;
            self.silence_since_voice_samples = 0;
            self.in_voice = false;
        }
    }

    fn flush_silence_if_needed(&mut self, out: &mut Vec<SegmentOutput>, is_voice: bool) {
        if is_voice || self.active.is_empty() {
            return;
        }
        let silence_samples = ms_to_samples(self.cfg.silence_flush_ms.max(1));
        if self.silence_since_voice_samples >= silence_samples {
            self.flush_active(SegmentFlushReason::Silence, out);
            self.in_voice = false;
            self.silence_since_voice_samples = 0;
        }
    }

    fn flush_soft_if_needed(&mut self, out: &mut Vec<SegmentOutput>, is_voice: bool) {
        if is_voice || self.active.is_empty() {
            return;
        }
        let soft_samples = self.effective_soft_flush_samples();
        if self.samples_since_flush >= soft_samples || self.active.len() >= soft_samples {
            let reason = if self.backpressure_scale > 1.0 {
                SegmentFlushReason::Backpressure
            } else {
                SegmentFlushReason::SoftInterval
            };
            self.flush_active(reason, out);
        }
    }

    fn flush_keepalive_if_needed(&mut self, out: &mut Vec<SegmentOutput>) {
        if self.active.is_empty() {
            return;
        }
        let keepalive_samples = ms_to_samples(self.cfg.idle_keepalive_ms.max(1));
        if self.samples_since_flush >= keepalive_samples {
            self.flush_active(SegmentFlushReason::SoftInterval, out);
        }
    }

    fn flush_active(&mut self, reason: SegmentFlushReason, out: &mut Vec<SegmentOutput>) {
        if self.active.is_empty() {
            return;
        }
        let chunk = std::mem::take(&mut self.active);
        self.emit_chunk(chunk, reason, out, false);
        self.samples_since_flush = 0;
        self.silence_since_voice_samples = 0;
    }

    fn emit_chunk(
        &mut self,
        mut chunk: Vec<i16>,
        reason: SegmentFlushReason,
        out: &mut Vec<SegmentOutput>,
        force_emit: bool,
    ) {
        if chunk.is_empty() {
            return;
        }

        if !self.pending_short.is_empty() {
            let mut merged = std::mem::take(&mut self.pending_short);
            merged.append(&mut chunk);
            chunk = merged;
        }

        let min_samples = ms_to_samples(self.cfg.min_chunk_ms);
        if !force_emit && chunk.len() < min_samples {
            self.pending_short.extend_from_slice(&chunk);
            return;
        }

        let rms = rms_i16(&chunk);
        let duration_ms = (chunk.len() as u64 * 1000) / TARGET_SAMPLE_RATE as u64;
        out.push(SegmentOutput {
            samples: chunk,
            reason,
            duration_ms,
            rms,
        });
    }

    fn extend_pre_roll(&mut self, samples: &[i16]) {
        let max_pre_roll = self.effective_pre_roll_samples();
        if max_pre_roll == 0 {
            self.pre_roll.clear();
            return;
        }
        for &sample in samples {
            self.pre_roll.push_back(sample);
            if self.pre_roll.len() > max_pre_roll {
                self.pre_roll.pop_front();
            }
        }
    }

    fn seed_active_with_pre_roll(&mut self) {
        if !self.pre_roll.is_empty() {
            self.active.reserve(self.pre_roll.len());
            for &sample in &self.pre_roll {
                self.active.push(sample);
            }
        }
    }
}

fn ms_to_samples(ms: u64) -> usize {
    ((TARGET_SAMPLE_RATE as u64 * ms) / 1000) as usize
}

fn rms_i16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for &sample in samples {
        let value = sample as f32 / i16::MAX as f32;
        sum += value * value;
    }
    (sum / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{AdaptiveSegmenter, AdaptiveSegmenterConfig, SegmentFlushReason};

    fn samples_for_ms(ms: u64) -> Vec<i16> {
        vec![1000; ((16_000 * ms) / 1000) as usize]
    }

    #[test]
    fn flushes_on_silence() {
        let mut cfg = AdaptiveSegmenterConfig::balanced_default();
        cfg.silence_flush_ms = 500;
        cfg.min_chunk_ms = 250;
        let mut seg = AdaptiveSegmenter::new(cfg);

        let mut out = seg.push_samples(&samples_for_ms(700), 0.08);
        assert!(out.is_empty());
        out.extend(seg.push_samples(&samples_for_ms(600), 0.0));
        assert!(!out.is_empty());
        assert_eq!(out[0].reason, SegmentFlushReason::Silence);
    }

    #[test]
    fn hard_cut_triggers() {
        let mut cfg = AdaptiveSegmenterConfig::balanced_default();
        cfg.hard_cut_ms = 15_000;
        cfg.min_chunk_ms = 250;
        cfg.silence_flush_ms = 5_000;
        let mut seg = AdaptiveSegmenter::new(cfg);

        let out = seg.push_samples(&samples_for_ms(16_000), 0.08);
        assert!(!out.is_empty());
        assert_eq!(out[0].reason, SegmentFlushReason::HardCut);
    }

    #[test]
    fn short_chunks_merge() {
        let mut cfg = AdaptiveSegmenterConfig::balanced_default();
        cfg.min_chunk_ms = 1000;
        cfg.silence_flush_ms = 400;
        cfg.pre_roll_ms = 0;
        let mut seg = AdaptiveSegmenter::new(cfg);

        let mut out = seg.push_samples(&samples_for_ms(450), 0.08);
        out.extend(seg.push_samples(&samples_for_ms(450), 0.0));
        assert!(out.is_empty());
        out.extend(seg.push_samples(&samples_for_ms(900), 0.08));
        out.extend(seg.push_samples(&samples_for_ms(600), 0.0));

        assert!(!out.is_empty());
        assert!(out[0].duration_ms >= 1000);
    }
}
