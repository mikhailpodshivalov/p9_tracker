use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::hardening::{
    clear_dirty_session_flag, default_autosave_path, default_dirty_flag_path,
    mark_dirty_session_flag, recover_from_dirty_session, AutosaveManager, AutosavePolicy,
    DirtyStateTracker, RecoveryStatus,
};
use crate::runtime::{RuntimeCommand, RuntimeCoordinator};
use crate::ui_shell::{
    apply_shell_command_with_history_state, ProjectHistory, SelectionRange, ShellCommandResult,
    ShellEditState,
};
use crate::ui::{UiAction, UiController, UiError, UiScreen, UiSnapshot};
use p9_core::engine::{Engine, EngineCommand};
use p9_core::model::{ProjectData, Step, CHAIN_ROW_COUNT, PHRASE_STEP_COUNT, SONG_ROW_COUNT};
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
use p9_rt::midi::NoopMidiOutput;
use p9_storage::project::ProjectEnvelope;

const BIND_ADDR_CANDIDATES: [&str; 5] = [
    "127.0.0.1:17717",
    "127.0.0.1:17718",
    "127.0.0.1:17719",
    "127.0.0.1:17720",
    "127.0.0.1:17721",
];
const TICK_SLEEP_MS: u64 = 16;
const GUI_AUTOSAVE_INTERVAL_TICKS: u64 = 16;
const GUI_HISTORY_LIMIT: usize = 128;
const SONG_VIEW_ROWS: usize = 8;
const CHAIN_VIEW_ROWS: usize = 8;
const RECENT_PROJECT_LIMIT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopControl {
    Continue,
    Quit,
}

#[derive(Clone, Debug)]
struct GuiSessionState {
    recovery: RecoveryStatus,
    dirty: bool,
    autosave_status: String,
    current_project_path: Option<PathBuf>,
    recent_project_paths: Vec<PathBuf>,
    history: ProjectHistory,
    edit_state: ShellEditState,
}

#[derive(Clone, Debug)]
struct SessionHardeningState {
    dirty: bool,
    autosave_status: String,
}

#[derive(Clone, Debug)]
struct ActionOutcome {
    status: String,
    quit: bool,
    confirm_required: bool,
}

impl GuiSessionState {
    fn new(recovery: RecoveryStatus) -> Self {
        Self {
            recovery,
            dirty: false,
            autosave_status: String::from("unknown"),
            current_project_path: None,
            recent_project_paths: Vec::new(),
            history: ProjectHistory::with_limit(GUI_HISTORY_LIMIT),
            edit_state: ShellEditState::default(),
        }
    }
}

pub fn run_web_shell(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> io::Result<()> {
    let autosave_path = default_autosave_path();
    let dirty_flag_path = default_dirty_flag_path();
    let recovery = recover_from_dirty_session(engine, &autosave_path, &dirty_flag_path);
    let mut dirty_tracker = DirtyStateTracker::from_engine(engine);
    let mut autosave = AutosaveManager::new(AutosavePolicy {
        interval_ticks: GUI_AUTOSAVE_INTERVAL_TICKS,
    });
    let mut session_state = GuiSessionState::new(recovery);

    let listener = bind_listener()?;
    listener.set_nonblocking(true)?;

    println!(
        "p9_tracker gui-shell stage19.2a running at http://{}",
        listener.local_addr()?
    );
    println!("Open this URL in browser. Press Ctrl+C or click Quit GUI Shell to stop.");

    let mut audio = NoopAudioBackend::default();
    audio.start();
    let mut midi_output = NoopMidiOutput::default();
    let hardening = update_session_hardening(
        engine,
        runtime,
        &mut dirty_tracker,
        &mut autosave,
        &autosave_path,
        &dirty_flag_path,
    );
    session_state.dirty = hardening.dirty;
    session_state.autosave_status = hardening.autosave_status;

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if handle_connection(
                    &mut stream,
                    ui,
                    engine,
                    runtime,
                    &mut session_state,
                    &mut dirty_tracker,
                )?
                    == LoopControl::Quit
                {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err),
        }

        let _ = runtime.run_tick_safe(engine, &mut audio, &mut midi_output);
        let hardening = update_session_hardening(
            engine,
            runtime,
            &mut dirty_tracker,
            &mut autosave,
            &autosave_path,
            &dirty_flag_path,
        );
        session_state.dirty = hardening.dirty;
        session_state.autosave_status = hardening.autosave_status;
        std::thread::sleep(Duration::from_millis(TICK_SLEEP_MS));
    }

    audio.stop();
    Ok(())
}

fn bind_listener() -> io::Result<TcpListener> {
    for addr in BIND_ADDR_CANDIDATES {
        if let Ok(listener) = TcpListener::bind(addr) {
            return Ok(listener);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        "unable to bind GUI shell listener",
    ))
}

fn handle_connection(
    stream: &mut TcpStream,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    session_state: &mut GuiSessionState,
    dirty_tracker: &mut DirtyStateTracker,
) -> io::Result<LoopControl> {
    let mut buffer = [0u8; 8192];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        return Ok(LoopControl::Continue);
    }

    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some((method, target)) = parse_request_line(request.lines().next().unwrap_or_default()) else {
        write_text_response(stream, 400, "text/plain; charset=utf-8", "bad request")?;
        return Ok(LoopControl::Continue);
    };

    let (path, query) = split_path_and_query(target);

    match (method, path) {
        ("GET", "/") => {
            write_text_response(stream, 200, "text/html; charset=utf-8", index_html())?;
            Ok(LoopControl::Continue)
        }
        ("GET", "/state") => {
            let body = build_state_json(ui, engine, runtime, session_state);
            write_text_response(stream, 200, "application/json; charset=utf-8", &body)?;
            Ok(LoopControl::Continue)
        }
        (_, "/action") => {
            let cmd = query_value(query, "cmd");
            let force = query_flag(query, "force");
            let path = query_value(query, "path").map(url_decode);

            let outcome = if let Some(name) = cmd {
                execute_action_command(
                    name,
                    query,
                    path.as_deref(),
                    force,
                    ui,
                    engine,
                    runtime,
                    session_state,
                    dirty_tracker,
                )
            } else {
                ActionOutcome {
                    status: String::from("warn: missing cmd parameter"),
                    quit: false,
                    confirm_required: false,
                }
            };

            let body = format!(
                "{{\"status\":\"{}\",\"quit\":{},\"confirm_required\":{}}}",
                json_escape(&outcome.status),
                if outcome.quit { "true" } else { "false" },
                if outcome.confirm_required {
                    "true"
                } else {
                    "false"
                },
            );
            write_text_response(stream, 200, "application/json; charset=utf-8", &body)?;
            Ok(if outcome.quit {
                LoopControl::Quit
            } else {
                LoopControl::Continue
            })
        }
        _ => {
            write_text_response(stream, 404, "text/plain; charset=utf-8", "not found")?;
            Ok(LoopControl::Continue)
        }
    }
}

fn execute_action_command(
    command: &str,
    query: Option<&str>,
    path: Option<&str>,
    force: bool,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    session_state: &mut GuiSessionState,
    dirty_tracker: &mut DirtyStateTracker,
) -> ActionOutcome {
    if requires_dirty_confirmation(command) && session_state.dirty && !force {
        return ActionOutcome {
            status: format!("warn: unsaved changes; confirm '{command}' with force=1"),
            quit: false,
            confirm_required: true,
        };
    }

    match command {
        "quit" => ActionOutcome {
            status: String::from("info: quitting gui shell"),
            quit: true,
            confirm_required: false,
        },
        "session_new" => {
            engine.replace_project(ProjectData::new("p9_tracker new song"));
            *ui = UiController::default();
            runtime.enqueue_commands([RuntimeCommand::Stop, RuntimeCommand::Rewind]);
            session_state.current_project_path = None;
            session_state.dirty = false;
            session_state.autosave_status = String::from("clean");
            session_state.history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
            session_state.edit_state = ShellEditState::default();
            dirty_tracker.mark_saved(engine);
            ActionOutcome {
                status: String::from("info: new project created"),
                quit: false,
                confirm_required: false,
            }
        }
        "session_open" => {
            let Some(target_path) = normalize_path(path) else {
                return ActionOutcome {
                    status: String::from("warn: open requires path parameter"),
                    quit: false,
                    confirm_required: false,
                };
            };

            match load_project_from_path(&target_path, engine) {
                Ok(()) => {
                    *ui = UiController::default();
                    runtime.enqueue_commands([RuntimeCommand::Stop, RuntimeCommand::Rewind]);
                    dirty_tracker.mark_saved(engine);
                    session_state.dirty = false;
                    session_state.autosave_status = String::from("loaded");
                    session_state.history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
                    session_state.edit_state = ShellEditState::default();
                    session_state.current_project_path = Some(target_path.clone());
                    register_recent_path(session_state, target_path);
                    ActionOutcome {
                        status: String::from("info: project opened"),
                        quit: false,
                        confirm_required: false,
                    }
                }
                Err(err) => ActionOutcome {
                    status: format!("error: open failed: {err}"),
                    quit: false,
                    confirm_required: false,
                },
            }
        }
        "session_save" | "session_save_as" => {
            let save_as = command == "session_save_as";
            let explicit_path = normalize_path(path);
            let target_path = if save_as {
                match explicit_path {
                    Some(path) => path,
                    None => {
                        return ActionOutcome {
                            status: String::from("warn: save-as requires path parameter"),
                            quit: false,
                            confirm_required: false,
                        };
                    }
                }
            } else if let Some(path) = explicit_path {
                path
            } else if let Some(path) = session_state.current_project_path.clone() {
                path
            } else {
                return ActionOutcome {
                    status: String::from("warn: no current path; use Save As with path"),
                    quit: false,
                    confirm_required: false,
                };
            };

            match save_project_to_path(&target_path, engine) {
                Ok(()) => {
                    dirty_tracker.mark_saved(engine);
                    session_state.dirty = false;
                    session_state.current_project_path = Some(target_path.clone());
                    register_recent_path(session_state, target_path);
                    session_state.autosave_status =
                        format!("saved-manual@{}", runtime.snapshot().tick);
                    ActionOutcome {
                        status: String::from("info: project saved"),
                        quit: false,
                        confirm_required: false,
                    }
                }
                Err(err) => ActionOutcome {
                    status: format!("error: save failed: {err}"),
                    quit: false,
                    confirm_required: false,
                },
            }
        }
        "session_recent" => {
            let recent = session_state
                .recent_project_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            ActionOutcome {
                status: if recent.is_empty() {
                    String::from("info: recent projects empty")
                } else {
                    format!("info: recent: {}", recent.join(" | "))
                },
                quit: false,
                confirm_required: false,
            }
        }
        _ => ActionOutcome {
            status: apply_gui_command_with_query(
                command,
                query,
                ui,
                engine,
                runtime,
                &mut session_state.history,
                &mut session_state.edit_state,
            ),
            quit: false,
            confirm_required: false,
        },
    }
}

fn requires_dirty_confirmation(command: &str) -> bool {
    matches!(command, "quit" | "session_new" | "session_open")
}

