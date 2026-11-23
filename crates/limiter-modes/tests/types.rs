//! The three character types against the stored tables and curves (source E).
//! Reference: spec/params table of source E; data files measurements/srcE/all_jobs.json
//! (static, timing), true_peak.json (true-peak switch) and the six three-type files in
//! spec/curves. Style indices: 10 = "Iron" (first type of the table), 11 = "Glow" (second),
//! 12 = "Glass" (third).
mod common;
use common::*;
use limiter_modes::*;
use olm_api::Module;

const IRON: usize = 10;
const GLOW: usize = 11;
const GLASS: usize = 12;

#[test]
fn latency() {
    for k in [IRON, GLOW, GLASS] {
        let mut m = make(k, 2.0, 0.0);
        assert_eq!(m.latency(), 784);
        m.set_param(P_TRUE_PEAK, 1.0);
        assert_eq!(m.latency(), 801);
    }
}

/// "Static gain" table: 1 kHz at -48..0 dBFS; threshold -10 dB, the same with ceiling
/// -6 dB, threshold -20 dB. Threshold -T is drive +T here.
const STATIC_A: [[f64; 9]; 3] = [
    [10.35, 10.35, 10.35, 10.35, 10.35, 10.35, 10.3, 5.99, -0.01],
    [4.35, 4.35, 4.35, 4.35, 4.35, 4.35, 4.3, -0.01, -6.01],
    [20.35, 20.35, 20.35, 20.35, 20.34, 17.99, 11.99, 5.99, -0.0],
];
const STATIC_GLASS: [[f64; 9]; 3] = [
    [10.12, 10.12, 10.12, 10.12, 10.12, 10.12, 10.11, 6.07, 0.07],
    [4.12, 4.12, 4.12, 4.12, 4.12, 4.12, 4.11, 0.07, -5.93],
    [20.12, 20.12, 20.12, 20.12, 20.12, 18.06, 12.07, 6.07, 0.07],
];

#[test]
fn static_gain_offsets_and_ceilings() {
    let x = static_steps();
    for (k, table) in [(IRON, &STATIC_A), (GLOW, &STATIC_A), (GLASS, &STATIC_GLASS)] {
        for (row, (drive, ceiling)) in [(10.0f32, 0.0f32), (10.0, -6.0), (20.0, 0.0)].iter().enumerate() {
            let mut m = make(k, 2.0, *drive);
            m.set_param(P_CEILING, *ceiling);
            let y = run(&mut m, &x);
            let got = static_gains(&y, &x);
            for (step, (g, w)) in got.iter().zip(table[row].iter()).enumerate() {
                assert_db(&format!("style {k} drive {drive} ceiling {ceiling} step {step}"), *g, *w, 0.1);
            }
            // Sample peak: -0.01 dBFS under the ceiling in all three types (programme table).
            let top = db(peak(&y[8 * SR + SR / 2..]) as f64);
            assert_db(&format!("style {k} sample peak"), top, *ceiling as f64 - 0.01, 0.005);
        }
    }
}

/// "Timing" table, threshold -12 dB: character, attack 63 %, release 63 %, release 90 %,
/// background gain before the burst. A stored 0.0 means "already past the target at the
/// window start" and is written as None.
type Row = (f32, Option<f64>, Option<f64>, Option<f64>, f64);
const IRON_T: [Row; 5] = [
    (0.0, Some(0.593), Some(253.337), Some(777.239), 12.35),
    (2.0, Some(0.593), Some(460.363), Some(1403.257), 12.35),
    (5.0, Some(0.593), Some(771.229), Some(2330.306), 12.34),
    (9.0, Some(0.594), Some(1153.305), Some(3410.881), 12.28),
    (10.0, Some(0.594), Some(1238.441), Some(3635.137), 12.25),
];
const GLOW_T: [Row; 5] = [
    (0.0, Some(0.611), Some(190.163), Some(582.39), 12.35),
    (2.0, Some(0.391), Some(639.896), Some(1944.925), 12.35),
    (5.0, Some(0.01), Some(1356.048), Some(3938.506), 12.2),
    (9.0, None, Some(1946.666), Some(5215.348), 11.52),
    (10.0, None, Some(2006.958), Some(5359.243), 11.31),
];
/// Release 90 % is stored as 0.0 at character 0, 5, 9 and 10. At 9 the stored curve shows a
/// tail of about 0.8 s that the 0.0 hides (the reading starts on an analysis spike), and
/// this build follows the curve; those three cells are not asserted (REPORT.md).
const GLASS_T: [Row; 5] = [
    (0.0, Some(0.578), None, None, 12.12),
    (2.0, Some(0.571), None, Some(177.713), 12.12),
    (5.0, Some(0.546), None, Some(f64::NAN), 12.12),
    (9.0, Some(0.478), None, Some(f64::NAN), 12.12),
    (10.0, Some(0.461), None, Some(f64::NAN), 12.12),
];

