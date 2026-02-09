use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{Chain, Groove, Instrument, InstrumentType, Phrase, Scale};
use p9_core::scheduler::Scheduler;
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
use p9_rt::midi::{
    decode_message, forward_render_events, DecodedMidi, MidiInput, NoopMidiInput, NoopMidiOutput,
};
use p9_storage::project::ProjectEnvelope;

fn main() {
    let mut engine = Engine::new("p9_tracker song");
    let _ = engine.apply_command(EngineCommand::SetTempo(128));

    let chain = Chain::new(0);
    let _ = engine.apply_command(EngineCommand::UpsertChain { chain });

    let phrase = Phrase::new(0);
    let _ = engine.apply_command(EngineCommand::UpsertPhrase { phrase });

    let instrument = Instrument::new(0, InstrumentType::Synth, "Init Synth");
    let _ = engine.apply_command(EngineCommand::UpsertInstrument { instrument });

    let groove = Groove {
        id: 1,
        ticks_pattern: vec![6, 6, 3, 9],
    };
    let _ = engine.apply_command(EngineCommand::UpsertGroove { groove });
    let _ = engine.apply_command(EngineCommand::SetDefaultGroove(1));
    let _ = engine.apply_command(EngineCommand::SetTrackGrooveOverride {
        track_index: 0,
        groove_id: Some(1),
    });

    let scale = Scale {
        id: 1,
        key: 0,
        interval_mask: major_scale_mask(),
    };
    let _ = engine.apply_command(EngineCommand::UpsertScale { scale });
    let _ = engine.apply_command(EngineCommand::SetDefaultScale(1));
    let _ = engine.apply_command(EngineCommand::SetTrackScaleOverride {
        track_index: 0,
        scale_id: Some(1),
    });

    let _ = engine.apply_command(EngineCommand::SetChainRowPhrase {
        chain_id: 0,
        row: 0,
        phrase_id: Some(0),
        transpose: 0,
    });

    let _ = engine.apply_command(EngineCommand::SetPhraseStep {
        phrase_id: 0,
        step_index: 0,
        note: Some(61), // intentionally out-of-scale to show quantization
        velocity: 100,
        instrument_id: Some(0),
    });
    let _ = engine.apply_command(EngineCommand::SetPhraseStep {
        phrase_id: 0,
        step_index: 4,
        note: Some(64),
        velocity: 100,
        instrument_id: Some(0),
    });
    let _ = engine.apply_command(EngineCommand::SetPhraseStep {
        phrase_id: 0,
        step_index: 8,
        note: Some(67),
        velocity: 100,
        instrument_id: Some(0),
    });

    let _ = engine.apply_command(EngineCommand::SetSongRowChain {
        track_index: 0,
        row: 0,
        chain_id: Some(0),
    });

    let mut midi_input = NoopMidiInput;
    let mut midi_output = NoopMidiOutput::default();
    let mut scheduler = Scheduler::new(24);

    for message in midi_input.poll() {
        match decode_message(message) {
            DecodedMidi::Start | DecodedMidi::Continue => scheduler.start(),
            DecodedMidi::Stop => scheduler.stop(),
            _ => {}
        }
    }

    let mut events = Vec::new();
    for _ in 0..24 {
        events.extend(scheduler.tick(&engine));
    }
    let midi_messages = forward_render_events(&events, &mut midi_output);

    let mut audio = NoopAudioBackend::default();
    audio.start();
    audio.push_events(&events);
    audio.stop();

    let envelope = ProjectEnvelope::new(engine.snapshot().clone());
    let _ = envelope.validate_format();
    let serialized = envelope.to_text();
    let restored = ProjectEnvelope::from_text(&serialized).expect("storage round-trip");

    println!(
        "p9_tracker stage7 midi: tempo={}, restored_tempo={}, ticks={}, events={}, audio_events={}, midi_events={}",
        envelope.project.song.tempo,
        restored.project.song.tempo,
        scheduler.current_tick,
        events.len(),
        audio.events_consumed(),
        midi_messages
    );
}

fn major_scale_mask() -> u16 {
    let intervals = [0u16, 2, 4, 5, 7, 9, 11];
    let mut mask = 0u16;
    for interval in intervals {
        mask |= 1 << interval;
    }
    mask
}
