//! Pass-through, click-free changes, robustness.

mod common;

use common::*;
use olm_api::Module;
use tone_six::{ToneSix, NPARAMS, PARAMS};

/// Default (flat) state is bit-exact, at 44.1, 48 and 96 kHz.
#[test]
fn defaults_bit_exact() {
    for &fs in &[44100.0f32, 48000.0, 96000.0] {
        let mut m = module(fs, &[]);
        let x = noise(96000, 7, 0.9);
        let y = noise(96000, 11, 0.9);
        let (mut l, mut r) = (x.clone(), y.clone());
        for (a, b) in l.chunks_mut(333).zip(r.chunks_mut(333)) {
            m.process(a, b);
        }
        assert!(l == x && r == y, "not bit-exact at {fs}");
    }
}

/// EQ off is bit-exact even with every band set; Mid/Side with flat bands is within 1e-6.
#[test]
fn eq_off_and_flat_mid_side() {
    let x = noise(48000, 3, 0.9);
    let y = noise(48000, 5, 0.9);
    let mut m = module(48000.0, &[("eqOn", 0.0), ("b3Gain", 12.0), ("lowCutOn", 1.0), ("b1Type", 8.0)]);
    let (mut l, mut r) = (x.clone(), y.clone());
    m.process(&mut l, &mut r);
    assert!(l == x && r == y);
    let mut m = module(48000.0, &[("matrix", 1.0)]);
    let (mut l, mut r) = (x.clone(), y.clone());
    m.process(&mut l, &mut r);
    let e = l.iter().zip(&x).chain(r.iter().zip(&y)).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    println!("flat Mid/Side round trip: max error {e:e}");
    assert!(e <= 1e-6);
}

/// Returning every band to flat goes back to exact pass-through.
#[test]
fn returns_to_exact_pass_through() {
    let mut m = module(48000.0, &[("b3Gain", 6.0)]);
    let x = noise(48000, 9, 0.5);
    let (mut l, mut r) = (x.clone(), x.clone());
    m.process(&mut l, &mut r);
    set(&mut m, "b3Gain", 0.0);
    let (mut l, mut r) = (x.clone(), x.clone());
    m.process(&mut l, &mut r); // smoothing back to 0 dB
    let (mut l, mut r) = (x.clone(), x.clone());
    m.process(&mut l, &mut r);
    assert!(l == x && r == x);
}

/// Runs `x` through `m`, applying `change` at sample `at`, and returns the output.
fn run_with_change(m: &mut ToneSix, x: &[f32], at: usize, change: &dyn Fn(&mut ToneSix)) -> Vec<f32> {
    let (mut l, mut r) = (x.to_vec(), x.to_vec());
    let mut i = 0;
    while i < x.len() {
        if i == at {
            change(m);
        }
        let e = (i + 64).min(x.len());
        m.process(&mut l[i..e], &mut r[i..e]);
        i = e;
    }
    l
}

