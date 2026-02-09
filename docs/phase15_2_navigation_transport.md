# Phase 15.2: Navigation and Transport in UI Shell

## Objective

Extend the shell from static screen switching to practical cursor navigation and transport control, so the UI loop can be exercised without manual engine injection.

## Delivered

- UI controller command surface extended:
- `UiAction::SelectSongRow(usize)`
- `UiAction::SelectChainRow(usize)`
- `UiAction::RewindTransport`
- Validation added for cursor bounds (`song row`, `chain row`).
- UI shell command map expanded:
- screen navigation: `n`, `p`
- track focus: `h`, `l`
- cursor navigation: `j`, `k` (context-aware per panel)
- transport: `t` (play/stop toggle), `r` (stop + rewind)
- exit: `q`
- Runtime integration improved in shell loop:
- `run_tick_safe` is executed after each shell command.
- status line now reflects live transport state and tick.
- Cursor behavior by screen:
- `Song`: moves selected song row
- `Chain`: moves selected chain row
- `Phrase`: moves selected step by phrase row stride
- `Mixer`: shifts focused track
- Main diagnostics stage marker bumped to `stage15.2 navigation-transport`.

## Test Coverage

- `ui::tests::row_selection_actions_update_cursor`
- `ui::tests::rewind_transport_action_queues_stop_and_rewind`
- `ui_shell::tests::shell_cursor_commands_move_rows_and_steps`
- `ui_shell::tests::shell_transport_commands_queue_runtime_updates`
- existing shell/runtime/core tests remain green.

## Behavior Summary

- Shell now supports practical movement across tracker cursors, not only tab switching.
- Transport controls are available directly in shell mode and reflected in status feedback.
- Rewind behavior is deterministic (`Stop + Rewind` queue) to avoid ambiguous play-state after reset.

## Current Limits

- Shell input remains line-based (command + Enter), not raw key capture.
- Transport progression still tied to command loop cadence, not separate realtime UI event thread.
- Editing actions are still minimal and will be expanded in next subphases.

## Exit Criteria (This Iteration)

- Cursor navigation works in Song/Chain/Phrase/Mixer shell views.
- Play/stop/rewind commands are available and reflected in transport state.
- Phase artifact prepared in `docs/phase15_2_navigation_transport.md`.
