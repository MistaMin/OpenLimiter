//! The `Module` contract of the api crate: parameter table, clamping, robustness.

mod common;
use common::*;
use limiter_wall::*;
use olm_api::{Kind, Module};

#[test]
fn parameter_table_is_sane() {
    let m = WallLimiter::default();
    let p = m.params();
    assert_eq!(p.len(), 11);
    for (i, d) in p.iter().enumerate() {
        assert!(d.min < d.max && d.default >= d.min && d.default <= d.max, "{}", d.id);
        assert!(p.iter().skip(i + 1).all(|o| o.id != d.id), "duplicate id {}", d.id);
        assert!(d.id.chars().next().unwrap().is_ascii_lowercase() && d.id.chars().all(|c| c.is_ascii_alphanumeric()));
        if let Kind::Choice(labels) = d.kind {
            assert_eq!(labels.len() as u32, d.steps);
        }
    }
    assert_eq!(p[P_STYLE].id, "style");
    assert_eq!(p[P_STRICT_TRUE_PEAK].id, "strictTruePeak");
}

#[test]
fn robust_against_misuse() {
    // process before prepare: leaves the audio alone, no panic
    let mut m = WallLimiter::default();
    let (mut l, mut r) = (vec![0.5f32; 64], vec![0.5f32; 64]);
    m.process(&mut l, &mut r);
    assert!(l.iter().all(|v| *v == 0.5));
    // unknown index ignored, out-of-range clamped, non-finite replaced
    m.set_param(99, 1.0);
    m.set_param(P_DRIVE, 99.0);
    m.set_param(P_CEILING, f32::NAN);
    m.set_param(P_STYLE, 7.0);
    m.prepare(48000.0, 64);
    assert_eq!(m.latency(), 64, "style clamps to the last choice");
    // non-finite and huge input: finite output under the ceiling
    let mut l: Vec<f32> = (0..4096).map(|i| match i % 7 { 0 => f32::NAN, 1 => f32::INFINITY, 2 => -1.0e30, _ => 0.3 }).collect();
    let mut r = l.clone();
    for (a, b) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
        m.process(a, b);
    }
    assert!(l.iter().chain(r.iter()).all(|v| v.is_finite() && v.abs() <= 1.0));
    // unequal slices and blocks longer than max_block do not panic
    let (mut a, mut b) = (vec![0.1f32; 1000], vec![0.1f32; 900]);
    m.process(&mut a, &mut b);
    // every sample rate from 8 kHz to 384 kHz prepares and runs
    for sr in [8000.0, 22050.0, 44100.0, 88200.0, 192000.0, 384000.0] {
        for style in [0.0, 1.0] {
            for strict in [0.0, 1.0] {
                let mut m = fresh(sr, &[(P_STYLE, style), (P_STRICT_TRUE_PEAK, strict), (P_DRIVE, 12.0)]);
                let (mut l, mut r) = (vec![0.9f32; 2048], vec![-0.9f32; 2048]);
                m.process(&mut l, &mut r);
                assert!(l.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
            }
        }
    }
}

#[test]
fn block_size_does_not_change_the_output() {
    let x = gated_noise(21, 1);
    for sets in [vec![(P_DRIVE, 12.0f32)], vec![(P_STYLE, 1.0), (P_THRESHOLD, -12.0)], vec![(P_DRIVE, 12.0), (P_STRICT_TRUE_PEAK, 1.0)]] {
        let mut a = fresh(48000.0, &sets);
        let mut b = fresh(48000.0, &sets);
        let mut la: Vec<f32> = x.iter().map(|v| *v as f32).collect();
        let mut ra = la.clone();
        let mut lb = la.clone();
        let mut rb = la.clone();
        a.process(&mut la, &mut ra);
        let mut i = 0;
        let mut size = 1;
        while i < lb.len() {
            let e = (i + size).min(lb.len());
            b.process(&mut lb[i..e], &mut rb[i..e]);
            i = e;
            size = size % 97 + 1;
        }
        assert!(la == lb && ra == rb);
    }
}

#[test]
fn reset_and_parameter_changes() {
    let x = gated_noise(22, 1);
    let mut m = fresh(48000.0, &[(P_DRIVE, 12.0)]);
    let a = run_mono(&mut m, &x);
    m.reset();
    assert_eq!(m.meters().reduction_db, 0.0);
    let b = run_mono(&mut m, &x);
    assert!(a == b, "reset restores the start state");
    // parameters set before prepare are kept by prepare; latency follows style and strict
    let mut m = WallLimiter::default();
    m.set_param(P_STYLE, 1.0);
    m.prepare(48000.0, 512);
    assert_eq!(m.latency(), 64);
    m.set_param(P_STYLE, 0.0);
    assert_eq!(m.latency(), 36);
    m.set_param(P_STRICT_TRUE_PEAK, 1.0);
    assert!(m.latency() > 36);
    // changing level parameters while running keeps the output under the ceiling in force
    let mut m = fresh(48000.0, &[(P_DRIVE, 6.0), (P_CEILING, -1.0)]);
    let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
    let mut r = l.clone();
    for (k, (ca, cb)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
        m.set_param(P_DRIVE, (k % 19) as f32);
        m.set_param(P_RELEASE, 1.0 + (k % 7) as f32 * 100.0);
        m.set_param(P_AUTO_RELEASE, (k % 2) as f32);
        m.set_param(P_PROFILE, (k % 4) as f32);
        m.process(ca, cb);
    }
    assert!(l.iter().all(|v| v.abs() <= lin(-1.0) as f32 * 1.000001));
}
