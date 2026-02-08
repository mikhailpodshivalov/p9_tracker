# Phase 2 Baseline (Implementation Bootstrap)

## Objective

Start Phase 2 with a minimal Rust implementation skeleton aligned with `docs/architecture_v0.md` and `docs/data_contracts_v0.md`.

## Delivered

- Rust workspace created at root `Cargo.toml`.
- Crates created:
- `crates/p9_core`: domain model v0, engine commands, scheduler, render events.
- `crates/p9_rt`: audio/midi/dsp interfaces with noop backends.
- `crates/p9_storage`: project envelope with `FORMAT_VERSION` validation.
- `crates/p9_app`: minimal end-to-end bootstrap pipeline.

## Architectural Mapping

- `UI` mapped to `p9_app` (temporary CLI shell).
- `Engine` and `Scheduler` mapped to `p9_core`.
- `Audio`, `MIDI`, `DSP/FX` mapped to `p9_rt` (stub level).
- `Storage` mapped to `p9_storage`.

## Data Contract Mapping

Implemented v0 structs for:

- `Song`, `Track`, `Chain`, `Phrase`, `Step`.
- `Instrument`, `Table`, `Groove`, `Scale`, `Mixer`, `FxCommand`.
- `ProjectData` aggregate.

## Current Limits

- No real audio backend integration yet.
- No serialization format persistence yet (only envelope/version validation).
- No UI implementation yet (CLI bootstrap only).
- No performance/RT verification executed in this environment.

## Tooling Constraint

- `cargo` and `rustc` are not available in the current environment.
- As a result, compile/test checks were not executed at this stage.

## Exit Criteria for This Baseline

- Workspace and crate boundaries reflect architecture v0.
- Domain v0 contracts are present in code.
- There is a minimal cross-component execution flow in `p9_app`.
