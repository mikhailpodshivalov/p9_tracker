# Phase 17.1: GUI Stack and App Shell

## Objective

Deliver the first non-terminal UI shell with a pragmatic stack choice, while preserving terminal shell fallback.

## Stack Choice

- Selected stack for 17.1: lightweight local web UI shell served directly from `p9_app` (`std::net` + HTML/JS).
- Rationale for this subphase:
- zero external crate dependency risk
- fast path to non-terminal interaction
- clear bridge to existing `UiController` and runtime loop

This is a foundation step. A full desktop toolkit can be introduced in later subphases if needed.

## Delivered

- Added non-terminal GUI shell module: `crates/p9_app/src/gui_shell.rs`
- Added new app mode flag:
- `cargo run -p p9_app -- --gui-shell`
- Keeps existing terminal fallback unchanged:
- `cargo run -p p9_app -- --ui-shell`
- GUI shell lifecycle now includes:
- local listener startup (`127.0.0.1:17717..17721` fallback range)
- request handling loop + runtime tick progression
- explicit quit action (`Quit GUI Shell` button)
- GUI shell exposes:
- screen tabs (Song/Chain/Phrase/Mixer)
- realtime status panel (tick, play state, cursor/focus, scale highlight)
- transport and navigation controls wired to existing action pipeline
- Updated app diagnostics marker to:
- `stage17.1 gui-stack-app-shell`

## UX Surface (17.1)

- Browser page served from app process (`/`)
- Polling state endpoint (`/state`)
- Action endpoint (`/action?cmd=...`) for transport/navigation actions
- Basic status messages with `info/warn/error` semantics

## Test Coverage

- `gui_shell::tests::parse_request_line_extracts_method_and_target`
- `gui_shell::tests::split_path_and_query_extracts_query`
- `gui_shell::tests::apply_gui_command_handles_known_and_unknown_actions`
- `gui_shell::tests::build_state_json_contains_core_fields`
- Existing workspace behavior remains test-compatible.

## Run Instructions

```bash
cd /home/mikhail/codex/p9_tracker
export CARGO_HOME=/home/mikhail/codex/.cargo
export RUSTUP_HOME=/home/mikhail/codex/.rustup
export PATH="$CARGO_HOME/bin:$PATH"
cargo run -p p9_app -- --gui-shell
```

Then open the printed local URL in browser.

If local socket binding is blocked in the current environment, app prints:
`p9_tracker gui-shell failed: unable to bind GUI shell listener`.

## Exit Criteria (This Iteration)

- Non-terminal UI shell starts and shows core tracker screens.
- GUI controls are connected to existing runtime/UI command path.
- Terminal shell fallback remains available and unchanged.
- Phase artifact prepared in `docs/phase17_1_gui_stack_app_shell.md`.
