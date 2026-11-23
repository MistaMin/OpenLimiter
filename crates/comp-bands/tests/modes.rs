//! Modes: downward, parallel, parallel unlinked, low-level lift (source F table file, sections
//! 4.4, 4.5 and 9). Signals of the source F signal script: `fine_<f>.wav`, `steps_<f>.wav`,
//! `lr_link.wav` (same tone, left -6 dBFS, right -30 dBFS) and `noise_lr.wav`.
//! Data: source F data folder, `round2/jobs_s*.json` ("static fine | ...", "defeat | ...",
//! "link | ..."), `round1/jobs_s*.json` ("comp static | Mid ..."), `round4/jobs_s0.json`
//! ("defeat | ..."), `round5/jobs_s0.json` ("stereo modes | ...").
mod common;
use common::*;
use comp_bands::*;
use olm_api::Module;

fn himid(thr: f32, ratio: f32, mode: f32) -> BandCompressor {
    let mut m = module();
    set_band(&mut m, 3, B_THRESHOLD, thr);
    set_band(&mut m, 3, B_RATIO, ratio);
    m.set_param(P_MODE, mode);
    m
}

fn fine(m: &mut BandCompressor, f: f64) -> Vec<f64> {
    let s = fine_steps(f);
    step_gains(&render_mono(m, &s), &s, 0.6, 0.95)
}

fn coarse(m: &mut BandCompressor, f: f64) -> Vec<f64> {
    let s = coarse_steps(f);
    step_gains(&render_mono(m, &s), &s, 0.5, 0.9)
}

fn compare(name: &str, got: &[f64], want: &[f64], tol: f64) {
    let worst = got.iter().zip(want.iter()).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    println!("{name}: worst {worst:.2} dB (tolerance {tol})\n   got  {:?}\n   want {:?}", got.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>(), want);
    assert!(worst <= tol, "{name}: {worst}");
}

/// Section 4.4: out = 0.5 * (dry + wet * compressed).
#[test]
fn parallel_static_curves() {
    // 4 kHz, high-mid threshold -10, Inf:1; -16 .. 0 dBFS. Downward gives -11.16 dB at 0 dBFS.
    for mode in [MODE_PARALLEL, MODE_PARALLEL_UNLINKED] {
        let g = fine(&mut himid(-10.0, RATIO_INFINITE, mode), 4000.0);
        assert!(g[..17].iter().all(|v| v.abs() < 0.02));
        compare("parallel, 4 kHz", &g[17..], &[-0.33, -1.02, -1.61, -2.1, -2.6, -3.06, -3.45, -3.79, -4.08], 0.1);
        // Wet gain -10 dB: 0.5 * (1 + 0.316) = -3.63 dB below the threshold (measured -3.65).
        let mut m = himid(-10.0, RATIO_INFINITE, mode);
        m.set_param(P_WET_GAIN, -10.0);
        let g = fine(&mut m, 4000.0);
        assert!(g[..17].iter().all(|v| (v + 3.65).abs() < 0.05));
        compare("parallel, wet -10 dB", &g[17..], &[-3.8, -4.12, -4.38, -4.59, -4.8, -4.98, -5.14, -5.27, -5.37], 0.1);
    }
    // Wet gain off: -6.02 dB at every level.
    let mut m = himid(-10.0, RATIO_INFINITE, MODE_PARALLEL);
    m.set_param(P_WET_GAIN, WET_OFF_DB);
    let g = fine(&mut m, 4000.0);
    println!("wet off: {:.3} .. {:.3} dB (measured -6.02)", g.iter().cloned().fold(f64::MAX, f64::min), g.iter().cloned().fold(f64::MIN, f64::max));
    assert!(g.iter().all(|v| (v + 6.02).abs() < 0.01));
    // Threshold -25: -30 .. 0 dBFS.
    let g = fine(&mut himid(-25.0, RATIO_INFINITE, MODE_PARALLEL), 4000.0);
    compare("parallel, threshold -25", &g[10..], &[-0.67, -1.31, -1.86, -2.31, -2.69, -3.01, -3.27, -3.48, -3.65, -3.79, -3.89, -4.07, -4.28, -4.45, -4.61, -4.75], 0.1);
    // The middle band at 1 kHz, all bands at their defaults: -12 / -6 / 0 dBFS.
    let mut m = module();
    m.set_param(P_MODE, MODE_PARALLEL);
    compare("parallel, 1 kHz defaults", &coarse(&mut m, 1000.0)[6..], &[-0.36, -1.46, -2.61], 0.1);
    m.set_param(P_WET_GAIN, -10.0);
    m.reset();
    compare("parallel, 1 kHz defaults, wet -10", &coarse(&mut m, 1000.0)[6..], &[-3.81, -4.32, -4.8], 0.1);
}

