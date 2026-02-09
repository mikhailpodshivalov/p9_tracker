use std::io::{self, Write};

use crate::runtime::RuntimeCoordinator;
use crate::ui::{ScaleHighlightState, UiAction, UiController, UiError, UiScreen, UiSnapshot};
use p9_core::engine::Engine;
use p9_core::model::ProjectData;
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
use p9_rt::midi::NoopMidiOutput;

const SONG_VIEW_ROWS: usize = 8;
const CHAIN_VIEW_ROWS: usize = 8;
const PHRASE_COLS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellCommandResult {
    Continue(String),
    Exit,
}

pub fn run_interactive_shell(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> io::Result<()> {
    let mut status = String::from("Shell ready. Commands: n/p/h/l/j/k/t/r/c/f/i/e/+/-/?/q");
    let mut audio = NoopAudioBackend::default();
    audio.start();
    let mut midi_output = NoopMidiOutput::default();

    loop {
        let snapshot = ui.snapshot(engine, runtime);
        let frame = render_frame(engine.snapshot(), snapshot, &status);

        print!("\x1B[2J\x1B[H{frame}");
        io::stdout().flush()?;

        let mut line = String::new();
        let read = io::stdin().read_line(&mut line)?;
        if read == 0 {
            break;
        }

        match apply_shell_command(line.trim(), ui, engine, runtime) {
            Ok(ShellCommandResult::Continue(next_status)) => {
                status = match runtime.run_tick_safe(engine, &mut audio, &mut midi_output) {
                    Ok(report) => format!(
                        "{} | transport={} tick={}",
                        next_status,
                        transport_label(report.is_playing),
                        report.tick
                    ),
                    Err(_) => format!("{} | runtime fault", next_status),
                };
            }
            Ok(ShellCommandResult::Exit) => {
                break;
            }
            Err(err) => {
                status = format!("command error: {err:?}");
            }
        }
    }

    Ok(())
}

pub fn apply_shell_command(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> Result<ShellCommandResult, UiError> {
    match command {
        "n" => {
            ui.handle_action(UiAction::NextScreen, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("screen -> next")))
        }
        "p" => {
            ui.handle_action(UiAction::PrevScreen, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("screen -> previous")))
        }
        "h" => {
            ui.handle_action(UiAction::FocusTrackLeft, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("focus -> left track")))
        }
        "l" => {
            ui.handle_action(UiAction::FocusTrackRight, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("focus -> right track")))
        }
        "j" => {
            let action = cursor_shift_down(ui.snapshot(engine, runtime));
            ui.handle_action(action, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("cursor -> down")))
        }
        "k" => {
            let action = cursor_shift_up(ui.snapshot(engine, runtime));
            ui.handle_action(action, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("cursor -> up")))
        }
        "t" => {
            ui.handle_action(UiAction::TogglePlayStop, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from("transport -> toggle")))
        }
        "r" => {
            ui.handle_action(UiAction::RewindTransport, engine, runtime)?;
            Ok(ShellCommandResult::Continue(String::from(
                "transport -> stop+rewind",
            )))
        }
        "c" => {
            let snapshot = ui.snapshot(engine, runtime);
            let chain_id = snapshot.selected_song_row as u8;
            ui.handle_action(UiAction::EnsureChain { chain_id }, engine, runtime)?;
            ui.handle_action(
                UiAction::BindTrackRowToChain {
                    song_row: snapshot.selected_song_row,
                    chain_id: Some(chain_id),
                },
                engine,
                runtime,
            )?;
            Ok(ShellCommandResult::Continue(format!(
                "edit -> bind song row {} to chain {}",
                snapshot.selected_song_row, chain_id
            )))
        }
        "f" => {
            let snapshot = ui.snapshot(engine, runtime);
            let Some(chain_id) = bound_chain_id(engine.snapshot(), snapshot) else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: no chain on selected song row; run c first",
                )));
            };
            let phrase_id = (snapshot.selected_song_row * CHAIN_VIEW_ROWS
                + snapshot.selected_chain_row) as u8;

            ui.handle_action(UiAction::EnsurePhrase { phrase_id }, engine, runtime)?;
            ui.handle_action(UiAction::SelectPhrase(phrase_id), engine, runtime)?;
            ui.handle_action(
                UiAction::BindChainRowToPhrase {
                    chain_id,
                    chain_row: snapshot.selected_chain_row,
                    phrase_id: Some(phrase_id),
                    transpose: 0,
                },
                engine,
                runtime,
            )?;
            Ok(ShellCommandResult::Continue(format!(
                "edit -> bind chain {} row {} to phrase {}",
                chain_id, snapshot.selected_chain_row, phrase_id
            )))
        }
        "i" => {
            let snapshot = ui.snapshot(engine, runtime);
            let instrument_id = snapshot.focused_track as u8;
            ui.handle_action(
                UiAction::EnsureInstrument {
                    instrument_id,
                    instrument_type: p9_core::model::InstrumentType::Synth,
                    name: format!("Track {} Synth", snapshot.focused_track),
                },
                engine,
                runtime,
            )?;
            Ok(ShellCommandResult::Continue(format!(
                "edit -> ensure instrument {}",
                instrument_id
            )))
        }
        "e" => {
            let snapshot = ui.snapshot(engine, runtime);
            let instrument_id = snapshot.focused_track as u8;
            if !engine.snapshot().instruments.contains_key(&instrument_id) {
                return Ok(ShellCommandResult::Continue(format!(
                    "warn: instrument {} missing; run i first",
                    instrument_id
                )));
            }

            let Some(chain_id) = bound_chain_id(engine.snapshot(), snapshot) else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: no chain on selected song row; run c first",
                )));
            };
            let Some(phrase_id) = bound_phrase_id(engine.snapshot(), chain_id, snapshot) else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: no phrase on selected chain row; run f first",
                )));
            };
            let note = seeded_note(snapshot.selected_step);

            ui.handle_action(UiAction::SelectPhrase(phrase_id), engine, runtime)?;
            ui.handle_action(
                UiAction::EditStep {
                    phrase_id,
                    step_index: snapshot.selected_step,
                    note: Some(note),
                    velocity: 100,
                    instrument_id: Some(instrument_id),
                },
                engine,
                runtime,
            )?;
            Ok(ShellCommandResult::Continue(format!(
                "edit -> phrase {} step {} note {} ins {}",
                phrase_id, snapshot.selected_step, note, instrument_id
            )))
        }
        "+" => {
            let snapshot = ui.snapshot(engine, runtime);
            let next_level = snapshot.focused_track_level.saturating_add(4);
            if next_level == snapshot.focused_track_level {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: track level already at max",
                )));
            }
            ui.handle_action(UiAction::SetTrackLevel(next_level), engine, runtime)?;
            Ok(ShellCommandResult::Continue(format!(
                "mixer -> track {} level {}",
                snapshot.focused_track, next_level
            )))
        }
        "-" => {
            let snapshot = ui.snapshot(engine, runtime);
            let next_level = snapshot.focused_track_level.saturating_sub(4);
            if next_level == snapshot.focused_track_level {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: track level already at min",
                )));
            }
            ui.handle_action(UiAction::SetTrackLevel(next_level), engine, runtime)?;
            Ok(ShellCommandResult::Continue(format!(
                "mixer -> track {} level {}",
                snapshot.focused_track, next_level
            )))
        }
        "?" => Ok(ShellCommandResult::Continue(String::from(command_help()))),
        "q" => Ok(ShellCommandResult::Exit),
        "" => Ok(ShellCommandResult::Continue(String::from("idle"))),
        _ => Ok(ShellCommandResult::Continue(String::from(
            "unknown command; use n/p/h/l/j/k/t/r/c/f/i/e/+/-/?/q",
        ))),
    }
}

