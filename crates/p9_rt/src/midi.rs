#[derive(Clone, Copy, Debug)]
pub struct MidiMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

pub trait MidiInput {
    fn poll(&mut self) -> Vec<MidiMessage>;
}

pub trait MidiOutput {
    fn send(&mut self, msg: MidiMessage);
}

#[derive(Default)]
pub struct NoopMidiInput;

impl MidiInput for NoopMidiInput {
    fn poll(&mut self) -> Vec<MidiMessage> {
        Vec::new()
    }
}

#[derive(Default)]
pub struct NoopMidiOutput {
    sent_count: usize,
}

impl MidiOutput for NoopMidiOutput {
    fn send(&mut self, _msg: MidiMessage) {
        self.sent_count = self.sent_count.saturating_add(1);
    }
}

impl NoopMidiOutput {
    pub fn sent_count(&self) -> usize {
        self.sent_count
    }
}
