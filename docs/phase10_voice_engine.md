# Phase 10: Voice Engine and Instrument v1

## Objective

Deliver the first complete voice lifecycle for synth playback:

- scheduler note-on/note-off policy
- bounded voice allocator with deterministic stealing
- instrument-level synth fields used during runtime playback

## Delivered

- `p9_core::model` instrument v1 extensions:
- `SynthWaveform` enum (`Sine`, `Square`, `Saw`, `Triangle`)
- `SynthParams` (`waveform`, `attack_ms`, `release_ms`, `gain`)
- `Instrument` now includes:
- `note_length_steps`
- `synth_params`
- `p9_core::events::RenderEvent::NoteOn` now carries synth playback fields:
- `waveform`, `attack_ms`, `release_ms`, `gain`
- `p9_core::scheduler` voice lifecycle updates:
- per-track active-note tracking (`active_note`, `note_steps_remaining`)
- scheduled note-off emission at step boundaries
- immediate note-off on retrigger/mute to avoid stuck notes
- note length resolved from instrument (`note_length_steps`, default fallback to `1`)
- synth params resolved from instrument and attached to note-on event
- `p9_rt::voice` added:
- deterministic bounded `VoiceAllocator`
- voice retrigger handling (same track/note reuses slot)
- oldest-voice stealing when polyphony limit is reached
- `p9_rt::audio::NativeAudioBackend` integration:
- consumes note-on/note-off events into allocator
- exports voice metrics: `active_voices`, `max_voices`, `voices_stolen_total`
- `p9_app` runtime output updated to stage10 voice telemetry.

## Test Coverage

- `p9_core::scheduler`:
- note-off emitted after one step by default
- instrument `note_length_steps` extends note hold
- groove and override tests adapted for note-off lifecycle
- `p9_rt::voice`:
- note-on/note-off lifecycle
- bounded allocation + deterministic stealing
- retrigger slot reuse
- `p9_rt::audio`:
- allocator remains bounded in backend metrics path
- xrun/callback metrics and fallback behavior still covered
- Full workspace tests pass.

## Behavior Summary

- Playback now generates full note lifecycle instead of note-on only.
- Instrument fields influence runtime behavior:
- note duration via `note_length_steps`
- synth voice parameters attached to emitted note-on events
- Native backend tracks active polyphony and voice stealing deterministically.

## Current Limits

- Voice engine is control-path complete but still uses simulated audio callback path.
- Envelope/oscillator parameters are not rendered into real PCM synthesis yet.
- Per-instrument modulation and sampler voice model are deferred.

## Exit Criteria (This Iteration)

- Scheduler emits note-off events deterministically.
- Voice allocator is bounded and test-covered for steal/retrigger behavior.
- Instrument v1 synth fields are present and used in runtime event path.
