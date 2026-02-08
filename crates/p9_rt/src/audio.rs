use p9_core::events::RenderEvent;

pub trait AudioBackend {
    fn start(&mut self);
    fn stop(&mut self);
    fn push_events(&mut self, events: &[RenderEvent]);
    fn events_consumed(&self) -> usize;
}

#[derive(Default)]
pub struct NoopAudioBackend {
    running: bool,
    events_total: usize,
}

impl AudioBackend for NoopAudioBackend {
    fn start(&mut self) {
        self.running = true;
    }

    fn stop(&mut self) {
        self.running = false;
    }

    fn push_events(&mut self, events: &[RenderEvent]) {
        if self.running {
            self.events_total = self.events_total.saturating_add(events.len());
        }
    }

    fn events_consumed(&self) -> usize {
        self.events_total
    }
}
