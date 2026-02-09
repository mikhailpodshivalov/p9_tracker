# Phase 15.3: Minimal Edit Loop in UI Shell

## Objective

Add first practical authoring commands to the shell so a user can create and hear a minimal pattern directly from UI flow.

## Delivered

- UI shell command map expanded with editing controls:
- `c`: ensure chain and bind selected song row to chain
- `f`: ensure phrase and bind selected chain row to phrase
- `i`: ensure default synth instrument for focused track
- `e`: write note into selected phrase step (deterministic seeded note)
- `+` / `-`: increase/decrease focused track mixer level
- Existing navigation/transport commands retained (`n/p/h/l/j/k/t/r/q`).
- Minimal loop behavior in shell now covers:
- arrangement binding (song row -> chain -> phrase)
- step content editing (note/velocity/instrument)
- basic mixer adjustment
- Shell rendering/version markers updated to `Phase 15.3`.
- Main diagnostics marker updated to `stage15.3 minimal-edit-loop`.

## Test Coverage

- `ui_shell::tests::shell_edit_commands_create_minimal_authoring_flow`
- `ui_shell::tests::shell_mixer_commands_change_track_level`
- Existing shell/navigation/transport tests remain green.

## Behavior Summary

- User can build a minimal playable pattern from shell without manual code/data injection.
- Cursor-based editing remains deterministic and screen-context aware.
- Track-level mixer edits are immediately reflected in UI snapshot.

## Current Limits

- Shell editing is still command-line based (one command per line), not direct cell key-entry.
- Only baseline edit operations are exposed; copy/paste, delete, and advanced instrument params are deferred.
- Phrase/chain IDs are deterministic defaults, not user-typed identifiers yet.

## Exit Criteria (This Iteration)

- Minimal authoring flow (bind + edit step + adjust level) works from shell commands.
- Resulting project state is validated by tests.
- Phase artifact prepared in `docs/phase15_3_minimal_edit_loop.md`.
