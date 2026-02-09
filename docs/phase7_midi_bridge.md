# Phase 7: MIDI Bridge and Transport Mapping

## Objective

Add a minimal but deterministic MIDI I/O bridge between tracker render events and runtime transport control.

## Delivered

- `p9_rt::midi` expanded from stubs to utility layer:
- `decode_message(MidiMessage) -> DecodedMidi` for note and transport messages.
- `render_event_to_midi(&RenderEvent) -> MidiMessage` mapping tracker events to MIDI.
- `forward_render_events(&[RenderEvent], &mut dyn MidiOutput)` batch forwarding helper.
- `NoopMidiOutput` counter kept for deterministic tests and smoke checks.
- Unit tests added in `p9_rt::midi`:
- note on/off decode
- transport decode (`Start`, `Stop`)
- render-event-to-MIDI mapping
- forwarded message count verification
- `p9_app` runtime flow updated:
- polls MIDI input once before playback loop
- maps incoming `Start/Continue/Stop` to scheduler transport
- forwards scheduler render events to MIDI output
- stage marker moved to `stage7 midi`

## Behavior Summary

- Scheduler note events now have a direct MIDI output path.
- MIDI transport messages can control scheduler playback state.
- Mapping is deterministic:
- MIDI channel is derived from `track_id & 0x0F`.
- `RenderEvent::NoteOn` maps to `0x9n`.
- `RenderEvent::NoteOff` maps to `0x8n`.

## Current Limits

- Runtime still uses noop MIDI backends only; no ALSA/JACK/PipeWire device backend yet.
- MIDI clock handling is decoded but not integrated into tick timing.
- MIDI input is currently polled at bootstrap, not in continuous realtime loop.

## Exit Criteria (This Iteration)

- There is a tested conversion path `RenderEvent -> MidiMessage`.
- Runtime can react to MIDI transport start/stop/continue commands.
- Stage 7 artifact is documented and linked in project README.