fn normalize_path(path: Option<&str>) -> Option<PathBuf> {
    let value = path?.trim();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

fn save_project_to_path(path: &Path, engine: &Engine) -> Result<(), String> {
    let envelope = ProjectEnvelope::new(engine.snapshot().clone());
    std::fs::write(path, envelope.to_text()).map_err(|err| err.to_string())
}

fn load_project_from_path(path: &Path, engine: &mut Engine) -> Result<(), String> {
    let source = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let envelope = ProjectEnvelope::from_text(&source).map_err(|err| format!("{err:?}"))?;
    envelope
        .validate_format()
        .map_err(|err| format!("{err:?}"))?;
    engine.replace_project(envelope.project);
    Ok(())
}

fn register_recent_path(session_state: &mut GuiSessionState, path: PathBuf) {
    session_state.recent_project_paths.retain(|item| item != &path);
    session_state.recent_project_paths.insert(0, path);
    if session_state.recent_project_paths.len() > RECENT_PROJECT_LIMIT {
        session_state.recent_project_paths.truncate(RECENT_PROJECT_LIMIT);
    }
}

#[cfg(test)]
fn apply_gui_command(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> String {
    let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
    let mut edit_state = ShellEditState::default();
    apply_gui_command_with_query(
        command,
        None,
        ui,
        engine,
        runtime,
        &mut history,
        &mut edit_state,
    )
}

fn apply_gui_command_with_query(
    command: &str,
    query: Option<&str>,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    match command {
        "play" => {
            runtime.enqueue_command(RuntimeCommand::Start);
            return String::from("info: transport start queued");
        }
        "stop" => {
            runtime.enqueue_command(RuntimeCommand::Stop);
            return String::from("info: transport stop queued");
        }
        "screen_song" => {
            return apply_screen_target(UiScreen::Song, ui, engine, runtime);
        }
        "screen_chain" => {
            return apply_screen_target(UiScreen::Chain, ui, engine, runtime);
        }
        "screen_phrase" => {
            return apply_screen_target(UiScreen::Phrase, ui, engine, runtime);
        }
        "screen_mixer" => {
            return apply_screen_target(UiScreen::Mixer, ui, engine, runtime);
        }
        "step_prev_fine" => {
            return apply_phrase_step_shift(-1, ui, engine, runtime);
        }
        "step_next_fine" => {
            return apply_phrase_step_shift(1, ui, engine, runtime);
        }
        "edit_focus_prepare" => {
            return apply_edit_focus_prepare(ui, engine, runtime, history, edit_state);
        }
        "edit_bind_chain" => {
            return run_shell_edit_command("c", ui, engine, runtime, history, edit_state);
        }
        "edit_bind_phrase" => {
            return run_shell_edit_command("f", ui, engine, runtime, history, edit_state);
        }
        "edit_ensure_instrument" => {
            return run_shell_edit_command("i", ui, engine, runtime, history, edit_state);
        }
        "edit_select_start" => {
            return run_shell_edit_command("a", ui, engine, runtime, history, edit_state);
        }
        "edit_select_end" => {
            return run_shell_edit_command("z", ui, engine, runtime, history, edit_state);
        }
        "edit_copy" => {
            return run_shell_edit_command("w", ui, engine, runtime, history, edit_state);
        }
        "edit_paste_safe" => {
            return run_shell_edit_command("v", ui, engine, runtime, history, edit_state);
        }
        "edit_paste_force" => {
            return run_shell_edit_command("V", ui, engine, runtime, history, edit_state);
        }
        "edit_clear_selection" => {
            return run_shell_edit_command("x", ui, engine, runtime, history, edit_state);
        }
        "edit_undo" => {
            return run_shell_edit_command("u", ui, engine, runtime, history, edit_state);
        }
        "edit_redo" => {
            return run_shell_edit_command("y", ui, engine, runtime, history, edit_state);
        }
        "edit_power_duplicate" => {
            return apply_power_duplicate(
                query, ui, engine, runtime, history, edit_state,
            );
        }
        "edit_power_fill" => {
            return apply_power_fill(
                query, ui, engine, runtime, history, edit_state,
            );
        }
        "edit_power_clear_range" => {
            return apply_power_clear_range(
                ui, engine, runtime, history, edit_state,
            );
        }
        "edit_power_transpose" => {
            return apply_power_transpose(
                query, ui, engine, runtime, history, edit_state,
            );
        }
        "edit_power_transpose_up" => {
            return apply_power_transpose(
                Some("delta=1"),
                ui,
                engine,
                runtime,
                history,
                edit_state,
            );
        }
        "edit_power_transpose_down" => {
            return apply_power_transpose(
                Some("delta=-1"),
                ui,
                engine,
                runtime,
                history,
                edit_state,
            );
        }
        "edit_power_rotate" => {
            return apply_power_rotate(
                query, ui, engine, runtime, history, edit_state,
            );
        }
        "edit_power_rotate_right" => {
            return apply_power_rotate(
                Some("shift=1"),
                ui,
                engine,
                runtime,
                history,
                edit_state,
            );
        }
        "edit_power_rotate_left" => {
            return apply_power_rotate(
                Some("shift=-1"),
                ui,
                engine,
                runtime,
                history,
                edit_state,
            );
        }
        "edit_song_clone_prev" => {
            return apply_song_clone_prev(
                ui, engine, runtime, history,
            );
        }
        "edit_chain_clone_prev" => {
            return apply_chain_clone_prev(
                ui, engine, runtime, history,
            );
        }
        "edit_write_step" => {
            let custom_edit = query_flag(query, "clear")
                || query_value(query, "note").is_some()
                || query_value(query, "velocity").is_some()
                || query_value(query, "instrument").is_some();
            if !custom_edit {
                return run_shell_edit_command("e", ui, engine, runtime, history, edit_state);
            }

            let snapshot = ui.snapshot(engine, runtime);
            let focused_instrument = snapshot.focused_track as u8;
            let clear = query_flag(query, "clear");

            let instrument_id = query_value(query, "instrument")
                .and_then(parse_u8_field)
                .unwrap_or(focused_instrument);
            let velocity = query_value(query, "velocity")
                .and_then(parse_u8_field)
                .unwrap_or(100);
            let note = if clear {
                None
            } else {
                Some(
                    query_value(query, "note")
                        .and_then(parse_u8_field)
                        .unwrap_or_else(|| seeded_note(snapshot.selected_step)),
                )
            };

            if !clear && !engine.snapshot().instruments.contains_key(&instrument_id) {
                return format!(
                    "warn: instrument {} missing; run edit_ensure_instrument first",
                    instrument_id
                );
            }

            let Some(phrase_id) = bound_phrase_id(engine.snapshot(), snapshot) else {
                return String::from(
                    "warn: no phrase on selected chain row; run edit_bind_phrase first",
                );
            };

            let before = engine.snapshot().clone();

            if let Err(err) = ui.handle_action(UiAction::SelectPhrase(phrase_id), engine, runtime) {
                return format!("error: action 'edit_write_step' failed: {}", ui_error_label(err));
            }
            if let Err(err) = ui.handle_action(
                UiAction::EditStep {
                    phrase_id,
                    step_index: snapshot.selected_step,
                    note,
                    velocity: if clear { 0x40 } else { velocity },
                    instrument_id: if clear { None } else { Some(instrument_id) },
                },
                engine,
                runtime,
                ) {
                return format!("error: action 'edit_write_step' failed: {}", ui_error_label(err));
            }
            history.record_change(before);

            if clear {
                return format!(
                    "info: edit -> phrase {} step {} cleared",
                    phrase_id, snapshot.selected_step
                );
            }

            return format!(
                "info: edit -> phrase {} step {} note {} vel {} ins {}",
                phrase_id,
                snapshot.selected_step,
                note.unwrap_or_default(),
                velocity,
                instrument_id
            );
        }
        _ => {}
    }

    let action = match command {
        "toggle_play" => Some(UiAction::TogglePlayStop),
        "rewind" => Some(UiAction::RewindTransport),
        "screen_next" => Some(UiAction::NextScreen),
        "screen_prev" => Some(UiAction::PrevScreen),
        "track_left" => Some(UiAction::FocusTrackLeft),
        "track_right" => Some(UiAction::FocusTrackRight),
        "cursor_up" => Some(cursor_up_action(ui.snapshot(engine, runtime))),
        "cursor_down" => Some(cursor_down_action(ui.snapshot(engine, runtime))),
        "toggle_scale" => Some(UiAction::ToggleScaleHighlight),
        _ => None,
    };

    let Some(action) = action else {
        return polish_status_for_gui(format!(
            "warn: unknown action '{command}'; use visible buttons or keyboard shortcuts"
        ));
    };

    match ui.handle_action(action, engine, runtime) {
        Ok(()) => format!("info: action '{command}' applied"),
        Err(err) => format!("error: action '{command}' failed: {}", ui_error_label(err)),
    }
}

fn run_shell_edit_command(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    match apply_shell_command_with_history_state(command, ui, engine, runtime, history, edit_state) {
        Ok(ShellCommandResult::Continue(message)) => normalize_status(message),
        Ok(ShellCommandResult::Exit) => String::from("warn: exit command ignored in gui shell"),
        Err(err) => format!("error: action '{command}' failed: {}", ui_error_label(err)),
    }
}

fn apply_screen_target(
    target: UiScreen,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> String {
    for _ in 0..4 {
        let snapshot = ui.snapshot(engine, runtime);
        if snapshot.screen == target {
            return format!("info: screen -> {}", ui_screen_tab_label(target));
        }
        if let Err(err) = ui.handle_action(UiAction::NextScreen, engine, runtime) {
            return format!("error: action 'screen' failed: {}", ui_error_label(err));
        }
    }

    format!(
        "error: action 'screen' failed: unable to reach target {}",
        ui_screen_tab_label(target)
    )
}

fn apply_phrase_step_shift(
    delta: i8,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    let next = if delta >= 0 {
        (snapshot.selected_step + 1) % PHRASE_STEP_COUNT
    } else {
        (snapshot.selected_step + PHRASE_STEP_COUNT - 1) % PHRASE_STEP_COUNT
    };

    match ui.handle_action(UiAction::SelectStep(next), engine, runtime) {
        Ok(()) => format!("info: cursor -> step {:02}", next),
        Err(err) => format!("error: action 'step_shift' failed: {}", ui_error_label(err)),
    }
}

fn apply_edit_focus_prepare(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    let mut snapshot = ui.snapshot(engine, runtime);

    if bound_chain_id(engine.snapshot(), snapshot).is_none() {
        let status = run_shell_edit_command("c", ui, engine, runtime, history, edit_state);
        if !status.starts_with("info:") {
            return status;
        }
        snapshot = ui.snapshot(engine, runtime);
    }

    if bound_phrase_id(engine.snapshot(), snapshot).is_none() {
        let status = run_shell_edit_command("f", ui, engine, runtime, history, edit_state);
        if !status.starts_with("info:") {
            return status;
        }
        snapshot = ui.snapshot(engine, runtime);
    }

    let instrument_id = snapshot.focused_track as u8;
    if !engine.snapshot().instruments.contains_key(&instrument_id) {
        let status = run_shell_edit_command("i", ui, engine, runtime, history, edit_state);
        if !status.starts_with("info:") {
            return status;
        }
    }

    let screen_status = apply_screen_target(UiScreen::Phrase, ui, engine, runtime);
    if !screen_status.starts_with("info:") {
        return screen_status;
    }

    snapshot = ui.snapshot(engine, runtime);
    let chain = bound_chain_id(engine.snapshot(), snapshot);
    let phrase = bound_phrase_id(engine.snapshot(), snapshot);
    format!(
        "info: focus -> phrase editor ready (chain {} phrase {} ins {:02})",
        option_u8_label(chain),
        option_u8_label(phrase),
        instrument_id
    )
}

fn apply_power_duplicate(
    query: Option<&str>,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    let selection = match resolve_selection_range(engine.snapshot(), snapshot, edit_state) {
        Ok(selection) => selection,
        Err(status) => return status,
    };
    let target_step = selection.end_step + 1;
    if target_step >= PHRASE_STEP_COUNT {
        return String::from("warn: duplicate target out of range; move selection earlier");
    }

    if let Err(err) = ui.handle_action(UiAction::SelectStep(target_step), engine, runtime) {
        return format!(
            "error: action 'edit_power_duplicate' failed: {}",
            ui_error_label(err)
        );
    }

    let copy_status = run_shell_edit_command("w", ui, engine, runtime, history, edit_state);
    if !copy_status.starts_with("info:") {
        return copy_status;
    }

    let paste_status = run_shell_edit_command("v", ui, engine, runtime, history, edit_state);
    if query_flag(query, "force")
        && paste_status.starts_with("warn:")
        && paste_status.contains("run V")
    {
        return run_shell_edit_command("V", ui, engine, runtime, history, edit_state);
    }

    paste_status
}

fn apply_power_fill(
    query: Option<&str>,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    let selection = match resolve_selection_range(engine.snapshot(), snapshot, edit_state) {
        Ok(selection) => selection,
        Err(status) => return status,
    };

    let instrument_id = query_value(query, "instrument")
        .and_then(parse_u8_field)
        .unwrap_or(snapshot.focused_track as u8);
    if !engine.snapshot().instruments.contains_key(&instrument_id) {
        return format!(
            "warn: instrument {} missing; run edit_ensure_instrument first",
            instrument_id
        );
    }
    let velocity = query_value(query, "velocity")
        .and_then(parse_u8_field)
        .unwrap_or(100)
        .max(1);
    let note_override = query_value(query, "note").and_then(parse_u8_field);

    let before = engine.snapshot().clone();
    if let Err(err) = ui.handle_action(UiAction::SelectPhrase(selection.phrase_id), engine, runtime) {
        return format!("error: action 'edit_power_fill' failed: {}", ui_error_label(err));
    }

    for step_index in selection.start_step..=selection.end_step {
        let note = note_override.unwrap_or_else(|| seeded_note(step_index));
        if let Err(err) = ui.handle_action(
            UiAction::EditStep {
                phrase_id: selection.phrase_id,
                step_index,
                note: Some(note),
                velocity,
                instrument_id: Some(instrument_id),
            },
            engine,
            runtime,
        ) {
            return format!("error: action 'edit_power_fill' failed: {}", ui_error_label(err));
        }
    }

    history.record_change(before);
    if let Some(note) = note_override {
        format!(
            "info: fill -> phrase {} steps {:02}-{:02} note {} vel {} ins {}",
            selection.phrase_id, selection.start_step, selection.end_step, note, velocity, instrument_id
        )
    } else {
        format!(
            "info: fill -> phrase {} steps {:02}-{:02} seeded vel {} ins {}",
            selection.phrase_id, selection.start_step, selection.end_step, velocity, instrument_id
        )
    }
}

fn apply_power_clear_range(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    let selection = match resolve_selection_range(engine.snapshot(), snapshot, edit_state) {
        Ok(selection) => selection,
        Err(status) => return status,
    };

    let before = engine.snapshot().clone();
    if let Err(err) = ui.handle_action(UiAction::SelectPhrase(selection.phrase_id), engine, runtime) {
        return format!(
            "error: action 'edit_power_clear_range' failed: {}",
            ui_error_label(err)
        );
    }
    for step_index in selection.start_step..=selection.end_step {
        if let Err(err) = write_step(engine, selection.phrase_id, step_index, &Step::default()) {
            return format!(
                "error: action 'edit_power_clear_range' failed: {}",
                ui_error_label(err)
            );
        }
    }

    history.record_change(before);
    format!(
        "info: clear -> phrase {} steps {:02}-{:02}",
        selection.phrase_id, selection.start_step, selection.end_step
    )
}

fn apply_power_transpose(
    query: Option<&str>,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    let selection = match resolve_selection_range(engine.snapshot(), snapshot, edit_state) {
        Ok(selection) => selection,
        Err(status) => return status,
    };

    let delta = query_value(query, "delta")
        .and_then(parse_i16_field)
        .unwrap_or(1)
        .clamp(-48, 48);
    if delta == 0 {
        return String::from("warn: transpose delta is zero; nothing to do");
    }

    let Some(phrase) = engine.snapshot().phrases.get(&selection.phrase_id) else {
        return String::from("warn: selected phrase missing; run edit_bind_phrase first");
    };

    let mut steps = phrase.steps[selection.start_step..=selection.end_step].to_vec();
    let mut note_count = 0usize;
    for step in &mut steps {
        if let Some(note) = step.note {
            let shifted = (note as i16 + delta).clamp(0, 127) as u8;
            step.note = Some(shifted);
            note_count += 1;
        }
    }
    if note_count == 0 {
        return String::from("warn: transpose skipped; no notes in selection");
    }

    let before = engine.snapshot().clone();
    for (offset, step) in steps.iter().enumerate() {
        if let Err(err) = write_step(engine, selection.phrase_id, selection.start_step + offset, step) {
            return format!(
                "error: action 'edit_power_transpose' failed: {}",
                ui_error_label(err)
            );
        }
    }

    history.record_change(before);
    format!(
        "info: transpose -> phrase {} steps {:02}-{:02} delta {:+} notes {}",
        selection.phrase_id, selection.start_step, selection.end_step, delta, note_count
    )
}

fn apply_power_rotate(
    query: Option<&str>,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
    edit_state: &mut ShellEditState,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    let selection = match resolve_selection_range(engine.snapshot(), snapshot, edit_state) {
        Ok(selection) => selection,
        Err(status) => return status,
    };

    let len = selection.end_step - selection.start_step + 1;
    if len < 2 {
        return String::from("warn: rotate needs at least two steps selected");
    }
    let shift = query_value(query, "shift")
        .and_then(parse_i16_field)
        .unwrap_or(1);
    let normalized = shift.rem_euclid(len as i16) as usize;
    if normalized == 0 {
        return String::from("warn: rotate shift is zero for this selection length");
    }

    let Some(phrase) = engine.snapshot().phrases.get(&selection.phrase_id) else {
        return String::from("warn: selected phrase missing; run edit_bind_phrase first");
    };
    let mut steps = phrase.steps[selection.start_step..=selection.end_step].to_vec();
    steps.rotate_right(normalized);

    let before = engine.snapshot().clone();
    for (offset, step) in steps.iter().enumerate() {
        if let Err(err) = write_step(engine, selection.phrase_id, selection.start_step + offset, step) {
            return format!("error: action 'edit_power_rotate' failed: {}", ui_error_label(err));
        }
    }

    history.record_change(before);
    format!(
        "info: rotate -> phrase {} steps {:02}-{:02} shift {:+}",
        selection.phrase_id, selection.start_step, selection.end_step, shift
    )
}

fn apply_song_clone_prev(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    if snapshot.selected_song_row == 0 {
        return String::from("warn: song row 00 has no previous row to clone");
    }

    let project = engine.snapshot();
    let Some(track) = project.song.tracks.get(snapshot.focused_track) else {
        return String::from("error: focused track missing");
    };
    let source_chain = track.song_rows[snapshot.selected_song_row - 1];
    let current_chain = track.song_rows[snapshot.selected_song_row];
    if source_chain == current_chain {
        return String::from("warn: song row already matches previous row");
    }

    let before = project.clone();
    if let Err(err) = ui.handle_action(
        UiAction::BindTrackRowToChain {
            song_row: snapshot.selected_song_row,
            chain_id: source_chain,
        },
        engine,
        runtime,
    ) {
        return format!("error: action 'edit_song_clone_prev' failed: {}", ui_error_label(err));
    }

    history.record_change(before);
    format!(
        "info: song clone -> row {} chain {}",
        snapshot.selected_song_row,
        option_u8_label(source_chain)
    )
}

fn apply_chain_clone_prev(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
    history: &mut ProjectHistory,
) -> String {
    let snapshot = ui.snapshot(engine, runtime);
    if snapshot.selected_chain_row == 0 {
        return String::from("warn: chain row 00 has no previous row to clone");
    }
    let Some(chain_id) = bound_chain_id(engine.snapshot(), snapshot) else {
        return String::from("warn: no chain on selected song row; run edit_bind_chain first");
    };

    let Some(chain) = engine.snapshot().chains.get(&chain_id) else {
        return format!("warn: chain {} missing; run edit_bind_chain first", chain_id);
    };
    let source_row = chain.rows[snapshot.selected_chain_row - 1].clone();
    let current_row = chain.rows[snapshot.selected_chain_row].clone();
    if source_row.phrase_id == current_row.phrase_id && source_row.transpose == current_row.transpose
    {
        return String::from("warn: chain row already matches previous row");
    }

    let before = engine.snapshot().clone();
    if let Err(err) = ui.handle_action(
        UiAction::BindChainRowToPhrase {
            chain_id,
            chain_row: snapshot.selected_chain_row,
            phrase_id: source_row.phrase_id,
            transpose: source_row.transpose,
        },
        engine,
        runtime,
    ) {
        return format!(
            "error: action 'edit_chain_clone_prev' failed: {}",
            ui_error_label(err)
        );
    }

    history.record_change(before);
    format!(
        "info: chain clone -> chain {} row {} phrase {} trn {:+}",
        chain_id,
        snapshot.selected_chain_row,
        option_u8_label(source_row.phrase_id),
        source_row.transpose
    )
}

fn normalize_status(message: String) -> String {
    let normalized =
        if message.starts_with("info:") || message.starts_with("warn:") || message.starts_with("error:")
        {
            message
        } else {
            format!("info: {message}")
        };
    polish_status_for_gui(normalized)
}

fn polish_status_for_gui(message: String) -> String {
    let mut polished = message;
    let replacements = [
        ("run c first", "click Bind Chain (c) first"),
        ("run f first", "click Bind Phrase (f) first"),
        ("run i first", "click Ensure Inst (i) first"),
        ("run a first", "click Select Start (a) first"),
        ("run w first", "click Copy (w) first"),
        ("run v first", "click Paste Safe (v) first"),
        (
            "run V to confirm overwrite",
            "press Paste Force (Shift+V) to confirm overwrite",
        ),
    ];
    for (from, to) in replacements {
        polished = polished.replace(from, to);
    }

    polished
}

fn cursor_down_action(snapshot: UiSnapshot) -> UiAction {
    match snapshot.screen {
        UiScreen::Song => UiAction::SelectSongRow((snapshot.selected_song_row + 1) % SONG_ROW_COUNT),
        UiScreen::Chain => UiAction::SelectChainRow((snapshot.selected_chain_row + 1) % CHAIN_ROW_COUNT),
        UiScreen::Phrase => UiAction::SelectStep((snapshot.selected_step + 4) % PHRASE_STEP_COUNT),
        UiScreen::Mixer => UiAction::FocusTrackRight,
    }
}

fn cursor_up_action(snapshot: UiSnapshot) -> UiAction {
    match snapshot.screen {
        UiScreen::Song => {
            let row = if snapshot.selected_song_row == 0 {
                SONG_ROW_COUNT - 1
            } else {
                snapshot.selected_song_row - 1
            };
            UiAction::SelectSongRow(row)
        }
        UiScreen::Chain => {
            let row = if snapshot.selected_chain_row == 0 {
                CHAIN_ROW_COUNT - 1
            } else {
                snapshot.selected_chain_row - 1
            };
            UiAction::SelectChainRow(row)
        }
        UiScreen::Phrase => {
            UiAction::SelectStep((snapshot.selected_step + PHRASE_STEP_COUNT - 4) % PHRASE_STEP_COUNT)
        }
        UiScreen::Mixer => UiAction::FocusTrackLeft,
    }
}

fn ui_screen_tab_label(screen: UiScreen) -> &'static str {
    match screen {
        UiScreen::Song => "SONG",
        UiScreen::Chain => "CHAIN",
        UiScreen::Phrase => "PHRASE",
        UiScreen::Mixer => "MIXER",
    }
}

fn build_state_json(
    ui: &UiController,
    engine: &Engine,
    runtime: &RuntimeCoordinator,
    session_state: &GuiSessionState,
) -> String {
    let ui_snapshot = ui.snapshot(engine, runtime);
    let transport = runtime.snapshot();
    let project = engine.snapshot();

    let song_view = build_song_view_json(project, ui_snapshot);
    let chain_view = build_chain_view_json(project, ui_snapshot);
    let phrase_view = build_phrase_view_json(project, ui_snapshot);
    let mixer_view = build_mixer_view_json(project, ui_snapshot);
    let session_json = build_session_json(session_state);
    let editor_json = build_editor_json(project, ui_snapshot, session_state);

    format!(
        "{{\"screen\":\"{}\",\"transport\":{{\"tick\":{},\"playing\":{},\"tempo\":{}}},\"cursor\":{{\"track\":{},\"song_row\":{},\"chain_row\":{},\"phrase_id\":{},\"step\":{},\"track_level\":{}}},\"status\":{{\"transport\":\"{}\",\"recovery\":\"{}\",\"dirty\":{},\"autosave\":\"{}\",\"queued_commands\":{},\"processed_commands\":{}}},\"session\":{},\"editor\":{},\"scale_highlight\":\"{:?}\",\"views\":{{\"song\":{},\"chain\":{},\"phrase\":{},\"mixer\":{}}}}}",
        screen_label(ui_snapshot.screen),
        transport.tick,
        transport.is_playing,
        project.song.tempo,
        ui_snapshot.focused_track,
        ui_snapshot.selected_song_row,
        ui_snapshot.selected_chain_row,
        ui_snapshot.selected_phrase_id,
        ui_snapshot.selected_step,
        ui_snapshot.focused_track_level,
        transport_label(transport.is_playing),
        session_state.recovery.label(),
        session_state.dirty,
        json_escape(&session_state.autosave_status),
        transport.queued_commands,
        transport.processed_commands,
        session_json,
        editor_json,
        ui_snapshot.scale_highlight,
        song_view,
        chain_view,
        phrase_view,
        mixer_view,
    )
}

fn update_session_hardening(
    engine: &Engine,
    runtime: &RuntimeCoordinator,
    dirty_tracker: &mut DirtyStateTracker,
    autosave: &mut AutosaveManager,
    autosave_path: &Path,
    dirty_flag_path: &Path,
) -> SessionHardeningState {
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

    if autosave_status == "idle" {
        autosave_status = if dirty {
            String::from("pending")
        } else {
            String::from("clean")
        };
    }

    SessionHardeningState {
        dirty,
        autosave_status,
    }
}

fn build_session_json(session_state: &GuiSessionState) -> String {
    format!(
        "{{\"current_path\":{},\"recent\":[{}]}}",
        option_path_json(session_state.current_project_path.as_deref()),
        recent_paths_json(&session_state.recent_project_paths)
    )
}

fn build_editor_json(project: &ProjectData, snapshot: UiSnapshot, session_state: &GuiSessionState) -> String {
    let bound_chain = bound_chain_id(project, snapshot);
    let bound_phrase = bound_phrase_id(project, snapshot);
    let focused_instrument = snapshot.focused_track as u8;
    let instrument_ready = project.instruments.contains_key(&focused_instrument);
    let selection = session_state.edit_state.selection_range();

    format!(
        "{{\"target\":\"{}\",\"focused_instrument\":{},\"instrument_ready\":{},\"bound_chain_id\":{},\"bound_phrase_id\":{},\"undo_depth\":{},\"redo_depth\":{},\"selection_active\":{},\"selection_start\":{},\"selection_end\":{},\"clipboard_ready\":{},\"clipboard_len\":{},\"overwrite_guard\":{}}}",
        json_escape(&editor_target_label(snapshot, bound_chain, bound_phrase)),
        focused_instrument,
        instrument_ready,
        option_u8_json(bound_chain),
        option_u8_json(bound_phrase),
        session_state.history.undo_depth(),
        session_state.history.redo_depth(),
        session_state.edit_state.has_selection(),
        option_usize_json(selection.map(|item| item.start_step)),
        option_usize_json(selection.map(|item| item.end_step)),
        session_state.edit_state.has_clipboard(),
        session_state.edit_state.clipboard_len(),
        session_state.edit_state.has_overwrite_guard(),
    )
}

fn build_song_view_json(project: &ProjectData, snapshot: UiSnapshot) -> String {
    let window_start = centered_window_start(snapshot.selected_song_row, SONG_ROW_COUNT, SONG_VIEW_ROWS);
    let window_end = window_start + SONG_VIEW_ROWS - 1;
    let mut rows = String::new();

    if let Some(track) = project.song.tracks.get(snapshot.focused_track) {
        for row in window_start..=window_end {
            if !rows.is_empty() {
                rows.push(',');
            }

            let chain_id = track.song_rows.get(row).copied().flatten();
            rows.push_str(&format!(
                "{{\"row\":{},\"chain_id\":{},\"selected\":{}}}",
                row,
                option_u8_json(chain_id),
                row == snapshot.selected_song_row,
            ));
        }
    }

    format!(
        "{{\"window_start\":{},\"window_end\":{},\"rows\":[{}]}}",
        window_start, window_end, rows
    )
}

fn build_chain_view_json(project: &ProjectData, snapshot: UiSnapshot) -> String {
    let bound_chain = bound_chain_id(project, snapshot);
    let window_start = centered_window_start(snapshot.selected_chain_row, CHAIN_ROW_COUNT, CHAIN_VIEW_ROWS);
    let window_end = window_start + CHAIN_VIEW_ROWS - 1;
    let mut rows = String::new();
    let mut exists = false;

    if let Some(chain_id) = bound_chain {
        if let Some(chain) = project.chains.get(&chain_id) {
            exists = true;
            for row in window_start..=window_end {
                if !rows.is_empty() {
                    rows.push(',');
                }

                let phrase_id = chain.rows.get(row).and_then(|entry| entry.phrase_id);
                let transpose = chain.rows.get(row).map(|entry| entry.transpose).unwrap_or(0);

                rows.push_str(&format!(
                    "{{\"row\":{},\"phrase_id\":{},\"transpose\":{},\"selected\":{}}}",
                    row,
                    option_u8_json(phrase_id),
                    transpose,
                    row == snapshot.selected_chain_row,
                ));
            }
        }
    }

    format!(
        "{{\"bound_chain_id\":{},\"exists\":{},\"window_start\":{},\"window_end\":{},\"rows\":[{}]}}",
        option_u8_json(bound_chain),
        exists,
        window_start,
        window_end,
        rows,
    )
}

fn build_phrase_view_json(project: &ProjectData, snapshot: UiSnapshot) -> String {
    let phrase_id = snapshot.selected_phrase_id;
    let bound_phrase = bound_phrase_id(project, snapshot);
    let phrase = project.phrases.get(&phrase_id);
    let mut rows = String::new();

    for step_index in 0..PHRASE_STEP_COUNT {
        if !rows.is_empty() {
            rows.push(',');
        }

        let (note, velocity, instrument_id, fx_label) = if let Some(phrase) = phrase {
            if let Some(step) = phrase.steps.get(step_index) {
                (
                    step.note,
                    step.velocity,
                    step.instrument_id,
                    step_fx_label(step),
                )
            } else {
                (None, 0x40, None, String::from("--"))
            }
        } else {
            (None, 0x40, None, String::from("--"))
        };

        rows.push_str(&format!(
            "{{\"step\":{},\"note\":{},\"velocity\":{},\"instrument_id\":{},\"fx\":\"{}\",\"scale\":\"{}\",\"selected\":{}}}",
            step_index,
            option_u8_json(note),
            velocity,
            option_u8_json(instrument_id),
            json_escape(&fx_label),
            step_scale_label(project, snapshot, note),
            step_index == snapshot.selected_step,
        ));
    }

    format!(
        "{{\"selected_phrase_id\":{},\"bound_phrase_id\":{},\"exists\":{},\"rows\":[{}]}}",
        phrase_id,
        option_u8_json(bound_phrase),
        phrase.is_some(),
        rows,
    )
}

fn build_mixer_view_json(project: &ProjectData, snapshot: UiSnapshot) -> String {
    let mut tracks = String::new();

    for (track_index, level) in project.mixer.track_levels.iter().enumerate() {
        if !tracks.is_empty() {
            tracks.push(',');
        }
        tracks.push_str(&format!(
            "{{\"track\":{},\"level\":{},\"focused\":{}}}",
            track_index,
            level,
            track_index == snapshot.focused_track,
        ));
    }

    format!(
        "{{\"master_level\":{},\"send_mfx\":{},\"send_delay\":{},\"send_reverb\":{},\"tracks\":[{}]}}",
        project.mixer.master_level,
        project.mixer.send_levels.mfx,
        project.mixer.send_levels.delay,
        project.mixer.send_levels.reverb,
        tracks,
    )
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

fn bound_phrase_id(project: &ProjectData, snapshot: UiSnapshot) -> Option<u8> {
    let chain_id = bound_chain_id(project, snapshot)?;
    project
        .chains
        .get(&chain_id)
        .and_then(|chain| chain.rows.get(snapshot.selected_chain_row))
        .and_then(|row| row.phrase_id)
}

fn resolve_selection_range(
    project: &ProjectData,
    snapshot: UiSnapshot,
    edit_state: &ShellEditState,
) -> Result<SelectionRange, String> {
    let Some(selection) = edit_state.selection_range() else {
        return Err(String::from("warn: selection start missing; run Select Start first"));
    };
    let Some(bound_phrase) = bound_phrase_id(project, snapshot) else {
        return Err(String::from(
            "warn: no phrase on selected chain row; run edit_bind_phrase first",
        ));
    };
    if selection.track_index != snapshot.focused_track || selection.phrase_id != bound_phrase {
        return Err(String::from(
            "warn: selection scope changed; run Select Start again",
        ));
    }
    Ok(selection)
}

fn write_step(
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

fn step_fx_label(step: &Step) -> String {
    let mut slots = Vec::new();

    for fx in step.fx.iter().flatten() {
        slots.push(format!("{}{:03}", fx.code, fx.value));
    }

    if slots.is_empty() {
        String::from("--")
    } else {
        slots.join(" ")
    }
}

fn step_scale_label(project: &ProjectData, snapshot: UiSnapshot, note: Option<u8>) -> &'static str {
    let Some(note) = note else {
        return "none";
    };

    match is_note_in_track_scale(project, snapshot, note) {
        Some(true) => "in",
        Some(false) => "out",
        None => "unknown",
    }
}

fn is_note_in_track_scale(project: &ProjectData, snapshot: UiSnapshot, note: u8) -> Option<bool> {
    let track = project.song.tracks.get(snapshot.focused_track)?;
    let scale_id = track.scale_override.unwrap_or(project.song.default_scale);
    let scale = project.scales.get(&scale_id)?;

    if scale.interval_mask == 0 {
        return Some(false);
    }

    let key = scale.key % 12;
    let pitch_class = note % 12;
    let interval = ((12 + pitch_class as i16 - key as i16) % 12) as u32;

    Some(((u32::from(scale.interval_mask) >> interval) & 1) != 0)
}

fn centered_window_start(selected: usize, total_rows: usize, window_rows: usize) -> usize {
    let clamped_window = window_rows.min(total_rows.max(1));
    if total_rows <= clamped_window {
        return 0;
    }

    let mut start = selected.saturating_sub(clamped_window / 2);
    if start + clamped_window > total_rows {
        start = total_rows - clamped_window;
    }
    start
}

fn option_u8_json(value: Option<u8>) -> String {
    value
        .map(|item| item.to_string())
        .unwrap_or_else(|| String::from("null"))
}

fn option_usize_json(value: Option<usize>) -> String {
    value
        .map(|item| item.to_string())
        .unwrap_or_else(|| String::from("null"))
}

fn option_u8_label(value: Option<u8>) -> String {
    value
        .map(|item| format!("{item:02}"))
        .unwrap_or_else(|| String::from("--"))
}

fn editor_target_label(snapshot: UiSnapshot, chain_id: Option<u8>, phrase_id: Option<u8>) -> String {
    format!(
        "track={} song_row={} chain_row={} chain={} phrase={} step={}",
        snapshot.focused_track,
        snapshot.selected_song_row,
        snapshot.selected_chain_row,
        chain_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("--")),
        phrase_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("--")),
        snapshot.selected_step,
    )
}

fn parse_u8_field(value: &str) -> Option<u8> {
    value.trim().parse::<u8>().ok()
}

fn parse_i16_field(value: &str) -> Option<i16> {
    value.trim().parse::<i16>().ok()
}

fn seeded_note(step_index: usize) -> u8 {
    const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
    let octave = (step_index / MAJOR.len()) as u8;
    let interval = MAJOR[step_index % MAJOR.len()];
    60u8
        .saturating_add(interval)
        .saturating_add(octave.saturating_mul(12))
}

fn option_path_json(path: Option<&Path>) -> String {
    if let Some(path) = path {
        format!("\"{}\"", json_escape(&path.display().to_string()))
    } else {
        String::from("null")
    }
}

fn recent_paths_json(paths: &[PathBuf]) -> String {
    let mut items = String::new();
    for path in paths {
        if !items.is_empty() {
            items.push(',');
        }
        items.push_str(&format!("\"{}\"", json_escape(&path.display().to_string())));
    }
    items
}

fn screen_label(screen: UiScreen) -> &'static str {
    match screen {
        UiScreen::Song => "song",
        UiScreen::Chain => "chain",
        UiScreen::Phrase => "phrase",
        UiScreen::Mixer => "mixer",
    }
}

