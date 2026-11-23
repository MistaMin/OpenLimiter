#![allow(unused_assignments)]
//! Zero-latency path, ported from OpenEQ 1.0.1 `RegressionTests.cpp`: checkRouting,
//! checkBlockHandling, checkSmoothing, checkSnapshotsAndResponse, checkNyquistAndTypes,
//! checkNonFiniteGuard, checkStereoPanAndMidSide, checkGlobalControls, checkOversampling,
//! checkUltraPrecision and parts of checkRoadmap / checkPlanCoverage.

mod common;
use common::*;
use eq_core::design::slope_norm_for_cut_db;
use olm_api::Module;

const PLUS12: f64 = 3.981071705534972;

fn ratio(band: &[(&str, f32)], side: bool, ch: usize) -> f64 {
    let mut m = module_with(SR, band, &[]);
    let l = tone(4800, 1000.0, 0.1, 0, SR);
    let r: Vec<f32> = if side { l.iter().map(|v| -v).collect() } else { l.clone() };
    let (mut a, mut b) = (l.clone(), r.clone());
    run(&mut m, &mut a, &mut b, 4800);
    assert!(a.iter().chain(&b).all(|v| v.is_finite()));
    let (o, i) = if ch == 0 { (&a, &l) } else { (&b, &r) };
    rms(&o[1440..]) / rms(&i[1440..])
}

#[test]
fn routing() {
    for ch in 0..2 {
        assert!(db(ratio(&bell(1000.0, 12.0, 1.0, 0.0), false, ch) / PLUS12).abs() < 0.4, "stereo ch {ch}");
    }
    // Left and Right placements leave the other channel bit-exact.
    for (placement, hit, other) in [(3.0, 0usize, 1usize), (4.0, 1, 0)] {
        let mut m = module_with(SR, &bell(1000.0, 12.0, 1.0, placement), &[]);
        let x = tone(4800, 1000.0, 0.1, 0, SR);
        let (mut a, mut b) = (x.clone(), x.clone());
        run(&mut m, &mut a, &mut b, 4800);
        let outs = [&a, &b];
        assert!(db(rms(&outs[hit][1440..]) / rms(&x[1440..]) / PLUS12).abs() < 0.4);
        assert_eq!(max_diff(outs[other], &x), 0.0, "placement {placement} touched the other channel");
    }
    assert!(db(ratio(&bell(1000.0, 12.0, 1.0, 1.0), false, 0) / PLUS12).abs() < 0.4, "mid on centre");
    assert!(db(ratio(&bell(1000.0, 12.0, 1.0, 2.0), true, 0) / PLUS12).abs() < 0.4, "side on side");
    for (placement, side) in [(1.0, true), (2.0, false)] {
        let mut m = module_with(SR, &bell(1000.0, 12.0, 1.0, placement), &[]);
        let l = tone(4800, 1000.0, 0.1, 0, SR);
        let r: Vec<f32> = if side { l.iter().map(|v| -v).collect() } else { l.clone() };
        let (mut a, mut b) = (l.clone(), r.clone());
        run(&mut m, &mut a, &mut b, 4800);
        assert!(max_diff(&a, &l).max(max_diff(&b, &r)) < 1e-5, "placement {placement} changed the other component");
    }
    // Solo excludes the unsoloed +18 dB band.
    let mut band = bell(1000.0, 0.0, 1.0, 0.0);
    band.extend_from_slice(&[("b1Solo", 1.0), ("b2On", 1.0), ("b2Freq", 1000.0), ("b2Gain", 18.0), ("b2Q", 1.0)]);
    let r = ratio(&band, false, 0);
    assert!(r < 1.25 && r > 0.8, "solo ratio {r}");
    // An inactive band is bit-exact.
    let mut b = bell(1000.0, 18.0, 1.0, 0.0);
    b[0].1 = 0.0;
    let mut m = module_with(SR, &b, &[]);
    let mut x = tone(2048, 1000.0, 0.2, 0, SR);
    x[3] = 0.8;
    let (mut a, mut c) = (x.clone(), x.clone());
    run(&mut m, &mut a, &mut c, 2048);
    assert!(a == x && c == x);
    // A 0 dB bell stays unity.
    let mut m = module_with(SR, &bell(1000.0, 0.0, 1.0, 0.0), &[]);
    let (mut a, mut c) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    a[0] = 1.0;
    c[0] = 1.0;
    m.process(&mut a, &mut c);
    assert!((a[0] - 1.0).abs() < 2e-4 && a[1..].iter().all(|v| v.abs() < 2e-4));
}

