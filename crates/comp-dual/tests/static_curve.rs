//! Static curve, knee, ratio table and auto make-up against the stored tables.
mod common;
use common::*;
use comp_dual::{auto_makeup_db, knee_half_width_db, static_gain_db};

/// `static_steps.wav` as used by jobs2.py: 1 kHz, nine one-second steps from -48 to 0 dBFS
/// peak; the gain of each step is the RMS change between 0.5 and 0.9 s into it.
fn static_steps() -> Vec<f32> {
    (0..9 * SR)
        .map(|i| {
            let k = i / SR;
            (lin(-48.0 + 6.0 * k as f64) * (PI2 * 1000.0 * i as f64 / SRF).sin()) as f32
        })
        .collect()
}

fn static_row(settings: &[(&str, f32)]) -> Vec<f64> {
    let x = static_steps();
    let mut m = with(settings);
    let y = render(&mut m, &x);
    (0..9).map(|k| lvl(&y, &x, k as f64 + 0.5, k as f64 + 0.9)).collect()
}

#[test]
fn static_curves_match_stored_rows() {
    // measurements/srcB_features/srcB_static_and_release_grid.json, "static" (spec section 24).
    // Attack and release at their defaults, as in the measurement, so the 2.1 dB offset between
    // the peak level and the detector level is produced by the follower, not put in by hand.
    let rows: [(&str, Vec<(&str, f32)>, [f64; 9], f64); 5] = [
        ("thr -30, 2:1, knee 100", vec![("threshold1", -30.0)], [-0.87, -1.66, -2.70, -3.98, -5.52, -7.31, -9.35, -11.64, -14.17], 0.2),
        (
            "thr -30, 1000:1, knee 100",
            vec![("threshold1", -30.0), ("ratio1", 1000.0)],
            [-1.75, -3.32, -5.39, -7.96, -11.03, -14.60, -18.68, -23.25, -28.32],
            0.2,
        ),
        (
            "thr -30, 2:1, knee 0",
            vec![("threshold1", -30.0), ("softKnee1", 0.0)],
            [0.0, 0.0, 0.0, 0.0, -1.95, -4.95, -7.95, -10.95, -13.95],
            0.2,
        ),
        ("thr -48, 2:1, knee 100", vec![("threshold1", -48.0)], [-6.23, -7.76, -9.45, -11.31, -13.33, -15.52, -17.88, -20.41, -23.10], 0.2),
        // Upward ratio 1:2.40 = 0.41667. The last three cells are the safety limiter (on, -0.01 dBFS).
        (
            "thr -30, 1:2.40, knee 100",
            vec![("threshold1", -30.0), ("ratio1", 1.0 / 2.4)],
            [1.18, 2.38, 4.16, 6.65, 10.13, 15.04, 11.99, 5.99, -0.01],
            0.2,
        ),
    ];
    let mut t = Table::new("Static curve (srcB_static_and_release_grid.json)");
    for (name, settings, want, tol) in rows.iter() {
        let got = static_row(settings);
        for k in 0..9 {
            t.level(&format!("{name}, in {} dBFS", -48 + 6 * k as i32), got[k], want[k], *tol);
        }
    }
    t.finish();
}

