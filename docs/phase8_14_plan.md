# Post-Phase-7 Development Plan (Phase 8 -> Phase 14)

## Objective

Define concrete next phases after Phase 7, keeping the same incremental delivery style:

- small vertical slices
- explicit limits per phase
- deterministic tests
- documented exit criteria

This plan is implementation-oriented but **does not include code changes**.

## Planning Baseline

- Current state: `Phase 7 MIDI bridge completed`.
- Stack remains `Rust` workspace (`p9_core`, `p9_rt`, `p9_storage`, `p9_app`).
- Each phase must end with:
- one phase artifact in `docs/`
- passing targeted tests
- updated `README.md` phase status and artifact link

## Phase Map

| Phase | Name | Effort (solo) | Primary Outcome | Depends On |
|---|---|---:|---|---|
| 8 | Realtime Runtime Loop | 1.5 weeks | deterministic runtime loop with continuous transport handling | 7 |
| 9 | Native Audio Backend + Metrics | 2 weeks | real device output and xrun/latency telemetry | 8 |
| 10 | Voice Engine + Instrument v1 | 2 weeks | audible synth path with note-on/note-off lifecycle | 9 |
| 11 | Tracker Editing Depth (FX/Table/Mixer v1) | 2 weeks | practical editing commands and playback interpretation | 10 |
| 12 | Storage v2 + Migration | 1.5 weeks | complete project persistence for MVP entities | 11 |
| 13 | UI Alpha (Tracker Workflow) | 2.5 weeks | usable tracker editing screen and transport controls | 11, 12 |
| 14 | Export + MIDI Device Sync + Hardening | 2 weeks | offline render, real MIDI sync modes, stabilization pass | 9, 10, 12, 13 |

Estimated total: `13.5 weeks` for one developer.

## Phase 8: Realtime Runtime Loop

## Objective

Move from single-shot demo flow to continuous runtime behavior with deterministic transport ownership.

## Scope

- In: continuous tick loop, scheduler lifecycle (`start/stop/rewind`), command ingress queue, transport snapshot.
- In: integrate MIDI transport messages into runtime loop (not bootstrap-only).
- Out: real audio device backend and DSP-heavy work.

## Planned Deliverables

- Runtime coordinator in `p9_app` with stable update cadence.
- Queue contract for non-RT commands entering engine/scheduler.
- Extended transport state visibility for diagnostics.
- Tests for transport transitions and deterministic tick progression.

## Exit Criteria

- Start/stop/continue/rewind are deterministic under repeated command bursts.
- No direct shared mutable state between control path and runtime loop.
- Phase artifact prepared: `docs/phase8_realtime_runtime.md`.

## Primary Risks

- Race conditions around transport commands.
- Drift between scheduler ticks and runtime cadence assumptions.

## Phase 9: Native Audio Backend + Runtime Metrics

## Objective

Replace audio noop path with real backend integration while preserving deterministic scheduler behavior.

## Scope

- In: real backend abstraction implementation (target first: `cpal` on Linux).
- In: runtime metrics (`callback time`, `xruns`, `buffer size`, `sample rate`).
- In: backend failover path to noop.
- Out: advanced low-latency tuning UI.

## Planned Deliverables

- `p9_rt::audio` real backend implementation behind current trait.
- Metrics collector exported to app diagnostics.
- Smoke test command path proving real-device playback loop starts/stops.

## Exit Criteria

- Audio backend starts and stops cleanly on supported Linux host.
- Runtime metrics are visible and test-covered where possible.
- Phase artifact prepared: `docs/phase9_audio_backend.md`.

## Primary Risks

- Host-specific backend instability.
- Callback overrun under debug builds.

## Phase 10: Voice Engine + Instrument v1

## Objective

Deliver first audible instrument with complete voice lifecycle.

## Scope

- In: note-on/note-off scheduling path completion.
- In: simple synth voice (`oscillator + envelope + gain`).
- In: per-track level path through mixer baseline.
- Out: sampler and complex modulation matrix.

## Planned Deliverables

- Scheduler support for note-off emission policy.
- DSP voice allocator (polyphony limits + stealing policy).
- Basic synth instrument model fields for runtime playback.
- Tests for voice lifecycle and stuck-note prevention.

## Exit Criteria

- Musical phrase playback produces audible note-on/note-off behavior.
- Voice allocator remains bounded and deterministic.
- Phase artifact prepared: `docs/phase10_voice_engine.md`.

## Primary Risks

- Voice leaks or stuck notes.
- CPU spikes during polyphony bursts.

## Phase 11: Tracker Editing Depth (FX/Table/Mixer v1)

## Objective

Expand editing and playback semantics to a practical tracker MVP set.

## Scope

- In: engine commands for step FX slots, table rows, mixer edits.
- In: scheduler interpretation for selected MVP FX commands.
- In: validation rules for FX/table command ranges.
- Out: full advanced FX ecosystem.

