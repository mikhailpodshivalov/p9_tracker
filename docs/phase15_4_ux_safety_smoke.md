# Phase 15.4: UX Safety and Smoke Validation

## Objective

Harden shell-based workflow with practical safety guards and scenario-level smoke checks before moving to richer editing flows.

## Delivered

- UX safety checks added in shell edit commands:
- `f` now requires chain binding on selected song row (`run c first` warning if missing).
- `e` now requires:
- bound chain on selected song row (`run c first` warning)
- bound phrase on selected chain row (`run f first` warning)
- existing instrument for focused track (`run i first` warning)
- Mixer safety behavior:
- `+` and `-` commands now return explicit warnings on min/max bounds instead of silent no-op.
- Help surface added:
- `?` command returns command reference summary.
- Shell labels and hints updated to `Phase 15.4` and include help command.
- Main stage marker updated to `stage15.4 ux-safety-smoke`.

## Test Coverage

- `ui_shell::tests::shell_safety_warns_when_phrase_bind_has_no_chain`
- `ui_shell::tests::shell_safety_warns_when_edit_has_no_instrument`
- `ui_shell::tests::shell_help_command_returns_reference`
- `ui_shell::tests::shell_smoke_flow_edit_and_play_emits_events`
- Existing navigation/transport/edit tests remain green.

## Behavior Summary

- Shell now prevents key invalid edit sequences with explicit guidance instead of implicit auto-healing.
- User receives actionable command hints directly in status flow (`run c/f/i first`).
- End-to-end smoke scenario confirms that edit loop plus transport still produces realtime events.

## Current Limits

- Input is still line-command based (not raw key event mode).
- Warning texts are status-line only; no dedicated modal or error panel yet.
- Smoke test validates core behavior but not long-duration session stability.

## Exit Criteria (This Iteration)

- Invalid edit order is guarded with deterministic user-facing warnings.
- Help command is available in shell loop.
- Smoke scenario verifies authoring + transport event emission path.
- Phase artifact prepared in `docs/phase15_4_ux_safety_smoke.md`.
