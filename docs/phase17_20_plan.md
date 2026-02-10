# Post-Phase-16 Development Plan (Phase 17 -> Phase 20)

## Objective

Define the next iteration after Phase 16 with clear focus:

- move from terminal-only workflow to usable non-terminal GUI
- keep deterministic editing/runtime behavior
- improve sound quality and runtime reliability
- prepare a practical beta release loop

This is a planning artifact only. No implementation changes are included.

## Planning Baseline

- Current state: `Phase 16.4 workflow hardening and regression pack completed`.
- Stack remains Rust workspace (`p9_core`, `p9_rt`, `p9_storage`, `p9_app`).
- Existing shell workflow and deterministic regression tests stay as baseline safety net.

## Phase Map

| Phase | Name | Effort (solo) | Primary Outcome | Depends On |
|---|---|---:|---|---|
| 17 | GUI Foundation v1 | 2.0 weeks | usable desktop UI shell replacing terminal-only operation | 16 |
| 18 | GUI Editing Workflow v1 | 2.5 weeks | production-like editing flow in GUI with parity for core commands | 17 |
| 19 | Sound and Performance v2 | 2.5 weeks | better instrument/FX behavior and stable realtime performance | 17, 18 |
| 20 | Beta Hardening and Release Prep | 2.0 weeks | beta-ready package with reliability checklist and release docs | 18, 19 |

Estimated total: `9.0 weeks` for one developer.

## Phase 17: GUI Foundation v1

## Objective

Start normal non-terminal UI and bind it to current runtime/engine core.

## Scope

- In: native window app shell, screen layout, realtime status panels, transport controls.
- In: bridge from GUI actions to existing command pipeline.
- Out: advanced editing tools and heavy visual polish.

## Subphases

### 17.1 GUI Stack and App Shell

- choose primary GUI stack for current Rust app (pragmatic target: fast iteration path)
- create non-terminal app shell with startup/loop lifecycle
- keep terminal shell as fallback mode

### 17.2 Core Screens and Data Binding

- render Song/Chain/Phrase/Mixer views in window UI
- live snapshot binding from engine/runtime into UI widgets
- preserve deterministic refresh path and clear separation of concerns

### 17.3 Transport and Input Layer

- add GUI transport controls (`play/stop/rewind`)
- keyboard routing for navigation parity with shell
- status panel for tick/transport/recovery/dirty indicators

### 17.4 Project Session Workflow v1

- `new/open/save/save-as/recent` basic actions in GUI
- dirty-state prompt before destructive navigation/close
- first-pass session UX without terminal dependency

## Exit Criteria

- user can run non-terminal UI and navigate all core screens
- transport controls work from GUI and are test-covered
- project open/save flow works in GUI path
- phase artifact prepared: `docs/phase17_gui_foundation.md`

## Primary Risks

- GUI framework integration complexity with existing runtime loop
- UI thread stalls affecting realtime feedback perception

## Phase 18: GUI Editing Workflow v1

## Objective

Reach practical editing productivity inside GUI for daily authoring.

## Scope

- In: step editing, selection/block operations, undo/redo, command/status feedback.
- In: workflow tools for repetitive tracker editing.
- Out: full custom theming and advanced plugin ecosystem.

## Subphases

### 18.1 Step Editor Parity

- phrase step grid editing parity for `c/f/i/e` style flow
- explicit cursor/focus model and visible edit target
- safety validations with user-facing status tags

### 18.2 Selection, Block Ops, Undo/Redo UX

- GUI-native `select/copy/paste/force-paste` interaction
- overwrite confirmations in GUI (not only command semantics)
- undo/redo timeline visibility and fast history actions

### 18.3 Power Tools v1

- duplicate/fill/clear/transpose/rotate for selected regions
- song/chain row quick actions for repetitive structuring
- constraints to keep deterministic results

### 18.4 Workflow Polish

