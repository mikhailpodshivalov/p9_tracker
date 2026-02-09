# Phase 12: Storage v2 and Migration

## Objective

Complete project persistence for MVP entities and add deterministic migration from storage `v1` to `v2`.

## Delivered

- Storage format version bumped:
- `FORMAT_VERSION` changed to `2`.
- `from_text` accepts `v1` and `v2`, migrates in-memory representation to `v2`.
- `to_text` always writes `v2`.
- `p9_storage::ProjectEnvelope` serialization expanded to include:
- `Instrument` data:
- type, name, table binding, `note_length_steps`
- send levels
- synth params (`waveform`, `attack_ms`, `release_ms`, `gain`)
- `Table` data:
- row `note_offset`, `volume`
- row FX slots
- phrase step FX slots
- mixer state:
- per-track levels
- master level
- send levels
- Parser/loader extensions:
- key parsers for instrument/table/mixer namespaces
- FX command parse/render (`CODE:VALUE`)
- strict index validation for track/row/fx-slot/table-row/mixer-track
- Backward compatibility:
- `v1` files with legacy `name`/`tempo` fields are accepted.
- missing `v2` entities load with deterministic defaults.

## Test Coverage

- `round_trip_preserves_instruments_tables_mixer_and_fx`
- `from_text_migrates_v1_to_v2`
- existing arrangement/override and bounds tests remain green
- full workspace test run passes

## Behavior Summary

- Project save/load now preserves full MVP editing state for `Instrument/Table/Mixer/FX`.
- Older `v1` projects remain loadable without manual migration steps.
- Saved files normalize to `v2`, reducing future compatibility branching.

## Current Limits

- No binary/compressed format yet; text format remains canonical.
- Migration path implemented for `v1 -> v2` only.
- Schema-level compatibility for future major versions is deferred.

## Exit Criteria (This Iteration)

- v2 storage includes all MVP entities introduced through Phase 11.
- v1 files load successfully and upgrade in memory.
- storage round-trip and migration behavior are covered by tests.
