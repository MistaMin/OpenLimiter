//! The editor read-out (`view`) of the module.

use comp_sone::{idx, view, DualSone};
use olm_api::Module;

fn read(m: &DualSone) -> [f32; view::LEN] {
    let mut v = [0.0; view::LEN];
    assert_eq!(m.view(&mut v), view::LEN);
    v
}

#[test]
fn view_reports_the_engine_meters() {
    let mut m = DualSone::default();
    m.prepare(48000.0, 512);
    m.set_param(idx::THRESHOLD, -30.0);
    // A 1 kHz tone at -12 dBFS with a short burst 12 dB louder every second.
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let mut v = read(&m);
    let mut deepest_fast = 0.0f32;
    for block in 0..(48000 * 6 / 512) {
        for i in 0..512 {
            let n = block * 512 + i;
            let a = if n % 48000 < 2400 && n > 48000 * 3 { 1.0 } else { 0.25 };
            l[i] = a * (std::f32::consts::TAU * 1000.0 * n as f32 / 48000.0).sin();
            r[i] = l[i];
        }
        m.process(&mut l, &mut r);
        v = read(&m);
        let c = m.core();
        assert_eq!(v[view::LOUDNESS], c.meter_loudness);
        assert_eq!(v[view::SLOW_GAIN], c.meter_slow_gr);
        assert_eq!(v[view::FAST_GAIN], c.meter_fast_gr);
        assert_eq!(v[view::TOTAL_GAIN], c.meter_total_gr);
        assert_eq!(v[view::CREST], c.meter_crest);
        assert_eq!(v[view::AUTO_GAIN], c.meter_auto_gain_db);
        assert!(v[view::TOTAL_GAIN] <= v[view::SLOW_GAIN].min(v[view::FAST_GAIN]) + 1e-4);
        deepest_fast = deepest_fast.min(v[view::FAST_GAIN]);
    }
    println!("{v:?}, deepest fast {deepest_fast}");
    // A steady tone at about -12 LUFS against a -30 LUFS threshold: the slow engine works.
    assert!(v[view::LOUDNESS] > -16.0 && v[view::LOUDNESS] < -8.0);
    assert!(v[view::SLOW_GAIN] < -3.0);
    // The bursts stand 12 dB above the loudness: the fast engine catches them.
    assert!(deepest_fast < -1.0);
    // The reference is the loudness the crest is measured from; the 5 ms level is above it.
    assert!(v[view::REFERENCE] > -20.0 && v[view::REFERENCE] < -6.0, "{}", v[view::REFERENCE]);
    assert!(v[view::FAST_LEVEL] >= v[view::LOUDNESS] - 3.0);
    assert_eq!(v[view::HOLD], 0.0);
}

#[test]
fn view_shows_the_hold_in_a_pause() {
    let mut m = DualSone::default();
    m.prepare(48000.0, 512);
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    for block in 0..(48000 * 8 / 512) {
        for i in 0..512 {
            let n = block * 512 + i;
            l[i] = if n < 48000 * 5 { 0.5 * (std::f32::consts::TAU * 1000.0 * n as f32 / 48000.0).sin() } else { 0.0 };
            r[i] = l[i];
        }
        m.process(&mut l, &mut r);
    }
    assert_eq!(read(&m)[view::HOLD], 1.0);
}
