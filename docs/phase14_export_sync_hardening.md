# Phase 14: Export, MIDI Sync, and Hardening

## Objective

Close MVP loop with offline export, clock sync mode control, continuous MIDI I/O flow, and baseline reliability guardrails.

## Delivered

- Offline export path (`wav`, mono 16-bit PCM):
- Added `p9_rt::export::render_project_to_wav`.
- Added `OfflineRenderConfig` and `ExportReport`.
- Deterministic scheduler-driven offline render that writes valid WAV headers and audio payload.
- Runtime sync mode model:
- Added `SyncMode` (`Internal`, `ExternalClock`).
- Added runtime sync mode API and runtime snapshot/report sync fields.
- External clock behavior: runtime tick advances only when MIDI clock (`0xF8`) is ingested.
- Internal clock behavior: runtime emits MIDI clock message each playing tick.
- Continuous MIDI device integration:
- Added `BufferedMidiInput` and `BufferedMidiOutput` in `p9_rt::midi`.
- Added `RuntimeCoordinator::run_cycle`/`run_cycle_safe` for per-cycle poll + tick flow.
- Reliability hardening primitives:
- Added `run_tick_safe`/`run_cycle_safe` panic boundaries (`RuntimeFault::TickPanic`).
- Added `AutosaveManager` + `AutosavePolicy` with interval-based save gate.
- `p9_app` stage flow updated to `stage14 export-sync-hardening`:
- runs external clock phase then internal clock phase
- performs storage round-trip
- performs offline WAV export
- performs autosave write check
- prints diagnostics for sync/export/autosave.

## Test Coverage

- `p9_rt::export`:
- `render_project_to_wav_writes_valid_riff_file`
- `render_project_to_wav_is_deterministic`
- `p9_rt::midi`:
- `buffered_input_drains_messages_in_poll`
- `buffered_output_records_messages`
- `p9_app::runtime`:
- `external_clock_mode_advances_only_on_clock_messages`
- `internal_sync_emits_clock_message_on_tick`
- `run_cycle_polls_midi_input_continuously`
- `run_tick_safe_catches_backend_panics`
- existing deterministic/runtime/audio tests remain in place.
- `p9_app::hardening`:
- `save_if_due_writes_snapshot_when_dirty`
- `save_if_due_skips_when_not_due_or_not_dirty`

## Behavior Summary

- Project can now be authored, played through runtime, and exported to WAV in one flow.
- Clock ownership is explicit and test-covered for both internal and external sync modes.
- MIDI I/O loop can run continuously without ad-hoc polling in app code.
- Runtime panic boundaries and autosave interval gate reduce crash-surface and data-loss risk.

## Current Limits

- Exporter uses lightweight synthetic render (single mono stream) and is not yet parity-checked against realtime audio backend output.
- MIDI sync is transport-clock focused; full external song-position-pointer workflows are deferred.
- Autosave uses simple interval+dirty policy without rotation/version retention.

## Exit Criteria (This Iteration)

- Offline WAV export exists and is deterministic.
- Internal/external clock sync mode behavior is implemented and test-covered.
- Reliability baseline (safe runtime wrapper + autosave gate) is implemented.
- Phase artifact prepared in `docs/phase14_export_sync_hardening.md`.
