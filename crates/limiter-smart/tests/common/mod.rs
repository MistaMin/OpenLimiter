//! Shared test harness: runner, signals, reference true-peak meters (BS.1770 4x and a 16x
//! windowed-sinc reconstruction) and analysis.
#![allow(dead_code)]

use limiter_smart::{idx, SmartLimiter};
use olm_api::Module;
use std::f64::consts::PI;

pub const SR: f64 = 48_000.0;

pub fn db(v: f64) -> f64 {
    20.0 * (v.abs() + 1e-300).log10()
}
pub fn lin(d: f64) -> f64 {
    10f64.powf(d / 20.0)
}

pub fn make(sr: f64, sets: &[(usize, f32)]) -> SmartLimiter {
    let mut m = SmartLimiter::default();
    for (i, v) in sets {
        m.set_param(*i, *v);
    }
    m.prepare(sr as f32, 512);
    m
}

/// Runs a stereo signal through the module in blocks of `block` and removes the latency.
pub fn run_block(m: &mut SmartLimiter, l: &[f64], r: &[f64], block: usize) -> (Vec<f64>, Vec<f64>) {
    let lat = m.latency();
    let n = l.len();
    let mut a: Vec<f32> = l.iter().map(|v| *v as f32).collect();
    let mut b: Vec<f32> = r.iter().map(|v| *v as f32).collect();
    a.extend(std::iter::repeat(0.0).take(lat));
    b.extend(std::iter::repeat(0.0).take(lat));
    for (ca, cb) in a.chunks_mut(block.max(1)).zip(b.chunks_mut(block.max(1))) {
        m.process(ca, cb);
    }
    (a[lat..lat + n].iter().map(|v| *v as f64).collect(), b[lat..lat + n].iter().map(|v| *v as f64).collect())
}

pub fn run(m: &mut SmartLimiter, l: &[f64], r: &[f64]) -> (Vec<f64>, Vec<f64>) {
    run_block(m, l, r, 512)
}

/// As `run`, also returning the per-sample gain trace (dB), aligned with the output.
pub fn run_traced(m: &mut SmartLimiter, l: &[f64], r: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let lat = m.latency();
    let n = l.len();
    let mut a: Vec<f32> = l.iter().map(|v| *v as f32).collect();
    let mut b: Vec<f32> = r.iter().map(|v| *v as f32).collect();
    a.extend(std::iter::repeat(0.0).take(lat));
    b.extend(std::iter::repeat(0.0).take(lat));
    let mut g = vec![0.0f32; a.len()];
    let mut pos = 0;
    while pos < a.len() {
        let e = (pos + 512).min(a.len());
        m.process_traced(&mut a[pos..e], &mut b[pos..e], Some(&mut g[pos..e]));
        pos = e;
    }
    (
        a[lat..lat + n].iter().map(|v| *v as f64).collect(),
        b[lat..lat + n].iter().map(|v| *v as f64).collect(),
        g[lat..lat + n].iter().map(|v| *v as f64).collect(),
    )
}

pub fn sine(f: f64, amp: f64, n: usize, sr: f64, phase: f64) -> Vec<f64> {
    (0..n).map(|i| amp * (2.0 * PI * f * i as f64 / sr + phase).sin()).collect()
}

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
    pub fn gauss(&mut self) -> f64 {
        let mut s = 0.0;
        for _ in 0..6 {
            s += self.next();
        }
        s / 6f64.sqrt() * 2f64.sqrt()
    }
}

pub fn noise(n: usize, amp: f64, seed: u64) -> Vec<f64> {
    let mut r = Rng(seed.max(1));
    (0..n).map(|_| amp * r.gauss()).collect()
}

pub fn peak(x: &[f64]) -> f64 {
    x.iter().fold(0.0f64, |a, v| a.max(v.abs()))
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..200 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < 1e-18 * sum {
            break;
        }
    }
    sum
}

/// BS.1770-4 Annex 2 true peak: 4x, 48-tap polyphase FIR (12 taps per phase).
pub fn tp_bs1770(x: &[f64]) -> f64 {
    const H: [[f64; 4]; 12] = [
        [0.0017089843750, -0.0291748046875, -0.0189208984375, -0.0083007812500],
        [0.0109863281250, 0.0292968750000, 0.0330810546875, 0.0148925781250],
        [-0.0196533203125, -0.0517578125000, -0.0582275390625, -0.0266113281250],
        [0.0332031250000, 0.0891113281250, 0.1015625000000, 0.0476074218750],
        [-0.0594482421875, -0.1665039062500, -0.2003173828125, -0.1022949218750],
        [0.1373291015625, 0.4650878906250, 0.7797851562500, 0.9721679687500],
        [0.9721679687500, 0.7797851562500, 0.4650878906250, 0.1373291015625],
        [-0.1022949218750, -0.2003173828125, -0.1665039062500, -0.0594482421875],
        [0.0476074218750, 0.1015625000000, 0.0891113281250, 0.0332031250000],
        [-0.0266113281250, -0.0582275390625, -0.0517578125000, -0.0196533203125],
        [0.0148925781250, 0.0330810546875, 0.0292968750000, 0.0109863281250],
        [-0.0083007812500, -0.0189208984375, -0.0291748046875, 0.0017089843750],
    ];
    // only outputs whose 12 taps all lie inside `x`, so the edges of a slice do not ring
    let mut pk = 0.0f64;
    for n in 11..x.len() {
        for p in 0..4 {
            let mut acc = 0.0;
            for t in 0..12 {
                acc += H[t][p] * x[n - t];
            }
            pk = pk.max(acc.abs());
        }
    }
    pk.max(peak(x))
}

