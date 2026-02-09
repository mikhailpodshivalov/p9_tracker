# Phase 11: FX/Table/Mixer Editing Depth v1

## Objective

Expand tracker editing and playback semantics with a practical MVP command set:

- step FX editing
- table row editing
- mixer editing
- deterministic playback interpretation for selected FX commands

## Delivered

- `p9_core::engine` command surface extended:
- `SetStepFx` for phrase-step FX slots
- `UpsertTable`, `SetTableRow`, `SetTableRowFx`
- `SetTrackLevel`, `SetMasterLevel`, `SetMixerSends`
- Validation rules for FX/table:
- FX slot bounds checks (`InvalidFxSlot`)
- table row bounds checks (`InvalidTableRow`)
- FX code whitelist (`VOL`, `TRN`, `LEN`)
- FX value bounds checks (`InvalidFxValue`)
- `p9_core::scheduler` playback interpretation:
- step FX processing for:
- `VOL` (velocity override)
- `TRN` (note transpose with center=48)
- `LEN` (note-length override in steps)
- table-row modulation applied per phrase step:
- `note_offset`
- `volume` scaling
- table-row FX commands processed with same whitelist
- existing note lifecycle remains deterministic with FX-affected note length.
- `p9_app` stage flow now exercises:
- step FX commands
- table row setup
- mixer commands

## Test Coverage

- `p9_core::engine`:
- rejects unknown FX codes
- rejects invalid FX slot
- table + mixer commands update project state
- `p9_core::scheduler`:
- step FX transpose/volume affect emitted note
- table row modifies note and velocity
- `LEN` FX overrides note-off timing
- existing groove/scale/override lifecycle tests remain green
- Full workspace tests pass.

## Behavior Summary

- Editing layer now supports core tracker control commands for FX/table/mixer.
- Playback path interprets selected MVP FX commands deterministically.
- Table data now affects rendered step behavior instead of being passive storage only.

## Current Limits

- FX command set intentionally small (`VOL`, `TRN`, `LEN`) in this iteration.
- Mixer values are stored but not yet fully routed into final audio gain graph.
- Advanced FX semantics and command chaining are deferred to later phases.

## Exit Criteria (This Iteration)

- Engine exposes edit commands for step FX, table rows, and mixer.
- Scheduler applies selected FX/table effects in note event generation.
- Validation failures are explicit and test-covered.
