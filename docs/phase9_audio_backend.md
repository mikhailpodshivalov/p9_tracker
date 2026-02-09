# Phase 9: Native Audio Backend Layer and Runtime Metrics

## Objective

Extend the audio path beyond noop-only mode by adding a native backend layer with runtime telemetry and explicit fallback behavior.

## Delivered

- `p9_rt::audio` expanded with backend runtime model:
- `AudioBackendConfig` for sample rate, buffer size, callback budget, and start-failure simulation.
- `AudioMetrics` snapshot (`callbacks`, `xruns`, callback timings, buffer/sample config).
- `AudioBackendError` and `start_checked()` for start validation.
- New backend implementation:
- `NativeAudioBackend` (`native-simulated-linux`) with callback-time accounting and xrun detection via DSP budget.
- Existing backend upgraded:
- `NoopAudioBackend` now exposes metrics and backend identity.
- Backend selection and failover:
- `build_preferred_audio_backend(prefer_native)` to choose primary backend.
- `start_with_noop_fallback(primary)` to switch to noop when primary start fails.
- Runtime integration (`p9_app`):
- app boot now starts audio via preferred backend + fallback path.
- runtime tick reports carry audio metrics and backend identity.
- final stage output includes backend mode and telemetry.

## Test Coverage

- `p9_rt::audio`:
- native backend metrics and xrun accounting.
- fallback to noop when native backend start fails.
- `p9_app::runtime`:
- tick report exposes audio backend metrics.
- existing phase 8 transport determinism tests remain green.
- Full workspace test run executed successfully.

## Behavior Summary

- Runtime now has measurable audio callback behavior per tick.
- Xrun-like conditions are represented via budget overruns in the native backend layer.
- Startup path is robust: if primary backend fails to initialize, runtime switches to noop backend and keeps execution deterministic.

## Current Limits

- Native backend is currently a deterministic simulated callback model, not a physical ALSA/JACK/PipeWire device stream yet.
- Metrics are callback-level and internal; no external monitoring endpoint or UI panel yet.
- Device enumeration and backend-specific tuning are deferred to next iterations.

## Exit Criteria (This Iteration)

- Audio backend contract supports metrics and checked startup.
- Runtime has backend failover path to noop.
- Audio metrics are accessible from runtime tick reports and covered by tests.
