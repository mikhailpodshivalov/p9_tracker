# Phase 16.3: Dirty State and Recovery Entry

## Objective

Add deterministic dirty-state tracking and startup recovery entry from autosave so shell sessions can resume safely after interruption.

## Delivered

- Added `DirtyStateTracker` in `hardening`:
- Fingerprint-based dirty detection from current engine snapshot.
- `mark_saved` baseline reset after successful autosave.
- Added dirty-session persistence helpers:
- `mark_dirty_session_flag(...)`
- `clear_dirty_session_flag(...)`
- Added default recovery paths in temp dir:
- `p9_tracker_phase16_autosave.p9`
- `p9_tracker_phase16_dirty.flag`
- Added startup recovery decision flow:
- `recover_from_dirty_session(...)`
- Recovery statuses: clean start, restored snapshot, missing snapshot, read failure, parse failure.
- Integrated shell hardening loop:
- On startup shell attempts recovery before command loop.
- On each loop iteration shell updates dirty state and autosave state.
- Dirty flag is created while dirty and cleared after successful autosave.
- Recovery status is shown directly in shell status diagnostics.
- Updated shell header marker to `Phase 16.3`.
- Updated app stage marker to `stage16.3 dirty-recovery-entry`.

## Test Coverage

- `hardening::tests::dirty_state_tracker_marks_dirty_and_resets_on_save`
- `hardening::tests::recover_from_dirty_session_restores_project_snapshot`
- `hardening::tests::recover_from_dirty_session_handles_missing_snapshot`
- `hardening::tests::recover_from_dirty_session_reports_parse_failure`
- `hardening::tests::dirty_flag_helpers_mark_and_clear`
- `hardening::tests::default_paths_point_to_temp_directory`
- Existing shell workflow tests remain green.

## Behavior Summary

- Dirty state now follows actual project-content changes and is independent from transport commands.
- Recovery is entered only when dirty-session flag exists.
- If dirty flag exists and autosave snapshot is valid, project state is restored on shell startup.
- If autosave is missing or invalid, startup reports explicit recovery status.
- Autosave success resets dirty baseline and clears dirty-session flag.

## Current Limits

- Recovery is based on single autosave snapshot path in temp directory.
- Recovery status is displayed in shell status line; no dedicated status panel yet.
- Parse/read failures are reported but not auto-repaired.

## Exit Criteria (This Iteration)

- Dirty tracking is tied to edit-history-driven project mutations.
- Recovery entry from autosave is available when dirty session marker exists.
- Shell diagnostics show recovery status and dirty/autosave state each loop.
- Dirty/reset/recovery decisions are covered by tests.
- Phase artifact prepared in `docs/phase16_3_dirty_recovery_entry.md`.