fn two_levels(freq: f64) -> (Vec<f32>, Vec<f32>) {
    let n = 6 * SRU;
    let s: Vec<f64> = (0..n).map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / SR as f64).sin()).collect();
    (s.iter().map(|v| (v * lin(-6.0)) as f32).collect(), s.iter().map(|v| (v * lin(-30.0)) as f32).collect())
}

fn lr_gain(m: &mut BandCompressor, l: &[f32], r: &[f32]) -> (f64, f64) {
    let (a, b) = render(m, l, r);
    let (i0, i1) = (SRU / 2, SRU * 11 / 2);
    (db(rms(&a[i0..i1])) - db(rms(&l[i0..i1])), db(rms(&b[i0..i1])) - db(rms(&r[i0..i1])))
}

/// Sections 4.4 and 9: one linked gain for both channels in every mode except parallel
/// unlinked, where each channel has its own detector. Tolerance 0.1 dB.
#[test]
fn stereo_link_per_mode() {
    // 4 kHz, left -6 / right -30 dBFS, high-mid threshold -10, Inf:1, reference -8.
    let (l, r) = two_levels(4000.0);
    for (name, mode, want) in [("downward", MODE_DOWNWARD, (-7.59, -7.59)), ("parallel", MODE_PARALLEL, (-3.06, -3.06)), ("parallel unlinked", MODE_PARALLEL_UNLINKED, (-3.06, 0.0))] {
        let got = lr_gain(&mut himid(-10.0, RATIO_INFINITE, mode), &l, &r);
        println!("4 kHz L -6 / R -30, {name}: {:.2} / {:.2} (measured {:.2} / {:.2})", got.0, got.1, want.0, want.1);
        assert!((got.0 - want.0).abs() < 0.1 && (got.1 - want.1).abs() < 0.1);
    }
    // 1 kHz, all bands at their defaults, reference -24.
    let (l, r) = two_levels(1000.0);
    for (name, mode, want) in [("downward", MODE_DOWNWARD, (-11.62, -11.62)), ("parallel", MODE_PARALLEL, (-4.04, -4.04)), ("parallel unlinked", MODE_PARALLEL_UNLINKED, (-4.03, -0.08)), ("lift", MODE_LIFT, (-2.62, -2.62))] {
        let mut m = module();
        m.set_param(P_REFERENCE, -24.0);
        m.set_param(P_MODE, mode);
        let got = lr_gain(&mut m, &l, &r);
        println!("1 kHz L -6 / R -30, ref -24, {name}: {:.2} / {:.2} (measured {:.2} / {:.2})", got.0, got.1, want.0, want.1);
        assert!((got.0 - want.0).abs() < 0.15 && (got.1 - want.1).abs() < 0.15);
    }
}

/// Section 4.4, last stereo figures: decorrelated pink noise at -20 dBFS, all defaults,
/// reference -24: downward -5.02 / -5.03 dB, parallel -2.20 / -2.20, parallel unlinked
/// -2.09 / -2.06. The noise here has the same spectrum and level but comes from another random
/// generator. This module reduces noise about 1 dB more than the original in downward mode
/// (REPORT.md); tolerance 1.3 dB downward, 0.6 dB parallel.
#[test]
fn pink_noise_per_mode() {
    let (l, r) = (pink(1), pink(2));
    for (name, mode, want, tol) in [("downward", MODE_DOWNWARD, (-5.02, -5.03), 1.3), ("parallel", MODE_PARALLEL, (-2.20, -2.20), 0.6), ("parallel unlinked", MODE_PARALLEL_UNLINKED, (-2.09, -2.06), 0.6)] {
        let mut m = module();
        m.set_param(P_REFERENCE, -24.0);
        m.set_param(P_MODE, mode);
        let got = lr_gain(&mut m, &l, &r);
        println!("pink noise, {name}: {:.2} / {:.2} (measured {:.2} / {:.2})", got.0, got.1, want.0, want.1);
        assert!((got.0 - want.0).abs() < tol && (got.1 - want.1).abs() < tol);
    }
    // Section 4.6: on programme the peak detector reduces more than RMS; crest 14 dB is almost
    // the same as RMS.
    let run = |det: f32| {
        let mut m = module();
        m.set_param(P_REFERENCE, -24.0);
        m.set_param(P_DETECTOR, det);
        lr_gain(&mut m, &l, &r).0
    };
    let (rms_gain, crest14, peak_gain) = (run(8.0), run(4.0), run(0.0));
    println!("pink noise, detector RMS {rms_gain:.2}, crest 14 dB {crest14:.2}, peak {peak_gain:.2} dB");
    assert!(peak_gain < rms_gain - 0.5);
    assert!((crest14 - rms_gain).abs() < 0.1);
}

