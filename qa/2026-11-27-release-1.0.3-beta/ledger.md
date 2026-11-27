# OpenLimiter 1.0.3-Beta release

**Owner's decisions (2026-11-27):**
- Version 1.0.3-Beta, no standalone.
- Merge to main and push; Windows and Linux built by GitHub Actions.
- "merge 61 and fix the parameter names".

**Source:** `27d0ecc` for the macOS binaries (clean tree), OpenPluginSDK `d93d926` (main, with #60 and #61). The later release commit changes documents, the parameter reference's introduction (test code) and this record only.

| Platform | Where | Output |
|---|---|---|
| macOS | this Mac (arm64), `scripts/build.sh`, release, universal arm64 + x86_64, minos 11.0 | VST3, AU, CLAP, LV2, AAX |
| Windows x64 | GitHub Actions, windows-latest (run in the table below) | VST3, CLAP; Inno Setup installer, not code-signed |
| Linux x86_64 | GitHub Actions, ubuntu-22.04 | CLAP, LV2 (no editor), tar.gz |

## Checks on the release build

| Check | Result | Evidence |
|---|---|---|
| Full tier on `27d0ecc` (lint, clippy, release tests with full conformance, soak 24 seeds, strict fuzz, bench) | PASS 6/6 | commit tester |
| Build: 0 warnings; every binary universal, minos 11.0 | PASS | build.log |
| Shell size and exports (SDK #61): VST3 7.75 MB / 3 exports, AU 7.67 MB / 1, AAX 7.76 MB / 7; no bundled font data | PASS | exports-check/ |
| Parameter names unique within 31 characters (test `names_differ_within_31_characters`) | PASS; ten pairs collided before | AAX Validator: the duplicate-name warning is gone |
| auval | PASS | auval.log |
| pluginval strictness 10 with editor tests, VST3 and AU | PASS | pluginval-*.log |
| clap-validator: 34 passed, 0 failed, 10 skipped (optional) | PASS | clap-validator.log |
| AAX: PACE Eden signature (wrap GUID F0BC4C20-E72E-11F1-AC32-00505692AD3E), signed and verified | PASS | aax-sign.log |
| AAX Validator 2024.6 on the signed AAX: 13 passed, 0 failed | PASS | aax-validator-summary.txt |
| AAX Validator `test.cycle_counts` | SKIPPED: needs HDX hardware. On this Mac it ends in E_LOST for this build and for the released 1.0.2 AAX alike | exports-check/ |
| REAPER suite, VST3, first 24 parameters: load (1985), finite, not silent, deterministic (-240 dB), editor opens, sound (14 ok, 10 conditional), GUI 24/24, state save and reopen, 5 editor cycles | PASS 9/9 | reaper-vst3/report.md |
| OpenGrid9: VST3 (1982), AU (1981), LV2 (1989 ports); every value compared after save and reopen | PASS; VST3 0 and AU 0 wrong | og9/, `../2026-11-26-sdk-19fac89/og9-strict.py` |
| Manual PDF rebuilt (17 pages) and looked at (page 1) | PASS | |
| Listening, Pro Tools, other hosts; Windows and Linux in a host; Windows code signing | not done | owner / no machines |

## Release soak (200 seeds) and the output bound

- **First run:** `OPENPLUGIN_SOAK_SEEDS=200` failed at seed 84: output peak 74.4 over the kit's default bound of 64 (state check, every parameter random; 2 of 204,615 bound assertions). The usual 24 seeds do not reach it.
- **Cause: gain by design, not instability.** A throwaway search shrank three such states to their parameters:
  - +56.7 dB of Compressor / De-esser make-up placed after Final Wall (peak 447).
  - Three gain controls adding about +60 dB (peak 102).
  - A 38-parameter state with limiter gain and transient boost (peak 158).
- **Why the bound cannot hold:** the user orders the chain and several modules have large gain controls, so no fixed ceiling of 64 fits.
- **Exception (NEW):** `tests/conformance.rs` and `tests/soak.rs` set `max_output` to 1e6 (+120 dBFS), with the reason in a comment. NaN and infinity stay a separate check. Test code only: the binaries are unchanged.
- **Rerun:** 200 seeds and the strict fuzz PASS (995 s).

## OpenGrid9 VST3: what failed first, and why

- **First run:** the VST3 check failed with 301 values wrong (machine load about 150), then 658 and 375 in two more runs.
- **Cause: a race in the host.** The test set each value, swept it to 0 and 1, and wrote the first value again, all in one burst. For the parameters its update had not reached yet, OpenGrid9 dropped the last write, so it reached neither the processor nor the controller.
- **Evidence:**
  - With a 0.5 s wait before the save, 4 of 4 runs lost exactly 135 values, with processor and controller identical. Time does not bring the values.
  - Without any pause, 5 of 8 runs lost 135 and 3 lost none.
  - With a 0.5 s pause after the sweep, 6 of 6 runs lost none.
  - The released 1.0.2 VST3 lost none in 3 runs without the pause; 1.0.3 lost 135 in 1 of 3 in the same rounds. The difference is timing, not state handling: the save and load code is the same mechanism.
- **Harness change:** `og9qa_worker.lua` (outside this repository) now pauses 0.5 s between the sweep and the second write, with the reason in a comment. The comparison itself is unchanged and strict.
- **SDK #60 is needed.** In the 6 passing runs the processor and the controller differed in 116 to 717 values at save time, and every value came back right. The controller's saved copy carried them.

## Not explained

- One REAPER suite run on the #60 test build (2026-11-27, load 131) rendered the same settings twice with a difference from 6.46 s to the end of a 9 s file. Three later runs, on that build and this one, were identical (-240 dB).
