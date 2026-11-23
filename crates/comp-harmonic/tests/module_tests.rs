//! Parameter table, latency, clamping, block-size independence and finite output at every
//! parameter's min, max and default.

mod common;
use common::*;
use comp_harmonic::{idx, view, HarmonicCompressor, Mode};
use olm_api::{Kind, Module};

#[test]
fn parameter_table_matches_the_original_order() {
    let m = HarmonicCompressor::default();
    let ids: Vec<&str> = m.params().iter().map(|p| p.id).collect();
    assert_eq!(
        ids,
        [
            "algorithmMode", "threshold", "ratio", "attack", "release", "knee", "makeupGain", "mix", "harmonic2",
            "harmonic3", "harmonic4", "harmonic5", "scEnabled", "scListen", "scHPFreq", "autoRelease", "autoAttack",
            "inputGain", "outputGain", "oversampleMode", "oversampleFilter", "transientEnabled", "transientAttack",
            "transientSustain", "transientSpeed", "autoGain", "delta", "keySource",
        ]
    );
    assert_eq!(ids.len(), idx::COUNT);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(HarmonicCompressor::index_of(id), Some(i));
    }
    let p = m.params();
    assert_eq!(p[idx::MODE].kind, Kind::Choice(Mode::LABELS));
    assert_eq!(Mode::LABELS, &["Class-A Peak", "Hybrid FF/FB", "Diode Bridge", "Bus VCA", "Fast FET", "Optical", "RMS VCA"]);
    assert_eq!(p[idx::KEY_SOURCE].kind, Kind::Choice(&["Internal", "External"]));
    assert_eq!(p[idx::KEY_SOURCE].default, 0.0);
    let expect = [
        (idx::THRESHOLD, -60.0, 0.0, -18.0),
        (idx::RATIO, -2.0, 100.0, 4.0),
        (idx::ATTACK, 0.1, 300.0, 10.0),
        (idx::RELEASE, 5.0, 3000.0, 100.0),
        (idx::KNEE, 0.0, 24.0, 6.0),
        (idx::MAKEUP, -12.0, 24.0, 0.0),
        (idx::MIX, 0.0, 100.0, 100.0),
        (idx::SC_HP_FREQ, 20.0, 500.0, 150.0),
        (idx::INPUT_GAIN, -24.0, 24.0, 0.0),
        (idx::OUTPUT_GAIN, -24.0, 24.0, 0.0),
        (idx::OS_MODE, 0.0, 3.0, 1.0),
        (idx::OS_FILTER, 0.0, 1.0, 0.0),
        (idx::TRANSIENT_ATTACK, -100.0, 100.0, 0.0),
        (idx::TRANSIENT_SPEED, 0.5, 50.0, 10.0),
    ];
    for (i, lo, hi, d) in expect {
        assert_eq!((p[i].min, p[i].max, p[i].default), (lo, hi, d), "{}", p[i].id);
    }
    for i in idx::H2..=idx::H5 {
        assert_eq!((p[i].min, p[i].max, p[i].default), (0.0, 100.0, 0.0));
    }
    for i in [idx::SC_ENABLED, idx::SC_LISTEN, idx::AUTO_RELEASE, idx::AUTO_ATTACK, idx::TRANSIENT_ENABLED, idx::AUTO_GAIN, idx::DELTA] {
        assert_eq!(p[i].kind, Kind::Toggle);
        assert_eq!(p[i].default, 0.0);
    }
    // Defaults are held exactly.
    for (i, d) in p.iter().enumerate() {
        assert_eq!(m.param(i), d.default);
    }
}

#[test]
fn set_param_clamps_snaps_and_ignores_unknown() {
    let mut m = HarmonicCompressor::default();
    m.set_param(idx::THRESHOLD, -100.0);
    assert_eq!(m.param(idx::THRESHOLD), -60.0);
    m.set_param(idx::THRESHOLD, -18.04);
    assert!((m.param(idx::THRESHOLD) + 18.0).abs() < 1e-5);
    m.set_param(idx::MODE, 4.4);
    assert_eq!(m.param(idx::MODE), 4.0);
    m.set_param(idx::MIX, 1e9);
    assert_eq!(m.param(idx::MIX), 100.0);
    m.set_param(idx::RATIO, f32::NAN);
    assert_eq!(m.param(idx::RATIO), 4.0);
    m.set_param(999, 1.0);
    // Round trip of a default through set_param keeps it.
    for (i, p) in m.params().iter().enumerate() {
        let mut n = HarmonicCompressor::default();
        n.set_param(i, p.default);
        assert!((n.param(i) - p.default).abs() <= 1e-5 * p.default.abs().max(1.0), "{}", p.id);
    }
}

