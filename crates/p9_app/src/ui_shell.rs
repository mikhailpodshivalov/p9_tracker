use std::io::{self, Write};
use std::path::Path;

use crate::hardening::{
    clear_dirty_session_flag, default_autosave_path, default_dirty_flag_path,
    mark_dirty_session_flag, recover_from_dirty_session, AutosaveManager, AutosavePolicy,
    DirtyStateTracker,
};
use crate::runtime::RuntimeCoordinator;
use crate::ui::{ScaleHighlightState, UiAction, UiController, UiError, UiScreen, UiSnapshot};
use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{ProjectData, Step, PHRASE_STEP_COUNT};
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
use p9_rt::midi::NoopMidiOutput;

const SONG_VIEW_ROWS: usize = 8;
const CHAIN_VIEW_ROWS: usize = 8;
const PHRASE_COLS: usize = 4;
const HISTORY_LIMIT: usize = 128;
const SHELL_AUTOSAVE_INTERVAL_TICKS: u64 = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellCommandResult {
    Continue(String),
    Exit,
}

#[derive(Clone, Debug)]
pub struct ProjectHistory {
    undo_stack: Vec<ProjectData>,
    redo_stack: Vec<ProjectData>,
    limit: usize,
}

impl ProjectHistory {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: limit.max(1),
        }
    }

    pub fn record_change(&mut self, previous: ProjectData) {
        if self.undo_stack.len() >= self.limit {
            let overflow = self.undo_stack.len() + 1 - self.limit;
            self.undo_stack.drain(0..overflow);
        }
        self.undo_stack.push(previous);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, engine: &mut Engine) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };

        self.redo_stack.push(engine.snapshot().clone());
        engine.replace_project(previous);
        true
    }

    pub fn redo(&mut self, engine: &mut Engine) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };

        self.undo_stack.push(engine.snapshot().clone());
        engine.replace_project(next);
        true
    }
}

#[derive(Clone, Debug)]
pub struct StepSelection {
    track_index: usize,
    phrase_id: u8,
    start_step: usize,
    end_step: usize,
}

impl StepSelection {
    fn matches_scope(&self, track_index: usize, phrase_id: u8) -> bool {
        self.track_index == track_index && self.phrase_id == phrase_id
    }

    fn normalized_range(&self) -> (usize, usize) {
        if self.start_step <= self.end_step {
            (self.start_step, self.end_step)
        } else {
            (self.end_step, self.start_step)
        }
    }

    fn len(&self) -> usize {
        let (start, end) = self.normalized_range();
        end - start + 1
    }
}

#[derive(Clone, Debug)]
pub struct StepClipboard {
    source_track: usize,
    steps: Vec<Step>,
}

#[derive(Clone, Debug, Default)]
pub struct ShellEditState {
    selection: Option<StepSelection>,
    clipboard: Option<StepClipboard>,
}