#[test]
fn block_size_independence() {
    let band = bell(1000.0, 6.0, 1.2, 0.0);
    let mut x = tone(4000, 1000.0, 0.15, 0, SR);
    x[10] = 0.55;
    let mut y = x.clone();
    y[400] = -0.4;
    let mut whole = module_with(SR, &band, &[]);
    let mut chunked = module_with(SR, &band, &[]);
    let (mut a, mut b) = (x.clone(), y.clone());
    run(&mut whole, &mut a, &mut b, 4000);
    let (mut c, mut d) = (x.clone(), y.clone());
    run(&mut chunked, &mut c, &mut d, 64);
    assert!(max_diff(&a, &c).max(max_diff(&b, &d)) < 1e-5);
    for size in [1usize, 7, 32, 511, 512, 513] {
        let mut m = module_with(SR, &band, &[]);
        let (mut a, mut b) = (tone(size, 1000.0, 0.2, 0, SR), tone(size, 1000.0, 0.2, 0, SR));
        m.process(&mut a, &mut b);
        assert!(a.iter().all(|v| v.is_finite()));
    }
    let mut m = module_with(SR, &band, &[]);
    m.process(&mut [], &mut []);
}

#[test]
fn smoothing_bypass_and_settled_designs() {
    let mut m = module_with(SR, &bell(1000.0, 0.0, 1.0, 0.0), &[]);
    let (mut s, mut t) = (vec![0.0; 256], vec![0.0; 256]);
    m.process(&mut s, &mut t);
    set(&mut m, "b1Gain", 12.0);
    let x = tone(5056, 1000.0, 0.1, 0, SR);
    let (mut a, mut b) = (x.clone(), x.clone());
    run(&mut m, &mut a, &mut b, 64);
    // After ~100 ms the boost is in; the first 64 samples are near the start of the ramp.
    let early = rms(&a[..64]) / rms(&x[..64]);
    let late = rms(&a[4000..]) / rms(&x[4000..]);
    println!("ramp early {:.2} dB late {:.2} dB", db(early), db(late));
    assert!(db(early) < 3.0 && (db(late) - 12.0).abs() < 0.3);
    let before = m.design_updates();
    for k in 0..8 {
        let (mut a, mut b) = (tone(256, 1000.0, 0.1, 5056 + k * 256, SR), tone(256, 1000.0, 0.1, 5056 + k * 256, SR));
        m.process(&mut a, &mut b);
    }
    assert_eq!(m.design_updates(), before, "settled gain kept redesigning");

    // Band bypass fades over 15 ms, then the band is bit-exact dry.
    let mut m = module_with(SR, &bell(1000.0, 12.0, 1.0, 0.0), &[]);
    let w = tone(2048, 1000.0, 0.1, 0, SR);
    let (mut a, mut b) = (w.clone(), w.clone());
    m.process(&mut a, &mut b);
    set(&mut m, "b1On", 0.0);
    let e = tone(64, 1000.0, 0.1, 2048, SR);
    let (mut a, mut b) = (e.clone(), e.clone());
    m.process(&mut a, &mut b);
    assert!(rms(&a) / rms(&e) > 2.5);
    let t = tone(4096, 1000.0, 0.1, 2112, SR);
    let (mut a, mut b) = (t.clone(), t.clone());
    m.process(&mut a, &mut b);
    assert!(max_diff(&a[2048..], &t[2048..]) < 1e-5);

    // Settled global bypass is bit-exact, also while editing.
    let mut m = module_with(SR, &bell(1000.0, 12.0, 1.0, 0.0), &[("bypass", 1.0)]);
    let x = tone(512, 1000.0, 0.15, 0, SR);
    let (mut a, mut b) = (x.clone(), x.clone());
    m.process(&mut a, &mut b);
    assert!(a == x && b == x);
    set(&mut m, "b1Freq", 880.0);
    let x = tone(256, 1000.0, 0.15, 512, SR);
    let (mut a, mut b) = (x.clone(), x.clone());
    m.process(&mut a, &mut b);
    assert!(a == x && b == x);
}

