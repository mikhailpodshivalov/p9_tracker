use crate::model::{
    Chain, ChainId, FxCommand, Groove, GrooveId, Instrument, InstrumentId, Phrase, PhraseId,
    ProjectData, Scale, ScaleId, Table, TableId,
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
    SetStepFx {
        phrase_id: PhraseId,
        step_index: usize,
        fx_slot: usize,
        fx: Option<FxCommand>,
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
    UpsertTable {
        table: Table,
    },
    SetTableRow {
        table_id: TableId,
        row: usize,
        note_offset: i8,
        volume: u8,
    },
    SetTableRowFx {
        table_id: TableId,
        row: usize,
        fx_slot: usize,
        fx: Option<FxCommand>,
    },
    SetTrackLevel {
        track_index: usize,
        level: u8,
    },
    SetMasterLevel {
        level: u8,
    },
    SetMixerSends {
        mfx: u8,
        delay: u8,
        reverb: u8,
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
    InvalidFxSlot(usize),
    InvalidTableRow(usize),
    InvalidFxCode(String),
    InvalidFxValue(String, u8),
    MissingChain(ChainId),
    MissingPhrase(PhraseId),
    MissingTable(TableId),
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
            EngineCommand::SetStepFx {
                phrase_id,
                step_index,
                fx_slot,
                fx,
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

                let slot = step
                    .fx
                    .get_mut(fx_slot)
                    .ok_or(EngineError::InvalidFxSlot(fx_slot))?;

                let normalized = normalize_fx(fx)?;
                *slot = normalized;
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
            EngineCommand::UpsertTable { table } => {
                self.project.tables.insert(table.id, table);
                Ok(())
            }
            EngineCommand::SetTableRow {
                table_id,
                row,
                note_offset,
                volume,
            } => {
                let table = self
                    .project
                    .tables
                    .get_mut(&table_id)
                    .ok_or(EngineError::MissingTable(table_id))?;

                let target_row = table
                    .rows
                    .get_mut(row)
                    .ok_or(EngineError::InvalidTableRow(row))?;
                target_row.note_offset = note_offset;
                target_row.volume = volume;
                Ok(())
            }
            EngineCommand::SetTableRowFx {
                table_id,
                row,
                fx_slot,
                fx,
            } => {
                let table = self
                    .project
                    .tables
                    .get_mut(&table_id)
                    .ok_or(EngineError::MissingTable(table_id))?;
                let target_row = table
                    .rows
                    .get_mut(row)
                    .ok_or(EngineError::InvalidTableRow(row))?;
                let slot = target_row
                    .fx
                    .get_mut(fx_slot)
                    .ok_or(EngineError::InvalidFxSlot(fx_slot))?;

                let normalized = normalize_fx(fx)?;
                *slot = normalized;
                Ok(())
            }
            EngineCommand::SetTrackLevel { track_index, level } => {
                let _track = self
                    .project
                    .song
                    .tracks
                    .get(track_index)
                    .ok_or(EngineError::InvalidTrackIndex(track_index))?;
                self.project.mixer.track_levels[track_index] = level;
                Ok(())
            }
            EngineCommand::SetMasterLevel { level } => {
                self.project.mixer.master_level = level;
                Ok(())
            }
            EngineCommand::SetMixerSends { mfx, delay, reverb } => {
                self.project.mixer.send_levels.mfx = mfx;
                self.project.mixer.send_levels.delay = delay;
                self.project.mixer.send_levels.reverb = reverb;
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

fn normalize_fx(fx: Option<FxCommand>) -> Result<Option<FxCommand>, EngineError> {
    let Some(mut command) = fx else {
        return Ok(None);
    };

    command.code = command.code.trim().to_ascii_uppercase();
    validate_fx_command(&command)?;
    Ok(Some(command))
}

fn validate_fx_command(command: &FxCommand) -> Result<(), EngineError> {
    match command.code.as_str() {
        "VOL" => {
            if (1..=127).contains(&command.value) {
                Ok(())
            } else {
                Err(EngineError::InvalidFxValue(command.code.clone(), command.value))
            }
        }
        "TRN" => {
            if command.value <= 96 {
                Ok(())
            } else {
                Err(EngineError::InvalidFxValue(command.code.clone(), command.value))
            }
        }
        "LEN" => {
            if (1..=16).contains(&command.value) {
                Ok(())
            } else {
                Err(EngineError::InvalidFxValue(command.code.clone(), command.value))
            }
        }
        _ => Err(EngineError::InvalidFxCode(command.code.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, EngineCommand, EngineError};
    use crate::model::{Chain, FxCommand, Phrase, Table};

    fn setup_engine() -> Engine {
        let mut engine = Engine::new("engine");
        engine
            .apply_command(EngineCommand::UpsertChain { chain: Chain::new(0) })
            .unwrap();
        engine
            .apply_command(EngineCommand::UpsertPhrase {
                phrase: Phrase::new(0),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetSongRowChain {
                track_index: 0,
                row: 0,
                chain_id: Some(0),
            })
            .unwrap();
        engine
    }

    #[test]
    fn rejects_unknown_fx_code() {
        let mut engine = setup_engine();
        let result = engine.apply_command(EngineCommand::SetStepFx {
            phrase_id: 0,
            step_index: 0,
            fx_slot: 0,
            fx: Some(FxCommand {
                code: "BAD".to_string(),
                value: 10,
            }),
        });
        match result {
            Err(EngineError::InvalidFxCode(code)) => assert_eq!(code, "BAD"),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_fx_slot() {
        let mut engine = setup_engine();
        let result = engine.apply_command(EngineCommand::SetStepFx {
            phrase_id: 0,
            step_index: 0,
            fx_slot: 99,
            fx: Some(FxCommand {
                code: "VOL".to_string(),
                value: 100,
            }),
        });
        match result {
            Err(EngineError::InvalidFxSlot(slot)) => assert_eq!(slot, 99),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn table_row_and_mixer_commands_update_state() {
        let mut engine = setup_engine();
        engine
            .apply_command(EngineCommand::UpsertTable {
                table: Table::new(0),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetTableRow {
                table_id: 0,
                row: 0,
                note_offset: 3,
                volume: 90,
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetTrackLevel {
                track_index: 0,
                level: 100,
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetMasterLevel { level: 110 })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetMixerSends {
                mfx: 10,
                delay: 20,
                reverb: 30,
            })
            .unwrap();

        let project = engine.snapshot();
        assert_eq!(project.tables.get(&0).unwrap().rows[0].note_offset, 3);
        assert_eq!(project.tables.get(&0).unwrap().rows[0].volume, 90);
        assert_eq!(project.mixer.track_levels[0], 100);
        assert_eq!(project.mixer.master_level, 110);
        assert_eq!(project.mixer.send_levels.mfx, 10);
        assert_eq!(project.mixer.send_levels.delay, 20);
        assert_eq!(project.mixer.send_levels.reverb, 30);
    }
}
