use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};

use p9_core::engine::Engine;
use p9_core::scheduler::Scheduler;
use p9_rt::audio::AudioBackend;
use p9_rt::midi::{
    decode_message, forward_render_events, DecodedMidi, MidiInput, MidiMessage, MidiOutput,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMode {
    Internal,
    ExternalClock,
}

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
    pub midi_clock_messages_sent: usize,
    pub midi_messages_ingested: u64,
    pub tick: u64,
    pub is_playing: bool,
    pub sync_mode: SyncMode,
    pub external_clock_pending: u32,
    pub audio_backend: &'static str,
    pub audio_callbacks_total: u64,
    pub audio_xruns_total: u64,
    pub audio_last_callback_us: u32,
    pub audio_avg_callback_us: u32,
    pub audio_buffer_size_frames: u32,
    pub audio_sample_rate_hz: u32,
    pub audio_active_voices: u32,
    pub audio_max_voices: u32,
    pub audio_voices_stolen_total: u64,
    pub audio_voice_note_on_total: u64,
    pub audio_voice_note_off_total: u64,
    pub audio_voice_note_off_miss_total: u64,
    pub audio_voice_retrigger_total: u64,
    pub audio_voice_zero_attack_total: u64,
    pub audio_voice_short_release_total: u64,
    pub audio_click_risk_total: u64,
    pub audio_voice_release_deferred_total: u64,
    pub audio_voice_release_completed_total: u64,
    pub audio_voice_release_pending_voices: u32,
    pub audio_voice_steal_releasing_total: u64,
    pub audio_voice_steal_active_total: u64,
    pub audio_voice_polyphony_pressure_total: u64,
    pub audio_voice_sampler_mode_note_on_total: u64,
    pub audio_voice_silent_note_on_total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportSnapshot {
    pub tick: u64,
    pub is_playing: bool,
    pub sync_mode: SyncMode,
    pub external_clock_pending: u32,
    pub queued_commands: usize,
    pub processed_commands: u64,
    pub midi_messages_ingested_total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeFault {
    TickPanic,
}

pub struct RuntimeCoordinator {
    scheduler: Scheduler,
    sync_mode: SyncMode,
    external_clock_pending: u32,
    command_queue: VecDeque<RuntimeCommand>,
    processed_commands: u64,
    midi_messages_ingested_total: u64,
}

impl RuntimeCoordinator {
    pub fn new(ppq: u16) -> Self {
        Self {
            scheduler: Scheduler::new(ppq),
            sync_mode: SyncMode::Internal,
            external_clock_pending: 0,
            command_queue: VecDeque::new(),
            processed_commands: 0,
            midi_messages_ingested_total: 0,
        }
    }

    pub fn set_sync_mode(&mut self, mode: SyncMode) {
        self.set_sync_mode_now(mode);
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
            self.midi_messages_ingested_total = self.midi_messages_ingested_total.saturating_add(1);

            match decode_message(message) {
                DecodedMidi::Start => {
                    self.enqueue_command(RuntimeCommand::Start);
                    mapped = mapped.saturating_add(1);
                }
                DecodedMidi::Continue => {
                    self.enqueue_command(RuntimeCommand::Continue);
                    mapped = mapped.saturating_add(1);
                }
                DecodedMidi::Stop => {
                    self.enqueue_command(RuntimeCommand::Stop);
                    mapped = mapped.saturating_add(1);
                }
                DecodedMidi::Clock if matches!(self.sync_mode, SyncMode::ExternalClock) => {
                    self.external_clock_pending = self.external_clock_pending.saturating_add(1);
                }
                _ => {}
            }
        }

        mapped
    }

    pub fn ingest_midi_input(&mut self, input: &mut dyn MidiInput) -> usize {
        self.enqueue_midi_messages(input.poll())
    }

    pub fn run_cycle(
        &mut self,
        engine: &Engine,
        audio: &mut dyn AudioBackend,
        midi_input: &mut dyn MidiInput,
        midi_output: &mut dyn MidiOutput,
    ) -> TickReport {
        let _ = self.ingest_midi_input(midi_input);
        self.run_tick(engine, audio, midi_output)
    }

    pub fn run_cycle_safe(
        &mut self,
        engine: &Engine,
        audio: &mut dyn AudioBackend,
        midi_input: &mut dyn MidiInput,
        midi_output: &mut dyn MidiOutput,
    ) -> Result<TickReport, RuntimeFault> {
        catch_unwind(AssertUnwindSafe(|| {
            self.run_cycle(engine, audio, midi_input, midi_output)
        }))
        .map_err(|_| RuntimeFault::TickPanic)
    }

    pub fn run_tick(
        &mut self,
        engine: &Engine,
        audio: &mut dyn AudioBackend,
        midi_output: &mut dyn MidiOutput,
    ) -> TickReport {
        self.apply_queued_commands();

        let events = if self.should_advance_tick() {
            self.scheduler.tick(engine)
        } else {
            Vec::new()
        };

        audio.push_events(&events);
        let mut midi_messages_sent = forward_render_events(&events, midi_output);

        let mut midi_clock_messages_sent = 0usize;
        if matches!(self.sync_mode, SyncMode::Internal) && self.scheduler.is_playing {
            midi_output.send(MidiMessage {
                status: 0xF8,
                data1: 0,
                data2: 0,
            });
            midi_messages_sent = midi_messages_sent.saturating_add(1);
            midi_clock_messages_sent = 1;
        }

        let audio_metrics = audio.metrics();

        TickReport {
            events_emitted: events.len(),
            midi_messages_sent,
            midi_clock_messages_sent,
            midi_messages_ingested: self.midi_messages_ingested_total,
            tick: self.scheduler.current_tick,
            is_playing: self.scheduler.is_playing,
            sync_mode: self.sync_mode,
            external_clock_pending: self.external_clock_pending,
            audio_backend: audio.backend_name(),
            audio_callbacks_total: audio_metrics.callbacks_total,
            audio_xruns_total: audio_metrics.xruns_total,
            audio_last_callback_us: audio_metrics.last_callback_us,
            audio_avg_callback_us: audio_metrics.avg_callback_us,
            audio_buffer_size_frames: audio_metrics.buffer_size_frames,
            audio_sample_rate_hz: audio_metrics.sample_rate_hz,
            audio_active_voices: audio_metrics.active_voices,
            audio_max_voices: audio_metrics.max_voices,
            audio_voices_stolen_total: audio_metrics.voices_stolen_total,
            audio_voice_note_on_total: audio_metrics.voice_note_on_total,
            audio_voice_note_off_total: audio_metrics.voice_note_off_total,
            audio_voice_note_off_miss_total: audio_metrics.voice_note_off_miss_total,
            audio_voice_retrigger_total: audio_metrics.voice_retrigger_total,
            audio_voice_zero_attack_total: audio_metrics.voice_zero_attack_total,
            audio_voice_short_release_total: audio_metrics.voice_short_release_total,
            audio_click_risk_total: audio_metrics.click_risk_total,
            audio_voice_release_deferred_total: audio_metrics.voice_release_deferred_total,
            audio_voice_release_completed_total: audio_metrics.voice_release_completed_total,
            audio_voice_release_pending_voices: audio_metrics.voice_release_pending_voices,
            audio_voice_steal_releasing_total: audio_metrics.voice_steal_releasing_total,
            audio_voice_steal_active_total: audio_metrics.voice_steal_active_total,
            audio_voice_polyphony_pressure_total: audio_metrics.voice_polyphony_pressure_total,
            audio_voice_sampler_mode_note_on_total: audio_metrics
                .voice_sampler_mode_note_on_total,
            audio_voice_silent_note_on_total: audio_metrics.voice_silent_note_on_total,
        }
    }

    pub fn run_tick_safe(
        &mut self,
        engine: &Engine,
        audio: &mut dyn AudioBackend,
        midi_output: &mut dyn MidiOutput,
    ) -> Result<TickReport, RuntimeFault> {
        catch_unwind(AssertUnwindSafe(|| self.run_tick(engine, audio, midi_output)))
            .map_err(|_| RuntimeFault::TickPanic)
    }

    pub fn snapshot(&self) -> TransportSnapshot {
        TransportSnapshot {
            tick: self.scheduler.current_tick,
            is_playing: self.scheduler.is_playing,
            sync_mode: self.sync_mode,
            external_clock_pending: self.external_clock_pending,
            queued_commands: self.command_queue.len(),
            processed_commands: self.processed_commands,
            midi_messages_ingested_total: self.midi_messages_ingested_total,
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

    fn set_sync_mode_now(&mut self, mode: SyncMode) {
        self.sync_mode = mode;

        if matches!(mode, SyncMode::Internal) {
            self.external_clock_pending = 0;
        }
    }

    fn should_advance_tick(&mut self) -> bool {
        if !self.scheduler.is_playing {
            return false;
        }

        match self.sync_mode {
            SyncMode::Internal => true,
            SyncMode::ExternalClock => {
                if self.external_clock_pending == 0 {
                    false
                } else {
                    self.external_clock_pending = self.external_clock_pending.saturating_sub(1);
                    true
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeCommand, RuntimeCoordinator, RuntimeFault, SyncMode};
    use p9_core::engine::{Engine, EngineCommand};
    use p9_core::events::RenderEvent;
    use p9_core::model::{Chain, Phrase};
    use p9_rt::audio::{
        AudioBackend, AudioBackendConfig, AudioMetrics, NativeAudioBackend, NoopAudioBackend,
    };
    use p9_rt::midi::{MidiInput, MidiMessage, MidiOutput, NoopMidiOutput};
    use std::collections::VecDeque;

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

    #[derive(Default)]
    struct CaptureMidiOutput {
        sent: Vec<MidiMessage>,
    }

    impl MidiOutput for CaptureMidiOutput {
        fn send(&mut self, msg: MidiMessage) {
            self.sent.push(msg);
        }
    }

    struct ScriptedMidiInput {
        queue: VecDeque<MidiMessage>,
    }

    impl ScriptedMidiInput {
        fn new(messages: Vec<MidiMessage>) -> Self {
            Self {
                queue: messages.into(),
            }
        }
    }

    impl MidiInput for ScriptedMidiInput {
        fn poll(&mut self) -> Vec<MidiMessage> {
            self.queue.pop_front().into_iter().collect()
        }
    }

    struct PanicAudioBackend;

    impl AudioBackend for PanicAudioBackend {
        fn start(&mut self) {}

        fn stop(&mut self) {}

        fn push_events(&mut self, _events: &[RenderEvent]) {
            panic!("panic-audio");
        }

        fn events_consumed(&self) -> usize {
            0
        }

        fn metrics(&self) -> AudioMetrics {
            AudioMetrics::default()
        }

        fn backend_name(&self) -> &'static str {
            "panic-audio"
        }
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
        fn run_sequence() -> Vec<(usize, bool, u64, SyncMode)> {
            let engine = setup_engine();
            let mut runtime = RuntimeCoordinator::new(4);
            let mut audio = started_audio();
            let mut midi_out = NoopMidiOutput::default();
            let mut trace = Vec::new();

            runtime.set_sync_mode(SyncMode::ExternalClock);
            runtime.enqueue_commands([
                RuntimeCommand::Stop,
                RuntimeCommand::Start,
            ]);
            runtime.enqueue_midi_messages([
                MidiMessage {
                    status: 0xF8,
                    data1: 0,
                    data2: 0,
                },
                MidiMessage {
                    status: 0xF8,
                    data1: 0,
                    data2: 0,
                },
                MidiMessage {
                    status: 0xF8,
                    data1: 0,
                    data2: 0,
                },
            ]);

            for _ in 0..4 {
                let report = runtime.run_tick(&engine, &mut audio, &mut midi_out);
                trace.push((
                    report.events_emitted,
                    report.is_playing,
                    report.tick,
                    report.sync_mode,
                ));
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
        assert_eq!(report.audio_max_voices, 16);
        assert_eq!(report.audio_voice_note_on_total, 1);
        assert_eq!(report.audio_voice_note_off_total, 0);
        assert_eq!(report.audio_voice_note_off_miss_total, 0);
        assert_eq!(report.audio_voice_retrigger_total, 0);
        assert_eq!(report.audio_voice_zero_attack_total, 0);
        assert_eq!(report.audio_voice_short_release_total, 0);
        assert_eq!(report.audio_click_risk_total, 0);
        assert_eq!(report.audio_voice_release_deferred_total, 0);
        assert_eq!(report.audio_voice_release_completed_total, 0);
        assert_eq!(report.audio_voice_release_pending_voices, 0);
        assert_eq!(report.audio_voice_steal_releasing_total, 0);
        assert_eq!(report.audio_voice_steal_active_total, 0);
        assert_eq!(report.audio_voice_polyphony_pressure_total, 0);
        assert_eq!(report.audio_voice_sampler_mode_note_on_total, 0);
        assert_eq!(report.audio_voice_silent_note_on_total, 0);
    }

    #[test]
    fn external_clock_mode_advances_only_on_clock_messages() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = started_audio();
        let mut midi_out = NoopMidiOutput::default();

        runtime.set_sync_mode(SyncMode::ExternalClock);
        runtime.enqueue_command(RuntimeCommand::Rewind);

        let first = runtime.run_tick(&engine, &mut audio, &mut midi_out);
        assert_eq!(first.sync_mode, SyncMode::ExternalClock);
        assert_eq!(first.tick, 0);
        assert_eq!(first.events_emitted, 0);

        runtime.enqueue_midi_messages([MidiMessage {
            status: 0xF8,
            data1: 0,
            data2: 0,
        }]);

        let second = runtime.run_tick(&engine, &mut audio, &mut midi_out);
        assert_eq!(second.tick, 1);
        assert_eq!(second.external_clock_pending, 0);
    }

    #[test]
    fn internal_sync_emits_clock_message_on_tick() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = started_audio();
        let mut midi_out = CaptureMidiOutput::default();

        let report = runtime.run_tick(&engine, &mut audio, &mut midi_out);

        assert_eq!(report.sync_mode, SyncMode::Internal);
        assert_eq!(report.midi_clock_messages_sent, 1);
        assert!(midi_out.sent.iter().any(|msg| msg.status == 0xF8));
    }

    #[test]
    fn run_cycle_polls_midi_input_continuously() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = started_audio();
        let mut midi_out = NoopMidiOutput::default();

        runtime.set_sync_mode(SyncMode::ExternalClock);

        let mut midi_input = ScriptedMidiInput::new(vec![
            MidiMessage {
                status: 0xF8,
                data1: 0,
                data2: 0,
            },
            MidiMessage {
                status: 0xF8,
                data1: 0,
                data2: 0,
            },
        ]);

        let first = runtime.run_cycle(&engine, &mut audio, &mut midi_input, &mut midi_out);
        let second = runtime.run_cycle(&engine, &mut audio, &mut midi_input, &mut midi_out);

        assert_eq!(first.tick, 1);
        assert_eq!(second.tick, 2);
        assert_eq!(second.midi_messages_ingested, 2);
    }

    #[test]
    fn run_tick_safe_catches_backend_panics() {
        let engine = setup_engine();
        let mut runtime = RuntimeCoordinator::new(4);
        let mut audio = PanicAudioBackend;
        let mut midi_out = NoopMidiOutput::default();

        let result = runtime.run_tick_safe(&engine, &mut audio, &mut midi_out);
        assert_eq!(result, Err(RuntimeFault::TickPanic));
    }
}
