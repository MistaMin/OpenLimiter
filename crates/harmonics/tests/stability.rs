//! Close samples, denormals, extreme and quiet inputs.

mod common;

use common::*;
use harmonics::adaa::{self, Adaa1, Adaa2};
use harmonics::curve::{Curve, ShapeCurve, FAMILY_POLY, FAMILY_SAT, FAMILY_TAPE};
use harmonics::*;

fn test_curves() -> Vec<(&'static str, ShapeCurve)> {
    vec![
        ("poly max", ShapeCurve::new(FAMILY_POLY, [0.25; 4], 0.0)),
        ("poly mild", ShapeCurve::new(FAMILY_POLY, [0.03, 0.01, 0.0, 0.0], 0.0)),
        ("sat bias", ShapeCurve::new(FAMILY_SAT, [0.03, 0.0, 0.0, 0.0], 0.3)),
        ("tape bias", ShapeCurve::new(FAMILY_TAPE, [0.0, 0.0, 0.01, 0.0], -0.4)),
    ]
}

/// Steps from 1e-13 to 1 around many operating points: the kernels stay within a bound of the
/// plain curve value that the step size explains (no 0/0, no cancellation spikes).
#[test]
fn close_samples_give_no_spikes() {
    let mut worst1: f64 = 0.0;
    let mut worst2: f64 = 0.0;
    for (name, c) in test_curves() {
        for g in [1.0, 16.0] {
            for centre in [-3.0, -1.0, -0.31, 0.0, 0.0001, 0.3, 0.9999, 1.0, 1.35, 2.0, 5.0] {
                for e in -13..=0 {
                    let step = 10f64.powi(e);
                    // three samples x2, x1, x0 around the centre
                    let xs = [centre / g, (centre + step) / g, (centre + 2.0 * step * 0.7) / g];
                    let mut a1 = Adaa1::default();
                    a1.no_blend = true;
                    let mut a2 = Adaa2::default();
                    a2.no_blend = true;
                    // prime both with the first two samples, read the third
                    let mut h1 = 0.0;
                    let mut h2 = 0.0;
                    for &x in &xs {
                        h1 = a1.step(&c, x, g);
                        h2 = a2.step(&c, x, g);
                    }
                    // ADAA2: the triangle mean over [x2, x0] centred on x1 lies within lip*span of r(u1)
                    let span = 2.0 * step;
                    let u1 = g * xs[1];
                    let r1 = c.r(u1) / g;
                    let bound = c.lip() * span / g + 1e-12;
                    let e2 = (h2 - r1).abs();
                    assert!(h2.is_finite() && e2 <= bound, "{name} g {g} centre {centre} step {step:e}: ADAA2 {h2} vs r {r1} (bound {bound:e})");
                    worst2 = worst2.max(e2 / bound);
                    // ADAA1 output is the all-passed mean (history starts at 0): finite
                    assert!(h1.is_finite(), "{name}: ADAA1 {h1}");
                    // the mean itself, directly
                    let m = adaa::mean1(&c, g * xs[1], g * xs[2]);
                    let mid = c.r(0.5 * g * (xs[1] + xs[2]));
                    let e1 = (m - mid).abs();
                    let b1 = 0.5 * c.lip() * (g * (xs[2] - xs[1])).abs() + 1e-12;
                    assert!(e1 <= b1, "{name}: mean1 {m} vs midpoint {mid}");
                    worst1 = worst1.max(e1 / b1);
                }
            }
        }
    }
    println!("worst |mean - r(mid)| / bound: ADAA1 {worst1:.3}, ADAA2 {worst2:.3}");
}

/// Constant input (every sample pair equal) gives exactly the plain curve value.
#[test]
fn constant_input_gives_the_plain_value() {
    for (name, c) in test_curves() {
        for x in [-2.0, -0.5, 0.0, 1e-9, 0.7, 3.0] {
            let mut a1 = Adaa1::default();
            let mut a2 = Adaa2::default();
            let (mut h1, mut h2) = (0.0, 0.0);
            for _ in 0..200 {
                h1 = a1.step(&c, x, 1.0);
                h2 = a2.step(&c, x, 1.0);
            }
            let want = adaa::naive(&c, x, 1.0);
            assert!((h1 - want).abs() < 1e-12, "{name} {x}: ADAA1 {h1} want {want}");
            assert!((h2 - want).abs() < 1e-12, "{name} {x}: ADAA2 {h2} want {want}");
        }
    }
}

