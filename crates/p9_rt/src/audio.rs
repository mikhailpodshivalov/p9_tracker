use crate::dsp::DspPipeline;
use crate::voice::VoiceAllocator;
use p9_core::events::{RenderEvent, RenderMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioMetrics {
    pub sample_rate_hz: u32,
    pub buffer_size_frames: u32,
    pub callbacks_total: u64,
    pub xruns_total: u64,
    pub last_callback_us: u32,
    pub avg_callback_us: u32,
    pub active_voices: u32,
    pub max_voices: u32,
    pub voices_stolen_total: u64,
    pub voice_note_on_total: u64,
    pub voice_note_off_total: u64,
    pub voice_note_off_miss_total: u64,
    pub voice_retrigger_total: u64,
    pub voice_zero_attack_total: u64,
    pub voice_short_release_total: u64,
    pub click_risk_total: u64,
    pub voice_release_deferred_total: u64,
    pub voice_release_completed_total: u64,
    pub voice_release_pending_voices: u32,
    pub voice_steal_releasing_total: u64,
    pub voice_steal_active_total: u64,
    pub voice_polyphony_pressure_total: u64,
    pub voice_sampler_mode_note_on_total: u64,
    pub voice_silent_note_on_total: u64,
}

impl Default for AudioMetrics {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            buffer_size_frames: 256,
            callbacks_total: 0,
            xruns_total: 0,
            last_callback_us: 0,
            avg_callback_us: 0,
            active_voices: 0,
            max_voices: 0,
            voices_stolen_total: 0,
            voice_note_on_total: 0,
            voice_note_off_total: 0,
            voice_note_off_miss_total: 0,
            voice_retrigger_total: 0,
            voice_zero_attack_total: 0,
            voice_short_release_total: 0,
            click_risk_total: 0,
            voice_release_deferred_total: 0,
            voice_release_completed_total: 0,
            voice_release_pending_voices: 0,
            voice_steal_releasing_total: 0,
            voice_steal_active_total: 0,
            voice_polyphony_pressure_total: 0,
            voice_sampler_mode_note_on_total: 0,
            voice_silent_note_on_total: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioBackendConfig {
    pub sample_rate_hz: u32,
    pub buffer_size_frames: u32,
    pub base_callback_us: u32,
    pub per_event_us: u32,
    pub max_callback_us: u32,
    pub max_voices: usize,
    pub fail_on_start: bool,
}

impl Default for AudioBackendConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            buffer_size_frames: 256,
            base_callback_us: 220,
            per_event_us: 35,
            max_callback_us: 1_200,
            max_voices: 16,
            fail_on_start: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioBackendError {
    StartFailed(&'static str),
}

pub trait AudioBackend {
    fn start(&mut self);
    fn start_checked(&mut self) -> Result<(), AudioBackendError> {
        self.start();
        Ok(())
    }
    fn stop(&mut self);
    fn push_events(&mut self, events: &[RenderEvent]);
    fn events_consumed(&self) -> usize;
    fn metrics(&self) -> AudioMetrics;
    fn backend_name(&self) -> &'static str;
}

#[derive(Default)]
pub struct NoopAudioBackend {
    running: bool,
    events_total: usize,
    metrics: AudioMetrics,
}

impl AudioBackend for NoopAudioBackend {
    fn start(&mut self) {
        self.running = true;
    }

    fn stop(&mut self) {
        self.running = false;
    }

    fn push_events(&mut self, events: &[RenderEvent]) {
        if self.running {
            self.events_total = self.events_total.saturating_add(events.len());
            self.metrics.callbacks_total = self.metrics.callbacks_total.saturating_add(1);
        }
    }

    fn events_consumed(&self) -> usize {
        self.events_total
    }

    fn metrics(&self) -> AudioMetrics {
        self.metrics
    }

    fn backend_name(&self) -> &'static str {
        "noop"
    }
}

pub struct NativeAudioBackend {
    config: AudioBackendConfig,
    running: bool,
    events_total: usize,
    metrics: AudioMetrics,
    callback_us_total: u64,
    dsp: DspPipeline,
    voices: VoiceAllocator,
    sampler_mode_note_on_total: u64,
    silent_note_on_total: u64,
}

impl NativeAudioBackend {
    pub fn new(config: AudioBackendConfig) -> Self {
        Self {
            running: false,
            events_total: 0,
            metrics: AudioMetrics {
                sample_rate_hz: config.sample_rate_hz,
                buffer_size_frames: config.buffer_size_frames,
                max_voices: config.max_voices as u32,
                ..AudioMetrics::default()
            },
            callback_us_total: 0,
            dsp: DspPipeline::new(config.max_callback_us),
            voices: VoiceAllocator::new(config.max_voices),
            sampler_mode_note_on_total: 0,
            silent_note_on_total: 0,
            config,
        }
    }
}

impl Default for NativeAudioBackend {
    fn default() -> Self {
        Self::new(AudioBackendConfig::default())
    }
}

impl AudioBackend for NativeAudioBackend {
    fn start(&mut self) {
        self.running = true;
    }

    fn start_checked(&mut self) -> Result<(), AudioBackendError> {
        if self.config.fail_on_start {
            return Err(AudioBackendError::StartFailed(
                "native audio backend start failed",
            ));
        }
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) {
        self.running = false;
    }

    fn push_events(&mut self, events: &[RenderEvent]) {
        if !self.running {
            return;
        }

        self.voices.advance_release_envelopes();

        for event in events {
            match event {
                RenderEvent::NoteOn {
                    track_id,
                    note,
                    velocity,
                    render_mode,
                    instrument_id,
                    waveform,
                    attack_ms,
                    release_ms,
                    gain,
                    ..
                } => {
                    if *gain == 0 || matches!(render_mode, RenderMode::ExternalMuted) {
                        self.silent_note_on_total = self.silent_note_on_total.saturating_add(1);
                        continue;
                    }
                    if matches!(render_mode, RenderMode::SamplerV1) {
                        self.sampler_mode_note_on_total =
                            self.sampler_mode_note_on_total.saturating_add(1);
                    }
                    self.voices.note_on(
                        *track_id,
                        *note,
                        *velocity,
                        *instrument_id,
                        *waveform,
                        *attack_ms,
                        *release_ms,
                        *gain,
                    );
                }
                RenderEvent::NoteOff { track_id, note } => {
                    let _ = self.voices.note_off(*track_id, *note);
                }
            }
        }

        let simulated_callback_us = self.config.base_callback_us.saturating_add(
            (events.len() as u32).saturating_mul(self.config.per_event_us),
        );
        let dsp_stats = self.dsp.process_block(simulated_callback_us);

        self.events_total = self.events_total.saturating_add(events.len());
        self.metrics.callbacks_total = self.metrics.callbacks_total.saturating_add(1);
        self.metrics.last_callback_us = dsp_stats.block_us;

        if dsp_stats.xrun {
            self.metrics.xruns_total = self.metrics.xruns_total.saturating_add(1);
        }

        self.callback_us_total = self
            .callback_us_total
            .saturating_add(dsp_stats.block_us as u64);
        let callbacks = self.metrics.callbacks_total.max(1);
        self.metrics.avg_callback_us = (self.callback_us_total / callbacks) as u32;
        self.metrics.active_voices = self.voices.active_voice_count() as u32;
        self.metrics.max_voices = self.voices.max_voices() as u32;
        self.metrics.voices_stolen_total = self.voices.voices_stolen_total();
        let lifecycle = self.voices.lifecycle_stats();
        self.metrics.voice_note_on_total = lifecycle.note_on_total;
        self.metrics.voice_note_off_total = lifecycle.note_off_total;
        self.metrics.voice_note_off_miss_total = lifecycle.note_off_miss_total;
        self.metrics.voice_retrigger_total = lifecycle.retrigger_total;
        self.metrics.voice_zero_attack_total = lifecycle.zero_attack_total;
        self.metrics.voice_short_release_total = lifecycle.short_release_total;
        self.metrics.click_risk_total = lifecycle.click_risk_total;
        self.metrics.voice_release_deferred_total = lifecycle.release_deferred_total;
        self.metrics.voice_release_completed_total = lifecycle.release_completed_total;
        self.metrics.voice_release_pending_voices = lifecycle.release_pending_voices;
        self.metrics.voice_steal_releasing_total = lifecycle.steal_releasing_total;
        self.metrics.voice_steal_active_total = lifecycle.steal_active_total;
        self.metrics.voice_polyphony_pressure_total = lifecycle.polyphony_pressure_total;
        self.metrics.voice_sampler_mode_note_on_total = self.sampler_mode_note_on_total;
        self.metrics.voice_silent_note_on_total = self.silent_note_on_total;
    }

    fn events_consumed(&self) -> usize {
        self.events_total
    }

    fn metrics(&self) -> AudioMetrics {
        self.metrics
    }

    fn backend_name(&self) -> &'static str {
        "native-simulated-linux"
    }
}

pub fn build_preferred_audio_backend(prefer_native: bool) -> Box<dyn AudioBackend> {
    if prefer_native {
        Box::new(NativeAudioBackend::default())
    } else {
        Box::new(NoopAudioBackend::default())
    }
}

pub struct StartedAudioBackend {
    backend: Box<dyn AudioBackend>,
    pub used_fallback: bool,
}

impl StartedAudioBackend {
    pub fn backend(&self) -> &dyn AudioBackend {
        self.backend.as_ref()
    }

    pub fn backend_mut(&mut self) -> &mut dyn AudioBackend {
        self.backend.as_mut()
    }
}

pub fn start_with_noop_fallback(mut primary: Box<dyn AudioBackend>) -> StartedAudioBackend {
    if primary.start_checked().is_ok() {
        return StartedAudioBackend {
            backend: primary,
            used_fallback: false,
        };
    }

    let mut fallback: Box<dyn AudioBackend> = Box::new(NoopAudioBackend::default());
    fallback.start();
    StartedAudioBackend {
        backend: fallback,
        used_fallback: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        start_with_noop_fallback, AudioBackend, AudioBackendConfig, NativeAudioBackend,
    };
    use p9_core::events::{RenderEvent, RenderMode};
    use p9_core::model::SynthWaveform;

    fn note_on(track_id: u8, note: u8) -> RenderEvent {
        RenderEvent::NoteOn {
            track_id,
            note,
            velocity: 100,
            render_mode: RenderMode::Synth,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 5,
            release_ms: 80,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        }
    }

    #[test]
    fn native_backend_collects_callback_and_xrun_metrics() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig {
            max_callback_us: 250,
            base_callback_us: 200,
            per_event_us: 100,
            ..AudioBackendConfig::default()
        });
        backend.start_checked().unwrap();

        backend.push_events(&[note_on(0, 60)]);
        backend.push_events(&[]);

        let metrics = backend.metrics();
        assert_eq!(metrics.callbacks_total, 2);
        assert_eq!(metrics.xruns_total, 1);
        assert_eq!(metrics.last_callback_us, 200);
        assert_eq!(metrics.avg_callback_us, 250);
        assert_eq!(metrics.active_voices, 1);
        assert_eq!(metrics.max_voices, 16);
        assert_eq!(metrics.voice_note_on_total, 1);
        assert_eq!(metrics.voice_note_off_total, 0);
        assert_eq!(metrics.click_risk_total, 0);
        assert_eq!(metrics.voice_release_deferred_total, 0);
        assert_eq!(metrics.voice_release_completed_total, 0);
        assert_eq!(metrics.voice_release_pending_voices, 0);
        assert_eq!(metrics.voice_steal_releasing_total, 0);
        assert_eq!(metrics.voice_steal_active_total, 0);
        assert_eq!(metrics.voice_polyphony_pressure_total, 0);
        assert_eq!(metrics.voice_sampler_mode_note_on_total, 0);
        assert_eq!(metrics.voice_silent_note_on_total, 0);
    }

    #[test]
    fn start_with_fallback_uses_noop_when_native_start_fails() {
        let primary = Box::new(NativeAudioBackend::new(AudioBackendConfig {
            fail_on_start: true,
            ..AudioBackendConfig::default()
        }));

        let started = start_with_noop_fallback(primary);
        assert!(started.used_fallback);
        assert_eq!(started.backend().backend_name(), "noop");
    }

    #[test]
    fn voice_allocator_stays_bounded_in_native_backend() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig {
            max_voices: 2,
            ..AudioBackendConfig::default()
        });
        backend.start_checked().unwrap();

        backend.push_events(&[note_on(0, 60)]);
        backend.push_events(&[note_on(0, 62)]);
        backend.push_events(&[note_on(0, 64)]);

        let metrics = backend.metrics();
        assert_eq!(metrics.max_voices, 2);
        assert_eq!(metrics.active_voices, 2);
        assert_eq!(metrics.voices_stolen_total, 1);
        assert_eq!(metrics.click_risk_total, 1);
        assert_eq!(metrics.voice_note_on_total, 3);
        assert_eq!(metrics.voice_retrigger_total, 0);
        assert_eq!(metrics.voice_steal_releasing_total, 0);
        assert_eq!(metrics.voice_steal_active_total, 1);
        assert_eq!(metrics.voice_polyphony_pressure_total, 1);
        assert_eq!(metrics.voice_sampler_mode_note_on_total, 0);
        assert_eq!(metrics.voice_silent_note_on_total, 0);
    }

    #[test]
    fn lifecycle_metrics_surface_retrigger_and_short_release() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig::default());
        backend.start_checked().unwrap();

        backend.push_events(&[RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::Synth,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 0,
            release_ms: 1,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        }]);
        backend.push_events(&[RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::Synth,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 4,
            release_ms: 1,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        }]);
        backend.push_events(&[RenderEvent::NoteOff {
            track_id: 0,
            note: 60,
        }]);
        backend.push_events(&[RenderEvent::NoteOff {
            track_id: 0,
            note: 61,
        }]);

        let metrics = backend.metrics();
        assert_eq!(metrics.voice_note_on_total, 2);
        assert_eq!(metrics.voice_note_off_total, 2);
        assert_eq!(metrics.voice_note_off_miss_total, 1);
        assert_eq!(metrics.voice_retrigger_total, 1);
        assert_eq!(metrics.voice_zero_attack_total, 1);
        assert_eq!(metrics.voice_short_release_total, 1);
        assert_eq!(metrics.click_risk_total, 3);
        assert_eq!(metrics.voice_release_deferred_total, 0);
        assert_eq!(metrics.voice_release_completed_total, 0);
        assert_eq!(metrics.voice_release_pending_voices, 0);
        assert_eq!(metrics.voice_steal_releasing_total, 0);
        assert_eq!(metrics.voice_steal_active_total, 0);
        assert_eq!(metrics.voice_polyphony_pressure_total, 0);
        assert_eq!(metrics.voice_sampler_mode_note_on_total, 0);
        assert_eq!(metrics.voice_silent_note_on_total, 0);
    }

    #[test]
    fn deferred_release_metrics_progress_after_callbacks() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig::default());
        backend.start_checked().unwrap();

        backend.push_events(&[RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::Synth,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 5,
            release_ms: 40,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        }]);
        backend.push_events(&[RenderEvent::NoteOff {
            track_id: 0,
            note: 60,
        }]);

        let mid = backend.metrics();
        assert_eq!(mid.voice_release_deferred_total, 1);
        assert_eq!(mid.voice_release_completed_total, 0);
        assert_eq!(mid.voice_release_pending_voices, 1);
        assert_eq!(mid.active_voices, 1);

        for _ in 0..4 {
            backend.push_events(&[]);
        }

        let end = backend.metrics();
        assert_eq!(end.voice_release_deferred_total, 1);
        assert_eq!(end.voice_release_completed_total, 1);
        assert_eq!(end.voice_release_pending_voices, 0);
        assert_eq!(end.active_voices, 0);
        assert_eq!(end.voice_steal_releasing_total, 0);
        assert_eq!(end.voice_steal_active_total, 0);
        assert_eq!(end.voice_polyphony_pressure_total, 0);
        assert_eq!(end.voice_sampler_mode_note_on_total, 0);
        assert_eq!(end.voice_silent_note_on_total, 0);
    }

    #[test]
    fn stress_burst_prefers_releasing_steals_before_active_steals() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig {
            max_voices: 2,
            ..AudioBackendConfig::default()
        });
        backend.start_checked().unwrap();

        backend.push_events(&[note_on(0, 60)]);
        backend.push_events(&[note_on(0, 62)]);
        backend.push_events(&[RenderEvent::NoteOff {
            track_id: 0,
            note: 60,
        }]);
        backend.push_events(&[note_on(0, 64)]);
        backend.push_events(&[note_on(0, 65)]);

        let metrics = backend.metrics();
        assert_eq!(metrics.voices_stolen_total, 2);
        assert_eq!(metrics.voice_steal_releasing_total, 1);
        assert_eq!(metrics.voice_steal_active_total, 1);
        assert_eq!(metrics.voice_polyphony_pressure_total, 2);
        assert_eq!(metrics.click_risk_total, 1);
        assert_eq!(metrics.voice_sampler_mode_note_on_total, 0);
        assert_eq!(metrics.voice_silent_note_on_total, 0);
    }

    #[test]
    fn zero_gain_note_on_is_counted_without_allocating_voice() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig::default());
        backend.start_checked().unwrap();

        backend.push_events(&[RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::ExternalMuted,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 1,
            release_ms: 24,
            gain: 0,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        }]);

        let metrics = backend.metrics();
        assert_eq!(metrics.voice_silent_note_on_total, 1);
        assert_eq!(metrics.voice_note_on_total, 0);
        assert_eq!(metrics.voice_sampler_mode_note_on_total, 0);
        assert_eq!(metrics.active_voices, 0);
    }

    #[test]
    fn sampler_render_mode_note_on_is_counted() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig::default());
        backend.start_checked().unwrap();

        backend.push_events(&[RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            render_mode: RenderMode::SamplerV1,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 1,
            release_ms: 32,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Punch,
            sampler_transient_level: 110,
            sampler_body_level: 40,
        }]);

        let metrics = backend.metrics();
        assert_eq!(metrics.voice_sampler_mode_note_on_total, 1);
        assert_eq!(metrics.voice_note_on_total, 1);
    }
}
