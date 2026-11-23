//! The static curve per mode: against the steady state the module settles to, and its shape.

mod common;
use common::*;
use comp_harmonic::{curve, curve_params, idx, CurveParams, Mode};

/// Output dB the module settles to for a DC input of `level_db`.
fn settled_output_db(mode: Mode, settings: &[(usize, f32)], level_db: f32) -> f64 {
    let secs = if mode == Mode::Optical { 12.0 } else { 3.0 };
    let n = (SR * secs) as usize;
    let mut s = with(&neutral(), settings);
    s.push((idx::MODE, mode as usize as f32));
    let mut m = module(SR, 4096, &s);
    let x = vec![10.0f32.powf(level_db / 20.0); n];
    let (y, _) = run(&mut m, &x, &x, 4096);
    db(y[n - 1] as f64)
}

#[test]
fn static_curve_matches_the_settled_module() {
    let mut worst = 0.0f64;
    for mode in Mode::ALL {
        for (ratio, knee) in [(4.0, 6.0), (20.0, 12.0), (100.0, 0.0), (1.5, 24.0)] {
            let settings = [(idx::THRESHOLD, -24.0), (idx::RATIO, ratio), (idx::KNEE, knee), (idx::MAKEUP, 3.0)];
            let mut values = *comp_harmonic::HarmonicCompressor::default().values();
            for &(i, v) in &settings {
                values[i] = v;
            }
            values[idx::MODE] = mode as usize as f32;
            let (_, cp) = curve_params(&values);
            for level in [-50.0f32, -36.0, -27.0, -24.0, -21.0, -15.0, -9.0, -3.0, 0.0] {
                let measured = settled_output_db(mode, &settings, level);
                let predicted = curve::output_db(mode, &cp, level) as f64;
                let e = (measured - predicted).abs();
                worst = worst.max(e);
                assert!(e < 0.02, "{} ratio {ratio} knee {knee} at {level} dB: module {measured:.4}, curve {predicted:.4}", mode.label());
            }
        }
    }
    println!("largest static-curve error against the settled module: {worst:.5} dB");
}

fn p(ratio: f32, knee: f32) -> CurveParams {
    CurveParams { threshold_db: -30.0, ratio, knee_db: knee, makeup_db: 0.0 }
}

fn slope(mode: Mode, cp: &CurveParams, a: f32, b: f32) -> f64 {
    ((curve::output_db(mode, cp, b) - curve::output_db(mode, cp, a)) / (b - a)) as f64
}

#[test]
fn curve_shapes_per_mode() {
    // Class-A Peak: ratio above the knee, soft knee centred on the threshold, unity below.
    let c = p(4.0, 12.0);
    assert!((slope(Mode::ClassAPeak, &c, -20.0, -5.0) - 0.25).abs() < 1e-4);
    assert_eq!(curve::gain_db(Mode::ClassAPeak, &c, -40.0), 0.0);
    let at_thr = curve::gain_db(Mode::ClassAPeak, &c, -30.0) as f64;
    assert!((at_thr - (-0.75 * 36.0 / 24.0)).abs() < 1e-3, "{at_thr}"); // -slope * (K/2)^2 / (2K)
    // Hybrid: the 25 % feedback share lowers the effective ratio: b = -s*o / (1 + s/4).
    let s = 0.75;
    let eff = 1.0 - s / (1.0 + 0.25 * s);
    assert!((slope(Mode::HybridFfFb, &p(4.0, 0.0), -20.0, -5.0) - eff).abs() < 1e-3);
    // Diode Bridge: exactly the ratio.
    assert!((slope(Mode::DiodeBridge, &p(4.0, 0.0), -20.0, -5.0) - 0.25).abs() < 1e-4);
    // Bus VCA: the 8.89/9.53 tracking steepens it: 1 - (r - 1) / (t r).
    let t = 8.89f64 / 9.53;
    assert!((slope(Mode::BusVca, &p(4.0, 0.0), -20.0, -5.0) - (1.0 - 3.0 / (t * 4.0))).abs() < 1e-3);
    // RMS VCA: (1 - 1/r) * 3/2.98 above, a 1 % rise below.
    let k = 3.0f64 / 2.98;
    assert!((slope(Mode::RmsVca, &p(4.0, 0.0), -20.0, -5.0) - (1.0 - 0.75 * k)).abs() < 1e-3);
    assert!((curve::gain_db(Mode::RmsVca, &p(4.0, 0.0), -50.0) as f64 - 0.75 * k * 0.2).abs() < 1e-4);
    // Fast FET: reduction grows with level, limited by the JFET's maximum conductance (about -37.9 dB).
    let f = p(20.0, 0.0);
    let deep = curve::gain_db(Mode::FastFet, &f, 20.0);
    assert!(deep < -30.0 && deep > -38.0, "{deep}");
    assert!(curve::gain_db(Mode::FastFet, &f, -60.0) > -0.01);
    // Optical: the limit configuration (ratio >= 10) compresses harder than compress.
    let comp = curve::gain_db(Mode::Optical, &p(4.0, 0.0), -10.0);
    let lim = curve::gain_db(Mode::Optical, &p(10.0, 0.0), -10.0);
    assert!(comp < -1.0 && lim < comp, "{comp} {lim}");
    // Bus VCA's tracking (8.89/9.53) makes its slope 1 - (r - 1) / (t r) negative above a ratio of
    // about 14.9: the output falls as the input rises, until the 80 dB clamp. This is the original's.
    assert!(slope(Mode::BusVca, &p(100.0, 0.0), -25.0, -5.0) < 0.0);
    // The optical limit configuration's fixed junction share is partly feed-forward with the cell's
    // steep law, so it over-compresses at high drive (with threshold -24: -30.5 dB out at -10 dB in,
    // -33.7 dB at 0 dB in). Also the original's.
    let lim = p(100.0, 0.0);
    let lim = CurveParams { threshold_db: -24.0, ..lim };
    assert!(curve::output_db(Mode::Optical, &lim, 0.0) < curve::output_db(Mode::Optical, &lim, -10.0));
    // Every other curve is monotonic in level (output never falls as input rises) for ratios >= 1.
    for mode in Mode::ALL {
        for ratio in [1.0, 2.0, 8.0, 100.0] {
            if (mode == Mode::BusVca || mode == Mode::Optical) && ratio > 9.0 {
                continue;
            }
            let c = CurveParams { threshold_db: -24.0, ratio, knee_db: 10.0, makeup_db: 2.0 };
            let mut last = f32::NEG_INFINITY;
            for i in 0..=240 {
                let x = -60.0 + i as f32 * 0.25;
                let y = curve::output_db(mode, &c, x);
                assert!(y.is_finite());
                assert!(y >= last - 1e-3, "{} ratio {ratio} at {x}: {y} < {last}", mode.label());
                last = y;
            }
        }
    }
}
