use crate::model::{InstrumentId, SynthWaveform};

#[derive(Clone, Debug)]
pub enum RenderEvent {
    NoteOn {
        track_id: u8,
        note: u8,
        velocity: u8,
        instrument_id: Option<InstrumentId>,
        waveform: SynthWaveform,
        attack_ms: u16,
        release_ms: u16,
        gain: u8,
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
