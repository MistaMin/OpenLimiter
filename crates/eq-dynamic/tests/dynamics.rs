//! Ported from OpenEQ 1.0.1 `RegressionTests.cpp`: checkDynamicsShortBlock, the dynamics parts
//! of checkRoadmap / checkPlanCoverage, checkTransientDetector, checkModernDynamics,
//! checkGlobalLookahead and the external sidechain case. Same signals, same limits.

use eq_dynamic::dynamics::{Dynamics, Mix, Settings};
use eq_dynamic::DynamicEq;
use olm_api::Module;

const SR: f64 = 48000.0;

fn module(settings: &[(&str, f32)]) -> DynamicEq {
    let mut m = DynamicEq::default();
    // OpenEQ's silenceBands: every band off, then band 1 a 0 dB bell at 1 kHz.
    for b in 1..=32 {
        set(&mut m, &format!("b{b}On"), 0.0);
    }
    for (id, v) in [("b1On", 1.0), ("b1Freq", 1000.0), ("b1Gain", 0.0), ("b1Q", 1.0)] {
        set(&mut m, id, v);
    }
    for (id, v) in settings {
        set(&mut m, id, *v);
    }
    m.prepare(SR as f32, 512);
    m
}

fn set(m: &mut DynamicEq, id: &str, v: f32) {
    let i = DynamicEq::param_index(id).unwrap_or_else(|| panic!("unknown {id}"));
    m.set_param(i, v);
}

fn tone(n: usize, f: f64, a: f32, phase: usize) -> Vec<f32> {
    (0..n).map(|i| (a as f64 * (2.0 * std::f64::consts::PI * f * (phase + i) as f64 / SR).sin()) as f32).collect()
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / x.len() as f64).sqrt()
}

fn run(m: &mut DynamicEq, l: &mut [f32], r: &mut [f32]) {
    m.process(l, r);
}

const DYN: [(&str, f32); 2] = [("b1Dyn", 1.0), ("b1Thresh", -24.0)];

#[test]
fn short_blocks_read_only_the_current_chunk() {
    let mut m = module(&[DYN[0], DYN[1], ("b1Range", 12.0), ("b1Attack", 1.0), ("b1Release", 8.0)]);
    let mut l = tone(512, 1000.0, 0.5, 0);
    let mut r = l.clone();
    run(&mut m, &mut l, &mut r);
    let hot = m.envelope_db(0);
    for _ in 0..50 {
        let (mut a, mut b) = (vec![0.0; 16], vec![0.0; 16]);
        run(&mut m, &mut a, &mut b);
        assert!(a.iter().all(|v| v.is_finite()));
    }
    let cold = m.envelope_db(0);
    println!("hot {hot:.1} cold {cold:.1}");
    assert!(hot > -30.0 && cold < hot - 12.0);
}

#[test]
fn anti_phase_reaches_the_detector_and_block_size_does_not_matter() {
    let mut m = module(&[DYN[0], DYN[1], ("b1Range", 12.0), ("b1Attack", 1.0)]);
    let mut l = tone(2048, 1000.0, 0.5, 0);
    let mut r: Vec<f32> = l.iter().map(|v| -v).collect();
    run(&mut m, &mut l, &mut r);
    assert!(m.envelope_db(0) > -30.0, "anti-phase cancelled the detector");

    let arm = || {
        let mut m = module(&[DYN[0], DYN[1], ("b1Range", 12.0), ("b1Attack", 1.0), ("b1Release", 8.0)]);
        let mut l = tone(512, 1000.0, 0.5, 0);
        let mut r = l.clone();
        run(&mut m, &mut l, &mut r);
        m
    };
    let mut wide = arm();
    let mut narrow = arm();
    let (mut a, mut b) = (vec![0.0; 800], vec![0.0; 800]);
    run(&mut wide, &mut a, &mut b);
    for _ in 0..50 {
        let (mut a, mut b) = (vec![0.0; 16], vec![0.0; 16]);
        run(&mut narrow, &mut a, &mut b);
    }
    println!("envelope 800 {:.2} 50x16 {:.2}", wide.envelope_db(0), narrow.envelope_db(0));
    assert!((wide.envelope_db(0) - narrow.envelope_db(0)).abs() < 1.5);
}

