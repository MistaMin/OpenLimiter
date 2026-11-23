//! Parameter changes do not click.
//!
//! Metric: the output through an 8th-order Butterworth high pass at 14 kHz (well above the
//! 1 kHz test tone's in-band harmonics). The peak of that signal in the 50 ms after a change
//! must stay within 6 dB of the larger steady-state peak before or after it, or under
//! -90 dBFS.

mod common;

use common::shipping::{Mode, Shipping};
use common::*;
use harmonics::adaa::Adaa1;
use harmonics::curve::{Curve, ShapeCurve, FAMILY_SAT};
use harmonics::filters::Biquad;
use harmonics::*;
use olm_api::Module;

fn hp(x: &[f64]) -> Vec<f64> {
    let mut f = [Biquad::default(); 4];
    for b in f.iter_mut() {
        b.set_butter(true, 14000.0, SR);
    }
    x.iter().map(|&v| f.iter_mut().fold(v, |a, b| b.run(a))).collect()
}

fn peak(x: &[f64]) -> f64 {
    x.iter().fold(0.0f64, |a, b| a.max(b.abs()))
}

fn dbfs(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

const T: usize = 24000;

/// (transition peak, steady peak) of the high-passed output, dBFS.
fn click(y: &[f64]) -> (f64, f64) {
    let h = hp(y);
    let before = peak(&h[T - 12000..T - 10]);
    let after = peak(&h[T + 7200..T + 19200]);
    let during = peak(&h[T - 10..T + 2400]);
    (dbfs(during), dbfs(before.max(after)))
}

fn run_change(base: &[(usize, f32)], change: (usize, f32), stereo_wide: bool) -> Vec<f64> {
    let mut m = module(base);
    let x = sine(T + 24000, 1000.0, 0.5, SR);
    let xr: Vec<f32> = if stereo_wide { sine(T + 24000, 1500.0, 0.3, SR) } else { x.clone() };
    let (mut l, mut r) = (x.clone(), xr);
    let mut out = Vec::with_capacity(l.len());
    let mut i = 0;
    while i < l.len() {
        let e = (i + 256).min(l.len());
        if i <= T && T < e {
            // change in the middle of a block: split the block there
            m.process(&mut l[i..T], &mut r[i..T]);
            m.set_param(change.0, change.1);
            m.process(&mut l[T..e], &mut r[T..e]);
        } else {
            m.process(&mut l[i..e], &mut r[i..e]);
        }
        out.extend(l[i..e].iter().map(|v| *v as f64));
        i = e;
    }
    out
}

#[test]
fn parameter_changes_do_not_click() {
    let poly = [(P_H2, -20.0f32), (P_H3, -26.0), (P_H4, -34.0), (P_H5, -40.0)];
    let cases: Vec<(&str, Vec<(usize, f32)>, (usize, f32), bool)> = vec![
        ("drive 0 -> +12 dB", poly.to_vec(), (P_DRIVE, 12.0), false),
        ("drive +12 -> -24 dB", [poly.to_vec(), vec![(P_DRIVE, 12.0)]].concat(), (P_DRIVE, -24.0), false),
        ("h2 off -> -12 dB", vec![], (P_H2, -12.0), false),
        ("h3 off -> -12 dB", vec![(P_H2, -80.0)], (P_H3, -12.0), false),
        ("h5 off -> -12 dB", vec![], (P_H5, -12.0), false),
        ("bias 0 -> 100 % (saturator, +12 dB)", vec![(P_FAMILY, 1.0), (P_DRIVE, 12.0)], (P_BIAS, 100.0), false),
        ("bias -100 -> 100 % (tape, +12 dB)", vec![(P_FAMILY, 2.0), (P_DRIVE, 12.0), (P_BIAS, -100.0)], (P_BIAS, 100.0), false),
        ("family poly -> saturator (+12 dB)", [poly.to_vec(), vec![(P_DRIVE, 12.0)]].concat(), (P_FAMILY, 1.0), false),
        ("family saturator -> tape (+12 dB)", vec![(P_FAMILY, 1.0), (P_DRIVE, 12.0)], (P_FAMILY, 2.0), false),
        ("channels stereo -> mid-side", poly.to_vec(), (P_CHANNELS, 1.0), true),
        ("mix 0 -> 100 % (saturator +12)", vec![(P_FAMILY, 1.0), (P_DRIVE, 12.0), (P_MIX, 0.0)], (P_MIX, 100.0), false),
        ("trim 0 -> -24 dB", poly.to_vec(), (P_TRIM, -24.0), false),
        ("auto gain off -> on (saturator +12)", vec![(P_FAMILY, 1.0), (P_DRIVE, 12.0)], (P_AUTO_GAIN, 1.0), false),
        ("source high cut off -> 1 kHz", poly.to_vec(), (P_SRC_HIGH, 1000.0), false),
        ("harmonics low cut off -> 3 kHz", poly.to_vec(), (P_HARM_LOW, 3000.0), false),
    ];
    let mut worst: f64 = -999.0;
    for aa in [AA_ADAA, AA_ADAA2, AA_OS4] {
        println!("anti-alias {}: HF peak during change / steady (dBFS)", ["ADAA", "ADAA2", "2x", "4x"][aa]);
        for (name, base, change, wide) in &cases {
            let mut b = base.clone();
            b.push((P_ANTI_ALIAS, aa as f32));
            let y = run_change(&b, *change, *wide);
            assert!(y.iter().all(|v| v.is_finite()));
            let (during, steady) = click(&y);
            println!("  {name:40} {during:7.1} / {steady:7.1}");
            worst = worst.max(during - steady.max(-90.0));
            assert!(during <= steady + 6.0 || during < -90.0, "{name} ({aa}): click {during:.1} dBFS over steady {steady:.1}");
        }
    }
    println!("worst transition over steady (or -90 dBFS): {worst:+.1} dB");
}

/// The curve-parameter handling compared on one engine: this crate re-evaluates both
/// antiderivatives with the current curve; the shipping method caches the previous one and
/// either uses it anyway (On) or switches to the plain value while the parameter moves (Hold).
#[test]
fn reevaluation_against_the_shipping_hold_switch() {
    let n = T + 24000;
    let x = sine(n, 1000.0, 1.0, SR);
    let ramp = |i: usize| -> f64 {
        // bias 0 -> 0.4 in 5 ms (the module's smoothing time) from T on
        let t = (i as f64 - T as f64) / (0.005 * SR);
        0.4 * t.clamp(0.0, 1.0)
    };
    let g = 4.0;
    // this crate
    let mut a = Adaa1::default();
    let mut x1 = 0.0;
    let mut ours = Vec::with_capacity(n);
    for i in 0..n {
        let c = ShapeCurve::new(FAMILY_SAT, [0.0; 4], ramp(i));
        let v = x[i] as f64;
        ours.push(x1 + a.step(&c, v, g));
        x1 = v;
    }
    // the shipping port, both ways (wet = curve output in input units)
    let ship = |hold: bool| -> Vec<f64> {
        let mut s = Shipping::default();
        let mut prev_b = 0.0;
        (0..n)
            .map(|i| {
                let b = ramp(i);
                let c = ShapeCurve::new(FAMILY_SAT, [0.0; 4], b);
                let u = g * x[i] as f64;
                let moving = (b - prev_b).abs() > 1e-6 * (1.0 + b.abs());
                prev_b = b;
                let mode = if hold && moving { Mode::Hold } else { Mode::On };
                s.step(u, c.phi(u), c.r1(u) + 0.5 * u * u, 1.0, mode) / g
            })
            .collect()
    };
    let on = ship(false);
    let hold = ship(true);
    println!("bias ramp 0 -> 0.4 in 5 ms, saturator, drive x4, 0 dBFS 1 kHz: HF peak during / steady (dBFS)");
    let mut res = Vec::new();
    for (name, y) in [("re-evaluated (this crate)", &ours), ("shipping, stale antiderivative", &on), ("shipping, Hold while moving", &hold)] {
        let (d, s) = click(y);
        println!("  {name:34} {d:7.1} / {s:7.1}");
        res.push((d, s));
    }
    assert!(res[0].0 <= res[0].1 + 6.0, "re-evaluated: {:?}", res[0]);
    assert!(res[0].0 < res[2].0 && res[0].0 < res[1].0, "{res:?}");
}