fn transport_label(playing: bool) -> &'static str {
    if playing {
        "play"
    } else {
        "stop"
    }
}

fn parse_request_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    Some((method, target))
}

fn split_path_and_query(target: &str) -> (&str, Option<&str>) {
    if let Some((path, query)) = target.split_once('?') {
        (path, Some(query))
    } else {
        (target, None)
    }
}

fn query_value<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    let query = query?;
    for pair in query.split('&') {
        if let Some((name, value)) = pair.split_once('=') {
            if name == key {
                return Some(value);
            }
        }
    }
    None
}

fn query_flag(query: Option<&str>, key: &str) -> bool {
    matches!(
        query_value(query, key),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

fn url_decode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hi = bytes[index + 1];
                let lo = bytes[index + 2];
                let hex = [hi, lo];
                let value = std::str::from_utf8(&hex)
                    .ok()
                    .and_then(|digits| u8::from_str_radix(digits, 16).ok());
                if let Some(decoded) = value {
                    out.push(decoded as char);
                    index += 3;
                } else {
                    out.push('%');
                    index += 1;
                }
            }
            other => {
                out.push(other as char);
                index += 1;
            }
        }
    }
    out
}

fn write_text_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        status_text,
        content_type,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn ui_error_label(error: UiError) -> &'static str {
    match error {
        UiError::Engine(_) => "engine",
        UiError::InvalidTrack(_) => "invalid-track",
        UiError::InvalidSongRow(_) => "invalid-song-row",
        UiError::InvalidChainRow(_) => "invalid-chain-row",
        UiError::InvalidStep(_) => "invalid-step",
    }
}