#[test]
fn latency_per_oversampling_setting() {
    let mut m = HarmonicCompressor::default();
    let mut table = Vec::new();
    for os in 0..4 {
        for f in 0..2 {
            m.set_param(idx::OS_MODE, os as f32);
            m.set_param(idx::OS_FILTER, f as f32);
            table.push((os, f, m.latency(), m.latency_fractional()));
        }
    }
    println!("oversampling, filter, latency, fractional: {table:?}");
    let expect = [0, 0, 3, 49, 4, 60, 5, 65];
    for (t, e) in table.iter().zip(expect) {
        assert_eq!(t.2, e, "{t:?}");
    }
    // The minimum-phase stages reproduce the original's reported fractional latencies.
    assert!((table[2].3 - 3.137).abs() < 0.001);
    assert!((table[4].3 - 4.433).abs() < 0.001);
    assert!((table[6].3 - 4.951).abs() < 0.001);
    assert_eq!(table[3].3, 49.0);
}

#[test]
fn impulse_arrives_at_the_reported_latency() {
    for os in 0..4 {
        for f in 0..2 {
            let mut m = module(SR, 512, &with(&neutral(), &[(idx::OS_MODE, os as f32), (idx::OS_FILTER, f as f32)]));
            let n = 2048;
            let mut x = vec![0.0f32; n];
            x[100] = 1.0;
            let (y, _) = run(&mut m, &x, &x, 512);
            let frac = m.latency_fractional();
            if f == 1 || os == 0 {
                // Linear phase: symmetric response, peak at the latency.
                let peak = y.iter().enumerate().fold((0, 0.0f32), |a, (i, &v)| if v.abs() > a.1 { (i, v.abs()) } else { a }).0;
                assert!(((peak as f64 - 100.0) - frac).abs() <= 0.5, "os {os} f {f}: peak {peak} latency {frac}");
            }
            // Group delay at low frequency from the response's centre of mass of the phase: use a 200 Hz tone.
            let tone = sine(200.0, 0.5, SR, 24000);
            let mut m = module(SR, 512, &with(&neutral(), &[(idx::OS_MODE, os as f32), (idx::OS_FILTER, f as f32)]));
            let (y, _) = run(&mut m, &tone, &tone, 512);
            let seg = &y[12000..24000];
            let refseg = &tone[12000..24000];
            // Phase of the output relative to the input at 200 Hz.
            let ph = |x: &[f32]| {
                let (mut re, mut im) = (0.0, 0.0);
                for (i, &v) in x.iter().enumerate() {
                    let w = 2.0 * std::f64::consts::PI * 200.0 * i as f64 / SR as f64;
                    re += v as f64 * w.cos();
                    im += v as f64 * w.sin();
                }
                im.atan2(re)
            };
            let d = (ph(seg) - ph(refseg)).rem_euclid(2.0 * std::f64::consts::PI);
            let delay = d / (2.0 * std::f64::consts::PI * 200.0) * SR as f64;
            let delay = if delay > SR as f64 / 400.0 { delay - SR as f64 / 200.0 } else { delay };
            assert!((delay - frac).abs() < 0.05, "os {os} f {f}: delay {delay} vs {frac}");
        }
    }
}

fn hot(n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut r = Rng(5);
    let l: Vec<f32> = (0..n).map(|i| (i as f32 * 0.07).sin() * 1.8 + r.next() * 0.5).collect();
    let rr: Vec<f32> = (0..n).map(|i| if (i / 2000) % 3 == 0 { 0.0 } else { (i as f32 * 0.31).sin() * 0.9 }).collect();
    (l, rr)
}