pub fn render_frame(project: &ProjectData, snapshot: UiSnapshot, status: &str) -> String {
    let mut out = String::new();

    out.push_str("P9 Tracker UI Shell (Phase 15.4)\n");
    out.push_str("Screen Tabs: ");
    out.push_str(&tab(UiScreen::Song, snapshot.screen));
    out.push(' ');
    out.push_str(&tab(UiScreen::Chain, snapshot.screen));
    out.push(' ');
    out.push_str(&tab(UiScreen::Phrase, snapshot.screen));
    out.push(' ');
    out.push_str(&tab(UiScreen::Mixer, snapshot.screen));
    out.push('\n');

    out.push_str(&format!(
        "Transport: {} | Tick: {} | Focus Track: T{} | Track Level: {}\n",
        transport_label(snapshot.is_playing),
        snapshot.tick,
        snapshot.focused_track,
        snapshot.focused_track_level,
    ));
    out.push_str(&format!(
        "Cursor: song_row={} chain_row={} phrase={} step={} scale={:?}\n",
        snapshot.selected_song_row,
        snapshot.selected_chain_row,
        snapshot.selected_phrase_id,
        snapshot.selected_step,
        snapshot.scale_highlight,
    ));
    out.push_str("----------------------------------------------------------------\n");

    match snapshot.screen {
        UiScreen::Song => render_song_panel(&mut out, project, snapshot),
        UiScreen::Chain => render_chain_panel(&mut out, project, snapshot),
        UiScreen::Phrase => render_phrase_panel(&mut out, project, snapshot),
        UiScreen::Mixer => render_mixer_panel(&mut out, project, snapshot),
    }

    out.push_str("----------------------------------------------------------------\n");
    out.push_str(&format!("Status: {status}\n"));
    out.push_str(
        "Commands: n/p screen, h/l track, j/k cursor, t play, r rewind, c/f/i/e edit, +/- level, ? help, q quit\n",
    );

    out
}

