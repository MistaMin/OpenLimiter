//! Every parameter at its minimum, maximum and default processes noise without NaN and without
//! exceeding +40 dBFS; the parameter table is consistent.

use eq_dynamic::{band_param, DynamicEq, F_DYN, NPARAMS, PARAMS};
use olm_api::Module;

fn noise(n: usize, seed: u64, a: f32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * a
        })
        .collect()
}

#[test]
fn every_parameter_at_min_max_default() {
    let mut m = DynamicEq::default();
    m.prepare(48000.0, 512);
    // Make the dynamic paths live, so their parameters matter.
    for b in 0..3 {
        m.set_param(band_param(b, F_DYN), 1.0);
    }
    let base: Vec<f32> = (0..NPARAMS).map(|i| if PARAMS[i].id.ends_with("Dyn") && i < 3 * eq_dynamic::PER_BAND { 1.0 } else { PARAMS[i].default }).collect();
    let x = noise(2048, 3, 0.1);
    let key = noise(2048, 5, 0.5);
    let mut worst = 0.0f32;
    for i in 0..NPARAMS {
        for v in [PARAMS[i].min, PARAMS[i].max, PARAMS[i].default] {
            m.set_param(i, v);
            let (mut l, mut r) = (x.clone(), x.clone());
            m.process_keyed(&mut l[..1024], &mut r[..1024], &key[..1024], &key[..1024]);
            m.process(&mut l[1024..], &mut r[1024..]);
            let peak = l.iter().chain(&r).fold(0.0f32, |a, &s| if s.is_finite() { a.max(s.abs()) } else { f32::INFINITY });
            worst = worst.max(peak);
            assert!(peak <= 100.0, "{} = {v}: peak {peak}", PARAMS[i].id);
            m.set_param(i, base[i]);
        }
        if i % 97 == 0 {
            m.reset();
        }
    }
    println!("worst peak {worst:.3} ({:.1} dBFS)", 20.0 * worst.log10());
}

#[test]
fn parameter_table() {
    assert_eq!(PARAMS.len(), NPARAMS);
    assert_eq!(NPARAMS, 32 * 34 + 2);
    let mut ids: Vec<&str> = PARAMS.iter().map(|p| p.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), NPARAMS);
    for p in PARAMS.iter() {
        assert!(p.default >= p.min && p.default <= p.max, "{}", p.id);
        assert!(p.id.chars().next().unwrap().is_ascii_lowercase() && !p.id.contains('_'));
    }
    let types = match PARAMS[1].kind {
        olm_api::Kind::Choice(l) => l,
        _ => panic!(),
    };
    assert_eq!(types.len(), 48);
    assert!(types.contains(&"Steep HP") && types.contains(&"Steep LP"));
}

#[test]
fn robust_to_bad_calls() {
    let mut m = DynamicEq::default();
    let (mut a, mut b) = (vec![0.1f32; 100], vec![0.1f32; 100]);
    m.process(&mut a, &mut b); // before prepare
    m.set_param(NPARAMS + 3, 1.0);
    m.set_param(0, f32::NAN);
    m.prepare(44100.0, 64);
    let (mut l, mut r) = (vec![0.1f32; 3000], vec![0.1f32; 10]);
    m.process(&mut l, &mut r);
    m.process(&mut [], &mut []);
    let (mut l, mut r) = (vec![f32::NAN; 600], vec![f32::INFINITY; 600]);
    m.process_keyed(&mut l, &mut r, &[0.0; 3], &[0.0; 3]);
    assert!(l.iter().chain(&r).all(|v| v.is_finite()));
}