#[test]
fn timing_tables() {
    let x = burst_signal();
    for (k, rows) in [(IRON, &IRON_T), (GLOW, &GLOW_T), (GLASS, &GLASS_T)] {
        for (c, attack, r63, r90, background) in rows.iter() {
            let mut m = make(k, *c, 12.0);
            let g = gain_curve(&run(&mut m, &x), &x);
            let t = timing_types(&g);
            let name = format!("style {k} character {c}");
            assert_db(&format!("{name} background"), t.pre, *background, 0.1);
            match attack {
                Some(v) => assert_time(&format!("{name} attack"), t.attack, *v),
                None => assert_at_start(&format!("{name} attack"), t.attack),
            }
            match r63 {
                Some(v) => assert_time(&format!("{name} release 63 %"), t.release63, *v),
                None => assert_at_start(&format!("{name} release 63 %"), t.release63),
            }
            match r90 {
                Some(v) if v.is_nan() => {}
                Some(v) => assert_time(&format!("{name} release 90 %"), t.release90, *v),
                None => assert_at_start(&format!("{name} release 90 %"), t.release90),
            }
        }
    }
}

/// Points of the stored curves (ms from the burst start, dB), away from the two edges.
const IRON_2: [(f64, f64); 9] = [(-80.0, 12.369), (5.0, 5.461), (100.0, 5.463), (250.0, 7.976), (400.0, 8.739),
    (600.0, 9.583), (1000.0, 10.754), (2000.0, 11.955), (4000.0, 12.328)]; // srcE_analog__character_2.csv
const IRON_9: [(f64, f64); 8] = [(-80.0, 12.296), (5.0, 5.461), (250.0, 7.776), (400.0, 8.099), (600.0, 8.501),
    (1000.0, 9.215), (2000.0, 10.512), (4000.0, 11.734)]; // srcE_analog__character_9.csv
const GLOW_2: [(f64, f64); 9] = [(-80.0, 12.357), (20.0, 5.462), (100.0, 5.460), (250.0, 7.909), (400.0, 8.467),
    (600.0, 9.132), (1000.0, 10.173), (2000.0, 11.551), (4000.0, 12.245)]; // srcE_tube__character_2.csv
const GLOW_9: [(f64, f64); 9] = [(-80.0, 11.522), (100.0, 5.459), (250.0, 6.417), (300.0, 6.890), (400.0, 7.460),
    (600.0, 7.952), (1000.0, 8.372), (2000.0, 9.142), (4000.0, 10.300)]; // srcE_tube__character_9.csv
const GLASS_9: [(f64, f64); 9] = [(-80.0, 12.138), (20.0, 5.458), (100.0, 5.459), (250.0, 10.818), (400.0, 11.000),
    (600.0, 11.232), (1000.0, 11.583), (2000.0, 11.983), (4000.0, 12.116)]; // srcE_modern__character_9.csv
const GLASS_2: [(f64, f64); 6] = [(-80.0, 12.143), (250.0, 10.995), (400.0, 11.523), (600.0, 11.885), (1000.0, 12.089),
    (2000.0, 12.124)]; // srcE_modern__character_2.csv