fn index_html() -> &'static str {
    r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>P9 Tracker GUI Shell</title>
<style>
:root {
  --bg: #f2f3ef;
  --ink: #101820;
  --panel: #ffffff;
  --accent: #ff6f3c;
  --muted: #68707a;
  --line: #d8dcd2;
  --good: #1f7a2f;
  --warn: #c63b2d;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  background: radial-gradient(circle at 20% 10%, #fff4e6 0%, var(--bg) 48%);
  color: var(--ink);
}
main {
  max-width: 1180px;
  margin: 20px auto;
  padding: 0 16px 24px;
}
header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
}
.small { color: var(--muted); font-size: 0.9rem; }
.panel {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 14px;
  margin-top: 14px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.views {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.view {
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 10px;
  background: #fbfcfa;
}
.view.active {
  border-color: var(--accent);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 30%, transparent);
}
.view h4 {
  margin: 0 0 8px;
}
.view-meta {
  color: var(--muted);
  font-size: 0.85rem;
  margin-bottom: 8px;
}
.tabs {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}
.tab {
  text-align: center;
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 8px;
  color: var(--muted);
  background: #f8faf5;
}
.tab.active {
  color: var(--ink);
  border-color: var(--accent);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 30%, transparent);
}
.controls {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
button {
  border: 1px solid var(--line);
  background: #fff;
  border-radius: 8px;
  padding: 8px 12px;
  cursor: pointer;
}
button:hover { border-color: var(--accent); }
button.danger { border-color: #c63b2d; color: #c63b2d; }
input[type="text"] {
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 8px 10px;
  min-width: 320px;
  max-width: 100%;
}
.kv { display: grid; grid-template-columns: 180px 1fr; gap: 8px; }
.recent-list {
  margin: 8px 0 0;
  padding-left: 16px;
}
.recent-list li {
  margin: 6px 0;
}
table {
  width: 100%;
  border-collapse: collapse;
  font-family: "IBM Plex Mono", "Consolas", monospace;
  font-size: 0.86rem;
}
th, td {
  border-bottom: 1px solid var(--line);
  padding: 4px 6px;
  text-align: left;
}
tr.selected td { background: #fff0e7; }
.note-in { color: var(--good); }
.note-out { color: var(--warn); font-weight: 600; }
footer { margin-top: 12px; color: var(--muted); font-size: 0.85rem; }
@media (max-width: 980px) {
  .grid { grid-template-columns: 1fr; }
  .views { grid-template-columns: 1fr; }
}
@media (max-width: 720px) {
  .kv { grid-template-columns: 1fr; }
}
</style>
</head>
<body>
<main>
  <header>
    <h1>P9 Tracker GUI Shell (Phase 19.2a)</h1>
    <span class="small">workflow polish + voice lifecycle/click-risk telemetry baseline</span>
  </header>

  <section class="panel">
    <div class="tabs">
      <div class="tab" id="tab-song">SONG</div>
      <div class="tab" id="tab-chain">CHAIN</div>
      <div class="tab" id="tab-phrase">PHRASE</div>
      <div class="tab" id="tab-mixer">MIXER</div>
    </div>
  </section>

  <section class="panel grid">
    <div>
      <h3>Transport</h3>
      <div class="controls">
        <button onclick="sendCmd('play')">Play</button>
        <button onclick="sendCmd('stop')">Stop</button>
        <button onclick="sendCmd('toggle_play')">Toggle</button>
        <button onclick="sendCmd('rewind')">Rewind</button>
      </div>
      <div class="controls" style="margin-top:8px">
        <button onclick="sendCmd('screen_prev')">Prev Screen</button>
        <button onclick="sendCmd('screen_next')">Next Screen</button>
      </div>
      <div class="controls" style="margin-top:8px">
        <button onclick="sendCmd('track_left')">Track Left</button>
        <button onclick="sendCmd('track_right')">Track Right</button>
      </div>
      <div class="controls" style="margin-top:8px">
        <button onclick="sendCmd('cursor_up')">Cursor Up</button>
        <button onclick="sendCmd('cursor_down')">Cursor Down</button>
      </div>
      <div class="controls" style="margin-top:8px">
        <button onclick="sendCmd('toggle_scale')">Toggle Scale Hint</button>
      </div>
      <div class="controls" style="margin-top:8px">
        <button onclick="sendCmd('screen_song')">Song (1)</button>
        <button onclick="sendCmd('screen_chain')">Chain (2)</button>
        <button onclick="sendCmd('screen_phrase')">Phrase (3)</button>
        <button onclick="sendCmd('screen_mixer')">Mixer (4)</button>
      </div>
      <div class="controls" style="margin-top:8px">
        <button onclick="sendCmd('edit_focus_prepare')">Focus Editor (/)</button>
        <button onclick="sendCmd('step_prev_fine')">Step -1 (PgUp)</button>
        <button onclick="sendCmd('step_next_fine')">Step +1 (PgDn)</button>
      </div>
      <div class="small" style="margin-top:10px">
        Keys: Space/T toggle, G play, S stop, R rewind, arrows or H/J/K/L move, N/P switch screen, 1/2/3/4 direct screens, / quick editor focus, PgUp/PgDn step -/+1, C/F/I/E edit flow, A/Z select, W/V copy-paste, Shift+V force paste, D duplicate, B fill, M clear block, [/] transpose, ,/. rotate, U/Y undo-redo, Ctrl+S save, Q quit.
      </div>
    </div>

    <div>
      <h3>State</h3>
      <div class="kv"><span>Tick</span><strong id="tick">-</strong></div>
      <div class="kv"><span>Playing</span><strong id="playing">-</strong></div>
      <div class="kv"><span>Tempo</span><strong id="tempo">-</strong></div>
      <div class="kv"><span>Focused Track</span><strong id="track">-</strong></div>
      <div class="kv"><span>Song Row</span><strong id="song-row">-</strong></div>
      <div class="kv"><span>Chain Row</span><strong id="chain-row">-</strong></div>
      <div class="kv"><span>Phrase / Step</span><strong id="phrase-step">-</strong></div>
      <div class="kv"><span>Track Level</span><strong id="track-level">-</strong></div>
      <div class="kv"><span>Scale Highlight</span><strong id="scale-highlight">-</strong></div>
    </div>
  </section>

  <section class="panel">
    <h3>Session</h3>
    <div class="controls">
      <button onclick="sessionNew()">New</button>
      <button onclick="sessionOpen()">Open Path</button>
      <button onclick="sessionSave()">Save</button>
      <button onclick="sessionSaveAs()">Save As Path</button>
      <button onclick="sendCmd('session_recent')">Recent</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <input id="session-path" type="text" placeholder="/absolute/or/relative/project.p9" />
    </div>
    <div class="kv" style="margin-top:8px"><span>Current Path</span><strong id="session-current">-</strong></div>
    <div class="small" style="margin-top:8px">Recent projects:</div>
    <ul id="recent-list" class="recent-list"><li>none</li></ul>
  </section>

  <section class="panel">
    <h3>Step Editor</h3>
    <div class="kv"><span>Edit Target</span><strong id="editor-target">-</strong></div>
    <div class="kv"><span>Bound Chain / Phrase</span><strong id="editor-bindings">-</strong></div>
    <div class="kv"><span>Focused Instrument</span><strong id="editor-instrument">-</strong></div>
    <div class="kv"><span>History</span><strong id="editor-history">-</strong></div>
    <div class="kv"><span>Selection / Clipboard</span><strong id="editor-buffer">-</strong></div>
    <div class="controls" style="margin-top:8px">
      <button onclick="sendCmd('edit_bind_chain')">Bind Chain (c)</button>
      <button onclick="sendCmd('edit_bind_phrase')">Bind Phrase (f)</button>
      <button onclick="sendCmd('edit_ensure_instrument')">Ensure Inst (i)</button>
      <button onclick="editWriteStep()">Write Step (e)</button>
      <button onclick="sendCmd('edit_write_step', { clear: 1 })">Clear Step</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <button onclick="sendCmd('edit_select_start')">Select Start (a)</button>
      <button onclick="sendCmd('edit_select_end')">Select End (z)</button>
      <button onclick="sendCmd('edit_copy')">Copy (w)</button>
      <button onclick="sendCmd('edit_paste_safe')">Paste Safe (v)</button>
      <button onclick="sendCmd('edit_paste_force')">Paste Force (Shift+V)</button>
      <button onclick="sendCmd('edit_clear_selection')">Clear Sel (x)</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <button onclick="sendCmd('edit_undo')">Undo (u)</button>
      <button onclick="sendCmd('edit_redo')">Redo (y)</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <button onclick="powerDuplicate()">Duplicate (d)</button>
      <button onclick="powerDuplicate(true)">Duplicate Force</button>
      <button onclick="powerFillSelection()">Fill (b)</button>
      <button onclick="sendCmd('edit_power_clear_range')">Clear Block (m)</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <button onclick="powerTranspose(-1)">Transpose -1 ([)</button>
      <button onclick="powerTranspose(1)">Transpose +1 (])</button>
      <button onclick="powerRotate(-1)">Rotate Left (,)</button>
      <button onclick="powerRotate(1)">Rotate Right (.)</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <button onclick="sendCmd('edit_song_clone_prev')">Song Clone Prev Row</button>
      <button onclick="sendCmd('edit_chain_clone_prev')">Chain Clone Prev Row</button>
    </div>
    <div class="controls" style="margin-top:8px">
      <input id="edit-note" type="text" placeholder="note 0..127 (empty=seeded)" />
      <input id="edit-velocity" type="text" placeholder="velocity 1..127 (default 100)" />
      <input id="edit-instrument" type="text" placeholder="instrument id (default focused)" />
    </div>
    <div class="controls" style="margin-top:8px">
      <input id="power-note" type="text" placeholder="fill note 0..127 (empty=seeded)" />
      <input id="power-velocity" type="text" placeholder="fill velocity 1..127 (default 100)" />
      <input id="power-instrument" type="text" placeholder="fill instrument (default focused)" />
      <input id="power-transpose" type="text" placeholder="transpose delta (default +1)" />
      <input id="power-rotate" type="text" placeholder="rotate shift (default +1)" />
    </div>
    <div class="small" style="margin-top:8px">Safety tags are returned as <code>info/warn/error</code> in Last Command.</div>
  </section>

  <section class="panel">
    <h3>Screens</h3>
    <div class="views">
      <article class="view" id="view-song">
        <h4>Song</h4>
        <div class="view-meta" id="song-meta">-</div>
        <table>
          <thead><tr><th>Row</th><th>Chain</th></tr></thead>
          <tbody id="song-body"></tbody>
        </table>
      </article>

      <article class="view" id="view-chain">
        <h4>Chain</h4>
        <div class="view-meta" id="chain-meta">-</div>
        <table>
          <thead><tr><th>Row</th><th>Phrase</th><th>Trn</th></tr></thead>
          <tbody id="chain-body"></tbody>
        </table>
      </article>

      <article class="view" id="view-phrase">
        <h4>Phrase</h4>
        <div class="view-meta" id="phrase-meta">-</div>
        <table>
          <thead><tr><th>Step</th><th>Note</th><th>Vel</th><th>Inst</th><th>FX</th></tr></thead>
          <tbody id="phrase-body"></tbody>
        </table>
      </article>

      <article class="view" id="view-mixer">
        <h4>Mixer</h4>
        <div class="view-meta" id="mixer-meta">-</div>
        <table>
          <thead><tr><th>Track</th><th>Level</th></tr></thead>
          <tbody id="mixer-body"></tbody>
        </table>
      </article>
    </div>
  </section>

  <section class="panel">
    <h3>Status</h3>
    <div class="kv"><span>Transport State</span><strong id="transport-state">-</strong></div>
    <div class="kv"><span>Recovery</span><strong id="recovery-state">-</strong></div>
    <div class="kv"><span>Dirty</span><strong id="dirty-state">-</strong></div>
    <div class="kv"><span>Autosave</span><strong id="autosave-state">-</strong></div>
    <div class="kv"><span>Runtime Queue</span><strong id="queue-state">-</strong></div>
    <div class="kv"><span>Last Command</span><strong id="status">ready</strong></div>
    <div class="controls" style="margin-top:10px">
      <button class="danger" onclick="sendCmd('quit')">Quit GUI Shell</button>
    </div>
  </section>

  <footer>
    Phase 19.2a goal: instrument depth v0 with deterministic playback profiles by instrument type.
  </footer>
</main>

<script>
const tabs = ['song', 'chain', 'phrase', 'mixer'];
let latestState = null;
const keyMap = {
  Space: 'toggle_play',
  KeyT: 'toggle_play',
  KeyG: 'play',
  KeyS: 'stop',
  KeyR: 'rewind',
  Digit1: 'screen_song',
  Digit2: 'screen_chain',
  Digit3: 'screen_phrase',
  Digit4: 'screen_mixer',
  ArrowLeft: 'track_left',
  ArrowRight: 'track_right',
  ArrowUp: 'cursor_up',
  ArrowDown: 'cursor_down',
  KeyH: 'track_left',
  KeyL: 'track_right',
  KeyK: 'cursor_up',
  KeyJ: 'cursor_down',
  KeyN: 'screen_next',
  KeyP: 'screen_prev',
  KeyX: 'toggle_scale',
  KeyC: 'edit_bind_chain',
  KeyF: 'edit_bind_phrase',
  KeyI: 'edit_ensure_instrument',
  KeyE: 'edit_write_step',
  KeyA: 'edit_select_start',
  KeyZ: 'edit_select_end',
  KeyW: 'edit_copy',
  KeyV: 'edit_paste_safe',
  KeyU: 'edit_undo',
  KeyY: 'edit_redo',
  KeyD: 'edit_power_duplicate',
  KeyB: 'edit_power_fill',
  KeyM: 'edit_power_clear_range',
  BracketLeft: 'edit_power_transpose_down',
  BracketRight: 'edit_power_transpose_up',
  Comma: 'edit_power_rotate_left',
  Period: 'edit_power_rotate_right',
  Slash: 'edit_focus_prepare',
  PageUp: 'step_prev_fine',
  PageDown: 'step_next_fine',
  KeyQ: 'quit',
};

function pad2(value) {
  return String(value).padStart(2, '0');
}

function fmtOptional(value, padded = false) {
  if (value === null || value === undefined) {
    return '--';
  }
  return padded ? pad2(value) : String(value);
}

function setActiveScreen(screen) {
  tabs.forEach((tab) => {
    document.getElementById(`tab-${tab}`).classList.toggle('active', screen === tab);
    document.getElementById(`view-${tab}`).classList.toggle('active', screen === tab);
  });
}

function renderSong(view) {
  document.getElementById('song-meta').textContent = `rows ${view.window_start}..${view.window_end}`;
  const body = view.rows.map((row) => {
    const selected = row.selected ? 'selected' : '';
    return `<tr class="${selected}"><td>${pad2(row.row)}</td><td>${fmtOptional(row.chain_id, true)}</td></tr>`;
  }).join('');
  document.getElementById('song-body').innerHTML = body;
}

function renderChain(view) {
  const label = view.bound_chain_id === null
    ? 'no chain on selected song row'
    : `chain ${pad2(view.bound_chain_id)} (${view.exists ? 'loaded' : 'missing'}) rows ${view.window_start}..${view.window_end}`;
  document.getElementById('chain-meta').textContent = label;

  if (!view.exists || view.rows.length === 0) {
    document.getElementById('chain-body').innerHTML = '<tr><td colspan="3">No chain data</td></tr>';
    return;
  }

  const body = view.rows.map((row) => {
    const selected = row.selected ? 'selected' : '';
    const transpose = row.transpose >= 0 ? `+${row.transpose}` : `${row.transpose}`;
    return `<tr class="${selected}"><td>${pad2(row.row)}</td><td>${fmtOptional(row.phrase_id, true)}</td><td>${transpose}</td></tr>`;
  }).join('');
  document.getElementById('chain-body').innerHTML = body;
}

function renderPhrase(view) {
  const bound = fmtOptional(view.bound_phrase_id, true);
  const selected = fmtOptional(view.selected_phrase_id, true);
  document.getElementById('phrase-meta').textContent = `selected ${selected} | bound ${bound} | ${view.exists ? 'loaded' : 'missing'}`;

  const body = view.rows.map((step) => {
    const selectedClass = step.selected ? 'selected' : '';
    const noteClass = step.scale === 'out' ? 'note-out' : (step.scale === 'in' ? 'note-in' : '');
    const note = fmtOptional(step.note, false);
    return `<tr class="${selectedClass}"><td>${pad2(step.step)}</td><td class="${noteClass}">${note}</td><td>${step.velocity}</td><td>${fmtOptional(step.instrument_id, true)}</td><td>${step.fx}</td></tr>`;
  }).join('');

  document.getElementById('phrase-body').innerHTML = body;
}

function renderMixer(view) {
  document.getElementById('mixer-meta').textContent = `master ${view.master_level} | send mfx ${view.send_mfx} delay ${view.send_delay} reverb ${view.send_reverb}`;
  const body = view.tracks.map((track) => {
    const selected = track.focused ? 'selected' : '';
    return `<tr class="${selected}"><td>${track.track}</td><td>${track.level}</td></tr>`;
  }).join('');
  document.getElementById('mixer-body').innerHTML = body;
}

function renderRecentList(paths) {
  const list = document.getElementById('recent-list');
  list.innerHTML = '';

  if (!paths || paths.length === 0) {
    list.innerHTML = '<li>none</li>';
    return;
  }

  paths.forEach((path) => {
    const item = document.createElement('li');
    const button = document.createElement('button');
    button.textContent = 'Open';
    button.onclick = () => sessionOpen(path);
    const label = document.createElement('code');
    label.textContent = path;
    item.appendChild(button);
    item.appendChild(document.createTextNode(' '));
    item.appendChild(label);
    list.appendChild(item);
  });
}

function readSessionPath() {
  return document.getElementById('session-path').value.trim();
}

function sessionNew() {
  sendCmd('session_new');
}

function sessionOpen(pathOverride) {
  const path = (pathOverride || readSessionPath()).trim();
  if (!path) {
    document.getElementById('status').textContent = 'warn: path is required for open';
    return;
  }
  sendCmd('session_open', { path });
}

function sessionSave() {
  const currentPath = latestState && latestState.session ? latestState.session.current_path : null;
  const path = readSessionPath();
  if (!currentPath && !path) {
    document.getElementById('status').textContent = 'warn: no current path; use Save As';
    return;
  }

  if (path) {
    sendCmd('session_save', { path });
  } else {
    sendCmd('session_save');
  }
}

function sessionSaveAs() {
  const path = readSessionPath();
  if (!path) {
    document.getElementById('status').textContent = 'warn: path is required for save-as';
    return;
  }
  sendCmd('session_save_as', { path });
}

function readOptionalNumberInput(id) {
  const value = document.getElementById(id).value.trim();
  if (!value) {
    return null;
  }
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) {
    return null;
  }
  return parsed;
}

function editWriteStep() {
  const note = readOptionalNumberInput('edit-note');
  const velocity = readOptionalNumberInput('edit-velocity');
  const instrument = readOptionalNumberInput('edit-instrument');

  sendCmd('edit_write_step', {
    note,
    velocity,
    instrument,
  });
}

function readOptionalSignedInput(id) {
  const value = document.getElementById(id).value.trim();
  if (!value) {
    return null;
  }
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed)) {
    return null;
  }
  return parsed;
}