#[test]
fn lookahead_takes_a_fixed_20_ms() {
    let mut m = module(&[("b1Dyn", 1.0), ("b1Action", 1.0), ("b1Knee", 6.0), ("b1Relative", 50.0), ("b1Lookahead", 5.0)]);
    let mut l = tone(2048, 1000.0, 0.2, 0);
    let mut r = l.clone();
    run(&mut m, &mut l, &mut r);
    assert!(l.iter().all(|v| v.is_finite()));
    assert_eq!(m.latency(), 960);
    // Global lookahead: 20 ms even at 0 ms, and none once off (after the fade).
    let mut g = module(&[("lookaheadOn", 1.0), ("lookaheadMs", 0.0)]);
    assert_eq!(g.latency(), 960);
    set(&mut g, "lookaheadOn", 0.0);
    assert_eq!(g.latency(), 0);
    let x = tone(512, 1000.0, 0.1, 0);
    for _ in 0..4 {
        let (mut a, mut b) = (x.clone(), x.clone());
        run(&mut g, &mut a, &mut b);
    }
    let (mut a, mut b) = (x.clone(), x.clone());
    run(&mut g, &mut a, &mut b);
    let e = a.iter().zip(&x).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
    assert!(e < 1e-6, "after switching lookahead off the audio is not back in time ({e})");
}

fn settled(knee: f32, action: f32, relative: f32, amp: f32, thr: f32) -> f64 {
    let mut m = module(&[("b1Dyn", 1.0), ("b1Thresh", thr), ("b1Range", -12.0), ("b1Attack", 1.0), ("b1Release", 40.0), ("b1Knee", knee), ("b1Action", action), ("b1Relative", relative)]);
    let mut l = tone(8192, 1000.0, amp, 0);
    let mut r = l.clone();
    run(&mut m, &mut l, &mut r);
    rms(&l[6000..8000])
}

#[test]
fn knee_action_relative_and_silence_floor() {
    let hard = settled(0.0, 0.0, 0.0, 0.5, -6.0);
    let soft = settled(24.0, 0.0, 0.0, 0.5, -6.0);
    let above = settled(0.0, 0.0, 0.0, 0.5, -20.0);
    let below = settled(0.0, 1.0, 0.0, 0.5, -20.0);
    let absolute = settled(0.0, 0.0, 0.0, 0.5, -3.0);
    let relative = settled(0.0, 0.0, 100.0, 0.5, -3.0);
    println!("hard {hard:.4} soft {soft:.4} above {above:.4} below {below:.4} abs {absolute:.4} rel {relative:.4}");
    assert!(soft < hard * 0.9);
    assert!(above < below * 0.6);
    assert!(relative < absolute * 0.75);

    let mut m = module(&[("b1Dyn", 1.0), ("b1Thresh", -40.0), ("b1Range", 12.0), ("b1Action", 1.0), ("b1Attack", 1.0)]);
    let mut l = tone(8192, 1000.0, 1.0e-8, 0);
    let mut r = l.clone();
    run(&mut m, &mut l, &mut r);
    assert!(rms(&l[6000..8000]) < 1e-6, "silence floor let a quiet passage expand");
}

#[test]
fn band_lookahead_sees_the_burst_early() {
    let burst = |ahead: f32| {
        let mut m = module(&[DYN[0], DYN[1], ("b1Range", -12.0), ("b1Attack", 1.0), ("b1Lookahead", ahead)]);
        let (mut a, mut b) = (vec![0.0; 2048], vec![0.0; 2048]);
        run(&mut m, &mut a, &mut b);
        let mut l = tone(512, 1000.0, 0.5, 0);
        let mut r = l.clone();
        run(&mut m, &mut l, &mut r);
        (rms(&l), m.envelope_db(0))
    };
    let now = burst(0.0);
    let ahead = burst(20.0);
    println!("now {now:?} ahead {ahead:?}");
    assert!(now.0 > 0.05 && ahead.0 < 0.02 && ahead.1 > -18.0);
}

