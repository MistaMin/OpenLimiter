//! Shared code for the OpenEQ port: filter design for all 48 OpenEQ types (continuous slope,
//! cut order map, fitted and component-model types), the band model and its zero-latency engine,
//! stereo placement and pan, response curves for an editor, an FFT, a 2x half-band
//! oversampler, and the latency-change fade.
//!
//! Port of OpenEQ 1.0.1 (MIT, the owner's own code): `FilterDesign.h`, `VintageFilters.h`,
//! `AnalogModel.h`, `ResponseModel.h`, `EQBand.h` and the band path of `PluginProcessor.cpp`.

// Fitted tables keep the original's literals as written.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

pub mod band;
pub mod curve;
pub mod cx;
pub mod design;
pub mod engine;
pub mod fade;
pub mod fft;
pub mod jfilt;
pub mod oversample;
pub mod response;
pub mod smooth;
pub mod types;
pub mod vintage;

pub use engine::{BandState, ZlEngine, BANDS};
pub use types::{FilterType, StereoMode, FILTER_TYPE_COUNT, FILTER_TYPE_NAMES, PLACEMENT_NAMES};

/// Replaces NaN and infinities with zero; true when there was one.
pub fn zero_non_finite(x: &mut [f32]) -> bool {
    let mut found = false;
    for v in x.iter_mut() {
        if !v.is_finite() {
            *v = 0.0;
            found = true;
        }
    }
    found
}
