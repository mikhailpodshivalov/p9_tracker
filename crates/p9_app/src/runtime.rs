use std::collections::VecDeque;

use p9_core::engine::Engine;
use p9_core::scheduler::Scheduler;
use p9_rt::audio::AudioBackend;
use p9_rt::midi::{
    decode_message, forward_render_events, DecodedMidi, MidiInput, MidiMessage, MidiOutput,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeCommand {
    Start,
    Stop,
    Continue,
    Rewind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickReport {
    pub events_emitted: usize,
    pub midi_messages_sent: usize,
    pub tick: u64,
    pub is_playing: bool,
    pub audio_backend: &'static str,
    pub audio_callbacks_total: u64,
    pub audio_xruns_total: u64,
    pub audio_last_callback_us: u32,
    pub audio_avg_callback_us: u32,
    pub audio_buffer_size_frames: u32,
    pub audio_sample_rate_hz: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportSnapshot {
    pub tick: u64,
    pub is_playing: bool,
    pub queued_commands: usize,
    pub processed_commands: u64,
}

pub struct RuntimeCoordinator {
    scheduler: Scheduler,
    command_queue: VecDeque<RuntimeCommand>,
    processed_commands: u64,
}

impl RuntimeCoordinator {
    pub fn new(ppq: u16) -> Self {
        Self {
            scheduler: Scheduler::new(ppq),
            command_queue: VecDeque::new(),
            processed_commands: 0,
        }
    }

    pub fn enqueue_command(&mut self, command: RuntimeCommand) {
        self.command_queue.push_back(command);
    }

    pub fn enqueue_commands<I>(&mut self, commands: I)
    where
        I: IntoIterator<Item = RuntimeCommand>,
    {
        self.command_queue.extend(commands);
    }

    pub fn enqueue_midi_messages<I>(&mut self, messages: I) -> usize
    where
        I: IntoIterator<Item = MidiMessage>,
    {
        let mut mapped = 0usize;

        for message in messages {
            let command = match decode_message(message) {
                DecodedMidi::Start => Some(RuntimeCommand::Start),
                DecodedMidi::Continue => Some(RuntimeCommand::Continue),
                DecodedMidi::Stop => Some(RuntimeCommand::Stop),
                _ => None,
            };

            if let Some(command) = command {
                self.enqueue_command(command);
                mapped = mapped.saturating_add(1);
            }
        }

        mapped
    }

    pub fn ingest_midi_input(&mut self, input: &mut dyn MidiInput) -> usize {
        self.enqueue_midi_messages(input.poll())
    }

    pub fn run_tick(
        &mut self,
        engine: &Engine,
        audio: &mut dyn AudioBackend,
        midi_output: &mut dyn MidiOutput,
    ) -> TickReport {
        self.apply_queued_commands();

        let events = self.scheduler.tick(engine);
        audio.push_events(&events);
        let midi_messages_sent = forward_render_events(&events, midi_output);
        let audio_metrics = audio.metrics();

        TickReport {
            events_emitted: events.len(),
            midi_messages_sent,
            tick: self.scheduler.current_tick,
            is_playing: self.scheduler.is_playing,
            audio_backend: audio.backend_name(),
            audio_callbacks_total: audio_metrics.callbacks_total,
            audio_xruns_total: audio_metrics.xruns_total,
            audio_last_callback_us: audio_metrics.last_callback_us,
            audio_avg_callback_us: audio_metrics.avg_callback_us,
            audio_buffer_size_frames: audio_metrics.buffer_size_frames,
            audio_sample_rate_hz: audio_metrics.sample_rate_hz,
        }
    }

    pub fn snapshot(&self) -> TransportSnapshot {
        TransportSnapshot {
            tick: self.scheduler.current_tick,
            is_playing: self.scheduler.is_playing,
            queued_commands: self.command_queue.len(),
            processed_commands: self.processed_commands,
        }
    }

    fn apply_queued_commands(&mut self) {
        while let Some(command) = self.command_queue.pop_front() {
            self.apply_command(command);
            self.processed_commands = self.processed_commands.saturating_add(1);
        }
    }

    fn apply_command(&mut self, command: RuntimeCommand) {
        match command {
            RuntimeCommand::Start => self.scheduler.start(),
            RuntimeCommand::Stop => self.scheduler.stop(),
            RuntimeCommand::Continue => self.scheduler.start(),
            RuntimeCommand::Rewind => self.scheduler.rewind(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeCommand, RuntimeCoordinator};
    use p9_core::engine::{Engine, EngineCommand};
    use p9_core::model::{Chain, Phrase};
    use p9_rt::audio::{AudioBackend, AudioBackendConfig, NativeAudioBackend, NoopAudioBackend};
    use p9_rt::midi::{MidiMessage, NoopMidiOutput};

    fn setup_engine() -> Engine {
        let mut engine = Engine::new("runtime-test");

        let mut chain = Chain::new(0);
        chain.rows[0].phrase_id = Some(0);
        engine
            .apply_command(EngineCommand::UpsertChain { chain })
            .unwrap();

        let mut phrase = Phrase::new(0);
        phrase.steps[0].note = Some(60);
        phrase.steps[0].velocity = 100;
        phrase.steps[1].note = Some(62);
        phrase.steps[1].velocity = 100;
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

    fn started_audio() -> NoopAudioBackend {
        let mut audio = NoopAudioBackend::default();
        audio.start();
        audio
    }

    #[test]
    fn command_burst_is_applied_before_tick() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = started_audio();
        let mut midi_out = NoopMidiOutput::default();

        runtime.enqueue_commands([
            RuntimeCommand::Stop,
            RuntimeCommand::Start,
            RuntimeCommand::Stop,
        ]);

        let report = runtime.run_tick(&engine, &mut audio, &mut midi_out);
        let snapshot = runtime.snapshot();

        assert_eq!(report.events_emitted, 0);
        assert!(!report.is_playing);
        assert_eq!(snapshot.tick, 0);
        assert!(!snapshot.is_playing);
        assert_eq!(snapshot.queued_commands, 0);
        assert_eq!(snapshot.processed_commands, 3);
    }

    #[test]
    fn rewind_after_stop_resets_transport_to_zero() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = started_audio();
        let mut midi_out = NoopMidiOutput::default();

        let first = runtime.run_tick(&engine, &mut audio, &mut midi_out);
        assert_eq!(first.tick, 1);

        runtime.enqueue_commands([RuntimeCommand::Stop, RuntimeCommand::Rewind]);
        let second = runtime.run_tick(&engine, &mut audio, &mut midi_out);
        let snapshot = runtime.snapshot();

        assert_eq!(second.events_emitted, 0);
        assert_eq!(second.tick, 0);
        assert!(!second.is_playing);
        assert_eq!(snapshot.tick, 0);
    }

    #[test]
    fn midi_transport_messages_map_to_runtime_commands() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = started_audio();
        let mut midi_out = NoopMidiOutput::default();

        runtime.enqueue_command(RuntimeCommand::Stop);
        runtime.run_tick(&engine, &mut audio, &mut midi_out);
        assert!(!runtime.snapshot().is_playing);

        let mapped_start = runtime.enqueue_midi_messages([MidiMessage {
            status: 0xFA,
            data1: 0,
            data2: 0,
        }]);
        assert_eq!(mapped_start, 1);

        runtime.run_tick(&engine, &mut audio, &mut midi_out);
        assert!(runtime.snapshot().is_playing);

        let mapped_stop = runtime.enqueue_midi_messages([MidiMessage {
            status: 0xFC,
            data1: 0,
            data2: 0,
        }]);
        assert_eq!(mapped_stop, 1);

        runtime.run_tick(&engine, &mut audio, &mut midi_out);
        assert!(!runtime.snapshot().is_playing);
    }

    #[test]
    fn repeated_command_sequences_are_deterministic() {
        fn run_sequence() -> Vec<(usize, bool, u64)> {
            let engine = setup_engine();
            let mut runtime = RuntimeCoordinator::new(4);
            let mut audio = started_audio();
            let mut midi_out = NoopMidiOutput::default();
            let mut trace = Vec::new();

            runtime.enqueue_commands([
                RuntimeCommand::Stop,
                RuntimeCommand::Start,
                RuntimeCommand::Continue,
            ]);

            for _ in 0..4 {
                let report = runtime.run_tick(&engine, &mut audio, &mut midi_out);
                trace.push((report.events_emitted, report.is_playing, report.tick));
            }

            trace
        }

        assert_eq!(run_sequence(), run_sequence());
    }

    #[test]
    fn tick_report_exposes_audio_metrics() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = NativeAudioBackend::new(AudioBackendConfig {
            max_callback_us: 150,
            base_callback_us: 200,
            per_event_us: 0,
            ..AudioBackendConfig::default()
        });
        audio.start_checked().unwrap();
        let mut midi_out = NoopMidiOutput::default();

        let report = runtime.run_tick(&engine, &mut audio, &mut midi_out);

        assert_eq!(report.audio_backend, "native-simulated-linux");
        assert_eq!(report.audio_callbacks_total, 1);
        assert_eq!(report.audio_xruns_total, 1);
        assert_eq!(report.audio_last_callback_us, 200);
        assert_eq!(report.audio_sample_rate_hz, 48_000);
        assert_eq!(report.audio_buffer_size_frames, 256);
    }
}
