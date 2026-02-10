use std::collections::VecDeque;

use p9_core::events::RenderEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiMessage {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodedMidi {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    Start,
    Stop,
    Continue,
    Clock,
    Unknown,
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

#[derive(Default)]
pub struct BufferedMidiInput {
    queue: VecDeque<MidiMessage>,
}

impl BufferedMidiInput {
    pub fn push_message(&mut self, message: MidiMessage) {
        self.queue.push_back(message);
    }

    pub fn push_messages<I>(&mut self, messages: I)
    where
        I: IntoIterator<Item = MidiMessage>,
    {
        self.queue.extend(messages);
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

impl MidiInput for BufferedMidiInput {
    fn poll(&mut self) -> Vec<MidiMessage> {
        self.queue.drain(..).collect()
    }
}

#[derive(Default)]
pub struct BufferedMidiOutput {
    sent: Vec<MidiMessage>,
}

impl MidiOutput for BufferedMidiOutput {
    fn send(&mut self, msg: MidiMessage) {
        self.sent.push(msg);
    }
}

impl BufferedMidiOutput {
    pub fn sent_messages(&self) -> &[MidiMessage] {
        &self.sent
    }

    pub fn sent_count(&self) -> usize {
        self.sent.len()
    }

    pub fn take_all(&mut self) -> Vec<MidiMessage> {
        std::mem::take(&mut self.sent)
    }
}

pub fn decode_message(msg: MidiMessage) -> DecodedMidi {
    let upper_status = msg.status & 0xF0;
    let channel = msg.status & 0x0F;

    match upper_status {
        0x90 if msg.data2 == 0 => DecodedMidi::NoteOff {
            channel,
            note: msg.data1,
            velocity: msg.data2,
        },
        0x90 => DecodedMidi::NoteOn {
            channel,
            note: msg.data1,
            velocity: msg.data2,
        },
        0x80 => DecodedMidi::NoteOff {
            channel,
            note: msg.data1,
            velocity: msg.data2,
        },
        _ => match msg.status {
            0xFA => DecodedMidi::Start,
            0xFC => DecodedMidi::Stop,
            0xFB => DecodedMidi::Continue,
            0xF8 => DecodedMidi::Clock,
            _ => DecodedMidi::Unknown,
        },
    }
}

pub fn render_event_to_midi(event: &RenderEvent) -> MidiMessage {
    match event {
        RenderEvent::NoteOn {
            track_id,
            note,
            velocity,
            ..
        } => MidiMessage {
            status: 0x90 | (track_id & 0x0F),
            data1: *note,
            data2: *velocity,
        },
        RenderEvent::NoteOff { track_id, note } => MidiMessage {
            status: 0x80 | (track_id & 0x0F),
            data1: *note,
            data2: 0,
        },
    }
}

pub fn forward_render_events(events: &[RenderEvent], output: &mut dyn MidiOutput) -> usize {
    let mut sent = 0usize;
    for event in events {
        output.send(render_event_to_midi(event));
        sent = sent.saturating_add(1);
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::{
        decode_message, forward_render_events, render_event_to_midi, BufferedMidiInput,
        BufferedMidiOutput, DecodedMidi, MidiInput, MidiMessage, MidiOutput, NoopMidiOutput,
    };
    use p9_core::events::{RenderEvent, RenderMode};
    use p9_core::model::SynthWaveform;

    fn note_on(track_id: u8, note: u8, velocity: u8) -> RenderEvent {
        RenderEvent::NoteOn {
            track_id,
            note,
            velocity,
            render_mode: RenderMode::Synth,
            instrument_id: Some(0),
            waveform: SynthWaveform::Saw,
            attack_ms: 5,
            release_ms: 80,
            gain: 100,
            sampler_variant: p9_core::model::SamplerRenderVariant::Classic,
            sampler_transient_level: 64,
            sampler_body_level: 96,
        }
    }

    #[test]
    fn decode_note_messages() {
        let note_on = decode_message(MidiMessage {
            status: 0x93,
            data1: 64,
            data2: 100,
        });
        assert_eq!(
            note_on,
            DecodedMidi::NoteOn {
                channel: 3,
                note: 64,
                velocity: 100
            }
        );

        let note_off = decode_message(MidiMessage {
            status: 0x83,
            data1: 64,
            data2: 0,
        });
        assert_eq!(
            note_off,
            DecodedMidi::NoteOff {
                channel: 3,
                note: 64,
                velocity: 0
            }
        );
    }

    #[test]
    fn decode_transport_messages() {
        assert_eq!(
            decode_message(MidiMessage {
                status: 0xFA,
                data1: 0,
                data2: 0
            }),
            DecodedMidi::Start
        );
        assert_eq!(
            decode_message(MidiMessage {
                status: 0xFC,
                data1: 0,
                data2: 0
            }),
            DecodedMidi::Stop
        );
    }

    #[test]
    fn render_event_maps_track_to_channel() {
        let msg = render_event_to_midi(&note_on(19, 72, 90));

        assert_eq!(
            msg,
            MidiMessage {
                status: 0x93,
                data1: 72,
                data2: 90
            }
        );
    }

    #[test]
    fn forward_render_events_sends_all_messages() {
        let events = vec![
            note_on(0, 60, 100),
            RenderEvent::NoteOff {
                track_id: 0,
                note: 60,
            },
        ];

        let mut output = NoopMidiOutput::default();
        let sent = forward_render_events(&events, &mut output);
        assert_eq!(sent, 2);
        assert_eq!(output.sent_count(), 2);
    }

    #[test]
    fn noop_output_counts_direct_send() {
        let mut output = NoopMidiOutput::default();
        output.send(MidiMessage {
            status: 0x90,
            data1: 60,
            data2: 100,
        });
        assert_eq!(output.sent_count(), 1);
    }

    #[test]
    fn buffered_input_drains_messages_in_poll() {
        let mut input = BufferedMidiInput::default();
        input.push_messages([
            MidiMessage {
                status: 0xFA,
                data1: 0,
                data2: 0,
            },
            MidiMessage {
                status: 0xF8,
                data1: 0,
                data2: 0,
            },
        ]);
        assert_eq!(input.pending(), 2);

        let polled = input.poll();
        assert_eq!(polled.len(), 2);
        assert_eq!(input.pending(), 0);
    }

    #[test]
    fn buffered_output_records_messages() {
        let mut output = BufferedMidiOutput::default();
        output.send(MidiMessage {
            status: 0x90,
            data1: 64,
            data2: 100,
        });
        output.send(MidiMessage {
            status: 0x80,
            data1: 64,
            data2: 0,
        });

        assert_eq!(output.sent_count(), 2);
        assert_eq!(output.sent_messages()[0].status, 0x90);
        assert_eq!(output.take_all().len(), 2);
        assert_eq!(output.sent_count(), 0);
    }
}
