//! Oversampling filters, Mix and Delta nulls, Auto Gain, the external key, Sidechain Listen, the
//! transient designer and the view read-out.

mod common;
use common::*;
use comp_harmonic::dynamics::LoudnessMatcher;
use comp_harmonic::resampler::{design_fir_halfband, design_iir_halfband, stage_spec};
use comp_harmonic::{idx, view, HarmonicCompressor};
use olm_api::Module;
use std::f64::consts::PI;

fn iir_mag(c: &[f64], f: f64) -> f64 {
    // H(z) = (A0(z^2) + z^-1 A1(z^2)) / 2, sections (a + z^-2) / (1 + a z^-2).
    let w = 2.0 * PI * f;
    let cm = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let cd = |a: (f64, f64), b: (f64, f64)| {
        let d = b.0 * b.0 + b.1 * b.1;
        ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
    };
    let z2 = ((2.0 * w).cos(), -(2.0 * w).sin());
    let mut p0 = (1.0, 0.0);
    let mut p1 = (w.cos(), -w.sin());
    for (i, &a) in c.iter().enumerate() {
        let s = cd((a + z2.0, z2.1), (1.0 + a * z2.0, a * z2.1));
        if i % 2 == 0 {
            p0 = cm(p0, s);
        } else {
            p1 = cm(p1, s);
        }
    }
    (0.5 * ((p0.0 + p1.0).powi(2) + (p0.1 + p1.1).powi(2)).sqrt()).max(1e-30)
}

fn fir_mag(h: &[f64], f: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (j, &t) in h.iter().enumerate() {
        re += t * (2.0 * PI * f * j as f64).cos();
        im -= t * (2.0 * PI * f * j as f64).sin();
    }
    (re * re + im * im).sqrt().max(1e-30)
}

#[test]
fn half_band_stages_meet_their_specification() {
    for n in 0..3 {
        let (tu, au, td, ad) = stage_spec(n);
        for (tw, att, which) in [(tu, au, "up"), (td, ad, "down")] {
            let iir = design_iir_halfband(tw, att);
            let fir = design_fir_halfband(tw, att);
            let (pass, stop) = (0.25 - tw / 2.0, 0.25 + tw / 2.0);
            let grid = |a: f64, b: f64| (0..=400).map(move |i| a + (b - a) * i as f64 / 400.0);
            let worst_stop = |m: &dyn Fn(f64) -> f64| grid(stop, 0.5).map(|f| 20.0 * m(f).log10()).fold(f64::MIN, f64::max);
            let worst_pass = |m: &dyn Fn(f64) -> f64| grid(0.0, pass).map(|f| (20.0 * m(f).log10()).abs()).fold(0.0, f64::max);
            let (is, ip) = (worst_stop(&|f| iir_mag(&iir, f)), worst_pass(&|f| iir_mag(&iir, f)));
            let (fs, fp) = (worst_stop(&|f| fir_mag(&fir, f)), worst_pass(&|f| fir_mag(&fir, f)));
            println!(
                "stage {n} {which}: spec {att} dB, tw {tw}: IIR {} sections, stop {is:.1} dB, pass ripple {ip:.2e} dB; FIR {} taps, stop {fs:.1} dB, pass ripple {fp:.2e} dB",
                iir.len(),
                fir.len()
            );
            assert!(is <= att + 0.1, "IIR stop {is}");
            assert!(ip < 1e-3, "IIR pass {ip}");
            // Kaiser at the original-latency length: within 2 dB of the spec at the band edge.
            assert!(fs <= att + 2.0, "FIR stop {fs}");
            // Window design: pass ripple equals the stop-band ripple (0.015 dB at -55 dB).
            assert!(fp < 0.02, "FIR pass {fp}");
        }
    }
}

fn programme(n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut r = Rng(9);
    let l: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let env = 0.5 + 0.45 * (2.0 * std::f32::consts::PI * 1.3 * t).sin();
            env * (0.4 * (2.0 * std::f32::consts::PI * 180.0 * t).sin() + 0.2 * (2.0 * std::f32::consts::PI * 3300.0 * t).sin()) + 0.1 * r.next()
        })
        .collect();
    let rr: Vec<f32> = l.iter().enumerate().map(|(i, &v)| 0.8 * v + 0.05 * (i as f32 * 0.013).sin()).collect();
    (l, rr)
}

