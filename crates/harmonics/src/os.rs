//! 2x / 4x oversampling of the residual (no ADAA at the high rate, as in the shipping preamp).
//!
//! The band source is interpolated, the residual `r(g u) / g` is taken at the high rate and
//! decimated. Only the residual passes the filters; the dry signal is delayed by the same whole
//! number of samples, so mix and harmonics-only listen line up exactly.
//!
//! Filters: Kaiser-windowed sinc with `32 L + 1` taps at the `L`-times rate, cutoff 0.479 of the
//! base rate (23 kHz at 48 kHz), beta 8 (about 80 dB stop band, transition about 7.5 kHz at
//! 48 kHz). Interpolator plus decimator delay: `2 * 16 L` high-rate samples = 32 base samples.

use crate::curve::Curve;

pub const MAX_TAPS: usize = 129;
/// Base-rate delay of the interpolator plus the decimator, for both factors.
pub const OS_LATENCY: usize = 32;
const XH: usize = 64; // base-rate history ring (>= ceil(129 / 2))
const ZH: usize = 256; // high-rate history ring (>= 129)

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..80 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < 1e-17 * sum {
            break;
        }
    }
    sum
}

/// The low-pass for factor `l` (2 or 4) at the high rate, unity DC gain; returns the tap count.
pub fn design(l: usize, h: &mut [f64; MAX_TAPS]) -> usize {
    let n = 32 * l + 1;
    let fc = 0.479 / l as f64; // cycles per high-rate sample
    let beta = 8.0;
    let m = (n - 1) as f64;
    let i0b = bessel_i0(beta);
    let mut sum = 0.0;
    *h = [0.0; MAX_TAPS];
    for (i, v) in h.iter_mut().take(n).enumerate() {
        let k = i as f64 - m / 2.0;
        let sinc = if k == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * k).sin() / (std::f64::consts::PI * k) };
        let r = 2.0 * i as f64 / m - 1.0;
        *v = sinc * bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b;
        sum += *v;
    }
    for v in h.iter_mut().take(n) {
        *v /= sum;
    }
    n
}

#[derive(Clone, Copy)]
pub struct Oversampler {
    xh: [f64; XH],
    xpos: usize,
    zh: [f64; ZH],
    zpos: usize,
}

impl Default for Oversampler {
    fn default() -> Self {
        Self { xh: [0.0; XH], xpos: 0, zh: [0.0; ZH], zpos: 0 }
    }
}

impl Oversampler {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One base-rate sample: returns the decimated residual, delayed by [`OS_LATENCY`].
    #[inline]
    pub fn step<C: Curve>(&mut self, c: &C, h: &[f64; MAX_TAPS], taps: usize, l: usize, xs: f64, g: f64) -> f64 {
        self.xpos = (self.xpos + 1) & (XH - 1);
        self.xh[self.xpos] = xs;
        let lf = l as f64;
        let inv_g = 1.0 / g;
        let mut out = 0.0;
        for p in 0..l {
            let mut acc = 0.0;
            let mut j = 0;
            loop {
                let i = l * j + p;
                if i >= taps {
                    break;
                }
                acc += h[i] * self.xh[self.xpos.wrapping_sub(j) & (XH - 1)];
                j += 1;
            }
            let z = c.r(g * lf * acc) * inv_g;
            self.zpos = (self.zpos + 1) & (ZH - 1);
            self.zh[self.zpos] = z;
            if p == 0 {
                let mut d = 0.0;
                for (i, hv) in h.iter().take(taps).enumerate() {
                    d += hv * self.zh[self.zpos.wrapping_sub(i) & (ZH - 1)];
                }
                out = d;
            }
        }
        out
    }

    pub fn flush(&mut self) {
        for v in self.xh.iter_mut().chain(self.zh.iter_mut()) {
            if v.abs() < 1e-30 {
                *v = 0.0;
            }
        }
    }
}
