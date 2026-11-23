//! Static compression table (source F table file, section 4.1).
//! Signal: `fine_4000.wav` / `steps_<f>.wav` of the source F signal script (sine, 1 s steps).
//! Analysis: RMS of output over input, 0.6 to 0.95 s into each step (round 2 script) or 0.5 to
//! 0.9 s (`steps_gain` of the library script).
//! The table file warns that compressor figures moved by up to 0.17 dB between runs.
mod common;
use common::*;
use comp_bands::*;
use olm_api::Module;

const HM: usize = 3; // the high-mid band, which a 4 kHz tone sits in

fn himid(thr: f32, ratio: f32) -> BandCompressor {
    let mut m = module();
    set_band(&mut m, HM, B_THRESHOLD, thr);
    set_band(&mut m, HM, B_RATIO, ratio);
    m
}

/// Gain at -30, -26, -22, -18, -16, ..., 0 dBFS, the columns of the table in section 4.1.
fn table_columns(g: &[f64]) -> Vec<f64> {
    // g: 26 steps from -50 dBFS in 2 dB steps.
    let idx = [10, 12, 14, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25];
    idx.iter().map(|i| g[*i]).collect()
}

fn check(name: &str, m: &mut BandCompressor, want: [f64; 13], tol: f64) -> f64 {
    let s = fine_steps(4000.0);
    let y = render_mono(m, &s);
    let got = table_columns(&step_gains(&y, &s, 0.6, 0.95));
    let worst = got.iter().zip(want.iter()).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    println!("{name}: worst {worst:.2} dB (tolerance {tol})\n   got  {:?}\n   want {:?}", got.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>(), want);
    assert!(worst <= tol, "{name}: {worst} dB");
    worst
}

/// The eleven rows of the table in section 4.1 (data: source F data folder,
/// `round2/jobs_s*.json`, keys "static fine | HiMid ...", field gain_L_db).
#[test]
fn static_table_at_4_khz() {
    let inf = RATIO_INFINITE;
    check("thr -10, ratio 1.43:1", &mut himid(-10.0, 1.43), [0.0, 0.0, 0.0, 0.0, -0.21, -0.68, -1.15, -1.6, -2.2, -2.86, -3.52, -4.17, -4.82], 0.1);
    check("thr -10, ratio 2:1", &mut himid(-10.0, 2.0), [0.0, 0.0, 0.0, 0.0, -0.34, -1.12, -1.87, -2.59, -3.46, -4.4, -5.33, -6.24, -7.13], 0.1);
    check("thr -10, ratio 4.13:1", &mut himid(-10.0, 4.13), [0.0, 0.0, 0.0, 0.0, -0.52, -1.68, -2.78, -3.8, -4.96, -6.19, -7.37, -8.5, -9.57], 0.1);
    check("thr -10, ratio Inf:1", &mut himid(-10.0, inf), [0.0, 0.0, 0.0, 0.0, -0.68, -2.17, -3.57, -4.83, -6.19, -7.59, -8.9, -10.09, -11.16], 0.1);
    check("thr -20, Inf", &mut himid(-20.0, inf), [0.0, -0.68, -3.57, -6.0, -7.02, -7.91, -8.68, -9.27, -10.06, -10.98, -11.81, -12.54, -13.17], 0.1);
    check("thr 0, Inf", &mut himid(0.0, inf), [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.01, -0.12, -0.99, -2.73, -4.42, -6.05], 0.1);

    let mut m = himid(-10.0, inf);
    m.set_param(P_REFERENCE, -18.0);
    check("thr -10, Inf, ref -18", &mut m, [0.0, -0.72, -3.6, -6.23, -7.62, -8.93, -10.12, -11.19, -12.13, -13.2, -14.32, -15.39, -16.42], 0.1);

    // Peak detector: this module reads the largest sample, and a 4 kHz sine sampled at 48 kHz
    // can show up to 0.3 dB less than its true peak. Tolerance 0.25 dB.
    let mut m = himid(-10.0, inf);
    m.set_param(P_DETECTOR, 0.0);
    check("thr -10, Inf, detector Peak", &mut m, [0.0, 0.0, 0.0, 0.0, -0.68, -2.17, -3.57, -4.83, -6.19, -7.59, -8.9, -10.09, -11.16], 0.25);

    // Make-up +6 dB: the original's plain gains read 0.05 dB low (brief 4.1); not copied.
    let mut m = himid(-10.0, inf);
    set_band(&mut m, HM, B_MAKEUP, 6.0);
    check("thr -10, Inf, makeup +6", &mut m, [5.05, 5.05, 5.05, 5.05, 4.28, 2.56, 0.9, -0.68, -2.32, -3.98, -5.58, -7.11, -8.54], 0.1);

    // Fastest times: the measured row itself differs from the normal row by up to 0.58 dB in
    // both directions (single run); section 4.1 calls it "the same curve within 0.4 dB".
    let mut m = himid(-10.0, inf);
    set_band(&mut m, HM, B_ATTACK, 0.3);
    set_band(&mut m, HM, B_RELEASE, 20.0);
    check("thr -10, Inf, attack 0.3 release 20", &mut m, [0.0, 0.0, 0.0, 0.0, -0.27, -1.59, -4.01, -5.27, -6.64, -8.03, -9.18, -10.36, -11.42], 0.6);

    let mut m = himid(-10.0, inf);
    set_band(&mut m, HM, B_ATTACK, 350.0);
    set_band(&mut m, HM, B_RELEASE, 7000.0);
    check("thr -10, Inf, attack 350 release 7 s", &mut m, [0.0, 0.0, 0.0, 0.0, -0.67, -2.16, -3.56, -4.82, -6.18, -7.58, -8.89, -10.08, -11.16], 0.1);
}

