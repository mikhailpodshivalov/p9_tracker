use crate::engine::Engine;
use crate::events::RenderEvent;
use crate::model::{
    ChainId, InstrumentId, ProjectData, Scale, SynthParams, PHRASE_STEP_COUNT, SONG_ROW_COUNT,
    TRACK_COUNT,
};

#[derive(Clone, Debug, Default)]
pub struct TrackPlaybackState {
    pub song_row: usize,
    pub chain_row: usize,
    pub phrase_step: usize,
    pub tick_in_step: u8,
    pub active_note: Option<u8>,
    pub note_steps_remaining: Option<u8>,
}

#[derive(Clone, Copy, Debug)]
struct StepPlaybackData {
    track_id: u8,
    note: u8,
    velocity: u8,
    instrument_id: Option<InstrumentId>,
    note_length_steps: u8,
    synth_params: SynthParams,
}

pub struct Scheduler {
    pub ppq: u16,
    pub ticks_per_step: u8,
    pub current_tick: u64,
    pub is_playing: bool,
    pub track_state: Vec<TrackPlaybackState>,
}

impl Scheduler {
    pub fn new(ppq: u16) -> Self {
        let ticks_per_step = (ppq / 4).max(1) as u8;

        Self {
            ppq,
            ticks_per_step,
            current_tick: 0,
            is_playing: true,
            track_state: vec![TrackPlaybackState::default(); TRACK_COUNT],
        }
    }

    pub fn start(&mut self) {
        self.is_playing = true;
    }

    pub fn stop(&mut self) {
        self.is_playing = false;
    }

    pub fn rewind(&mut self) {
        self.current_tick = 0;
        for state in &mut self.track_state {
            *state = TrackPlaybackState::default();
        }
    }

    pub fn tick(&mut self, engine: &Engine) -> Vec<RenderEvent> {
        if !self.is_playing {
            return Vec::new();
        }

        let project = engine.snapshot();
        let mut out = Vec::new();

        for track_index in 0..project.song.tracks.len() {
            if !self.track_is_audible(project, track_index) {
                self.force_note_off_if_active(project, track_index, &mut out);
                self.advance_one_tick(project, track_index);
                continue;
            }

            self.ensure_playable_position(project, track_index);

            if self.track_state[track_index].tick_in_step == 0 {
                self.process_step_boundary(project, track_index, &mut out);
            }

            self.advance_one_tick(project, track_index);
        }

        self.current_tick = self.current_tick.saturating_add(1);
        out
    }

    fn track_is_audible(&self, project: &ProjectData, track_index: usize) -> bool {
        let has_solo = project.song.tracks.iter().any(|track| track.solo);
        let track = &project.song.tracks[track_index];

        if has_solo {
            track.solo && !track.mute
        } else {
            !track.mute
        }
    }

    fn ensure_playable_position(&mut self, project: &ProjectData, track_index: usize) {
        let song_row = self.track_state[track_index].song_row;
        let chain_row = self.track_state[track_index].chain_row;

        if self.is_chain_row_playable(project, track_index, song_row, chain_row) {
            return;
        }

        let next_song_row = self.next_song_row_with_chain(project, track_index, song_row);
        let state = &mut self.track_state[track_index];
        state.song_row = next_song_row;
        state.chain_row = 0;
        state.phrase_step = 0;
        state.tick_in_step = 0;
    }

    fn process_step_boundary(
        &mut self,
        project: &ProjectData,
        track_index: usize,
        out: &mut Vec<RenderEvent>,
    ) {
        self.emit_scheduled_note_off(project, track_index, out);

        let Some(step_data) = self.resolve_step_data(project, track_index) else {
            return;
        };

        self.force_note_off_if_active(project, track_index, out);

        out.push(RenderEvent::NoteOn {
            track_id: step_data.track_id,
            note: step_data.note,
            velocity: step_data.velocity,
            instrument_id: step_data.instrument_id,
            waveform: step_data.synth_params.waveform,
            attack_ms: step_data.synth_params.attack_ms,
            release_ms: step_data.synth_params.release_ms,
            gain: step_data.synth_params.gain,
        });

        let state = &mut self.track_state[track_index];
        state.active_note = Some(step_data.note);
        state.note_steps_remaining = Some(step_data.note_length_steps.max(1));
    }