#[test]
fn auto_makeup_matches_stored_rows() {
    // measurements/jobs2/all_jobs.json, "srcB static | thr -30, ..., AUTO MAKE-UP" (spec section 37).
    let mut t = Table::new("Auto make-up (jobs2/all_jobs.json)");
    let got = static_row(&[("threshold1", -30.0), ("softKnee1", 0.0), ("autoMakeup", 1.0)]);
    let want = [15.0, 15.0, 15.0, 15.0, 13.05, 10.05, 7.05, 4.05, -0.01];
    for k in 0..9 {
        t.level(&format!("2:1, in {} dBFS", -48 + 6 * k as i32), got[k], want[k], 0.2);
    }
    let got = static_row(&[("threshold1", -30.0), ("softKnee1", 0.0), ("ratio1", 1000.0), ("autoMakeup", 1.0)]);
    let want = [29.97, 29.97, 29.97, 29.97, 24.0, 18.0, 12.0, 6.0, 0.0];
    for k in 0..9 {
        t.level(&format!("1000:1, in {} dBFS", -48 + 6 * k as i32), got[k], want[k], 0.2);
    }
    t.level("law: -thr x (1 - 1/ratio), 2:1", auto_makeup_db(-30.0, 2.0), 15.0, 0.01);
    t.level("law: 1000:1", auto_makeup_db(-30.0, 1000.0), 29.97, 0.01);
    t.finish();
}

#[test]
fn knee_and_ratio_laws() {
    let mut t = Table::new("Knee and ratio laws (spec 24, 37; params table ratio range)");
    // Full knee: half-width 36 dB at -30 and 54 dB at -48 (second differences of the stored rows).
    t.level("knee half-width at thr -30", knee_half_width_db(-30.0, 100.0), 36.0, 0.01);
    t.level("knee half-width at thr -48", knee_half_width_db(-48.0, 100.0), 54.0, 0.01);
    t.level("knee 0 is hard", knee_half_width_db(-30.0, 0.0), 0.0, 0.0);
    // Stored row "thr -30, 2:1, knee 100" expressed on the detector level (2.1 dB under the peak).
    for (inp, want) in [(-48.0, -0.87), (-30.0, -3.98), (-12.0, -9.35), (0.0, -14.17)] {
        t.level(&format!("curve formula at {inp}"), static_gain_db(inp - 2.1, -30.0, 36.0, 2.0), want, 0.05);
    }
    // Ratio table, hard knee, 20 dB over the threshold: gain = -(1 - 1/ratio) x 20.
    for (ratio, want) in [(1.0, 0.0), (1.5, -6.667), (2.0, -10.0), (4.0, -15.0), (10.0, -18.0), (1000.0, -19.98)] {
        t.level(&format!("ratio {ratio}:1, 20 dB over"), static_gain_db(-10.0, -30.0, 0.0, ratio), want, 0.01);
    }
    // Upward ratios 1:x, hard knee, 5 dB over: output is x times as far over, gain = (x - 1) x 5.
    for (x, want) in [(2.4, 7.0), (5.0, 20.0)] {
        t.level(&format!("ratio 1:{x}, 5 dB over"), static_gain_db(-25.0, -30.0, 0.0, 1.0 / x), want, 0.01);
    }
    // Below a hard knee nothing happens, for any ratio.
    for ratio in [0.2, 1.0, 1000.0] {
        t.level(&format!("ratio {ratio}, below threshold"), static_gain_db(-40.0, -30.0, 0.0, ratio), 0.0, 0.0);
    }
    // Monotonic and continuous across the knee joints for a soft-knee upward ratio.
    let mut prev = static_gain_db(-100.0, -30.0, 36.0, 0.25);
    let mut ok = true;
    let mut l = -100.0;
    while l < 20.0 {
        let g = static_gain_db(l, -30.0, 36.0, 0.25);
        if g < prev - 1.0e-9 || g - prev > 0.5 {
            ok = false;
        }
        prev = g;
        l += 0.05;
    }
    t.truth("upward soft knee is monotonic and continuous", ok, "0.05 dB steps, -100 to +20 dBFS");
    t.finish();
}

#[test]
fn default_settings_are_transparent_up_to_minus_6() {
    // spec section 6.2 with the label correction of section 35: 0.00 dB from -48 to -6 dBFS.
    let got = static_row(&[]);
    let mut t = Table::new("Default settings (spec 6.2)");
    for k in 0..8 {
        t.level(&format!("in {} dBFS", -48 + 6 * k as i32), got[k], 0.0, 0.02);
    }
    t.finish();
}
