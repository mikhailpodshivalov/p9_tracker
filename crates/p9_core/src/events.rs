use crate::model::InstrumentId;

#[derive(Clone, Debug)]
pub enum RenderEvent {
    NoteOn {
        track_id: u8,
        note: u8,
        velocity: u8,
        instrument_id: Option<InstrumentId>,
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
