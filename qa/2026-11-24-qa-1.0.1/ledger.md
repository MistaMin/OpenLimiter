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
| 6 Host matrix | `qa/tools/module_sweep.py`, 4 hidden REAPERs, VST3 1.0.1, 7.4 min: every module ok; CONDITIONAL only where a parameter acts under a condition (eq 38, peq 215, deq 481); no DEAD, no SILENT | PASS | sweep/module-sweep.md |
| 6 Host matrix | first sweep run: one REAPER stopped mid-run, later chunks rendered silent; discarded (single-module rerun clean) | void | sweep-failed-run/ |
| 6 Host matrix | REAPER suite, VST3 1.0.1, 24 parameters: loads, finite, not silent, deterministic, editor opens, parameters change sound and GUI, state round trip, 5 editor cycles | PASS | reaper/report.md |
| 6 Host matrix | OpenGrid9 headless | pending the move | |
| 7 Interactive | listening, Pro Tools, Studio One | owner | |

## Fix after the owner's report (2026-11-24): labels on knob rings

Owner: "you have clipping on some words and knobs again ... for example Drive in the classic limiter".
- Cause: the page-drawn `Knob` (`src/ui/pages/limiter.rs`, used by Classic DRIVE and the Modes MORE gain
  knobs) had its own geometry (big: centre y+54, r 36; small: y+36, r 20) instead of the cell knobs'
  (y+59, r 33; y+37, r 17), so its ring ran into the label. Now the same geometry.
- The text test missed it: it checked text against clips and other text only. `text_is_whole_apart_and_readable`
  now also checks text on a ring (ON RING), rings cut by their clip (RING CLIPPED) and labels cut to "…"
  (TRUNCATED; "measuring…" is wording), on every page, both heights, MORE closed and open, every page state.
  Before the fix it reported DRIVE and the six Modes MORE knobs (INPUT, INPUT L/R, TRIM, TRIM L/R); after: none.
- Every module page looked at again at 2x (light theme): no other cut text or knob.
- Rebuilt and installed to the user folders; auval PASS (1.0.1). Not yet host-swept again (UI-only change).

## Fix after the owner's report (2026-11-24): jagged gain reduction

Owner: "the gain reduction is a bit jagged, it might be difficult to understand the real gain reduction".
- Module pages (GR trace and bar, `src/ui/mod.rs` `tick_slots`): each frame swaps the slot's GR to 0, so a
  frame with no audio block since the last one read 0 and notched the trace. Now peak, then 60 dB/s fall.
- History plot (`src/ui/live.rs` `paint_plot`): GR was the raw per-column peak while the levels fell at
  60 dB/s; GR now uses the same fall. Peaks stay exact; only the recovery shown is limited to 60 dB/s.
- 75 lib tests PASS; rebuilt, installed to the user folders; auval PASS. Not looked at on screen yet.

## Owner's requests (2026-11-24): radar lines, pop-out windows, multiband GR

- Radar looked hand-drawn: it plotted momentary loudness (400 ms) 20 times a second, a zig-zag on
  drums, and stroked it along the radius, so steep steps broke into dashes. Now short-term loudness
  (3 s, the ring in `shared.rs`; momentary readout moved to `loud_now`) and strokes measured across
  the slope (`live.rs` `paint_radar`). Seen in REAPER on a drum loop: solid lines.
- Pop-out windows (macOS): buttons in the History and Radar headers open each in its own resizable
  floating window (`ui/detached.rs`, `ui/popout.rs`). Seen in REAPER: open, live, resize, close, reopen;
  closing the plug-in window closes them, REAPER kept running. Windows/Linux: buttons hidden (not built).
  Radar render 66 ms at 1600x1000 @2x, so a big radar redraws at most every 3x its render time.
- Multiband: the part of a band over its threshold was the pale input grey; now accent. The band
  read-out (levels, reduction, hold) rises in about 50 ms and falls at 20 dB/s instead of jumping per
  frame. Checked in a 2x render; not yet seen live in REAPER.
- 76 lib tests PASS (new: `detached_views_paint_at_any_size`); auval PASS.

## Owner's requests (2026-11-24): pop-outs everywhere, kept, remembered; spectrum in History

- Pop-outs belong to the instance (`ui/pops.rs`), not the editor: they stay open when the plug-in
  window closes (seen in REAPER) and close when the instance is destroyed (`impl Drop for OpenLimiter`;
  REAPER quit with a radar open: exited in 1 s, no crash report). The header button toggles the window
  and is lit while it is open (seen in REAPER).
- Positions: saved per window in the preferences on close (`pops`), restored on open; seen in REAPER
  (moved, closed, reopened at the same place). `pref_file` now uses %APPDATA% on Windows and
  $XDG_CONFIG_HOME or ~/.config on Linux (it was a macOS path everywhere).
