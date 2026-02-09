# Phase 5: Per-Track Groove/Scale Overrides

## Objective

Add per-track timing/scale control so each track can deviate from global groove and scale settings.

## Delivered

- `p9_core::model::Track`: added `groove_override: Option<GrooveId>`.
- `p9_core::model::Track`: added `scale_override: Option<ScaleId>`.
- `p9_core::engine`: new command `SetTrackGrooveOverride`.
- `p9_core::engine`: new command `SetTrackScaleOverride`.
- `p9_core::scheduler`: resolves effective groove/scale per track.
- `p9_core::scheduler`: track override has priority over song default.
- `p9_core::scheduler`: deterministic fallback if override/default is missing.
- `p9_core::scheduler`: tests for track groove override priority.
- `p9_core::scheduler`: tests for track scale override priority.
- `p9_app`: updated to drive stage-5 flow and configure track overrides.

## Behavior Summary

- Global song groove/scale remains default behavior.
- Track can explicitly override groove/scale without affecting other tracks.
- Playback stays deterministic when override data is absent.

## Current Limits

- Overrides are currently static config, not step-level automation.
- No UI/editor layer for track override management yet.
- No compile/test execution in this environment (`cargo`/`rustc` unavailable).

## Exit Criteria (This Iteration)

- Track model contains optional groove/scale override fields.
- Engine can set/clear per-track overrides.
- Scheduler uses effective per-track groove/scale selection.
- Unit tests cover override priority behavior.
