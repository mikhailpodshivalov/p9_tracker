# Phase 6: Storage Round-Trip Expansion

## Objective

Expand project persistence so `p9_storage` can save and restore core tracker state, not only song name/tempo.

## Delivered

- `p9_storage::project::ProjectEnvelope::to_text()` now serializes:
- song defaults (`tempo`, `default_groove`, `default_scale`)
- track state (`mute`, `solo`, `groove_override`, `scale_override`)
- song arrangement rows (`track.{i}.row.{n}.chain`)
- chain rows (`phrase`, `transpose`)
- phrase steps (`note`, `velocity`, `instrument`)
- grooves (`ticks_pattern`)
- scales (`key`, `interval_mask`)
- `ProjectEnvelope::from_text()` now restores the same entities with index bounds checks.
- Added parser-level helpers for typed value decoding and option fields (`none` handling).
- Kept backward compatibility for legacy keys: `name` and `tempo`.
- Added tests:
- full arrangement + overrides + groove/scale round-trip
- legacy key compatibility
- invalid track index rejection

## Behavior Summary

- Storage format remains text-based with explicit keys and deterministic ordering.
- Only non-default chain/phrase rows are emitted to keep files compact.
- Deserialization creates missing `Chain`/`Phrase` containers on demand.
- Invalid indices fail fast with `StorageError::InvalidIndex`.

## Current Limits

- `Instrument`, `Table`, `Mixer`, and per-step/per-table `FX` are still not persisted.
- No schema migration layer beyond `format_version` and legacy song key aliases.
- No compile/test execution in this environment (`cargo`/`rustc` unavailable).

## Exit Criteria (This Iteration)

- Core tracker arrangement and timing/scale data survive save/load round-trip.
- Parser validates boundaries for track/song-row/chain-row/phrase-step indices.
- Backward compatibility with minimal legacy project text is preserved.
