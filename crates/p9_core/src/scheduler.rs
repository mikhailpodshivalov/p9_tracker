use crate::engine::Engine;
use crate::events::RenderEvent;

pub struct Scheduler {
    pub ppq: u16,
    pub current_tick: u64,
}

impl Scheduler {
    pub fn new(ppq: u16) -> Self {
        Self {
            ppq,
            current_tick: 0,
        }
    }

    pub fn tick(&mut self, engine: &Engine) -> Vec<RenderEvent> {
        let events = self.collect_row_zero_events(engine);
        self.current_tick = self.current_tick.saturating_add(1);
        events
    }

    fn collect_row_zero_events(&self, engine: &Engine) -> Vec<RenderEvent> {
        let mut out = Vec::new();
        let project = engine.snapshot();

        for track in &project.song.tracks {
            if track.mute {
                continue;
            }

            let Some(chain_id) = track.song_rows.first().and_then(|slot| *slot) else {
                continue;
            };

            let Some(chain) = project.chains.get(&chain_id) else {
                continue;
            };

            let Some(chain_row) = chain.rows.first() else {
                continue;
            };

            let Some(phrase_id) = chain_row.phrase_id else {
                continue;
            };

            let Some(phrase) = project.phrases.get(&phrase_id) else {
                continue;
            };

            let Some(step) = phrase.steps.first() else {
                continue;
            };

            if let Some(note) = step.note {
                out.push(RenderEvent::NoteOn {
                    track_id: track.index,
                    note,
                    velocity: step.velocity,
                    instrument_id: step.instrument_id,
                });
            }
        }

        out
    }
}
