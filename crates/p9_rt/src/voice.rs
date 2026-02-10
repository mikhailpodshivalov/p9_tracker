use p9_core::model::{InstrumentId, SynthWaveform};

const ZERO_ATTACK_THRESHOLD_MS: u16 = 1;
const SHORT_RELEASE_THRESHOLD_MS: u16 = 2;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceLifecycleStats {
    pub note_on_total: u64,
    pub note_off_total: u64,
    pub note_off_miss_total: u64,
    pub retrigger_total: u64,
    pub zero_attack_total: u64,
    pub short_release_total: u64,
    pub click_risk_total: u64,
}

pub struct VoiceAllocator {
    max_voices: usize,
    slots: Vec<Option<Voice>>,
    activation_counter: u64,
    voices_stolen_total: u64,
    note_on_total: u64,
    note_off_total: u64,
    note_off_miss_total: u64,
    retrigger_total: u64,
    zero_attack_total: u64,
    short_release_total: u64,
    click_risk_total: u64,
}

impl VoiceAllocator {
    pub fn new(max_voices: usize) -> Self {
        let bounded = max_voices.max(1);
        Self {
            max_voices: bounded,
            slots: vec![None; bounded],
            activation_counter: 0,
            voices_stolen_total: 0,
            note_on_total: 0,
            note_off_total: 0,
            note_off_miss_total: 0,
            retrigger_total: 0,
            zero_attack_total: 0,
            short_release_total: 0,
            click_risk_total: 0,
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
        self.note_on_total = self.note_on_total.saturating_add(1);
        if attack_ms <= ZERO_ATTACK_THRESHOLD_MS {
            self.zero_attack_total = self.zero_attack_total.saturating_add(1);
            self.click_risk_total = self.click_risk_total.saturating_add(1);
        }

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
            self.retrigger_total = self.retrigger_total.saturating_add(1);
            self.click_risk_total = self.click_risk_total.saturating_add(1);
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
        self.click_risk_total = self.click_risk_total.saturating_add(1);
    }

    pub fn note_off(&mut self, track_id: u8, note: u8) -> bool {
        self.note_off_total = self.note_off_total.saturating_add(1);
        let Some(index) = self.find_voice_slot(track_id, note) else {
            self.note_off_miss_total = self.note_off_miss_total.saturating_add(1);
            return false;
        };
        if self.slots[index]
            .map(|voice| voice.release_ms <= SHORT_RELEASE_THRESHOLD_MS)
            .unwrap_or(false)
        {
            self.short_release_total = self.short_release_total.saturating_add(1);
            self.click_risk_total = self.click_risk_total.saturating_add(1);
        }
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

    pub fn lifecycle_stats(&self) -> VoiceLifecycleStats {
        VoiceLifecycleStats {
            note_on_total: self.note_on_total,
            note_off_total: self.note_off_total,
            note_off_miss_total: self.note_off_miss_total,
            retrigger_total: self.retrigger_total,
            zero_attack_total: self.zero_attack_total,
            short_release_total: self.short_release_total,
            click_risk_total: self.click_risk_total,
        }
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

    #[test]
    fn lifecycle_counters_capture_click_risk_signals() {
        let mut allocator = VoiceAllocator::new(2);

        allocator.note_on(0, 60, 100, Some(0), SynthWaveform::Saw, 0, 80, 90);
        allocator.note_on(0, 60, 100, Some(0), SynthWaveform::Saw, 5, 80, 90);
        allocator.note_on(0, 62, 100, Some(0), SynthWaveform::Saw, 5, 80, 90);
        allocator.note_on(0, 63, 100, Some(0), SynthWaveform::Saw, 5, 1, 90);

        assert!(!allocator.note_off(0, 60));
        assert!(allocator.note_off(0, 63));
        assert!(!allocator.note_off(0, 99));

        let stats = allocator.lifecycle_stats();
        assert_eq!(stats.note_on_total, 4);
        assert_eq!(stats.note_off_total, 3);
        assert_eq!(stats.note_off_miss_total, 2);
        assert_eq!(stats.retrigger_total, 1);
        assert_eq!(stats.zero_attack_total, 1);
        assert_eq!(stats.short_release_total, 1);
        assert_eq!(stats.click_risk_total, 4);
        assert_eq!(allocator.voices_stolen_total(), 1);
    }
}