pub fn run_interactive_shell(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> io::Result<()> {
    let autosave_path = default_autosave_path();
    let dirty_flag_path = default_dirty_flag_path();
    let recovery_status = recover_from_dirty_session(engine, &autosave_path, &dirty_flag_path);
    let mut dirty_tracker = DirtyStateTracker::from_engine(engine);
    let mut autosave = AutosaveManager::new(AutosavePolicy {
        interval_ticks: SHELL_AUTOSAVE_INTERVAL_TICKS,
    });

    let mut status = format!(
        "Shell ready. Commands: n/p/h/l/j/k/t/r/c/f/i/e/a/z/w/v/x/+/-/u/y/?/q | recovery={}",
        recovery_status.label()
    );
    let mut audio = NoopAudioBackend::default();
    audio.start();
    let mut midi_output = NoopMidiOutput::default();
    let mut history = ProjectHistory::with_limit(HISTORY_LIMIT);
    let mut edit_state = ShellEditState::default();

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

        match apply_shell_command_with_history_state(
            line.trim(),
            ui,
            engine,
            runtime,
            &mut history,
            &mut edit_state,
        ) {
            Ok(ShellCommandResult::Continue(next_status)) => {
                let tick_status = match runtime.run_tick_safe(engine, &mut audio, &mut midi_output) {
                    Ok(report) => {
                        format!(
                            "transport={} tick={}",
                            transport_label(report.is_playing),
                            report.tick
                        )
                    }
                    Err(_) => String::from("runtime fault"),
                };
                let hardening_status = update_session_hardening(
                    engine,
                    runtime,
                    &mut dirty_tracker,
                    &mut autosave,
                    &autosave_path,
                    &dirty_flag_path,
                );
                status = format!(
                    "{} | {} | recovery={} | {}",
                    next_status,
                    tick_status,
                    recovery_status.label(),
                    hardening_status
                );
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

#[cfg(test)]
pub fn apply_shell_command_with_history(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
) -> Result<ShellCommandResult, UiError> {
    let mut edit_state = ShellEditState::default();
    apply_shell_command_with_history_state(command, ui, engine, runtime, history, &mut edit_state)
}

pub fn apply_shell_command_with_history_state(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> Result<ShellCommandResult, UiError> {
    match command {
        "u" => {
            if history.undo(engine) {
                Ok(ShellCommandResult::Continue(String::from("undo -> applied")))
            } else {
                Ok(ShellCommandResult::Continue(String::from(
                    "undo -> empty history",
                )))
            }
        }
        "y" => {
            if history.redo(engine) {
                Ok(ShellCommandResult::Continue(String::from("redo -> applied")))
            } else {
                Ok(ShellCommandResult::Continue(String::from(
                    "redo -> empty history",
                )))
            }
        }
        _ => {
            let before = if is_mutating_command(command) {
                Some(engine.snapshot().clone())
            } else {
                None
            };

            let result = apply_shell_command_with_state(command, ui, engine, runtime, edit_state)?;

            if let Some(previous) = before {
                if command_did_mutate(&result) {
                    history.record_change(previous);
                }
            }

            Ok(result)
        }
    }
}

#[cfg(test)]
pub fn apply_shell_command(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> Result<ShellCommandResult, UiError> {
    let mut edit_state = ShellEditState::default();
    apply_shell_command_with_state(command, ui, engine, runtime, &mut edit_state)
}

pub fn apply_shell_command_with_state(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    edit_state: &mut ShellEditState,
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

            let phrase_id = match resolve_bound_phrase_id(engine.snapshot(), snapshot) {
                Ok(phrase_id) => phrase_id,
                Err(message) => return Ok(ShellCommandResult::Continue(String::from(message))),
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
        "a" => {
            let snapshot = ui.snapshot(engine, runtime);
            let phrase_id = match resolve_bound_phrase_id(engine.snapshot(), snapshot) {
                Ok(phrase_id) => phrase_id,
                Err(message) => return Ok(ShellCommandResult::Continue(String::from(message))),
            };
            ui.handle_action(UiAction::SelectPhrase(phrase_id), engine, runtime)?;
            edit_state.selection = Some(StepSelection {
                track_index: snapshot.focused_track,
                phrase_id,
                start_step: snapshot.selected_step,
                end_step: snapshot.selected_step,
            });
            Ok(ShellCommandResult::Continue(format!(
                "select -> start t{} phrase {} step {:02}",
                snapshot.focused_track, phrase_id, snapshot.selected_step
            )))
        }
        "z" => {
            let snapshot = ui.snapshot(engine, runtime);
            let phrase_id = match resolve_bound_phrase_id(engine.snapshot(), snapshot) {
                Ok(phrase_id) => phrase_id,
                Err(message) => return Ok(ShellCommandResult::Continue(String::from(message))),
            };
            let Some(selection) = edit_state.selection.as_mut() else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selection start missing; run a first",
                )));
            };
            if !selection.matches_scope(snapshot.focused_track, phrase_id) {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selection scope changed; run a to restart selection",
                )));
            }
            selection.end_step = snapshot.selected_step;
            let (start, end) = selection.normalized_range();
            Ok(ShellCommandResult::Continue(format!(
                "select -> range {:02}-{:02} len {}",
                start,
                end,
                selection.len()
            )))
        }
        "w" => {
            let snapshot = ui.snapshot(engine, runtime);
            let phrase_id = match resolve_bound_phrase_id(engine.snapshot(), snapshot) {
                Ok(phrase_id) => phrase_id,
                Err(message) => return Ok(ShellCommandResult::Continue(String::from(message))),
            };
            let Some(selection) = edit_state.selection.as_ref() else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selection start missing; run a first",
                )));
            };
            if !selection.matches_scope(snapshot.focused_track, phrase_id) {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selection scope changed; run a to restart selection",
                )));
            }

            let (start, end) = selection.normalized_range();
            let Some(phrase) = engine.snapshot().phrases.get(&selection.phrase_id) else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selected phrase missing; run f first",
                )));
            };
            if end >= phrase.steps.len() {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selection bounds out of phrase range",
                )));
            }

            let steps = phrase.steps[start..=end].to_vec();
            edit_state.clipboard = Some(StepClipboard {
                source_track: selection.track_index,
                steps,
            });
            Ok(ShellCommandResult::Continue(format!(
                "copy -> phrase {} steps {:02}-{:02} len {}",
                selection.phrase_id,
                start,
                end,
                selection.len()
            )))
        }
        "v" => {
            let snapshot = ui.snapshot(engine, runtime);
            let target_phrase_id = match resolve_bound_phrase_id(engine.snapshot(), snapshot) {
                Ok(phrase_id) => phrase_id,
                Err(message) => return Ok(ShellCommandResult::Continue(String::from(message))),
            };

            let Some(clipboard) = edit_state.clipboard.as_ref() else {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: clipboard empty; run w first",
                )));
            };
            if clipboard.source_track != snapshot.focused_track {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: clipboard track mismatch; focus source track or recopy",
                )));
            }
            if !engine.snapshot().phrases.contains_key(&target_phrase_id) {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: selected phrase missing; run f first",
                )));
            }

            let available = PHRASE_STEP_COUNT.saturating_sub(snapshot.selected_step);
            let paste_len = clipboard.steps.len().min(available);
            if paste_len == 0 {
                return Ok(ShellCommandResult::Continue(String::from(
                    "warn: paste target out of range",
                )));
            }

            ui.handle_action(UiAction::SelectPhrase(target_phrase_id), engine, runtime)?;
            for (offset, step) in clipboard.steps.iter().take(paste_len).enumerate() {
                apply_step(engine, target_phrase_id, snapshot.selected_step + offset, step)?;
            }

            let end_step = snapshot.selected_step + paste_len - 1;
            edit_state.selection = Some(StepSelection {
                track_index: snapshot.focused_track,
                phrase_id: target_phrase_id,
                start_step: snapshot.selected_step,
                end_step,
            });

            let clipped = clipboard.steps.len() - paste_len;
            if clipped > 0 {
                Ok(ShellCommandResult::Continue(format!(
                    "paste -> phrase {} steps {:02}-{:02} len {} (clipped {})",
                    target_phrase_id, snapshot.selected_step, end_step, paste_len, clipped
                )))
            } else {
                Ok(ShellCommandResult::Continue(format!(
                    "paste -> phrase {} steps {:02}-{:02} len {}",
                    target_phrase_id, snapshot.selected_step, end_step, paste_len
                )))
            }
        }
        "x" => {
            if edit_state.selection.take().is_some() {
                Ok(ShellCommandResult::Continue(String::from("select -> cleared")))
            } else {
                Ok(ShellCommandResult::Continue(String::from("select -> empty")))
            }
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
            "unknown command; use n/p/h/l/j/k/t/r/c/f/i/e/a/z/w/v/x/+/-/u/y/?/q",
        ))),
    }
}