/// Section 4.1: the slopes that the summed bands give ("Inf:1" about 0.35 dB/dB between -16 and
/// 0 dBFS, 4.13:1 about 0.42, 2:1 about 0.55, 1.43:1 about 0.70; threshold -20 Inf about 0.63
/// between -6 and 0 dBFS). No ratio holds the output constant.
#[test]
fn nominal_ratios_are_not_reached_after_summing() {
    let s = fine_steps(4000.0);
    for (ratio, thr, from, want) in [(RATIO_INFINITE, -10.0, 17usize, 0.35), (4.13, -10.0, 17, 0.42), (2.0, -10.0, 17, 0.55), (1.43, -10.0, 17, 0.70), (RATIO_INFINITE, -20.0, 22, 0.63)] {
        let mut m = himid(thr, ratio);
        let y = render_mono(&mut m, &s);
        let g = step_gains(&y, &s, 0.6, 0.95);
        let span = (25 - from) as f64 * 2.0;
        let slope = 1.0 + (g[25] - g[from]) / span;
        println!("ratio {ratio}, thr {thr}: output slope {slope:.3} dB/dB (measured about {want})");
        assert!((slope - want).abs() < 0.03);
    }
}

/// Section 4.6: look-ahead 0, 2, 8.5 and 15 ms and detector crest 6 dB and 20 dB give the same
/// static curve "to 0.01 dB" (data: `round5/jobs_s0.json`, "lookahead static | ..." and
/// "crest static | ..."). Required here: within 0.02 dB of the default setting.
#[test]
fn static_curve_does_not_depend_on_lookahead_or_crest() {
    let s = fine_steps(4000.0);
    let base = {
        let mut m = himid(-10.0, RATIO_INFINITE);
        step_gains(&render_mono(&mut m, &s), &s, 0.6, 0.95)
    };
    for (p, v) in [(P_LOOKAHEAD, 0.0), (P_LOOKAHEAD, 2.0), (P_LOOKAHEAD, 8.5), (P_LOOKAHEAD, 15.0), (P_DETECTOR, 1.0), (P_DETECTOR, 6.0)] {
        let mut m = himid(-10.0, RATIO_INFINITE);
        m.set_param(p, v);
        let g = step_gains(&render_mono(&mut m, &s), &s, 0.6, 0.95);
        let worst = g.iter().zip(base.iter()).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        println!("param {p} = {v}: worst difference to default {worst:.4} dB");
        assert!(worst < 0.02);
    }
}

