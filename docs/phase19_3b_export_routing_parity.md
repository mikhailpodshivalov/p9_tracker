# Phase 19.3b: Export Routing Parity

## Objective

Apply the new routing contract (`track/master/send`) inside offline render, so exported audio reflects mixer/send routing deterministically instead of dry-only synthesis.

## Delivered

- Offline export now consumes routing fields from `RenderEvent::NoteOn`:
- `track_level` and `master_level` scale effective voice gain
- `send_mfx`, `send_delay`, `send_reverb` drive deterministic return buses
- Added deterministic routing state in export path:
- lightweight FX return model (`mfx` soft-clip, delay line, reverb low-pass tail)
- fixed coefficients and no randomness to keep exports reproducible
- Render loop now uses routed synthesis (`dry + returns`) for WAV output.
- Existing synth/sampler/external render mode behavior remains intact.
- Updated stage markers/output to `19.3b`.

## Test Coverage

- Added export tests:
- `mixer_levels_scale_export_energy`
- `send_routing_changes_export_signature`
- Existing deterministic export tests remain green:
- byte-for-byte determinism
- sampler-vs-synth differentiation
- mixed-mode long-session parity

## Exit Criteria (19.3b)

- Export path applies routing contract (track/master/send) deterministically.
- Routing differences measurably affect output signatures.
- Existing export regressions remain green.

## Next (19.3c)

Align realtime backend routing decisions with export path and expose routing-focused telemetry for parity checks.