function powerDuplicate(force = false) {
  sendCmd('edit_power_duplicate', { force });
}

function powerFillSelection() {
  const note = readOptionalNumberInput('power-note');
  const velocity = readOptionalNumberInput('power-velocity');
  const instrument = readOptionalNumberInput('power-instrument');
  sendCmd('edit_power_fill', { note, velocity, instrument });
}

function powerTranspose(fallbackDelta = 1) {
  const delta = readOptionalSignedInput('power-transpose');
  sendCmd('edit_power_transpose', { delta: delta === null ? fallbackDelta : delta });
}

function powerRotate(fallbackShift = 1) {
  const shift = readOptionalSignedInput('power-rotate');
  sendCmd('edit_power_rotate', { shift: shift === null ? fallbackShift : shift });
}

async function refreshState() {
  try {
    const response = await fetch('/state');
    const state = await response.json();
    latestState = state;
    const transport = state.transport;
    const cursor = state.cursor;
    const status = state.status;
    const session = state.session;
    const editor = state.editor;

    document.getElementById('tick').textContent = transport.tick;
    document.getElementById('playing').textContent = transport.playing ? 'yes' : 'no';
    document.getElementById('tempo').textContent = transport.tempo;
    document.getElementById('track').textContent = cursor.track;
    document.getElementById('song-row').textContent = cursor.song_row;
    document.getElementById('chain-row').textContent = cursor.chain_row;
    document.getElementById('phrase-step').textContent = `${cursor.phrase_id} / ${cursor.step}`;
    document.getElementById('track-level').textContent = cursor.track_level;
    document.getElementById('scale-highlight').textContent = state.scale_highlight;
    document.getElementById('transport-state').textContent = status.transport;
    document.getElementById('recovery-state').textContent = status.recovery;
    document.getElementById('dirty-state').textContent = status.dirty ? 'yes' : 'no';
    document.getElementById('autosave-state').textContent = status.autosave;
    document.getElementById('queue-state').textContent = `${status.queued_commands} queued / ${status.processed_commands} processed`;
    document.getElementById('session-current').textContent = session.current_path || '-';
    document.getElementById('editor-target').textContent = editor.target;
    document.getElementById('editor-bindings').textContent = `${fmtOptional(editor.bound_chain_id, true)} / ${fmtOptional(editor.bound_phrase_id, true)}`;
    document.getElementById('editor-instrument').textContent = `${editor.focused_instrument} (${editor.instrument_ready ? 'ready' : 'missing'})`;
    document.getElementById('editor-history').textContent = `undo ${editor.undo_depth} / redo ${editor.redo_depth}`;
    const selectionSpan = editor.selection_active
      ? `${pad2(editor.selection_start)}-${pad2(editor.selection_end)}`
      : 'empty';
    const clipboardState = editor.clipboard_ready
      ? `ready (${editor.clipboard_len})`
      : 'empty';
    document.getElementById('editor-buffer').textContent = `selection ${selectionSpan} | clipboard ${clipboardState}${editor.overwrite_guard ? ' | overwrite-guard armed' : ''}`;

    renderSong(state.views.song);
    renderChain(state.views.chain);
    renderPhrase(state.views.phrase);
    renderMixer(state.views.mixer);
    renderRecentList(session.recent);
    setActiveScreen(state.screen);
  } catch (error) {
    document.getElementById('status').textContent = `error: ${error}`;
  }
}

