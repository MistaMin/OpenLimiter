//! Shared helpers for the acceptance tests: the analysis code of the measurement scripts
//! (FFT-based Hilbert envelope, FFT oversampling, level read-outs) rewritten in Rust, and a
//! runner that does what the measurement host did (block processing, latency compensated).
#![allow(dead_code)]

pub mod reference;

use limiter_clear::ClearLimiter;
use olm_api::Module;
use std::f64::consts::PI;

pub fn lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn db(v: f64) -> f64 {
    20.0 * (v.abs() + 1e-15).log10()
}

// ---------------------------------------------------------------------------------------------
// FFT of any length (radix 2, and Bluestein's method for the other lengths).

fn fft_pow2(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    assert!(n.is_power_of_two());
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = if inverse { 2.0 * PI / len as f64 } else { -2.0 * PI / len as f64 };
        let half = len / 2;
        let wr: Vec<f64> = (0..half).map(|k| (ang * k as f64).cos()).collect();
        let wi: Vec<f64> = (0..half).map(|k| (ang * k as f64).sin()).collect();
        let mut i = 0;
        while i < n {
            for k in 0..half {
                let (a, b) = (i + k, i + k + half);
                let tr = re[b] * wr[k] - im[b] * wi[k];
                let ti = re[b] * wi[k] + im[b] * wr[k];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Unnormalised DFT (forward) or unnormalised inverse DFT of any length, in place.
pub fn fft(re: &mut Vec<f64>, im: &mut Vec<f64>, inverse: bool) {
    let n = re.len();
    assert_eq!(n, im.len());
    if n <= 1 {
        return;
    }
    if n.is_power_of_two() {
        fft_pow2(re, im, inverse);
        return;
    }
    let m = (2 * n - 1).next_power_of_two();
    let sign = if inverse { 1.0 } else { -1.0 };
    let (mut cr, mut ci) = (vec![0.0; n], vec![0.0; n]);
    for k in 0..n {
        let kk = ((k as u128 * k as u128) % (2 * n as u128)) as f64;
        let a = sign * PI * kk / n as f64;
        cr[k] = a.cos();
        ci[k] = a.sin();
    }
    let (mut ar, mut ai) = (vec![0.0; m], vec![0.0; m]);
    let (mut br, mut bi) = (vec![0.0; m], vec![0.0; m]);
    for k in 0..n {
        ar[k] = re[k] * cr[k] - im[k] * ci[k];
        ai[k] = re[k] * ci[k] + im[k] * cr[k];
        br[k] = cr[k];
        bi[k] = -ci[k];
        if k > 0 {
            br[m - k] = cr[k];
            bi[m - k] = -ci[k];
        }
    }
    fft_pow2(&mut ar, &mut ai, false);
    fft_pow2(&mut br, &mut bi, false);
    for k in 0..m {
        let tr = ar[k] * br[k] - ai[k] * bi[k];
        let ti = ar[k] * bi[k] + ai[k] * br[k];
        ar[k] = tr;
        ai[k] = ti;
    }
    fft_pow2(&mut ar, &mut ai, true);
    let s = 1.0 / m as f64;
    for k in 0..n {
        let (xr, xi) = (ar[k] * s, ai[k] * s);
        re[k] = xr * cr[k] - xi * ci[k];
        im[k] = xr * ci[k] + xi * cr[k];
    }
}

/// `hil` of the measurement scripts: magnitude of the analytic signal, FFT over the whole array.
pub fn hil(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    let mut re = v.to_vec();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im, false);
    for k in 0..n {
        let h = if k == 0 || k == n / 2 {
            1.0
        } else if k < n / 2 {
            2.0
        } else {
            0.0
        };
        re[k] *= h;
        im[k] *= h;
    }
    fft(&mut re, &mut im, true);
    let s = 1.0 / n as f64;
    (0..n).map(|k| (re[k] * s).hypot(im[k] * s)).collect()
}

/// Gain trace of the scripts: `20 log10(hil(y) + 1e-9) - 20 log10(hil(x) + 1e-9)`.
pub fn gain_trace(y: &[f64], x: &[f64]) -> Vec<f64> {
    let (ey, ex) = (hil(&y[..x.len()]), hil(x));
    ey.iter().zip(ex.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect()
}

/// `up` of the scripts: zero-padded spectrum, `k` times more samples.
pub fn up(v: &[f64], k: usize) -> Vec<f64> {
    let n = v.len();
    let mut re = v.to_vec();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im, false);
    let big = n * k;
    let (mut yr, mut yi) = (vec![0.0; big], vec![0.0; big]);
    for i in 0..=n / 2 {
        yr[i] = re[i];
        yi[i] = im[i];
        if i > 0 {
            yr[big - i] = re[i];
            yi[big - i] = -im[i];
        }
    }
    fft(&mut yr, &mut yi, true);
    let s = k as f64 / big as f64;
    yr.iter().map(|a| a * s).collect()
}

/// Magnitudes of `rfft(m * hanning(len))`.
pub fn hann_spectrum(m: &[f64]) -> Vec<f64> {
    let n = m.len();
    let mut re: Vec<f64> =
        m.iter().enumerate().map(|(i, v)| v * (0.5 - 0.5 * (2.0 * PI * i as f64 / (n - 1) as f64).cos())).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im, false);
    (0..=n / 2).map(|k| re[k].hypot(im[k])).collect()
}

pub fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

pub fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0f64, |m, x| m.max(x.abs()))
}

