# Phase 16 Plan: Editing Workflow and Safety

## Objective

Turn shell-driven editing into a reliable workflow with history, structured selection tools, and recovery safety so longer editing sessions are practical.

## Subphases

## Phase 16.1: Undo/Redo Foundation

- Add project history model with bounded undo/redo stacks.
- Integrate history into shell edit command path.
- Expose user commands for undo (`u`) and redo (`y`).
- Add tests for deterministic rollback/replay behavior.

## Phase 16.2: Selection and Block Operations

- Add explicit selection model (start/end, track-local scope).
- Add copy/paste primitives for phrase step regions.
- Validate bounds and deterministic paste semantics.
- Add smoke tests for selection + paste workflows.

## Phase 16.3: Dirty State and Recovery Entry

- Add dirty tracking tied to editing history changes.
- Add startup recovery path from autosave snapshot when dirty session exists.
- Show clear recovery status in shell diagnostics.
- Add tests for dirty/reset/recovery decisions.

## Phase 16.4: Workflow Hardening and Regression Pack

- Add scenario regression tests for long command sequences.
- Add UX safeguards for destructive operations and invalid contexts.
- Consolidate command help and status taxonomy (`info/warn/error`).
- Close phase with end-to-end smoke script for "edit -> play -> save -> recover".

## Critical Path

`16.1 -> 16.2 -> 16.3 -> 16.4`

## Definition of Done (Phase 16)

- Editing history is reversible and deterministic.
- Block editing works within explicit cursor/selection constraints.
- Recovery path is test-covered and user-visible.
- Regression pack catches workflow breakage before merge.
