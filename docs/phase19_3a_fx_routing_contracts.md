# Phase 19.3a: FX Routing Contracts

## Objective

Establish explicit routing fields for mixer and send behavior in the render-event contract, so downstream runtime/export paths can apply the same routing decisions deterministically.

## Delivered

- Extended `RenderEvent::NoteOn` with routing fields:
- `track_level`
- `master_level`
- `send_mfx`
- `send_delay`
- `send_reverb`
- Scheduler now resolves and forwards routing context per note:
- track and master levels from `project.mixer`
- effective send levels from `instrument.send_levels` scaled by `project.mixer.send_levels`
- Added deterministic send-scaling helper (`instrument * global / 127`) to avoid hidden routing heuristics.
- Updated stage markers/output to `19.3a`.

## Test Coverage

- Added scheduler test:
- `mixer_routing_levels_are_forwarded_to_render_event`
- verifies track/master levels and scaled send values are emitted correctly.
- Existing scheduler, runtime, export, and app tests remain green after contract change.

## Exit Criteria (19.3a)

- Routing metadata exists in render events and is emitted by scheduler.
- Send-level composition rule is explicit and covered by tests.
- Stage/docs updated for `19.3a`.

## Next (19.3b)

Apply routing fields in offline render path (dry/send behavior) and add deterministic routing-parity tests for mixed sessions.
