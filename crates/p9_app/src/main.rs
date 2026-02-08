use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{Chain, Instrument, InstrumentType, Phrase};
use p9_core::scheduler::Scheduler;
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
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

    let _ = engine.apply_command(EngineCommand::SetChainRowPhrase {
        chain_id: 0,
        row: 0,
        phrase_id: Some(0),
        transpose: 0,
    });

    let _ = engine.apply_command(EngineCommand::SetPhraseStep {
        phrase_id: 0,
        step_index: 0,
        note: Some(60),
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

    let mut scheduler = Scheduler::new(24);
    let mut events = Vec::new();
    for _ in 0..24 {
        events.extend(scheduler.tick(&engine));
    }

    let mut audio = NoopAudioBackend::default();
    audio.start();
    audio.push_events(&events);
    audio.stop();

    let envelope = ProjectEnvelope::new(engine.snapshot().clone());
    let _ = envelope.validate_format();
    let serialized = envelope.to_text();
    let restored = ProjectEnvelope::from_text(&serialized).expect("storage round-trip");

    println!(
        "p9_tracker stage3 core: tempo={}, restored_tempo={}, ticks={}, events={}, audio_events={}",
        envelope.project.song.tempo,
        restored.project.song.tempo,
        scheduler.current_tick,
        events.len(),
        audio.events_consumed()
    );
}