/// 16x reconstruction with a long Kaiser-windowed sinc (64 taps each side, beta 12, unit DC gain).
pub fn tp_sinc16(x: &[f64]) -> f64 {
    const HALF: i64 = 64;
    let beta = 12.0;
    let mut kern = vec![[0.0f64; 2 * HALF as usize]; 15];
    for (p, row) in kern.iter_mut().enumerate() {
        let frac = (p + 1) as f64 / 16.0;
        for (j, h) in row.iter_mut().enumerate() {
            let t = j as f64 - (HALF as f64 - 1.0) - frac;
            let s = if t.abs() < 1e-12 { 1.0 } else { (PI * t).sin() / (PI * t) };
            let r = t / HALF as f64;
            let w = if r.abs() < 1.0 { bessel_i0(beta * (1.0 - r * r).sqrt()) / bessel_i0(beta) } else { 0.0 };
            *h = s * w;
        }
        let sum: f64 = row.iter().sum();
        for h in row.iter_mut() {
            *h /= sum;
        }
    }
    let n = x.len() as i64;
    let mut pk = peak(x);
    // only intervals whose kernel lies inside `x`, so the edges of a slice do not ring
    for c in (HALF - 1)..(n - HALF) {
        for row in kern.iter() {
            let mut acc = 0.0;
            for (j, h) in row.iter().enumerate() {
                acc += h * x[(c - (HALF - 1)) as usize + j];
            }
            pk = pk.max(acc.abs());
        }
    }
    pk
}

/// Both reference meters over both channels, dB.
pub fn true_peak_db(l: &[f64], r: &[f64]) -> (f64, f64) {
    (db(tp_bs1770(l).max(tp_bs1770(r))), db(tp_sinc16(l).max(tp_sinc16(r))))
}

/// THD+N of `x` against the best-fitting sine at `f` (plus DC), dB.
pub fn thd_n(x: &[f64], f: f64, sr: f64) -> f64 {
    let n = x.len();
    let w = 2.0 * PI * f / sr;
    // least squares on [sin, cos, 1]
    let mut a = [[0.0f64; 3]; 3];
    let mut b = [0.0f64; 3];
    for (i, v) in x.iter().enumerate() {
        let (s, c) = (w * i as f64).sin_cos();
        let row = [s, c, 1.0];
        for p in 0..3 {
            for q in 0..3 {
                a[p][q] += row[p] * row[q];
            }
            b[p] += row[p] * v;
        }
    }
    let sol = solve3(a, b);
    let mut e = 0.0;
    let mut s = 0.0;
    for (i, v) in x.iter().enumerate() {
        let (sn, cs) = (w * i as f64).sin_cos();
        let fit = sol[0] * sn + sol[1] * cs + sol[2];
        e += (v - fit) * (v - fit);
        s += fit * fit;
    }
    let _ = n;
    10.0 * (e / s).log10()
}

fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> [f64; 3] {
    for i in 0..3 {
        let mut p = i;
        for r in i + 1..3 {
            if a[r][i].abs() > a[p][i].abs() {
                p = r;
            }
        }
        a.swap(i, p);
        b.swap(i, p);
        for r in i + 1..3 {
            let f = a[r][i] / a[i][i];
            for c in i..3 {
                a[r][c] -= f * a[i][c];
            }
            b[r] -= f * b[i];
        }
    }
    let mut x = [0.0; 3];
    for i in (0..3).rev() {
        let mut s = b[i];
        for c in i + 1..3 {
            s -= a[i][c] * x[c];
        }
        x[i] = s / a[i][i];
    }
    x
}

/// Amplitude of the component at `f` (single-bin DFT with a Hann window).
pub fn tone_amp(x: &[f64], f: f64, sr: f64) -> f64 {
    let n = x.len();
    let (mut re, mut im, mut wsum) = (0.0, 0.0, 0.0);
    for (i, v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
        let ph = 2.0 * PI * f * i as f64 / sr;
        re += w * v * ph.cos();
        im += w * v * ph.sin();
        wsum += w;
    }
    2.0 * (re * re + im * im).sqrt() / wsum
}

pub const ENGINE_NAMES: [&str; 6] = ["Pure", "Smooth", "Precise", "Punch", "Multiband", "Glide"];

pub fn engine(e: usize) -> (usize, f32) {
    (idx::ENGINE, e as f32)
}

/// Amplitude of the component at `f` with a rectangular window (exact for components on the
/// 1/len grid of a periodic steady state).
pub fn dft_amp(x: &[f64], f: f64, sr: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    let w = 2.0 * PI * f / sr;
    for (i, v) in x.iter().enumerate() {
        let (s, c) = (w * i as f64).sin_cos();
        re += v * c;
        im += v * s;
    }
    2.0 * (re * re + im * im).sqrt() / x.len() as f64
}

/// Folds a frequency into 0..sr/2.
pub fn fold(f: f64, sr: f64) -> f64 {
    let m = f.rem_euclid(sr);
    if m > sr / 2.0 { sr - m } else { m }
}