/// Section 4.4, low-level lift: levels below the threshold are raised by a fixed amount, and
/// the gain falls towards and below 0 dB above it. The amount is set by the lift amount (the
/// original's per-band "steer"), not by the ratio. Tolerance 0.15 dB at the default amount.
#[test]
fn lift_static_curves() {
    let normal = [2.79, 2.18, 1.64, 1.16, 0.62, 0.1, -0.36, -0.76, -1.12]; // -16 .. 0 dBFS
    for ratio in [2.0, RATIO_INFINITE] {
        let g = fine(&mut himid(-10.0, ratio, MODE_LIFT), 4000.0);
        assert!(g[..17].iter().all(|v| (v - 3.07).abs() < 0.1), "below threshold: {:?}", &g[..17]);
        compare("lift, threshold -10", &g[17..], &normal, 0.1);
    }
    // Threshold -25 dB: +9.66 dB up to -32 dBFS (the table file prints 9.83 from another run).
    let g = fine(&mut himid(-25.0, 2.0, MODE_LIFT), 4000.0);
    compare("lift, threshold -25", &g[9..], &[9.66, 8.82, 7.78, 6.79, 5.86, 4.99, 4.19, 3.46, 2.79, 2.18, 1.64, 1.16, 0.63, 0.11, -0.35, -0.76, -1.12], 0.2);
    // The middle band at 1 kHz with all bands at their defaults: +2.35 dB below threshold.
    let mut m = module();
    m.set_param(P_MODE, MODE_LIFT);
    compare("lift, 1 kHz defaults", &coarse(&mut m, 1000.0), &[2.35, 2.35, 2.35, 2.35, 2.35, 2.35, 1.82, 0.27, -1.16], 0.15);
}

/// Section 4.5: lift amount. The original's steer positions OFF, -11, -5, Normal, 5, 10 gave
/// +0.49, +0.62, +1.49, +3.07, +4.46, +5.67 dB below the threshold and -0.04, -0.12, -0.51,
/// -1.12, -1.68, -2.13 dB at 0 dBFS (high-mid, threshold -10). Fitted lift amounts for those
/// positions: 0, 3, 19, 35, 44 and 50 %. Tolerance 0.1 dB on the whole curve
/// (`round2/jobs_s*.json`, "static fine | HiMid thr -10, mode DXP, steer ...").
#[test]
fn lift_amount() {
    // -16 .. 0 dBFS; below that the gain is constant at the first figure given.
    let rows: [(f32, f64, [f64; 9]); 6] = [
        (0.0, 0.49, [0.49, 0.49, 0.49, 0.5, 0.39, 0.26, 0.14, 0.04, -0.04]),
        (3.0, 0.62, [0.61, 0.58, 0.55, 0.54, 0.41, 0.25, 0.11, -0.02, -0.12]),
        (19.0, 1.49, [1.38, 1.15, 0.95, 0.76, 0.49, 0.19, -0.07, -0.3, -0.51]),
        (35.0, 3.07, [2.79, 2.18, 1.64, 1.16, 0.62, 0.1, -0.36, -0.76, -1.12]),
        (44.0, 4.46, [4.03, 3.1, 2.26, 1.52, 0.75, 0.02, -0.62, -1.19, -1.68]),
        (50.0, 5.67, [5.11, 3.9, 2.8, 1.82, 0.86, -0.04, -0.84, -1.53, -2.13]),
    ];
    for (amount, below, want) in rows {
        let mut m = himid(-10.0, 2.0, MODE_LIFT);
        set_band(&mut m, 3, B_LIFT, amount);
        let g = fine(&mut m, 4000.0);
        assert!(g[..17].iter().all(|v| (v - below).abs() < 0.1), "amount {amount}: below threshold {:.2} against {below}", g[5]);
        compare(&format!("lift amount {amount} %"), &g[17..], &want, 0.1);
    }
    // The lift amount does nothing in downward mode (section 4.5).
    let base = fine(&mut himid(-10.0, RATIO_INFINITE, MODE_DOWNWARD), 4000.0);
    let mut m = himid(-10.0, RATIO_INFINITE, MODE_DOWNWARD);
    set_band(&mut m, 3, B_LIFT, 50.0);
    m.set_param(P_WET_GAIN, -10.0);
    m.set_param(P_FLOOR_GUARD_RATIO, RATIO_INFINITE);
    let g = fine(&mut m, 4000.0);
    assert!(g.iter().zip(base.iter()).all(|(a, b)| (a - b).abs() < 1e-6), "lift, wet gain and floor guard must not act in downward mode");
}