#[test]
fn transient_weight_and_automatic() {
    let onset = |w: f32| {
        let mut m = module(&[("b1Dyn", 1.0), ("b1Thresh", -30.0), ("b1Range", -12.0), ("b1Attack", 80.0), ("b1Release", 200.0), ("b1Transient", w)]);
        let (mut a, mut b) = (vec![0.0; 2048], vec![0.0; 2048]);
        run(&mut m, &mut a, &mut b);
        let mut l = tone(512, 1000.0, 0.5, 0);
        let mut r = l.clone();
        run(&mut m, &mut l, &mut r);
        rms(&l)
    };
    let tail = |w: f32| {
        let mut m = module(&[("b1Dyn", 1.0), ("b1Thresh", -30.0), ("b1Range", -12.0), ("b1Attack", 80.0), ("b1Release", 200.0), ("b1Transient", w)]);
        let mut l = tone(16384, 1000.0, 0.5, 0);
        let mut r = l.clone();
        run(&mut m, &mut l, &mut r);
        rms(&l[14336..16384])
    };
    let (so, fo, st, ft) = (onset(0.0), onset(100.0), tail(0.0), tail(100.0));
    println!("onset {so:.4}/{fo:.4} tail {st:.4}/{ft:.4}");
    assert!(fo < so * 0.75 && st < 0.16 && (ft - st).abs() < st * 0.05);

    let auto = |on: f32| {
        let mut m = module(&[DYN[0], DYN[1], ("b1Range", -12.0), ("b1Attack", 1.0), ("b1Release", 40.0), ("b1Auto", on)]);
        let mut l = tone(8192, 1000.0, 0.5, 0);
        let mut r = l.clone();
        run(&mut m, &mut l, &mut r);
        (rms(&l[6000..8000]), m.band_gain_db(0), m.threshold_db(0))
    };
    let manual = auto(0.0);
    let automatic = auto(1.0);
    println!("manual {manual:?} auto {automatic:?}");
    assert!(manual.0 < 0.15 && manual.1 < -8.0 && (manual.2 + 24.0).abs() < 0.2);
    assert!(automatic.0 > manual.0 * 2.0 && automatic.2 > -12.0);

    let mut m = module(&[("b1Dyn", 1.0), ("b1Range", -12.0), ("b1Auto", 1.0)]);
    let (mut a, mut b) = (vec![0.0; 2048], vec![0.0; 2048]);
    run(&mut m, &mut a, &mut b);
    let mut l = tone(2048, 1000.0, 0.5, 0);
    let mut r = l.clone();
    run(&mut m, &mut l, &mut r);
    let burst = rms(&l);
    assert!(burst < 0.22 && burst > 0.02, "auto burst {burst}");
}

#[test]
fn detector_link_frequency_and_q() {
    let case = |unlink: bool, df: f32, dq: f32, bf: f32| {
        let mut s = vec![("b1Freq", bf), ("b1Dyn", 1.0), ("b1Thresh", -30.0), ("b1Range", -12.0), ("b1Attack", 1.0), ("b1Release", 40.0)];
        if unlink {
            s.extend_from_slice(&[("b1DetLink", 0.0), ("b1DetFreq", df), ("b1DetQ", dq)]);
        }
        let mut m = module(&s);
        let mut l = tone(8192, 1000.0, 0.5, 0);
        let mut r = l.clone();
        run(&mut m, &mut l, &mut r);
        (rms(&l[6000..8000]), m.envelope_db(0), m.band_gain_db(0))
    };
    assert!(DynamicEq::default().band_state(0).dyn_det_link);
    let linked = case(false, 1000.0, 1.0, 1000.0);
    let detuned = case(true, 80.0, 16.0, 1000.0);
    let retarget = case(true, 1000.0, 1.0, 200.0);
    println!("linked {linked:?} detuned {detuned:?} retarget {retarget:?}");
    assert!(linked.1 > -18.0 && linked.2 < -8.0);
    assert!(detuned.1 < -40.0 && detuned.0 > 0.25 && detuned.2.abs() < 1.0);
    assert!(retarget.1 > -18.0 && retarget.2 < -8.0);
}