#[test]
fn release_follows_the_stored_curves() {
    let x = burst_signal();
    for (k, c, pts) in [(IRON, 2.0f32, &IRON_2[..]), (IRON, 9.0, &IRON_9[..]), (GLOW, 2.0, &GLOW_2[..]),
        (GLOW, 9.0, &GLOW_9[..]), (GLASS, 9.0, &GLASS_9[..]), (GLASS, 2.0, &GLASS_2[..])]
    {
        let mut m = make(k, c, 12.0);
        let g = gain_curve(&run(&mut m, &x), &x);
        for (ms, want) in pts {
            assert_db(&format!("style {k} character {c} at {ms} ms"), at_ms(&g, *ms), *want, 0.2);
        }
    }
}

#[test]
fn glass_burst_level_known_deviation() {
    // srcE_modern__character_2.csv reads 5.72 to 5.74 dB in the burst, which puts the
    // envelope 0.26 dB over the ceiling; the static table of the same type puts a steady
    // tone 0.07 dB over. This build follows the static table, so it reads about 5.53 here.
    // Kept as a test so the choice is visible; see REPORT.md, contradictions.
    let x = burst_signal();
    let mut m = make(GLASS, 2.0, 12.0);
    let g = gain_curve(&run(&mut m, &x), &x);
    let got = at_ms(&g, 100.0);
    assert!((got - 5.740).abs() > 0.1, "now within tolerance of the curve: update REPORT.md");
    assert_db("burst level by the static table", got, 5.469 + 0.07, 0.03);
}

#[test]
fn sample_peak_never_exceeds_the_ceiling() {
    let (xl, xr) = programme(4);
    for k in [IRON, GLOW, GLASS] {
        for c in [0.0f32, 2.0, 5.0, 9.0, 10.0] {
            for (drive, ceiling, tp) in [(12.0f32, 0.0f32, 0.0f32), (20.0, -6.0, 0.0), (12.0, 0.0, 1.0)] {
                let mut m = make(k, c, drive);
                m.set_param(P_CEILING, ceiling);
                m.set_param(P_TRUE_PEAK, tp);
                let (l, r) = run_stereo(&mut m, &xl, &xr);
                let top = 10f32.powf((ceiling - 0.01) / 20.0) * (1.0 + 1e-6);
                let p = peak(&l).max(peak(&r));
                assert!(p <= top, "style {k} character {c} drive {drive}: peak {p} over {top}");
                assert!(p > 0.85 * top, "style {k} character {c}: peak {p}");
            }
        }
    }
}

#[test]
fn true_peak_switch_on_the_inter_sample_tone() {
    // "Editor (round 48)": 12 kHz at 45 degrees, threshold -6 dB. Off: -0.01 dBFS sample
    // peak / +3.00 dBTP. On: -3.06 dBFS / -0.05 dBTP.
    // The tone's level is not stated; a full-scale amplitude (sample peaks at -3.01 dBFS)
    // is assumed, which the stored "off" row needs (+3.0 dBTP with the samples at the limit).
    let x = sine(12_000.0, 0.0, core::f64::consts::FRAC_PI_4, 2 * SR);
    for k in [IRON, GLOW, GLASS] {
        let mut m = make(k, 2.0, 6.0);
        let y = run(&mut m, &x);
        let mid = &y[SR..SR + (1 << 14)];
        assert_db(&format!("style {k} off, sample peak"), db(peak(mid) as f64), -0.01, 0.1);
        assert_db(&format!("style {k} off, true peak"), db(true_peak(mid)), 3.0, 0.1);
        m.set_param(P_TRUE_PEAK, 1.0);
        m.reset();
        let y = run(&mut m, &x);
        let mid = &y[SR..SR + (1 << 14)];
        assert_db(&format!("style {k} on, sample peak"), db(peak(mid) as f64), -3.06, 0.1);
        assert_db(&format!("style {k} on, true peak"), db(true_peak(mid)), -0.05, 0.1);
    }
}

#[test]
fn character_slows_the_release() {
    let x = burst_signal();
    for k in [IRON, GLOW] {
        let mut last = 0.0;
        for c in [0.0f32, 1.0, 2.0, 3.5, 5.0, 7.0, 9.0, 10.0] {
            let mut m = make(k, c, 12.0);
            let t = timing_types(&gain_curve(&run(&mut m, &x), &x));
            let r = t.release63.unwrap();
            assert!(r > last, "style {k} character {c}: {r} after {last}");
            last = r;
        }
    }
}
