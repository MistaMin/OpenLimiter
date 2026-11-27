//! TS-2 and TS-3. Ignored in a plain `cargo test`; the commit tester runs them with `--ignored`.
//! OPENPLUGIN_SOAK_SEEDS sets the number of seeds (24 per commit, 200 for a release).

use openlimiter::OpenLimiter;
use openplugin::testing::{Options, conformance};

openplugin::testing::install_allocator!();

// The chain has no fixed output ceiling: the user orders the modules, and several have large
// gain controls (Compressor / De-esser make-up to +60 dB, input gain to +24 dB), so a random
// state can put +57 dB of make-up after the last limiter (peak 447 from full-scale noise; soak
// seed 84 reached 74). The kit's default bound of 64 cannot hold; 1e6 (+120 dBFS) still catches
// a runaway, and NaN and infinity are a separate check.
const MAX_OUTPUT: f32 = 1.0e6;

fn seeds() -> u64 {
    std::env::var("OPENPLUGIN_SOAK_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(24)
}

#[test]
#[ignore]
fn soak_many_seeds() {
    for seed in 1..=seeds() {
        let report = conformance(OpenLimiter::default, Options { fuzz_sessions: 12, latency_depends_on_params: true, max_output: MAX_OUTPUT, ..Options::default() }.with_seed(seed));
        assert!(report.passed(), "seed {seed}:\n{report}");
    }
}

#[test]
#[ignore]
fn fuzz_strict() {
    // Switching modules, look-ahead or linear-phase modes changes the delay by design; the host is told.
    let options = Options {
        fuzz_sessions: 32,
        strict_hostile_output: true,
        strict_denormals: true,
        latency_depends_on_params: true,
        max_output: MAX_OUTPUT,
        ..Options::default()
    };
    conformance(OpenLimiter::default, options).assert_pass();
}
