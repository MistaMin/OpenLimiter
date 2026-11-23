//! The editor read-out (`view`): what it reports matches the static curve and the meters.
mod common;
use common::*;
use comp_dual::{knee_half_width_db, static_gain_db, view, auto_makeup_db};
use olm_api::Module;

fn read(m: &CompDual) -> [f32; view::LEN] {
    let mut v = [0.0; view::LEN];
    assert_eq!(m.view(&mut v), view::LEN);
    v
}
use comp_dual::CompDual;

#[test]
fn view_reports_detector_gain_makeup_and_limiter() {
    // Per channel: left at -12 dBFS, right at -18 dBFS, hard knee, threshold -30, 4:1.
    let mut m = with(&[("channelMode", 1.0), ("threshold1", -30.0), ("threshold2", -30.0), ("ratio1", 4.0), ("ratio2", 4.0), ("softKnee1", 0.0), ("softKnee2", 0.0), ("makeup2", 3.0), ("limiterOn", 0.0)]);
    let n = 2 * SR;
    let l = sine(1000.0, lin(-12.0), 0.0, n);
    let r = sine(1000.0, lin(-18.0), 0.0, n);
    let _ = render2(&mut m, &l, &r, None);
    let v = read(&m);
    println!("{v:?}");
    // The follower sits a little under the sine peak (its release ripples between peaks); the
    // two channels are 6 dB apart, as their inputs.
    assert!(v[view::DETECTOR] < -12.0 && v[view::DETECTOR] > -15.0, "{}", v[view::DETECTOR]);
    assert!((v[view::DETECTOR] - v[view::DETECTOR + 1] - 6.0).abs() < 0.1);
    for c in 0..2 {
        let want = static_gain_db(v[view::DETECTOR + c] as f64, -30.0, 0.0, 4.0) as f32;
        assert!((v[view::GAIN + c] - want).abs() < 0.5, "channel {c}: {} against {want}", v[view::GAIN + c]);
    }
    assert_eq!(v[view::MAKEUP], 0.0);
    assert_eq!(v[view::MAKEUP + 1], 3.0);
    assert_eq!(v[view::LIMIT], 0.0);
    assert_eq!(v[view::KEYED], 0.0);
    // The meters agree on the reduction.
    let mt = m.meters();
    assert!((mt.band_reduction_db[0] + v[view::GAIN]).abs() < 1e-3);

    // Auto make-up is in the make-up slot; the safety limiter's reduction in its own.
    let mut m = with(&[("threshold1", -20.0), ("ratio1", 4.0), ("autoMakeup", 1.0), ("limiterGain", 12.0)]);
    let x = sine(1000.0, lin(-6.0), 0.0, SR);
    let _ = render(&mut m, &x);
    let v = read(&m);
    assert!((v[view::MAKEUP] - auto_makeup_db(-20.0, 4.0) as f32).abs() < 1e-4);
    assert!(v[view::LIMIT] > 1.0, "{}", v[view::LIMIT]);
    assert_eq!(v[view::DETECTOR], v[view::DETECTOR + 1], "ganged");
    let _ = knee_half_width_db(-20.0, 100.0);
}

#[test]
fn view_flags_the_key() {
    let mut m = with(&[("keySource", 1.0), ("threshold1", -30.0)]);
    let x = sine(1000.0, lin(-12.0), 0.0, SR / 2);
    let k = sine(200.0, lin(-6.0), 0.0, SR / 2);
    let _ = render2(&mut m, &x, &x, Some((&k, &k)));
    let v = read(&m);
    assert_eq!(v[view::KEYED], 1.0);
    assert!(v[view::DETECTOR] < -6.0 && v[view::DETECTOR] > -9.0, "the detector reads the key (-6 dBFS): {}", v[view::DETECTOR]);
    // Without a key connected the detector has nothing to read.
    let _ = render2(&mut m, &x, &x, None);
    assert_eq!(read(&m)[view::KEYED], 0.0);
}
