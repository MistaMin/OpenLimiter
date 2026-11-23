//! The curves: antiderivatives are right, the generator is exact at the reference level, the
//! public transfer function is what the audio path runs.

mod common;

use common::*;
use harmonics::curve::*;
use harmonics::*;
use olm_api::Module;

fn curves() -> Vec<(String, ShapeCurve)> {
    let mut v = Vec::new();
    for fam in [FAMILY_POLY, FAMILY_SAT, FAMILY_TAPE] {
        for (rho, name) in [([0.0; 4], "plain"), ([0.1, 0.05, 0.03, 0.02], "gen"), ([0.25; 4], "max")] {
            for b in [-0.4, 0.0, 0.15, 0.4] {
                v.push((format!("family {fam} {name} bias {b}"), ShapeCurve::new(fam, rho, b)));
            }
        }
    }
    v
}

#[test]
fn antiderivatives_match_by_central_differences() {
    let mut worst: f64 = 0.0;
    for (name, c) in curves() {
        let mut u = -6.0;
        while u <= 6.0 {
            let h = 1e-5;
            let d1 = (c.r1(u + h) - c.r1(u - h)) / (2.0 * h);
            let d2 = (c.r2(u + h) - c.r2(u - h)) / (2.0 * h);
            let e1 = (d1 - c.r(u)).abs() / (1.0 + c.r(u).abs());
            let e2 = (d2 - c.r1(u)).abs() / (1.0 + c.r1(u).abs());
            worst = worst.max(e1).max(e2);
            assert!(e1 < 1e-7 && e2 < 1e-7, "{name} at u {u}: r1' err {e1:e}, r2' err {e2:e}");
            u += 0.0137;
        }
        assert_eq!(c.r(0.0), 0.0, "{name}");
        assert_eq!(c.r1(0.0), 0.0, "{name}");
        assert_eq!(c.r2(0.0), 0.0, "{name}");
        // unit slope at the origin
        let s = (c.phi(1e-6) - c.phi(-1e-6)) / 2e-6;
        assert!((s - 1.0).abs() < 1e-5, "{name}: slope {s}");
        // r is continuous everywhere and |r'| stays under the bound the safety checks use
        let mut u = -6.0;
        while u < 6.0 {
            let du = 1e-4;
            let slope = (c.r(u + du) - c.r(u)) / du;
            assert!(slope.abs() <= c.lip() * (1.0 + 1e-6) + 1e-6, "{name} at {u}: |r'| {slope} > lip {}", c.lip());
            u += du * 7.0;
        }
    }
    println!("worst relative antiderivative error {worst:e}");
}

#[test]
fn generator_is_exact_at_the_reference_level() {
    for (h, name) in [([-20.0, -30.0, -40.0, -50.0], "mixed"), ([-12.0, -80.0, -80.0, -80.0], "h2 max"), ([-80.0, -12.0, -80.0, -80.0], "h3 max"), ([-80.0, -80.0, -80.0, -12.0], "h5 max"), ([-80.0, -80.0, -25.0, -80.0], "h4")] {
        let shape = Shape { family: POLYNOMIAL, harmonics_db: h, drive_db: 0.0, bias_pct: 0.0 };
        let got = harmonic_levels_db(&shape, 1.0);
        for n in 0..4 {
            if h[n] > -80.0 {
                assert!((got[n] - h[n]).abs() < 1e-9, "{name}: H{} set {} got {}", n + 2, h[n], got[n]);
            } else {
                assert!(got[n] < -200.0 || got[n] == view::FLOOR_DB as f64, "{name}: H{} should be absent, got {}", n + 2, got[n]);
            }
        }
        // the generator follows drive: at -6 dB re the reference H2 falls 6 dB, H3 about 12
        let lower = harmonic_levels_db(&shape, 0.5);
        println!("{name}: at 1.0 {got:?}, at 0.5 {lower:?}");
    }
    // half amplitude: pure H2 falls by 6.02 dB exactly (2 u^2 has no other order)
    let s2 = Shape { family: POLYNOMIAL, harmonics_db: [-20.0, -80.0, -80.0, -80.0], drive_db: 0.0, bias_pct: 0.0 };
    let l = harmonic_levels_db(&s2, 0.5);
    assert!((l[0] - (-20.0 - 6.0206)).abs() < 1e-3, "{l:?}");
    // drive moves the reference: +6 dB drive at half amplitude is the reference again
    let s2d = Shape { drive_db: 20.0 * 2f64.log10(), ..s2 };
    assert!((harmonic_levels_db(&s2d, 0.5)[0] + 20.0).abs() < 1e-9);
}

#[test]
fn transfer_is_the_curve_the_audio_runs() {
    for fam in [POLYNOMIAL, SATURATOR, TAPE] {
        let mut m = module(&[(P_FAMILY, fam as f32), (P_ANTI_ALIAS, AA_NONE as f32), (P_DRIVE, 6.0), (P_BIAS, 40.0), (P_H2, -20.0), (P_H3, -25.0), (P_H5, -30.0)]);
        let shape = m.shape();
        // one sample after reset: no history, and the DC blocker passes a step unchanged, so
        // the output is exactly the static curve
        let x: Vec<f32> = (0..256).map(|i| -1.5 + 3.0 * i as f32 / 255.0).collect();
        let mut max_err: f64 = 0.0;
        for &xi in &x {
            m.reset();
            let mut l = vec![0.0f32; 1];
            let mut r = vec![0.0f32; 1];
            l[0] = xi;
            r[0] = xi;
            m.process(&mut l, &mut r);
            let want = transfer(&shape, xi as f64);
            max_err = max_err.max((l[0] as f64 - want).abs());
        }
        assert!(max_err < 1e-6, "family {fam}: max error {max_err:e}");
    }
}

#[test]
fn family_shapes_are_sane() {
    // saturator and tape are odd at zero bias, bounded, monotonic
    for fam in [SATURATOR, TAPE] {
        let c = ShapeCurve::new(fam, [0.0; 4], 0.0);
        let mut prev = c.phi(-20.0);
        let mut u = -20.0;
        while u < 20.0 {
            u += 0.01;
            let y = c.phi(u);
            assert!(y >= prev - 1e-12, "family {fam} not monotonic at {u}");
            assert!(y.abs() <= 1.0 + 1e-12);
            assert!((c.phi(-u) + y).abs() < 1e-12);
            prev = y;
        }
    }
    // tape is exactly linear below the knee, the saturator is not
    let t = ShapeCurve::new(TAPE, [0.0; 4], 0.0);
    assert_eq!(t.r(0.29), 0.0);
    let s = ShapeCurve::new(SATURATOR, [0.0; 4], 0.0);
    assert!(s.r(0.29) < -0.01);
    // bias: asymmetric, still zero output at zero input and unit slope
    let b = ShapeCurve::new(SATURATOR, [0.0; 4], 0.3);
    assert_eq!(b.phi(0.0), 0.0);
    assert!((b.phi(3.0) + b.phi(-3.0)).abs() > 0.1);
    let h = curve_harmonics(&b, 1.0);
    assert!(h[2] / h[1] > 0.02, "bias makes even harmonics: {h:?}");
}
