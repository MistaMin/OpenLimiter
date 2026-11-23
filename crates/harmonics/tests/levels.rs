//! Harmonic levels per order match the settings: FFT of sines through the whole module.

mod common;

use common::*;
use harmonics::*;

const N: usize = 16384;

/// Harmonic amplitudes 1..=5 (re the input amplitude) of a bin-exact sine through the module.
fn measure(settings: &[(usize, f32)], f: f64, amp: f64) -> ([f64; 6], f64) {
    let mut m = module(settings);
    let (k, f) = bin_freq(f, N, SR);
    let skip = 24000;
    let x = sine(skip + N, f, amp, SR);
    let y = run_mono(&mut m, &x);
    let seg: Vec<f64> = y[skip..].iter().map(|v| *v as f64).collect();
    let p = power(&seg);
    let mut h = [0.0; 6];
    for n in 1..6 {
        let b = n * k;
        if b < N / 2 {
            h[n] = p[b].sqrt() * 2.0 / N as f64 / amp;
        }
    }
    (h, f)
}

fn rel_db(h: &[f64; 6], n: usize) -> f64 {
    20.0 * (h[n] / h[1]).log10()
}

#[test]
fn polynomial_levels_match_the_settings_at_the_reference_level() {
    let set = [-20.0f32, -30.0, -40.0, -50.0];
    let base = [(P_H2, set[0]), (P_H3, set[1]), (P_H4, set[2]), (P_H5, set[3])];
    println!("mode       f0 Hz   H2      H3      H4      H5   (set -20 -30 -40 -50 dB re fundamental, 0 dBFS sine, drive 0)");
    for (aa, name, tol, f0) in [
        (AA_NONE, "none", 0.01, 500.0),
        (AA_OS4, "4x", 0.02, 500.0),
        (AA_OS2, "2x", 0.02, 500.0),
        (AA_ADAA, "adaa", 0.15, 500.0),
        (AA_ADAA2, "adaa2", 0.15, 500.0),
        (AA_NONE, "none", 0.01, 2000.0),
        (AA_OS4, "4x", 0.05, 2000.0),
        (AA_ADAA, "adaa", 1.5, 2000.0),
        (AA_ADAA2, "adaa2", 1.5, 2000.0),
    ] {
        let mut s = base.to_vec();
        s.push((P_ANTI_ALIAS, aa as f32));
        let (h, f) = measure(&s, f0, 1.0);
        let got: Vec<f64> = (2..6).map(|n| rel_db(&h, n)).collect();
        println!("{name:6} {f:8.1} {:7.2} {:7.2} {:7.2} {:7.2}", got[0], got[1], got[2], got[3]);
        for n in 0..4 {
            // the kernels' own top-end loss on harmonic n (ADAA: box, ADAA2: triangle)
            let fh = (n + 2) as f64 * f;
            let w = 2.0 * std::f64::consts::PI * fh / SR;
            let kernel = match aa {
                AA_ADAA => 20.0 * (w / 2.0).cos().log10(),
                AA_ADAA2 => 20.0 * ((2.0 + w.cos()) / 3.0).log10(),
                _ => 0.0,
            };
            let err = got[n] - (set[n] as f64 + kernel);
            assert!(err.abs() < tol, "{name} at {f} Hz: H{} {} want {} (+ kernel {kernel:.3})", n + 2, got[n], set[n]);
        }
    }
}

#[test]
fn each_order_is_independent_at_the_reference_level() {
    for order in 0..4 {
        let mut s = vec![(P_H2, -80.0), (P_H3, -80.0), (P_H4, -80.0), (P_H5, -80.0), (P_ANTI_ALIAS, AA_OS4 as f32)];
        s[order].1 = -24.0;
        let (h, _) = measure(&s, 500.0, 1.0);
        for n in 2..6 {
            let l = rel_db(&h, n);
            if n == order + 2 {
                assert!((l + 24.0).abs() < 0.02, "order {} alone: {l}", order + 2);
            } else {
                assert!(l < -90.0, "order {} alone leaks into H{n}: {l} dB", order + 2);
            }
        }
    }
}

#[test]
fn levels_below_the_reference_follow_the_curve_estimate() {
    // Below the reference level all orders fall and the higher orders fold into the lower
    // ones; the editor's estimate (harmonic_levels_db) must say so exactly.
    let shape = Shape { family: POLYNOMIAL, harmonics_db: [-20.0, -30.0, -40.0, -50.0], drive_db: 0.0, bias_pct: 0.0 };
    println!("peak dBFS  H2 meas/est    H3 meas/est    H4 meas/est    H5 meas/est   (4x)");
    for peak_db in [0.0, -6.0, -12.0, -20.0] {
        let amp = 10f64.powf(peak_db / 20.0);
        let (h, _) = measure(&[(P_H2, -20.0), (P_H3, -30.0), (P_H4, -40.0), (P_H5, -50.0), (P_ANTI_ALIAS, AA_OS4 as f32)], 500.0, amp);
        let est = harmonic_levels_db(&shape, amp);
        let line: Vec<String> = (0..4).map(|n| format!("{:6.1}/{:6.1}", rel_db(&h, n + 2), est[n])).collect();
        println!("{peak_db:6.1}    {}", line.join("  "));
        for n in 0..4 {
            if est[n] > -100.0 {
                assert!((rel_db(&h, n + 2) - est[n]).abs() < 0.1, "peak {peak_db}: H{} meas {} est {}", n + 2, rel_db(&h, n + 2), est[n]);
            }
        }
    }
}

#[test]
fn saturator_and_tape_levels_follow_the_curve_estimate() {
    for fam in [SATURATOR, TAPE] {
        for (drive, bias) in [(0.0f32, 0.0f32), (6.0, 30.0), (12.0, -50.0)] {
            let s = [(P_FAMILY, fam as f32), (P_DRIVE, drive), (P_BIAS, bias), (P_H2, -80.0), (P_H3, -80.0), (P_ANTI_ALIAS, AA_OS4 as f32)];
            let (h, _) = measure(&s, 300.0, 0.5);
            let shape = Shape { family: fam, harmonics_db: [-80.0; 4], drive_db: drive as f64, bias_pct: bias as f64 };
            let est = harmonic_levels_db(&shape, 0.5);
            for n in 0..4 {
                if est[n] > -90.0 {
                    assert!((rel_db(&h, n + 2) - est[n]).abs() < 0.15, "family {fam} drive {drive} bias {bias}: H{} meas {} est {}", n + 2, rel_db(&h, n + 2), est[n]);
                }
            }
            println!("family {fam} drive {drive:5.1} bias {bias:6.1}: H2..H5 est {:?}", est.map(|v| (v * 10.0).round() / 10.0));
        }
    }
}
