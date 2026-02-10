# Phase 19.2c: Explicit Render-Mode Tagging

## Objective

Replace heuristic sampler detection with explicit render-mode tags and keep scheduler, realtime runtime, and export paths aligned.

## Delivered

- Added explicit mode enum in core events:
- `RenderMode::Synth`
- `RenderMode::SamplerV1`
- `RenderMode::ExternalMuted`
- Extended `RenderEvent::NoteOn` with `render_mode`.
- Updated scheduler instrument profile resolution:
- maps `InstrumentType` directly to `RenderMode`
- keeps existing envelope/length shaping rules from 19.2a
- Export path now uses explicit `render_mode` instead of attack/release heuristics.
- Realtime audio path now uses explicit `render_mode` for silent external bypass.
- Added runtime observability counter:
- `voice_sampler_mode_note_on_total` (via `AudioMetrics` and `TickReport`)
- Updated stage markers/output to `19.2c`.

## Test Coverage

- `p9_core::scheduler`:
- sampler profile test now asserts `RenderMode::SamplerV1`
- midi-out profile test now asserts `RenderMode::ExternalMuted`
- `p9_rt::audio`:
- `sampler_render_mode_note_on_is_counted`
- zero-gain/external bypass tests remain green
- `p9_rt::export`:
- existing sampler and midiout consistency tests remain green with explicit mode path
- overall crates (`p9_core`, `p9_rt`, `p9_app`) pass test suites.

## Exit Criteria (19.2c)

- Render mode is explicit in event contracts (no sampler heuristics required).
- Scheduler/runtime/export consume the same mode signal consistently.
- Runtime metrics expose sampler-mode note-on activity for regression tracking.

## Next (19.2d)

Introduce optional per-instrument render-variant parameters (still deterministic) and extend parity checks with longer mixed-mode sessions.
