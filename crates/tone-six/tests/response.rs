//! Magnitude responses against the stored tables, bell centre gain, 96 kHz against 48 kHz,
//! minimum phase, channel routing. Run with `-- --nocapture` to see the tables.

mod common;
#[path = "data/reference.rs"]
mod reference;

use common::*;
use olm_api::Module;
use reference::{CASES, FREQS};
use tone_six::design::{self, Shape};

const FS: f64 = 48000.0;

/// Allowed error at one point: the case tolerance, plus 2.5 % of the level below -6 dB (deep
/// skirts of cuts and rejects are limited by the tone spacing and file resolution, F table
/// section 7 "The measured depth is limited by the tone spacing"). Points below -45 dB skipped.
fn allowed(tol: f64, measured: f64) -> Option<f64> {
    if measured < -45.0 {
        None
    } else if measured < -6.0 {
        Some(tol + 0.025 * measured.abs())
    } else {
        Some(tol)
    }
}

#[test]
fn stored_responses_48k() {
    let mut fails = Vec::new();
    println!("\n{:<40} {:>9} {:>9} {:>9} {:>7}  result", "case", "max err", "at Hz", "rms err", "tol");
    for c in CASES {
        let mut m = module(FS as f32, c.settings);
        let h = impulse(&mut m, 1 << 17);
        let (mut worst, mut at, mut sq, mut n, mut bad) = (0.0f64, 0.0, 0.0, 0, false);
        for (k, &f) in FREQS.iter().enumerate() {
            let meas = c.db[k];
            let Some(tol) = allowed(c.tol_db, meas) else { continue };
            let e = dft_db(&h, f, FS) - meas;
            sq += e * e;
            n += 1;
            if e.abs() > worst.abs() {
                worst = e;
                at = f;
            }
            if e.abs() > tol {
                bad = true;
            }
        }
        let rms = (sq / n.max(1) as f64).sqrt();
        println!("{:<40} {:>+9.3} {:>9.0} {:>9.3} {:>7.2}  {}", c.label, worst, at, rms, c.tol_db, if bad { "FAIL" } else { "pass" });
        if bad {
            fails.push(c.label);
        }
    }
    assert!(fails.is_empty(), "outside tolerance: {fails:?}");
}

/// Brief section 4.3: "the set gain appears at the set frequency within 0.05 dB", widths
/// 0.10 to 4.00 octaves. Checked at 44.1, 48 and 96 kHz on the designed coefficients and,
/// for a subset, through `process`.
#[test]
fn bell_centre_gain() {
    let mut worst = 0.0f64;
    for &fs in &[44100.0, 48000.0, 96000.0] {
        for i in 0..13 {
            let f0 = 20.0 * 1000f64.powf(i as f64 / 12.0);
            if f0 > 0.45 * fs {
                continue;
            }
            for &bw in &[0.10, 0.25, 0.5, 1.0, 2.0, 4.0] {
                for &g in &[-12.0, -6.0, -1.0, 0.5, 1.0, 6.0, 12.0] {
                    let c = design::band(Shape::Bell, fs, f0, g, bw, 0.0);
                    let e = c.magnitude_db(f0, fs) - g;
                    worst = worst.max(e.abs());
                    assert!(e.abs() <= 0.05, "fs {fs} f0 {f0} bw {bw} g {g}: {e}");
                }
            }
        }
    }
    println!("bell centre gain: worst error {worst:.2e} dB");
    // through the module
    for &(f0, bw, g) in &[(50.0, 0.1, 12.0), (500.0, 2.0, -12.0), (2000.0, 4.0, 6.0), (10000.0, 1.0, 12.0)] {
        let mut m = module(48000.0, &[("b3Type", 0.0), ("b3Freq", f0 as f32), ("b3Width", bw as f32), ("b3Gain", g as f32)]);
        let h = impulse(&mut m, 1 << 16);
        let e = dft_db(&h, f0, 48000.0) - g;
        assert!(e.abs() <= 0.05, "module f0 {f0}: {e}");
    }
}

/// The set frequency is the peak of a boost (F table section 7: "the set frequency is the
/// peak"). The magnitude-fitted design can move the true maximum a little off the centre for
/// wide bells high up (where the top is nearly flat); the test requires that nothing in the
/// band rises more than 0.05 dB above the set gain, the brief's tolerance (section 4.3).
#[test]
fn bell_peak_at_set_frequency() {
    let (mut worst_db, mut worst_oct) = (0.0f64, 0.0f64);
    for &f0 in &[30.0, 200.0, 1000.0, 3000.0, 5000.0, 10000.0] {
        for &bw in &[0.1, 1.0, 2.0, 4.0] {
            let c = design::band(Shape::Bell, 48000.0, f0, 12.0, bw, 0.0);
            let at = c.magnitude_db(f0, 48000.0);
            for k in -200..=200 {
                let d = 0.001 * k as f64;
                let excess = c.magnitude_db(f0 * 2f64.powf(d), 48000.0) - at;
                if excess > 1e-9 {
                    worst_db = worst_db.max(excess);
                    worst_oct = worst_oct.max(d.abs());
                }
            }
        }
    }
    println!("bell peak off centre: at most {worst_oct:.3} oct away and {worst_db:.4} dB above the centre gain");
    assert!(worst_db <= 0.05);
}