const OS_CONFIGS: [(f32, f32); 7] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (2.0, 0.0), (2.0, 1.0), (3.0, 0.0), (3.0, 1.0)];

#[test]
fn mix_nulls_in_every_oversampling_mode() {
    let (l, r) = programme(30000);
    for (os, f) in OS_CONFIGS {
        let base = [(idx::OS_MODE, os), (idx::OS_FILTER, f), (idx::RATIO, 1.0), (idx::THRESHOLD, 0.0)];
        // Neutral: wet and dry run through identical chains, so every mix is the same, bit for bit.
        let outs: Vec<(Vec<f32>, Vec<f32>)> = [0.0, 50.0, 100.0]
            .iter()
            .map(|&mix| {
                let mut m = module(SR, 512, &with(&base, &[(idx::MIX, mix)]));
                run(&mut m, &l, &r, 512)
            })
            .collect();
        assert!(outs[0] == outs[2] && outs[1] == outs[2], "os {os} filter {f}");
        // Compressing with harmonics at Mix 0 gives exactly the neutral (dry) output.
        let mut m = module(SR, 512, &with(&base, &[(idx::RATIO, 10.0), (idx::THRESHOLD, -30.0), (idx::H3, 80.0), (idx::MIX, 0.0)]));
        assert!(run(&mut m, &l, &r, 512) == outs[2], "mix 0 os {os}");
        // Mix 50 is the mean of wet and dry.
        let mut wet = module(SR, 512, &with(&base, &[(idx::RATIO, 10.0), (idx::THRESHOLD, -30.0), (idx::H3, 80.0)]));
        let w = run(&mut wet, &l, &r, 512);
        let mut half = module(SR, 512, &with(&base, &[(idx::RATIO, 10.0), (idx::THRESHOLD, -30.0), (idx::H3, 80.0), (idx::MIX, 50.0)]));
        let h = run(&mut half, &l, &r, 512);
        let e = h.0.iter().zip(&w.0).zip(&outs[2].0).map(|((&h, &w), &d)| (h - 0.5 * (w + d)).abs()).fold(0.0f32, f32::max);
        assert!(e < 1e-6, "mix 50 os {os}: {e}");
    }
}

#[test]
fn delta_nulls_and_equals_output_minus_dry() {
    let (l, r) = programme(30000);
    for (os, f) in OS_CONFIGS {
        let base = [(idx::OS_MODE, os), (idx::OS_FILTER, f), (idx::RATIO, 1.0), (idx::THRESHOLD, 0.0)];
        let mut m = module(SR, 512, &with(&base, &[(idx::DELTA, 1.0)]));
        let (a, b) = run(&mut m, &l, &r, 512);
        let peak = a.iter().chain(&b).fold(0.0f32, |p, v| p.max(v.abs()));
        assert!(peak == 0.0, "neutral delta os {os} filter {f}: {peak}");

        let comp = [(idx::RATIO, 8.0), (idx::THRESHOLD, -30.0), (idx::H2, 50.0), (idx::MAKEUP, 4.0)];
        let mut wet = module(SR, 512, &with(&base, &comp));
        let w = run(&mut wet, &l, &r, 512);
        let mut dry = module(SR, 512, &base);
        let d = run(&mut dry, &l, &r, 512);
        let mut dm = module(SR, 512, &with(&with(&base, &comp), &[(idx::DELTA, 1.0)]));
        let dl = run(&mut dm, &l, &r, 512);
        let e = dl.0.iter().zip(&w.0).zip(&d.0).map(|((&x, &w), &d)| (x - (w - d)).abs()).fold(0.0f32, f32::max);
        assert!(e < 1e-6, "delta os {os}: {e}");
        assert!(rms(&dl.0) > 0.01);
    }
    // Delta with Auto Gain on, neutral and with Input gain: the reference is the untouched input,
    // so Delta plays (input gain - 1) x input, level-matched by Auto Gain.
    // Auto Gain measures the untouched input against the (filter-delayed) output, so its gain is
    // a hair off unity and Delta is not an exact null; it stays below -80 dBFS.
    let mut m = module(SR, 512, &[(idx::OS_MODE, 1.0), (idx::RATIO, 1.0), (idx::THRESHOLD, 0.0), (idx::DELTA, 1.0), (idx::AUTO_GAIN, 1.0)]);
    let (a, _) = run(&mut m, &l, &r, 512);
    let peak = a.iter().fold(0.0f32, |p, v| p.max(v.abs()));
    println!("neutral Delta with Auto Gain at 2x: residual peak {:.1} dBFS", db(peak as f64));
    assert!(peak < 1e-4);
}