pub fn render_frame(project: &ProjectData, snapshot: UiSnapshot, status: &str) -> String {
    let mut out = String::new();

    out.push_str("P9 Tracker UI Shell (Phase 16.3)\n");
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
        "Commands: n/p screen, h/l track, j/k cursor, t play, r rewind, c/f/i/e edit, a/z/w/v/x block, +/- level, u/y undo-redo, ? help, q quit\n",
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

fn resolve_bound_phrase_id(project: &ProjectData, snapshot: UiSnapshot) -> Result<u8, &'static str> {
    let chain_id = bound_chain_id(project, snapshot)
        .ok_or("warn: no chain on selected song row; run c first")?;
    bound_phrase_id(project, chain_id, snapshot)
        .ok_or("warn: no phrase on selected chain row; run f first")
}

fn apply_step(
    engine: &mut Engine,
    phrase_id: u8,
    step_index: usize,
    step: &Step,
) -> Result<(), UiError> {
    engine.apply_command(EngineCommand::SetPhraseStep {
        phrase_id,
        step_index,
        note: step.note,
        velocity: step.velocity,
        instrument_id: step.instrument_id,
    })?;
    for (fx_slot, fx) in step.fx.iter().cloned().enumerate() {
        engine.apply_command(EngineCommand::SetStepFx {
            phrase_id,
            step_index,
            fx_slot,
            fx,
        })?;
    }
    Ok(())
}

fn command_help() -> &'static str {
    "help: n/p screen, h/l track, j/k cursor, t play/stop, r stop+rewind, c bind chain, f bind phrase, i ensure instrument, e edit step, a/z selection start/end, w copy, v paste, x clear selection, +/- level, u undo, y redo"
}

fn is_mutating_command(command: &str) -> bool {
    matches!(command, "c" | "f" | "i" | "e" | "v" | "+" | "-")
}

fn command_did_mutate(result: &ShellCommandResult) -> bool {
    matches!(result, ShellCommandResult::Continue(msg) if !msg.starts_with("warn:"))
}