fn tab(target: UiScreen, current: UiScreen) -> String {
    let label = match target {
        UiScreen::Song => "SONG",
        UiScreen::Chain => "CHAIN",
        UiScreen::Phrase => "PHRASE",
        UiScreen::Mixer => "MIXER",
    };

    if target == current {
        format!("[{label}]")
    } else {
        format!(" {label} ")
    }
}

fn transport_label(playing: bool) -> &'static str {
    if playing {
        "PLAY"
    } else {
        "STOP"
    }
}

fn seeded_note(step_index: usize) -> u8 {
    const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
    let octave = (step_index / MAJOR.len()) as u8;
    let interval = MAJOR[step_index % MAJOR.len()];
    60u8.saturating_add(interval).saturating_add(octave.saturating_mul(12))
}

fn bound_chain_id(project: &ProjectData, snapshot: UiSnapshot) -> Option<u8> {
    project
        .song
        .tracks
        .get(snapshot.focused_track)
        .and_then(|track| track.song_rows.get(snapshot.selected_song_row))
        .copied()
        .flatten()
}

fn bound_phrase_id(project: &ProjectData, chain_id: u8, snapshot: UiSnapshot) -> Option<u8> {
    project
        .chains
        .get(&chain_id)
        .and_then(|chain| chain.rows.get(snapshot.selected_chain_row))
        .and_then(|row| row.phrase_id)
}

fn command_help() -> &'static str {
    "help: n/p screen, h/l track, j/k cursor, t play/stop, r stop+rewind, c bind chain, f bind phrase, i ensure instrument, e edit step, +/- level"
}

fn wrap_next(current: usize, len: usize) -> usize {
    (current + 1) % len
}

fn wrap_prev(current: usize, len: usize) -> usize {
    if current == 0 {
        len - 1
    } else {
        current - 1
    }
}

fn cursor_shift_down(snapshot: UiSnapshot) -> UiAction {
    match snapshot.screen {
        UiScreen::Song => UiAction::SelectSongRow(wrap_next(snapshot.selected_song_row, SONG_VIEW_ROWS)),
        UiScreen::Chain => {
            UiAction::SelectChainRow(wrap_next(snapshot.selected_chain_row, CHAIN_VIEW_ROWS))
        }
        UiScreen::Phrase => UiAction::SelectStep((snapshot.selected_step + PHRASE_COLS) % 16),
        UiScreen::Mixer => UiAction::FocusTrackRight,
    }
}

fn cursor_shift_up(snapshot: UiSnapshot) -> UiAction {
    match snapshot.screen {
        UiScreen::Song => UiAction::SelectSongRow(wrap_prev(snapshot.selected_song_row, SONG_VIEW_ROWS)),
        UiScreen::Chain => {
            UiAction::SelectChainRow(wrap_prev(snapshot.selected_chain_row, CHAIN_VIEW_ROWS))
        }
        UiScreen::Phrase => UiAction::SelectStep((snapshot.selected_step + 16 - PHRASE_COLS) % 16),
        UiScreen::Mixer => UiAction::FocusTrackLeft,
    }
}

fn render_song_panel(out: &mut String, project: &ProjectData, snapshot: UiSnapshot) {
    out.push_str("Song Panel\n");

    if let Some(track) = project.song.tracks.get(snapshot.focused_track) {
        for row in 0..SONG_VIEW_ROWS {
            let marker = if row == snapshot.selected_song_row {
                ">"
            } else {
                " "
            };

            let chain = track
                .song_rows
                .get(row)
                .copied()
                .flatten()
                .map(|id| format!("{id:02}"))
                .unwrap_or_else(|| String::from("--"));

            out.push_str(&format!("{marker} row {row:02} -> chain {chain}\n"));
        }
    }
}