/// Own design target (the source's 96 kHz shapes were not measured, F table section 10 and 14):
/// a bell below 5 kHz gives the same response against analogue frequency at 96 kHz as at
/// 48 kHz. Limits: 0.25 dB up to 10 kHz, 0.45 dB up to 20 kHz (wide bells near the top of the
/// 48 kHz band cannot follow the 96 kHz shape exactly; see REPORT.md).
#[test]
fn bells_96k_equal_48k() {
    println!("\n96 kHz against 48 kHz, bells with f0 <= 5 kHz, worst |difference| in dB");
    println!("{:>6} {:>12} {:>12}", "width", "to 10 kHz", "to 20 kHz");
    let mut grand = (0.0f64, 0.0f64);
    for &bw in &[0.1, 0.5, 1.0, 2.0, 4.0] {
        let (mut w10, mut w20) = (0.0f64, 0.0f64);
        for i in 0..25 {
            let f0 = 20.0 * 250f64.powf(i as f64 / 24.0);
            for &g in &[-12.0, -6.0, 6.0, 12.0] {
                let a = design::band(Shape::Bell, 48000.0, f0, g, bw, 0.0);
                let b = design::band(Shape::Bell, 96000.0, f0, g, bw, 0.0);
                for &f in FREQS.iter() {
                    let d = (a.magnitude_db(f, 48000.0) - b.magnitude_db(f, 96000.0)).abs();
                    if f <= 10000.0 {
                        w10 = w10.max(d);
                    }
                    w20 = w20.max(d);
                }
            }
        }
        println!("{bw:>6.2} {w10:>12.3} {w20:>12.3}");
        grand = (grand.0.max(w10), grand.1.max(w20));
    }
    assert!(grand.0 <= 0.25 && grand.1 <= 0.45, "{grand:?}");
    // the same through `process` for one case
    let s = [("b4Type", 0.0), ("b4Freq", 3000.0), ("b4Width", 2.0), ("b4Gain", 12.0)];
    let h48 = impulse(&mut module(48000.0, &s), 1 << 16);
    let h96 = impulse(&mut module(96000.0, &s), 1 << 17);
    for &f in FREQS.iter() {
        let d = dft_db(&h48, f, 48000.0) - dft_db(&h96, f, 96000.0);
        assert!(d.abs() <= 0.25, "{f}: {d}");
    }
}

fn roots_radius(b: [f64; 3]) -> f64 {
    if b[2] == 0.0 || b[0] == 0.0 {
        return if b[0] != 0.0 { (b[1] / b[0]).abs() } else { 0.0 };
    }
    let disc = b[1] * b[1] - 4.0 * b[0] * b[2];
    if disc < 0.0 {
        (b[2] / b[0]).abs().sqrt()
    } else {
        let s = disc.sqrt();
        ((-b[1] + s) / (2.0 * b[0])).abs().max(((-b[1] - s) / (2.0 * b[0])).abs())
    }
}

/// Zero latency: every section is stable and minimum phase (zeros on or inside the unit disc).
#[test]
fn minimum_phase_and_stable() {
    let mut worst_zero = 0.0f64;
    let mut worst_pole = 0.0f64;
    for &fs in &[44100.0, 48000.0, 96000.0, 192000.0] {
        for t in 0..10 {
            for i in 0..19 {
                let f0 = 20.0 * 1000f64.powf(i as f64 / 18.0);
                for &g in &[-12.0, -3.0, 3.0, 12.0] {
                    for &bw in &[0.02, 0.1, 0.5, 1.0, 4.0] {
                        for &d in &[-100.0, -20.0, -3.0] {
                            let c = design::band(Shape::from_index(t), fs, f0, g, bw, d);
                            for k in 0..c.n {
                                worst_zero = worst_zero.max(roots_radius(c.s[k].b));
                                worst_pole = worst_pole.max(roots_radius(c.s[k].a));
                            }
                        }
                    }
                }
            }
        }
        for i in 0..11 {
            let s = design::low_cut(fs, 2.0 * 100f64.powf(i as f64 / 10.0));
            worst_pole = worst_pole.max(roots_radius(s.a));
            worst_zero = worst_zero.max(roots_radius(s.b));
        }
    }
    println!("largest zero radius {worst_zero:.9}, largest pole radius {worst_pole:.9}");
    assert!(worst_pole < 1.0);
    assert!(worst_zero <= 1.0 + 1e-6);
    assert_eq!(tone_six::ToneSix::default().latency(), 0);
}

