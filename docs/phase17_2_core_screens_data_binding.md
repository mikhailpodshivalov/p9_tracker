# Phase 17.2: Core Screens and Data Binding

## Objective

Render `Song/Chain/Phrase/Mixer` views in the non-terminal GUI and bind them to live engine/runtime snapshots through a deterministic refresh path.

## Delivered

- Expanded GUI shell state contract (`/state`) to structured snapshot payload:
- `transport` section (`tick`, `playing`, `tempo`)
- `cursor` section (`track`, `song_row`, `chain_row`, `phrase_id`, `step`, `track_level`)
- `views.song`, `views.chain`, `views.phrase`, `views.mixer`
- Added deterministic view builders in `gui_shell`:
- song row window around current cursor
- chain row window with bound-chain resolution
- phrase step table with note/velocity/instrument/fx and scale status (`in/out/none/unknown`)
- mixer table with focused track, master level, and send levels
- Upgraded GUI HTML/JS layer:
- four concrete screen panels in browser (`Song/Chain/Phrase/Mixer`)
- live data-binding from `/state` into each panel
- active-screen highlighting synchronized with `UiScreen`
- retained existing transport/navigation controls and action routing
- Updated stage marker to `stage17.2 core-screens-data-binding`.

## Determinism and Separation

- Runtime tick progression remains inside the same shell loop cadence (`16ms` sleep + safe tick).
- `/state` payload is built via explicit component-specific functions:
- `build_song_view_json`
- `build_chain_view_json`
- `build_phrase_view_json`
- `build_mixer_view_json`
- Windowing strategy is fixed-size and deterministic (`SONG_VIEW_ROWS=8`, `CHAIN_VIEW_ROWS=8`).

## Test Coverage

Added/updated `crates/p9_app/src/gui_shell.rs` tests:

- `build_state_json_contains_core_fields`
- `build_state_json_includes_bound_entities`
- `build_state_json_is_deterministic_for_same_snapshot`
- existing request parsing and action routing tests remain.

## Exit Criteria (17.2)

- GUI shows all four core tracker screens with live runtime/engine data.
- Data binding updates on refresh loop without introducing non-deterministic ordering.
- State snapshot contract and tests cover core screen payload composition.
- Phase artifact prepared in `docs/phase17_2_core_screens_data_binding.md`.
