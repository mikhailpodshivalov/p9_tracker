use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{Chain, Instrument, InstrumentType, Phrase};
use p9_core::scheduler::Scheduler;
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
use p9_storage::project::ProjectEnvelope;

fn main() {
    let mut engine = Engine::new("p9_tracker song");
    let _ = engine.apply_command(EngineCommand::SetTempo(128));

    let mut chain = Chain::new(0);
    chain.rows[0].phrase_id = Some(0);
    let _ = engine.apply_command(EngineCommand::UpsertChain { chain });

    let mut phrase = Phrase::new(0);
    phrase.steps[0].note = Some(60);
    phrase.steps[0].velocity = 100;
    phrase.steps[0].instrument_id = Some(0);
    let _ = engine.apply_command(EngineCommand::UpsertPhrase { phrase });

    let instrument = Instrument::new(0, InstrumentType::Synth, "Init Synth");
    let _ = engine.apply_command(EngineCommand::UpsertInstrument { instrument });

    let _ = engine.apply_command(EngineCommand::SetSongRowChain {
        track_index: 0,
        row: 0,
        chain_id: Some(0),
    });

    let mut scheduler = Scheduler::new(24);
    let events = scheduler.tick(&engine);

    let mut audio = NoopAudioBackend::default();
    audio.start();
    audio.push_events(&events);
    audio.stop();

    let envelope = ProjectEnvelope::new(engine.snapshot().clone());
    let _ = envelope.validate_format();

    println!(
        "p9_tracker stage2 baseline: tempo={}, events={}, audio_events={}",
        envelope.project.song.tempo,
        events.len(),
        audio.events_consumed()
    );
}
