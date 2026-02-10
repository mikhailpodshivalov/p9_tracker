# Phase 19.2a: Instrument Profiles v0

## Objective

Start Phase 19.2 by introducing deterministic playback profiles per `InstrumentType`, so instrument modes produce distinct runtime behavior without breaking core scheduling flow.

## Delivered

- Added instrument profile resolver in `p9_core::scheduler`:
- `Synth`: existing behavior unchanged
- `Sampler`: tighter onset (`attack_ms <= 1`), longer release floor (`release_ms >= 24`), minimum `note_length_steps = 2`
- `MidiOut` / `External`: internal gain muted (`gain = 0`) to avoid doubled internal audio path; short envelope cap for deterministic transitions
- `None`: fallback to default synth profile
- Replaced separate note-length/synth-param resolution with single profile resolution path.
- Updated stage markers to `19.2a` in runtime output and GUI shell copy.

## Test Coverage

- Added scheduler tests:
- `sampler_profile_shapes_envelope_and_note_length`
- `midiout_profile_mutes_internal_gain`
- Existing scheduler regression tests remain green.

## Exit Criteria (19.2a)

- Instrument mode affects playback profile deterministically.
- Sampler and MidiOut modes are covered by direct scheduler tests.
- Stage/docs updated for `19.2a`.

## Next (19.2b)

Introduce a deeper synthesis increment (hybrid layer or sampler-v1 playback detail) and validate export/runtime consistency for the new instrument path.
