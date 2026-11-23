//! Alias suppression, measured the way the November 2026 ADAA page measured it, on the same
//! test clipper, and then on this module's own curves.
//!
//! Method (experiment 1 of the page): 48 bin-exact tones from 100 Hz to 20 kHz at 48 kHz,
//! -6 dBFS float input, coherent 65,536-point FFT (rectangular window). Alias power is every
//! bin that is not DC or an in-band harmonic, in dBc. In-band error is the error power of the
//! in-band harmonics (fundamental included) against the continuous-time curve's own harmonics
//! (single-period DFT), after removing each method's own delay. Means are over the tones, in dB.

mod common;

use common::shipping::{Mode, Shipping};
use common::*;
use harmonics::adaa::{self, Adaa1, Adaa2};
use harmonics::curve::{Curve, ShapeCurve, FAMILY_POLY, FAMILY_SAT, FAMILY_TAPE};
use harmonics::os::{self, Oversampler, MAX_TAPS, OS_LATENCY};

const N: usize = 65536;
const WARM: usize = 8192;

// ---------------------------------------------------------------- the page's test clipper

/// `f(x) = 1.5 p(g x + b)`, `p(u) = u - u^3/3` inside, `+-2/3` outside, written in `u = g x`
/// with the bias offset removed and the slope normalised: `phi(u) = (f - 1.5 p(b)) / s`,
/// `s = 1.5 (1 - b^2)`. Scaling does not change any dBc figure.
struct Clipper {
    b: f64,
    s: f64,
    pb: [f64; 3],
}

fn p_all(v: f64) -> (f64, f64, f64) {
    let a = v.abs();
    if a <= 1.0 {
        (v - v * v * v / 3.0, v * v / 2.0 - v.powi(4) / 12.0, v * v * v / 6.0 - v.powi(5) / 60.0)
    } else {
        ((2.0 / 3.0f64).copysign(v), 2.0 * a / 3.0 - 0.25, (a * a / 3.0 - a / 4.0 + 1.0 / 15.0).copysign(v))
    }
}

impl Clipper {
    fn new(b: f64) -> Self {
        let (p, p1, p2) = p_all(b);
        Self { b, s: 1.5 * (1.0 - b * b), pb: [p, p1, p2] }
    }
    fn all(&self, u: f64) -> (f64, f64, f64) {
        let (p, p1, p2) = p_all(u + self.b);
        let [pb, p1b, p2b] = self.pb;
        let k = 1.5 / self.s;
        let phi = (p - pb) * k;
        let phi1 = (p1 - p1b - pb * u) * k;
        let phi2 = (p2 - p2b - p1b * u - 0.5 * pb * u * u) * k;
        (phi - u, phi1 - 0.5 * u * u, phi2 - u * u * u / 6.0)
    }
}

impl Curve for Clipper {
    fn r(&self, u: f64) -> f64 {
        self.all(u).0
    }
    fn r1(&self, u: f64) -> f64 {
        self.all(u).1
    }
    fn r2(&self, u: f64) -> f64 {
        self.all(u).2
    }
    fn lip(&self) -> f64 {
        1.5 / self.s + 1.0
    }
}

// ---------------------------------------------------------------- methods

#[derive(Clone, Copy, Debug, PartialEq)]
enum M {
    Naive,
    PlainAdaa1,
    Shipping,
    Adaa1,
    Adaa1NoBlend,
    Adaa2,
    Adaa2NoBlend,
    TrianglePlain,
    Os2,
    Os4,
    Os4Ref383,
}

const ALL: [M; 11] = [M::Naive, M::PlainAdaa1, M::Shipping, M::Adaa1, M::Adaa1NoBlend, M::Adaa2, M::Adaa2NoBlend, M::TrianglePlain, M::Os2, M::Os4, M::Os4Ref383];

fn name(m: M) -> &'static str {
    match m {
        M::Naive => "naive",
        M::PlainAdaa1 => "plain ADAA1",
        M::Shipping => "shipping residual ADAA1 (port)",
        M::Adaa1 => "this ADAA (residual 1st, delay 1)",
        M::Adaa1NoBlend => "  same without blend",
        M::Adaa2 => "this ADAA2 (residual triangle)",
        M::Adaa2NoBlend => "  same without blend",
        M::TrianglePlain => "triangle, no correction",
        M::Os2 => "this 2x",
        M::Os4 => "this 4x",
        M::Os4Ref383 => "4x, 383-tap reference",
    }
}