async function sendCmd(cmd, options = {}) {
  const params = new URLSearchParams();
  params.set('cmd', cmd);
  Object.entries(options).forEach(([key, value]) => {
    if (value === undefined || value === null || value === false || value === '') {
      return;
    }
    if (value === true) {
      params.set(key, '1');
      return;
    }
    params.set(key, String(value));
  });

  if (options.force) {
    params.set('force', '1');
  }

  try {
    const response = await fetch(`/action?${params.toString()}`, {
      method: 'POST',
    });
    const body = await response.json();

    if (body.confirm_required && !options.force) {
      const message = body.status || `Confirm action '${cmd}'`;
      const confirmed = window.confirm(`${message}\n\nContinue?`);
      if (confirmed) {
        await sendCmd(cmd, { ...options, force: true });
      } else {
        document.getElementById('status').textContent = 'warn: action cancelled';
      }
      return;
    }

    document.getElementById('status').textContent = body.status;
    await refreshState();
    if (body.quit) {
      document.getElementById('status').textContent = 'gui shell stopped';
    }
  } catch (error) {
    document.getElementById('status').textContent = `error: ${error}`;
  }
}

function initKeyboardRouting() {
  document.addEventListener('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.code === 'KeyS') {
      event.preventDefault();
      sessionSave();
      return;
    }

    const tag = event.target && event.target.tagName ? event.target.tagName : '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
      return;
    }

    if (event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }

    if (event.shiftKey && event.code === 'KeyV') {
      event.preventDefault();
      sendCmd('edit_paste_force');
      return;
    }

    const cmd = keyMap[event.code];
    if (!cmd) {
      return;
    }

    event.preventDefault();
    sendCmd(cmd);
  });
}

