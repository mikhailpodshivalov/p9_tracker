# Phase 8: Realtime Runtime Loop and Transport Queue

## Objective

Move from one-shot scheduler demo flow to deterministic runtime coordination with explicit transport command ingress.

## Delivered

- Added runtime coordinator in `p9_app`:
- `RuntimeCoordinator` owns scheduler lifecycle and tick execution.
- `RuntimeCommand` queue (`Start`, `Stop`, `Continue`, `Rewind`) with FIFO ordering.
- MIDI transport messages are mapped into runtime commands.
- Added runtime diagnostics structures:
- `TickReport` per tick (`events`, `midi messages`, `tick`, `playing state`).
- `TransportSnapshot` (`tick`, `is_playing`, queued/processed command counts).
- Updated `p9_app` main flow:
- runtime loop of 24 ticks now runs through coordinator.
- command queue is used for deterministic start/rewind at boot.
- MIDI input ingestion happens inside each loop iteration.
- scheduler events are forwarded to both audio and MIDI through runtime loop.
- Added deterministic tests in `crates/p9_app/src/runtime.rs`:
- command burst processing order before tick execution
- stop + rewind transport reset behavior
- MIDI transport mapping (`Start`, `Stop`) to runtime commands
- repeated command sequence determinism

## Behavior Summary

- Transport commands are now applied in a single, ordered ingress queue before each scheduler tick.
- Runtime state can be inspected via a snapshot without mutating scheduler internals.
- Event fanout path is explicit per tick:
- `Scheduler -> AudioBackend`
- `Scheduler -> MidiOutput`

## Current Limits

- Queue is in-process and single-owner; no cross-thread producer API yet.
- MIDI clock (`0xF8`) is decoded in `p9_rt`, but Phase 8 does not yet drive tick timing from external clock.
- Runtime still uses noop device backends in app-level flow.

## Exit Criteria (This Iteration)

- Runtime loop no longer manipulates scheduler transport directly from ad-hoc code.
- Transport command bursts are deterministic and test-covered.
- Runtime exposes tick/transport diagnostics for next-phase audio backend work.