/// Output (in input units: wet curve phi(g x)/g) and the method's delay in samples.
fn render<C: Curve>(m: M, c: &C, g: f64, x: &[f64], ref383: &[f64]) -> (Vec<f64>, f64) {
    let mut y = vec![0.0; x.len()];
    match m {
        M::Naive => {
            for (i, &v) in x.iter().enumerate() {
                y[i] = v + adaa::naive(c, v, g);
            }
            (y, 0.0)
        }
        M::PlainAdaa1 => {
            let mut x1 = 0.0;
            for (i, &v) in x.iter().enumerate() {
                y[i] = 0.5 * (v + x1) + adaa::mean1(c, g * x1, g * v) / g;
                x1 = v;
            }
            (y, 0.5)
        }
        M::Shipping => {
            let mut s = Shipping::default();
            for (i, &v) in x.iter().enumerate() {
                let u = g * v;
                let (r, r1, _) = (c.r(u), c.r1(u), 0.0);
                y[i] = s.step(u, u + r, r1 + 0.5 * u * u, 1.0, Mode::On) / g;
            }
            (y, 0.5)
        }
        M::Adaa1 | M::Adaa1NoBlend => {
            let mut e = Adaa1::default();
            e.no_blend = m == M::Adaa1NoBlend;
            let mut x1 = 0.0;
            for (i, &v) in x.iter().enumerate() {
                y[i] = x1 + e.step(c, v, g);
                x1 = v;
            }
            (y, 1.0)
        }
        M::Adaa2 | M::Adaa2NoBlend => {
            let mut e = Adaa2::default();
            e.no_blend = m == M::Adaa2NoBlend;
            let mut x1 = 0.0;
            for (i, &v) in x.iter().enumerate() {
                y[i] = x1 + e.step(c, v, g);
                x1 = v;
            }
            (y, 1.0)
        }
        M::TrianglePlain => {
            let (mut x1, mut x2) = (0.0, 0.0);
            for (i, &v) in x.iter().enumerate() {
                let (u0, u1, u2) = (g * v, g * x1, g * x2);
                let (rc, r1c, r2c) = (c.r(u1), c.r1(u1), c.r2(u1));
                let tri = adaa::tri_half(c, u1, rc, r1c, r2c, u0) + adaa::tri_half(c, u1, rc, r1c, r2c, u2);
                y[i] = (v + 4.0 * x1 + x2) / 6.0 + tri / g;
                x2 = x1;
                x1 = v;
            }
            (y, 1.0)
        }
        M::Os2 | M::Os4 => {
            let l = if m == M::Os2 { 2 } else { 4 };
            let mut h = [0.0; MAX_TAPS];
            let taps = os::design(l, &mut h);
            let mut o = Oversampler::default();
            let mut dry = vec![0.0; OS_LATENCY];
            dry.extend_from_slice(x);
            for (i, &v) in x.iter().enumerate() {
                y[i] = dry[i] + o.step(c, &h, taps, l, v, g);
            }
            (y, OS_LATENCY as f64)
        }
        M::Os4Ref383 => {
            // plain 4x: zero-stuff, 383-tap Kaiser (beta 10) at 4x, curve, same filter, keep every 4th
            let taps = ref383.len();
            // polyphase interpolation of the zero-stuffed input, curve at 4x, decimation
            let mut shaped = vec![0.0; x.len() * 4];
            for i in 0..x.len() * 4 {
                let mut acc = 0.0;
                let mut j = i % 4;
                while j < taps && j <= i {
                    acc += ref383[j] * 4.0 * x[(i - j) / 4];
                    j += 4;
                }
                shaped[i] = acc + c.r(g * acc) / g;
            }
            for i in 0..x.len() {
                let n = 4 * i;
                let mut acc = 0.0;
                for (j, hj) in ref383.iter().enumerate() {
                    if j > n {
                        break;
                    }
                    acc += hj * shaped[n - j];
                }
                y[i] = acc;
            }
            (y, (taps - 1) as f64 / 4.0)
        }
    }
}

fn kaiser_383() -> Vec<f64> {
    let n = 383;
    let fc = 0.5 / 4.0 * 0.958; // 23 kHz at 48 kHz base
    let beta = 10.0;
    let i0 = |x: f64| {
        let (mut s, mut t) = (1.0, 1.0);
        for k in 1..100 {
            t *= x * x / 4.0 / (k as f64 * k as f64);
            s += t;
        }
        s
    };
    let m = (n - 1) as f64;
    let mut h: Vec<f64> = (0..n)
        .map(|i| {
            let k = i as f64 - m / 2.0;
            let sinc = if k == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * k).sin() / (std::f64::consts::PI * k) };
            let r = 2.0 * i as f64 / m - 1.0;
            sinc * i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0(beta)
        })
        .collect();
    let s: f64 = h.iter().sum();
    h.iter_mut().for_each(|v| *v /= s);
    h
}