fn loudness_gap_db(reference: (&[f32], &[f32]), processed: (&[f32], &[f32])) -> f32 {
    let mut lm = LoudnessMatcher::default();
    lm.prepare(SR as f64);
    lm.process([reference.0, reference.1], [processed.0, processed.1], reference.0.len());
    lm.target_gain_db()
}

#[test]
fn auto_gain_matches_loudness() {
    let n = (SR * 8.0) as usize;
    let (l, r) = programme(n);
    let tail = n - (SR * 3.0) as usize;
    for (os, f) in [(0.0, 0.0), (1.0, 0.0), (2.0, 1.0)] {
        let s = [(idx::OS_MODE, os), (idx::OS_FILTER, f), (idx::THRESHOLD, -30.0), (idx::RATIO, 4.0), (idx::H3, 60.0), (idx::MODE, 3.0)];
        let mut off = module(SR, 512, &s);
        let o = run(&mut off, &l, &r, 512);
        let mut on = module(SR, 512, &with(&s, &[(idx::AUTO_GAIN, 1.0)]));
        let a = run(&mut on, &l, &r, 512);
        let mut vb = [0.0f32; view::LEN];
        on.view(&mut vb);
        let gap_off = loudness_gap_db((&l[tail..], &r[tail..]), (&o.0[tail..], &o.1[tail..]));
        let gap_on = loudness_gap_db((&l[tail..], &r[tail..]), (&a.0[tail..], &a.1[tail..]));
        println!("os {os} filter {f}: loudness gap without Auto Gain {gap_off:.2} dB, with {gap_on:.2} dB; applied {:.2} dB", vb[view::AUTO_GAIN_DB]);
        assert!(gap_off > 6.0);
        assert!(gap_on.abs() < 0.3, "{gap_on}");
        assert!((vb[view::AUTO_GAIN_DB] - gap_off).abs() < 0.5);
    }
    // Silence holds the last gain (the -70 LUFS gate).
    let s = [(idx::OS_MODE, 0.0), (idx::THRESHOLD, -30.0), (idx::RATIO, 4.0), (idx::AUTO_GAIN, 1.0)];
    let mut m = module(SR, 512, &s);
    run(&mut m, &l, &r, 512);
    let mut vb = [0.0f32; view::LEN];
    m.view(&mut vb);
    let z = vec![0.0f32; (SR * 2.0) as usize];
    // The first 100 ms chunk of the silence still holds the programme's tail.
    run(&mut m, &z[..(SR * 0.3) as usize], &z[..(SR * 0.3) as usize], 512);
    m.view(&mut vb);
    let before = vb[view::AUTO_GAIN_TARGET_DB];
    run(&mut m, &z, &z, 512);
    m.view(&mut vb);
    assert_eq!(vb[view::AUTO_GAIN_TARGET_DB], before);
    // and the applied gain has ramped onto that held target.
    assert!((vb[view::AUTO_GAIN_DB] - before).abs() < 1e-3);
}

