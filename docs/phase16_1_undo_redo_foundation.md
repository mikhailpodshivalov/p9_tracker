# Phase 16.1: Undo/Redo Foundation

## Objective

Establish deterministic undo/redo behavior for shell editing commands so users can safely iterate without losing control of project state.

## Delivered

- Added bounded history model in `ui_shell`:
- `ProjectHistory` with undo/redo stacks.
- Configured default stack limit (`128`) for shell session.
- Added project restore capability in engine:
- `Engine::replace_project(ProjectData)` for safe state rollback/replay.
- Added history-aware shell command execution path:
- `apply_shell_command_with_history(...)`
- Tracks mutating commands (`c`, `f`, `i`, `e`, `+`, `-`).
- Exposed user commands:
- `u` undo
- `y` redo
- Added explicit empty-history statuses:
- `undo -> empty history`
- `redo -> empty history`
- Updated shell labels/help to `Phase 16.1` and include `u/y`.
- Main diagnostics marker updated to `stage16.1 undo-redo-foundation`.

## Test Coverage

- `ui_shell::tests::shell_undo_redo_restores_edit_state`
- `ui_shell::tests::shell_undo_redo_reports_empty_history`
- Existing shell navigation/edit/safety/smoke tests remain green.

## Behavior Summary

- Editing commands are now reversible in deterministic order.
- Redo history is reset automatically on new mutating change.
- Undo/redo does not affect transport command queue semantics.

## Current Limits

- History scope currently covers project edit state, not runtime transport timeline.
- History is in-memory per shell session (no persistent history across restarts).
- Selection-aware batch history entries are deferred to Phase 16.2.

## Exit Criteria (This Iteration)

- Undo/redo commands are available in shell and function for core edit actions.
- Empty-history cases return clear statuses.
- Project rollback/replay path is test-covered and stable.
- Phase artifact prepared in `docs/phase16_1_undo_redo_foundation.md`.
