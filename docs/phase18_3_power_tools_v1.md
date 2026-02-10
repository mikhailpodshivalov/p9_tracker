# Phase 18.3: Power Tools v1

## Objective

Add productivity tools for repetitive authoring in GUI: block-level edit transforms and quick row-structuring actions.

## Delivered

- Added GUI power commands on `/action`:
- `edit_power_duplicate` (safe duplicate to next block, optional `force=1`)
- `edit_power_fill` (`note/velocity/instrument` optional; seeded-note fill by default)
- `edit_power_clear_range` (full clear for selected range including FX)
- `edit_power_transpose` (`delta`, default `+1`, clamped)
- `edit_power_rotate` (`shift`, default `+1`, cyclic block rotate)
- `edit_power_transpose_up` / `edit_power_transpose_down`
- `edit_power_rotate_right` / `edit_power_rotate_left`
- Added row quick actions for repetitive structure work:
- `edit_song_clone_prev` copies previous song-row chain binding into current row
- `edit_chain_clone_prev` copies previous chain-row phrase/transposition into current row
- Added safety guards:
- all power tools require valid current selection scope
- instrument validation for fill
- explicit warnings for out-of-range, empty/no-op, and context mismatch
- Added GUI controls and shortcuts:
- buttons for duplicate/fill/clear/transpose/rotate and song/chain clone helpers
- shortcut bindings: `D`, `B`, `M`, `[`, `]`, `,`, `.`
- status panel keeps `info/warn/error` semantics
- Extended editor state payload:
- `selection_start`, `selection_end`, `clipboard_len`
- `selection/clipboard` indicator now shows span and clipboard size

## Test Coverage

Updated tests in `crates/p9_app/src/gui_shell.rs`:

- `edit_power_tools_fill_transpose_rotate_clear`
- `edit_power_duplicate_and_row_clone_helpers_work`
- core state payload test extended for `selection_start/selection_end/clipboard_len`

All `p9_app` tests pass with the new behavior.

## Exit Criteria (18.3)

- GUI supports repetitive block editing without terminal fallback.
- Song/chain repetitive row setup can be done with one action.
- New tools are deterministic, guard invalid contexts, and are regression-covered.
- Phase artifact prepared in `docs/phase18_3_power_tools_v1.md`.