- low-friction shortcuts and focus transitions
- clearer warnings/errors for invalid operation contexts
- lightweight interaction regression tests for GUI flows

## Exit Criteria

- user can author a short track fully in GUI without terminal
- block operations and undo/redo are stable and predictable
- repetitive editing tools reduce command count for common tasks
- phase artifact prepared: `docs/phase18_gui_editing_workflow.md`

## Primary Risks

- feature creep in editor actions
- inconsistent UX between keyboard and pointer paths

## Phase 19: Sound and Performance v2

## Objective

Improve audio quality and sustain realtime stability under heavier sessions.

## Scope

- In: voice behavior improvements, instrument depth increment, FX path hardening, performance profiling.
- In: deterministic stress checks for CPU and transport behavior.
- Out: final mastering-grade DSP chain.

## Subphases

### 19.1 Voice Lifecycle and Click-Safety

- tighten note-on/off transitions and release behavior
- improve voice stealing policy under polyphony pressure
- reduce audible clicks at transition boundaries

### 19.2 Instrument Depth v2

- add one major instrument-depth increment (sampler-v1 or hybrid synth layer)
- align instrument params with storage/runtime contracts
- deterministic tests for voice behavior per instrument mode

### 19.3 FX Routing and Mixer Behavior

- refine send/return behavior and routing safety
- improve consistency between realtime playback and export path
- validate FX command interactions in complex phrases

### 19.4 Performance and Stability Pass

- profile callback/load hotspots and optimize critical path
- add stress scenarios for long playback/edit loops
- define acceptable realtime metrics thresholds

## Exit Criteria

- audible quality and stability are clearly improved versus Phase 18 baseline
- realtime metrics remain within defined thresholds on reference setup
- stress scenarios pass without regressions
- phase artifact prepared: `docs/phase19_sound_performance_v2.md`

## Primary Risks

- regressions in deterministic behavior while optimizing DSP/runtime paths
- host-specific audio backend edge cases under high load

## Phase 20: Beta Hardening and Release Prep

## Objective

Prepare a beta-ready build with clear reliability boundaries and usage guidance.

## Scope

- In: persistence/recovery hardening, long-run regression pack, release checklist, beta docs.
- In: packaging path for target OS and reproducible run instructions.
- Out: broad multi-platform installer matrix.

## Subphases

### 20.1 Persistence and Recovery Hardening

- validate open/save/recover edge cases on malformed/interrupted sessions
- ensure dirty/autosave/recovery behavior is explicit and deterministic
- add recovery diagnostics suitable for support/debug

### 20.2 Long-Run Regression Suite

- scripted scenarios with long playback/edit runs (30-60 min equivalent)
- detect memory growth, transport drift, and command pipeline regressions
- enforce regression gate before release candidate tagging

### 20.3 Beta Packaging and Onboarding

- produce reproducible beta build process
- add quickstart/demo project and troubleshooting notes
- tighten runtime dependency checks and startup diagnostics

### 20.4 Release Candidate Cycle

- freeze scope for beta RC
- execute RC checklist and triage remaining P0/P1 issues
- publish beta notes and known limitations

## Exit Criteria

- beta artifact can be built and launched from clean environment
- long-run regressions and recovery checks pass
- onboarding and release notes are complete
- phase artifact prepared: `docs/phase20_beta_release_prep.md`

## Primary Risks

- hidden stability bugs appearing only in long sessions
- packaging/runtime environment mismatch on user systems

## Critical Path

`17 -> 18 -> 19 -> 20`

Parallelizable support work:

- selected sound-engine investigations can begin late in Phase 18
- beta docs/onboarding drafts can start during late Phase 19

## Definition of Completion for Every Future Phase

- Scope closure against this plan section.
- Deterministic tests for all new behavior paths.
- Phase artifact created in `docs/`.
- `README.md` updated with phase status and artifacts.
- Open risks for next phase explicitly documented.