window.addEventListener('beforeunload', (event) => {
  if (latestState && latestState.status && latestState.status.dirty) {
    event.preventDefault();
    event.returnValue = '';
  }
});

initKeyboardRouting();
setInterval(refreshState, 250);
refreshState();
</script>
</body>
</html>
"#
}

#[cfg(test)]
mod tests {
    use super::{
        apply_gui_command, apply_gui_command_with_query, build_state_json, execute_action_command,
        parse_request_line, query_value, split_path_and_query, GuiSessionState, ProjectHistory,
        ShellEditState, GUI_HISTORY_LIMIT,
    };
    use crate::hardening::{DirtyStateTracker, RecoveryStatus};
    use crate::runtime::RuntimeCoordinator;
    use crate::ui::{UiAction, UiController, UiScreen};
    use p9_core::engine::Engine;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn session_state() -> GuiSessionState {
        GuiSessionState {
            recovery: RecoveryStatus::CleanStart,
            dirty: false,
            autosave_status: String::from("clean"),
            current_project_path: None,
            recent_project_paths: Vec::new(),
            history: ProjectHistory::with_limit(GUI_HISTORY_LIMIT),
            edit_state: ShellEditState::default(),
        }
    }

    fn temp_file(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let mut path = std::env::temp_dir();
        path.push(format!("{}_{}_{}.p9", prefix, std::process::id(), nanos));
        path
    }

    #[test]
    fn parse_request_line_extracts_method_and_target() {
        let line = "GET /state HTTP/1.1";
        let parsed = parse_request_line(line).unwrap();

        assert_eq!(parsed.0, "GET");
        assert_eq!(parsed.1, "/state");
    }

    #[test]
    fn split_path_and_query_extracts_query() {
        let (path, query) = split_path_and_query("/action?cmd=screen_next&x=1");

        assert_eq!(path, "/action");
        assert_eq!(query_value(query, "cmd"), Some("screen_next"));
        assert_eq!(query_value(query, "x"), Some("1"));
        assert_eq!(query_value(query, "missing"), None);
    }

