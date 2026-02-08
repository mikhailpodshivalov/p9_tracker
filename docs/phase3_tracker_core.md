# Phase 3: Tracker Core Iteration

## Objective

Deliver the first deterministic tracker-core playback behavior on top of the Phase 2 bootstrap.

## Delivered

- `p9_core::engine` expanded with editing commands:
- `SetChainRowPhrase`
- `SetPhraseStep`
- `p9_core::scheduler` upgraded from row-0 stub to deterministic traversal:
- independent track playback state
- step timing (`ticks_per_step` from `ppq`)
- traversal `song -> chain -> phrase -> step`
- basic chain-row transpose application
- mute/solo-aware audibility filtering
- `p9_storage::project` extended with minimal text round-trip:
- `ProjectEnvelope::to_text()`
- `ProjectEnvelope::from_text()`
- `p9_app` switched to new command flow and multi-tick playback simulation.

## Technical Notes

- Scheduler now keeps explicit per-track cursors:
- `song_row`, `chain_row`, `phrase_step`, `tick_in_step`
- Step events are emitted only at step boundary (`tick_in_step == 0`).
- On chain end (empty/non-playable row), scheduler moves to next playable song row.

## Current Limits

- No note-off/retrigger semantics yet.
- No groove/scale timing logic in playback path yet.
- No real audio backend; noop backend only.
- No compile/test run in this environment (`cargo`/`rustc` not available).

## Exit Criteria (This Iteration)

- Playback is no longer hardcoded to row 0.
- Engine supports programmatic phrase/step editing commands.
- Baseline app produces events through deterministic scheduler flow.