    fn emit_scheduled_note_off(
        &mut self,
        project: &ProjectData,
        track_index: usize,
        out: &mut Vec<RenderEvent>,
    ) {
        let (track_id, active_note, note_steps_remaining) = {
            let Some(track) = project.song.tracks.get(track_index) else {
                return;
            };
            let state = &self.track_state[track_index];
            (track.index, state.active_note, state.note_steps_remaining)
        };

        let (Some(note), Some(remaining)) = (active_note, note_steps_remaining) else {
            return;
        };

        if remaining <= 1 {
            out.push(RenderEvent::NoteOff { track_id, note });
            let state = &mut self.track_state[track_index];
            state.active_note = None;
            state.note_steps_remaining = None;
        } else {
            self.track_state[track_index].note_steps_remaining = Some(remaining - 1);
        }
    }

    fn force_note_off_if_active(
        &mut self,
        project: &ProjectData,
        track_index: usize,
        out: &mut Vec<RenderEvent>,
    ) {
        let active_note = self.track_state[track_index].active_note;
        let Some(note) = active_note else {
            return;
        };

        let Some(track) = project.song.tracks.get(track_index) else {
            return;
        };

        out.push(RenderEvent::NoteOff {
            track_id: track.index,
            note,
        });
        let state = &mut self.track_state[track_index];
        state.active_note = None;
        state.note_steps_remaining = None;
    }

    fn resolve_step_data(&self, project: &ProjectData, track_index: usize) -> Option<StepPlaybackData> {
        let track = project.song.tracks.get(track_index)?;
        let state = self.track_state.get(track_index)?;

        let chain_id = track.song_rows.get(state.song_row).and_then(|slot| *slot)?;
        let chain = project.chains.get(&chain_id)?;
        let chain_row = chain.rows.get(state.chain_row)?;
        let phrase_id = chain_row.phrase_id?;
        let phrase = project.phrases.get(&phrase_id)?;
        let step = phrase.steps.get(state.phrase_step)?;

        let note = step.note.map(|raw_note| Self::apply_transpose(raw_note, chain_row.transpose))?;
        let note = self.apply_scale(project, track_index, note);
        let note_length_steps = self.resolve_note_length_steps(project, step.instrument_id);
        let synth_params = self.resolve_synth_params(project, step.instrument_id);

        Some(StepPlaybackData {
            track_id: track.index,
            note,
            velocity: step.velocity,
            instrument_id: step.instrument_id,
            note_length_steps,
            synth_params,
        })
    }

    fn resolve_note_length_steps(
        &self,
        project: &ProjectData,
        instrument_id: Option<InstrumentId>,
    ) -> u8 {
        instrument_id
            .and_then(|id| project.instruments.get(&id))
            .map(|inst| inst.note_length_steps.max(1))
            .unwrap_or(1)
    }

    fn resolve_synth_params(
        &self,
        project: &ProjectData,
        instrument_id: Option<InstrumentId>,
    ) -> SynthParams {
        instrument_id
            .and_then(|id| project.instruments.get(&id))
            .map(|inst| inst.synth_params)
            .unwrap_or_default()
    }

    fn apply_transpose(note: u8, transpose: i8) -> u8 {
        let value = note as i16 + transpose as i16;
        value.clamp(0, 127) as u8
    }

    fn apply_scale(&self, project: &ProjectData, track_index: usize, note: u8) -> u8 {
        let Some(scale) = self.effective_scale(project, track_index) else {
            return note;
        };

        Self::quantize_to_scale(note, scale)
    }

