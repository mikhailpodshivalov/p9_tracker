use crate::model::{Chain, ChainId, Instrument, Phrase, ProjectData};

#[derive(Clone, Debug)]
pub enum EngineCommand {
    SetTempo(u16),
    ToggleTrackMute {
        track_index: usize,
    },
    SetSongRowChain {
        track_index: usize,
        row: usize,
        chain_id: Option<ChainId>,
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
}

#[derive(Clone, Debug)]
pub enum EngineError {
    InvalidTempo,
    InvalidTrackIndex(usize),
    InvalidSongRow(usize),
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
        }
    }
}
