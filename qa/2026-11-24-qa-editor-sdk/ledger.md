# OpenLimiter QA: editor, SDK and DSP changes after 1.0.1 (2026-11-24)

Pipeline: Engineering Wiki `processes.md`, "QA pipeline (canonical order)". Build under test: the commit
named in the gate table, release, universal, macOS 11.0, from `/Volumes/Developer/OpenGrid/OpenLimiter`,
installed to `~/Library/Audio/Plug-Ins` (VST3, AU, CLAP). No system copies in `/Library` (moved aside
2026-11-24; AAX there untouched and not tested).

## Scope (owner's requests this session, 2026-11-24)

| # | Item | Code | Evidence needed |
|---|---|---|---|
| S1 | Knob labels and rings never overlap (Classic DRIVE, Modes MORE knobs) | `pages/limiter.rs` Knob::circle | text audit, screenshots |
| S2 | GR trace and bar smooth (peak, 60 dB/s fall); history GR smooth | `mod.rs` tick_slots, `live.rs` paint_plot | screenshots, REAPER look |
| S3 | Radar: defined lines, short-term loudness | `live.rs` paint_radar, `shared.rs` | screenshots, REAPER look |
| S4 | Multiband GR in accent, slower ballistics | `pages/bands.rs` | screenshots |
| S5 | History and Radar pop-out windows: macOS, Windows, Linux; toggle; kept open after the editor closes; closed with the instance; positions remembered | `ui/pops.rs`, `popout.rs`, `popwin.rs`, `popx11.rs`, `detached.rs` | REAPER (macOS), CI (Windows, Linux) |
| S6 | History LEVELS / 2D / 3D spectrum, saturated 3D colours, slow smooth 3D | `ui/spectro.rs` | screenshots, frame benchmark |
| S7 | Rack: Alt-click removes a module, Cmd-click (Ctrl off macOS) resets it | `mod.rs` rack_down | unit test, host |
| S8 | One SDK: OpenPluginSDK main + shell fixes (dea89d1, PR #43) | `Cargo.toml`, `deps/` | build, validators, host |
| S9 | Editor optimisation per the wiki | `spectro.rs`, `live.rs` | frame benchmark |
| S10 | DSP optimisation per the wiki: Harmonic Compressor FIR (exact), dry chain skip (owner's OK) | `crates/comp-harmonic` | null renders, dsp_cost |
| S11 | Project moved to the OpenGrid folder | `deps/`, CI | build, CI |

## Gates

Build under test: `8f3b143` plus the dead-code removal below (uncommitted at test time), release,
universal, from `/Volumes/Developer/OpenGrid/OpenLimiter`, SDK `dea89d1` (deps/ link, checked by build.sh).

| Gate | Check | Result | Evidence |
|---|---|---|---|
| 1 Scope | ledger above | done | this file |
| 2 DSP | Harmonic Compressor changes: 80/80 null renders bit-identical at fixed settings; mix switch differs 1.7 to 3.3 ms, at most -29 dBFS (owner's OK) | PASS (exact) / accepted (behavioural) | `../2026-11-24-qa-1.0.1/ledger.md`, DSP sections |
| 2 DSP | per-module cost, default and heaviest modes | recorded | dsp-cost.log |
| 3 GUI coverage | every_parameter_can_be_reached, renders_every_page..., text_is_whole_apart_and_readable (ON RING, RING CLIPPED, TRUNCATED), every_page_shows_its_whole_description, numbers_fit_three_digits, every_module_fits_in_the_rack_at_once, the_rack_... (Alt-click remove, Cmd-click reset) | PASS | cargo-test.log |
| 3 GUI coverage | screenshots: all 18 pages x 4 themes at 2x (72 renders) looked at: light all pages; dark, graphite, paper sampled (limiter, modes, bands, clear, smart, sone) | PASS (rendered, not interacted) | scratchpad contact sheets |
| 3 GUI performance | frame_cost every page, 2x, LEVELS / 2D / 3D: renders 3.6 to 22.2 ms (Multiband worst) | PASS | frame-cost.log (load 35 to 73) |
| 3 GUI performance | Smart page tick 32 ms in the benchmark | false positive: test builds run the recovery measurement inline (`cfg!(test)`); release builds run it on the `olm-bridge` thread | `src/ui/pages/smart.rs` tick |
| 4 Local suites | main crate 77 passed; 19 module crates 440 passed, 0 failed | PASS | cargo-test.log, crate-tests.log |
| 4 Build | release build warnings: 4 dead-code items (pre-existing) | FIXED: removed unused `value_text`, `P_RELEASE_A/M`, `Ui::unit`; build now has no warnings, 77 tests pass | build.log (before) |
| 5 Validators | auval (no system copies) | PASS (one warning: a recommended property not supported) | auval.log |
| 5 Validators | clap-validator: 34 passed, 0 failed, 10 skipped (optional extensions) | PASS | clap-validator.log |
| 5 Validators | pluginval strictness 10, VST3, without and with editor tests | PASS | pluginval-vst3*.log |
| 5 Validators | AAX Validator | SKIPPED: no signed AAX build (release on hold) | |
| 6 Host | Windows x64 (VST3, CLAP) and Linux x86_64 (CLAP, LV2) CI builds with tests | PASS | GitHub Actions run 45397365860 |
| 6 Host | pop-outs on screen in REAPER (macOS): open, live, resize, toggle, kept after the editor closes, position restored, closed with the instance | PASS (earlier today, own REAPER) | `../2026-11-24-qa-1.0.1/ledger.md` |
| 6 Host | pop-outs on Windows and Linux in a host | SKIPPED: no Windows or Linux machine here; compiled and tested in CI only | |
| 7 Interactive | listening, Pro Tools, Studio One, Alt/Cmd-click in a host | owner | |
| Ballistics | GR displays (trace, bar, history, multiband): display smoothing only, no standard meter; audio meters and loudness unchanged since 1.0.1 | n/a, recorded | |
| 6 Host | REAPER suite, VST3, 24 parameters: loads (1985), finite, not silent, deterministic, editor opens, parameters change sound and GUI, state round trip, 5 editor cycles | PASS (9/9) | reaper/report.md, suite-run.log |
| 6 Host | module sweep, 4 workers (twice) | void: after a chunk or two every REAPER render came back empty (0 s, no file), no REAPER crash report; with 1 worker the same modules render and pass (classic 14 ok + 5 conditional + 2 non-audio, shaper 9 ok). Machine load 18 to 90 from other sessions | sweep-void-run/, sweep-void-run-2/ |
| 6 Host | module sweep, 1 worker, all modules, VST3, 29.9 min: every module ok; CONDITIONAL only where a parameter acts under a condition (eq 15, dual 5, bands 4, peq 214, deq 523); non-audio: delayUnit (display unit) and 2 input rows; no DEAD, no SILENT | PASS | sweep/module-sweep.md |
| 6 Host | OpenGrid9 headless, LV2: loads, 1982 control ports | PASS | og9/run.log |
| 6 Host | OpenGrid9 headless, VST3 and AU, first run | not a plug-in result: OpenGrid9's index had OpenLimiter VST3/AU as "new, not scanned" (scan_log result 1), the headless backend cannot insert it. Indexed with `ardour-vst3-scanner -f` / `ardour-au-scanner -f aufx OLim Ogrd` (cache backed up first) | og9/run.log |
| 6 Host | OpenGrid9 headless, VST3: loads, 1982 parameters, 0/1 sweep ok; state after save and reopen: **148 of 1982 values come back at their defaults** (DynEQ 95, Multiband 13, positions 15, others) and do not take a new value either | **FAIL (regression)** | og9/run2.log, og9/OpenLimiter_VST3_load.json |
| 6 Host | A/B: the same OpenGrid9 check on the 1.0.0 build (old SDK branch, from `~/OpenLimiter-1.0.0-system-backup`, swapped in for the run and restored): 0 mismatches | confirms the regression came with the SDK merge (VST3 shell state handling: `main`'s 3a8c9ca kept instead of the branch's 0f50066, controller values saved in its own state) | scratchpad og9/ab |
| 6 Host | OpenGrid9 parameter count 1982 (VST3) / 1981 (AU) vs 1985 expected | expectation was REAPER's total (it also lists 3 parameters OpenGrid9 does not count); LV2 shows the same 1982 inputs. Not a defect | |
| 6 Host | OpenGrid9 AU state round trip | not judged: the count check stopped the AU run before the load phase | og9/run2.log |

## Summary

- One FAIL: VST3 state after save and reopen in OpenGrid9 loses 148 of 1982 values (1.0.0: none). It came
  with the SDK merge; fixing it is an SDK change (owner's OK needed). REAPER's state round trip (24 shell
  parameters) passes, so it is not seen there; untested in other hosts.
- Everything else PASS or recorded: tests, GUI coverage and text audit, every page in four themes,
  frame benchmark, validators, REAPER suite and sweep, CI Windows and Linux, OpenGrid9 LV2.
- Fixed in this run: four dead-code warnings (unused `value_text`, `P_RELEASE_A/M`, `Ui::unit`).
- Not done: AAX (release on hold), pop-outs in a host on Windows and Linux, listening, Pro Tools, Studio One.

## Fix: VST3 state round trip (owner: "fix the qa check")

Cause: OpenLimiter was pinned to SDK `dea89d1`. Its VST3 shell sent controller edits to the processor only
while no block had come for 500 ms, so the edits made just after activation never reached the processor's saved
state. Those were the values the test pushed past a range limit (their only change came first). The
controller-state fix (`0f50066`, dropped in the SDK merge) has since landed on SDK `main` as PR #40 (`291a11e`,
merged `789d2f8`). No SDK change was needed: OpenLimiter's pin moves to SDK `main` `34420a4` (also brings
#44 VST3 block cutting, #21 Linux VST3 shell, #48 docs). Shared SDK checkout switched to `main` at `34420a4`.

Build: `8f3b143` + dead-code removal + pin bump (uncommitted), release, universal, no warnings. Evidence in `sdk-34420a4/`.

| Gate | Check | Result | Evidence |
|---|---|---|---|
| 4 | main crate tests | PASS (77) | |
| 4 | release build | PASS, 0 warnings | sdk-34420a4/build.log |
| 5 | auval | PASS | sdk-34420a4/auval.log |
| 5 | clap-validator | PASS (34/0, 10 skipped) | sdk-34420a4/clap-validator.log |
| 5 | pluginval strictness 10, VST3, without and with editor tests | PASS | sdk-34420a4/pluginval-vst3*.log |
| 6 | OpenGrid9 VST3: loads, 1982 parameters, sweep ok, state after save and reopen | PASS: 0 values lost (was 148). The run reports one entry, "param 2 missing in pattern": harness fault, see below; param 2 reads 1.9073486328125e-06 before and after | sdk-34420a4/og9/, vst3-load-rerun.log |
| 6 | OpenGrid9 AU: loads, 1981 parameters, state round trip | PASS (first time judged; expected count set to 1982, the host's own total, as recorded above) | sdk-34420a4/og9/run.log |
| 6 | OpenGrid9 LV2 | PASS | sdk-34420a4/og9/run.log |
| 6 | REAPER suite, VST3, 24 parameters (9 checks incl. state round trip, 5 editor cycles) | PASS (9/9) | sdk-34420a4/suite-run.log, sdk-34420a4/reaper/report.md |

Harness faults found (shared `/Volumes/Developer/OpenGrid/qa/og9qa_worker.lua`, not changed here): the pattern
reader's number pattern does not accept exponent notation (`1.9e-06`), so such a parameter is reported "missing in
pattern"; the JSON writer encodes Lua lists from index 0, so it writes `null` first and drops the last entry (the
earlier "148" was really 148 plus this one). Same in the 1.0.0 A/B run.
