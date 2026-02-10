# Phase 19.1c: Polyphony Pressure Voice-Steal Policy

## Objective

Reduce audible risk during polyphony bursts by preferring already-releasing voices as steal candidates, and expose deterministic pressure metrics.

## Delivered

- Updated `p9_rt::voice::VoiceAllocator` steal policy:
- when polyphony is saturated, allocator now tries to steal a releasing voice first
- releasing candidate priority: lower `release_pending_blocks`, then older `started_at`
- fallback remains oldest active voice when no releasing candidate exists
- Added lifecycle counters for pressure behavior:
- `steal_releasing_total`
- `steal_active_total`
- `polyphony_pressure_total`
- Refined click-risk accounting:
- stealing active voices increments `click_risk_total`
- stealing releasing voices does not increment click-risk
- Wired new metrics to:
- `p9_rt::audio::AudioMetrics`
- `p9_app::runtime::TickReport`
- `p9_app` stage output (`stage19.1c voice-steal-policy`)
- Updated GUI shell phase markers to `19.1c`.

## Test Coverage

- `p9_rt::voice`:
- `stealing_prefers_releasing_voice_under_polyphony_pressure`
- existing lifecycle and bounded-polyphony tests extended with steal/pressure assertions
- `p9_rt::audio`:
- `stress_burst_prefers_releasing_steals_before_active_steals`
- existing metrics tests extended with steal/pressure assertions
- `p9_app::runtime`:
- `tick_report_exposes_audio_metrics` extended with zero-value assertions for new counters

## Exit Criteria (19.1c)

- Under pressure, releasing voices are chosen before active voices for steals.
- Runtime counters separate low-risk release-tail steals from higher-risk active steals.
- Stress scenario proves deterministic pressure accounting and click-risk delta behavior.
- Phase artifact prepared in `docs/phase19_1c_voice_steal_policy.md`.

## Next (19.1d)

Add configurable steal-profile tuning (safe/aggressive) and compare metrics deltas on longer stress runs.
