# Phase 4: Timing and Scale Core

## Objective

Extend tracker core playback with groove-aware timing and scale-aware note quantization.

## Delivered

- `p9_core::engine`: new commands `SetDefaultGroove`, `SetDefaultScale`, `UpsertGroove`, `UpsertScale`.
- `p9_core::scheduler`: per-step tick duration from active groove pattern.
- `p9_core::scheduler`: scale-aware note quantization in note event path.
- `p9_core::scheduler`: deterministic fallback when groove/scale data is missing.
- `p9_core::scheduler`: unit tests for groove timing and scale quantization.
- `p9_app`: updated to configure groove + scale and drive stage-4 flow.

## Behavior Summary

- Playback step duration is now dynamic per step when groove exists.
- Notes can be quantized to active scale before emitting `NoteOn`.
- If groove/scale is absent, scheduler keeps deterministic fallback behavior from previous stage.

## Current Limits

- Per-track groove/scale override commands are not implemented yet.
- Quantization strategy is simple nearest-step adjustment, no advanced musical heuristics.
- No real audio backend integration yet.
- No compile/test execution in this environment (`cargo`/`rustc` unavailable).

## Exit Criteria (This Iteration)

- Engine can set default groove/scale and upsert their data.
- Scheduler uses groove ticks in timing loop.
- Scheduler applies scale quantization in note emission path.
- Test coverage includes at least one groove timing case and one scale quantization case.
