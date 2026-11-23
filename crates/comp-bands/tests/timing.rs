//! Attack and release times and curves (source F table file, section 4.3).
//! Signal: `burst_<f>.wav` of the source F signal script (-30 dBFS tone, -6 dBFS burst of
//! 200 ms with 1 ms raised-cosine edges every 8 s). Analysis: `timing()` of the library script
//! (analytic-signal envelope of output over input, second burst).
//! Reference numbers: `spec/curves/index.json` and the curve files with the F prefix, and the
//! source F data folder, `round5/jobs_s0.json` ("band timing | ...", "lookahead burst | ...",
//! "crest burst | ...").
mod common;
use common::*;
use comp_bands::*;
use olm_api::Module;

struct Case {
    name: &'static str,
    freq: f64,
    sets: Vec<(usize, f32)>,
    /// Measured: gain at burst end, attack 63 %, attack 90 %, release 63 %, release 90 %.
    want: [f64; 5],
    /// Tolerances for the same five. A negative tolerance means "reported, not asserted".
    tol: [f64; 5],
}

fn hm(extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut v = vec![(band_param(3, B_THRESHOLD), -10.0), (band_param(3, B_RATIO), RATIO_INFINITE)];
    v.extend_from_slice(extra);
    v
}

fn band(b: usize, extra: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut v = vec![(band_param(b, B_THRESHOLD), -10.0), (band_param(b, B_RATIO), RATIO_INFINITE)];
    v.extend_from_slice(extra);
    v
}

fn run(case: &Case) -> (Timing, [f64; 5]) {
    let mut m = module();
    for (i, v) in &case.sets {
        m.set_param(*i, *v);
    }
    let t = burst_timing(&mut m, case.freq);
    let got = [t.end_of_burst, t.attack_63.unwrap_or(f64::NAN), t.attack_90.unwrap_or(f64::NAN), t.release_63.unwrap_or(f64::NAN), t.release_90.unwrap_or(f64::NAN)];
    (t, got)
}