    #[test]
    fn apply_gui_command_handles_known_and_unknown_actions() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);

        let ok = apply_gui_command("screen_next", &mut ui, &mut engine, &mut runtime);
        assert!(ok.starts_with("info:"));

        let unknown = apply_gui_command("nope", &mut ui, &mut engine, &mut runtime);
        assert!(unknown.starts_with("warn:"));
        assert!(unknown.contains("visible buttons"));
    }

    #[test]
    fn apply_gui_command_queues_explicit_transport_commands() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);

        let play = apply_gui_command("play", &mut ui, &mut engine, &mut runtime);
        let stop = apply_gui_command("stop", &mut ui, &mut engine, &mut runtime);

        assert!(play.contains("queued"));
        assert!(stop.contains("queued"));
        assert_eq!(runtime.snapshot().queued_commands, 2);
    }

    #[test]
    fn screen_shortcuts_and_fine_step_shift_work() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);

        let to_mixer = apply_gui_command("screen_mixer", &mut ui, &mut engine, &mut runtime);
        let to_phrase = apply_gui_command("screen_phrase", &mut ui, &mut engine, &mut runtime);
        assert!(to_mixer.starts_with("info:"));
        assert!(to_phrase.starts_with("info:"));
        assert_eq!(ui.snapshot(&engine, &runtime).screen, UiScreen::Phrase);

        let prev = apply_gui_command("step_prev_fine", &mut ui, &mut engine, &mut runtime);
        assert!(prev.starts_with("info:"));
        assert_eq!(ui.snapshot(&engine, &runtime).selected_step, 15);

        let next = apply_gui_command("step_next_fine", &mut ui, &mut engine, &mut runtime);
        assert!(next.starts_with("info:"));
        assert_eq!(ui.snapshot(&engine, &runtime).selected_step, 0);
    }

    #[test]
    fn edit_flow_parity_c_f_i_e_writes_step() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
        let mut edit_state = ShellEditState::default();

        let c = apply_gui_command_with_query(
            "edit_bind_chain",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        let f = apply_gui_command_with_query(
            "edit_bind_phrase",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        let i = apply_gui_command_with_query(
            "edit_ensure_instrument",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        let e = apply_gui_command_with_query(
            "edit_write_step",
            Some("note=72&velocity=101&instrument=0"),
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );

        assert!(c.starts_with("info:"));
        assert!(f.starts_with("info:"));
        assert!(i.starts_with("info:"));
        assert!(e.starts_with("info:"));

        let project = engine.snapshot();
        let step = &project.phrases.get(&0).unwrap().steps[0];
        assert_eq!(step.note, Some(72));
        assert_eq!(step.velocity, 101);
        assert_eq!(step.instrument_id, Some(0));
    }

    #[test]
    fn edit_focus_prepare_bootstraps_editor_context() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
        let mut edit_state = ShellEditState::default();

        let status = apply_gui_command_with_query(
            "edit_focus_prepare",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(status.starts_with("info:"));
        assert!(status.contains("phrase editor ready"));

        let snapshot = ui.snapshot(&engine, &runtime);
        assert_eq!(snapshot.screen, UiScreen::Phrase);
        assert_eq!(
            engine
                .snapshot()
                .song
                .tracks
                .get(0)
                .unwrap()
                .song_rows
                .get(0)
                .copied()
                .flatten(),
            Some(0)
        );
        assert_eq!(
            engine
                .snapshot()
                .chains
                .get(&0)
                .and_then(|chain| chain.rows.get(0))
                .and_then(|row| row.phrase_id),
            Some(0)
        );
        assert!(engine.snapshot().instruments.contains_key(&0));
    }

    #[test]
    fn polished_warnings_include_actionable_shortcuts() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
        let mut edit_state = ShellEditState::default();

        let warn = apply_gui_command_with_query(
            "edit_paste_force",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(warn.starts_with("warn:"));
        assert!(warn.contains("Bind Chain (c)"));
    }

    #[test]
    fn edit_block_ops_and_undo_redo_apply_in_gui_flow() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
        let mut edit_state = ShellEditState::default();

        for command in ["edit_bind_chain", "edit_bind_phrase", "edit_ensure_instrument"] {
            let status = apply_gui_command_with_query(
                command,
                None,
                &mut ui,
                &mut engine,
                &mut runtime,
                &mut history,
                &mut edit_state,
            );
            assert!(status.starts_with("info:"));
        }

        let write = apply_gui_command_with_query(
            "edit_write_step",
            Some("note=65&velocity=95&instrument=0"),
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(write.starts_with("info:"));

        let select_start = apply_gui_command_with_query(
            "edit_select_start",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        let select_end = apply_gui_command_with_query(
            "edit_select_end",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        let copy = apply_gui_command_with_query(
            "edit_copy",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );

        assert!(select_start.starts_with("info:"));
        assert!(select_end.starts_with("info:"));
        assert!(copy.starts_with("info:"));

        ui.handle_action(UiAction::SelectStep(4), &mut engine, &mut runtime)
            .unwrap();

        let paste = apply_gui_command_with_query(
            "edit_paste_safe",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(paste.starts_with("info:"));
        assert_eq!(
            engine
                .snapshot()
                .phrases
                .get(&0)
                .and_then(|phrase| phrase.steps.get(4))
                .and_then(|step| step.note),
            Some(65)
        );

        let undo = apply_gui_command_with_query(
            "edit_undo",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(undo.starts_with("info:"));
        assert_eq!(
            engine
                .snapshot()
                .phrases
                .get(&0)
                .and_then(|phrase| phrase.steps.get(4))
                .and_then(|step| step.note),
            None
        );

        let redo = apply_gui_command_with_query(
            "edit_redo",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(redo.starts_with("info:"));
        assert_eq!(
            engine
                .snapshot()
                .phrases
                .get(&0)
                .and_then(|phrase| phrase.steps.get(4))
                .and_then(|step| step.note),
            Some(65)
        );
    }

    #[test]
    fn edit_power_tools_fill_transpose_rotate_clear() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
        let mut edit_state = ShellEditState::default();

        for command in ["edit_bind_chain", "edit_bind_phrase", "edit_ensure_instrument"] {
            let status = apply_gui_command_with_query(
                command,
                None,
                &mut ui,
                &mut engine,
                &mut runtime,
                &mut history,
                &mut edit_state,
            );
            assert!(status.starts_with("info:"));
        }

        ui.handle_action(UiAction::SelectStep(0), &mut engine, &mut runtime)
            .unwrap();
        let start = apply_gui_command_with_query(
            "edit_select_start",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        ui.handle_action(UiAction::SelectStep(3), &mut engine, &mut runtime)
            .unwrap();
        let end = apply_gui_command_with_query(
            "edit_select_end",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(start.starts_with("info:"));
        assert!(end.starts_with("info:"));

        let fill = apply_gui_command_with_query(
            "edit_power_fill",
            Some("velocity=99&instrument=0"),
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(fill.starts_with("info:"));
        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[0].note, Some(60));
        assert_eq!(phrase.steps[1].note, Some(62));
        assert_eq!(phrase.steps[2].note, Some(64));
        assert_eq!(phrase.steps[3].note, Some(65));
        assert_eq!(phrase.steps[0].velocity, 99);
        assert_eq!(phrase.steps[0].instrument_id, Some(0));

        let transpose = apply_gui_command_with_query(
            "edit_power_transpose",
            Some("delta=2"),
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(transpose.starts_with("info:"));
        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[0].note, Some(62));
        assert_eq!(phrase.steps[1].note, Some(64));
        assert_eq!(phrase.steps[2].note, Some(66));
        assert_eq!(phrase.steps[3].note, Some(67));

        let rotate = apply_gui_command_with_query(
            "edit_power_rotate",
            Some("shift=1"),
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(rotate.starts_with("info:"));
        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[0].note, Some(67));
        assert_eq!(phrase.steps[1].note, Some(62));
        assert_eq!(phrase.steps[2].note, Some(64));
        assert_eq!(phrase.steps[3].note, Some(66));

        let clear = apply_gui_command_with_query(
            "edit_power_clear_range",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(clear.starts_with("info:"));
        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        for step in &phrase.steps[0..=3] {
            assert_eq!(step.note, None);
            assert_eq!(step.velocity, 0x40);
            assert_eq!(step.instrument_id, None);
        }
    }

    #[test]
    fn edit_power_duplicate_and_row_clone_helpers_work() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut history = ProjectHistory::with_limit(GUI_HISTORY_LIMIT);
        let mut edit_state = ShellEditState::default();

        for command in ["edit_bind_chain", "edit_bind_phrase", "edit_ensure_instrument"] {
            let status = apply_gui_command_with_query(
                command,
                None,
                &mut ui,
                &mut engine,
                &mut runtime,
                &mut history,
                &mut edit_state,
            );
            assert!(status.starts_with("info:"));
        }

        ui.handle_action(UiAction::SelectStep(0), &mut engine, &mut runtime)
            .unwrap();
        apply_gui_command_with_query(
            "edit_select_start",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        ui.handle_action(UiAction::SelectStep(1), &mut engine, &mut runtime)
            .unwrap();
        apply_gui_command_with_query(
            "edit_select_end",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        let _ = apply_gui_command_with_query(
            "edit_power_fill",
            Some("note=72&velocity=88&instrument=0"),
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );

        ui.handle_action(UiAction::SelectStep(0), &mut engine, &mut runtime)
            .unwrap();
        let duplicate = apply_gui_command_with_query(
            "edit_power_duplicate",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(duplicate.starts_with("info:"));
        let phrase = engine.snapshot().phrases.get(&0).unwrap();
        assert_eq!(phrase.steps[2].note, Some(72));
        assert_eq!(phrase.steps[3].note, Some(72));
        assert_eq!(phrase.steps[2].velocity, 88);
        assert_eq!(phrase.steps[3].velocity, 88);

        ui.handle_action(UiAction::EnsureChain { chain_id: 9 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(
            UiAction::BindTrackRowToChain {
                song_row: 0,
                chain_id: Some(9),
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(UiAction::SelectSongRow(1), &mut engine, &mut runtime)
            .unwrap();

        let song_clone = apply_gui_command_with_query(
            "edit_song_clone_prev",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(song_clone.starts_with("info:"));
        assert_eq!(
            engine
                .snapshot()
                .song
                .tracks
                .get(0)
                .unwrap()
                .song_rows
                .get(1)
                .copied()
                .flatten(),
            Some(9)
        );

        ui.handle_action(UiAction::EnsurePhrase { phrase_id: 42 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(
            UiAction::BindChainRowToPhrase {
                chain_id: 9,
                chain_row: 0,
                phrase_id: Some(42),
                transpose: 3,
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(UiAction::SelectChainRow(1), &mut engine, &mut runtime)
            .unwrap();

        let chain_clone = apply_gui_command_with_query(
            "edit_chain_clone_prev",
            None,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut history,
            &mut edit_state,
        );
        assert!(chain_clone.starts_with("info:"));
        let chain = engine.snapshot().chains.get(&9).unwrap();
        assert_eq!(chain.rows[1].phrase_id, Some(42));
        assert_eq!(chain.rows[1].transpose, 3);
    }

    #[test]
    fn edit_write_step_warns_when_bind_context_missing() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);

        let _ = apply_gui_command("edit_ensure_instrument", &mut ui, &mut engine, &mut runtime);
        let warn = apply_gui_command("edit_write_step", &mut ui, &mut engine, &mut runtime);

        assert!(warn.starts_with("warn:"));
    }

    #[test]
    fn session_new_requires_confirmation_when_dirty() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut session = session_state();
        let mut dirty_tracker = DirtyStateTracker::from_engine(&engine);
        session.dirty = true;

        let outcome = execute_action_command(
            "session_new",
            None,
            None,
            false,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut session,
            &mut dirty_tracker,
        );

        assert!(outcome.confirm_required);
        assert!(outcome.status.starts_with("warn:"));
    }

    #[test]
    fn session_save_as_and_open_roundtrip_updates_recent() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let mut session = session_state();
        let mut dirty_tracker = DirtyStateTracker::from_engine(&engine);
        let path = temp_file("p9_gui_session_roundtrip");

        ui.handle_action(
            UiAction::EnsurePhrase { phrase_id: 0 },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(
            UiAction::EditStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(62),
                velocity: 90,
                instrument_id: None,
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        session.dirty = true;

        let save_outcome = execute_action_command(
            "session_save_as",
            None,
            path.to_str(),
            false,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut session,
            &mut dirty_tracker,
        );
        assert!(save_outcome.status.starts_with("info:"));
        assert!(!session.dirty);
        assert_eq!(session.current_project_path.as_deref(), Some(path.as_path()));
        assert_eq!(session.recent_project_paths.first().map(PathBuf::as_path), Some(path.as_path()));

        let new_outcome = execute_action_command(
            "session_new",
            None,
            None,
            true,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut session,
            &mut dirty_tracker,
        );
        assert!(new_outcome.status.starts_with("info:"));
        assert!(engine.snapshot().phrases.is_empty());

        let open_outcome = execute_action_command(
            "session_open",
            None,
            path.to_str(),
            false,
            &mut ui,
            &mut engine,
            &mut runtime,
            &mut session,
            &mut dirty_tracker,
        );
        assert!(open_outcome.status.starts_with("info:"));
        assert_eq!(
            engine
                .snapshot()
                .phrases
                .get(&0)
                .and_then(|phrase| phrase.steps[0].note),
            Some(62)
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn build_state_json_contains_core_fields() {
        let ui = UiController::default();
        let engine = Engine::new("gui");
        let runtime = RuntimeCoordinator::new(24);
        let session = session_state();

        let json = build_state_json(&ui, &engine, &runtime, &session);

        assert!(json.contains("\"screen\":\"song\""));
        assert!(json.contains("\"transport\":{"));
        assert!(json.contains("\"status\":{"));
        assert!(json.contains("\"session\":{"));
        assert!(json.contains("\"editor\":{"));
        assert!(json.contains("\"undo_depth\":"));
        assert!(json.contains("\"redo_depth\":"));
        assert!(json.contains("\"selection_active\":"));
        assert!(json.contains("\"selection_start\":"));
        assert!(json.contains("\"selection_end\":"));
        assert!(json.contains("\"clipboard_ready\":"));
        assert!(json.contains("\"clipboard_len\":"));
        assert!(json.contains("\"overwrite_guard\":"));
        assert!(json.contains("\"recovery\":\"clean-start\""));
        assert!(json.contains("\"views\":{"));
        assert!(json.contains("\"song\":{"));
        assert!(json.contains("\"chain\":{"));
        assert!(json.contains("\"phrase\":{"));
        assert!(json.contains("\"mixer\":{"));
    }

    #[test]
    fn build_state_json_includes_bound_entities() {
        let mut ui = UiController::default();
        let mut engine = Engine::new("gui");
        let mut runtime = RuntimeCoordinator::new(24);
        let session = session_state();

        ui.handle_action(UiAction::EnsureChain { chain_id: 0 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::EnsurePhrase { phrase_id: 0 }, &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(
            UiAction::BindTrackRowToChain {
                song_row: 0,
                chain_id: Some(0),
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(
            UiAction::BindChainRowToPhrase {
                chain_id: 0,
                chain_row: 0,
                phrase_id: Some(0),
                transpose: 1,
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();
        ui.handle_action(UiAction::SelectPhrase(0), &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(UiAction::SelectStep(0), &mut engine, &mut runtime)
            .unwrap();
        ui.handle_action(
            UiAction::EditStep {
                phrase_id: 0,
                step_index: 0,
                note: Some(61),
                velocity: 100,
                instrument_id: Some(0),
            },
            &mut engine,
            &mut runtime,
        )
        .unwrap();

        let json = build_state_json(&ui, &engine, &runtime, &session);

        assert!(json.contains("\"bound_chain_id\":0"));
        assert!(json.contains("\"phrase_id\":0"));
        assert!(json.contains("\"selected_phrase_id\":0"));
        assert!(json.contains("\"step\":0"));
        assert!(json.contains("\"track\":0"));
    }

    #[test]
    fn build_state_json_is_deterministic_for_same_snapshot() {
        let ui = UiController::default();
        let engine = Engine::new("gui");
        let runtime = RuntimeCoordinator::new(24);
        let session = session_state();

        let first = build_state_json(&ui, &engine, &runtime, &session);
        let second = build_state_json(&ui, &engine, &runtime, &session);

        assert_eq!(first, second);
    }
}