## Planned Deliverables

- Command surface for editing FX/table/mixer data.
- Playback interpretation for selected MVP commands (volume/pitch/timing-safe set).
- Unit tests for command validation and playback side effects.

## Exit Criteria

- Users can express non-trivial phrase behavior via commands/tables.
- Invalid command data is rejected predictably.
- Phase artifact prepared: `docs/phase11_fx_table_mixer.md`.

## Primary Risks

- Feature creep in FX command surface.
- Non-deterministic command ordering in playback.

## Phase 12: Storage v2 + Migration

## Objective

Make project format complete for MVP entities and safe for forward changes.

## Scope

- In: persist instruments, tables, mixer, FX command data.
- In: `format_version` migration path from v1 to v2.
- In: malformed-file validation strategy.
- Out: compressed/binary format optimization.

## Planned Deliverables

- `ProjectEnvelope` v2 serialization/deserialization coverage.
- Backward compatibility loader for v1 files.
- Fixtures and migration tests (`v1 -> v2`).

## Exit Criteria

- Saving and loading preserves complete MVP state.
- Versioned migrations are deterministic and test-covered.
- Phase artifact prepared: `docs/phase12_storage_v2.md`.

## Primary Risks

- Breaking compatibility with earlier snapshots.
- Under-specified migration behavior for missing fields.

## Phase 13: UI Alpha (Tracker Workflow)

## Objective

Deliver first usable tracker UI workflow on top of runtime core.

## Scope

- In: song/chain/phrase editing screens, transport controls, basic mixer view.
- In: keyboard-driven navigation and editing actions.
- In: visual states for scale feedback and track focus workflow.
- Out: polished theme system and advanced customization.

## Planned Deliverables

- UI shell connected to engine command pipeline.
- Minimal edit loop: create phrase, edit steps, assign instrument, play/stop.
- Deterministic state refresh path from engine/runtime snapshots.
- UI smoke tests for critical input flows.

## Exit Criteria

- End-to-end authoring flow works without manual data injection.
- Keyboard-first workflow is functional for core operations.
- Phase artifact prepared: `docs/phase13_ui_alpha.md`.

## Primary Risks

- UI latency due to heavy state redraw.
- Input mapping conflicts reducing editing speed.

## Phase 14: Export + MIDI Device Sync + Hardening

## Objective

Close MVP loop with export capability, real MIDI sync modes, and reliability pass.

## Scope

- In: offline render/export (`wav` first target).
- In: continuous MIDI device input/output integration.
- In: internal/external clock transport mode selection.
- In: reliability tasks (panic boundaries, error reporting, autosave strategy).
- Out: installer packaging across all platforms.

## Planned Deliverables

- Offline render path from project timeline to audio file.
- MIDI sync mode controls and runtime behavior tests.
- Hardening checklist with resolved P0 reliability issues.

## Exit Criteria

- User can author, play, and export a track in one flow.
- MIDI sync behavior is stable in internal and external clock modes.
- Phase artifact prepared: `docs/phase14_export_sync_hardening.md`.

## Primary Risks

- Sync jitter under external clock.
- Export path deviating from realtime playback result.

## Critical Path

`8 -> 9 -> 10 -> 11 -> 12 -> 13 -> 14`

Parallelizable support work:

- UI prototyping can start late in Phase 11 while storage v2 is in progress.
- Export prototype can start in Phase 13 once voice engine and storage v2 are stable.

## Ready-to-Start Backlog (Next 10 Working Days)

| ID | Task | Priority | Estimate | Phase |
|---|---|---|---:|---|
| NXT-001 | Define runtime loop ownership and threading rules | P0 | 6h | 8 |
| NXT-002 | Implement transport command queue contract spec (doc + tests plan) | P0 | 5h | 8 |
| NXT-003 | Add transport state diagnostics schema | P1 | 3h | 8 |
| NXT-004 | Select audio backend strategy (`cpal` first) and fallback rules | P0 | 4h | 9 |
| NXT-005 | Define xrun/latency metric schema and sampling intervals | P1 | 4h | 9 |
| NXT-006 | Specify note-off policy and voice lifecycle states | P0 | 5h | 10 |
| NXT-007 | Define MVP FX/Table command subset and validation ranges | P0 | 6h | 11 |
| NXT-008 | Draft storage v2 schema changes and migration table | P0 | 6h | 12 |
| NXT-009 | Define UI alpha keyboard map and navigation model | P1 | 5h | 13 |
| NXT-010 | Define export acceptance criteria (`render parity` checks) | P1 | 4h | 14 |

## Definition of Completion for Every Future Phase

- Scope is closed against the phase section in this document.
- Tests for new deterministic behavior are present and passing.
- Phase-specific artifact is created in `docs/`.
- `README.md` stage and artifact list are updated.
- Open risks for next phase are explicitly listed at phase close.