    fn effective_scale<'a>(&self, project: &'a ProjectData, track_index: usize) -> Option<&'a Scale> {
        let track = project.song.tracks.get(track_index)?;
        let scale_id = track.scale_override.unwrap_or(project.song.default_scale);
        project.scales.get(&scale_id)
    }

    fn quantize_to_scale(note: u8, scale: &Scale) -> u8 {
        if scale.interval_mask == 0 {
            return note;
        }

        let key = scale.key % 12;
        let is_allowed = |pitch_class: u8| -> bool {
            let interval = (12 + pitch_class as i16 - key as i16) % 12;
            ((scale.interval_mask >> interval) & 1) != 0
        };

        let base_pc = note % 12;
        if is_allowed(base_pc) {
            return note;
        }

        for distance in 1..=12 {
            if note >= distance {
                let down = note - distance;
                if is_allowed(down % 12) {
                    return down;
                }
            }

            if note + distance <= 127 {
                let up = note + distance;
                if is_allowed(up % 12) {
                    return up;
                }
            }
        }

        note
    }

    fn ticks_for_current_step(&self, project: &ProjectData, track_index: usize) -> u8 {
        let state = &self.track_state[track_index];
        let Some(track) = project.song.tracks.get(track_index) else {
            return self.ticks_per_step;
        };

        let Some(chain_id) = track.song_rows.get(state.song_row).and_then(|slot| *slot) else {
            return self.ticks_per_step;
        };

        let Some(chain) = project.chains.get(&chain_id) else {
            return self.ticks_per_step;
        };

        if chain.rows.get(state.chain_row).is_none() {
            return self.ticks_per_step;
        }

        let Some(groove) = self.effective_groove(project, track_index) else {
            return self.ticks_per_step;
        };

        if groove.ticks_pattern.is_empty() {
            return self.ticks_per_step;
        }

        let pattern_index = state.phrase_step % groove.ticks_pattern.len();
        let value = groove.ticks_pattern[pattern_index];
        if value == 0 {
            1
        } else {
            value
        }
    }

    fn effective_groove<'a>(
        &self,
        project: &'a ProjectData,
        track_index: usize,
    ) -> Option<&'a crate::model::Groove> {
        let track = project.song.tracks.get(track_index)?;
        let groove_id = track.groove_override.unwrap_or(project.song.default_groove);
        project.grooves.get(&groove_id)
    }

    fn advance_one_tick(&mut self, project: &ProjectData, track_index: usize) {
        let ticks_needed = self.ticks_for_current_step(project, track_index).max(1);
        let mut song_row = self.track_state[track_index].song_row;
        let mut chain_row = self.track_state[track_index].chain_row;
        let mut phrase_step = self.track_state[track_index].phrase_step;
        let mut tick_in_step = self.track_state[track_index].tick_in_step.saturating_add(1);

        if tick_in_step >= ticks_needed {
            tick_in_step = 0;
            phrase_step += 1;

            if phrase_step >= PHRASE_STEP_COUNT {
                phrase_step = 0;
                chain_row += 1;

                if !self.is_chain_row_playable(project, track_index, song_row, chain_row) {
                    chain_row = 0;
                    song_row = self.next_song_row_with_chain(project, track_index, song_row);
                }
            }
        }

        let state = &mut self.track_state[track_index];
        state.song_row = song_row;
        state.chain_row = chain_row;
        state.phrase_step = phrase_step;
        state.tick_in_step = tick_in_step;
    }

    fn is_chain_row_playable(
        &self,
        project: &ProjectData,
        track_index: usize,
        song_row: usize,
        chain_row: usize,
    ) -> bool {
        let Some(track) = project.song.tracks.get(track_index) else {
            return false;
        };

        let Some(chain_id) = track.song_rows.get(song_row).and_then(|slot| *slot) else {
            return false;
        };

        let Some(chain) = project.chains.get(&chain_id) else {
            return false;
        };

        let Some(row) = chain.rows.get(chain_row) else {
            return false;
        };

        let Some(phrase_id) = row.phrase_id else {
            return false;
        };

        project.phrases.contains_key(&phrase_id)
    }

    fn next_song_row_with_chain(
        &self,
        project: &ProjectData,
        track_index: usize,
        from_row: usize,
    ) -> usize {
        let Some(track) = project.song.tracks.get(track_index) else {
            return 0;
        };

        let valid_chain = |chain_id: ChainId| project.chains.contains_key(&chain_id);

        for row in (from_row + 1)..SONG_ROW_COUNT {
            if let Some(chain_id) = track.song_rows[row] {
                if valid_chain(chain_id) {
                    return row;
                }
            }
        }

        for row in 0..=from_row.min(SONG_ROW_COUNT.saturating_sub(1)) {
            if let Some(chain_id) = track.song_rows[row] {
                if valid_chain(chain_id) {
                    return row;
                }
            }
        }

        0
    }
}

