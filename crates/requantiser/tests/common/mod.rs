//! Test helpers: rendering, an FFT for any length (Bluestein over radix 2) and the band analyses of the
//! measurement scripts (`measurements/scripts/srcA_dither_float.py`, `srcD_dither.py`, `srcB_rms_dither.py`,
//! `srcH_jobs.py`), re-implemented in Rust.
#![allow(dead_code)]

use olm_api::Module;
use requantiser::Requantiser;
use std::f64::consts::PI;

pub const SR: usize = 48_000;

pub fn lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn db_pow(p: f64) -> f64 {
    10.0 * (p + 1e-40).log10()
}

pub fn rms_db(x: &[f64]) -> f64 {
    let ms = x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64;
    if ms == 0.0 { f64::NEG_INFINITY } else { 10.0 * ms.log10() }
}

/// A 1 kHz sine of `n` samples at `level_db` dBFS peak, phase 0.
pub fn tone(level_db: f64, n: usize) -> Vec<f64> {
    let a = lin(level_db);
    (0..n).map(|i| a * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect()
}

/// As written to a 24-bit WAV by the scripts (`round(x * 8388607)`) and read back (`/ 8388608`).
pub fn as_24bit_file(x: &[f64]) -> Vec<f64> {
    x.iter().map(|v| (v * 8_388_607.0).round() / 8_388_608.0).collect()
}

/// Settings: word index (0 Off, 1 = 24, 2 = 22, 3 = 20, 4 = 18, 5 = 16, 6 = 12, 7 = 8 bits), dither amount in %,
/// shaping index, depth in %.
pub fn module(word: usize, amount: f32, shaping: usize, depth: f32) -> Requantiser {
    let mut m = Requantiser::default();
    m.prepare(SR as f32, 512);
    m.set_param(requantiser::P_WORD_LENGTH, word as f32);
    m.set_param(requantiser::P_DITHER_AMOUNT, amount);
    m.set_param(requantiser::P_SHAPING, shaping as f32);
    m.set_param(requantiser::P_SHAPING_DEPTH, depth);
    m
}

/// Renders the same signal on both channels; returns (left, right).
pub fn render2(m: &mut Requantiser, x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut outl = Vec::with_capacity(x.len());
    let mut outr = Vec::with_capacity(x.len());
    let mut l = [0f32; 512];
    let mut r = [0f32; 512];
    for chunk in x.chunks(512) {
        let n = chunk.len();
        for i in 0..n {
            l[i] = chunk[i] as f32;
            r[i] = chunk[i] as f32;
        }
        m.process(&mut l[..n], &mut r[..n]);
        outl.extend(l[..n].iter().map(|&v| v as f64));
        outr.extend(r[..n].iter().map(|&v| v as f64));
    }
    (outl, outr)
}

pub fn render(m: &mut Requantiser, x: &[f64]) -> Vec<f64> {
    render2(m, x).0
}

// ---------------------------------------------------------------- FFT

#[derive(Clone, Copy, Debug, Default)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

impl C {
    fn mul(self, o: C) -> C {
        C { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
    fn conj(self) -> C {
        C { re: self.re, im: -self.im }
    }
    pub fn norm2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}

fn fft_pow2(a: &mut [C], inverse: bool) {
    let n = a.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let wl = C { re: ang.cos(), im: ang.sin() };
        for s in (0..n).step_by(len) {
            let mut w = C { re: 1.0, im: 0.0 };
            for k in 0..len / 2 {
                let u = a[s + k];
                let v = a[s + k + len / 2].mul(w);
                a[s + k] = C { re: u.re + v.re, im: u.im + v.im };
                a[s + k + len / 2] = C { re: u.re - v.re, im: u.im - v.im };
                w = w.mul(wl);
            }
        }
        len <<= 1;
    }
    if inverse {
        let s = 1.0 / n as f64;
        for v in a.iter_mut() {
            v.re *= s;
            v.im *= s;
        }
    }
}

/// DFT of a real signal of any length, bins 0..=n/2 (like numpy's rfft).
pub fn rfft(x: &[f64]) -> Vec<C> {
    let n = x.len();
    let half = n / 2 + 1;
    if n.is_power_of_two() {
        let mut a: Vec<C> = x.iter().map(|&v| C { re: v, im: 0.0 }).collect();
        fft_pow2(&mut a, false);
        a.truncate(half);
        return a;
    }
    let m = (2 * n - 1).next_power_of_two();
    let w: Vec<C> = (0..n)
        .map(|k| {
            let k2 = ((k as u128 * k as u128) % (2 * n as u128)) as f64;
            let ang = -PI * k2 / n as f64;
            C { re: ang.cos(), im: ang.sin() }
        })
        .collect();
    let mut a = vec![C::default(); m];
    let mut b = vec![C::default(); m];
    for k in 0..n {
        a[k] = C { re: x[k] * w[k].re, im: x[k] * w[k].im };
        b[k] = w[k].conj();
        if k > 0 {
            b[m - k] = w[k].conj();
        }
    }
    fft_pow2(&mut a, false);
    fft_pow2(&mut b, false);
    for i in 0..m {
        a[i] = a[i].mul(b[i]);
    }
    fft_pow2(&mut a, true);
    (0..half).map(|k| a[k].mul(w[k])).collect()
}

/// numpy.hanning(n)
pub fn hanning(n: usize) -> Vec<f64> {
    if n == 1 {
        return vec![1.0];
    }
    (0..n).map(|k| 0.5 - 0.5 * (2.0 * PI * k as f64 / (n - 1) as f64).cos()).collect()
}

/// Power per bin scaled so a band sum is the mean square in that band (the scripts' `Z`), and bin frequencies.
pub fn power_spectrum(m: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = m.len();
    let w = hanning(n);
    let sw2: f64 = w.iter().map(|v| v * v).sum();
    let xw: Vec<f64> = m.iter().zip(&w).map(|(a, b)| a * b).collect();
    let z: Vec<f64> = rfft(&xw).iter().map(|c| c.norm2() / (sw2 * n as f64 / 2.0)).collect();
    let f: Vec<f64> = (0..z.len()).map(|k| k as f64 * SR as f64 / n as f64).collect();
    (z, f)
}

pub fn band_sum(z: &[f64], f: &[f64], lo: f64, hi: f64) -> f64 {
    z.iter().zip(f).filter(|(_, &ff)| ff >= lo && ff < hi).map(|(v, _)| *v).sum()
}

pub const BANDS5: [(f64, f64); 5] =
    [(2000.0, 4000.0), (4000.0, 12000.0), (12000.0, 16000.0), (16000.0, 20000.0), (20000.0, 24000.0)];
pub const SHARE_BANDS: [(f64, f64); 5] =
    [(0.0, 4000.0), (4000.0, 12000.0), (12000.0, 16000.0), (16000.0, 20000.0), (20000.0, 24000.0)];

/// Noise 2-24 kHz and the five absolute bands, rounded to 0.1 dB like the stored figures.
pub fn noise_bands(m: &[f64]) -> (f64, [f64; 5]) {
    let (z, f) = power_spectrum(m);
    let r = |v: f64| (v * 10.0).round() / 10.0;
    let tot = r(db_pow(band_sum(&z, &f, 2000.0, 24000.0)));
    let mut b = [0.0; 5];
    for (i, (lo, hi)) in BANDS5.iter().enumerate() {
        b[i] = r(db_pow(band_sum(&z, &f, *lo, *hi)));
    }
    (tot, b)
}

/// Share of each band in the total power, dB (the source-H analysis; Hann window, no scaling needed).
pub fn band_shares(z_sig: &[f64]) -> [f64; 5] {
    let (z, f) = power_spectrum(z_sig);
    let tot: f64 = z.iter().sum::<f64>() + 1e-300;
    let mut s = [0.0; 5];
    for (i, (lo, hi)) in SHARE_BANDS.iter().enumerate() {
        s[i] = 10.0 * (band_sum(&z, &f, *lo, *hi) / tot + 1e-30).log10();
    }
    s
}

// ---------------------------------------------------------------- result table

pub struct Table {
    pub rows: Vec<String>,
    pub failures: usize,
}

impl Table {
    pub fn new(title: &str) -> Self {
        println!("\n### {title}\n| Setting | Quantity | Stored | Measured | Diff | Tol | Result |\n|---|---|---|---|---|---|---|");
        Self { rows: Vec::new(), failures: 0 }
    }

    /// `tol` <= 0 marks an informational row (reported, not judged).
    pub fn row(&mut self, setting: &str, quantity: &str, stored: f64, measured: f64, tol: f64) {
        let fmt = |v: f64| if v.is_finite() { format!("{v:.1}") } else if v < 0.0 { "silent".into() } else { "nan".into() };
        let both_silent = stored <= -199.0 && (measured <= -199.0 || !measured.is_finite());
        let diff = if both_silent { 0.0 } else { measured - stored };
        let result = if tol <= 0.0 {
            "info"
        } else if both_silent || diff.abs() <= tol {
            "pass"
        } else {
            self.failures += 1;
            "FAIL"
        };
        let tol_s = if tol <= 0.0 { "-".to_string() } else { format!("{tol}") };
        let diff_s = if diff.is_finite() { format!("{diff:+.1}") } else { "-".into() };
        let line = format!("| {setting} | {quantity} | {} | {} | {diff_s} | {tol_s} | {result} |", fmt(stored), fmt(measured));
        println!("{line}");
        self.rows.push(line);
    }

    pub fn finish(self) {
        assert_eq!(self.failures, 0, "{} judged rows outside tolerance", self.failures);
    }
}