fn update_session_hardening(
    engine: &Engine,
    runtime: &RuntimeCoordinator,
    dirty_tracker: &mut DirtyStateTracker,
    autosave: &mut AutosaveManager,
    autosave_path: &Path,
    dirty_flag_path: &Path,
) -> String {
    let mut dirty = dirty_tracker.is_dirty(engine);
    let mut autosave_status = String::from("idle");

    if dirty && mark_dirty_session_flag(dirty_flag_path).is_err() {
        autosave_status = String::from("flag-error");
    }

    match autosave.save_if_due(engine, runtime.snapshot(), dirty, autosave_path) {
        Ok(true) => {
            dirty_tracker.mark_saved(engine);
            dirty = false;
            if clear_dirty_session_flag(dirty_flag_path).is_err() {
                autosave_status = format!("saved@{}+flag-error", autosave.last_saved_tick());
            } else {
                autosave_status = format!("saved@{}", autosave.last_saved_tick());
            }
        }
        Ok(false) => {
            if !dirty {
                let _ = clear_dirty_session_flag(dirty_flag_path);
            }
        }
        Err(_) => {
            autosave_status = String::from("error");
        }
    }

    format!(
        "dirty={} autosave={}",
        if dirty { "yes" } else { "no" },
        autosave_status
    )
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
    use super::{
        apply_shell_command, apply_shell_command_with_history, apply_shell_command_with_history_state,
        render_frame, ProjectHistory, ShellCommandResult, ShellEditState,
    };
    use crate::runtime::RuntimeCoordinator;
    use crate::ui::{UiAction, UiController, UiScreen};
    use p9_core::engine::{Engine, EngineCommand};
    use p9_core::model::FxCommand;
    use p9_rt::audio::{AudioBackend, NoopAudioBackend};
    use p9_rt::midi::NoopMidiOutput;

    #[test]
    fn render_frame_contains_shell_layout_sections() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);

        let snapshot = ui.snapshot(&engine, &runtime);
        let frame = render_frame(engine.snapshot(), snapshot, "ok");

        assert!(frame.contains("P9 Tracker UI Shell (Phase 16.3)"));
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

    #[test]
    fn shell_selection_copy_paste_transfers_step_block() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(32);
        let mut edit_state = ShellEditState::default();

        for command in ["c", "f", "i"] {
            let _ = apply_shell_command_with_history_state(
                command,
                &mut ui,
                &mut engine,
                &mut runtime,
                &mut history,
                &mut edit_state,
            )
            .unwrap();
        }

        for (step_index, note, velocity) in [(0usize, 60u8, 90u8), (1, 62, 91), (2, 64, 92)] {
            engine
                .apply_command(EngineCommand::SetPhraseStep {
                    phrase_id: 0,
                    step_index,
                    note: Some(note),
                    velocity,
                    instrument_id: Some(0),
                })
                .unwrap();
        }
        engine
            .apply_command(EngineCommand::SetStepFx {
                phrase_id: 0,
                step_index: 1,
                fx_slot: 0,
                fx: Some(FxCommand {
                    code: "VOL".to_string(),
                    value: 88,
                }),
            })
            .unwrap();

        ui.handle_action(UiAction::SelectStep(0), &mut engine, &mut runtime)
            .unwrap();
        let start = apply_shell_command_with_history_state(
            "a",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(
            start,
            ShellCommandResult::Continue(String::from("select -> start t0 phrase 0 step 00"))
        );

        ui.handle_action(UiAction::SelectStep(2), &mut engine, &mut runtime)
            .unwrap();
        let end = apply_shell_command_with_history_state(
            "z",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(
            end,
            ShellCommandResult::Continue(String::from("select -> range 00-02 len 3"))
        );

        let copy = apply_shell_command_with_history_state(
            "w",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(
            copy,
            ShellCommandResult::Continue(String::from("copy -> phrase 0 steps 00-02 len 3"))
        );

        ui.handle_action(UiAction::SelectStep(8), &mut engine, &mut runtime)
            .unwrap();
        let paste = apply_shell_command_with_history_state(
            "v",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(
            paste,
            ShellCommandResult::Continue(String::from("paste -> phrase 0 steps 08-10 len 3"))
        );

        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[8].note, Some(60));
        assert_eq!(phrase.steps[8].velocity, 90);
        assert_eq!(phrase.steps[9].note, Some(62));
        assert_eq!(phrase.steps[9].velocity, 91);
        assert_eq!(phrase.steps[10].note, Some(64));
        assert_eq!(phrase.steps[10].velocity, 92);
        assert_eq!(phrase.steps[9].fx[0].as_ref().unwrap().code, "VOL");
        assert_eq!(phrase.steps[9].fx[0].as_ref().unwrap().value, 88);

        let _ = apply_shell_command_with_history_state(
            "u",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(engine.snapshot().phrases.get(&0).unwrap().steps[8].note, None);

        let _ = apply_shell_command_with_history_state(
            "y",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(engine.snapshot().phrases.get(&0).unwrap().steps[8].note, Some(60));
    }

    #[test]
    fn shell_paste_clips_to_phrase_bounds() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(32);
        let mut edit_state = ShellEditState::default();

        for command in ["c", "f", "i"] {
            let _ = apply_shell_command_with_history_state(
                command,
                &mut ui,
                &mut engine,
                &mut runtime,
                &mut history,
                &mut edit_state,
            )
            .unwrap();
        }

        for (step_index, note) in [(0usize, 70u8), (1, 71), (2, 72), (3, 73)] {
            engine
                .apply_command(EngineCommand::SetPhraseStep {
                    phrase_id: 0,
                    step_index,
                    note: Some(note),
                    velocity: 100,
                    instrument_id: Some(0),
                })
                .unwrap();
        }

        ui.handle_action(UiAction::SelectStep(0), &mut engine, &mut runtime)
            .unwrap();
        let _ = apply_shell_command_with_history_state(
            "a",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        ui.handle_action(UiAction::SelectStep(3), &mut engine, &mut runtime)
            .unwrap();
        let _ = apply_shell_command_with_history_state(
            "z",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        let _ = apply_shell_command_with_history_state(
            "w",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();

        ui.handle_action(UiAction::SelectStep(14), &mut engine, &mut runtime)
            .unwrap();
        let paste = apply_shell_command_with_history_state(
            "v",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(
            paste,
            ShellCommandResult::Continue(String::from(
                "paste -> phrase 0 steps 14-15 len 2 (clipped 2)"
            ))
        );

        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[14].note, Some(70));
        assert_eq!(phrase.steps[15].note, Some(71));
    }

    #[test]
    fn shell_paste_warns_on_track_scope_mismatch() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(32);
        let mut edit_state = ShellEditState::default();

        for command in ["c", "f", "i"] {
            let _ = apply_shell_command_with_history_state(
                command,
                &mut ui,
                &mut engine,
                &mut runtime,
                &mut history,
                &mut edit_state,
            )
            .unwrap();
        }

        engine
            .apply_command(EngineCommand::SetPhraseStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(60),
                velocity: 100,
                instrument_id: Some(0),
            })
            .unwrap();

        let _ = apply_shell_command_with_history_state(
            "a",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        let _ = apply_shell_command_with_history_state(
            "w",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();

        let _ = apply_shell_command_with_history_state(
            "l",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        let _ = apply_shell_command_with_history_state(
            "c",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        let _ = apply_shell_command_with_history_state(
            "f",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        let warn = apply_shell_command_with_history_state(
            "v",
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        )
        .unwrap();
        assert_eq!(
            warn,
            ShellCommandResult::Continue(String::from(
                "warn: clipboard track mismatch; focus source track or recopy"
            ))
        );
    }

    #[test]
    fn shell_undo_redo_restores_edit_state() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(32);

        let _ = apply_shell_command_with_history("c", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        let _ = apply_shell_command_with_history("f", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        let _ = apply_shell_command_with_history("i", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        let _ = apply_shell_command_with_history("e", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        assert_eq!(engine.snapshot().phrases.get(&0).unwrap().steps[0].note, Some(60));

        let undo = apply_shell_command_with_history("u", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        assert_eq!(undo, ShellCommandResult::Continue(String::from("undo -> applied")));
        assert_eq!(engine.snapshot().phrases.get(&0).unwrap().steps[0].note, None);

        let redo = apply_shell_command_with_history("y", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        assert_eq!(redo, ShellCommandResult::Continue(String::from("redo -> applied")));
        assert_eq!(engine.snapshot().phrases.get(&0).unwrap().steps[0].note, Some(60));
    }

    #[test]
    fn shell_undo_redo_reports_empty_history() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("shell");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(8);

        let undo = apply_shell_command_with_history("u", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        assert_eq!(
            undo,
            ShellCommandResult::Continue(String::from("undo -> empty history"))
        );

        let redo = apply_shell_command_with_history("y", &mut ui, &mut engine, &mut runtime, &mut history)
            .unwrap();
        assert_eq!(
            redo,
            ShellCommandResult::Continue(String::from("redo -> empty history"))
        );
    }
}
