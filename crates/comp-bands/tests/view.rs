//! The key source switch, the editor read-out (`view`) and the public curve helpers.
mod common;
use common::*;
use comp_bands::*;
use olm_api::{Kind, Module};

fn tone(freq: f64, level_db: f64, len: usize) -> Vec<f32> {
    (0..len).map(|i| (lin(level_db) * (2.0 * std::f64::consts::PI * freq * i as f64 / SR as f64).sin()) as f32).collect()
}

/// Key Source is the last parameter, a two-way choice that starts on Internal. On Internal the
/// key is ignored: `process_keyed` gives exactly what `process` gives. On External a silent key
/// leaves the audio uncompressed.
#[test]
fn key_source_picks_the_detector_input() {
    let p = module().params()[P_KEY_SOURCE];
    assert_eq!(P_KEY_SOURCE, PARAM_COUNT - 1);
    assert_eq!(p.id, "keySource");
    assert!(matches!(p.kind, Kind::Choice(l) if l == ["Internal", "External"]));
    assert_eq!(p.default, 0.0);
    let x = tone(4000.0, -6.0, SRU);
    let silent = vec![0.0f32; x.len()];
    let run = |external: Option<f32>| {
        let mut m = module();
        set_band(&mut m, 3, B_RATIO, RATIO_INFINITE);
        let (mut a, mut b) = (x.clone(), x.clone());
        match external {
            Some(v) => {
                m.set_param(P_KEY_SOURCE, v);
                for i in (0..a.len()).step_by(512) {
                    let e = (i + 512).min(a.len());
                    m.process_keyed(&mut a[i..e], &mut b[i..e], &silent[i..e], &silent[i..e]);
                }
            }
            None => {
                for (l, r) in a.chunks_mut(512).zip(b.chunks_mut(512)) {
                    m.process(l, r);
                }
            }
        }
        let mut v = [0.0f32; view::LEN];
        m.view(&mut v);
        (a, v[view::KEYED])
    };
    let (plain, k0) = run(None);
    let (internal, k1) = run(Some(0.0));
    let (external, k2) = run(Some(1.0));
    assert!(plain == internal, "Internal ignores the key");
    assert_eq!((k0, k1, k2), (0.0, 0.0, 1.0));
    let lat = module().latency();
    let tail = SRU / 2..SRU;
    let g_plain = db(rms(&plain[tail.clone()])) - db(rms(&x[tail.start - lat..tail.end - lat]));
    let g_ext = db(rms(&external[tail.clone()])) - db(rms(&x[tail.start - lat..tail.end - lat]));
    assert!(g_plain < -3.0 && g_ext.abs() < 0.01, "{g_plain} {g_ext}");
}

/// The read-out: 36 values; per-band reduction equals the meters; the band gain at the end of
/// a steady tone is the static curve at the level the detector reports.
#[test]
fn view_reports_what_the_bands_do() {
    let mut m = module();
    set_band(&mut m, 3, B_THRESHOLD, -10.0);
    set_band(&mut m, 3, B_RATIO, 4.0);
    let x = tone(4000.0, -6.0, 2 * SRU);
    let (mut l, mut r) = (x.clone(), x.clone());
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(a, b);
    }
    let mut v = [0.0f32; 64];
    assert_eq!(m.view(&mut v), view::LEN);
    assert_eq!(view::LEN, 36);
    let mt = m.meters();
    for b in 0..BANDS {
        assert!((v[view::GR + b] - mt.band_reduction_db[b]).abs() < 1e-6, "band {b}");
        assert_eq!(v[view::LIMIT + b], 0.0);
        assert_eq!(v[view::GR_CHANNEL + 2 * b], v[view::GR_CHANNEL + 2 * b + 1], "linked");
    }
    let det = v[view::DETECTOR + 3];
    let want = static_gain_db(0, det - m.param(P_REFERENCE), -10.0, 4.0, 35.0, -30.0, 1.0);
    println!("high-mid: detector {det:.2} dBFS, gain {:.2} dB, static curve {want:.2} dB, out {:.2} dBFS", v[view::GAIN + 3], v[view::OUTPUT + 3]);
    assert!(det > -10.0 && det < -4.0);
    assert!((v[view::GAIN + 3] - want).abs() < 0.3);
    assert!(v[view::OUTPUT + 3] < -6.0 && v[view::OUTPUT + 3] > -20.0);
    // The band limiter's share shows in its own slots.
    m.set_param(P_BAND_LIMITER, 1.0);
    set_band(&mut m, 3, B_CEILING, -18.0);
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        a.copy_from_slice(&x[..a.len()]);
        b.copy_from_slice(&x[..b.len()]);
        m.process(a, b);
    }
    m.view(&mut v);
    assert!(v[view::LIMIT + 3] > 1.0 && v[view::OUTPUT + 3] <= -17.9, "{} {}", v[view::LIMIT + 3], v[view::OUTPUT + 3]);
}

/// The static curve helper: hard knee, slope 1 / ratio, infinite at 100; the lift rests above
/// unity below its threshold and goes to 0 dB at the reference.
#[test]
fn static_gain_helper() {
    assert_eq!(static_gain_db(0, -20.0, -10.0, 4.0, 0.0, -30.0, 1.0), 0.0);
    assert!((static_gain_db(0, -2.0, -10.0, 4.0, 0.0, -30.0, 1.0) + 6.0).abs() < 1e-5);
    assert!((static_gain_db(1, 0.0, -10.0, RATIO_INFINITE, 0.0, -30.0, 1.0) + 10.0).abs() < 1e-5);
    let rest = static_gain_db(3, -40.0, -20.0, 2.0, 35.0, -45.0, 1.0);
    assert!(rest > 3.0, "{rest}");
    assert!(static_gain_db(3, 0.0, -20.0, 2.0, 35.0, -45.0, 1.0).abs() < 1e-4);
    // The floor guard takes the lift back below its threshold.
    assert!(static_gain_db(3, -60.0, -20.0, 2.0, 35.0, -45.0, RATIO_INFINITE).abs() < 1e-4);
}

/// The band responses of the helper sum to one (complex) and follow the off rules.
#[test]
fn band_response_helper() {
    let splits = [125.0, 500.0, 2000.0, 8000.0];
    for f in [20.0, 100.0, 440.0, 3000.0, 15000.0] {
        let (mut re, mut im) = (0.0, 0.0);
        for b in 0..BANDS {
            let (r, i) = band_response(splits, b, f, SR);
            re += r;
            im += i;
        }
        assert!((re - 1.0).abs() < 1e-9 && im.abs() < 1e-9, "{f}: {re} {im}");
    }
    assert!(band_response_db(splits, 2, 1000.0, SR) > -3.0);
    assert!(band_response_db([20.0, 500.0, 2000.0, 8000.0], 0, 100.0, SR) < -200.0, "split 1 off empties the lowest band");
    assert!(band_response_db([125.0, 500.0, 2000.0, 20000.0], 4, 10000.0, SR) < -200.0, "split 4 off empties the highest band");
}