/// The continuous-time harmonics (complex, n = 0..nmax) of `phi(g a sin t) / g`.
fn ct_harmonics<C: Curve>(c: &C, g: f64, a: f64, nmax: usize) -> Vec<(f64, f64)> {
    let m = 4096;
    let ys: Vec<f64> = (0..m)
        .map(|k| {
            let v = a * (2.0 * std::f64::consts::PI * k as f64 / m as f64).sin();
            v + c.r(g * v) / g
        })
        .collect();
    (0..=nmax)
        .map(|n| {
            let (mut re, mut im) = (0.0, 0.0);
            for (k, y) in ys.iter().enumerate() {
                let t = 2.0 * std::f64::consts::PI * (n * k) as f64 / m as f64;
                re += y * t.cos();
                im -= y * t.sin();
            }
            (re / m as f64, im / m as f64)
        })
        .collect()
}

struct ToneResult {
    alias_dbc: f64,
    inband_db: f64,
}

fn measure_tone<C: Curve>(m: M, c: &C, g: f64, k: usize, amp: f64, ref383: &[f64]) -> ToneResult {
    let total = WARM + N;
    let x: Vec<f64> = (0..total).map(|i| (amp * (2.0 * std::f64::consts::PI * k as f64 * i as f64 / N as f64).sin()) as f32 as f64).collect();
    let (y, delay) = render(m, c, g, &x, ref383);
    let seg = &y[WARM..];
    let (re, im) = spectrum(seg);
    let p = |b: usize| re[b] * re[b] + im[b] * im[b];
    let mut harm_bins = vec![false; N / 2 + 1];
    let mut n = 1;
    while n * k <= N / 2 {
        harm_bins[n * k] = true;
        n += 1;
    }
    let nmax = n - 1;
    let fund = p(k);
    let mut alias = 0.0;
    for b in 1..=N / 2 {
        if !harm_bins[b] {
            alias += p(b);
        }
    }
    // in-band harmonic error after removing the delay (and the WARM offset, a multiple of nothing)
    let ct = ct_harmonics(c, g, amp, nmax);
    let mut err = 0.0;
    let mut c1 = 0.0;
    for n in 1..=nmax {
        let b = n * k;
        // X[b] / N = c_n exp(j n w (WARM - delay)): rotate back to compare with c_n
        let ph = 2.0 * std::f64::consts::PI * (n * k) as f64 * (WARM as f64 - delay) / N as f64;
        let (cr, ci) = (ph.cos(), ph.sin());
        let (mr, mi) = (re[b] / N as f64, im[b] / N as f64);
        let (ar, ai) = (mr * cr + mi * ci, mi * cr - mr * ci);
        let (er, ei) = (ar - ct[n].0, ai - ct[n].1);
        err += er * er + ei * ei;
        if n == 1 {
            c1 = ct[1].0 * ct[1].0 + ct[1].1 * ct[1].1;
        }
    }
    ToneResult { alias_dbc: db(alias / fund), inband_db: db(err / c1) }
}

fn tones(count: usize) -> Vec<usize> {
    (0..count)
        .map(|i| {
            let f = 100.0 * (200.0f64).powf(i as f64 / (count - 1) as f64);
            bin_freq(f, N, SR).0
        })
        .collect()
}

struct Row {
    all: f64,
    upper: f64,
    inband: f64,
}

