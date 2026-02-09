# Phase 13: UI Alpha (Tracker Workflow)

## Objective

Deliver first usable tracker workflow on top of runtime core with keyboard-first actions and deterministic UI snapshot refresh.

## Delivered

- New UI module in `p9_app`:
- `UiController` with screen, focus, selection, transport, and scale-highlight state.
- `UiAction` command set for tracker MVP flow:
- screen navigation (`Song/Chain/Phrase/Mixer`)
- track focus left/right
- phrase/step selection
- create/bind instrument/chain/phrase
- phrase step editing
- mixer level edits (track/master)
- transport toggle (`play/stop` via runtime command queue)
- `UiSnapshot` built from engine/runtime snapshots:
- focused track and selected cursor state
- transport (`tick`, `is_playing`)
- scale highlight state (`InScale/OutOfScale/Disabled/NoNote/NoScale`)
- focused track mixer level
- `p9_app` stage flow wired through UI actions for the minimal authoring loop:
- create instrument/chain/phrase
- bind song row and chain row
- edit phrase steps
- toggle transport
- read UI snapshot for diagnostics
- Runtime diagnostics output bumped to `stage13 ui-alpha` and now includes UI state fields.

## Test Coverage

- `navigation_and_focus_are_keyboard_driven`
- `minimal_edit_loop_creates_phrase_and_emits_events`
- `play_stop_control_toggles_transport_state`
- `scale_highlight_state_can_be_toggled`
- Full workspace test run passes with UI module enabled.

## Behavior Summary

- Core tracker loop no longer relies on manual state injection only; key edits flow through `UiController` actions.
- UI state refresh is deterministic because snapshot data is derived from existing engine/runtime snapshots.
- Transport control is integrated through the runtime queue path, preserving realtime ownership boundaries.

## Current Limits

- UI alpha is state/controller-only and diagnostics-driven; no rendered terminal/GUI layer yet.
- Input mapping is represented as actions; physical keymap and remapping UX are deferred.
- Theme/polish/customization are out of scope for this phase.

## Exit Criteria (This Iteration)

- End-to-end authoring flow works through UI actions without manual project injection.
- Keyboard-first workflow primitives for navigation/edit/play/stop are present and tested.
- Phase artifact prepared in `docs/phase13_ui_alpha.md`.
