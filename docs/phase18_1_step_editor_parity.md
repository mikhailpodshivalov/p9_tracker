# Phase 18.1: Step Editor Parity

## Objective

Deliver GUI parity for the tracker editing baseline (`c/f/i/e` flow), with an explicit edit target model and user-visible safety statuses.

## Delivered

- Added edit workflow commands to GUI action endpoint:
- `edit_bind_chain` (shell parity with `c`)
- `edit_bind_phrase` (shell parity with `f`)
- `edit_ensure_instrument` (shell parity with `i`)
- `edit_write_step` (shell parity with `e`, supports optional `note/velocity/instrument`)
- `edit_write_step&clear=1` for safe step clear
- Added explicit editor state to `/state` payload:
- `editor.target` (focused track + song/chain row + resolved chain/phrase + step)
- `editor.bound_chain_id`
- `editor.bound_phrase_id`
- `editor.focused_instrument`
- `editor.instrument_ready`
- Added GUI Step Editor panel:
- buttons for `Bind Chain`, `Bind Phrase`, `Ensure Instrument`, `Write Step`, `Clear Step`
- note/velocity/instrument inputs for controlled write
- visible edit target and binding indicators
- Added keyboard parity shortcuts for editing flow:
- `C/F/I/E` -> bind chain / bind phrase / ensure instrument / write step
- Existing navigation + transport shortcuts retained.
- Kept safety-first status tags in command results (`info/warn/error`) with practical validation messages:
- missing chain before phrase bind
- missing phrase before step write
- missing instrument before note write

## Test Coverage

Updated `crates/p9_app/src/gui_shell.rs` tests:

- `edit_flow_parity_c_f_i_e_writes_step`
- `edit_write_step_warns_when_phrase_not_bound`
- state payload tests extended with `editor` block checks
- Existing transport/session/determinism tests remain green.

## Exit Criteria (18.1)

- Core GUI edit loop mirrors baseline shell flow (`c/f/i/e`) without terminal dependency.
- Edit target is explicit and visible in UI state.
- Invalid edit contexts produce clear `warn` statuses.
- Phase artifact prepared in `docs/phase18_1_step_editor_parity.md`.
