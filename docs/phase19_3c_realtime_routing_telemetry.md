# Phase 19.3c: Realtime Routing Telemetry

## Objective

Align realtime backend routing decisions with export gain-routing behavior and expose routing-focused telemetry for regression checks.

## Delivered

- Realtime backend now applies routed gain before voice allocation:
- effective gain is computed from `gain * track_level * master_level / (127 * 127)`
- note-ons that collapse to zero gain are treated as silent and skipped
- Added routing telemetry counters in `AudioMetrics`:
- `voice_mixer_muted_note_on_total`
- `voice_send_routed_note_on_total`
- `voice_send_level_total`
- Extended runtime `TickReport` and app status aggregation to include new routing counters.
- Added routing-aware checks in audio backend tests.
- Updated stage markers/output to `19.3c`.

## Test Coverage

- Added audio backend tests:
- `mixer_zero_level_mutes_note_on_and_counts_routing_mute`
- `send_routing_activity_is_tracked`
- Existing runtime tick-report metrics test updated for new routing counters.
- Full suites stay green for `p9_rt` and `p9_app`.

## Exit Criteria (19.3c)

- Realtime note allocation respects mixer-derived effective gain.
- Routing telemetry is available in runtime report and app output.
- Deterministic routing regression tests remain green.

## Next (19.3d)

Finish Phase 19.3 by validating complex FX command interactions against routing behavior and documenting routing safety constraints.
