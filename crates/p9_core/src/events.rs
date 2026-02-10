use crate::model::{InstrumentId, SamplerRenderVariant, SynthWaveform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Synth,
    SamplerV1,
    ExternalMuted,
}

#[derive(Clone, Debug)]
pub enum RenderEvent {
    NoteOn {
        track_id: u8,
        note: u8,
        velocity: u8,
        render_mode: RenderMode,
        instrument_id: Option<InstrumentId>,
        waveform: SynthWaveform,
        attack_ms: u16,
        release_ms: u16,
        gain: u8,
        sampler_variant: SamplerRenderVariant,
        sampler_transient_level: u8,
        sampler_body_level: u8,
    },
    NoteOff {
        track_id: u8,
        note: u8,
    },
}

#[derive(Clone, Debug, Default)]
pub struct TransportState {
    pub tick: u64,
    pub is_playing: bool,
}
