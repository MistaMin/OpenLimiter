//! Biquad coefficient formulas with the conventions of the IIR coefficient helpers the original
//! was built on (bilinear designs; shelves and peaks clamp the frequency at 2 Hz). Each returns
//! raw `[b0, b1, b2, a0, a1, a2]`; first-order designs leave b2 = a2 = 0.

use std::f64::consts::PI;

pub type Raw = [f64; 6];

pub const UNITY: Raw = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// dB to linear gain, with silence at or below -100 dB (the decibel helper the original used).
#[inline]
pub fn db_to_gain(db: f64) -> f64 {
    if db > -100.0 {
        10f64.powf(db * 0.05)
    } else {
        0.0
    }
}

/// Linear gain to dB, floored at `floor`.
#[inline]
pub fn gain_to_db(g: f64, floor: f64) -> f64 {
    if g > 0.0 {
        (20.0 * g.log10()).max(floor)
    } else {
        floor
    }
}

pub fn first_order_low_pass(sr: f64, f: f64) -> Raw {
    let n = (PI * f / sr).tan();
    [n, n, 0.0, n + 1.0, n - 1.0, 0.0]
}

pub fn first_order_high_pass(sr: f64, f: f64) -> Raw {
    let n = (PI * f / sr).tan();
    [1.0, -1.0, 0.0, n + 1.0, n - 1.0, 0.0]
}

pub fn low_pass(sr: f64, f: f64, q: f64) -> Raw {
    let n = 1.0 / (PI * f / sr).tan();
    let n2 = n * n;
    let iq = 1.0 / q;
    let c1 = 1.0 / (1.0 + iq * n + n2);
    [c1, c1 * 2.0, c1, 1.0, c1 * 2.0 * (1.0 - n2), c1 * (1.0 - iq * n + n2)]
}

pub fn high_pass(sr: f64, f: f64, q: f64) -> Raw {
    let n = (PI * f / sr).tan();
    let n2 = n * n;
    let iq = 1.0 / q;
    let c1 = 1.0 / (1.0 + iq * n + n2);
    [c1, c1 * -2.0, c1, 1.0, c1 * 2.0 * (n2 - 1.0), c1 * (1.0 - iq * n + n2)]
}

pub fn band_pass(sr: f64, f: f64, q: f64) -> Raw {
    let n = 1.0 / (PI * f / sr).tan();
    let n2 = n * n;
    let iq = 1.0 / q;
    let c1 = 1.0 / (1.0 + iq * n + n2);
    [c1 * n * iq, 0.0, -c1 * n * iq, 1.0, c1 * 2.0 * (1.0 - n2), c1 * (1.0 - iq * n + n2)]
}

pub fn notch(sr: f64, f: f64, q: f64) -> Raw {
    let n = 1.0 / (PI * f / sr).tan();
    let n2 = n * n;
    let iq = 1.0 / q;
    let c1 = 1.0 / (1.0 + n * iq + n2);
    let b0 = c1 * (1.0 + n2);
    let b1 = 2.0 * c1 * (1.0 - n2);
    [b0, b1, b0, 1.0, b1, c1 * (1.0 - n * iq + n2)]
}

pub fn all_pass(sr: f64, f: f64, q: f64) -> Raw {
    let n = 1.0 / (PI * f / sr).tan();
    let n2 = n * n;
    let iq = 1.0 / q;
    let c1 = 1.0 / (1.0 + iq * n + n2);
    let b0 = c1 * (1.0 - n * iq + n2);
    let b1 = c1 * 2.0 * (1.0 - n2);
    [b0, b1, 1.0, 1.0, b1, b0]
}

pub fn low_shelf(sr: f64, f: f64, q: f64, gain: f64) -> Raw {
    let a = gain.sqrt().max(0.0);
    let am = a - 1.0;
    let ap = a + 1.0;
    let w = 2.0 * PI * f.max(2.0) / sr;
    let c = w.cos();
    let beta = w.sin() * a.sqrt() / q;
    let amc = am * c;
    [
        a * (ap - amc + beta),
        a * 2.0 * (am - ap * c),
        a * (ap - amc - beta),
        ap + amc + beta,
        -2.0 * (am + ap * c),
        ap + amc - beta,
    ]
}

pub fn high_shelf(sr: f64, f: f64, q: f64, gain: f64) -> Raw {
    let a = gain.sqrt().max(0.0);
    let am = a - 1.0;
    let ap = a + 1.0;
    let w = 2.0 * PI * f.max(2.0) / sr;
    let c = w.cos();
    let beta = w.sin() * a.sqrt() / q;
    let amc = am * c;
    [
        a * (ap + amc + beta),
        a * -2.0 * (am + ap * c),
        a * (ap + amc - beta),
        ap - amc + beta,
        2.0 * (am - ap * c),
        ap - amc - beta,
    ]
}

pub fn peak(sr: f64, f: f64, q: f64, gain: f64) -> Raw {
    let a = gain.sqrt().max(0.0);
    let w = 2.0 * PI * f.max(2.0) / sr;
    let alpha = w.sin() / (q * 2.0);
    let c2 = -2.0 * w.cos();
    let at = alpha * a;
    let ao = alpha / a;
    [1.0 + at, c2, 1.0 - at, 1.0 + ao, c2, 1.0 - ao]
}
