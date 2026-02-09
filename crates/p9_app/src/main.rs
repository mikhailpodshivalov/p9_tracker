mod runtime;
mod ui;

use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{FxCommand, Groove, Instrument, InstrumentType, Scale, Table};
use p9_rt::audio::{
    build_preferred_audio_backend, start_with_noop_fallback, AudioMetrics,
};
use p9_rt::midi::{NoopMidiInput, NoopMidiOutput};
use p9_storage::project::ProjectEnvelope;
use runtime::{RuntimeCommand, RuntimeCoordinator};
use ui::{UiAction, UiController};

fn main() {
    let mut engine = Engine::new("p9_tracker song");
    let _ = engine.apply_command(EngineCommand::SetTempo(128));

    let mut runtime = RuntimeCoordinator::new(24);
    let mut ui = UiController::default();

    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::EnsureInstrument {
            instrument_id: 0,
            instrument_type: InstrumentType::Synth,
            name: "UI Init Synth".to_string(),
        },
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::EnsureChain { chain_id: 0 },
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::EnsurePhrase { phrase_id: 0 },
    );

    let table = Table::new(0);
    let _ = engine.apply_command(EngineCommand::UpsertTable { table });
    let _ = engine.apply_command(EngineCommand::SetTableRow {
        table_id: 0,
        row: 0,
        note_offset: 2,
        volume: 96,
    });

    let mut instrument = Instrument::new(0, InstrumentType::Synth, "UI Init Synth");
    instrument.table_id = Some(0);
    instrument.note_length_steps = 2;
    let _ = engine.apply_command(EngineCommand::UpsertInstrument { instrument });

    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::BindTrackRowToChain {
            song_row: 0,
            chain_id: Some(0),
        },
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::BindChainRowToPhrase {
            chain_id: 0,
            chain_row: 0,
            phrase_id: Some(0),
            transpose: 0,
        },
    );

    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::SelectPhrase(0),
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::SelectStep(0),
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::EditStep {
            phrase_id: 0,
            step_index: 0,
            note: Some(61),
            velocity: 100,
            instrument_id: Some(0),
        },
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::EditStep {
            phrase_id: 0,
            step_index: 4,
            note: Some(64),
            velocity: 100,
            instrument_id: Some(0),
        },
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::EditStep {
            phrase_id: 0,
            step_index: 8,
            note: Some(67),
            velocity: 100,
            instrument_id: Some(0),
        },
    );

    let _ = engine.apply_command(EngineCommand::SetStepFx {
        phrase_id: 0,
        step_index: 0,
        fx_slot: 0,
        fx: Some(FxCommand {
            code: "TRN".to_string(),
            value: 52,
        }),
    });
    let _ = engine.apply_command(EngineCommand::SetStepFx {
        phrase_id: 0,
        step_index: 0,
        fx_slot: 1,
        fx: Some(FxCommand {
            code: "VOL".to_string(),
            value: 90,
        }),
    });

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

    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::SetTrackLevel(100),
    );
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::SetMasterLevel(110),
    );
    let _ = engine.apply_command(EngineCommand::SetMixerSends {
        mfx: 20,
        delay: 30,
        reverb: 40,
    });

    let mut started_audio = start_with_noop_fallback(build_preferred_audio_backend(true));
    let audio_backend_name = started_audio.backend().backend_name();
    let audio_used_fallback = started_audio.used_fallback;

    let mut midi_input = NoopMidiInput;
    let mut midi_output = NoopMidiOutput::default();
    let mut events_total = 0usize;
    let mut midi_total = 0usize;
    let mut last_audio_metrics = AudioMetrics::default();
    let mut last_voice_steals = 0u64;

    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::TogglePlayStop,
    );
    let _ = runtime.run_tick(&engine, started_audio.backend_mut(), &mut midi_output);

    runtime.enqueue_command(RuntimeCommand::Rewind);
    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::TogglePlayStop,
    );

    for _ in 0..24 {
        let _mapped = runtime.ingest_midi_input(&mut midi_input);
        let report = runtime.run_tick(
            &engine,
            started_audio.backend_mut(),
            &mut midi_output,
        );
        events_total = events_total.saturating_add(report.events_emitted);
        midi_total = midi_total.saturating_add(report.midi_messages_sent);
        last_audio_metrics = AudioMetrics {
            sample_rate_hz: report.audio_sample_rate_hz,
            buffer_size_frames: report.audio_buffer_size_frames,
            callbacks_total: report.audio_callbacks_total,
            xruns_total: report.audio_xruns_total,
            last_callback_us: report.audio_last_callback_us,
            avg_callback_us: report.audio_avg_callback_us,
            active_voices: report.audio_active_voices,
            max_voices: report.audio_max_voices,
            voices_stolen_total: report.audio_voices_stolen_total,
        };
        last_voice_steals = report.audio_voices_stolen_total;
    }

    apply_ui(
        &mut ui,
        &mut engine,
        &mut runtime,
        UiAction::SelectStep(0),
    );

    started_audio.backend_mut().stop();
    let transport = runtime.snapshot();
    let ui_snapshot = ui.snapshot(&engine, &runtime);

    let envelope = ProjectEnvelope::new(engine.snapshot().clone());
    let _ = envelope.validate_format();
    let serialized = envelope.to_text();
    let restored = ProjectEnvelope::from_text(&serialized).expect("storage round-trip");

    println!(
        "p9_tracker stage13 ui-alpha: tempo={}, restored_tempo={}, ticks={}, playing={}, events={}, audio_events={}, midi_events={}, processed_commands={}, backend={}, fallback={}, callbacks={}, xruns={}, last_callback_us={}, avg_callback_us={}, sample_rate={}, buffer_size={}, active_voices={}, max_voices={}, voice_steals={}, ui_screen={:?}, ui_track={}, ui_song_row={}, ui_chain_row={}, ui_phrase={}, ui_step={}, ui_scale_highlight={:?}, ui_track_level={}",
        envelope.project.song.tempo,
        restored.project.song.tempo,
        transport.tick,
        transport.is_playing,
        events_total,
        started_audio.backend().events_consumed(),
        midi_total,
        transport.processed_commands,
        audio_backend_name,
        audio_used_fallback,
        last_audio_metrics.callbacks_total,
        last_audio_metrics.xruns_total,
        last_audio_metrics.last_callback_us,
        last_audio_metrics.avg_callback_us,
        last_audio_metrics.sample_rate_hz,
        last_audio_metrics.buffer_size_frames,
        last_audio_metrics.active_voices,
        last_audio_metrics.max_voices,
        last_voice_steals,
        ui_snapshot.screen,
        ui_snapshot.focused_track,
        ui_snapshot.selected_song_row,
        ui_snapshot.selected_chain_row,
        ui_snapshot.selected_phrase_id,
        ui_snapshot.selected_step,
        ui_snapshot.scale_highlight,
        ui_snapshot.focused_track_level,
    );
}

fn apply_ui(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    action: UiAction,
) {
    ui.handle_action(action, engine, runtime)
        .expect("ui action failed");
}

fn major_scale_mask() -> u16 {
    let intervals = [0u16, 2, 4, 5, 7, 9, 11];
    let mut mask = 0u16;
    for interval in intervals {
        mask |= 1 << interval;
    }
    mask
}
