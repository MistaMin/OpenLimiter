//! Harmonic levels per order, bit-exact bypass at 0 %, ADAA alias suppression, oversampling,
//! DC blocking and the rail.

mod common;
use common::*;
use comp_harmonic::harmonics::{harmonic_weights, rail_limit, shaper_naive, Harmonics};
use comp_harmonic::idx;

#[test]
fn zero_percent_is_bit_exact() {
    let x: Vec<f32> = noise(3, 1.5, 20000);
    let mut h = Harmonics::default();
    h.prepare(48000.0);
    for (i, &v) in x.iter().enumerate() {
        assert_eq!(h.process(v, 0.0, 0.0, 0.0, 0.0, i & 1).to_bits(), v.to_bits());
    }
    // Through the module at 1x with nothing compressing: the output is the input.
    let mut m = module(SR, 512, &neutral());
    let (a, b) = run(&mut m, &x, &x, 512);
    assert!(a == x && b == x);
}

#[test]
fn harmonic_levels_per_order() {
    // A 0.5 sine at 1 kHz, each order at 100 %; the harmonic amplitude of c_k x^k is c_k A^k / 2^(k-1).
    // First-order ADAA averages the residual over one sample: a sinc(f / fs) roll-off on it.
    let a = 0.5f64;
    let n = 48000;
    let x = sine(1000.0, a, SR, 2 * n);
    for (k, hidx) in [(2usize, idx::H2), (3, idx::H3), (4, idx::H4), (5, idx::H5)] {
        let mut m = module(SR, 512, &with(&neutral(), &[(hidx, 100.0)]));
        let (y, _) = run(&mut m, &x, &x, 512);
        let seg = &y[n..];
        let c = [0.5, 0.5, 0.25, 0.2][k - 2];
        let fk = 1000.0 * k as f64;
        let sinc = {
            let t = std::f64::consts::PI * fk / SR as f64;
            t.sin() / t
        };
        let expect = c * a.powi(k as i32) / 2f64.powi(k as i32 - 1) * sinc;
        let got = amplitude(seg, fk, SR);
        let others: Vec<f64> = (2..=7).filter(|&j| j != k).map(|j| amplitude(seg, 1000.0 * j as f64, SR)).collect();
        println!("H{k} 100 %: {:.2} dB re fundamental (expected {:.2}), other orders <= {:.1} dB", db(got / a), db(expect / a), db(others.iter().cloned().fold(0.0, f64::max) / a));
        // The sinc factor is an approximation of averaging along a straight segment: 3 % (0.26 dB).
        assert!((got / expect - 1.0).abs() < 0.03, "H{k}: {got} vs {expect}");
        // The order that is set dominates. Odd orders' lower images (x^5 -> 3rd, x^4 -> 2nd) are part
        // of the polynomial; the rail adds odd orders below -70 dB re the fundamental (1e-4).
        for (j, o) in (2..=7).filter(|&j| j != k).zip(&others) {
            let allowed = if k == 5 && j == 3 { 5.5 * got } else if k == 4 && j == 2 { 4.5 * got } else { (0.02 * got).max(1e-4) };
            assert!(*o < allowed, "H{k}: order {j} at {o}");
        }
    }
}

/// Energy in bins that are not harmonics of `f0` (aliases), relative to the fundamental, dB.
fn alias_db(y: &[f32], f0_bin: usize) -> f64 {
    let n = y.len();
    let fund = bin_power(y, f0_bin);
    let mut alias = 0.0;
    for b in 1..n / 2 {
        if b % f0_bin != 0 {
            alias += bin_power(y, b);
        }
    }
    10.0 * (alias / fund).log10()
}

#[test]
fn adaa_suppresses_aliasing() {
    // 7130 Hz at 44.1 kHz over 0.1 s (bins of 10 Hz): the 4th and 5th harmonics fold back onto
    // non-harmonic bins (1558 and 845 bins), as do the rail's higher orders.
    let sr = 44100.0f32;
    let n = 4410;
    let f0_bin = 713;
    let f0 = f0_bin as f64 * sr as f64 / n as f64;
    let x = sine(f0, 0.95, sr, 3 * n);
    let c = harmonic_weights(0.8, 1.0, 0.6, 1.0);
    // Naive: the same shaper and rail without anti-aliasing.
    let naive: Vec<f32> = x.iter().map(|&v| rail_limit(shaper_naive(v as f64, c)) as f32).collect();
    let mut h = Harmonics::default();
    h.prepare(sr as f64);
    let adaa: Vec<f32> = x.iter().map(|&v| h.process(v, 0.8, 1.0, 0.6, 1.0, 0)).collect();
    let a_naive = alias_db(&naive[2 * n..], f0_bin);
    let a_adaa = alias_db(&adaa[2 * n..], f0_bin);
    // Through the module at 1x and at 4x.
    let s = [(idx::RATIO, 1.0), (idx::THRESHOLD, 0.0), (idx::H2, 80.0), (idx::H3, 100.0), (idx::H4, 60.0), (idx::H5, 100.0)];
    let mut m1 = module(sr, 512, &with(&s, &[(idx::OS_MODE, 0.0)]));
    let (y1, _) = run(&mut m1, &x, &x, 512);
    let mut m4 = module(sr, 512, &with(&s, &[(idx::OS_MODE, 2.0), (idx::OS_FILTER, 1.0)]));
    let (y4, _) = run(&mut m4, &x, &x, 512);
    let a1 = alias_db(&y1[2 * n..], f0_bin);
    let a4 = alias_db(&y4[2 * n..], f0_bin);
    println!("alias energy re fundamental: naive {a_naive:.1} dB, ADAA {a_adaa:.1} dB, module 1x {a1:.1} dB, module 4x lin {a4:.1} dB");
    assert!(a_adaa < a_naive - 6.0, "ADAA {a_adaa} vs naive {a_naive}");
    assert!((a1 - a_adaa).abs() < 1.0);
    assert!(a4 < a1 - 20.0, "4x {a4} vs 1x {a1}");
}

#[test]
fn even_orders_are_dc_blocked_and_the_rail_holds() {
    let x = sine(100.0, 0.9, SR, 96000);
    let mut m = module(SR, 512, &with(&neutral(), &[(idx::H2, 100.0), (idx::H4, 100.0)]));
    let (y, _) = run(&mut m, &x, &x, 512);
    let mean: f64 = y[48000..].iter().map(|&v| v as f64).sum::<f64>() / 48000.0;
    assert!(mean.abs() < 1e-3, "DC {mean}");
    // The rail: |f(v)| < 1 whatever the input.
    for v in [0.5, 1.0, 2.0, 10.0, 1e3, -7.0] {
        assert!(rail_limit(v).abs() <= 1.0);
    }
    let big = sine(300.0, 8.0, SR, 48000);
    let mut h = Harmonics::default();
    h.prepare(48000.0);
    let peak = big.iter().map(|&v| h.process(v, 0.0, 1.0, 0.0, 0.0, 0).abs()).fold(0.0f32, f32::max);
    assert!(peak < 1.6, "peak {peak}"); // rail <= 1, plus the DC blocker's tiny overshoot
}
