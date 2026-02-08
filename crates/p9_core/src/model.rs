use std::collections::HashMap;

pub const TRACK_COUNT: usize = 8;
pub const SONG_ROW_COUNT: usize = 256;
pub const CHAIN_ROW_COUNT: usize = 16;
pub const PHRASE_STEP_COUNT: usize = 16;

pub type ChainId = u8;
pub type PhraseId = u8;
pub type InstrumentId = u8;
pub type TableId = u8;
pub type GrooveId = u8;
pub type ScaleId = u8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstrumentType {
    None,
    Synth,
    Sampler,
    MidiOut,
    External,
}

#[derive(Clone, Debug)]
pub struct Song {
    pub name: String,
    pub tempo: u16,
    pub default_groove: GrooveId,
    pub default_scale: ScaleId,
    pub tracks: Vec<Track>,
}

impl Song {
    pub fn new(name: impl Into<String>) -> Self {
        let mut tracks = Vec::with_capacity(TRACK_COUNT);
        for index in 0..TRACK_COUNT {
            tracks.push(Track::new(index as u8));
        }

        Self {
            name: name.into(),
            tempo: 120,
            default_groove: 0,
            default_scale: 0,
            tracks,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Track {
    pub index: u8,
    pub song_rows: Vec<Option<ChainId>>,
    pub mute: bool,
    pub solo: bool,
}

impl Track {
    pub fn new(index: u8) -> Self {
        Self {
            index,
            song_rows: vec![None; SONG_ROW_COUNT],
            mute: false,
            solo: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Chain {
    pub id: ChainId,
    pub rows: Vec<ChainRow>,
}

impl Chain {
    pub fn new(id: ChainId) -> Self {
        Self {
            id,
            rows: vec![ChainRow::default(); CHAIN_ROW_COUNT],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChainRow {
    pub phrase_id: Option<PhraseId>,
    pub transpose: i8,
}

#[derive(Clone, Debug)]
pub struct Phrase {
    pub id: PhraseId,
    pub steps: Vec<Step>,
}

impl Phrase {
    pub fn new(id: PhraseId) -> Self {
        Self {
            id,
            steps: vec![Step::default(); PHRASE_STEP_COUNT],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Step {
    pub note: Option<u8>,
    pub velocity: u8,
    pub instrument_id: Option<InstrumentId>,
    pub fx: Vec<Option<FxCommand>>,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            note: None,
            velocity: 0x40,
            instrument_id: None,
            fx: vec![None; 3],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instrument {
    pub id: InstrumentId,
    pub instrument_type: InstrumentType,
    pub name: String,
    pub send_levels: SendLevels,
    pub table_id: Option<TableId>,
}

impl Instrument {
    pub fn new(id: InstrumentId, instrument_type: InstrumentType, name: impl Into<String>) -> Self {
        Self {
            id,
            instrument_type,
            name: name.into(),
            send_levels: SendLevels::default(),
            table_id: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Table {
    pub id: TableId,
    pub rows: Vec<TableRow>,
}

impl Table {
    pub fn new(id: TableId) -> Self {
        Self {
            id,
            rows: vec![TableRow::default(); CHAIN_ROW_COUNT],
        }
    }
}

#[derive(Clone, Debug)]
pub struct TableRow {
    pub note_offset: i8,
    pub volume: u8,
    pub fx: Vec<Option<FxCommand>>,
}

impl Default for TableRow {
    fn default() -> Self {
        Self {
            note_offset: 0,
            volume: 0x40,
            fx: vec![None; 3],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Groove {
    pub id: GrooveId,
    pub ticks_pattern: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Scale {
    pub id: ScaleId,
    pub key: u8,
    pub interval_mask: u16,
}

#[derive(Clone, Debug)]
pub struct Mixer {
    pub track_levels: [u8; TRACK_COUNT],
    pub master_level: u8,
    pub send_levels: SendLevels,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            track_levels: [0x80; TRACK_COUNT],
            master_level: 0x80,
            send_levels: SendLevels::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SendLevels {
    pub mfx: u8,
    pub delay: u8,
    pub reverb: u8,
}

#[derive(Clone, Debug)]
pub struct FxCommand {
    pub code: String,
    pub value: u8,
}

#[derive(Clone, Debug)]
pub struct ProjectData {
    pub song: Song,
    pub chains: HashMap<ChainId, Chain>,
    pub phrases: HashMap<PhraseId, Phrase>,
    pub instruments: HashMap<InstrumentId, Instrument>,
    pub tables: HashMap<TableId, Table>,
    pub grooves: HashMap<GrooveId, Groove>,
    pub scales: HashMap<ScaleId, Scale>,
    pub mixer: Mixer,
}

impl ProjectData {
    pub fn new(song_name: impl Into<String>) -> Self {
        Self {
            song: Song::new(song_name),
            chains: HashMap::new(),
            phrases: HashMap::new(),
            instruments: HashMap::new(),
            tables: HashMap::new(),
            grooves: HashMap::new(),
            scales: HashMap::new(),
            mixer: Mixer::default(),
        }
    }
}