fn sweep<C: Curve + Sync>(m: M, c: &C, g: f64, ks: &[usize], amp: f64, ref383: &[f64]) -> Row {
    let res: Vec<(f64, ToneResult)> = std::thread::scope(|s| {
        let hs: Vec<_> = ks.iter().map(|&k| s.spawn(move || (k as f64 * SR / N as f64, measure_tone(m, c, g, k, amp, ref383)))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mean = |v: Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
    Row {
        all: mean(res.iter().map(|r| r.1.alias_dbc).collect()),
        upper: mean(res.iter().filter(|r| r.0 > 4000.0).map(|r| r.1.alias_dbc).collect()),
        inband: mean(res.iter().map(|r| r.1.inband_db).collect()),
    }
}


/// The page's experiment-1 table, values all / above 4 kHz / in-band, for drives 2, 8, 32.
fn wiki(m: M) -> Option<[[f64; 3]; 3]> {
    match m {
        M::Naive => Some([[-75.2, -29.4, f64::NAN], [-44.2, -10.9, f64::NAN], [-28.5, -8.4, f64::NAN]]),
        M::PlainAdaa1 => Some([[-83.3, -41.3, -49.3], [-51.1, -19.4, -41.3], [-36.2, -18.0, -33.3]]),
        M::Shipping => Some([[-71.1, -29.1, -55.3], [-47.6, -10.6, -42.8], [-31.6, -4.3, -34.4]]),
        M::TrianglePlain => Some([[-86.0, -39.2, -46.0], [-55.5, -22.3, -36.4], [-41.0, -21.5, -28.4]]),
        M::Os4Ref383 => Some([[-117.6, -81.0, -101.2], [-84.9, -44.6, -79.4], [-62.0, -24.4, -67.3]]),
        _ => None,
    }
}

#[test]
fn reproduces_the_wiki_clipper_experiment() {
    let ks = tones(48);
    let amp = 10f64.powf(-6.0 / 20.0);
    let c = Clipper::new(0.15);
    let r383 = kaiser_383();
    println!("Test clipper, b = 0.15, 48 tones 100 Hz-20 kHz, -6 dBFS, 48 kHz. dBc all / >4 kHz / in-band dB; [wiki]");
    let mut table = std::collections::HashMap::new();
    for m in ALL {
        let mut line = format!("{:34}", name(m));
        for (di, g) in [2.0, 8.0, 32.0].into_iter().enumerate() {
            let row = sweep(m, &c, g, &ks, amp, &r383);
            line += &format!(" | g{g:<2} {:6.1} {:6.1} {:6.1}", row.all, row.upper, row.inband);
            if let Some(w) = wiki(m) {
                line += &format!(" [{:.1} {:.1} {:.1}]", w[di][0], w[di][1], w[di][2]);
            }
            table.insert((m as usize, di), row);
        }
        println!("{line}");
    }
    let t = |m: M, d: usize| &table[&(m as usize, d)];
    // The harness reproduces the page's naive and plain-ADAA1 rows (same clipper, same method).
    for d in 0..3 {
        let w = wiki(M::Naive).unwrap()[d];
        assert!((t(M::Naive, d).all - w[0]).abs() < 1.5 && (t(M::Naive, d).upper - w[1]).abs() < 1.5, "naive drive index {d}");
        let w = wiki(M::PlainAdaa1).unwrap()[d];
        assert!((t(M::PlainAdaa1, d).all - w[0]).abs() < 2.0 && (t(M::PlainAdaa1, d).upper - w[1]).abs() < 2.0, "plain ADAA1 drive index {d}");
    }
    // Oversampling beats every 1x method at heavy drive in the upper band.
    for d in 1..3 {
        for m in [M::Adaa1, M::Adaa2, M::Shipping] {
            assert!(t(M::Os4, d).upper < t(m, d).upper - 3.0, "4x vs {m:?} at drive index {d}");
        }
    }
    // Every 1x anti-aliasing method in this crate removes aliasing at every drive.
    for d in 0..3 {
        for m in [M::Adaa1, M::Adaa2] {
            assert!(t(m, d).all < t(M::Naive, d).all, "{m:?} at drive index {d}: {} vs naive {}", t(m, d).all, t(M::Naive, d).all);
        }
    }
}

#[test]
fn alias_on_this_modules_curves() {
    let ks = tones(24);
    let amp = 10f64.powf(-6.0 / 20.0);
    let r383 = kaiser_383();
    let curves: [(&str, ShapeCurve); 3] = [
        ("polynomial H2 -20 H3 -26 H4 -34 H5 -40", ShapeCurve::new(FAMILY_POLY, [0.1, 0.05, 0.02, 0.01], 0.0)),
        ("soft saturator, bias 30 %, H2 -30", ShapeCurve::new(FAMILY_SAT, [0.0316, 0.0, 0.0, 0.0], 0.12)),
        ("tape knee, bias 0", ShapeCurve::new(FAMILY_TAPE, [0.0; 4], 0.0)),
    ];
    println!("This module's curves, 24 tones, -6 dBFS input, drive 0 / +12 / +24 dB. dBc all / >4 kHz / in-band dB");
    for (cname, c) in curves.iter() {
        println!("{cname}");
        let mut rows = Vec::new();
        for m in [M::Naive, M::Adaa1, M::Adaa2, M::Os2, M::Os4] {
            let mut line = format!("  {:34}", name(m));
            let mut r3 = Vec::new();
            for g_db in [0.0, 12.0, 24.0] {
                let g = 10f64.powf(g_db / 20.0);
                let row = sweep(m, c, g, &ks, amp, &r383);
                line += &format!(" | {g_db:+3.0} dB {:6.1} {:6.1} {:6.1}", row.all, row.upper, row.inband);
                r3.push(row);
            }
            println!("{line}");
            rows.push(r3);
        }
        for d in 0..3 {
            // ADAA and ADAA2 below naive in the all-band mean, oversampling below both at heavy drive
            assert!(rows[1][d].all < rows[0][d].all + 0.5, "{cname}: ADAA vs naive at drive index {d}");
            assert!(rows[2][d].all < rows[0][d].all + 0.5, "{cname}: ADAA2 vs naive at drive index {d}");
        }
        assert!(rows[4][2].upper < rows[1][2].upper, "{cname}: 4x vs ADAA at +24 dB");
    }
}