/// Section 4.5, the floor guard (the original's "defeat"): only in lift mode; below reference
/// + guard threshold the lift is taken back, fully 10 dB below it at an infinite ratio. The
/// table file says "only the direction is established", so only the direction is asserted:
/// measured with threshold -30 dB (-38 dBFS), 1:Inf: 0.00 dB at -50 dBFS, +3.07 dB from -30 dBFS.
#[test]
fn floor_guard_takes_the_lift_back_at_low_levels() {
    let mut m = himid(-10.0, 2.0, MODE_LIFT);
    m.set_param(P_FLOOR_GUARD_THRESHOLD, -30.0);
    m.set_param(P_FLOOR_GUARD_RATIO, RATIO_INFINITE);
    let g = fine(&mut m, 4000.0);
    println!("floor guard -30 dB, infinite: -50 dBFS {:.2}, -44 {:.2}, -40 {:.2}, -30 {:.2}, 0 {:.2} (measured 0.00, 2.71, 3.06, 3.07, -1.12)", g[0], g[3], g[5], g[10], g[25]);
    assert!(g[0].abs() < 0.1, "the lift is gone at -50 dBFS");
    assert!(g[0] < g[3] && g[3] <= g[10] + 0.01);
    assert!((g[10] - 3.07).abs() < 0.15 && (g[25] + 1.12).abs() < 0.15);
    // Parallel mode: no effect.
    let base = fine(&mut himid(-25.0, RATIO_INFINITE, MODE_PARALLEL), 4000.0);
    let mut m = himid(-25.0, RATIO_INFINITE, MODE_PARALLEL);
    m.set_param(P_FLOOR_GUARD_THRESHOLD, -20.0);
    m.set_param(P_FLOOR_GUARD_RATIO, RATIO_INFINITE);
    let g = fine(&mut m, 4000.0);
    assert!(g.iter().zip(base.iter()).all(|(a, b)| (a - b).abs() < 1e-6));
}

/// Band bypass and solo. Bypass: the band passes unprocessed (no compression, no make-up).
/// Solo: only soloed bands are heard.
#[test]
fn bypass_and_solo() {
    let s = fine_steps(4000.0);
    let mut m = himid(-20.0, RATIO_INFINITE, MODE_DOWNWARD);
    for b in 0..BANDS {
        set_band(&mut m, b, B_BYPASS, 1.0);
        set_band(&mut m, b, B_MAKEUP, 6.0);
    }
    let g = step_gains(&render_mono(&mut m, &s), &s, 0.6, 0.95);
    assert!(g.iter().all(|v| v.abs() < 1e-3), "all bands bypassed: {:?}", g);
    // Only the high-mid band bypassed: what remains is the neighbours' default compression.
    let mut m = himid(-20.0, RATIO_INFINITE, MODE_DOWNWARD);
    set_band(&mut m, 3, B_BYPASS, 1.0);
    let g = step_gains(&render_mono(&mut m, &s), &s, 0.6, 0.95);
    println!("high-mid bypassed, 0 dBFS: {:.2} dB", g[25]);
    assert!(g[25] > -1.0 && g[25] < 0.1);
    // Solo of two bands equals the sum of the two single solos.
    let x = pink(3);
    let x = &x[..SRU];
    let solo = |bands: &[usize]| {
        let mut m = module();
        nothing_compresses(&mut m);
        for b in bands {
            set_band(&mut m, *b, B_SOLO, 1.0);
        }
        render_mono(&mut m, x)
    };
    let (a, b, both) = (solo(&[1]), solo(&[3]), solo(&[1, 3]));
    let err: Vec<f32> = (0..x.len()).map(|i| both[i] - a[i] - b[i]).collect();
    assert!(db(rms(&err)) < -120.0);
    assert!(db(rms(&a)) < db(rms(x)) - 1.0);
}