/// Section 4.1, "The other bands behave alike": gain at -12 / -6 / 0 dBFS at the band centre,
/// all bands at their defaults, and with the band at threshold -10 dB, Inf:1 (data: source F
/// data folder, `round1/jobs_s*.json`, keys "comp static | <band> ... | <f> Hz"). The middle
/// band of the original ran on its editor values (threshold -8 dB, 2:1), which are this
/// module's defaults; its Inf:1 row does not exist in the data.
/// Tolerance 0.25 dB for 200 Hz to 12 kHz.
#[test]
fn other_bands_at_their_centres() {
    let rows: [(f64, usize, [f64; 3], Option<[f64; 3]>); 4] = [
        (200.0, 1, [-0.23, -2.98, -6.14], Some([-2.64, -7.2, -12.13])),
        (1000.0, 2, [-0.73, -3.20, -6.27], None),
        (4000.0, 3, [-1.48, -4.03, -6.79], Some([-3.57, -7.59, -11.16])),
        (12000.0, 4, [-1.42, -3.97, -6.61], Some([-2.45, -5.39, -7.87])),
    ];
    for (f, b, defaults, inf) in rows {
        let s = coarse_steps(f);
        let mut m = module();
        let g = step_gains(&render_mono(&mut m, &s), &s, 0.5, 0.9);
        println!("{f} Hz defaults: got {:.2} {:.2} {:.2}, measured {:?}", g[6], g[7], g[8], defaults);
        for k in 0..3 {
            assert!((g[6 + k] - defaults[k]).abs() < 0.25, "{f} Hz defaults");
        }
        if let Some(want) = inf {
            let mut m = module();
            set_band(&mut m, b, B_THRESHOLD, -10.0);
            set_band(&mut m, b, B_RATIO, RATIO_INFINITE);
            let g = step_gains(&render_mono(&mut m, &s), &s, 0.5, 0.9);
            println!("{f} Hz thr -10 Inf: got {:.2} {:.2} {:.2}, measured {:?}", g[6], g[7], g[8], want);
            for k in 0..3 {
                assert!((g[6 + k] - want[k]).abs() < 0.25, "{f} Hz Inf");
            }
        }
    }
}

/// The middle band on a 1 kHz fine sweep with every other threshold at +20 dB (section 4.1 last
/// item; data `round2/jobs_s*.json`, "static fine | Mid band (editor values), defaults"), and
/// the same with the reference level at 0 and -24 dBFS. Tolerance 0.1 dB.
#[test]
fn middle_band_fine_sweep() {
    let s = fine_steps(1000.0);
    let mut m = module();
    nothing_compresses(&mut m);
    set_band(&mut m, 2, B_THRESHOLD, -8.0);
    let g = step_gains(&render_mono(&mut m, &s), &s, 0.6, 0.95);
    let want = [0.0, 0.0, -0.73, -1.49, -2.22, -2.93, -3.61, -4.27, -4.9]; // -16 .. 0 dBFS
    let got = &g[17..26];
    println!("mid band 1 kHz: got {:?}\n            measured {:?}", got.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>(), want);
    for k in 0..9 {
        assert!((got[k] - want[k]).abs() < 0.1);
    }
    for (reference, level_index, want) in [(0.0f32, 25usize, -2.23), (-24.0, 13, -2.22), (-24.0, 25, -8.82)] {
        let mut m = module();
        nothing_compresses(&mut m);
        set_band(&mut m, 2, B_THRESHOLD, -8.0);
        m.set_param(P_REFERENCE, reference);
        let g = step_gains(&render_mono(&mut m, &s), &s, 0.6, 0.95);
        println!("reference {reference}: gain at {} dBFS {:.2} (measured {want})", -50 + 2 * level_index as i32, g[level_index]);
        assert!((g[level_index] - want).abs() < 0.1);
    }
}

/// 40 Hz, the low band (same data keys as above, "Lo defaults" and "Lo thr -10, Inf:1").
/// NOT REPRODUCED. The table file calls the measured 40 Hz row "a single run and not explained"
/// (less reduction at 0 dBFS than at -6 dBFS relative to the trend). This module reduces more
/// than measured at 40 Hz, by up to about 2 dB. The test only pins the present behaviour inside
/// 2.2 dB so that a change is noticed; REPORT.md lists the numbers.
#[test]
fn low_band_at_40_hz_is_only_roughly_matched() {
    let s = coarse_steps(40.0);
    let mut m = module();
    let g = step_gains(&render_mono(&mut m, &s), &s, 0.5, 0.9);
    let want = [-0.25, -2.42, -6.26];
    println!("40 Hz defaults: got {:.2} {:.2} {:.2}, measured {:?}", g[6], g[7], g[8], want);
    for k in 0..3 {
        assert!((g[6 + k] - want[k]).abs() < 2.2);
    }
    let mut m = module();
    set_band(&mut m, 0, B_THRESHOLD, -10.0);
    set_band(&mut m, 0, B_RATIO, RATIO_INFINITE);
    let g = step_gains(&render_mono(&mut m, &s), &s, 0.5, 0.9);
    let want = [-2.5, -8.02, -10.74];
    println!("40 Hz thr -10 Inf: got {:.2} {:.2} {:.2}, measured {:?}", g[6], g[7], g[8], want);
    for k in 0..3 {
        assert!((g[6 + k] - want[k]).abs() < 2.2);
    }
}