- Windows (`ui/popwin.rs`, the SDK's TopWindow, owned by the host's main window) and Linux
  (`ui/popx11.rs`, an X11 top level on its own thread holding the SDK view): type-checked with
  `cargo check --target x86_64-pc-windows-gnu` / `x86_64-unknown-linux-gnu` only. NOT run on either.
- History: LEVELS | 2D | 3D. The chain's output (after the requantiser, before the listening monitor)
  goes to a ring every reader can read (`Shared::push_out`); 2D is a spectrogram over the 4 s window,
  3D a waterfall of 0.2 s averaged, third-octave-smoothed slices that slide back smoothly (the first
  version, one raw slice per tick, was too fast and jagged: owner). The threshold grip and tag are off
  in the spectrum modes. Seen live in REAPER (2D and the first 3D); the smoothed 3D in a 2x render.
- 77 lib tests PASS; auval PASS.

- Rack chips: Alt-click removes the module (as "Remove from chain"); Cmd-click (Ctrl-click on
  Windows/Linux) resets its settings, keeping it on and in place. Tooltip says so. Test PASS.
- 3D colours: own scale from the theme's cool colour up (no fade into the background), fills 80% opaque,
  back slices fade to 65% (owner: needs stronger saturation). Seen in a 2x render.

## Optimisation pass against the wiki (GUI Performance & Readability, plugin-gui-performance), 2026-11-24

Frame benchmark, 2x, dark theme, M1 Max, load average 5.6 (`frame_cost`, now also `HIST=1|2` for the
History spectrum and the tick time, which carries the FFT):

| Page | LEVELS ms/frame | 2D | 3D | tick ms |
|---|---|---|---|---|
| Classic limiter | 16.4 | 16.1 | 16.6 | 0.9 to 1.1 |
| SimplEQ | 14.1 | 14.5 | 14.1 | 0.3 |
| Multiband | 22.0 | 21.8 | 21.7 | 0.0 |
| PrecisionEQ | 16.7 | 16.7 | 16.5 | 0.3 |
All 18 pages LEVELS: 3.6 to 22.0 ms, as at 1.0.1. The main editor is inside the 33 ms tick in every mode.

Pop-out views (`detached_views_paint_at_any_size`, 2x), before -> after:
| View | 420x300 | 1600x1000 |
|---|---|---|
| History levels | 3.7 | 11.1 -> 9.5 |
| History 2D | 0.9 | 13.4 -> 13.6 |
| History 3D | 3.1 -> 1.0 | 47.0 / 36.2 -> 13.4 (front-to-back with a per-column cover line: no overdraw, same picture) |
| Radar | 4.1 | 67.1 -> 65.9 (traces skipped beyond the furthest trace; the test signal reaches the rim) |
Over the tick: only a radar window far larger than the editor. It redraws at most every 3x its own render
time (a quarter of the main thread). Not done: redrawing only the swept wedge.
SKIPPED: on-screen measurement in a host (another session's REAPER on screen); 1x displays.

## DSP optimisation pass (plugin-dsp-engineering phase 7, optimisation gate), 2026-11-24

Profile (`dsp_cost`, new ignored test: each module alone in the whole plug-in, 48 kHz stereo, 512-sample
blocks, 20 s programme-like audio after 1 s warm-up, M1 Max, release):
- Shell only (taps, loudness, requantiser): 0.8 % of one core. Every module at default: 0.8 to 2.6 %
  including the shell; all 17 together about 15 %.
- `HEAVY=1` (every oversampling / upsampling / true-peak switch at maximum): all modules 0.8 to 2.6 %
  except Harmonic Compressor at 8x linear-phase oversampling, 10.0 %. `sample` of that: the oversampling
  filters (wet path and the dry/reference alignment chains) and the oversampled harmonics stage.

Kept (exact class): Harmonic Compressor FIR delay lines store each sample twice so the filters read one
contiguous slice in the same tap order (no per-tap mask or bounds check). Null test
(`crates/comp-harmonic/tests/null_render.rs`: os 1x/2x/4x/8x x min/linear filter x wet / mix 60 % with
harmonics and Auto Gain x blocks 1, 17, 64, 129, 512; silence, quiet and full-scale sines, bursts, sweep;
L and R differ): 80 of 80 renders bit-identical. Matched benchmark (old, new, new, old; load 6.4):
old 9.73 / 9.82 %, new 8.95 / 9.29 % of one core (about 6 % less).

Proposed, not done (behavioural, needs the owner): skip the dry alignment chain at 100 % mix, about a
fifth of Harmonic Compressor's cost at 8x; the chain would restart from stale state when the mix is lowered.
Not tried: stereo lockstep of the oversampling filters (exact, more code; the module is already cheap at
its default 2x).