#[cfg(test)]
mod tests {
    use super::Scheduler;
    use crate::engine::{Engine, EngineCommand};
    use crate::events::RenderEvent;
    use crate::model::{Chain, Groove, Instrument, InstrumentType, Phrase, Scale};

    fn setup_engine() -> Engine {
        let mut engine = Engine::new("test");

        let mut chain = Chain::new(0);
        chain.rows[0].phrase_id = Some(0);
        engine
            .apply_command(EngineCommand::UpsertChain { chain })
            .unwrap();

        let mut phrase = Phrase::new(0);
        phrase.steps[0].note = Some(60);
        phrase.steps[0].velocity = 100;
        phrase.steps[1].note = Some(61);
        phrase.steps[1].velocity = 90;
        engine
            .apply_command(EngineCommand::UpsertPhrase { phrase })
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
    fn emits_events_for_phrase_steps() {
        let engine = setup_engine();
        let mut scheduler = Scheduler::new(4); // 1 tick per step

        let first = scheduler.tick(&engine);
        let second = scheduler.tick(&engine);

        assert_eq!(first.len(), 1);
        assert_eq!(count_note_on(&second), 1);
        assert_eq!(count_note_off(&second), 1);
    }

    #[test]
    fn respects_track_mute() {
        let mut engine = setup_engine();
        engine
            .apply_command(EngineCommand::ToggleTrackMute { track_index: 0 })
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let events = scheduler.tick(&engine);

        assert!(events.is_empty());
    }

    #[test]
    fn groove_changes_step_timing() {
        let mut engine = setup_engine();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 1,
                note: None,
                velocity: 90,
                instrument_id: Some(0),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 2,
                note: Some(62),
                velocity: 90,
                instrument_id: Some(0),
            })
            .unwrap();