#[test]
fn processed_low_cut_matches_curve_and_butterworth() {
    let slope = slope_norm_for_cut_db(24.0) * 100.0;
    let band = [("b1On", 1.0), ("b1Type", 3.0), ("b1Freq", 1000.0), ("b1Slope", slope)];
    let mut m = module_with(SR, &band, &[]);
    let (mut l, mut r) = (vec![0.0f32; 32768], vec![0.0f32; 32768]);
    l[0] = 1.0;
    r[0] = 1.0;
    run(&mut m, &mut l, &mut r, 512);
    let freqs = [700.0f32, 1000.0, 1500.0];
    let mut curve = [0.0f32; 3];
    m.response_db(&freqs, &mut curve);
    for (k, &f) in freqs.iter().enumerate() {
        let ratio = (std::f64::consts::PI * f as f64 / SR).tan() / (std::f64::consts::PI * 1000.0 / SR).tan();
        let analytic = 1.0 / (1.0 + (1.0 / ratio).powf(8.0)).sqrt();
        let audio = impulse_mag(&l, f as f64, SR);
        let c = 10f64.powf(curve[k] as f64 / 20.0);
        assert!((audio - c).abs() < 0.01 && (audio - analytic).abs() < 0.01, "{f}: audio {audio} curve {c} analytic {analytic}");
    }
}

#[test]
fn every_type_finite_at_44k_and_96k_and_nyquist_clamp() {
    let mut m = module_with(22050.0, &bell(20000.0, 18.0, 40.0, 0.0), &[]);
    let (mut a, mut b) = (tone(1024, 1000.0, 0.2, 0, 22050.0), tone(1024, 1000.0, 0.2, 0, 22050.0));
    m.process(&mut a, &mut b);
    assert!(a.iter().all(|v| v.is_finite()));
    for fs in [44100.0, 96000.0] {
        for t in 0..48 {
            let band = [("b1On", 1.0), ("b1Type", t as f32), ("b1Freq", 1000.0), ("b1Gain", 6.0), ("b1Q", 2.0), ("b1Slope", 50.0)];
            let mut m = module_with(fs, &band, &[]);
            let (mut a, mut b) = (tone(256, 1000.0, 0.2, 0, fs), tone(256, 1000.0, 0.2, 0, fs));
            m.process(&mut a, &mut b);
            assert!(a.iter().chain(&b).all(|v| v.is_finite()), "type {t} at {fs}");
        }
    }
}

#[test]
fn non_finite_guard() {
    let mut m = module_with(SR, &[], &[]);
    for b in 1..=32 {
        for (f, v) in [("On", 1.0), ("Type", 0.0), ("Freq", 1000.0), ("Gain", 30.0), ("Q", 20.0)] {
            set(&mut m, &format!("b{b}{f}"), v);
        }
    }
    let x: Vec<f32> = (0..512).map(|i| if i % 5 == 0 { 0.5 } else { -0.5 }).collect();
    for _ in 0..200 {
        let (mut a, mut b) = (x.clone(), x.clone());
        m.process(&mut a, &mut b);
        assert!(a.iter().chain(&b).all(|v| v.is_finite()));
    }
}

#[test]
fn pan_and_mid_side() {
    let run_band = |placement: f32, pan: f32, li: f32, ri: f32| {
        let mut b = bell(1000.0, 12.0, 1.0, placement);
        b.push(("b1Pan", pan));
        let mut m = module_with(SR, &b, &[]);
        let (mut l, mut r) = (vec![], vec![]);
        for k in 0..60 {
            let s = tone(512, 1000.0, 1.0, k * 512, SR);
            l = s.iter().map(|v| v * li).collect();
            r = s.iter().map(|v| v * ri).collect();
            m.process(&mut l, &mut r);
        }
        (l, r)
    };
    let dry = 0.1 / 2f64.sqrt();
    let (l, r) = run_band(0.0, 100.0, 0.1, 0.1);
    assert!((rms(&l) - dry).abs() / dry < 0.03 && rms(&r) / dry > 3.5);
    let (sl, sr) = run_band(0.0, 0.0, 0.1, 0.03);
    let (ml, mr) = run_band(5.0, 0.0, 0.1, 0.03);
    assert!(max_diff(&sl, &ml).max(max_diff(&sr, &mr)) < 1e-3);
    let (l, _) = run_band(5.0, 100.0, 0.1, 0.1);
    assert!((rms(&l) - dry).abs() / dry < 0.03);
    let (l, _) = run_band(5.0, 100.0, 0.1, -0.1);
    assert!(rms(&l) / dry > 3.5);
}

