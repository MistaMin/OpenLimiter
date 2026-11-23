//! Spec 7.5 (Punch): aliasing floor of 5 and 10 kHz tones at +3 dB at 4x/8x/16x; exact sample
//! peak with truePeak off.

mod common;
use common::*;
use limiter_smart::idx;

const C_DB: f64 = -0.2;

/// Largest aliased (non-harmonic) component and the total aliased power, dBc, of a steady
/// tone at `f` (integer Hz, 1 s window: every component falls on a 1 Hz bin).
fn aliasing(y: &[f64], f: f64) -> (f64, f64, f64, f64, f64) {
    let a1 = dft_amp(y, f, SR);
    let mut worst = 0.0f64;
    let mut worst_in = 0.0f64;
    let mut worst_at = 0.0f64;
    let mut total = 0.0f64;
    let mut seen = Vec::new();
    for h in 2..=80 {
        let hf = h as f64 * f;
        if hf < SR / 2.0 {
            continue; // a true harmonic
        }
        let a = fold(hf, SR);
        // skip aliases that land on a harmonic (they cannot be told apart) and repeats
        if (a / f - (a / f).round()).abs() < 1e-9 || seen.iter().any(|s: &f64| (s - a).abs() < 0.5) || a < 1.0 {
            continue;
        }
        seen.push(a);
        let v = dft_amp(y, a, SR);
        if v > worst {
            worst = v;
            worst_at = a;
        }
        if a <= 20000.0 {
            worst_in = worst_in.max(v);
        }
        total += v * v;
    }
    (db(worst / a1), 10.0 * (total / (a1 * a1)).log10(), a1, db(worst_in / a1), worst_at)
}

#[test]
fn aliasing_floor() {
    let n = 48000 * 2;
    let mut rows = Vec::new();
    for f in [5000.0, 10000.0] {
        for os in 0..3 {
            for tp in [1.0f32, 0.0] {
                let mut m = make(SR, &[engine(3), (idx::DRIVE, 3.0), (idx::OVERSAMPLING, os as f32), (idx::TRUE_PEAK, tp)]);
                let x = sine(f, lin(C_DB), n, SR, 0.0);
                let (y, _) = run(&mut m, &x, &x);
                let seg = &y[48000 - 24000..48000 + 24000];
                let (worst, total, a1, inband, at) = aliasing(seg, f);
                println!(
                    "Punch {:5} Hz +3 dB {:>3}x truePeak {}: largest alias {:7.1} dBc at {:5.0} Hz, largest alias <= 20 kHz {:7.1} dBc, total aliased {:7.1} dBc (fundamental {:.2} dBFS)",
                    f,
                    [4, 8, 16][os],
                    if tp > 0.5 { "on " } else { "off" },
                    worst,
                    at,
                    inband,
                    total,
                    db(a1)
                );
                rows.push((f, os, tp, inband));
            }
        }
    }
    // more oversampling never aliases more (truePeak on, the default)
    for f in [5000.0, 10000.0] {
        let w: Vec<f64> = rows.iter().filter(|r| r.0 == f && r.2 > 0.5).map(|r| r.3).collect();
        assert!(w[1] <= w[0] + 1.0 && w[2] <= w[1] + 1.0, "{f} Hz: {w:?}");
    }
}

#[test]
fn sample_peak_is_exact_with_true_peak_off() {
    let mut worst = f64::NEG_INFINITY;
    for c in [-0.2f32, -3.0, -12.0] {
        for os in 0..3 {
            for (name, x) in [("noise", noise(48000, 0.5, 11)), ("square", (0..48000).map(|i| if (i / 37) % 2 == 0 { 0.9 } else { -0.9 }).collect::<Vec<f64>>()), ("sine 9 kHz", sine(9000.0, 0.9, 48000, SR, 0.1))] {
                let mut m = make(SR, &[engine(3), (idx::DRIVE, 18.0), (idx::CEILING, c), (idx::TRUE_PEAK, 0.0), (idx::OVERSAMPLING, os as f32)]);
                let (yl, yr) = run(&mut m, &x, &x.iter().map(|v| -0.5 * v).collect::<Vec<f64>>());
                let over = db(peak(&yl).max(peak(&yr))) - c as f64;
                worst = worst.max(over);
                println!("Punch truePeak off, ceiling {c:6} dB, {:>3}x, {name:10}: sample peak {:+.5} dB re ceiling", [4, 8, 16][os], over);
            }
        }
    }
    println!("worst sample peak re ceiling with truePeak off: {worst:+.6} dB");
    assert!(worst <= 1e-4);
}