#[test]
fn external_key_opens_the_detector() {
    let s = [("b1Dyn", 1.0), ("b1Source", 5.0), ("b1Thresh", -30.0), ("b1Attack", 1.0)];
    let main = tone(9600, 1000.0, 0.2, 0);
    let key = tone(9600, 1000.0, 0.5, 0);
    let silent = vec![0.0f32; 9600];
    let mut hot = module(&s);
    let (mut l, mut r) = (main.clone(), main.clone());
    hot.process_keyed(&mut l, &mut r, &key, &key);
    let mut cold = module(&s);
    let (mut l, mut r) = (main.clone(), main.clone());
    cold.process_keyed(&mut l, &mut r, &silent, &silent);
    // Plain process: External falls back to the main input.
    let mut plain = module(&s);
    let (mut l, mut r) = (main.clone(), main.clone());
    plain.process(&mut l, &mut r);
    println!("key hot {:.1} cold {:.1} plain {:.1}", hot.envelope_db(0), cold.envelope_db(0), plain.envelope_db(0));
    assert!(hot.envelope_db(0) > -18.0 && cold.envelope_db(0) < -40.0);
    assert!((plain.envelope_db(0) - (20.0 * 0.2f32.log10() - 3.0)).abs() < 3.0);
    // Through the lookahead delay too.
    let mut ahead = module(&[s[0], s[1], s[2], s[3], ("b1Lookahead", 10.0)]);
    let (mut l, mut r) = (main.clone(), main.clone());
    ahead.process_keyed(&mut l, &mut r, &key, &key);
    assert!(ahead.envelope_db(0) > -18.0);
}

