# Phase 18.2: Selection, Block Ops, Undo/Redo UX

## Objective

Deliver GUI-native block editing flow (`select/copy/paste/force-paste`) with visible history/buffer state and fast undo/redo actions.

## Delivered

- Reused shell editing engine inside GUI action path:
- `edit_select_start` (`a`)
- `edit_select_end` (`z`)
- `edit_copy` (`w`)
- `edit_paste_safe` (`v`)
- `edit_paste_force` (`V`)
- `edit_clear_selection` (`x`)
- `edit_undo` (`u`)
- `edit_redo` (`y`)
- Kept one shared semantic path via `apply_shell_command_with_history_state`, so GUI and shell use identical validation/safety behavior.
- Added GUI session-scoped edit state:
- `ProjectHistory` (`undo/redo` stacks)
- `ShellEditState` (selection, clipboard, overwrite-guard)
- Added editor telemetry to `/state.editor`:
- `undo_depth`
- `redo_depth`
- `selection_active`
- `clipboard_ready`
- `overwrite_guard`
- Added Step Editor UX status fields:
- `History` (`undo N / redo M`)
- `Selection / Clipboard` (`selection|clipboard|overwrite-guard`)
- Added keyboard routing for fast block workflow:
- `A/Z/W/V/U/Y`
- `Shift+V` for force paste confirmation path
- Existing session/transport flow remains unchanged.

## Test Coverage

Updated tests in `crates/p9_app/src/gui_shell.rs`:

- fixed test setup for extended `GuiSessionState`
- updated `edit_flow_parity_c_f_i_e_writes_step` to run through shared history/edit-state path
- added `edit_block_ops_and_undo_redo_apply_in_gui_flow`
- extended JSON payload checks to include editor history/buffer flags

## Exit Criteria (18.2)

- GUI supports block selection/copy/paste + force-paste without terminal fallback.
- Undo/redo works through GUI edit actions and is visible in state.
- Shared shell/GUI semantics preserve deterministic and safety-first behavior.
- Phase artifact prepared in `docs/phase18_2_selection_block_undo_redo.md`.