fn render_chain_panel(out: &mut String, project: &ProjectData, snapshot: UiSnapshot) {
    out.push_str("Chain Panel\n");

    let chain_id = project
        .song
        .tracks
        .get(snapshot.focused_track)
        .and_then(|track| track.song_rows.get(snapshot.selected_song_row))
        .copied()
        .flatten();

    let Some(chain_id) = chain_id else {
        out.push_str("No chain bound on selected song row.\n");
        return;
    };

    let Some(chain) = project.chains.get(&chain_id) else {
        out.push_str("Selected chain not found in project.\n");
        return;
    };

    out.push_str(&format!("Chain ID: {chain_id}\n"));

    for row in 0..CHAIN_VIEW_ROWS {
        let marker = if row == snapshot.selected_chain_row {
            ">"
        } else {
            " "
        };

        let phrase = chain
            .rows
            .get(row)
            .and_then(|entry| entry.phrase_id)
            .map(|id| format!("{id:02}"))
            .unwrap_or_else(|| String::from("--"));

        let transpose = chain.rows.get(row).map(|entry| entry.transpose).unwrap_or(0);

        out.push_str(&format!(
            "{marker} row {row:02} -> phrase {phrase} | trn {transpose:+}\n"
        ));
    }
}

fn render_phrase_panel(out: &mut String, project: &ProjectData, snapshot: UiSnapshot) {
    out.push_str("Phrase Panel\n");

    let Some(phrase) = project.phrases.get(&snapshot.selected_phrase_id) else {
        out.push_str("Selected phrase not found.\n");
        return;
    };

    out.push_str(&format!("Phrase ID: {}\n", snapshot.selected_phrase_id));

    for row in 0..4usize {
        let mut row_line = String::new();
        for col in 0..4usize {
            let step_index = row * 4 + col;
            let marker = if step_index == snapshot.selected_step {
                ">"
            } else {
                " "
            };

            let cell = if let Some(step) = phrase.steps.get(step_index) {
                match step.note {
                    Some(note) => format!("{note:02}:v{:03}", step.velocity),
                    None => String::from("--:v---"),
                }
            } else {
                String::from("--:v---")
            };

            row_line.push_str(&format!("{marker}{step_index:02} {cell}  "));
        }

        out.push_str(row_line.trim_end());
        out.push('\n');
    }

    if snapshot.scale_highlight == ScaleHighlightState::OutOfScale {
        out.push_str("Scale hint: selected note is out of scale.\n");
    }
}

fn render_mixer_panel(out: &mut String, project: &ProjectData, snapshot: UiSnapshot) {
    out.push_str("Mixer Panel\n");

    for (track_index, level) in project.mixer.track_levels.iter().enumerate() {
        let marker = if track_index == snapshot.focused_track {
            ">"
        } else {
            " "
        };

        out.push_str(&format!("{marker} track {track_index}: level {level}\n"));
    }

    out.push_str(&format!("Master: {}\n", project.mixer.master_level));
    out.push_str(&format!(
        "Sends: mfx={} delay={} reverb={}\n",
        project.mixer.send_levels.mfx,
        project.mixer.send_levels.delay,
        project.mixer.send_levels.reverb,
    ));
}

#[cfg(test)]
mod tests {
    use super::{apply_shell_command, render_frame, ShellCommandResult};
    use crate::runtime::RuntimeCoordinator;
    use crate::ui::{UiController, UiScreen};
    use p9_core::engine::Engine;
    use p9_rt::audio::{AudioBackend, NoopAudioBackend};
    use p9_rt::midi::NoopMidiOutput;

    #[test]
    fn render_frame_contains_shell_layout_sections() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let snapshot = ui.snapshot(&engine, &runtime);
        let frame = render_frame(engine.snapshot(), snapshot, "ok");

        assert!(frame.contains("P9 Tracker UI Shell (Phase 15.4)"));
        assert!(frame.contains("Screen Tabs:"));
        assert!(frame.contains("Song Panel"));
        assert!(frame.contains("Commands: n/p screen"));