/// Every kind of change on a running 200 Hz tone: the output's largest second difference stays
/// within 1.25 x the largest a clean tone at the highest level reached could have. A hard
/// coefficient or type switch would leave a step that breaks this by orders of magnitude.
#[test]
fn changes_are_click_free() {
    let fs = 48000.0;
    let f = 200.0;
    let a = 0.25;
    let x = sine(48000, f, fs as f64, a);
    let d2_unit = (2.0 * std::f64::consts::PI * f / fs as f64).powi(2);
    type Change = (&'static str, Vec<(&'static str, f32)>, Vec<(&'static str, f32)>, f64);
    let cases: Vec<Change> = vec![
        ("gain 0 -> +12", vec![("b3Freq", 200.0)], vec![("b3Gain", 12.0)], 12.0),
        ("gain +12 -> -12", vec![("b3Freq", 200.0), ("b3Gain", 12.0)], vec![("b3Gain", -12.0)], 12.0),
        ("freq 20 Hz -> 200 Hz at +12", vec![("b3Freq", 20.0), ("b3Gain", 12.0)], vec![("b3Freq", 200.0)], 12.0),
        ("width 0.1 -> 4 at +12", vec![("b3Freq", 200.0), ("b3Gain", 12.0), ("b3Width", 0.1)], vec![("b3Width", 4.0)], 12.0),
        ("bell +12 -> high-pass", vec![("b3Freq", 200.0), ("b3Gain", 12.0)], vec![("b3Type", 8.0)], 12.0),
        ("high-pass -> low-pass", vec![("b3Freq", 200.0), ("b3Type", 8.0)], vec![("b3Type", 9.0)], 0.0),
        ("band on -> off at +12", vec![("b3Freq", 200.0), ("b3Gain", 12.0)], vec![("b3On", 0.0)], 12.0),
        ("reject depth 0 -> -100", vec![("b3Type", 1.0), ("b3Freq", 200.0), ("b3Width", 0.5)], vec![("b3Depth", -100.0)], 0.0),
        ("shelf 3 dB/oct +12 -> 12 dB/oct", vec![("b1Freq", 300.0), ("b1Gain", 12.0)], vec![("b1Type", 4.0)], 12.6),
        ("low cut on at 200 Hz", vec![("lowCutFreq", 200.0)], vec![("lowCutOn", 1.0)], 0.0),
        ("matrix L/R -> M/S with band", vec![("b3Freq", 200.0), ("b3Gain", 12.0)], vec![("matrix", 1.0)], 12.0),
        ("unlink with different set 2", vec![("b3Freq", 200.0), ("b3Gain", 12.0), ("b3TypeCh2", 8.0), ("b3FreqCh2", 200.0)], vec![("linkSets", 0.0)], 12.0),
        ("EQ off", vec![("b3Freq", 200.0), ("b3Gain", 12.0)], vec![("eqOn", 0.0)], 12.0),
    ];
    println!("\n{:<34} {:>10} {:>10}", "change", "max d2", "limit");
    for (name, before, after, max_db) in cases {
        let mut m = module(fs, &before);
        let y = run_with_change(&mut m, &x, 24000, &|m| {
            for (id, v) in &after {
                set(m, id, *v);
            }
        });
        let limit = 1.25 * a * 10f64.powf(max_db / 20.0) * d2_unit;
        let got = max_d2(&y[100..]);
        println!("{name:<34} {got:>10.2e} {limit:>10.2e}");
        assert!(got <= limit, "{name}: {got} > {limit}");
    }
}

/// Unknown index, NaN and out-of-range values; unequal and empty slices; blocks longer than
/// `max_block`; every type at extreme settings stays finite and bounded (six +12 dB bands at
/// 20 kHz stack to +72 dB, so the noise is at -40 dBFS and the bound is +40 dBFS).
#[test]
fn robust() {
    let mut m = ToneSix::default();
    m.set_param(NPARAMS + 5, 1.0);
    m.set_param(0, f32::NAN);
    m.set_param(4, 1e9);
    m.set_param(4, -1e9);
    m.prepare(44100.0, 64);
    let (mut l, mut r) = (vec![0.1f32; 1000], vec![0.1f32; 10]);
    m.process(&mut l, &mut r);
    m.process(&mut [], &mut []);
    for p in PARAMS.iter() {
        assert!(p.default >= p.min && p.default <= p.max, "{}", p.id);
    }
    for t in 0..10 {
        let mut m = module(44100.0, &[]);
        for b in 1..=6 {
            set(&mut m, &format!("b{b}Type"), t as f32);
            set(&mut m, &format!("b{b}Freq"), 20000.0);
            set(&mut m, &format!("b{b}Gain"), 12.0);
            set(&mut m, &format!("b{b}Width"), if b % 2 == 0 { 4.0 } else { 0.02 });
            set(&mut m, &format!("b{b}Depth"), -100.0);
        }
        set(&mut m, "lowCutOn", 1.0);
        set(&mut m, "lowCutFreq", 200.0);
        let x = noise(44100, 1 + t, 0.01);
        let (mut l, mut r) = (x.clone(), x.clone());
        m.process(&mut l, &mut r); // 20 kHz everywhere, block of 44100 > max_block
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 1e2), "type {t}");
        // and a sweep of fast random changes on fresh input
        let (mut l, mut r) = (x.clone(), x.clone());
        for k in 0..200 {
            let idx = (k * 7919 + t as usize * 31) % NPARAMS;
            let p = PARAMS[idx];
            let v = p.min + (p.max - p.min) * ((k * 37 % 101) as f32 / 100.0);
            m.set_param(idx, v);
            let (a, b) = (&mut l[k * 200..k * 200 + 200], &mut r[k * 200..k * 200 + 200]);
            m.process(a, b);
        }
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 1e2), "type {t}");
    }
}

/// Parameter table: ids unique, names, ranges as documented in REPORT.md.
#[test]
fn parameter_table() {
    assert_eq!(PARAMS.len(), NPARAMS);
    assert_eq!(NPARAMS, 77);
    for (i, p) in PARAMS.iter().enumerate() {
        assert!(PARAMS.iter().skip(i + 1).all(|q| q.id != p.id), "duplicate {}", p.id);
    }
    for p in PARAMS.iter() {
        println!("{:<14} {:<34} {:>8} {:>8} {:>8} {:?}{}", p.id, p.name, p.min, p.max, p.default, p.kind, if p.log { " log" } else { "" });
    }
}
