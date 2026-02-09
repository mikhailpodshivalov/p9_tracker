use crate::dsp::DspPipeline;
use p9_core::events::RenderEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioMetrics {
    pub sample_rate_hz: u32,
    pub buffer_size_frames: u32,
    pub callbacks_total: u64,
    pub xruns_total: u64,
    pub last_callback_us: u32,
    pub avg_callback_us: u32,
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
}

impl NativeAudioBackend {
    pub fn new(config: AudioBackendConfig) -> Self {
        Self {
            running: false,
            events_total: 0,
            metrics: AudioMetrics {
                sample_rate_hz: config.sample_rate_hz,
                buffer_size_frames: config.buffer_size_frames,
                ..AudioMetrics::default()
            },
            callback_us_total: 0,
            dsp: DspPipeline::new(config.max_callback_us),
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
    use p9_core::events::RenderEvent;

    #[test]
    fn native_backend_collects_callback_and_xrun_metrics() {
        let mut backend = NativeAudioBackend::new(AudioBackendConfig {
            max_callback_us: 250,
            base_callback_us: 200,
            per_event_us: 100,
            ..AudioBackendConfig::default()
        });
        backend.start_checked().unwrap();

        backend.push_events(&[RenderEvent::NoteOn {
            track_id: 0,
            note: 60,
            velocity: 100,
            instrument_id: Some(0),
        }]);
        backend.push_events(&[]);

        let metrics = backend.metrics();
        assert_eq!(metrics.callbacks_total, 2);
        assert_eq!(metrics.xruns_total, 1);
        assert_eq!(metrics.last_callback_us, 200);
        assert_eq!(metrics.avg_callback_us, 250);
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
}