#[test]
fn every_parameter_at_min_max_default_is_finite() {
    let (l, r) = hot(9000);
    let p = HarmonicCompressor::default().params();
    for os in [0.0, 1.0, 3.0] {
        for filter in [0.0, 1.0] {
            for (i, d) in p.iter().enumerate() {
                for v in [d.min, d.max, d.default] {
                    for mode in [0.0, 2.0, 4.0, 5.0, 6.0] {
                        let mut m = module(SR, 512, &[(idx::OS_MODE, os), (idx::OS_FILTER, filter), (idx::MODE, mode), (idx::H3, 50.0), (i, v)]);
                        let (a, b) = run(&mut m, &l, &r, 512);
                        assert!(all_finite(&a) && all_finite(&b), "{} = {v} os {os} filter {filter} mode {mode}", d.id);
                        let mut vb = [0.0f32; view::LEN];
                        assert_eq!(m.view(&mut vb), view::LEN);
                        assert!(vb.iter().all(|x| x.is_finite()), "view {} = {v}", d.id);
                        assert!(m.meters().reduction_db.is_finite());
                    }
                }
            }
        }
    }
    // Everything at its min, then everything at its max, in every mode.
    for mode in 0..7 {
        for pick in [0, 1] {
            let mut m = HarmonicCompressor::default();
            for (i, d) in p.iter().enumerate() {
                m.set_param(i, if pick == 0 { d.min } else { d.max });
            }
            m.set_param(idx::MODE, mode as f32);
            m.set_param(idx::OS_MODE, 2.0);
            m.prepare(SR, 256);
            let (a, b) = run(&mut m, &l, &r, 256);
            assert!(all_finite(&a) && all_finite(&b), "all {} mode {mode}", if pick == 0 { "min" } else { "max" });
        }
    }
}

#[test]
fn block_size_does_not_change_the_output() {
    let (l, r) = hot(20000);
    let settings = [
        (idx::OS_MODE, 2.0),
        (idx::OS_FILTER, 1.0),
        (idx::THRESHOLD, -30.0),
        (idx::H2, 40.0),
        (idx::H5, 30.0),
        (idx::MIX, 70.0),
        (idx::TRANSIENT_ENABLED, 1.0),
        (idx::TRANSIENT_ATTACK, 50.0),
        (idx::DELTA, 1.0),
    ];
    // (Auto Gain is left out: as in the original, its ramp target is taken once per block, so
    // its output depends on where the host's blocks fall.)
    let mut m = module(SR, 512, &settings);
    let reference = run(&mut m, &l, &r, 512);
    for block in [1usize, 7, 64, 333] {
        let mut m = module(SR, 512, &settings);
        let out = run(&mut m, &l, &r, block);
        assert!(out == reference, "block {block}");
    }
    // A host call longer than max_block is split into max_block pieces.
    let mut m = module(SR, 512, &settings);
    let out = run(&mut m, &l, &r, 2000);
    assert!(out == reference);
}

#[test]
fn reset_restores_the_prepared_state() {
    let (l, r) = hot(6000);
    let s = [(idx::THRESHOLD, -30.0), (idx::H2, 40.0), (idx::MODE, 1.0)];
    let mut m = module(SR, 512, &s);
    let a = run(&mut m, &l, &r, 512);
    m.reset();
    let b = run(&mut m, &l, &r, 512);
    let mut fresh = module(SR, 512, &s);
    let c = run(&mut fresh, &l, &r, 512);
    assert!(a == c);
    assert!(b == c);
}

#[test]
fn oversampling_change_switches_latency_and_stays_clean() {
    let tone = sine(1000.0, 0.3, SR, 48000);
    let mut m = module(SR, 512, &neutral());
    let mut out = tone.clone();
    let mut right = tone.clone();
    for (b, chunk) in out.chunks_mut(512).zip(right.chunks_mut(512)).enumerate() {
        if b % 10 == 0 {
            m.set_param(idx::OS_MODE, ((b / 10) % 4) as f32);
            m.set_param(idx::OS_FILTER, ((b / 40) % 2) as f32);
        }
        m.process(chunk.0, chunk.1);
    }
    assert!(all_finite(&out));
    assert!(out.iter().all(|v| v.abs() < 0.6));
}
