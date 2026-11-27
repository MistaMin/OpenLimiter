# OpenLimiter QA: port to OpenPluginSDK main and the audit fixes (2026-11-27)

**Build under test:** `b40a04f` on branch `sdk-ce6454f`.
- SDK `6d33f2c` (main; `scripts/build.sh` now enforces the pin).
- Release, universal, macOS 11.0.
- Installed to `~/Library/Audio/Plug-Ins` (VST3, AU, CLAP, LV2). No system copies in `/Library`.

## Scope

| # | Item | Evidence |
|---|---|---|
| S1 | Port to SDK main: GoodLookin DRI from the SDK, SDK test set, conformance fixes (reset in place, group name, latency declared) | full tier, validators, hosts |
| S2 | NaN, infinity and denormals zeroed at input and output | T-FUZZ (strict), T-SOAK |
| S3 | Clippy clean | T-CLIPPY |
| S4 | Bench baseline on this machine | T-BENCH |
| A1 | VST3 values lost on save: SDK fix, OpenPluginSDK PR #60 (not merged; not in this build) | SDK proof, OpenGrid9 on 22a6073 (`../2026-11-26-sdk-19fac89/ledger.md`) |
| A2 | OpenGrid9 worker strict (outside the repo) | strict runs below |
| A3 | `build.sh` stops at another SDK revision | build run: exit 1 at 6d33f2c vs ce6454f |
| A4 | PrecisionEQ worker parks until woken | 50 idle instances: 377.6 to 6.6 ms CPU/s; eq-parametric tests |
| A5 | CHANGELOG Unreleased | read |
| A6 | `Chain::process` test-only | build, tests |

## Gates

| Gate | Check | Result | Evidence |
|---|---|---|---|
| 2 DSP | No sound change intended. Loop rewrites keep the order of operations; the input and output clean-up touches only non-normal samples; PrecisionEQ kernels can only arrive sooner | no null render made | review |
| 3 GUI coverage | every_parameter_can_be_reached, text audit, numbers fit, rack tests (in T-TEST) | PASS | commit tester, b40a04f |
| 3 GUI | 72 page renders byte-identical before and after the clippy UI edits; UI unchanged since | PASS | `../2026-11-26-sdk-19fac89/ledger.md` |
| 3 GUI perf | frame_cost, 2x, all pages, three History modes: 3.5 to 24.7 ms | PASS (load 17 to 38) | `../2026-11-26-sdk-19fac89/frame-cost.log` |
| 4 Local | full tier on b40a04f: T-LINT, T-CLIPPY, T-TEST, T-SOAK, T-FUZZ, T-BENCH | PASS 6/6 | commit tester |
| 4 Build | release build, 0 warnings | PASS | `../2026-11-26-sdk-19fac89/build-6d33f2c.log` |
| 5 Validators | auval | PASS | auval.log |
| 5 Validators | pluginval strictness 10, VST3 and AU (no GUI tests) | PASS | pluginval-*.log |
| 5 Validators | clap-validator: 34 passed, 0 failed, 10 skipped (optional) | PASS | clap-validator.log |
| 5 Validators | AAX Validator | SKIPPED: no signed AAX build | |
| 6 Host | REAPER suite, VST3, first 24 parameters: load (1985), finite, not silent, deterministic, editor opens, sound (14 ok, 10 conditional), GUI 24/24, state save and reopen, 5 editor cycles | PASS 9/9 | reaper-vst3/report.md |
| 6 Host | OpenGrid9, fixed worker: VST3 1982 params, AU 1981, LV2 1989 ports; sweep ok; state round trip | PASS | og9/og9qa-report.json |
| 6 Host | Strict comparison of every value after save and reopen | VST3 0, AU 0 mismatches | og9-strict.py in `../2026-11-26-sdk-19fac89/` |
| 6 Host | Windows and Linux builds | not run: CI runs on push to main; the branch is not pushed | |
| 7 Acceptance | listening, Pro Tools, other hosts | owner | |

## Notes

- **VST3 on this build** (SDK main, without #60) passes OpenGrid9 now, at load about 20. Its loss shows only under heavy load (60 to 85). The deterministic check is the SDK proof `check_save_before_delivery`, which fails on main. This build still has the bug until the pin moves to #60's merge.
- **Exceptions:** `too_many_arguments` is allowed on 6 drawing primitives (reason in code). No parameter is excused in the REAPER suite.

## The build that ships after SDK #60: OpenLimiter on SDK 22a6073 (test build), 2026-11-27

**Build:** `b40a04f` built against the #60 branch with `ALLOW_SDK_MISMATCH=1` (a test build only), 0 warnings. Afterwards `dist/` and the user plug-in folders were put back to the `b40a04f` build. Evidence in `sdk-fix-22a6073/`.

| Gate | Check | Result | Evidence |
|---|---|---|---|
| 5 | auval | PASS | sdk-fix-22a6073/auval.log |
| 5 | pluginval strictness 10, VST3 and AU | PASS | pluginval-*.log |
| 5 | clap-validator: 34 passed, 0 failed, 10 skipped | PASS | clap-validator.log |
| 6 | OpenGrid9 VST3 (1982), AU (1981), LV2 (1989 ports) | PASS | og9/og9qa-report.json |
| 6 | Strict comparison after save and reopen | VST3 0, AU 0 mismatches | og9/ |
| 6 | Compatibility: the fix build opens the VST3 session saved by the b40a04f build (no controller state) | PASS, 0 mismatches | worker load on a copy of `og9/sessions/OpenLimiter_VST3` |
| 6 | REAPER suite, VST3, 24 parameters, run 1 (load 131) | FAIL: "deterministic render": the repeat differed from 6.46 s to the end of the 9 s render, so the sound sweep was skipped | sdk-fix-22a6073/reaper-vst3/ |
| 6 | REAPER suite, run 2 (load 79 to 104) | PASS 9/9: deterministic (-240 dB), sound 16 ok and 8 conditional, GUI 24/24, state survives save and reopen | sdk-fix-22a6073/reaper-vst3-run2/ |

**The run 1 failure did not come back.** It started mid-render, at 6.46 s, which points away from the state change, but the cause is not proven. Earlier REAPER runs on this machine also misbehaved under heavy load (see `../2026-11-24-qa-editor-sdk/ledger.md`, "void"). Rerun the suite on the final pinned build.
