//! The editor read-out and the public curve: they describe the audio path without changing it.

use olm_api::Module;
use shaper::*;

fn make(curve: usize, thr: f32, drive: f32) -> Shaper {
    let mut s = Shaper::default();
    s.prepare(48_000.0, 512);
    s.set_param(P_CURVE, curve as f32);
    s.set_param(P_THRESHOLD, thr);
    s.set_param(P_DRIVE, drive);
    s.set_param(P_OVERSAMPLE, 0.0);
    s.reset();
    s
}

#[test]
fn transfer_is_the_curve_the_audio_runs() {
    for kind in 0..8 {
        let thr_db = -6.0f32;
        let mut s = make(kind, thr_db, 0.0);
        let t = 10f64.powf(thr_db as f64 / 20.0);
        let x: Vec<f32> = (0..256).map(|i| -1.5 + 3.0 * i as f32 / 255.0).collect();
        let (mut l, mut r) = (x.clone(), x.clone());
        s.process(&mut l, &mut r);
        for (i, &xi) in x.iter().enumerate() {
            let want = transfer(kind, xi as f64, t) as f32;
            assert!((l[i] - want).abs() < 1e-5, "kind {kind}, x {xi}: audio {} curve {want}", l[i]);
        }
    }
}

#[test]
fn transfer_keeps_the_threshold_on_dense_and_knee_curves() {
    for kind in [CURVE_DENSE_LIGHT, CURVE_DENSE_MEDIUM, CURVE_DENSE_HEAVY, CURVE_QUAD_2DB, CURVE_QUAD_6DB, CURVE_QUAD_9DB] {
        assert!((transfer(kind, 0.5, 0.5) - 0.5).abs() < 1e-9, "kind {kind}");
    }
    assert_eq!(transfer(CURVE_OFF, 0.3, 0.5), 0.3);
    assert!((upward_boost_db(6.0, -16.0, -21.0) - 3.0).abs() < 1e-12);
    assert_eq!(upward_boost_db(6.0, -16.0, -40.0), 6.0);
    assert_eq!(upward_boost_db(6.0, -16.0, -10.0), 0.0);
}

#[test]
fn view_reports_peaks_and_boosts_per_block() {
    let mut s = make(CURVE_QUAD_6DB, -6.0, 6.0);
    let mut out = [0.0f32; 8];
    assert_eq!(s.view(&mut out), view::LEN);
    assert_eq!(out[view::IN_PEAK_DB], view::FLOOR_DB, "silent before audio");
    let x: Vec<f32> = (0..480).map(|i| 0.5 * (i as f32 * 0.13).sin()).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    s.process(&mut l, &mut r);
    s.view(&mut out);
    let peak_in = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let peak_out = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
    assert!((out[view::IN_PEAK_DB] - 20.0 * peak_in.log10()).abs() < 1e-3);
    assert!((out[view::DRIVE_PEAK_DB] - (20.0 * peak_in.log10() + 6.0)).abs() < 0.05, "drive peak {}", out[view::DRIVE_PEAK_DB]);
    assert!((out[view::OUT_PEAK_DB] - 20.0 * peak_out.log10()).abs() < 1e-3);
    assert_eq!(out[view::UPWARD_DB], 0.0);
    assert_eq!(out[view::TRANSIENT_DB], 0.0);

    // Upward and transient boosts show while they act.
    let mut s = make(CURVE_OFF, 0.0, 0.0);
    s.set_param(P_UPWARD, 6.0);
    s.set_param(P_UPWARD_THRESHOLD, -20.0);
    s.set_param(P_TRANSIENT, 100.0);
    let mut quiet: Vec<f32> = (0..4800).map(|i| 0.01 * (i as f32 * 0.05).sin()).collect();
    for v in quiet.iter_mut().skip(4000) {
        *v *= 30.0;
    }
    let (mut l, mut r) = (quiet.clone(), quiet);
    s.process(&mut l, &mut r);
    s.view(&mut out);
    assert!(out[view::UPWARD_DB] > 5.0 && out[view::UPWARD_DB] <= 6.0, "upward {}", out[view::UPWARD_DB]);
    assert!(out[view::TRANSIENT_DB] > 0.1 && out[view::TRANSIENT_DB] <= 3.0, "transient {}", out[view::TRANSIENT_DB]);
}
