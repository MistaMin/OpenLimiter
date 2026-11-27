//! TS-1: the conformance kit on this plugin. Debug builds run Options::quick() (the Stop hook);
//! release builds run every rate and block size (the commit tester).

use openlimiter::OpenLimiter;
use openplugin::testing::{Options, conformance};

openplugin::testing::install_allocator!();

// The chain has no fixed output ceiling: the user orders the modules, and several have large
// gain controls (Compressor / De-esser make-up to +60 dB, input gain to +24 dB), so a random
// state can put +57 dB of make-up after the last limiter (peak 447 from full-scale noise; soak
// seed 84 reached 74). The kit's default bound of 64 cannot hold; 1e6 (+120 dBFS) still catches
// a runaway, and NaN and infinity are a separate check.
const MAX_OUTPUT: f32 = 1.0e6;

#[test]
fn conforms() {
    let mut options = if cfg!(debug_assertions) { Options::quick() } else { Options::default() };
    // Switching modules, look-ahead or linear-phase modes changes the delay by design; the host is told.
    options.latency_depends_on_params = true;
    options.max_output = MAX_OUTPUT;
    conformance(OpenLimiter::default, options).assert_pass();
}