        let groove = Groove {
            id: 1,
            ticks_pattern: vec![1, 2, 1, 1],
        };
        engine
            .apply_command(EngineCommand::UpsertGroove { groove })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetDefaultGroove(1))
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let t1 = scheduler.tick(&engine);
        let t2 = scheduler.tick(&engine);
        let t3 = scheduler.tick(&engine);
        let t4 = scheduler.tick(&engine);

        assert_eq!(t1.len(), 1);
        assert_eq!(count_note_on(&t2), 0);
        assert_eq!(count_note_off(&t2), 1);
        assert_eq!(t3.len(), 0);
        assert_eq!(count_note_on(&t4), 1);
    }

    #[test]
    fn track_groove_override_has_priority() {
        let mut engine = setup_engine();

        let default_groove = Groove {
            id: 10,
            ticks_pattern: vec![2, 2, 2, 2],
        };
        let fast_groove = Groove {
            id: 11,
            ticks_pattern: vec![1, 1, 1, 1],
        };
        engine
            .apply_command(EngineCommand::UpsertGroove {
                groove: default_groove,
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::UpsertGroove { groove: fast_groove })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetDefaultGroove(10))
            .unwrap();
        engine
            .apply_command(EngineCommand::SetTrackGrooveOverride {
                track_index: 0,
                groove_id: Some(11),
            })
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let t1 = scheduler.tick(&engine);
        let t2 = scheduler.tick(&engine);

        assert_eq!(t1.len(), 1);
        assert_eq!(count_note_on(&t2), 1);
        assert_eq!(count_note_off(&t2), 1);
    }

    #[test]
    fn scale_quantizes_out_of_scale_note() {
        let mut engine = setup_engine();

        let scale = Scale {
            id: 2,
            key: 0,
            interval_mask: major_scale_mask(),
        };
        engine
            .apply_command(EngineCommand::UpsertScale { scale })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetDefaultScale(2))
            .unwrap();

        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(61), // C#
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let events = scheduler.tick(&engine);

        match &events[0] {
            RenderEvent::NoteOn { note, .. } => assert_eq!(*note, 60),
            _ => panic!("expected note on"),
        }
    }

    #[test]
    fn track_scale_override_has_priority() {
        let mut engine = setup_engine();

        let major = Scale {
            id: 20,
            key: 0,
            interval_mask: major_scale_mask(),
        };
        let chromatic = Scale {
            id: 21,
            key: 0,
            interval_mask: 0x0FFF,
        };
        engine
            .apply_command(EngineCommand::UpsertScale { scale: major })
            .unwrap();
        engine
            .apply_command(EngineCommand::UpsertScale { scale: chromatic })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetDefaultScale(20))
            .unwrap();
        engine
            .apply_command(EngineCommand::SetTrackScaleOverride {
                track_index: 0,
                scale_id: Some(21),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(61), // C#
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let events = scheduler.tick(&engine);

        match &events[0] {
            RenderEvent::NoteOn { note, .. } => assert_eq!(*note, 61),
            _ => panic!("expected note on"),
        }
    }

    #[test]
    fn emits_note_off_after_one_step_by_default() {
        let mut engine = setup_engine();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 1,
                note: None,
                velocity: 90,
                instrument_id: None,
            })
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let t1 = scheduler.tick(&engine);
        let t2 = scheduler.tick(&engine);

        assert_eq!(count_note_on(&t1), 1);
        assert_eq!(count_note_off(&t1), 0);
        assert_eq!(count_note_off(&t2), 1);
    }

    #[test]
    fn respects_instrument_note_length_steps() {
        let mut engine = setup_engine();
        let mut instrument = Instrument::new(0, InstrumentType::Synth, "Long");
        instrument.note_length_steps = 3;
        engine
            .apply_command(EngineCommand::UpsertInstrument { instrument })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(60),
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 1,
                note: None,
                velocity: 90,
                instrument_id: Some(0),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 2,
                note: None,
                velocity: 90,
                instrument_id: Some(0),
            })
            .unwrap();
        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 3,
                note: None,
                velocity: 90,
                instrument_id: Some(0),
            })
            .unwrap();

        let mut scheduler = Scheduler::new(4);
        let t1 = scheduler.tick(&engine);
        let t2 = scheduler.tick(&engine);
        let t3 = scheduler.tick(&engine);
        let t4 = scheduler.tick(&engine);

        assert_eq!(count_note_on(&t1), 1);
        assert_eq!(count_note_off(&t2), 0);
        assert_eq!(count_note_off(&t3), 0);
        assert_eq!(count_note_off(&t4), 1);
    }

    fn count_note_on(events: &[RenderEvent]) -> usize {
        events
            .iter()
            .filter(|event| matches!(event, RenderEvent::NoteOn { .. }))
            .count()
    }

    fn count_note_off(events: &[RenderEvent]) -> usize {
        events
            .iter()
            .filter(|event| matches!(event, RenderEvent::NoteOff { .. }))
            .count()
    }

    fn major_scale_mask() -> u16 {
        let intervals = [0u16, 2, 4, 5, 7, 9, 11];
        let mut mask = 0u16;
        for i in intervals {
            mask |= 1 << i;
        }
        mask
    }
}
