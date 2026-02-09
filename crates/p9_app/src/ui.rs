use crate::runtime::{RuntimeCommand, RuntimeCoordinator};
use p9_core::engine::{Engine, EngineCommand, EngineError};
use p9_core::model::{
    Chain, Instrument, InstrumentId, InstrumentType, Phrase, Scale, CHAIN_ROW_COUNT,
    SONG_ROW_COUNT, TRACK_COUNT,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiScreen {
    Song,
    Chain,
    Phrase,
    Mixer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleHighlightState {
    Disabled,
    InScale,
    OutOfScale,
    NoNote,
    NoScale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiSnapshot {
    pub screen: UiScreen,
    pub focused_track: usize,
    pub selected_song_row: usize,
    pub selected_chain_row: usize,
    pub selected_phrase_id: u8,
    pub selected_step: usize,
    pub is_playing: bool,
    pub tick: u64,
    pub scale_highlight: ScaleHighlightState,
    pub focused_track_level: u8,
}

#[derive(Clone, Debug)]
pub struct UiController {
    screen: UiScreen,
    focused_track: usize,
    selected_song_row: usize,
    selected_chain_row: usize,
    selected_phrase_id: u8,
    selected_step: usize,
    scale_highlight_enabled: bool,
}

impl Default for UiController {
    fn default() -> Self {
        Self {
            screen: UiScreen::Song,
            focused_track: 0,
            selected_song_row: 0,
            selected_chain_row: 0,
            selected_phrase_id: 0,
            selected_step: 0,
            scale_highlight_enabled: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum UiAction {
    NextScreen,
    PrevScreen,
    FocusTrackLeft,
    FocusTrackRight,
    SelectSongRow(usize),
    SelectChainRow(usize),
    SelectPhrase(u8),
    SelectStep(usize),
    ToggleScaleHighlight,
    TogglePlayStop,
    RewindTransport,
    EnsureInstrument {
        instrument_id: InstrumentId,
        instrument_type: InstrumentType,
        name: String,
    },
    EnsureChain {
        chain_id: u8,
    },
    EnsurePhrase {
        phrase_id: u8,
    },
    BindTrackRowToChain {
        song_row: usize,
        chain_id: Option<u8>,
    },
    BindChainRowToPhrase {
        chain_id: u8,
        chain_row: usize,
        phrase_id: Option<u8>,
        transpose: i8,
    },
    EditStep {
        phrase_id: u8,
        step_index: usize,
        note: Option<u8>,
        velocity: u8,
        instrument_id: Option<u8>,
    },
    SetTrackLevel(u8),
    SetMasterLevel(u8),
}

#[derive(Clone, Debug)]
pub enum UiError {
    Engine(EngineError),
    InvalidTrack(usize),
    InvalidSongRow(usize),
    InvalidChainRow(usize),
    InvalidStep(usize),
}

impl From<EngineError> for UiError {
    fn from(value: EngineError) -> Self {
        UiError::Engine(value)
    }
}

impl UiController {
    pub fn handle_action(
        &mut self,
        action: UiAction,
        engine: &mut Engine,
        runtime: &mut RuntimeCoordinator,
    ) -> Result<(), UiError> {
        match action {
            UiAction::NextScreen => {
                self.screen = match self.screen {
                    UiScreen::Song => UiScreen::Chain,
                    UiScreen::Chain => UiScreen::Phrase,
                    UiScreen::Phrase => UiScreen::Mixer,
                    UiScreen::Mixer => UiScreen::Song,
                };
                Ok(())
            }
            UiAction::PrevScreen => {
                self.screen = match self.screen {
                    UiScreen::Song => UiScreen::Mixer,
                    UiScreen::Chain => UiScreen::Song,
                    UiScreen::Phrase => UiScreen::Chain,
                    UiScreen::Mixer => UiScreen::Phrase,
                };
                Ok(())
            }
            UiAction::FocusTrackLeft => {
                if self.focused_track == 0 {
                    self.focused_track = TRACK_COUNT - 1;
                } else {
                    self.focused_track -= 1;
                }
                Ok(())
            }
            UiAction::FocusTrackRight => {
                self.focused_track = (self.focused_track + 1) % TRACK_COUNT;
                Ok(())
            }
            UiAction::SelectSongRow(row) => {
                if row >= SONG_ROW_COUNT {
                    return Err(UiError::InvalidSongRow(row));
                }
                self.selected_song_row = row;
                Ok(())
            }
            UiAction::SelectChainRow(row) => {
                if row >= CHAIN_ROW_COUNT {
                    return Err(UiError::InvalidChainRow(row));
                }
                self.selected_chain_row = row;
                Ok(())
            }
            UiAction::SelectPhrase(phrase_id) => {
                self.selected_phrase_id = phrase_id;
                Ok(())
            }
            UiAction::SelectStep(step) => {
                if step >= p9_core::model::PHRASE_STEP_COUNT {
                    return Err(UiError::InvalidStep(step));
                }
                self.selected_step = step;
                Ok(())
            }
            UiAction::ToggleScaleHighlight => {
                self.scale_highlight_enabled = !self.scale_highlight_enabled;
                Ok(())
            }
            UiAction::TogglePlayStop => {
                let playing = runtime.snapshot().is_playing;
                runtime.enqueue_command(if playing {
                    RuntimeCommand::Stop
                } else {
                    RuntimeCommand::Start
                });
                Ok(())
            }
            UiAction::RewindTransport => {
                runtime.enqueue_commands([RuntimeCommand::Stop, RuntimeCommand::Rewind]);
                Ok(())
            }
            UiAction::EnsureInstrument {
                instrument_id,
                instrument_type,
                name,
            } => {
                engine.apply_command(EngineCommand::UpsertInstrument {
                    instrument: Instrument::new(instrument_id, instrument_type, name),
                })?;
                Ok(())
            }
            UiAction::EnsureChain { chain_id } => {
                engine.apply_command(EngineCommand::UpsertChain {
                    chain: Chain::new(chain_id),
                })?;
                Ok(())
            }
            UiAction::EnsurePhrase { phrase_id } => {
                engine.apply_command(EngineCommand::UpsertPhrase {
                    phrase: Phrase::new(phrase_id),
                })?;
                Ok(())
            }
            UiAction::BindTrackRowToChain { song_row, chain_id } => {
                self.selected_song_row = song_row;
                engine.apply_command(EngineCommand::SetSongRowChain {
                    track_index: self.focused_track,
                    row: song_row,
                    chain_id,
                })?;
                Ok(())
            }
            UiAction::BindChainRowToPhrase {
                chain_id,
                chain_row,
                phrase_id,
                transpose,
            } => {
                self.selected_chain_row = chain_row;
                engine.apply_command(EngineCommand::SetChainRowPhrase {
                    chain_id,
                    row: chain_row,
                    phrase_id,
                    transpose,
                })?;
                Ok(())
            }
            UiAction::EditStep {
                phrase_id,
                step_index,
                note,
                velocity,
                instrument_id,
            } => {
                self.selected_phrase_id = phrase_id;
                self.selected_step = step_index;
                engine.apply_command(EngineCommand::SetPhraseStep {
                    phrase_id,
                    step_index,
                    note,
                    velocity,
                    instrument_id,
                })?;
                Ok(())
            }
            UiAction::SetTrackLevel(level) => {
                if self.focused_track >= TRACK_COUNT {
                    return Err(UiError::InvalidTrack(self.focused_track));
                }
                engine.apply_command(EngineCommand::SetTrackLevel {
                    track_index: self.focused_track,
                    level,
                })?;
                Ok(())
            }
            UiAction::SetMasterLevel(level) => {
                engine.apply_command(EngineCommand::SetMasterLevel { level })?;
                Ok(())
            }
        }
    }

    pub fn snapshot(&self, engine: &Engine, runtime: &RuntimeCoordinator) -> UiSnapshot {
        let transport = runtime.snapshot();
        let project = engine.snapshot();
        let focused_track_level = project.mixer.track_levels[self.focused_track];

        let scale_highlight = if !self.scale_highlight_enabled {
            ScaleHighlightState::Disabled
        } else {
            self.compute_scale_highlight(project)
        };

        UiSnapshot {
            screen: self.screen,
            focused_track: self.focused_track,
            selected_song_row: self.selected_song_row,
            selected_chain_row: self.selected_chain_row,
            selected_phrase_id: self.selected_phrase_id,
            selected_step: self.selected_step,
            is_playing: transport.is_playing,
            tick: transport.tick,
            scale_highlight,
            focused_track_level,
        }
    }

    fn compute_scale_highlight(&self, project: &p9_core::model::ProjectData) -> ScaleHighlightState {
        let Some(phrase) = project.phrases.get(&self.selected_phrase_id) else {
            return ScaleHighlightState::NoNote;
        };
        let Some(step) = phrase.steps.get(self.selected_step) else {
            return ScaleHighlightState::NoNote;
        };
        let Some(note) = step.note else {
            return ScaleHighlightState::NoNote;
        };

        let track = &project.song.tracks[self.focused_track];
        let scale_id = track.scale_override.unwrap_or(project.song.default_scale);
        let Some(scale) = project.scales.get(&scale_id) else {
            return ScaleHighlightState::NoScale;
        };

        if is_note_in_scale(note, scale) {
            ScaleHighlightState::InScale
        } else {
            ScaleHighlightState::OutOfScale
        }
    }
}

fn is_note_in_scale(note: u8, scale: &Scale) -> bool {
    if scale.interval_mask == 0 {
        return false;
    }
    let key = scale.key % 12;
    let pitch_class = note % 12;
    let interval = (12 + pitch_class as i16 - key as i16) % 12;
    ((scale.interval_mask >> interval) & 1) != 0
}

#[cfg(test)]
mod tests {
    use super::{ScaleHighlightState, UiAction, UiController, UiScreen};
    use crate::runtime::RuntimeCoordinator;
    use p9_core::engine::{Engine, EngineCommand};
    use p9_core::model::{Scale, TRACK_COUNT};
    use p9_rt::audio::{AudioBackend, NoopAudioBackend};
    use p9_rt::midi::NoopMidiOutput;

    fn major_scale_mask() -> u16 {
        let intervals = [0u16, 2, 4, 5, 7, 9, 11];
        let mut mask = 0u16;
        for interval in intervals {
            mask |= 1 << interval;
        }
        mask
    }

    #[test]
    fn navigation_and_focus_are_keyboard_driven() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("ui");
        let mut runtime = RuntimeCoordinator::new(24);

        ui.handle_action(UiAction::NextScreen, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::NextScreen, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::NextScreen, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::NextScreen, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::FocusTrackLeft, &mut engine, &mut runtime)
            .unwrap();

        let snap = ui.snapshot(&engine, &runtime);
        assert_eq!(snap.screen, UiScreen::Song);
        assert_eq!(snap.focused_track, TRACK_COUNT - 1);
    }

    #[test]
    fn minimal_edit_loop_creates_phrase_and_emits_events() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("flow");
        let mut runtime = RuntimeCoordinator::new(4);

        ui.handle_action(
            UiAction::EnsureInstrument {
                instrument_id: 0,
                instrument_type: p9_core::model::InstrumentType::Synth,
                name: "UI Lead".to_string(),
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(UiAction::EnsureChain { chain_id: 0 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::EnsurePhrase { phrase_id: 0 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(
            UiAction::BindTrackRowToChain {
                song_row: 0,
                chain_id: Some(0),
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(
            UiAction::BindChainRowToPhrase {
                chain_id: 0,
                chain_row: 0,
                phrase_id: Some(0),
                transpose: 0,
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(
            UiAction::EditStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(60),
                velocity: 100,
                instrument_id: Some(0),
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(UiAction::SetTrackLevel(96), &mut engine, &mut runtime)
            .unwrap();

        let mut audio = NoopAudioBackend::default();
        audio.start();
        let mut midi = NoopMidiOutput::default();
        let report = runtime.run_tick(&engine, &mut audio, &mut midi);

        assert_eq!(report.events_emitted, 1);
        assert_eq!(engine.snapshot().mixer.track_levels[0], 96);
    }

    #[test]
    fn play_stop_control_toggles_transport_state() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("transport");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut audio = NoopAudioBackend::default();
        audio.start();
        let mut midi = NoopMidiOutput::default();

        ui.handle_action(UiAction::TogglePlayStop, &mut engine, &mut runtime)
            .unwrap();
        runtime.run_tick(&engine, &mut audio, &mut midi);
        assert!(!runtime.snapshot().is_playing);

        ui.handle_action(UiAction::TogglePlayStop, &mut engine, &mut runtime)
            .unwrap();
        runtime.run_tick(&engine, &mut audio, &mut midi);
        assert!(runtime.snapshot().is_playing);
    }

    #[test]
    fn row_selection_actions_update_cursor() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("cursor");
        let mut runtime = RuntimeCoordinator::new(24);

        ui.handle_action(UiAction::SelectSongRow(3), &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::SelectChainRow(2), &mut engine, &mut runtime)
            .unwrap();

        let snapshot = ui.snapshot(&engine, &runtime);
        assert_eq!(snapshot.selected_song_row, 3);
        assert_eq!(snapshot.selected_chain_row, 2);
    }

    #[test]
    fn rewind_transport_action_queues_stop_and_rewind() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("rewind");
        let mut runtime = RuntimeCoordinator::new(24);

        ui.handle_action(UiAction::RewindTransport, &mut engine, &mut runtime)
            .unwrap();

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.queued_commands, 2);
    }

    #[test]
    fn scale_highlight_state_can_be_toggled() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("scale");
        let mut runtime = RuntimeCoordinator::new(24);

        ui.handle_action(UiAction::EnsurePhrase { phrase_id: 0 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(
            UiAction::EditStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(61),
                velocity: 100,
                instrument_id: None,
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        engine
            .apply_command(EngineCommand::UpsertScale {
                scale: Scale {
                    id: 1,
                    key: 0,
                    interval_mask: major_scale_mask(),
                },
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetDefaultScale(1))
            .unwrap();

        let before = ui.snapshot(&engine, &runtime);
        assert_eq!(before.scale_highlight, ScaleHighlightState::OutOfScale);

        ui.handle_action(UiAction::ToggleScaleHighlight, &mut engine, &mut runtime)
            .unwrap();
        let after = ui.snapshot(&engine, &runtime);
        assert_eq!(after.scale_highlight, ScaleHighlightState::Disabled);
    }
}
