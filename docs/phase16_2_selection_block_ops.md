# Phase 16.2: Selection and Block Operations

## Objective

Add explicit phrase-step selection and deterministic block copy/paste so editing loops can reuse note patterns safely.

## Delivered

- Added shell edit session state:
- `ShellEditState` keeps selection and clipboard between commands in one shell session.
- Added explicit selection model:
- `StepSelection` stores `track + phrase + start_step + end_step`.
- Start/end are controlled by separate commands (`a` start, `z` end).
- Added clipboard model:
- `StepClipboard` stores copied `Step` data and source scope.
- Added block commands:
- `a` set selection start on current cursor step.
- `z` set selection end on current cursor step.
- `w` copy selected step range.
- `v` paste clipboard at current step with deterministic clipping to phrase bounds.
- `x` clear active selection.
- Added track-local scope protection:
- Paste is rejected if focused track differs from clipboard source track.
- Added phrase binding checks for block operations:
- Uses the same `run c first / run f first` guardrails as edit commands.
- Paste now participates in undo/redo history (`v` is mutating).
- Updated shell labels/help/status to `Phase 16.2` and include block commands.
- Updated app stage diagnostics marker to `stage16.2 selection-block-ops`.

## Test Coverage

- `ui_shell::tests::shell_selection_copy_paste_transfers_step_block`
- `ui_shell::tests::shell_paste_clips_to_phrase_bounds`
- `ui_shell::tests::shell_paste_warns_on_track_scope_mismatch`
- Existing undo/redo, safety, transport, and smoke tests remain green.

## Behavior Summary

- Selection lifecycle is explicit and user-driven (`a` then `z`).
- Copy captures exact step payload (note/velocity/instrument/fx) for selected range.
- Paste applies sequentially from cursor step and clips at phrase end (`16` steps total).
- Clipboard remains stable until recopy; selection updates to pasted target range.
- Undo/redo reverses and reapplies paste operations deterministically.

## Current Limits

- Selection scope is phrase-step only (no chain/song block operations yet).
- Clipboard is in-memory for current shell session only.
- Paste currently enforces same-track scope to avoid accidental cross-track edits.

## Exit Criteria (This Iteration)

- Explicit selection start/end commands exist and are test-covered.
- Deterministic copy/paste works with bound checks and phrase-bound clipping.
- Track-local scope mismatch is guarded with explicit warning.
- Paste integrates with history and can be undone/redone.
- Phase artifact prepared in `docs/phase16_2_selection_block_ops.md`.