/// F table section 9 (EQ paragraph): Mid/Side, unlinked, the second set acts on Side only:
/// a side-only tone drops by 12.0 dB, a left-only tone drops by 4.07 dB on L (its side half).
#[test]
fn mid_side_routing() {
    let fs = 48000.0;
    let s = [("matrix", 1.0), ("linkSets", 0.0), ("b3TypeCh2", 0.0), ("b3FreqCh2", 1000.0), ("b3GainCh2", -12.0), ("b3WidthCh2", 2.0)];
    let n = 48000;
    let x = sine(n, 1000.0, fs, 0.1);
    // left only
    let mut m = module(fs as f32, &s);
    let (mut l, mut r) = (x.clone(), vec![0.0f32; n]);
    m.process(&mut l, &mut r);
    let gl = tone_db(&l[24000..], 1000.0, fs) - tone_db(&x[24000..], 1000.0, fs);
    // anti-phase (side only)
    let mut m = module(fs as f32, &s);
    let (mut l_side, mut r_side) = (x.clone(), x.iter().map(|v| -v).collect::<Vec<_>>());
    m.process(&mut l_side, &mut r_side);
    let gs = tone_db(&l_side[24000..], 1000.0, fs) - tone_db(&x[24000..], 1000.0, fs);
    println!("M/S unlinked side band -12: left-only L {gl:.2} dB (stored -4.07), side-only {gs:.2} dB (stored -12.0)");
    // F table section 9: "-4.07 dB on L", "side-only part -12.0 dB"
    assert!((gl + 4.07).abs() < 0.02 && (gs + 12.0).abs() < 0.02);
    // the same settings linked: the second set is ignored
    let mut m = module(fs as f32, &[("matrix", 1.0), ("b3GainCh2", -12.0), ("b3FreqCh2", 1000.0)]);
    let (mut l3, mut r3) = (x.clone(), x.iter().map(|v| -v).collect::<Vec<_>>());
    m.process(&mut l3, &mut r3);
    let peak = l3.iter().zip(&x).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(peak < 1e-6, "linked: set 2 must be ignored ({peak})");
}

/// F table section 9 (round 3, HI1 R/S +12 dB at 4 kHz, Stereo): unlinked, the second set acts
/// on R alone; L is unchanged and the left-only part leaves R silent.
#[test]
fn stereo_routing() {
    let fs = 48000.0;
    let s = [("linkSets", 0.0), ("b5TypeCh2", 0.0), ("b5FreqCh2", 4000.0), ("b5GainCh2", 12.0)];
    let n = 48000;
    let x = sine(n, 4000.0, fs, 0.05);
    let mut m = module(fs as f32, &s);
    let (mut l, mut r) = (x.clone(), vec![0.0f32; n]);
    m.process(&mut l, &mut r);
    assert!(l.iter().zip(&x).all(|(a, b)| a == b), "L must be untouched");
    assert!(r.iter().all(|v| *v == 0.0), "R must stay silent");
    let mut m = module(fs as f32, &s);
    let (mut l, mut r) = (vec![0.0f32; n], x.clone());
    m.process(&mut l, &mut r);
    let g = tone_db(&r[24000..], 4000.0, fs) - tone_db(&x[24000..], 4000.0, fs);
    println!("stereo unlinked R band +12 at 4 kHz: R {g:.2} dB (stored +12.0)");
    assert!((g - 12.0).abs() < 0.02);
}

/// Brief section 4.3 / F table section 7: Butterworth cuts of about 12 dB/oct, the 3 dB/oct
/// shelf is a slope, the 12 dB/oct shelf dips by about 0.5 dB beside it.
#[test]
fn shape_landmarks() {
    let fs = 48000.0;
    let hp = design::band(Shape::HighPass, fs, 1000.0, 0.0, 1.0, 0.0);
    let slope = hp.magnitude_db(250.0, fs) - hp.magnitude_db(125.0, fs);
    assert!((slope - 12.04).abs() < 0.1, "high-pass slope {slope}");
    assert!((hp.magnitude_db(1000.0, fs) + 3.01).abs() < 0.01);
    let s12 = design::band(Shape::LowShelf12, fs, 50.0, 12.0, 1.0, 0.0);
    let dip = (100..200).map(|f| s12.magnitude_db(f as f64, fs)).fold(0.0, f64::min);
    assert!((dip + 0.55).abs() < 0.05, "dip {dip}");
    let s3 = design::band(Shape::LowShelf3, fs, 200.0, 12.0, 1.0, 0.0);
    // still rising 3.3 octaves below the set frequency: a slope, not a plateau
    assert!(s3.magnitude_db(20.0, fs) < 11.8 && s3.magnitude_db(40.0, fs) < s3.magnitude_db(20.0, fs) - 0.4);
    let lc = design::low_cut(fs, 100.0);
    let one = tone_six::design::Cascade { n: 1, s: [lc, design::IDENTITY] };
    assert!((one.magnitude_db(100.0, fs) + 3.01).abs() < 0.01);
}
