#![allow(dead_code)]

pub mod shipping;

use harmonics::*;
use olm_api::Module;

pub const SR: f64 = 48_000.0;

/// In-place radix-2 complex FFT (length a power of two).
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
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
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Power spectrum (|X|^2) of a real block.
pub fn power(x: &[f64]) -> Vec<f64> {
    let mut re = x.to_vec();
    let mut im = vec![0.0; x.len()];
    fft(&mut re, &mut im);
    re.iter().zip(&im).take(x.len() / 2 + 1).map(|(a, b)| a * a + b * b).collect()
}

/// Complex spectrum of a real block.
pub fn spectrum(x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut re = x.to_vec();
    let mut im = vec![0.0; x.len()];
    fft(&mut re, &mut im);
    (re, im)
}

pub fn db(x: f64) -> f64 {
    10.0 * x.max(1e-300).log10()
}

pub fn sine(n: usize, f: f64, amp: f64, sr: f64) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * std::f64::consts::PI * f * i as f64 / sr).sin()) as f32).collect()
}

pub fn noise(n: usize, seed: u64, amp: f32) -> Vec<f32> {
    let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            amp * (((s >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0) as f32
        })
        .collect()
}

/// A module at 48 kHz with the given (index, value) settings and auto gain off unless set.
pub fn module(settings: &[(usize, f32)]) -> Harmonics {
    let mut m = Harmonics::default();
    m.prepare(SR as f32, 512);
    m.set_param(P_AUTO_GAIN, 0.0);
    for &(i, v) in settings {
        m.set_param(i, v);
    }
    m.reset();
    m
}

/// Runs a stereo signal through in blocks of `block`.
pub fn run(m: &mut Harmonics, l: &[f32], r: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    let mut i = 0;
    while i < a.len() {
        let e = (i + block).min(a.len());
        m.process(&mut a[i..e], &mut b[i..e]);
        i = e;
    }
    (a, b)
}

pub fn run_mono(m: &mut Harmonics, x: &[f32]) -> Vec<f32> {
    run(m, x, x, 256).0
}

/// A bin-exact frequency near `f` for an `n`-point FFT at `sr`, odd bin.
pub fn bin_freq(f: f64, n: usize, sr: f64) -> (usize, f64) {
    let mut k = (f * n as f64 / sr).round() as usize;
    if k % 2 == 0 {
        k += 1;
    }
    (k, k as f64 * sr / n as f64)
}
