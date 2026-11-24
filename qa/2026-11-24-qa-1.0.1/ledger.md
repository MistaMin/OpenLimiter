# OpenLimiter 1.0.1 Beta QA (2026-11-24)

Pipeline: Engineering Wiki `processes.md`, "QA pipeline (canonical order)". Build under test: commit 717a5e7
(1.0.1-beta), release, universal, macOS 11.0, installed to `~/Library/Audio/Plug-Ins` (VST3, AU, CLAP).
Scope of the change: `qa/2026-11-24-editor-performance/ledger.md` (editor speed, whole-number readouts,
Semibold text, history/radar, text sizes). Earlier scope rows: `qa/2026-11-23-full-qa/ledger.md`.

**Blocker found:** the 1.0.0 .pkg installed root-owned copies in `/Library/Audio/Plug-Ins` (VST3, Components,
CLAP, LV2, AAX). Hosts and the AU registry load those (1.0.0), not the 1.0.1 build. Checks that load by
path test 1.0.1; checks that load by name (REAPER, auval) test 1.0.0 until the system copies are moved.
Owner moved them (VST3, AU, CLAP, LV2) to `~/OpenLimiter-1.0.0-system-backup` on 2026-11-24; the AAX in
`/Library/Application Support/Avid` is untouched.

| Gate | Check | Result | Evidence |
|---|---|---|---|
| 1 Scope ledger | change scope in the editor-performance ledger | done | `../2026-11-24-editor-performance/ledger.md` |
| 2 DSP audit | no DSP change in 1.0.1 (UI only) | n/a | `git show --stat 717a5e7` |
| 3 GUI coverage | `every_parameter_can_be_reached`, `renders_every_page_in_both_themes_and_modes`, `text_is_whole_apart_and_readable`, `every_page_shows_its_whole_description`, `numbers_fit_three_digits`, `every_module_fits_in_the_rack_at_once` | PASS | cargo-test.log |
| 3 GUI coverage | screenshot review | PASS (every page, 1x, second pass of the perf work; on-screen limiter, SimplEQ, PrecisionEQ in REAPER) | `../2026-11-24-editor-performance/ledger.md` |
| 4 Local suites | main crate, release: 75 passed, 19 ignored (benchmarks / host rechecks) | PASS | cargo-test.log |
| 4 Local suites | 19 module crates, release: 441 passed, 0 failed | PASS | crate-tests.log |
| 5 Validators | pluginval 1.0.4, strictness 10, VST3 by path (1.0.1), without and with editor tests | PASS (SUCCESS) | pluginval-vst3.log, pluginval-vst3-gui.log |
| 5 Validators | clap-validator 0.4.1 (dev build 127-g152b982), CLAP by path (1.0.1): 34 passed, 0 failed, 10 skipped (optional extensions not implemented: preset discovery, 64-bit audio, note-port and audio-port options) | PASS | clap-validator.log |
| 5 Validators | auval, after the system copies were moved (component version 1.0.1 confirmed) | PASS (one warning: a recommended property not supported) | auval.log |
| 5 Validators | AAX Validator | SKIPPED: 1.0.1 has no AAX build (not signed: release on hold) | |
| 6 Host matrix | REAPER suite + `qa/tools/module_sweep.py` | pending the move (would load 1.0.0) | |
| 6 Host matrix | OpenGrid9 headless | pending the move | |
| 7 Interactive | listening, Pro Tools, Studio One | owner | |
