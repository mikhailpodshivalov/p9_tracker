# Phase 19.1b: Voice Release Transitions and Click-Risk Reduction

## Objective

Move from telemetry-only baseline (`19.1a`) to deterministic release-transition behavior that reduces abrupt note-off cuts.

## Delivered

- Added deferred release lifecycle in `p9_rt::voice::VoiceAllocator`:
- `note_off` now keeps non-short releases in a releasing state instead of immediate hard clear.
- release countdown progresses via `advance_release_envelopes()` on each audio callback.
- Added lifecycle metrics for release transitions:
- `release_deferred_total`
- `release_completed_total`
- `release_pending_voices`
- Kept immediate clear behavior for very short releases (`<= 2ms`) as explicit click-risk path.
- Wired new release metrics into:
- `p9_rt::audio::AudioMetrics`
- `p9_app::runtime::TickReport`
- `p9_app` runtime output line (`stage19.1b voice-release-transitions`)
- Updated GUI shell stage copy from `19.1a` to `19.1b`.

## Test Coverage

- `p9_rt::voice`:
- `note_off_enters_release_before_voice_is_cleared`
- `lifecycle_counters_capture_click_risk_signals` updated with release counters
- `p9_rt::audio`:
- `deferred_release_metrics_progress_after_callbacks`
- existing lifecycle/backend tests extended with new release counter assertions
- `p9_app::runtime`:
- `tick_report_exposes_audio_metrics` extended with release metric assertions

## Exit Criteria (19.1b)

- Note-off behavior distinguishes short-release hard cuts from longer deferred release transitions.
- Deferred-release lifecycle is deterministic and callback-driven.
- Runtime telemetry exposes release-deferred/completed/pending counters for regression gates.
- Phase artifact prepared in `docs/phase19_1b_voice_release_transitions.md`.

## Next (19.1c)

Tighten voice-stealing policy under polyphony pressure and validate audible-risk deltas under stress scenarios.
