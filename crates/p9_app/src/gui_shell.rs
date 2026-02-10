use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use crate::hardening::{
    clear_dirty_session_flag, default_autosave_path, default_dirty_flag_path,
    mark_dirty_session_flag, recover_from_dirty_session, AutosaveManager, AutosavePolicy,
    DirtyStateTracker, RecoveryStatus,
};
use crate::runtime::{RuntimeCommand, RuntimeCoordinator};
use crate::ui::{UiAction, UiController, UiError, UiScreen, UiSnapshot};
use p9_core::engine::Engine;
use p9_core::model::{ProjectData, Step, CHAIN_ROW_COUNT, PHRASE_STEP_COUNT, SONG_ROW_COUNT};
use p9_rt::audio::{AudioBackend, NoopAudioBackend};
use p9_rt::midi::NoopMidiOutput;

const BIND_ADDR_CANDIDATES: [&str; 5] = [
    "127.0.0.1:17717",
    "127.0.0.1:17718",
    "127.0.0.1:17719",
    "127.0.0.1:17720",
    "127.0.0.1:17721",
];
const TICK_SLEEP_MS: u64 = 16;
const GUI_AUTOSAVE_INTERVAL_TICKS: u64 = 16;
const SONG_VIEW_ROWS: usize = 8;
const CHAIN_VIEW_ROWS: usize = 8;

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
}

#[derive(Clone, Debug)]
struct SessionHardeningState {
    dirty: bool,
    autosave_status: String,
}

impl GuiSessionState {
    fn new(recovery: RecoveryStatus) -> Self {
        Self {
            recovery,
            dirty: false,
            autosave_status: String::from("unknown"),
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
        "p9_tracker gui-shell stage17.3 running at http://{}",
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
                if handle_connection(&mut stream, ui, engine, runtime, &session_state)?
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
    session_state: &GuiSessionState,
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
            let control = if matches!(cmd, Some("quit")) {
                LoopControl::Quit
            } else {
                LoopControl::Continue
            };

            let status = if let Some(name) = cmd {
                if name == "quit" {
                    String::from("info: quitting gui shell")
                } else {
                    apply_gui_command(name, ui, engine, runtime)
                }
            } else {
                String::from("warn: missing cmd parameter")
            };

            let body = format!(
                "{{\"status\":\"{}\",\"quit\":{}}}",
                json_escape(&status),
                if control == LoopControl::Quit {
                    "true"
                } else {
                    "false"
                }
            );
            write_text_response(stream, 200, "application/json; charset=utf-8", &body)?;
            Ok(control)
        }
        _ => {
            write_text_response(stream, 404, "text/plain; charset=utf-8", "not found")?;
            Ok(LoopControl::Continue)
        }
    }
}

fn apply_gui_command(
    command: &str,
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
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
        return format!("warn: unknown action '{command}'");
    };

    match ui.handle_action(action, engine, runtime) {
        Ok(()) => format!("info: action '{command}' applied"),
        Err(err) => format!("error: action '{command}' failed: {}", ui_error_label(err)),
    }
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

    format!(
        "{{\"screen\":\"{}\",\"transport\":{{\"tick\":{},\"playing\":{},\"tempo\":{}}},\"cursor\":{{\"track\":{},\"song_row\":{},\"chain_row\":{},\"phrase_id\":{},\"step\":{},\"track_level\":{}}},\"status\":{{\"transport\":\"{}\",\"recovery\":\"{}\",\"dirty\":{},\"autosave\":\"{}\",\"queued_commands\":{},\"processed_commands\":{}}},\"scale_highlight\":\"{:?}\",\"views\":{{\"song\":{},\"chain\":{},\"phrase\":{},\"mixer\":{}}}}}",
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
.kv { display: grid; grid-template-columns: 180px 1fr; gap: 8px; }
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
    <h1>P9 Tracker GUI Shell (Phase 17.3)</h1>
    <span class="small">transport and keyboard input layer</span>
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
      <div class="small" style="margin-top:10px">
        Keys: Space/T toggle, G play, S stop, R rewind, arrows or H/J/K/L move, N/P switch screen.
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
    Phase 17.3 goal: transport controls, keyboard routing, and status indicators are wired to realtime runtime state.
  </footer>
</main>

<script>
const tabs = ['song', 'chain', 'phrase', 'mixer'];
const keyMap = {
  Space: 'toggle_play',
  KeyT: 'toggle_play',
  KeyG: 'play',
  KeyS: 'stop',
  KeyR: 'rewind',
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

async function refreshState() {
  try {
    const response = await fetch('/state');
    const state = await response.json();
    const transport = state.transport;
    const cursor = state.cursor;
    const status = state.status;

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

    renderSong(state.views.song);
    renderChain(state.views.chain);
    renderPhrase(state.views.phrase);
    renderMixer(state.views.mixer);
    setActiveScreen(state.screen);
  } catch (error) {
    document.getElementById('status').textContent = `error: ${error}`;
  }
}

async function sendCmd(cmd) {
  try {
    const response = await fetch(`/action?cmd=${encodeURIComponent(cmd)}`, {
      method: 'POST',
    });
    const body = await response.json();
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
    const tag = event.target && event.target.tagName ? event.target.tagName : '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
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
        apply_gui_command, build_state_json, parse_request_line, query_value, split_path_and_query,
        GuiSessionState,
    };
    use crate::hardening::RecoveryStatus;
    use crate::runtime::RuntimeCoordinator;
    use crate::ui::{UiAction, UiController};
    use p9_core::engine::Engine;

    fn session_state() -> GuiSessionState {
        GuiSessionState {
            recovery: RecoveryStatus::CleanStart,
            dirty: false,
            autosave_status: String::from("clean"),
        }
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
    fn build_state_json_contains_core_fields() {
        let ui = UiController::default();
        let engine = Engine::new("gui");
        let runtime = RuntimeCoordinator::new(24);
        let session = session_state();

        let json = build_state_json(&ui, &engine, &runtime, &session);

        assert!(json.contains("\"screen\":\"song\""));
        assert!(json.contains("\"transport\":{"));
        assert!(json.contains("\"status\":{"));
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