        let _ = apply_shell_command("n", &mut ui, &mut engine, &mut runtime).unwrap();
        let chain_frame = render_frame(engine.snapshot(), ui.snapshot(&engine, &runtime), "ok");
        assert!(chain_frame.contains("[CHAIN]"));
    }

    #[test]
    fn shell_commands_switch_screen_and_focus() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let first = apply_shell_command("n", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(first, ShellCommandResult::Continue(String::from("screen -> next")));
        assert_eq!(ui.snapshot(&engine, &runtime).screen, UiScreen::Chain);

        let second = apply_shell_command("l", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(
            second,
            ShellCommandResult::Continue(String::from("focus -> right track"))
        );
        assert_eq!(ui.snapshot(&engine, &runtime).focused_track, 1);
    }

    #[test]
    fn shell_cursor_commands_move_rows_and_steps() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let _ = apply_shell_command("j", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(ui.snapshot(&engine, &runtime).selected_song_row, 1);

        let _ = apply_shell_command("n", &mut ui, &mut engine, &mut runtime).unwrap();
        let _ = apply_shell_command("j", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(ui.snapshot(&engine, &runtime).selected_chain_row, 1);

        let _ = apply_shell_command("n", &mut ui, &mut engine, &mut runtime).unwrap();
        let _ = apply_shell_command("j", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(ui.snapshot(&engine, &runtime).selected_step, 4);

        let _ = apply_shell_command("k", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(ui.snapshot(&engine, &runtime).selected_step, 0);
    }

    #[test]
    fn shell_transport_commands_queue_runtime_updates() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let _ = apply_shell_command("t", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(runtime.snapshot().queued_commands, 1);

        let _ = apply_shell_command("r", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(runtime.snapshot().queued_commands, 3);
    }

    #[test]
    fn shell_edit_commands_create_minimal_authoring_flow() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let _ = apply_shell_command("c", &mut ui, &mut engine, &mut runtime).unwrap();
        let _ = apply_shell_command("f", &mut ui, &mut engine, &mut runtime).unwrap();
        let _ = apply_shell_command("i", &mut ui, &mut engine, &mut runtime).unwrap();
        let _ = apply_shell_command("e", &mut ui, &mut engine, &mut runtime).unwrap();

        let project = engine.snapshot();
        assert_eq!(project.song.tracks[0].song_rows[0], Some(0));
        assert_eq!(project.chains.get(&0).unwrap().rows[0].phrase_id, Some(0));

        let phrase = project.phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[0].note, Some(60));
        assert_eq!(phrase.steps[0].velocity, 100);
        assert_eq!(phrase.steps[0].instrument_id, Some(0));
        assert!(project.instruments.contains_key(&0));
    }

    #[test]
    fn shell_mixer_commands_change_track_level() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        assert_eq!(ui.snapshot(&engine, &runtime).focused_track_level, 128);

        let _ = apply_shell_command("+", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(ui.snapshot(&engine, &runtime).focused_track_level, 132);

        let _ = apply_shell_command("-", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(ui.snapshot(&engine, &runtime).focused_track_level, 128);
    }

    #[test]
    fn shell_command_quit_returns_exit() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let result = apply_shell_command("q", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(result, ShellCommandResult::Exit);
    }

    #[test]
    fn shell_safety_warns_when_phrase_bind_has_no_chain() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let result = apply_shell_command("f", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(
            result,
            ShellCommandResult::Continue(String::from(
                "warn: no chain on selected song row; run c first"
            ))
        );
    }

    #[test]
    fn shell_safety_warns_when_edit_has_no_instrument() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let _ = apply_shell_command("c", &mut ui, &mut engine, &mut runtime).unwrap();
        let _ = apply_shell_command("f", &mut ui, &mut engine, &mut runtime).unwrap();

        let result = apply_shell_command("e", &mut ui, &mut engine, &mut runtime).unwrap();
        assert_eq!(
            result,
            ShellCommandResult::Continue(String::from(
                "warn: instrument 0 missing; run i first"
            ))
        );
    }

    #[test]
    fn shell_help_command_returns_reference() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let result = apply_shell_command("?", &mut ui, &mut engine, &mut runtime).unwrap();
        assert!(matches!(result, ShellCommandResult::Continue(msg) if msg.contains("help:")));
    }

    #[test]
    fn shell_smoke_flow_edit_and_play_emits_events() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let mut audio = NoopAudioBackend::default();
        audio.start();
        let mut midi = NoopMidiOutput::default();

        let mut run_step = |command: &str| {
            let _ = apply_shell_command(command, &mut ui, &mut engine, &mut runtime).unwrap();
            runtime.run_tick(&engine, &mut audio, &mut midi)
        };

        let _ = run_step("c");
        let _ = run_step("f");
        let _ = run_step("i");
        let _ = run_step("e");
        let _ = run_step("r");
        let report = run_step("t");

        assert_eq!(report.events_emitted, 1);
        assert!(runtime.snapshot().is_playing);
    }
}
