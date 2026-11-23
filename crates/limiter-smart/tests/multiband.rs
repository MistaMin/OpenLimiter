//! Spec 7.6 (Multiband): 50 Hz (+6 dB) + 5 kHz (-18 dBFS) cross-band IMD; flat unity
//! pass-through; LR4 delta nulls exactly below threshold; band 0 stays linked in Mid-Side.

mod common;
use common::*;
use limiter_smart::idx;
use olm_api::Module;

const C_DB: f64 = -0.2;

/// 5 kHz carrier level and the sidebands at 5 kHz +- k x 100 Hz (k = 1..6), dBc.
fn imd(y: &[f64]) -> (f64, f64) {
    let c = dft_amp(y, 5000.0, SR);
    let mut p = 0.0;
    for k in 1..=6 {
        for s in [-1.0, 1.0] {
            let a = dft_amp(y, 5000.0 + s * 100.0 * k as f64, SR);
            p += a * a;
        }
    }
    (db(c), 10.0 * (p / (c * c)).log10())
}

#[test]
fn cross_band_imd_is_suppressed() {
    let n = 48000 * 3;
    let x: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            lin(C_DB + 6.0) * (2.0 * std::f64::consts::PI * 50.0 * t).sin() + lin(-18.0) * (2.0 * std::f64::consts::PI * 5000.0 * t).sin()
        })
        .collect();
    // (engine, crossover, bandFocus)
    let cases = [(4usize, 0.0f32, 60.0f32), (4, 1.0, 60.0), (4, 0.0, 100.0), (4, 1.0, 100.0), (1, 0.0, 60.0), (0, 0.0, 60.0), (5, 0.0, 60.0)];
    let mut res = Vec::new();
    for (e, xo, focus) in cases {
        let mut m = make(SR, &[engine(e), (idx::CROSSOVER_MODE, xo), (idx::BAND_FOCUS, focus)]);
        let (y, _) = run(&mut m, &x, &x);
        let seg = &y[48000..96000];
        let (c, s) = imd(seg);
        let name = if e == 4 {
            format!("Multiband {} focus {focus}", if xo > 0.5 { "lin-phase" } else { "LR4" })
        } else {
            ENGINE_NAMES[e].to_string()
        };
        println!("{:28}: 5 kHz carrier {:+6.2} dB re input, 100 Hz sidebands {:7.1} dBc, output peak {:.3} dBFS", name, c + 18.0, s, db(peak(seg)));
        res.push((c + 18.0, s));
    }
    let smooth = res[4].1;
    let mb_default = res[0].1.max(res[1].1);
    let mb_focus = res[2].1.max(res[3].1);
    println!(
        "sidebands: Multiband default focus {mb_default:.1} dBc, focus 100 % {mb_focus:.1} dBc; Smooth {smooth:.1}, Pure {:.1}, Glide {:.1} dBc",
        res[5].1,
        res[6].1
    );
    // the directional allocation keeps the 5 kHz line and its modulation off the bass
    assert!(mb_focus < smooth - 20.0);
    assert!(res[2].0 > -0.5 && res[3].0 > -0.5);
    // at the default focus the uniform share (40 %) still modulates the upper bands: less
    // than a broadband limiter, and the carrier keeps most of its level
    assert!(mb_default < smooth - 3.0);
    assert!(res[0].0 > 0.5 * res[4].0);
}

