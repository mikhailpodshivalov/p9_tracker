use crate::model::{
    Chain, ChainId, Groove, GrooveId, Instrument, InstrumentId, Phrase, PhraseId, ProjectData,
    Scale, ScaleId,
};

#[derive(Clone, Debug)]
pub enum EngineCommand {
    SetTempo(u16),
    SetDefaultGroove(GrooveId),
    SetDefaultScale(ScaleId),
    ToggleTrackMute {
        track_index: usize,
    },
    SetTrackGrooveOverride {
        track_index: usize,
        groove_id: Option<GrooveId>,
    },
    SetTrackScaleOverride {
        track_index: usize,
        scale_id: Option<ScaleId>,
    },
    SetSongRowChain {
        track_index: usize,
        row: usize,
        chain_id: Option<ChainId>,
    },
    SetChainRowPhrase {
        chain_id: ChainId,
        row: usize,
        phrase_id: Option<PhraseId>,
        transpose: i8,
    },
    SetPhraseStep {
        phrase_id: PhraseId,
        step_index: usize,
        note: Option<u8>,
        velocity: u8,
        instrument_id: Option<InstrumentId>,
    },
    UpsertChain {
        chain: Chain,
    },
    UpsertPhrase {
        phrase: Phrase,
    },
    UpsertInstrument {
        instrument: Instrument,
    },
    UpsertGroove {
        groove: Groove,
    },
    UpsertScale {
        scale: Scale,
    },
}

#[derive(Clone, Debug)]
pub enum EngineError {
    InvalidTempo,
    InvalidTrackIndex(usize),
    InvalidSongRow(usize),
    InvalidChainRow(usize),
    InvalidPhraseStep(usize),
    MissingChain(ChainId),
    MissingPhrase(PhraseId),
}

pub struct Engine {
    project: ProjectData,
}

impl Engine {
    pub fn new(song_name: impl Into<String>) -> Self {
        Self {
            project: ProjectData::new(song_name),
        }
    }

    pub fn snapshot(&self) -> &ProjectData {
        &self.project
    }

    pub fn apply_command(&mut self, command: EngineCommand) -> Result<(), EngineError> {
        match command {
            EngineCommand::SetTempo(tempo) => {
                if tempo == 0 {
                    return Err(EngineError::InvalidTempo);
                }
                self.project.song.tempo = tempo;
                Ok(())
            }
            EngineCommand::SetDefaultGroove(groove_id) => {
                self.project.song.default_groove = groove_id;
                Ok(())
            }
            EngineCommand::SetDefaultScale(scale_id) => {
                self.project.song.default_scale = scale_id;
                Ok(())
            }
            EngineCommand::ToggleTrackMute { track_index } => {
                let track = self
                    .project
                    .song
                    .tracks
                    .get_mut(track_index)
                    .ok_or(EngineError::InvalidTrackIndex(track_index))?;
                track.mute = !track.mute;
                Ok(())
            }
            EngineCommand::SetTrackGrooveOverride {
                track_index,
                groove_id,
            } => {
                let track = self
                    .project
                    .song
                    .tracks
                    .get_mut(track_index)
                    .ok_or(EngineError::InvalidTrackIndex(track_index))?;
                track.groove_override = groove_id;
                Ok(())
            }
            EngineCommand::SetTrackScaleOverride {
                track_index,
                scale_id,
            } => {
                let track = self
                    .project
                    .song
                    .tracks
                    .get_mut(track_index)
                    .ok_or(EngineError::InvalidTrackIndex(track_index))?;
                track.scale_override = scale_id;
                Ok(())
            }
            EngineCommand::SetSongRowChain {
                track_index,
                row,
                chain_id,
            } => {
                let track = self
                    .project
                    .song
                    .tracks
                    .get_mut(track_index)
                    .ok_or(EngineError::InvalidTrackIndex(track_index))?;

                let slot = track
                    .song_rows
                    .get_mut(row)
                    .ok_or(EngineError::InvalidSongRow(row))?;
                *slot = chain_id;
                Ok(())
            }
            EngineCommand::SetChainRowPhrase {
                chain_id,
                row,
                phrase_id,
                transpose,
            } => {
                let chain = self
                    .project
                    .chains
                    .get_mut(&chain_id)
                    .ok_or(EngineError::MissingChain(chain_id))?;

                let target_row = chain
                    .rows
                    .get_mut(row)
                    .ok_or(EngineError::InvalidChainRow(row))?;
                target_row.phrase_id = phrase_id;
                target_row.transpose = transpose;
                Ok(())
            }
            EngineCommand::SetPhraseStep {
                phrase_id,
                step_index,
                note,
                velocity,
                instrument_id,
            } => {
                let phrase = self
                    .project
                    .phrases
                    .get_mut(&phrase_id)
                    .ok_or(EngineError::MissingPhrase(phrase_id))?;

                let step = phrase
                    .steps
                    .get_mut(step_index)
                    .ok_or(EngineError::InvalidPhraseStep(step_index))?;
                step.note = note;
                step.velocity = velocity;
                step.instrument_id = instrument_id;
                Ok(())
            }
            EngineCommand::UpsertChain { chain } => {
                self.project.chains.insert(chain.id, chain);
                Ok(())
            }
            EngineCommand::UpsertPhrase { phrase } => {
                self.project.phrases.insert(phrase.id, phrase);
                Ok(())
            }
            EngineCommand::UpsertInstrument { instrument } => {
                self.project.instruments.insert(instrument.id, instrument);
                Ok(())
            }
            EngineCommand::UpsertGroove { groove } => {
                self.project.grooves.insert(groove.id, groove);
                Ok(())
            }
            EngineCommand::UpsertScale { scale } => {
                self.project.scales.insert(scale.id, scale);
                Ok(())
            }
        }
    }
}
