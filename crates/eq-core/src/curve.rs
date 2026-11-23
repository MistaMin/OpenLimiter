//! Response curves for an editor: per band and combined, in dB at a list of frequencies.
//! Allocation-free (designs live on the stack).

use crate::engine::{BandState, BANDS};
use crate::response::{band_matrix, design_band, multiply, DesignedBand, Matrix};

fn to_db(m: f64) -> f32 {
    (20.0 * m.max(1.0e-12).log10()) as f32
}

fn design(b: &BandState, sr: f64, offset_db: f32) -> DesignedBand {
    design_band(b.ty, b.frequency, b.gain_db + offset_db, b.q, b.slope_db, sr)
}

/// One band's magnitude in dB at each of `freqs` (Hz), ignoring placement. `gain_offset_db` is
/// added to the band gain (the dynamic module passes its live gain here).
pub fn band_db(b: &BandState, sr: f64, gain_offset_db: f32, freqs: &[f32], out: &mut [f32]) {
    let d = design(b, sr, gain_offset_db);
    for (o, &f) in out.iter_mut().zip(freqs.iter()) {
        *o = to_db(d.magnitude(f as f64, sr));
    }
}

/// The combined magnitude of every audible band (the product of their magnitudes), in dB.
/// `offsets` adds a per-band gain in dB (dynamic gain), `None` for the static curve.
pub fn total_db(bands: &[BandState; BANDS], sr: f64, offsets: Option<&[f32; BANDS]>, freqs: &[f32], out: &mut [f32]) {
    let solo = crate::engine::any_solo(bands);
    let n = freqs.len().min(out.len());
    for o in out[..n].iter_mut() {
        *o = 1.0;
    }
    for (i, b) in bands.iter().enumerate() {
        if !b.audible(solo) {
            continue;
        }
        let d = design(b, sr, offsets.map_or(0.0, |o| o[i]));
        for k in 0..n {
            out[k] *= d.magnitude(freqs[k] as f64, sr) as f32;
        }
    }
    for o in out[..n].iter_mut() {
        *o = to_db(*o as f64);
    }
}

/// Channel-aware curves: the left-to-left and right-to-right gains in dB of the 2x2 matrix built
/// from every audible band in order, so Left/Right/Mid/Side/pan placements show per channel.
pub fn channel_db(bands: &[BandState; BANDS], sr: f64, offsets: Option<&[f32; BANDS]>, freqs: &[f32], left: &mut [f32], right: &mut [f32]) {
    let solo = crate::engine::any_solo(bands);
    let mut designs = [DesignedBand::default(); BANDS];
    let mut on = [false; BANDS];
    for (i, b) in bands.iter().enumerate() {
        if b.audible(solo) {
            designs[i] = design(b, sr, offsets.map_or(0.0, |o| o[i]));
            on[i] = true;
        }
    }
    let n = freqs.len().min(left.len()).min(right.len());
    for k in 0..n {
        let f = freqs[k] as f64;
        let mut m = Matrix::default();
        for i in 0..BANDS {
            if on[i] {
                m = multiply(band_matrix(designs[i].magnitude(f, sr), bands[i].stereo, bands[i].pan), m);
            }
        }
        left[k] = to_db(m.ll.abs());
        right[k] = to_db(m.rr.abs());
    }
}