#[test]
fn flat_unity_pass_through_below_threshold() {
    for xo in [0.0f32, 1.0] {
        let mut worst_db = 0.0f64;
        let mut m = make(SR, &[engine(4), (idx::CROSSOVER_MODE, xo)]);
        for f in [20.0, 50.0, 120.0, 300.0, 800.0, 2000.0, 4500.0, 8000.0, 16000.0, 20000.0] {
            m.reset();
            let x = sine(f, 0.3, 48000, SR, 0.0);
            let (y, _) = run(&mut m, &x, &x);
            let g = db(dft_amp(&y[12000..36000], f, SR) / 0.3);
            worst_db = worst_db.max(g.abs());
        }
        // and the waveform: linear phase must be the input, delayed by exactly the latency
        m.reset();
        let x = noise(48000, 0.1, 21);
        let (y, _) = run(&mut m, &x, &x);
        let err = x.iter().zip(y.iter()).skip(2000).take(40000).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        println!(
            "Multiband {} below threshold: magnitude within {:.5} dB from 20 Hz to 20 kHz; waveform error {:.2e} ({:.1} dB re signal)",
            if xo > 0.5 { "linear phase" } else { "LR4" },
            worst_db,
            err,
            db(err / 0.1)
        );
        assert!(worst_db < 0.01);
        if xo > 0.5 {
            assert!(err < 1e-6);
        }
    }
}

#[test]
fn lr4_delta_nulls_exactly_below_threshold() {
    for xo in [0.0f32, 1.0] {
        let mut m = make(SR, &[engine(4), (idx::CROSSOVER_MODE, xo), (idx::DELTA, 1.0)]);
        let x = noise(48000 * 2, 0.08, 31);
        let r: Vec<f64> = noise(48000 * 2, 0.08, 32);
        let (yl, yr) = run(&mut m, &x, &r);
        let worst = peak(&yl[4000..]).max(peak(&yr[4000..]));
        println!("Multiband {} delta below threshold: largest output sample {:e}", if xo > 0.5 { "linear phase" } else { "LR4" }, worst);
        assert_eq!(worst, 0.0, "delta must null to exact zero");
        // above threshold the delta carries what was removed
        let mut m = make(SR, &[engine(4), (idx::CROSSOVER_MODE, xo), (idx::DELTA, 1.0), (idx::DRIVE, 12.0)]);
        let (yl, _) = run(&mut m, &x, &r);
        println!("  with 12 dB drive the delta peaks at {:.2} dBFS", db(peak(&yl[4000..])));
        assert!(peak(&yl[4000..]) > 0.01);
    }
}

#[test]
fn band_0_stays_linked_in_mid_side() {
    // L = 1.5 s, R = 0.9 s: M = 1.2 s, S = 0.3 s (S/M = 0.25). With transient link 0 the upper
    // bands share the excess between M and S by the diamond rule; band 0 must keep S/M.
    let ratio = |f: f64, link: f32| -> f64 {
        let mut m = make(SR, &[engine(4), (idx::CHANNEL_MODE, 1.0), (idx::TRANSIENT_LINK, link), (idx::RELEASE_LINK, link)]);
        let s = sine(f, 1.0, 48000 * 2, SR, 0.0);
        let l: Vec<f64> = s.iter().map(|v| 1.5 * v * lin(C_DB)).collect();
        let r: Vec<f64> = s.iter().map(|v| 0.9 * v * lin(C_DB)).collect();
        let (yl, yr) = run(&mut m, &l, &r);
        let seg = 48000..72000;
        let mm: Vec<f64> = seg.clone().map(|i| 0.5 * (yl[i] + yr[i])).collect();
        let ss: Vec<f64> = seg.map(|i| 0.5 * (yl[i] - yr[i])).collect();
        dft_amp(&ss, f, SR) / dft_amp(&mm, f, SR)
    };
    let b0 = ratio(50.0, 0.0);
    let b1 = ratio(400.0, 0.0);
    let b1l = ratio(400.0, 100.0);
    println!("Mid-Side, input S/M = 0.2500: band 0 (50 Hz) out S/M {b0:.4} with link 0; band 1 (400 Hz) out S/M {b1:.4} with link 0, {b1l:.4} with link 100 %");
    assert!((b0 - 0.25).abs() < 0.002, "band 0 must stay linked");
    assert!((b1 - 0.25).abs() > 0.01, "band 1 unlinks M/S");
    assert!((b1l - 0.25).abs() < 0.002);
}
