# Phase 19.2b: Sampler Export Detail and Runtime Consistency

## Objective

Add a deeper instrument-depth increment for sampler-like playback and align runtime behavior for external/zero-gain instrument paths.

## Delivered

- Extended `p9_rt::export` voice rendering:
- added `SamplerV1` render mode (hybrid body + deterministic transient layer)
- applied attack/release envelope during offline synthesis
- switched note-off handling from hard cut to release-tail progression
- skip creating offline voices when `gain == 0` (external/midi-like path)
- Added runtime consistency guard in `p9_rt::audio`:
- note-on events with `gain == 0` no longer allocate internal voices
- new metric `voice_silent_note_on_total` tracks bypassed note-ons
- Wired new metric through:
- `p9_rt::audio::AudioMetrics`
- `p9_app::runtime::TickReport`
- `p9_app` stage output (`stage19.2b sampler-export-runtime`)
- Updated GUI shell stage markers to `19.2b`.

## Test Coverage

- `p9_rt::export`:
- `render_project_to_wav_midiout_profile_is_silent`
- `render_project_sampler_profile_differs_from_synth_profile`
- existing deterministic export tests stay green
- `p9_rt::audio`:
- `zero_gain_note_on_is_counted_without_allocating_voice`
- existing lifecycle/pressure tests remain green
- `p9_app::runtime`:
- `tick_report_exposes_audio_metrics` extended with `audio_voice_silent_note_on_total`

## Exit Criteria (19.2b)

- Sampler-like path produces a distinct deterministic offline waveform profile.
- Zero-gain instrument events stay external-only in both export and runtime audio backend paths.
- Metrics expose this path for regression gating and observability.

## Next (19.2c)

Add explicit instrument render mode tagging (instead of heuristic detection) and validate parity across scheduler, runtime, and export.
