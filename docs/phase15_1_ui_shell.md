# Phase 15.1: UI Shell

## Objective

Introduce a first interactive shell for tracker navigation so users can inspect core panels directly from terminal without editing implementation code.

## Delivered

- Added `p9_app::ui_shell` module with text UI frame renderer.
- Added interactive shell mode:
- `cargo run -p p9_app -- --ui-shell`
- Keyboard command loop:
- `n` next screen
- `p` previous screen
- `h` focus track left
- `l` focus track right
- `q` quit shell
- Implemented panel rendering for:
- `Song` (row-to-chain view)
- `Chain` (chain row/phrase/transpose view)
- `Phrase` (16-step compact grid)
- `Mixer` (track levels + master + sends)
- Added status/footer area with transport/cursor summary and command hints.
- Integrated shell entry point into `main.rs` behind `--ui-shell` flag.
- Main diagnostics stage marker updated to `stage15.1 ui-shell`.

## Test Coverage

- `ui_shell::tests::render_frame_contains_shell_layout_sections`
- `ui_shell::tests::shell_commands_switch_screen_and_focus`
- `ui_shell::tests::shell_command_quit_returns_exit`
- Existing workspace tests remain green.

## Behavior Summary

- Users can now launch a live UI shell and switch tracker screens/focus with keyboard input.
- Shell is wired to `UiController` and uses deterministic snapshots from existing engine/runtime state.
- No new editing semantics were added in this subphase; scope is shell/navigation only.

## Current Limits

- No dedicated TUI library yet (ASCII shell output only).
- No real-time key capture; input is line-based command entry.
- Transport control and editing commands in shell mode are deferred to next subphases.

## Exit Criteria (This Iteration)

- Shell launches from CLI and renders all four core panels.
- Screen switch and focus navigation work through keyboard commands.
- Phase artifact prepared in `docs/phase15_1_ui_shell.md`.