#[test]
fn external_key_drives_the_detector() {
    let n = (SR * 1.0) as usize;
    // -50 dBFS: under every mode's knee (the FET and optical modes' detectors start well below
    // the threshold: at -30 dBFS the FET mode already reduces 3 dB with a -20 dB threshold).
    let quiet = sine(440.0, 0.003, SR, n);
    let loud = sine(97.0, 1.0, SR, n);
    for (os, f) in [(0.0, 0.0), (2.0, 1.0)] {
        for mode in 0..7 {
            let s = [(idx::OS_MODE, os), (idx::OS_FILTER, f), (idx::MODE, mode as f32), (idx::THRESHOLD, -20.0), (idx::RATIO, 10.0)];
            let gr = |m: &HarmonicCompressor| m.meters().reduction_db;
            let mut ext = module(SR, 512, &with(&s, &[(idx::KEY_SOURCE, 1.0)]));
            let (a, _) = run_keyed(&mut ext, &quiet, &quiet, &loud, &loud, 512);
            let mut int = module(SR, 512, &s);
            let (b, _) = run_keyed(&mut int, &quiet, &quiet, &loud, &loud, 512);
            let mut plain = module(SR, 512, &with(&s, &[(idx::KEY_SOURCE, 1.0)]));
            let (c, _) = run(&mut plain, &quiet, &quiet, 512);
            let mut vb = [0.0f32; view::LEN];
            ext.view(&mut vb);
            assert_eq!(vb[view::KEY_ACTIVE], 1.0);
            assert!(gr(&ext) > 5.0, "mode {mode}: keyed reduction {}", gr(&ext));
            assert!(rms(&a[n / 2..]) < 0.6 * rms(&quiet[n / 2..]));
            // Internal ignores the key; External without a key detects the input.
            assert!(gr(&int) < 1.0 && gr(&plain) < 1.0, "mode {mode}: {} {}", gr(&int), gr(&plain));
            assert!(b == c);
        }
    }
    // The key's high-pass: a 40 Hz key with the 500 Hz high-pass on barely reaches the detector.
    let low = sine(40.0, 1.0, SR, n);
    let s = [(idx::OS_MODE, 0.0), (idx::THRESHOLD, -20.0), (idx::RATIO, 10.0), (idx::KEY_SOURCE, 1.0), (idx::SC_ENABLED, 1.0), (idx::SC_HP_FREQ, 500.0)];
    let mut m = module(SR, 512, &s);
    run_keyed(&mut m, &quiet, &quiet, &low, &low, 512);
    assert!(m.meters().reduction_db < 1.0);
    // Once the key has carried signal, a silent key means no compression, even of a loud input.
    let mut m = module(SR, 512, &[(idx::OS_MODE, 0.0), (idx::THRESHOLD, -20.0), (idx::RATIO, 10.0), (idx::KEY_SOURCE, 1.0)]);
    let silent = vec![0.0f32; n];
    run_keyed(&mut m, &quiet, &quiet, &loud, &loud, 512);
    run_keyed(&mut m, &loud, &loud, &silent, &silent, 512);
    assert!(m.meters().reduction_db < 0.01);
}

#[test]
fn sidechain_listen_plays_the_detector_input() {
    let n = 24000;
    let x = sine(1000.0, 0.5, SR, n);
    let lowx = sine(50.0, 0.5, SR, n);
    let s = [(idx::OS_MODE, 0.0), (idx::SC_LISTEN, 1.0), (idx::SC_ENABLED, 1.0), (idx::SC_HP_FREQ, 400.0), (idx::H2, 100.0), (idx::MIX, 30.0)];
    let mut m = module(SR, 512, &s);
    let (a, _) = run(&mut m, &x, &x, 512);
    let mut m = module(SR, 512, &s);
    let (b, _) = run(&mut m, &lowx, &lowx, 512);
    // 1 kHz passes the 400 Hz high-pass; 50 Hz is down by about 36 dB; no harmonics, no mix.
    assert!((db(rms(&a[n / 2..]) / rms(&x[n / 2..]))).abs() < 0.5);
    assert!(db(rms(&b[n / 2..]) / rms(&lowx[n / 2..])) < -30.0);
    assert!(amplitude(&a[n / 2..], 2000.0, SR) < 1e-4);
}

