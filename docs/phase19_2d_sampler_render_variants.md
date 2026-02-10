# Phase 19.2d: Sampler Render Variants and Mixed-Mode Parity

## Objective

Add deterministic per-instrument sampler render variant parameters and verify parity across longer mixed-mode playback sessions.

## Delivered

- Extended core model with optional sampler render parameters per instrument:
- `sampler_render.variant` (`classic`, `punch`, `air`)
- `sampler_render.transient_level` (`0..127`)
- `sampler_render.body_level` (`0..127`)
- Extended `RenderEvent::NoteOn` contract with explicit sampler render fields:
- `sampler_variant`
- `sampler_transient_level`
- `sampler_body_level`
- Scheduler now forwards instrument-level sampler render parameters into emitted note events.
- Export renderer applies sampler variant + transient/body mix deterministically (no randomness, no hidden state).
- Storage format v2 now round-trips sampler render parameters:
- serializer emits `instrument.{id}.sampler.*` keys when configured
- parser accepts and validates these keys into instrument state
- Updated runtime stage markers/output to `19.2d`.

## Test Coverage

- `p9_core::scheduler`
- `sampler_render_params_are_forwarded_to_render_event`
- `p9_storage::project`
- instrument sampler params survive text round-trip
- `p9_rt::export`
- existing explicit render-mode tests remain green
- added long mixed-mode deterministic parity test that:
- combines synth, sampler, and external-muted flows
- asserts run-to-run deterministic signature
- asserts changed sampler variant profile produces different signature
- Full suites pass for `p9_core`, `p9_rt`, and `p9_app`.

## Exit Criteria (19.2d)

- Sampler render shaping is explicit in data contracts and event path.
- Scheduler, runtime/export behavior remains deterministic.
- Longer mixed-mode parity checks exist and are green.

## Next (19.3a)

Start FX routing/mixer behavior work: chain order guarantees, send-return semantics, and deterministic bypass rules.