/// checkModernDynamics: the Below and Above sections on the engine itself.
#[test]
fn below_and_above_sections() {
    let run_sine = |e: &mut Dynamics, s: &Settings, amp: f32| {
        let mut off = 0.0;
        for b in 0..80 {
            let x = tone(512, 1000.0, amp, b * 512);
            off = e.advance(&x, &x, Mix::Energy, None, Mix::Energy, s);
        }
        off
    };
    let base = Settings { attack_ms: 1.0, release_ms: 10.0, modern: true, ..Settings::default() };
    let expected = |env: f32, thr: f32, above: bool, range: f32, ratio: f32| {
        let depth = if above { env - thr } else { thr - env };
        if depth <= 0.0 {
            return 0.0;
        }
        let m = range.abs().min(depth * ratio);
        if range < 0.0 {
            -m
        } else {
            m
        }
    };
    let fresh = || {
        let mut e = Dynamics::default();
        e.prepare(SR);
        e.set_trigger_filter(1000.0, 1.0);
        e
    };
    let mut e = fresh();
    let s = Settings { threshold_db: -20.0, above_on: true, above_range_db: 6.0, above_ratio: 0.5, ..base };
    let off = run_sine(&mut e, &s, 0.3);
    assert!(e.envelope_db() > -20.0 && (off - expected(e.envelope_db(), -20.0, true, 6.0, 0.5)).abs() < 0.01);
    let mut e = fresh();
    let s = Settings { threshold_db: -8.0, below_on: true, below_range_db: -6.0, below_ratio: 0.5, ..base };
    let off = run_sine(&mut e, &s, 0.3);
    let want = expected(e.envelope_db(), -8.0, false, -6.0, 0.5);
    assert!(want < 0.0 && (off - want).abs() < 0.01);
    let mut e = fresh();
    let s = Settings { threshold_db: -20.0, above_on: true, above_range_db: 30.0, above_ratio: 2.0, ..base };
    let off = run_sine(&mut e, &s, 0.3);
    assert!((off - expected(e.envelope_db(), -20.0, true, 30.0, 2.0)).abs() < 0.01);
    let mut e = fresh();
    let s = Settings { threshold_db: -20.0, below_on: true, below_range_db: 6.0, below_ratio: 1.0, ..base };
    assert!(run_sine(&mut e, &s, 0.3).abs() < 1e-6);
    let mut e = fresh();
    let s = Settings { threshold_db: -20.0, above_on: true, above_range_db: -4.0, above_ratio: 1.0, below_on: true, below_range_db: 12.0, below_ratio: 1.0, ..base };
    let off = run_sine(&mut e, &s, 0.3);
    assert!((off - expected(e.envelope_db(), -20.0, true, -4.0, 1.0)).abs() < 0.01);

    // Relative Free: a 1 kHz reference sits well below the broadband reference under a 100 Hz tone.
    let (mut whole, mut banded) = (fresh(), fresh());
    banded.set_relative_filter(1000.0, 1.0);
    let sf = Settings { relative_free: true, threshold_db: -20.0, ..base };
    let sw = Settings { relative_free: false, ..sf };
    for b in 0..80 {
        let x: Vec<f32> = (0..512)
            .map(|i| {
                let t = (b * 512 + i) as f64 / SR;
                (0.5 * (2.0 * std::f64::consts::PI * 100.0 * t).sin() + 0.05 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin()) as f32
            })
            .collect();
        whole.advance(&x, &x, Mix::Energy, None, Mix::Energy, &sw);
        banded.advance(&x, &x, Mix::Energy, None, Mix::Energy, &sf);
    }
    println!("reference whole {:.1} band {:.1}", whole.reference_db(), banded.reference_db());
    assert!(banded.reference_db() < whole.reference_db() - 12.0);

    // The single-action path and the Below section agree.
    let (mut legacy, mut modern) = (fresh(), fresh());
    let ls = Settings { modern: false, threshold_db: -20.0, action: 1, range_db: 9.0, ..base };
    let ms = Settings { modern: true, below_on: true, below_range_db: 9.0, below_ratio: 1.0, ..ls };
    let a = run_sine(&mut legacy, &ls, 0.05);
    let b = run_sine(&mut modern, &ms, 0.05);
    assert!(a > 0.5 && (a - b).abs() < 1e-4);
}

#[test]
fn view_meters_and_curves() {
    let mut m = module(&[DYN[0], ("b1Thresh", -30.0), ("b1Range", -12.0), ("b1Attack", 1.0)]);
    let mut l = tone(8192, 1000.0, 0.5, 0);
    let mut r = l.clone();
    run(&mut m, &mut l, &mut r);
    let mut v = [0.0f32; eq_dynamic::view::LEN];
    assert_eq!(m.view(&mut v), eq_dynamic::view::LEN);
    assert_eq!(v[eq_dynamic::view::GAIN_DB], m.band_gain_db(0));
    assert_eq!(v[eq_dynamic::view::ENVELOPE_DB], m.envelope_db(0));
    assert_eq!(v[eq_dynamic::view::THRESHOLD_DB], -30.0);
    assert!(m.band_gain_db(0) < -8.0);
    assert!((m.meters().reduction_db + m.band_gain_db(0)).abs() < 1e-6);
    let freqs = [100.0f32, 1000.0, 10000.0];
    let (mut stat, mut live, mut band) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
    m.response_db(&freqs, &mut stat, false);
    m.response_db(&freqs, &mut live, true);
    m.band_response_db(0, Some(m.band_gain_db(0)), &freqs, &mut band);
    println!("static {stat:?} live {live:?} band {band:?}");
    assert!(stat[1].abs() < 1e-3 && (live[1] - m.band_gain_db(0)).abs() < 0.01 && (band[1] - live[1]).abs() < 1e-4);
}