#[test]
fn denormal_and_non_finite_inputs_stay_quiet() {
    for aa in [AA_ADAA, AA_ADAA2, AA_OS2, AA_OS4, AA_NONE] {
        for fam in [POLYNOMIAL, SATURATOR, TAPE] {
            let mut m = module(&[(P_ANTI_ALIAS, aa as f32), (P_FAMILY, fam as f32), (P_DRIVE, 24.0), (P_BIAS, 100.0), (P_H2, -12.0), (P_H5, -12.0), (P_AUTO_GAIN, 1.0)]);
            let mut x: Vec<f32> = (0..4800).map(|i| if i % 2 == 0 { 1e-40 } else { -3e-41 }).collect();
            x[100] = f32::NAN;
            x[200] = f32::INFINITY;
            x[300] = f32::MIN_POSITIVE;
            let y = run_mono(&mut m, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 1e-30), "mode {aa} family {fam}: {:?}", y.iter().fold(0.0f32, |a, b| a.max(b.abs())));
            // then music again: finite
            let s = sine(4800, 997.0, 0.9, SR);
            let y = run_mono(&mut m, &s);
            assert!(y.iter().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn extreme_inputs_stay_finite_and_bounded() {
    for aa in [AA_ADAA, AA_ADAA2, AA_OS2, AA_OS4, AA_NONE] {
        for fam in [POLYNOMIAL, SATURATOR, TAPE] {
            let mut m = module(&[(P_ANTI_ALIAS, aa as f32), (P_FAMILY, fam as f32), (P_DRIVE, 24.0), (P_H2, -12.0), (P_H3, -12.0), (P_H4, -12.0), (P_H5, -12.0), (P_BIAS, -100.0)]);
            let mut x = noise(9600, 7, 1.0);
            for (i, v) in x.iter_mut().enumerate() {
                *v *= [1.0, 1e3, 1e6, 1e-3][(i / 1200) % 4];
            }
            let y = run_mono(&mut m, &x);
            let peak_in = x.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            let peak = y.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            assert!(y.iter().all(|v| v.is_finite()), "mode {aa} family {fam}");
            // the generator is bounded (taper), the families are bounded: the output is the
            // input plus a bounded amount times 1/g, filtered
            assert!(peak <= peak_in * 1.01 + 50.0, "mode {aa} family {fam}: peak {peak}");
        }
    }
}

/// A 20 Hz sine from -120 to -60 dBFS: deviation from the naive curve (aligned by latency) is
/// what the page called the quiet-signal deviation (-114 dBFS for the shipping method).
#[test]
fn quiet_signals_match_the_plain_curve() {
    println!("worst deviation from naive, 20 Hz sine at <= -60 dBFS (dBFS):");
    for (fam, drive, label) in [(POLYNOMIAL, 0.0f32, "poly H2 -30 H3 -40, drive 0"), (SATURATOR, 0.0, "saturator, drive 0"), (SATURATOR, 24.0, "saturator, drive +24"), (TAPE, 24.0, "tape, drive +24")] {
        let mut line = format!("  {label:32}");
        for aa in [AA_ADAA, AA_ADAA2, AA_OS2, AA_OS4] {
            let mut worst: f64 = 0.0;
            for lvl in [-120.0, -100.0, -80.0, -60.0] {
                let amp = 10f64.powf(lvl / 20.0);
                let x = sine(48000, 20.0, amp, SR);
                let mut a = module(&[(P_FAMILY, fam as f32), (P_DRIVE, drive), (P_ANTI_ALIAS, aa as f32)]);
                let mut b = module(&[(P_FAMILY, fam as f32), (P_DRIVE, drive), (P_ANTI_ALIAS, AA_NONE as f32)]);
                let ya = run_mono(&mut a, &x);
                let yb = run_mono(&mut b, &x);
                let d = latency_of(aa);
                for i in 4800..48000 {
                    worst = worst.max((ya[i] as f64 - yb[i - d] as f64).abs());
                }
            }
            line += &format!(" {:>6} {:7.1}", ["adaa", "adaa2", "2x", "4x"][aa], 20.0 * worst.max(1e-300).log10());
        }
        println!("{line}");
    }
    // f32 output: anything at the f32 rounding of the signal itself is a pass
    let x = sine(48000, 20.0, 1e-3, SR);
    let mut a = module(&[(P_FAMILY, SATURATOR as f32), (P_DRIVE, 24.0)]);
    let mut b = module(&[(P_FAMILY, SATURATOR as f32), (P_DRIVE, 24.0), (P_ANTI_ALIAS, AA_NONE as f32)]);
    let ya = run_mono(&mut a, &x);
    let yb = run_mono(&mut b, &x);
    let worst = (4800..48000).map(|i| (ya[i] - yb[i - 1]).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-6, "saturator +24 dB, -60 dBFS 20 Hz: deviation {worst:e}");
}