#[test]
fn gain_scale_trim_side_balance_polarity() {
    let mut m = module_with(SR, &bell(1000.0, 12.0, 1.0, 0.0), &[("gainScale", 50.0)]);
    assert!((m.band_state(0).gain_db - 6.0).abs() < 1e-3);
    set(&mut m, "gainScale", 0.0);
    assert!(m.band_state(0).gain_db.abs() < 1e-6);
    let mut m = module_with(SR, &[], &[("inputGain", -6.0206)]);
    let (mut a, mut b) = (vec![], vec![]);
    for _ in 0..80 {
        a = vec![0.5f32; 512];
        b = vec![0.5f32; 512];
        m.process(&mut a, &mut b);
    }
    assert!((a[511] - 0.25).abs() < 0.01);
    let steady = |settings: &[(&str, f32)], l: f32, r: f32| {
        let mut m = module_with(SR, &[], settings);
        let (mut a, mut b) = (vec![], vec![]);
        for _ in 0..60 {
            a = vec![l; 512];
            b = vec![r; 512];
            m.process(&mut a, &mut b);
        }
        (a[511], b[511])
    };
    assert!((steady(&[("sideGain", -6.0206)], 1.0, -1.0).0 - 0.5).abs() < 0.01);
    assert!(steady(&[("sideGain", -100.0)], 1.0, -1.0).0.abs() < 0.01);
    let (l, r) = steady(&[("outputPan", 100.0)], 1.0, 1.0);
    assert!(l.abs() < 0.01 && (r - 1.0).abs() < 0.01);
    let (l, r) = steady(&[("outputPan", -50.0)], 1.0, 1.0);
    assert!((l - 1.0).abs() < 0.01 && (r - 0.5).abs() < 0.01);
    let (l, _) = steady(&[("stereoWidth", 0.0)], 1.0, -1.0);
    assert!(l.abs() < 0.01);
    let (l, _) = steady(&[("outputGain", 6.0206)], 0.25, 0.25);
    assert!((l - 0.5).abs() < 0.01);
    // Polarity inverts the output.
    let mut m = module_with(SR, &[], &[]);
    let (mut a, mut b) = (tone(4096, 1000.0, 0.25, 0, SR), tone(4096, 1000.0, 0.25, 0, SR));
    m.process(&mut a, &mut b);
    set(&mut m, "polarity", 1.0);
    let x = tone(512, 1000.0, 0.25, 4096, SR);
    let (mut a, mut b) = (x.clone(), x.clone());
    m.process(&mut a, &mut b);
    assert!(a.iter().zip(&x).all(|(p, q)| (p + q).abs() < 1e-4));
}

#[test]
fn auto_gain_and_tilt() {
    let tail = |auto: f32| {
        let mut m = module_with(SR, &bell(1000.0, 12.0, 1.0, 0.0), &[("autoGain", auto)]);
        let (mut a, mut b) = (tone(8192, 1000.0, 0.1, 0, SR), tone(8192, 1000.0, 0.1, 0, SR));
        run(&mut m, &mut a, &mut b, 512);
        let (mut a, mut b) = (tone(1024, 1000.0, 0.1, 8192, SR), tone(1024, 1000.0, 0.1, 8192, SR));
        m.process(&mut a, &mut b);
        rms(&a[256..])
    };
    let (plain, comp) = (tail(0.0), tail(1.0));
    assert!(comp < plain * 0.85 && comp > plain * 0.2);
    let tilt = |f: f64| {
        let band = [("b1On", 1.0), ("b1Type", 7.0), ("b1Freq", 1000.0), ("b1Gain", 6.0), ("b1Q", 0.7)];
        let mut m = module_with(SR, &band, &[]);
        let x = tone(4800, f, 0.1, 0, SR);
        let (mut a, mut b) = (x.clone(), x.clone());
        m.process(&mut a, &mut b);
        db(rms(&a[1440..]) / rms(&x[1440..]))
    };
    assert!(tilt(8000.0) > tilt(100.0) + 1.5);
}

