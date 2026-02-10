# Phase 19.1a: Voice Lifecycle + Click-Risk Metrics Baseline

## Objective

Establish measurable telemetry for voice lifecycle health and click-risk signals before changing synthesis behavior.

## Delivered

- Added lifecycle instrumentation in `p9_rt::voice::VoiceAllocator`:
- `note_on_total`
- `note_off_total`
- `note_off_miss_total`
- `retrigger_total`
- `zero_attack_total` (attack <= 1ms)
- `short_release_total` (release <= 2ms)
- `click_risk_total` (aggregated risk signal)
- Added `VoiceLifecycleStats` snapshot API in voice allocator.
- Wired lifecycle stats into `p9_rt::audio::AudioMetrics`:
- voice note-on/off counters
- missed note-off counter
- retrigger counter
- zero-attack / short-release counters
- aggregated click-risk counter
- Extended `p9_app::runtime::TickReport` with new audio telemetry fields.
- Updated stage runtime output to include lifecycle/click-risk counters.
- Updated GUI shell stage markers to `19.1a` while preserving 18.4 workflow polish behavior.

## Test Coverage

- `p9_rt::voice`:
- `lifecycle_counters_capture_click_risk_signals`
- `p9_rt::audio`:
- `lifecycle_metrics_surface_retrigger_and_short_release`
- existing backend metric tests extended with lifecycle assertions
- `p9_app::runtime`:
- `tick_report_exposes_audio_metrics` extended with lifecycle/click-risk assertions

## Exit Criteria (19.1a)

- Lifecycle and click-risk telemetry is deterministic and available from audio backend metrics.
- Runtime report surfaces telemetry for integration and regression gating.
- Regression tests cover baseline lifecycle/click-risk accounting paths.
- Phase artifact prepared in `docs/phase19_1a_voice_click_metrics.md`.

## Next (19.1b)

Use these counters to validate envelope/release transitions and reduce real click-risk paths rather than guessing by ear.
