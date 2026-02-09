use p9_core::model::{InstrumentId, SynthWaveform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    pub track_id: u8,
    pub note: u8,
    pub velocity: u8,
    pub instrument_id: Option<InstrumentId>,
    pub waveform: SynthWaveform,
    pub attack_ms: u16,
    pub release_ms: u16,
    pub gain: u8,
    pub started_at: u64,
}

pub struct VoiceAllocator {
    max_voices: usize,
    slots: Vec<Option<Voice>>,
    activation_counter: u64,
    voices_stolen_total: u64,
}

impl VoiceAllocator {
    pub fn new(max_voices: usize) -> Self {
        let bounded = max_voices.max(1);
        Self {
            max_voices: bounded,
            slots: vec![None; bounded],
            activation_counter: 0,
            voices_stolen_total: 0,
        }
    }

    pub fn note_on(
        &mut self,
        track_id: u8,
        note: u8,
        velocity: u8,
        instrument_id: Option<InstrumentId>,
        waveform: SynthWaveform,
        attack_ms: u16,
        release_ms: u16,
        gain: u8,
    ) {
        self.activation_counter = self.activation_counter.saturating_add(1);
        let voice = Voice {
            track_id,
            note,
            velocity,
            instrument_id,
            waveform,
            attack_ms,
            release_ms,
            gain,
            started_at: self.activation_counter,
        };

        if let Some(index) = self.find_voice_slot(track_id, note) {
            self.slots[index] = Some(voice);
            return;
        }

        if let Some(index) = self.free_slot_index() {
            self.slots[index] = Some(voice);
            return;
        }

        let oldest = self.oldest_voice_index();
        self.slots[oldest] = Some(voice);
        self.voices_stolen_total = self.voices_stolen_total.saturating_add(1);
    }

    pub fn note_off(&mut self, track_id: u8, note: u8) -> bool {
        let Some(index) = self.find_voice_slot(track_id, note) else {
            return false;
        };
        self.slots[index] = None;
        true
    }

    pub fn active_voice_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    pub fn voices_stolen_total(&self) -> u64 {
        self.voices_stolen_total
    }

    fn find_voice_slot(&self, track_id: u8, note: u8) -> Option<usize> {
        self.slots.iter().position(|slot| {
            slot.map(|voice| voice.track_id == track_id && voice.note == note)
                .unwrap_or(false)
        })
    }

    fn free_slot_index(&self) -> Option<usize> {
        self.slots.iter().position(|slot| slot.is_none())
    }

    fn oldest_voice_index(&self) -> usize {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.map(|voice| (index, voice.started_at)))
            .min_by_key(|(_, started_at)| *started_at)
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::VoiceAllocator;
    use p9_core::model::SynthWaveform;

    #[test]
    fn note_on_then_note_off_clears_active_voice() {
        let mut allocator = VoiceAllocator::new(4);

        allocator.note_on(0, 60, 100, Some(0), SynthWaveform::Saw, 5, 80, 90);
        assert_eq!(allocator.active_voice_count(), 1);

        assert!(allocator.note_off(0, 60));
        assert_eq!(allocator.active_voice_count(), 0);
    }

    #[test]
    fn allocator_stays_bounded_and_steals_oldest() {
        let mut allocator = VoiceAllocator::new(2);

        allocator.note_on(0, 60, 100, Some(0), SynthWaveform::Saw, 5, 80, 90);
        allocator.note_on(0, 62, 100, Some(0), SynthWaveform::Saw, 5, 80, 90);
        allocator.note_on(0, 64, 100, Some(0), SynthWaveform::Saw, 5, 80, 90);

        assert_eq!(allocator.active_voice_count(), 2);
        assert_eq!(allocator.max_voices(), 2);
        assert_eq!(allocator.voices_stolen_total(), 1);
        assert!(!allocator.note_off(0, 60)); // oldest was stolen
        assert!(allocator.note_off(0, 62) || allocator.note_off(0, 64));
    }

    #[test]
    fn retrigger_same_note_reuses_existing_slot() {
        let mut allocator = VoiceAllocator::new(2);

        allocator.note_on(0, 60, 90, Some(0), SynthWaveform::Sine, 1, 20, 80);
        allocator.note_on(0, 60, 120, Some(0), SynthWaveform::Square, 2, 30, 100);

        assert_eq!(allocator.active_voice_count(), 1);
        assert_eq!(allocator.voices_stolen_total(), 0);
    }
}