#[test]
fn oversampling_keeps_level_and_reports_latency() {
    let render = |os: f32| {
        let mut m = module_with(SR, &bell(1000.0, 6.0, 1.0, 0.0), &[("oversample2x", os)]);
        let mut last = vec![];
        for k in 0..80 {
            let (mut a, mut b) = (tone(512, 1000.0, 0.1, k * 512, SR), tone(512, 1000.0, 0.1, k * 512, SR));
            m.process(&mut a, &mut b);
            last = a;
        }
        (rms(&last), m.latency())
    };
    let (off, l0) = render(0.0);
    let (on, l1) = render(1.0);
    println!("oversampling rms {off:.5} / {on:.5}, latency {l0} / {l1}");
    assert_eq!(l0, 0);
    assert!(l1 > 0);
    assert!((on / off - 1.0).abs() < 0.02 && (on / (0.1 / 2f64.sqrt()) - 2.0).abs() < 0.05);
    // The reported latency is the real delay: an impulse through a flat EQ peaks there.
    let mut m = module_with(SR, &[], &[("oversample2x", 1.0)]);
    let (mut a, mut b) = (vec![0.0f32; 2048], vec![0.0f32; 2048]);
    a[100] = 1.0;
    m.process(&mut a, &mut b);
    let p = peak_index(&a) as i64 - 100;
    println!("2x flat impulse delay {p}, reported {}", m.latency());
    assert!((p - m.latency() as i64).abs() <= 1);
}

#[test]
fn ultra_precision_and_denormals() {
    for t in [0.0f32, 3.0] {
        let render = |ultra: f32| {
            let band = [("b1On", 1.0), ("b1Type", t), ("b1Freq", 60.0), ("b1Gain", 6.0), ("b1Q", 6.0), ("b1Slope", 50.0)];
            let mut m = module_with(SR, &band, &[("ultraPrecision", ultra)]);
            let mut out = vec![];
            for k in 0..40 {
                let (mut a, mut b) = (tone(512, 137.0, 0.3, k * 512, SR), tone(512, 137.0, 0.3, k * 512, SR));
                m.process(&mut a, &mut b);
                out.extend(a);
            }
            out
        };
        let (a, b) = (render(0.0), render(1.0));
        assert!(b.iter().all(|v| v.is_finite()) && max_diff(&a, &b) < 1e-5);
    }
    let mut m = module_with(SR, &bell(1000.0, 6.0, 1.0, 0.0), &[]);
    let (mut a, mut b) = (tone(512, 1000.0, 1e-40, 0, SR), tone(512, 1000.0, 1e-40, 0, SR));
    m.process(&mut a, &mut b);
    assert!(a.iter().all(|v| v.is_finite() && v.abs() < 1e-20));
}

#[test]
fn drive_reports_latency_and_saturates_gently() {
    let mut m = module_with(SR, &[], &[("drive", 100.0)]);
    assert!(m.latency() > 0);
    let x = tone(9600, 1000.0, 0.1, 0, SR);
    let (mut a, mut b) = (x.clone(), x.clone());
    run(&mut m, &mut a, &mut b, 512);
    let change = db(rms(&a[4800..]) / rms(&x[4800..]));
    println!("full drive on a -20 dBFS sine: {change:.2} dB");
    assert!(change < -0.5 && change > -4.0 && a.iter().all(|v| v.is_finite()));
}

#[test]
fn curves_agree_with_the_designs() {
    let mut b = bell(1000.0, 12.0, 1.0, 3.0);
    b.extend_from_slice(&[("b2On", 1.0), ("b2Type", 1.0), ("b2Freq", 200.0), ("b2Gain", -6.0)]);
    let m = module_with(SR, &b, &[]);
    let freqs = [100.0f32, 1000.0, 5000.0];
    let (mut total, mut l, mut r, mut b1) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
    m.response_db(&freqs, &mut total);
    m.response_channels_db(&freqs, &mut l, &mut r);
    m.band_response_db(0, None, &freqs, &mut b1);
    println!("total {total:?} left {l:?} right {r:?} band1 {b1:?}");
    assert!((b1[1] - 12.0).abs() < 0.01 && (l[1] - total[1]).abs() < 0.01 && l[1] > r[1] + 10.0);
    let mut o = [0.0f32; 3];
    m.band_response_db(0, Some(-3.0), &freqs, &mut o);
    assert!((o[1] + 3.0).abs() < 0.01);
}
