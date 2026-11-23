# OpenLimiter 0.3.0 scope ledger (QA run 2026-11-23)

Sources: the owner's requests in the 2026-11-22/23 sessions, `design/*.md`, `NOTES.md`, the QA intake answers of 2026-11-23.
Build under test: release, AU + VST3 universal (macOS 11.0), installed to the user plug-in folders. No git repository: "uncommitted".

| ID | Item | Source | Code | GUI | Test | Evidence | Status |
|---|---|---|---|---|---|---|---|
| S1 | 17 movable modules in the chain, append-only parameter order | requests 11-22 | `src/lib.rs` `modules()` | rack + add menu | `lib.rs` slot test (17 slots), `every_module_fits_in_the_rack_at_once` | cargo-test.log | verified |
| S2 | Module names as the owner listed them | rename list 11-22 | `src/lib.rs`, `src/ui/mod.rs` `module_names` | page header, rack chips | screenshots | screens/*.png | verified |
| S3 | PrecisionEQ: all OpenEQ bands and features | request 11-22 | `crates/eq-parametric` | page `peq` | crate tests (28), reachability | crate-tests.log, peq-dark.png | verified (unit), host sweep: see S20 |
| S4 | DynEQ module | request 11-22 | `crates/eq-dynamic` | page `deq` | crate tests (14), reachability | crate-tests.log | verified (unit) |
| S5 | Suppressor as its own module | request 11-22 | `crates/reso-suppress` | page `sup` | crate tests (11) | crate-tests.log | verified (unit) |
| S6 | External sidechain works per module, delay-aligned | request 11-22 | `src/chain.rs` key ring | KEY INT/EXT in headers | `the_watched_slot_publishes_its_audio_key_and_meters` | cargo-test.log | verified in harness; not tested in a host with a key bus (SKIPPED: suite has no sidechain route) |
| S7 | Harmonic Compressor module | request 11-22 | `crates/comp-harmonic` | page `hcomp` | crate tests (29) | crate-tests.log | verified (unit) |
| S8 | Dynamics Restore, Floor Lifter, Overtone Generator (ADAA) | request 11-22 | `crates/dyn-decompress`, `dyn-deexpand`, `harmonics` | pages | crate tests (27, 24, 32) | crate-tests.log | verified (unit) |
| S9 | Smart Limiter per spec and owner decisions | spec + decisions | `crates/limiter-smart` | page `smart` | crate tests (35), `design/smart-limiter-decisions-results.md` | signed off by owner 11-23 | verified |
| S10 | Two extra colour schemes (Graphite, Paper) | request 11-22 | `src/ui/theme.rs` | SETUP theme | `renders_every_page_in_both_themes_and_modes` | screens/*-graphite/paper.png | verified |
| S11 | No clipped text | request 11-22 | chrome, pages | all pages | zone tests, screenshot review, `every_page_shows_its_whole_description` (NEW) | screens, cargo-test | verified after fix F2 |
| S12 | RMS readout under true peak, above Requantise; plain RMS default, AES +3 option, window default 2 s, no window label | request 11-22/23 | `src/ui/mod.rs` `rms_now`, `live.rs` | right rail, SETUP | `the_rms_meter_reads_...` | cargo-test.log, limiter-dark.png | verified |
| S13 | Readouts refresh slowly enough to read (owner: "slow readout", 11-23) | QA intake | `tick_readout`, `READOUT_PERIOD` 0.25 s | loudness bar, RMS | `the_numbers_change_four_times_a_second_not_every_frame` (NEW) | cargo-test | verified after fix F1 |
| S14 | Plugin-wide Autogain: heard loudness matched to the input before the input gain (K-weighted, 3 s average, ±24 dB, held through silence); meters keep the real output | QA intake 11-23 ("plugin-wide both"), owner 11-23 "build" | `src/monitor.rs`, `src/lib.rs` param `autogain` (appended, preset-excluded, default off) | top bar AUTOGAIN pill + applied gain readout | `autogain_matches_the_heard_loudness_to_the_input`, `autogain_holds_through_silence`, `autogain_and_delta_are_off_and_leave_the_audio_alone`, `the_header_shows_the_gain_autogain_applies` | cargo-test, build/monitor-*.png, auval-3.log | verified (harness); host: see S20 |
| S15 | Plugin-wide Delta: output minus the input after the input gain, delayed by the plugin latency; at the matched level when Autogain is on | QA intake 11-23, owner 11-23 "build" | `src/monitor.rs`, param `delta` | top bar DELTA pill | `delta_is_the_output_minus_the_aligned_input` (exact to 1e-5 against a twin instance) | cargo-test | verified (harness); host: see S20 |
| S16 | Every parameter has a GUI control | QA rule 2 | pages | all | `every_parameter_can_be_reached` | cargo-test | verified (static) |
| S17 | Numbers show at most three digits, kHz / s above 1000 | QA display rule 1 | `kit::three_digits` | all number controls | `numbers_fit_three_digits` (NEW) | value-texts.tsv | verified after fix F3 |
| S18 | Whole steps for dynamics; no 0.x outside EQ | QA display rules 2-3 | - | 61 / 76 controls show tenths | value-texts.tsv | - | out of scope: owner 11-23 "ignore the whole db rule" |
| S19 | AU validates | QA | - | - | auval | auval.log | verified |
| S20 | Host: loads, renders, parameters act, state round trip | QA | - | - | REAPER suite (VST3) + module sweep | reaper/report.md, sweep/module-sweep.md, sweep/resolution.md, reaper-monitor/report.md | verified: plugin-level checks PASS; 1964 of the 1981 own parameters swept in the host one module at a time (the 17 chain positions need two modules on: unit test position_decides_the_order) (28.6 min, 4 REAPERs); 723 host suspects resolved by a second route (in-process recheck, targeted test, one non-audio); no dead parameter. Autogain/Delta also through the standard suite (audio, visual, state, editor: PASS) |
| S24 | Header descriptions never stop mid-phrase | screenshot review | `chrome::header_line`, `fit_words` | page headers | `every_page_shows_its_whole_description` | cargo-test | verified after fix F2 |
| S25 | Meter readouts never show -inf | QA meter rule 8 | `pages/deq.rs` | DynEQ detector readout | screenshot | deq-dark.png | verified after fix F4 |
| S21 | VST3 class ID is 32 hex | QA | SDK | - | REAPER scan cache | `BB41C9C0BFC7DFACA591166AB45A3364` | verified |
| S22 | Build class: modern, macOS 11.0 | QA | build script | - | `otool` LC_BUILD_VERSION minos 11.0, universal | - | verified (no manifest: not a release) |
| S23 | Install only with go-ahead; user folders only | standing rule | - | - | - | owner OK 11-23 | verified |

## Open questions
- Q1 (answered 11-23: ignore the rule for OpenLimiter): The display rules ask for whole-dB steps on thresholds, gains and ceilings and no 0.x values outside EQ. A mastering limiter's ceiling and true-peak values are normally set in tenths (-0.3 dBTP). Apply the rule (whole steps everywhere), or record mastering dynamics as an exception?
- Q2 (answered 11-23: build): Build plugin-wide Autogain and Delta now (S14, S15)? Built, F5.

## Fixes made
- F1 `src/ui/mod.rs` `tick_readout`, `READOUT_PERIOD`; `live.rs` loudness bar reads `ui.readout`: numbers change 4 times a second.
- F2 `src/ui/chrome.rs` `header_line`, `fit_words` cuts only at phrase ends; `src/ui/mod.rs` 12 shorter module descriptions.
- F3 `src/ui/kit.rs` `three_digits` in `Cell::text`: 34 controls fixed.
- F4 `src/ui/pages/deq.rs`: detector readout "--" instead of "-inf".
- F6 `qa/tools/module_sweep.py` (new): host sweep one module at a time in parallel hidden REAPERs, replacing the all-parameter random retry for this plugin. `src/lib.rs` `recheck_dead_parameters` (ignored) and `conditional_parameters_act_once_their_condition_holds`.
- F5 `src/monitor.rs` (new), `src/lib.rs` params `autogain`/`delta` appended after `rackShown` (own parameters 1979 -> 1981; REAPER lists 1985 with its own), `src/shared.rs` applied gain, `src/ui/mod.rs` + `chrome.rs` top-bar pills and readout. Both are listening aids after the output meters; the pill tooltips say to switch them off before printing.

## Not verified / skipped
- Host visual sweep per module parameter: not meaningful (the rack chip changes with any module); static reachability instead.
- AU in REAPER, Logic: not run (auval passed).
- pluginval, VST3 SDK validator: not installed (SKIPPED).
- Sidechain in a host (suite has no key-bus route): SKIPPED.
- Performance comparison old/new: SKIPPED (no saved old build to compare).
- Meter ballistics against a reference render (RMS window, LUFS gating at two buffer sizes): only harness tests; host measurement SKIPPED.
- Listening: owner.
- Visual sweep: every parameter changed about 6% of the editor, but that is the rack chip appearing when the module switches on, so "shown in GUI" rests on the static reachability test, not the sweep.
- Cosmetic: "-0.0" on a few readouts (Punch clip, Suppressor max, Multiband GR).