/// `sqrt(2 * mean(m^2))`: the level of a sine, from its RMS.
pub fn sine_level(m: &[f64]) -> f64 {
    (2.0 * m.iter().map(|x| x * x).sum::<f64>() / m.len() as f64).sqrt()
}

pub fn rms(m: &[f64]) -> f64 {
    (m.iter().map(|x| x * x).sum::<f64>() / m.len() as f64).sqrt()
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------------------------
// Runner.

/// A fresh module with the given plain parameter values on top of the defaults.
pub fn module(sets: &[(usize, f32)]) -> ClearLimiter {
    let mut m = ClearLimiter::default();
    for (i, v) in sets {
        m.set_param(*i, *v);
    }
    m
}

/// Processes a stereo signal in blocks of 512 at `sr` and removes the reported latency, as
/// the measurement host's offline render does. Returns left and right, as long as the input.
pub fn render_stereo(m: &mut ClearLimiter, left: &[f64], right: &[f64], sr: f64) -> (Vec<f64>, Vec<f64>) {
    m.prepare(sr as f32, 512);
    let lat = m.latency();
    let n = left.len();
    let mut l: Vec<f32> = left.iter().map(|v| *v as f32).collect();
    let mut r: Vec<f32> = right.iter().map(|v| *v as f32).collect();
    l.resize(n + lat, 0.0);
    r.resize(n + lat, 0.0);
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(a, b);
    }
    (l[lat..].iter().map(|v| *v as f64).collect(), r[lat..].iter().map(|v| *v as f64).collect())
}

pub fn render(m: &mut ClearLimiter, x: &[f64], sr: f64) -> Vec<f64> {
    render_stereo(m, x, x, sr).0
}

// ---------------------------------------------------------------------------------------------
// Report rows.

pub struct Report {
    name: &'static str,
    failed: usize,
    worst: f64,
}

impl Report {
    pub fn new(name: &'static str) -> Self {
        println!("\n=== {name}");
        Self { name, failed: 0, worst: 0.0 }
    }

    /// One compared figure; `tol` is the allowed absolute difference.
    pub fn row(&mut self, what: &str, mine: f64, stored: f64, tol: f64) {
        let diff = mine - stored;
        let ok = diff.abs() <= tol + 1e-9;
        if !ok {
            self.failed += 1;
        }
        self.worst = self.worst.max(diff.abs() / tol);
        println!("ROW | {} | {what} | mine {mine:.3} | stored {stored:.3} | diff {diff:+.3} | tol {tol:.3} | {}", self.name, if ok { "pass" } else { "FAIL" });
    }

    /// A figure that is reported but has no tolerance in the brief.
    pub fn info(&self, what: &str, mine: f64, stored: f64) {
        println!("INFO | {} | {what} | mine {mine:.3} | stored {stored:.3} | diff {:+.3}", self.name, mine - stored);
    }

    pub fn check(&mut self, what: &str, ok: bool) {
        if !ok {
            self.failed += 1;
        }
        println!("ROW | {} | {what} | {}", self.name, if ok { "pass" } else { "FAIL" });
    }

    pub fn finish(self) {
        println!("SUMMARY | {} | failed rows {} | worst difference {:.0} % of tolerance", self.name, self.failed, self.worst * 100.0);
        assert_eq!(self.failed, 0, "{}: {} rows outside tolerance", self.name, self.failed);
    }
}
