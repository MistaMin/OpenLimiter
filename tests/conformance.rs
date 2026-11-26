//! TS-1: the conformance kit on this plugin. Debug builds run Options::quick() (the Stop hook);
//! release builds run every rate and block size (the commit tester).

use openlimiter::OpenLimiter;
use openplugin::testing::{Options, conformance};

openplugin::testing::install_allocator!();

#[test]
fn conforms() {
    let mut options = if cfg!(debug_assertions) { Options::quick() } else { Options::default() };
    // Switching modules, look-ahead or linear-phase modes changes the delay by design; the host is told.
    options.latency_depends_on_params = true;
    conformance(OpenLimiter::default, options).assert_pass();
}
