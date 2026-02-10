mod hardening;
mod gui_shell;
mod runtime;
mod ui;
mod ui_shell;

use hardening::{AutosaveManager, AutosavePolicy};
use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{FxCommand, Groove, Instrument, InstrumentType, Scale, Table};
use p9_rt::audio::{build_preferred_audio_backend, start_with_noop_fallback, AudioMetrics};
use p9_rt::export::{render_project_to_wav, OfflineRenderConfig};
use p9_rt::midi::{BufferedMidiInput, BufferedMidiOutput, MidiMessage};
use p9_storage::project::ProjectEnvelope;
use runtime::{RuntimeCommand, RuntimeCoordinator, SyncMode};
use ui::{UiAction, UiController};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ui_shell_mode = args.iter().any(|arg| arg == "--ui-shell");
    let gui_shell_mode = args.iter().any(|arg| arg == "--gui-shell");

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

    if gui_shell_mode {
        if let Err(err) = gui_shell::run_web_shell(&mut ui, &mut engine, &mut runtime) {
            eprintln!("p9_tracker gui-shell failed: {err}");
        }
        return;
    }

    if ui_shell_mode {
        ui_shell::run_interactive_shell(&mut ui, &mut engine, &mut runtime)
            .expect("ui shell failed");
        return;
    }

    let mut started_audio = start_with_noop_fallback(build_preferred_audio_backend(true));
    let audio_backend_name = started_audio.backend().backend_name();
    let audio_used_fallback = started_audio.used_fallback;

    let mut midi_input = BufferedMidiInput::default();
    let mut midi_output = BufferedMidiOutput::default();
    let mut events_total = 0usize;
    let mut midi_total = 0usize;
    let mut midi_clock_total = 0usize;
    let mut last_audio_metrics = AudioMetrics::default();
    let mut last_voice_steals = 0u64;

    runtime.set_sync_mode(SyncMode::ExternalClock);
    runtime.enqueue_command(RuntimeCommand::Rewind);
    runtime.enqueue_command(RuntimeCommand::Start);

    for _ in 0..12 {
        midi_input.push_message(MidiMessage {
            status: 0xF8,
            data1: 0,
            data2: 0,
        });

        let report = runtime
            .run_cycle_safe(
                &engine,
                started_audio.backend_mut(),
                &mut midi_input,
                &mut midi_output,
            )
            .expect("runtime cycle failed");

        events_total = events_total.saturating_add(report.events_emitted);
        midi_total = midi_total.saturating_add(report.midi_messages_sent);
        midi_clock_total = midi_clock_total.saturating_add(report.midi_clock_messages_sent);
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
            voice_note_on_total: report.audio_voice_note_on_total,
            voice_note_off_total: report.audio_voice_note_off_total,
            voice_note_off_miss_total: report.audio_voice_note_off_miss_total,
            voice_retrigger_total: report.audio_voice_retrigger_total,
            voice_zero_attack_total: report.audio_voice_zero_attack_total,
            voice_short_release_total: report.audio_voice_short_release_total,
            click_risk_total: report.audio_click_risk_total,
            voice_release_deferred_total: report.audio_voice_release_deferred_total,
            voice_release_completed_total: report.audio_voice_release_completed_total,
            voice_release_pending_voices: report.audio_voice_release_pending_voices,
            voice_steal_releasing_total: report.audio_voice_steal_releasing_total,
            voice_steal_active_total: report.audio_voice_steal_active_total,
            voice_polyphony_pressure_total: report.audio_voice_polyphony_pressure_total,
            voice_sampler_mode_note_on_total: report.audio_voice_sampler_mode_note_on_total,
            voice_silent_note_on_total: report.audio_voice_silent_note_on_total,
        };
        last_voice_steals = report.audio_voices_stolen_total;
    }

    runtime.set_sync_mode(SyncMode::Internal);

    for _ in 0..12 {
        let report = runtime
            .run_cycle_safe(
                &engine,
                started_audio.backend_mut(),
                &mut midi_input,
                &mut midi_output,
            )
            .expect("runtime cycle failed");

        events_total = events_total.saturating_add(report.events_emitted);
        midi_total = midi_total.saturating_add(report.midi_messages_sent);
        midi_clock_total = midi_clock_total.saturating_add(report.midi_clock_messages_sent);
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
            voice_note_on_total: report.audio_voice_note_on_total,
            voice_note_off_total: report.audio_voice_note_off_total,
            voice_note_off_miss_total: report.audio_voice_note_off_miss_total,
            voice_retrigger_total: report.audio_voice_retrigger_total,
            voice_zero_attack_total: report.audio_voice_zero_attack_total,
            voice_short_release_total: report.audio_voice_short_release_total,
            click_risk_total: report.audio_click_risk_total,
            voice_release_deferred_total: report.audio_voice_release_deferred_total,
            voice_release_completed_total: report.audio_voice_release_completed_total,
            voice_release_pending_voices: report.audio_voice_release_pending_voices,
            voice_steal_releasing_total: report.audio_voice_steal_releasing_total,
            voice_steal_active_total: report.audio_voice_steal_active_total,
            voice_polyphony_pressure_total: report.audio_voice_polyphony_pressure_total,
            voice_sampler_mode_note_on_total: report.audio_voice_sampler_mode_note_on_total,
            voice_silent_note_on_total: report.audio_voice_silent_note_on_total,
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

    let export_path = std::env::temp_dir().join("p9_tracker_phase14_export.wav");
    let export_report = render_project_to_wav(
        &engine,
        &export_path,
        OfflineRenderConfig {
            sample_rate_hz: 48_000,
            ppq: 24,
            ticks: 96,
        },
    )
    .expect("offline export failed");

    let autosave_path = std::env::temp_dir().join("p9_tracker_phase14_autosave.p9");
    let mut autosave = AutosaveManager::new(AutosavePolicy { interval_ticks: 16 });
    let autosave_written = autosave
        .save_if_due(&engine, transport, true, &autosave_path)
        .expect("autosave failed");

    println!(
        "p9_tracker stage19.3a fx-routing-contracts: tempo={}, restored_tempo={}, ticks={}, playing={}, sync_mode={:?}, external_clock_pending={}, events={}, audio_events={}, midi_events={}, midi_clock_events={}, midi_ingested={}, midi_out_messages={}, processed_commands={}, backend={}, fallback={}, callbacks={}, xruns={}, last_callback_us={}, avg_callback_us={}, sample_rate={}, buffer_size={}, active_voices={}, max_voices={}, voice_steals={}, note_on_total={}, note_off_total={}, note_off_miss_total={}, retrigger_total={}, zero_attack_total={}, short_release_total={}, click_risk_total={}, release_deferred_total={}, release_completed_total={}, release_pending_voices={}, steal_releasing_total={}, steal_active_total={}, polyphony_pressure_total={}, sampler_mode_note_on_total={}, silent_note_on_total={}, ui_screen={:?}, ui_track={}, ui_song_row={}, ui_chain_row={}, ui_phrase={}, ui_step={}, ui_scale_highlight={:?}, ui_track_level={}, export_ticks={}, export_events={}, export_samples={}, export_peak={}, export_path={}, autosave_written={}, autosave_tick={}, autosave_path={}, ui_shell_mode_supported={}",
        envelope.project.song.tempo,
        restored.project.song.tempo,
        transport.tick,
        transport.is_playing,
        transport.sync_mode,
        transport.external_clock_pending,
        events_total,
        started_audio.backend().events_consumed(),
        midi_total,
        midi_clock_total,
        transport.midi_messages_ingested_total,
        midi_output.sent_count(),
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
        last_audio_metrics.voice_note_on_total,
        last_audio_metrics.voice_note_off_total,
        last_audio_metrics.voice_note_off_miss_total,
        last_audio_metrics.voice_retrigger_total,
        last_audio_metrics.voice_zero_attack_total,
        last_audio_metrics.voice_short_release_total,
        last_audio_metrics.click_risk_total,
        last_audio_metrics.voice_release_deferred_total,
        last_audio_metrics.voice_release_completed_total,
        last_audio_metrics.voice_release_pending_voices,
        last_audio_metrics.voice_steal_releasing_total,
        last_audio_metrics.voice_steal_active_total,
        last_audio_metrics.voice_polyphony_pressure_total,
        last_audio_metrics.voice_sampler_mode_note_on_total,
        last_audio_metrics.voice_silent_note_on_total,
        ui_snapshot.screen,
        ui_snapshot.focused_track,
        ui_snapshot.selected_song_row,
        ui_snapshot.selected_chain_row,
        ui_snapshot.selected_phrase_id,
        ui_snapshot.selected_step,
        ui_snapshot.scale_highlight,
        ui_snapshot.focused_track_level,
        export_report.ticks_rendered,
        export_report.events_rendered,
        export_report.samples_rendered,
        export_report.peak_abs_sample,
        export_path.display(),
        autosave_written,
        autosave.last_saved_tick(),
        autosave_path.display(),
        true,
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