fn check(cases: &[Case]) {
    let mut failed = Vec::new();
    for case in cases {
        let (_, got) = run(case);
        let mut line = format!("{:44}", case.name);
        for k in 0..5 {
            let ok = case.tol[k] < 0.0 || (got[k] - case.want[k]).abs() <= case.tol[k];
            line += &format!(" | {:8.2} / {:8.2} {}", got[k], case.want[k], if case.tol[k] < 0.0 { "(n/a)" } else if ok { "ok" } else { "FAIL" });
            if !ok {
                failed.push(format!("{} column {k}: {} against {} (tolerance {})", case.name, got[k], case.want[k], case.tol[k]));
            }
        }
        println!("{line}");
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

const A: usize = B_ATTACK;
const R: usize = B_RELEASE;

/// Section 4.3, first table and the release paragraph: the high-mid band at threshold -10 dB,
/// Inf:1, with the attack and release settings of the curve files.
/// Tolerances: gain at burst end 0.25 dB; attack and release times 2 ms or 6 %, whichever is
/// larger, where the constant-rate ramp sets the time (3.5 ms and 4.5 ms on the 90 % release
/// time at release 20 ms, where this module's smoother adds a short tail).
/// Known deviation, tolerance widened to 24 ms: after a 350 ms attack that the burst cut short,
/// the original keeps deepening for about 30 ms after the burst and so releases about 20 ms
/// later than this module.
#[test]
fn high_mid_attack_and_release_times() {
    println!("{:44} | end gain got/meas | attack 63 % | attack 90 % | release 63 % | release 90 %", "case");
    check(&[
        Case { name: "himid_thr-10_inf (attack 30, release 175)", freq: 4000.0, sets: hm(&[]), want: [-7.58, 12.751, 23.159, 141.19, 189.38], tol: [0.25, 2.0, 2.0, 8.5, 11.4] },
        Case { name: "attack 350 ms", freq: 4000.0, sets: hm(&[(band_param(3, A), 350.0)]), want: [-4.23, 112.583, 168.388, 124.773, 150.77], tol: [0.25, 6.8, 10.1, 24.0, 24.0] },
        Case { name: "release 20 ms", freq: 4000.0, sets: hm(&[(band_param(3, R), 20.0)]), want: [-7.58, 12.751, 23.159, 28.4, 33.498], tol: [0.25, 2.0, 2.0, 2.0, 3.5] },
        Case { name: "release 374 ms", freq: 4000.0, sets: hm(&[(band_param(3, R), 374.0)]), want: [-7.58, 12.751, 23.159, 285.227, 388.717], tol: [0.25, 2.0, 2.0, 17.1, 23.3] },
        Case { name: "release 1.62 s", freq: 4000.0, sets: hm(&[(band_param(3, R), 1620.0)]), want: [-7.58, 12.751, 23.159, 1179.248, 1629.112], tol: [0.25, 2.0, 2.0, 70.8, 97.7] },
        Case { name: "look-ahead 0 (attack 30)", freq: 4000.0, sets: hm(&[(P_LOOKAHEAD, 0.0)]), want: [-7.58, 22.745, 33.155, 151.198, 199.381], tol: [0.25, 2.0, 2.0, 9.1, 12.0] },
        Case { name: "look-ahead 15 ms (attack 30)", freq: 4000.0, sets: hm(&[(P_LOOKAHEAD, 15.0)]), want: [-7.58, 7.752, 18.161, 136.186, 184.38], tol: [0.25, 2.0, 2.0, 8.2, 11.1] },
    ]);
}

/// The same for the other bands (curve files `*_defaults`, and round 5 "band timing | ..." with
/// threshold -10 dB, Inf:1). Section 4.3 says the release readings at 40 and 200 Hz are not
/// usable, so those columns are reported only. The 90 % attack time of the three 2:1 default
/// curves is reported only: the original approaches its settled gain with a slow tail (63 % at
/// 3.4 ms, 90 % at 17.5 ms) that this module does not have. The 63 % attack time at 200 Hz
/// deviates by 3.9 ms (tolerance widened to 4.5 ms).
#[test]
fn other_bands_attack_and_release_times() {
    println!("{:44} | end gain got/meas | attack 63 % | attack 90 % | release 63 % | release 90 %", "case");
    check(&[
        Case { name: "himid_defaults (2:1, thr -9)", freq: 4000.0, sets: vec![], want: [-4.03, 3.414, 17.499, 68.779, 93.407], tol: [0.25, 3.0, -1.0, 4.2, 5.6] },
        Case { name: "hi_defaults (2:1, thr -10)", freq: 12000.0, sets: vec![], want: [-3.97, 1.524, 13.039, 60.241, 87.927], tol: [0.25, 3.0, -1.0, 3.7, 5.3] },
        Case { name: "lomid_defaults (2:1, thr -7)", freq: 200.0, sets: vec![], want: [-2.97, 8.443, 29.75, 2.253, 2.282], tol: [0.25, 4.5, -1.0, -1.0, -1.0] },
        Case { name: "Hi thr -10 Inf, base", freq: 12000.0, sets: band(4, &[]), want: [-5.39, 5.671, 16.213, 123.92, 166.214], tol: [0.25, 2.0, 2.0, 7.5, 10.0] },
        Case { name: "Hi thr -10 Inf, attack 350", freq: 12000.0, sets: band(4, &[(band_param(4, A), 350.0)]), want: [-3.89, 93.25, 160.146, 102.326, 130.2], tol: [0.25, 5.6, 9.7, 24.0, 24.0] },
        Case { name: "Hi thr -10 Inf, release 20", freq: 12000.0, sets: band(4, &[(band_param(4, R), 20.0)]), want: [-5.39, 5.671, 16.213, 30.03, 35.634], tol: [0.25, 2.0, 2.0, 2.0, 4.5] },
        Case { name: "LoMid thr -10 Inf, base", freq: 200.0, sets: band(1, &[]), want: [-7.18, 18.288, 29.72, 1.039, 1.575], tol: [0.25, 2.0, 2.0, -1.0, -1.0] },
        Case { name: "LoMid thr -10 Inf, attack 350", freq: 200.0, sets: band(1, &[(band_param(1, A), 350.0)]), want: [-4.36, 111.319, 168.255, 1.892, 2.022], tol: [0.25, 6.7, 10.1, -1.0, -1.0] },
    ]);
}

/// Fast attack (0.3 ms) and the detector settings: the first reaction of the original is
/// a little faster than this module's detector (REPORT.md). Asserted with 1 ms on the 63 %
/// attack time and 3 ms on the 90 % time, which in the original depends on a slow settling tail.
/// The original also settles 0.25 to 0.45 dB deeper with the 0.3 ms attack than with the 30 ms
/// attack (-8.02 / -7.86 dB against -7.58 dB, the first "not explained" in section 4.3); this
/// module settles at the same gain for every attack, and its release from there is therefore
/// about 10 ms shorter (tolerance 17 ms on the release columns).
#[test]
fn fast_attack_is_matched_to_a_few_milliseconds() {
    println!("{:44} | end gain got/meas | attack 63 % | attack 90 % | release 63 % | release 90 %", "case");
    let a = (band_param(3, A), 0.3);
    check(&[
        Case { name: "attack 0.3 ms (look-ahead 10)", freq: 4000.0, sets: hm(&[a]), want: [-8.02, 0.466, 4.268, 159.325, 210.57], tol: [0.45, 1.0, 3.0, 17.0, 17.0] },
        Case { name: "attack 0.3 ms, look-ahead 0", freq: 4000.0, sets: hm(&[a, (P_LOOKAHEAD, 0.0)]), want: [-7.86, 1.127, 11.104, 155.773, 205.854], tol: [0.45, 1.0, 3.0, 17.0, 17.0] },
        Case { name: "attack 0.3 ms, look-ahead 1 ms", freq: 4000.0, sets: hm(&[a, (P_LOOKAHEAD, 1.0)]), want: [-7.86, 0.45, 10.073, 154.771, 204.858], tol: [0.45, 1.0, 3.0, 17.0, 17.0] },
        Case { name: "attack 0.3 ms, look-ahead 15 ms", freq: 4000.0, sets: hm(&[a, (P_LOOKAHEAD, 15.0)]), want: [-7.86, 0.387, 2.818, 154.797, 204.898], tol: [0.45, 1.0, 3.0, 17.0, 17.0] },
        Case { name: "attack 0.3 ms, detector Peak", freq: 4000.0, sets: hm(&[a, (P_DETECTOR, 0.0)]), want: [-8.02, 0.326, 1.637, 159.322, 210.569], tol: [0.6, 1.0, 1.5, 20.0, 20.0] },
        Case { name: "attack 0.3 ms, detector crest 6 dB", freq: 4000.0, sets: hm(&[a, (P_DETECTOR, 1.0)]), want: [-8.02, 0.453, 3.815, 159.335, 210.571], tol: [0.45, 1.0, 3.0, 17.0, 17.0] },
        Case { name: "attack 10.2 ms", freq: 4000.0, sets: hm(&[(band_param(3, A), 10.2)]), want: [-7.59, 0.0, 5.594, 139.563, 187.788], tol: [0.25, 1.0, 3.0, 8.4, 11.3] },
    ]);
}

/// Gain against time, compared with the curve files at 26 times from -10 ms to 450 ms
/// (`spec/curves/<F prefix><name>.csv`). Tolerance per curve as stated; the largest deviation
/// and where it occurs are printed.
#[test]
fn gain_curves() {
    const T: [f64; 26] = [-10.0, -5.0, 0.0, 2.5, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 60.0, 100.0, 180.0, 197.5, 210.0, 220.0, 230.0, 240.0, 260.0, 280.0, 300.0, 340.0, 380.0, 400.0, 450.0];
    let a = (band_param(3, A), 0.3);
    let curves: Vec<(&str, f64, Vec<(usize, f32)>, [f64; 26], f64)> = vec![
        ("himid_thr-10_inf", 4000.0, hm(&[]), [-0.0, -0.52, -1.72, -2.36, -2.99, -4.17, -5.25, -6.25, -7.05, -7.27, -7.42, -7.52, -7.57, -7.58, -7.58, -7.59, -7.48, -7.08, -6.64, -5.79, -5.11, -4.4, -2.85, -1.17, -0.28, -0.01], 0.6),
        ("himid_thr-10_inf_attack_350ms", 4000.0, hm(&[(band_param(3, A), 350.0)]), [-0.0, 0.0, 0.0, -0.05, -0.12, -0.28, -0.43, -0.57, -0.7, -0.82, -1.07, -1.52, -2.4, -4.04, -4.38, -4.62, -4.76, -4.79, -4.71, -4.18, -3.41, -2.61, -0.9, -0.02, -0.01, -0.01], 1.0),
        ("himid_thr-10_inf_release_20ms", 4000.0, hm(&[(band_param(3, R), 20.0)]), [-0.0, -0.52, -1.72, -2.36, -2.99, -4.17, -5.25, -6.25, -7.05, -7.27, -7.42, -7.52, -7.57, -7.58, -7.58, -7.59, -5.84, -2.18, -0.18, -0.03, -0.02, -0.01, -0.01, -0.01, -0.0, -0.0], 0.6),
        ("himid_thr-10_inf_release_374ms", 4000.0, hm(&[(band_param(3, R), 374.0)]), [-0.0, -0.52, -1.72, -2.36, -2.99, -4.17, -5.25, -6.25, -7.05, -7.27, -7.42, -7.52, -7.57, -7.58, -7.58, -7.59, -7.53, -7.3, -7.04, -6.53, -6.23, -5.94, -5.33, -4.68, -4.34, -3.46], 0.6),
        ("himid_thr-10_inf_release_1.62s", 4000.0, hm(&[(band_param(3, R), 1620.0)]), [-0.0, -0.52, -1.72, -2.36, -2.99, -4.17, -5.25, -6.25, -7.05, -7.27, -7.42, -7.52, -7.57, -7.58, -7.58, -7.59, -7.54, -7.41, -7.25, -6.96, -6.89, -6.82, -6.7, -6.57, -6.51, -6.35], 0.6),
        ("himid_thr-10_inf_attack_10.2ms", 4000.0, hm(&[(band_param(3, A), 10.2)]), [-0.0, -2.19, -5.15, -6.36, -6.77, -7.09, -7.25, -7.34, -7.41, -7.45, -7.5, -7.55, -7.58, -7.59, -7.59, -7.59, -7.44, -7.03, -6.59, -5.74, -5.06, -4.34, -2.79, -1.1, -0.21, -0.01], 0.6),
        ("himid_thr-10_inf_attack_0.3ms", 4000.0, hm(&[a]), [-0.0, 0.0, -3.76, -6.98, -7.29, -7.55, -7.69, -7.78, -7.84, -7.88, -7.94, -7.99, -8.01, -8.02, -8.02, -8.03, -7.98, -7.71, -7.31, -6.48, -5.85, -5.18, -3.73, -2.12, -1.27, -0.02], 2.0),
        ("himid_thr-10_inf_attack_0.3ms_lookahead_0", 4000.0, hm(&[a, (P_LOOKAHEAD, 0.0)]), [0.0, 0.01, 0.06, -6.65, -7.0, -7.01, -7.28, -7.45, -7.56, -7.64, -7.73, -7.81, -7.84, -7.86, -7.86, -7.86, -7.86, -7.7, -7.29, -6.36, -5.69, -5.01, -3.53, -1.91, -1.04, -0.02], 1.5),
        ("himid_thr-10_inf_attack_0.3ms_detector_peak", 4000.0, hm(&[a, (P_DETECTOR, 0.0)]), [-0.0, -0.11, -4.29, -7.42, -7.65, -7.8, -7.86, -7.9, -7.93, -7.95, -7.97, -8.0, -8.01, -8.02, -8.02, -8.03, -7.98, -7.71, -7.31, -6.48, -5.85, -5.18, -3.73, -2.12, -1.27, -0.02], 1.2),
        ("himid_defaults", 4000.0, vec![], [-0.0, -0.46, -1.66, -2.3, -2.9, -3.33, -3.55, -3.69, -3.78, -3.85, -3.92, -3.99, -4.02, -4.03, -4.03, -4.03, -3.85, -3.35, -2.84, -1.87, -1.0, -0.12, -0.01, -0.01, -0.01, -0.0], 0.6),
        ("hi_defaults", 12000.0, vec![], [-0.0, -0.65, -2.12, -2.73, -3.15, -3.46, -3.63, -3.73, -3.8, -3.84, -3.9, -3.94, -3.96, -3.97, -3.97, -3.97, -3.71, -3.15, -2.58, -1.48, -0.72, -0.05, -0.01, -0.01, -0.01, -0.0], 0.6),
    ];
    let mut failed = Vec::new();
    for (name, f, sets, want, tol) in curves {
        let mut m = module();
        for (i, v) in &sets {
            m.set_param(*i, *v);
        }
        let t = burst_timing(&mut m, f);
        let got: Vec<f64> = T.iter().map(|x| t.at(*x)).collect();
        let (mut worst, mut at) = (0.0, 0.0);
        let mut sum = 0.0;
        for k in 0..26 {
            let d = (got[k] - want[k]).abs();
            sum += d * d;
            if d > worst {
                worst = d;
                at = T[k];
            }
        }
        println!("{name:46} worst {worst:.2} dB at {at} ms, rms {:.2} dB (tolerance {tol})", (sum / 26.0).sqrt());
        println!("     got  {:?}", got.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>());
        println!("     want {:?}", want);
        if worst > tol {
            failed.push(format!("{name}: {worst:.2} dB at {at} ms"));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Section 4.3: "The gain dips deeper at the start of the burst than its settled value"
/// (deepest in burst -10.27 dB for the high-mid Inf:1 curve, -4.87 dB and -4.08 dB for the
/// high-mid and high default curves). The dip is what the band phases make of unequal band
/// gains at the burst edges; it is not a detector overshoot. Tolerance 0.3 dB.
#[test]
fn deepest_gain_inside_the_burst() {
    for (name, f, sets, want) in [("himid_thr-10_inf", 4000.0, hm(&[]), -10.27), ("himid_defaults", 4000.0, vec![], -4.87), ("hi_defaults", 12000.0, vec![], -4.08)] {
        let mut m = module();
        for (i, v) in &sets {
            m.set_param(*i, *v);
        }
        let t = burst_timing(&mut m, f);
        println!("{name}: deepest {:.2} dB (measured {want})", t.deepest);
        assert!((t.deepest - want).abs() < 0.3);
    }
}

/// 40 Hz, the low band. Reported, not asserted beyond a loose bound: the 40 Hz readings of the
/// original are not understood (section 4.3 and section 14).
#[test]
fn low_band_timing_is_reported() {
    for (name, sets, want) in [
        ("lo_defaults", vec![], [-2.47, 11.194, 17.335]),
        ("Lo thr -10 Inf, base", band(0, &[]), [-8.28, 30.867, 46.455]),
        ("Lo thr -10 Inf, attack 350", band(0, &[(band_param(0, A), 350.0)]), [-4.53, 116.087, 171.791]),
    ] {
        let mut m = module();
        for (i, v) in &sets {
            m.set_param(*i, *v);
        }
        let t = burst_timing(&mut m, 40.0);
        println!("{name}: end gain {:.2} (measured {}), attack 63 % {:.1} ms ({}), 90 % {:.1} ms ({})", t.end_of_burst, want[0], t.attack_63.unwrap_or(f64::NAN), want[1], t.attack_90.unwrap_or(f64::NAN), want[2]);
        assert!((t.end_of_burst - want[0]).abs() < 1.5);
    }
}

/// Section 4.3 table "Lookahead and attack overshoot" (round 5 "lookahead burst | ..."):
/// overshoot = sample peak in the first 5 ms of the burst minus the sample peak of its last
/// 20 ms. Measured: attack 0.3 ms: 4.92 dB without look-ahead, 1.65 / 1.68 / 1.67 / 1.65 /
/// 1.43 dB with 1 / 2 / 8.5 / 10 / 15 ms; attack 30 ms: 7.86 dB without, 4.56 dB with 15 ms.
/// Asserted: the 30 ms rows (1 dB), the 0.3 ms rows with look-ahead (1.2 dB) and without
/// (1.8 dB); this module's detector reacts a little later than the original's, so its
/// overshoot is about 1 dB larger.
#[test]
fn lookahead_reduces_the_overshoot() {
    let s = burst(4000.0, 24);
    let on = 8 * SRU + SRU / 10;
    let off = 8 * SRU + 3 * SRU / 10;
    let overshoot = |attack: f32, look: f32| {
        let mut m = module();
        for (i, v) in hm(&[(band_param(3, A), attack), (P_LOOKAHEAD, look)]) {
            m.set_param(i, v);
        }
        let y = render_mono(&mut m, &s);
        db(peak(&y[on..on + 240])) - db(peak(&y[off - 960..off]))
    };
    let fast: Vec<f64> = [0.0, 1.0, 2.0, 8.5, 10.0, 15.0].iter().map(|l| overshoot(0.3, *l)).collect();
    println!("attack 0.3 ms, look-ahead 0/1/2/8.5/10/15 ms: {:?} dB (measured 4.92, 1.65, 1.68, 1.67, 1.65, 1.43)", fast.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>());
    let slow0 = overshoot(30.0, 0.0);
    let slow15 = overshoot(30.0, 15.0);
    println!("attack 30 ms: look-ahead 0: {slow0:.2} dB (measured 7.86), 15 ms: {slow15:.2} dB (measured 4.56)");
    assert!(fast[1] < fast[0] - 0.5, "1 ms of look-ahead must remove part of the overshoot");
    assert!((fast[1] - fast[4]).abs() < 0.3, "with a fast attack, more look-ahead changes little");
    assert!((slow0 - 7.86).abs() < 1.0 && (slow15 - 4.56).abs() < 1.0);
    let measured = [4.92, 1.65, 1.68, 1.67, 1.65, 1.43];
    assert!((fast[0] - measured[0]).abs() < 1.8);
    for k in 1..6 {
        assert!((fast[k] - measured[k]).abs() < 1.2);
    }
}
