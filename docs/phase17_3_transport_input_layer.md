# Phase 17.3: Transport and Input Layer

## Objective

Complete GUI transport controls, keyboard routing, and runtime status indicators for non-terminal operation.

## Delivered

- Added explicit transport actions in GUI shell endpoint:
- `play` -> queue `RuntimeCommand::Start`
- `stop` -> queue `RuntimeCommand::Stop`
- existing `toggle_play` and `rewind` retained
- Added keyboard routing in browser UI:
- `Space/T` toggle play
- `G` play, `S` stop, `R` rewind
- `N/P` screen next/prev
- arrows or `H/J/K/L` navigation parity
- `X` toggle scale hint
- `Q` quit GUI shell
- Extended state payload (`/state`) with status block:
- transport state (`play/stop`)
- recovery status label
- dirty flag indicator
- autosave status
- runtime queue counters (`queued_commands`, `processed_commands`)
- Integrated GUI shell with hardening primitives:
- dirty-session recovery check on startup
- dirty-flag tracking and autosave cadence in runtime loop
- status propagation into GUI state panel
- Updated stage marker to `stage17.3 transport-input-layer`.

## Test Coverage

Added/updated tests in `crates/p9_app/src/gui_shell.rs`:

- `apply_gui_command_queues_explicit_transport_commands`
- `build_state_json_contains_core_fields` now checks status/recovery fields
- existing parsing/action/determinism tests retained

## Behavior Notes

- Runtime status indicators are refreshed through the same deterministic polling path as screens.
- Autosave/dirty indicators are best-effort in GUI shell and share existing hardening logic used in terminal shell.
- In restricted environments where local bind is blocked, GUI shell launch still reports `unable to bind GUI shell listener`.

## Exit Criteria (17.3)

- GUI transport controls support explicit play/stop/rewind paths.
- Keyboard navigation parity is available without terminal shell.
- Status panel shows tick/transport/recovery/dirty-related indicators.
- Phase artifact prepared in `docs/phase17_3_transport_input_layer.md`.
