# Phase 17.4: Project Session Workflow v1

## Objective

Add first-pass GUI session workflow without terminal dependency: `new/open/save/save-as/recent` with dirty-state confirmation for destructive actions.

## Delivered

- Added GUI session commands on `/action` endpoint:
- `session_new`
- `session_open` (with `path`)
- `session_save` (current path or explicit `path`)
- `session_save_as` (with `path`)
- `session_recent`
- Added dirty confirmation gate for destructive actions:
- `quit`, `session_new`, `session_open`
- Server returns `confirm_required=true` when unsaved changes exist and `force=1` is missing.
- Added GUI session UX panel:
- path input
- buttons for `New/Open/Save/Save As/Recent`
- current project path indicator
- clickable recent project list
- Added session block to `/state` snapshot:
- `session.current_path`
- `session.recent[]`
- Integrated save/open pipeline with storage envelope (`ProjectEnvelope`):
- save writes current project to path
- open loads, validates format, and replaces engine project
- Added first-pass session lifecycle behavior:
- successful `new/open` resets UI cursor state and queues `Stop + Rewind`
- successful `save/save-as` marks tracker state as clean and updates recent list
- Updated stage marker to `stage17.4 project-session-workflow`.

## Test Coverage

Updated/added tests in `crates/p9_app/src/gui_shell.rs`:

- `session_new_requires_confirmation_when_dirty`
- `session_save_as_and_open_roundtrip_updates_recent`
- Existing transport/action/state determinism tests retained and updated for `session` payload.

## Exit Criteria (17.4)

- GUI supports `new/open/save/save-as/recent` end-to-end.
- Dirty sessions are guarded by explicit confirmation before destructive actions.
- Project session workflow is operable from GUI without terminal commands.
- Phase artifact prepared in `docs/phase17_4_project_session_workflow.md`.
