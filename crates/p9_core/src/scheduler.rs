use crate::engine::Engine;
use crate::events::RenderEvent;
use crate::model::{
    ChainId, PHRASE_STEP_COUNT, ProjectData, SONG_ROW_COUNT, TRACK_COUNT,
};

#[derive(Clone, Debug, Default)]
pub struct TrackPlaybackState {
    pub song_row: usize,
    pub chain_row: usize,
    pub phrase_step: usize,
    pub tick_in_step: u8,
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
                self.advance_one_tick(project, track_index);
                continue;
            }

            self.ensure_playable_position(project, track_index);

            if self.track_state[track_index].tick_in_step == 0 {
                if let Some(event) = self.resolve_step_event(project, track_index) {
                    out.push(event);
                }
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

    fn resolve_step_event(&self, project: &ProjectData, track_index: usize) -> Option<RenderEvent> {
        let track = project.song.tracks.get(track_index)?;
        let state = self.track_state.get(track_index)?;

        let chain_id = track.song_rows.get(state.song_row).and_then(|slot| *slot)?;
        let chain = project.chains.get(&chain_id)?;
        let chain_row = chain.rows.get(state.chain_row)?;
        let phrase_id = chain_row.phrase_id?;
        let phrase = project.phrases.get(&phrase_id)?;
        let step = phrase.steps.get(state.phrase_step)?;

        let note = step.note.map(|raw_note| Self::apply_transpose(raw_note, chain_row.transpose))?;

        Some(RenderEvent::NoteOn {
            track_id: track.index,
            note,
            velocity: step.velocity,
            instrument_id: step.instrument_id,
        })
    }

    fn apply_transpose(note: u8, transpose: i8) -> u8 {
        let value = note as i16 + transpose as i16;
        value.clamp(0, 127) as u8
    }

    fn advance_one_tick(&mut self, project: &ProjectData, track_index: usize) {
        let ticks_per_step = self.ticks_per_step;
        let mut song_row = self.track_state[track_index].song_row;
        let mut chain_row = self.track_state[track_index].chain_row;
        let mut phrase_step = self.track_state[track_index].phrase_step;
        let mut tick_in_step = self.track_state[track_index].tick_in_step.saturating_add(1);

        if tick_in_step >= ticks_per_step {
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
    use crate::model::{Chain, Phrase};

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
        assert_eq!(second.len(), 1);
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
}