#[test]
fn transient_designer_shapes_onsets_and_tails() {
    // Decaying 150 Hz hits every 250 ms.
    let n = (SR * 2.0) as usize;
    let x: Vec<f32> = (0..n)
        .map(|i| {
            let t = (i % 12000) as f32 / SR;
            0.7 * (-t / 0.06).exp() * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
        })
        .collect();
    let s = [(idx::OS_MODE, 0.0), (idx::RATIO, 1.0), (idx::TRANSIENT_ENABLED, 1.0)];
    let onset = |y: &[f32]| (1..8).map(|k| rms(&y[k * 12000..k * 12000 + 240])).sum::<f64>();
    let tail = |y: &[f32]| (1..8).map(|k| rms(&y[k * 12000 + 4800..k * 12000 + 9600])).sum::<f64>();
    let mut plain = module(SR, 512, &[(idx::OS_MODE, 0.0), (idx::RATIO, 1.0)]);
    let (p, _) = run(&mut plain, &x, &x, 512);
    let mut m = module(SR, 512, &with(&s, &[(idx::TRANSIENT_ATTACK, 100.0)]));
    let (a, _) = run(&mut m, &x, &x, 512);
    let mut m2 = module(SR, 512, &with(&s, &[(idx::TRANSIENT_SUSTAIN, -100.0)]));
    let (b, _) = run(&mut m2, &x, &x, 512);
    // A block holding an onset reports its largest transient gain.
    let (mut ol, mut or) = (x[..512].to_vec(), x[..512].to_vec());
    m.process(&mut ol, &mut or);
    let mut vb = [0.0f32; view::LEN];
    m.view(&mut vb);
    println!(
        "transient: attack +100 onset {:.2} dB; sustain -100 tail {:.2} dB",
        db(onset(&a) / onset(&p)),
        db(tail(&b) / tail(&p))
    );
    assert!(db(onset(&a) / onset(&p)) > 2.0);
    assert!(db(tail(&b) / tail(&p)) < -2.0);
    assert!(vb[view::TRANSIENT_EXTREME_DB].abs() > 1.0);
    // Enabled at 0 % is bit-exact unity.
    let mut z = module(SR, 512, &s);
    assert!(run(&mut z, &x, &x, 512).0 == p);
}

#[test]
fn view_reports_the_block() {
    let n = 48000;
    let x = sine(1000.0, 0.5, SR, n);
    let mut m = module(SR, 512, &[(idx::THRESHOLD, -24.0), (idx::RATIO, 4.0), (idx::KNEE, 0.0), (idx::MAKEUP, 2.0), (idx::OS_MODE, 2.0)]);
    let (y, _) = run(&mut m, &x, &x, 512);
    let mut vb = [0.0f32; view::LEN + 3];
    assert_eq!(m.view(&mut vb), view::LEN);
    let mut short = [0.0f32; 4];
    assert_eq!(m.view(&mut short), 4);
    // -6 dBFS peak into -24 dB at 4:1: up to 13.5 dB at the peaks, less as the 10 ms attack
    // smooths the rectified sine.
    assert!((vb[view::DETECTOR_DB] + 6.02).abs() < 0.1, "{}", vb[view::DETECTOR_DB]);
    assert!(m.meters().reduction_db > 8.0 && m.meters().reduction_db < 13.5, "{}", m.meters().reduction_db);
    assert!(vb[view::GAIN_REDUCTION_DB] < 0.0);
    assert_eq!(vb[view::DEEPEST_REDUCTION_DB], m.meters().reduction_db);
    // The live point on the static curve sits at the detector peak.
    assert!((vb[view::CURVE_OUTPUT_DB] - (-6.02 - 13.5 + 2.0)).abs() < 0.2, "{}", vb[view::CURVE_OUTPUT_DB]);
    assert_eq!(vb[view::MODE], 0.0);
    assert_eq!(vb[view::THRESHOLD_DB], -24.0);
    assert_eq!(vb[view::RATIO], 4.0);
    assert_eq!(vb[view::MAKEUP_DB], 2.0);
    assert_eq!(vb[view::OS_FACTOR], 4.0);
    assert!((vb[view::LATENCY_SAMPLES] - 4.433).abs() < 0.001);
    assert_eq!(vb[view::KEY_ACTIVE], 0.0);
    let peak_out = y[n / 2..].iter().fold(0.0f32, |p, v| p.max(v.abs()));
    assert!((vb[view::OUTPUT_PEAK_DB] - 20.0 * peak_out.log10()).abs() < 1.0);
    assert!((vb[view::INPUT_PEAK_DB] + 6.02).abs() < 0.01);
    // A mode without a knee reports knee 0 for the curve.
    m.set_param(idx::MODE, 2.0);
    m.set_param(idx::KNEE, 12.0);
    m.view(&mut vb);
    assert_eq!(vb[view::KNEE_DB], 0.0);
}
