# Port to OpenPluginSDK main 19fac89 (2026-11-26)

Owner: "port the openlimiter to the latest openpluginsdk". From 34420a4 to 19fac89 (#45-#55: docs, AU discrete
snap #50, VST3 bypass dedupe #51, ARA 2 #53, VST3 reset on the audio thread #54, GoodLookin DRI in the tree #55).

Changes in OpenLimiter (uncommitted):
- GoodLookin DRI from `deps/OpenPluginSDK/crates/goodlookindri` (the `[patch]` and the separate DRI checkout are gone,
  in Cargo.toml, build.sh and CI). Same DRI commit 3fe0426.
- SDK test set added (`new_plugin.py adopt`): tests/conformance.rs, host_behaviour.rs, soak.rs, examples/bench.rs.
  The two agent notes files it writes stay local (excluded in .git/info/exclude, not committed).
- Conformance found three faults, fixed:
  - `reset` allocated 15 times (rebuilt Taps and Monitor). Now `Taps::reset` and `Monitor::reset` clear in place.
    Matters since #54 runs VST3's reset on the audio thread.
  - Host group "Compressor / De-esser": '/' is a group separator now; the group is "Compressor & De-esser"
    (`host_group` in src/chain.rs). Ids and names unchanged.
  - Latency changes with parameters by design (module switches, look-ahead, linear phase); declared with
    `latency_depends_on_params` (the shells report the change: VST3 kLatencyChanged, CLAP latency.changed).

| Check | Result | Evidence |
|---|---|---|
| build, release, universal | PASS, 0 warnings | build.log |
| cargo test --release (lib 77, conformance full, host_behaviour 3, soak quick) | PASS | |
| SDK quick tier (realtime lint, conformance quick) | PASS | target/openplugin-dev/logs |
| auval | PASS | auval.log |
| clap-validator | PASS (34/0/10 skipped) | clap-validator.log |
| pluginval strictness 10, VST3 and AU | PASS | pluginval-*.log |
| OpenGrid9 AU state round trip | PASS (2 entries, both the harness's exponent fault) | og9/OpenLimiter_AU-load-rerun.log |
| OpenGrid9 LV2 | PASS | og9/run.log |
| OpenGrid9 VST3 state round trip | **FAIL (regression in the SDK): 148 values lost**, as on dea89d1 | og9/OpenLimiter_VST3-load-rerun.log |
| REAPER suite | not run yet | |

Cause (confirmed): #51 removed the controller state (OPCS) as redundant with `openplugin.edit`. But the processor
takes an edit only when idle, and `setProcessing(true)` stamps `lastBlockMs_`, so for 500 ms after activation every
edit is dropped while the host's parameter queue never reaches a block (OpenGrid9 headless runs none). Probe: an SDK
copy with that one stamp removed (`native/vst3/OpenPluginVST3.cpp`, setProcessing) loses 0 values in the same check
(scratchpad sdk-probe). The fix is an SDK change: owner's OK needed.

## SDK fix (owner's OK, 2026-11-26)

- **Change:** OpenPluginSDK PR #56 (branch `vst3-edits-after-start`, commit 1b966b1). `setProcessing` now clears the last-block time instead of stamping it, so edits land until the stream's first block.
- **Why not just delete the stamp:** the new proof case "restart right after a block" still failed with the stamp only deleted. The timer from the last block was still running, so the restart has to clear it.
- **SDK VST3 tests:**
  - With the old line, `vst3_proof` fails at the new check (host.cpp:720).
  - With the fix, all 14 VST3 tests pass on macOS (`-DOP_EDITOR=OFF`).
- **OpenGrid9 VST3 save and reopen,** OpenLimiter built on 1b966b1: no values lost (148 before). The only mismatch entry is the harness's known null.
  - OpenGrid9's VST3 cache had to be rescanned with `ardour-vst3-scanner -f`; without it the create phase fails.
- **Still open:** merge #56, pin OpenLimiter to the merge commit, then rerun OpenGrid9 (all formats) and the REAPER suite.

## Full test tier (2026-11-26)

- **First run:** 3 rows failed: T-CLIPPY (31 lints), T-SOAK and T-FUZZ.
- **Soak and fuzz latency:** `tests/soak.rs` lacked the `latency_depends_on_params` declaration that conformance.rs has.
- **Fuzz strict:** NaN and denormal input reached the output.
- **Soak seed 8:** a denormal output (1e-45). Narrowed by a throwaway search to the Shaper after the multiband compressor: an f64 result too small for f32, while the kit runs without flush-to-zero.
- **Fix:** `zero_abnormal` in `src/lib.rs` turns every sample that is not a normal float into 0, once on the input and once on the output. Normal samples pass bit for bit.
- **Clippy:** fixed by clippy's automatic fixes, iterator loops (same order of operations), three type aliases, and a TAU constant in a test signal.
  - **Exception:** `too_many_arguments` is allowed, with a reason, on 6 drawing primitives (`kit::freq_grid`, `kit::dashed_h`, two `paint_node`, `raster::vspan`, `raster::text`).
- **Rerun:** PASS for T-LINT, T-CLIPPY, T-TEST, T-SOAK and T-FUZZ. T-BENCH SKIPPED: there is no `bench/baseline.json` yet.

## Editor performance and readability (plugin-gui-performance, 2026-11-26)

- **Frame benchmark `frame_cost`, 2x, every page, History LEVELS / 2D / 3D** (`frame-cost.log`, load 17 to 38):
  - Render 3.5 to 24.7 ms per frame. Worst: Multiband (`bands`) and `peq` in 3D.
  - Last run (qa/2026-11-24-qa-editor-sdk, load 35 to 73): 3.6 to 22.2 ms.
  - Per-page change at most +22 %, within the noise of a loaded machine. PASS: every page inside the 33 ms tick.
- **Smart page tick 32 ms:** the known false positive. Test builds run the recovery measurement inline (`cfg!(test)`, smart.rs:341). Release builds run it on the bridge thread.
- **`column_fill_matches_the_polygon`:** PASS. **`text_is_whole_apart_and_readable` (text audit):** PASS.
- **Pixel check of the clippy UI edits:** `dump_editor` was rendered with and without the 14 changed `src/ui` files (git stash). All 72 images are byte-identical. PASS.
- **SKIPPED: on-screen measurement in a host** (GPU, host CPU, WindowServer, `sample` breakdown). Visible host testing needs the owner's request. It would cover the display copy and the host's main thread.

## Bench baseline (owner: "there has to be a t-bench on this system", 2026-11-26)

- **Baseline:** `bench/baseline.json` written on this machine (`cargo run --release --example bench -- --out bench/baseline.json`), 2000 iterations per case, load average 26 to 29.
- **T-BENCH against it:** PASS twice, at load 22 to 35. The full tier now passes all 6 rows.
- **Caveat:** the baseline was taken under load. A regression smaller than the load noise can slip through, and a quiet-machine run will look faster, never falsely slower.

## SDK ce6454f (after #56 and #57), 2026-11-26

- **Pin and build:** OpenLimiter pinned to ce6454f (`scripts/build.sh`, CI). Build clean, 0 warnings.
- **Checks:** full tier 6/6 PASS. auval PASS. OpenGrid9 LV2 PASS.
- **Correction: the OpenGrid9 worker's verdict is too lenient.** It excuses a value that is wrong after reload when setting it again reads back right, and its JSON writer loses the list.
  - `og9-strict.py` (copied here) compares every value read after reload with what was set, parsing exponents correctly.
  - **AU:** 0 mismatches.
  - **VST3 on ce6454f: 135 mismatches.** Almost every parameter from index 0 to 324 kept the value of the worker's min/max sweep (1.0); everything from 325 on is right.
  - The processor's own saved chunk holds the same wrong values, so the edits never reached the processor; it isn't a host read error.
  - The earlier "0 values lost" for the #56 fix branch was wrong: it read the worker's lenient list.
- **Not the 500 ms rule:** a scratch build that takes every `openplugin.edit` (idle check removed) loses the same 135 values.
- **Released 1.0.2 Beta VST3 (SDK 34420a4, controller state of #40): 0 strict mismatches.** So the regression came with #51, and #56 did not cover it.
- **Cause: not found yet.** Next step is to instrument the controller and processor (counts of setParamNormalized, sendEdit and notify) in a scratch copy of the SDK.
- **Status:** the port is not committed. The installed VST3 is the normal ce6454f build.
