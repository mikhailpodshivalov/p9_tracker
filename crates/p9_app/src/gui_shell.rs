use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::runtime::RuntimeCoordinator;
use crate::ui::{UiAction, UiController, UiError, UiScreen};
use p9_core::engine::Engine;
use p9_core::model::{CHAIN_ROW_COUNT, PHRASE_STEP_COUNT, SONG_ROW_COUNT};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopControl {
    Continue,
    Quit,
}

pub fn run_web_shell(
    ui: &mut UiController,
    engine: &mut Engine,
    runtime: &mut RuntimeCoordinator,
) -> io::Result<()> {
    let listener = bind_listener()?;
    listener.set_nonblocking(true)?;

    println!(
        "p9_tracker gui-shell stage17.1 running at http://{}",
        listener.local_addr()?
    );
    println!("Open this URL in browser. Press Ctrl+C or click Quit GUI Shell to stop.");

    let mut audio = NoopAudioBackend::default();
    audio.start();
    let mut midi_output = NoopMidiOutput::default();

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if handle_connection(&mut stream, ui, engine, runtime)? == LoopControl::Quit {
                    break;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err),
        }

        let _ = runtime.run_tick_safe(engine, &mut audio, &mut midi_output);
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
            let body = build_state_json(ui, engine, runtime);
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

fn cursor_down_action(snapshot: crate::ui::UiSnapshot) -> UiAction {
    match snapshot.screen {
        UiScreen::Song => UiAction::SelectSongRow((snapshot.selected_song_row + 1) % SONG_ROW_COUNT),
        UiScreen::Chain => UiAction::SelectChainRow((snapshot.selected_chain_row + 1) % CHAIN_ROW_COUNT),
        UiScreen::Phrase => UiAction::SelectStep((snapshot.selected_step + 4) % PHRASE_STEP_COUNT),
        UiScreen::Mixer => UiAction::FocusTrackRight,
    }
}

fn cursor_up_action(snapshot: crate::ui::UiSnapshot) -> UiAction {
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

fn build_state_json(ui: &UiController, engine: &Engine, runtime: &RuntimeCoordinator) -> String {
    let ui_snapshot = ui.snapshot(engine, runtime);
    let transport = runtime.snapshot();
    let project = engine.snapshot();

    format!(
        "{{\"screen\":\"{}\",\"tick\":{},\"playing\":{},\"tempo\":{},\"focused_track\":{},\"song_row\":{},\"chain_row\":{},\"phrase\":{},\"step\":{},\"track_level\":{},\"scale_highlight\":\"{:?}\"}}",
        screen_label(ui_snapshot.screen),
        transport.tick,
        if transport.is_playing { "true" } else { "false" },
        project.song.tempo,
        ui_snapshot.focused_track,
        ui_snapshot.selected_song_row,
        ui_snapshot.selected_chain_row,
        ui_snapshot.selected_phrase_id,
        ui_snapshot.selected_step,
        ui_snapshot.focused_track_level,
        ui_snapshot.scale_highlight,
    )
}

fn screen_label(screen: UiScreen) -> &'static str {
    match screen {
        UiScreen::Song => "song",
        UiScreen::Chain => "chain",
        UiScreen::Phrase => "phrase",
        UiScreen::Mixer => "mixer",
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
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  background: radial-gradient(circle at 20% 10%, #fff4e6 0%, var(--bg) 48%);
  color: var(--ink);
}
main {
  max-width: 980px;
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
footer { margin-top: 12px; color: var(--muted); font-size: 0.85rem; }
@media (max-width: 720px) {
  .grid { grid-template-columns: 1fr; }
  .kv { grid-template-columns: 1fr; }
}
</style>
</head>
<body>
<main>
  <header>
    <h1>P9 Tracker GUI Shell (Phase 17.1)</h1>
    <span class="small">non-terminal foundation + runtime bridge</span>
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
        <button onclick="sendCmd('toggle_play')">Play/Stop</button>
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
    <h3>Status</h3>
    <div id="status">ready</div>
    <div class="controls" style="margin-top:10px">
      <button class="danger" onclick="sendCmd('quit')">Quit GUI Shell</button>
    </div>
  </section>

  <footer>
    Phase 17.1 goal: GUI stack + app shell lifecycle. Terminal fallback stays available with <code>--ui-shell</code>.
  </footer>
</main>

<script>
const tabs = ['song', 'chain', 'phrase', 'mixer'];

async function refreshState() {
  try {
    const response = await fetch('/state');
    const state = await response.json();
    document.getElementById('tick').textContent = state.tick;
    document.getElementById('playing').textContent = state.playing ? 'yes' : 'no';
    document.getElementById('tempo').textContent = state.tempo;
    document.getElementById('track').textContent = state.focused_track;
    document.getElementById('song-row').textContent = state.song_row;
    document.getElementById('chain-row').textContent = state.chain_row;
    document.getElementById('phrase-step').textContent = `${state.phrase} / ${state.step}`;
    document.getElementById('track-level').textContent = state.track_level;
    document.getElementById('scale-highlight').textContent = state.scale_highlight;

    tabs.forEach((tab) => {
      document.getElementById(`tab-${tab}`).classList.toggle('active', state.screen === tab);
    });
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
    };
    use crate::runtime::RuntimeCoordinator;
    use crate::ui::UiController;
    use p9_core::engine::Engine;

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
    fn build_state_json_contains_core_fields() {
        let ui = UiController::default();
        let engine = Engine::new("gui");
        let runtime = RuntimeCoordinator::new(24);

        let json = build_state_json(&ui, &engine, &runtime);

        assert!(json.contains("\"screen\":\"song\""));
        assert!(json.contains("\"tick\":"));
        assert!(json.contains("\"tempo\":"));
    }
}
