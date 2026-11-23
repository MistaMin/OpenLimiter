//! Style "Contour" against the stored measurements of source G (`spec/MASTER_BRIEF.md`
//! section 2; the G params file's sections are cited per test; the numbers come from that
//! file's tables and from `jobs_s0.json` and `step5.json` in the G measurement folder).
//! Tolerances: 10 % or 0.3 ms on times, 0.1 dB on levels (brief section 7).

mod common;
use common::*;
use limiter_wall::*;
use olm_api::Module;
use std::f64::consts::PI;

const T_REL: f64 = 0.10;
const T_MS: f64 = 0.3;
const T_DB: f64 = 0.1;

fn manual(ms: f32) -> Vec<(usize, f32)> {
    vec![(P_AUTO_RELEASE, 0.0), (P_RELEASE, ms)]
}
fn auto(profile: f32) -> Vec<(usize, f32)> {
    vec![(P_AUTO_RELEASE, 1.0), (P_PROFILE, profile)]
}
fn with(drive: f32, mut rest: Vec<(usize, f32)>) -> Vec<(usize, f32)> {
    rest.insert(0, (P_DRIVE, drive));
    rest
}

/// G section 1: the real delay is 36 samples at 48 kHz, whole samples, and the default state
/// passes a signal under the ceiling unchanged.
#[test]
fn latency_and_unity() {
    let mut c = Check::new();
    let mut m = fresh(48000.0, &[]);
    c.cmp("latency at 48 kHz (samples); G section 1: 36 measured", m.latency() as f64, 36.0, 0.0, 0.0, false);
    let mut l = vec![0.0f32; 4096];
    let mut r = vec![0.0f32; 4096];
    l[1000] = 0.25;
    r[1000] = -0.25;
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(a, b);
    }
    let k = l.iter().position(|v| v.abs() > 1e-6).unwrap();
    c.cmp("impulse arrives after (samples)", (k - 1000) as f64, 36.0, 0.0, 0.0, false);
    c.cmp("impulse value (unity below the limit)", l[k] as f64, 0.25, 0.0, 1e-7, false);
    c.cmp("only one non-zero output sample", l.iter().filter(|v| v.abs() > 1e-9).count() as f64, 1.0, 0.0, 0.0, false);
    // programme-like signal under the ceiling: bit-exact delay (G section 1, default state)
    let x = gated_noise(7, 1);
    let x: Vec<f64> = x.iter().map(|v| (*v * 0.6) as f32 as f64).collect();
    let mut m = fresh(48000.0, &[]);
    let y = run_mono(&mut m, &x);
    let d = x.iter().zip(y.iter()).fold(0.0f64, |a, (p, q)| a.max((p - q).abs()));
    c.cmp("default state, peaks under the ceiling: largest difference to the input", d, 0.0, 0.0, 0.0, false);
    let m96 = fresh(96000.0, &[]);
    c.note("latency at 96 kHz (samples)", m96.latency() as f64, "free choice: constant in time; G section 1 infers about 37");
    c.finish("Contour: latency and unity");
}

fn static_row(sets: &[(usize, f32)], sr: usize) -> (Vec<f64>, Vec<f64>) {
    let x = static_steps(sr);
    let mut m = fresh(sr as f32, sets);
    let y = run_mono(&mut m, &x);
    let mut gains = Vec::new();
    let mut peaks = Vec::new();
    for k in 0..9 {
        let (a, b) = (((k as f64 + 0.5) * sr as f64) as usize, ((k as f64 + 0.9) * sr as f64) as usize);
        gains.push(db(rms(&y[a..b])) - db(rms(&x[a..b])));
        peaks.push(db(sample_peak(&y[a..b])));
    }
    (gains, peaks)
}

/// G section 3 table (static curve, Master mode); values from `jobs_s0.json` "static | ...".
#[test]
fn static_curve() {
    let mut c = Check::new();
    let rows: Vec<(&str, Vec<(usize, f32)>, [f64; 9], f64)> = vec![
        ("drive 0, ceiling -0.2", vec![], [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.26], -0.26),
        ("drive 6", vec![(P_DRIVE, 6.0)], [6.0, 6.0, 6.0, 6.0, 6.0, 6.0, 6.0, 5.74, -0.26], -0.26),
        ("drive 12", vec![(P_DRIVE, 12.0)], [12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 11.74, 5.74, -0.26], -0.26),
        ("drive 18", vec![(P_DRIVE, 18.0)], [18.0, 18.0, 18.0, 18.0, 18.0, 17.74, 11.74, 5.74, -0.26], -0.26),
        ("drive 12, ceiling 0", vec![(P_DRIVE, 12.0), (P_CEILING, 0.0)], [12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 11.94, 5.94, -0.06], -0.06),
        ("drive 12, ceiling -1", vec![(P_DRIVE, 12.0), (P_CEILING, -1.0)], [12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 10.94, 4.94, -1.06], -1.06),
        ("drive 12, ceiling -6", vec![(P_DRIVE, 12.0), (P_CEILING, -6.0)], [12.0, 12.0, 12.0, 12.0, 12.0, 11.94, 5.94, -0.06, -6.06], -6.06),
        ("drive 12, manual 20 ms", with(12.0, manual(20.0)), [12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 11.74, 5.74, -0.26], -0.26),
        ("drive 12, manual 3000 ms", with(12.0, manual(3000.0)), [12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 11.74, 5.74, -0.26], -0.26),
    ];
    for (name, sets, want, peak) in rows {
        let (g, p) = static_row(&sets, 48000);
        let worst = g.iter().zip(want.iter()).fold(0.0f64, |a, (x, y)| a.max((x - y).abs()));
        c.cmp(&format!("{name}: largest gain error over the 9 steps (dB)"), worst, 0.0, 0.0, T_DB, false);
        c.cmp(&format!("{name}: output sample peak when limiting (dBFS)"), p[8], peak, 0.0, T_DB, false);
    }
    let (g, p) = static_row(&[(P_DRIVE, 12.0)], 96000);
    let want = [12.0, 12.0, 12.0, 12.0, 12.0, 12.0, 11.74, 5.74, -0.26];
    let worst = g.iter().zip(want.iter()).fold(0.0f64, |a, (x, y)| a.max((x - y).abs()));
    c.cmp("96 kHz, drive 12: largest gain error (dB)", worst, 0.0, 0.0, T_DB, false);
    c.cmp("96 kHz, drive 12: output sample peak (dBFS)", p[8], -0.26, 0.0, T_DB, false);

    // Distortion in limiting, G section 3: about -150 dB with auto and manual 3000 ms,
    // -119 to -120 dB with manual 20 ms. Reported, not asserted to 0.1 dB.
    for (name, sets, stored) in [
        ("auto", with(12.0, auto(0.0)), -149.7),
        ("manual 3000 ms", with(12.0, manual(3000.0)), -149.4),
        ("manual 20 ms", with(12.0, manual(20.0)), -119.0),
    ] {
        let x = static_steps(48000);
        let mut m = fresh(48000.0, &sets);
        let y = run_mono(&mut m, &x);
        let seg = &y[(8.5 * 48000.0) as usize..(8.9 * 48000.0) as usize];
        c.note(&format!("harmonics 2 to 9 at 0 dBFS in, drive 12, {name} (dB rel. fundamental)"), thd_db(seg, 48000), &format!("stored {stored}"));
    }
    c.finish("Contour: static curve (G section 3)");
}

struct Timing {
    bg: f64,
    in_burst: f64,
    recovered: f64,
    attack63: f64,
    rel63: f64,
    rel90: f64,
    min_in_burst: f64,
    peak_in_burst: f64,
    after10: f64,
    trace: Vec<f64>,
}

/// The shared helper's `metrics` for the timing signal plus the G job script's extras.
fn timing(sets: &[(usize, f32)], sr: usize) -> Timing {
    let x = timing_signal(16.5, sr);
    let mut m = fresh(sr as f32, sets);
    let y = run_mono(&mut m, &x);
    let g = gain_trace(&y, &x);
    let f = sr as f64;
    let base = 8 * sr;
    let (on, off, end) = (base + (0.1 * f) as usize, base + (0.3 * f) as usize, base + (7.9 * f) as usize);
    let pre = median(&g[on - (0.05 * f) as usize..on]);
    let fin = median(&g[off - (0.05 * f) as usize..off]);
    let post = median(&g[end - (0.25 * f) as usize..end]);
    Timing {
        bg: pre,
        in_burst: fin,
        recovered: post,
        attack63: cross(&g, on, pre + 0.63 * (fin - pre), fin > pre, sr).unwrap_or(f64::NAN),
        rel63: cross(&g, off, fin + 0.63 * (post - fin), post > fin, sr).unwrap_or(f64::NAN),
        rel90: cross(&g, off, fin + 0.9 * (post - fin), post > fin, sr).unwrap_or(f64::NAN),
        min_in_burst: g[on..on + (0.2 * f) as usize].iter().cloned().fold(f64::INFINITY, f64::min),
        peak_in_burst: db(sample_peak(&y[on..on + (0.2 * f) as usize])),
        after10: g[on + (0.21 * f) as usize],
        trace: g,
    }
}

/// G section 5, first table. Stored values from `jobs_s0.json` "timing | ...".
#[test]
fn timing_table() {
    let mut c = Check::new();
    // name, settings, release 63 %, release 90 %, attack 63 %, gain 10 ms after the end
    let rows: Vec<(&str, Vec<(usize, f32)>, f64, f64, f64, f64, f64)> = vec![
        ("manual 20 ms", with(12.0, manual(20.0)), 7.722, 14.791, 0.222, 10.41, 12.0),
        ("manual 54.5 ms", with(12.0, manual(54.5)), 20.954, 40.802, 0.222, 7.19, 12.0),
        ("manual 200 ms", with(12.0, manual(200.0)), 77.879, 150.750, 0.222, 5.45, 12.0),
        ("manual 857 ms", with(12.0, manual(857.0)), 334.734, 646.587, 0.222, 5.23, 12.0),
        ("manual 3000 ms", with(12.0, manual(3000.0)), 1171.861, 2262.576, 0.222, 5.21, 12.0),
        ("auto Universal", with(12.0, auto(0.0)), 781.375, 1508.764, 0.222, 5.22, 12.0),
        ("auto Dense", with(12.0, auto(1.0)), 781.327, 1508.744, 0.222, 5.22, 12.0),
        ("auto Natural", with(12.0, auto(2.0)), 781.729, 1509.142, 0.222, 5.22, 12.0),
        ("auto Voice", with(12.0, auto(3.0)), 390.611, 754.405, 0.222, 5.22, 12.0),
        ("drive 18, manual 200 ms", with(18.0, manual(200.0)), 66.780, 136.788, 0.201, 5.87, 18.0),
        ("drive 18, auto Universal", with(18.0, auto(0.0)), 668.090, 1368.661, 0.201, 5.22, 18.0),
    ];
    for (name, sets, r63, r90, a63, after10, bg) in rows {
        let t = timing(&sets, 48000);
        c.cmp(&format!("{name}: release 63 % (ms)"), t.rel63, r63, T_REL, T_MS, false);
        c.cmp(&format!("{name}: release 90 % (ms)"), t.rel90, r90, T_REL, T_MS, false);
        c.cmp(&format!("{name}: attack 63 % (ms)"), t.attack63, a63, T_REL, T_MS, false);
        c.cmp(&format!("{name}: gain before the burst (dB)"), t.bg, bg, 0.0, T_DB, false);
        c.cmp(&format!("{name}: gain in the burst (dB)"), t.in_burst, 5.21, 0.0, T_DB, false);
        c.cmp(&format!("{name}: lowest gain in the burst (dB)"), t.min_in_burst, 5.19, 0.0, T_DB, false);
        c.cmp(&format!("{name}: gain 10 ms after the end (dB)"), t.after10, after10, 0.0, T_DB, false);
        c.cmp(&format!("{name}: gain recovered (dB)"), t.recovered, bg, 0.0, T_DB, false);
        c.cmp(&format!("{name}: output peak in the burst (dBFS)"), t.peak_in_burst, -0.25, 0.0, T_DB, false);
    }
    // 96 kHz: same times as 48 kHz to 3 decimals in the original (G section 5, last bullet).
    let t = timing(&with(12.0, manual(200.0)), 96000);
    c.cmp("96 kHz, manual 200 ms: release 63 % (ms)", t.rel63, 77.879, T_REL, T_MS, false);
    c.cmp("96 kHz, manual 200 ms: release 90 % (ms)", t.rel90, 150.750, T_REL, T_MS, false);
    let t = timing(&with(12.0, auto(0.0)), 96000);
    c.cmp("96 kHz, auto Universal: release 63 % (ms)", t.rel63, 781.371, T_REL, T_MS, false);
    c.cmp("96 kHz, auto Universal: release 90 % (ms)", t.rel90, 1508.764, T_REL, T_MS, false);
    c.finish("Contour: timing on the 200 ms burst (G section 5)");
}

/// The stored gain-against-time curves (`spec/curves/`, G prefix). Compared from 1 ms after
/// the burst start on; the 5 ms before the burst hold an envelope artefact (G section 5) and
/// are reported separately.
#[test]
fn curves_match() {
    let mut c = Check::new();
    let sets: Vec<(&str, Vec<(usize, f32)>, &[(f64, f64)])> = vec![
        ("manual 20 ms", with(12.0, manual(20.0)), &curves::MANUAL_20),
        ("manual 54.5 ms", with(12.0, manual(54.5)), &curves::MANUAL_54),
        ("manual 200 ms", with(12.0, manual(200.0)), &curves::MANUAL_200),
        ("manual 857 ms", with(12.0, manual(857.0)), &curves::MANUAL_857),
        ("manual 3000 ms", with(12.0, manual(3000.0)), &curves::MANUAL_3000),
        ("auto Universal", with(12.0, auto(0.0)), &curves::AUTO_P0),
        ("auto Dense", with(12.0, auto(1.0)), &curves::AUTO_P1),
        ("auto Natural", with(12.0, auto(2.0)), &curves::AUTO_P2),
        ("auto Voice", with(12.0, auto(3.0)), &curves::AUTO_P3),
        ("drive 18, manual 200 ms", with(18.0, manual(200.0)), &curves::G18_MANUAL_200),
        ("drive 18, auto Universal", with(18.0, auto(0.0)), &curves::G18_AUTO_P0),
        (
            "drive 0, soft clip Continuous at -12 dB, manual 200 ms",
            vec![(P_SOFT_CLIP, 1.0), (P_SOFT_CLIP_THRESHOLD, -12.0), (P_SOFT_CLIP_CURVE, 1.0), (P_AUTO_RELEASE, 0.0), (P_RELEASE, 200.0)],
            &curves::CLIP_CONT_M12,
        ),
    ];
    for (name, s, curve) in sets {
        let t = timing(&s, 48000);
        let on = 8 * SR + SR / 10;
        let (mut worst, mut worst_t, mut edge) = (0.0f64, 0.0, 0.0f64);
        let (mut sum2, mut n) = (0.0, 0);
        for (ms, stored) in curve.iter() {
            let i = (on as f64 + (ms * SR as f64 / 1000.0).round()) as usize;
            let d = (t.trace[i] - stored).abs();
            if *ms >= 1.0 && !(*ms >= 199.0 && *ms < 201.0) {
                if d > worst {
                    worst = d;
                    worst_t = *ms;
                }
                sum2 += d * d;
                n += 1;
            } else {
                edge = edge.max(d);
            }
        }
        c.cmp(&format!("{name}: largest deviation from the stored curve, 1 ms on, burst end +-1 ms excluded (dB)"), worst, 0.0, 0.0, 0.3, false);
        c.note(&format!("{name}: rms deviation over the same points (dB)"), (sum2 / n as f64).sqrt(), &format!("worst point at {worst_t} ms"));
        c.note(&format!("{name}: largest deviation before 1 ms and within 1 ms of the burst end (dB)"), edge, "edge region, envelope artefact");
    }
    c.finish("Contour: stored curves (tolerance 0.3 dB, see REPORT.md)");
}

struct BurstRow {
    pre: f64,
    depth: f64,
    rec50: f64,
    rec90: f64,
}

/// The G job script's burst-length analysis.
fn bursts(sets: &[(usize, f32)]) -> Vec<BurstRow> {
    let x = burst_length_signal();
    let mut m = fresh(48000.0, sets);
    let y = run_mono(&mut m, &x);
    let g = gain_trace(&y, &x);
    let sr = SR as f64;
    let mut out = Vec::new();
    for (st, len) in BURST_STARTS_S.iter().zip(BURST_LENS_MS.iter()) {
        let i0 = st * SR;
        let i1 = ((*st as f64 + *len as f64 / 1000.0) * sr) as usize;
        let pre = median(&g[i0 - (0.2 * sr) as usize..i0 - (0.01 * sr) as usize]);
        let j = i1 + (0.003 * sr) as usize;
        let g0 = g[j..j + (0.02 * sr) as usize].iter().cloned().fold(f64::INFINITY, f64::min);
        let seg = &g[j..j + (7.5 * sr) as usize];
        let rec = |fr: f64| {
            seg.iter().position(|v| *v >= g0 + fr * (pre - g0)).map(|k| (k as f64 + 0.003 * sr) / sr * 1000.0).unwrap_or(f64::NAN)
        };
        out.push(BurstRow { pre, depth: g0, rec50: rec(0.5), rec90: rec(0.9) });
    }
    out
}

/// G section 5, second table (release after bursts of 5, 20, 100, 500, 2000 ms). Stored
/// values from `jobs_s0.json` "burstlen | ..." (depth dB, 50 % ms, 90 % ms per burst).
#[test]
fn burst_length_table() {
    let mut c = Check::new();
    type Row = [(f64, f64, f64); 5];
    let rows: Vec<(&str, Vec<(usize, f32)>, Row)> = vec![
        ("manual 20 ms", with(12.0, manual(20.0)), [(7.16, 7.6, 16.6), (7.19, 7.7, 16.7), (7.19, 7.7, 16.0), (7.19, 7.7, 15.9), (7.19, 7.7, 15.9)]),
        ("manual 200 ms", with(12.0, manual(200.0)), [(5.73, 60.8, 151.6), (5.70, 60.6, 151.4), (5.69, 60.6, 150.9), (5.69, 60.6, 150.9), (5.69, 60.6, 150.9)]),
        ("manual 3000 ms", with(12.0, manual(3000.0)), [(5.70, 909.4, 2269.4), (5.68, 905.4, 2265.9), (5.67, 903.9, 2264.4), (5.66, 903.4, 2263.9), (5.66, 903.4, 2263.9)]),
        ("auto Universal", with(12.0, auto(0.0)), [(11.49, 3.8, 4.9), (7.12, 412.9, 1301.9), (5.67, 602.8, 1509.9), (5.66, 602.4, 1509.9), (5.66, 602.4, 1509.9)]),
        ("auto Dense", with(12.0, auto(1.0)), [(11.84, 3.0, 3.0), (6.24, 487.4, 1387.4), (5.67, 602.6, 1509.9), (5.66, 602.4, 1509.9), (5.66, 602.4, 1509.9)]),
        ("auto Natural", with(12.0, auto(2.0)), [(7.51, 6.5, 15.9), (5.96, 462.4, 1365.9), (5.67, 602.9, 1510.4), (5.66, 602.9, 1509.9), (5.66, 602.8, 1509.9)]),
        ("auto Voice", with(12.0, auto(3.0)), [(11.87, 3.0, 3.0), (6.51, 235.8, 683.9), (5.67, 301.4, 754.9), (5.66, 300.9, 754.9), (5.66, 300.9, 754.9)]),
    ];
    for (name, sets, want) in rows {
        let got = bursts(&sets);
        for (k, (b, w)) in got.iter().zip(want.iter()).enumerate() {
            let len = BURST_LENS_MS[k];
            let known = declared_miss(name, len);
            c.cmp(&format!("{name}, {len} ms burst: gain before (dB)"), b.pre, 12.0, 0.0, T_DB, false);
            c.cmp(&format!("{name}, {len} ms burst: depth 3 ms after (dB)"), b.depth, w.0, 0.0, T_DB, known);
            c.cmp(&format!("{name}, {len} ms burst: 50 % recovery (ms)"), b.rec50, w.1, T_REL, T_MS, known);
            c.cmp(&format!("{name}, {len} ms burst: 90 % recovery (ms)"), b.rec90, w.2, T_REL, T_MS, known);
        }
    }
    c.finish("Contour: release against burst length (G section 5)");
}

/// Deviations REPORT.md declares for the burst-length table.
fn declared_miss(_name: &str, _len: usize) -> bool {
    false
}

/// G section 4 table: soft clip static gain, distortion and output peak.
/// Stored values from `jobs_s0.json` "softclip static | ...".
#[test]
fn soft_clip_table() {
    let mut c = Check::new();
    // name, settings, gains at -24 -18 -12 -6 0, thd at the same, output peak at 0 dBFS in
    type Five = [f64; 5];
    let on = |thr: f32, curve: f32| vec![(P_SOFT_CLIP, 1.0), (P_SOFT_CLIP_THRESHOLD, thr), (P_SOFT_CLIP_CURVE, curve)];
    let rows: Vec<(&str, Vec<(usize, f32)>, Five, Five, f64)> = vec![
        ("off (threshold -6, Narrow set)", vec![(P_SOFT_CLIP_THRESHOLD, -6.0), (P_SOFT_CLIP_CURVE, 2.0)], [0.0, 0.0, 0.0, 0.0, -0.26], [-125.0, -132.0, -139.0, -145.0, -150.0], -0.26),
        ("Wide knee at -6", on(-6.0, 0.0), [0.0, 0.0, 0.0, 0.0, -0.39], [-125.2, -131.5, -138.8, -144.9, -32.4], -0.61),
        ("Continuous at -6", on(-6.0, 1.0), [0.0, -0.01, -0.02, -0.09, -0.33], [-85.6, -73.6, -61.6, -49.7, -37.9], -0.44),
        ("Narrow knee at -6", on(-6.0, 2.0), [0.0, 0.0, 0.0, 0.0, -2.46], [-125.2, -131.5, -138.8, -144.9, -15.7], -4.0),
        ("Continuous at -12", on(-12.0, 1.0), [-0.01, -0.02, -0.09, -0.33, -1.21], [-73.6, -61.6, -49.7, -37.9, -26.8], -1.60),
        ("Continuous at 0", on(0.0, 1.0), [0.0, 0.0, -0.01, -0.02, -0.23], [-97.6, -85.6, -73.6, -61.6, -49.7], -0.26),
        ("Narrow knee at -6, drive 6", { let mut v = on(-6.0, 2.0); v.push((P_DRIVE, 6.0)); v }, [6.0, 6.0, 6.0, 5.74, 1.28], [f64::NAN; 5], -0.26),
    ];
    for (name, sets, gains, thds, peak) in rows {
        let x = static_steps(48000);
        let mut m = fresh(48000.0, &sets);
        let y = run_mono(&mut m, &x);
        for (k, step) in (4..9).enumerate() {
            let (a, b) = (((step as f64 + 0.5) * 48000.0) as usize, ((step as f64 + 0.9) * 48000.0) as usize);
            let level = -48 + 6 * step as i32;
            let g = db(rms(&y[a..b])) - db(rms(&x[a..b]));
            c.cmp(&format!("{name}: gain at {level} dBFS (dB)"), g, gains[k], 0.0, T_DB, false);
            let t = thd_db(&y[a..b], 48000);
            if thds[k].is_finite() {
                if thds[k] > -100.0 {
                    c.cmp(&format!("{name}: harmonics at {level} dBFS (dB)"), t, thds[k], 0.0, 1.0, false);
                } else {
                    // stored figures below -100 dB are the method floor of the original run
                    c.cmp(&format!("{name}: harmonics at {level} dBFS under -100 dB (1 = yes)"), if t < -100.0 { 1.0 } else { 0.0 }, 1.0, 0.0, 0.0, false);
                }
            }
            if step == 8 {
                c.cmp(&format!("{name}: output peak at 0 dBFS in (dBFS)"), db(sample_peak(&y[a..b])), peak, 0.0, T_DB, false);
            }
        }
    }
    // Instant, no memory (G section 4, fourth bullet): timing burst through the Continuous
    // curve at -12 dB, drive 0: gain in the burst -0.38 dB, back at 0.00 dB at once.
    let t = timing(&[(P_SOFT_CLIP, 1.0), (P_SOFT_CLIP_THRESHOLD, -12.0), (P_SOFT_CLIP_CURVE, 1.0), (P_AUTO_RELEASE, 0.0), (P_RELEASE, 200.0)], 48000);
    c.cmp("Continuous at -12, timing burst: gain in the burst (dB)", t.in_burst, -0.38, 0.0, T_DB, false);
    c.cmp("Continuous at -12, timing burst: gain 10 ms after the end (dB)", t.after10, 0.01, 0.0, T_DB, false);
    c.cmp("Continuous at -12, timing burst: release 90 % (ms)", t.rel90, 0.0, 0.0, T_MS, false);
    c.cmp("Continuous at -12, timing burst: output peak (dBFS)", t.peak_in_burst, -5.97, 0.0, T_DB, false);
    c.finish("Contour: soft clip (G section 4)");
}

/// G section 6: ceiling and true peak on tones. Stored values from `step5.json` and
/// `jobs_s0.json` "truepeak | ...". The test tones have no generator script; they are rebuilt
/// from their description (12 kHz, sample peak -3.01 dBFS, true peak 0.0 dBTP).
#[test]
fn ceiling_and_true_peak_on_tones() {
    let mut c = Check::new();
    let n = 4 * SR;
    let isp: Vec<f64> = (0..n).map(|i| (2.0 * PI * 12000.0 * i as f64 / SR as f64 + PI / 4.0).sin()).collect();
    c.note("inter-sample tone as rebuilt: sample peak (dBFS)", db(sample_peak(&isp)), "described as -3.01");
    let seg = |y: &[f64]| y[SR / 2..SR / 2 + 131072].to_vec();
    for (name, sets, sp, tp) in [
        ("inter-sample tone, drive 6", vec![(P_DRIVE, 6.0)], -3.14, -0.13),
        ("inter-sample tone, drive 6, ceiling -1", vec![(P_DRIVE, 6.0), (P_CEILING, -1.0)], -3.94, -0.93),
    ] {
        let mut m = fresh(48000.0, &sets);
        let y = seg(&run_mono(&mut m, &isp));
        c.cmp(&format!("{name}: sample peak (dBFS)"), db(sample_peak(&y)), sp, 0.0, T_DB, false);
        c.cmp(&format!("{name}: true peak, 8x ideal reconstruction (dBTP)"), db(true_peak(&y, 2000)), tp, 0.0, T_DB, false);
    }
    let hot: Vec<f64> = (0..n).map(|i| (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect();
    let mut m = fresh(48000.0, &[(P_DRIVE, 3.0)]);
    let y = seg(&run_mono(&mut m, &hot));
    c.cmp("hot 1 kHz sine, drive 3: sample peak (dBFS)", db(sample_peak(&y)), -0.26, 0.0, T_DB, false);
    c.cmp("hot 1 kHz sine, drive 3: true peak (dBTP)", db(true_peak(&y, 2000)), -0.26, 0.0, T_DB, false);
    // 96 kHz (one job, integer analysis only): sample peak and true peak both -0.13.
    let isp96: Vec<f64> = (0..4 * 96000).map(|i| (2.0 * PI * 12000.0 * i as f64 / 96000.0 + PI / 4.0).sin()).collect();
    let mut m = fresh(96000.0, &[(P_DRIVE, 6.0)]);
    let y = run_mono(&mut m, &isp96);
    let y = y[48000..48000 + 131072].to_vec();
    c.cmp("96 kHz inter-sample tone, drive 6: true peak (dBTP)", db(true_peak(&y, 2000)), -0.13, 0.0, T_DB, true);
    c.finish("Contour: ceiling and true peak on tones (G section 6)");
}

/// G section 6, first bullet: the sample peak is always held. Dense material is driven hard;
/// the true peak is reported (the original is +0.23 to +1.05 dBTP over on its programme-like
/// test file, which has no generator script and was not rebuilt).
#[test]
fn sample_peak_always_held() {
    let mut c = Check::new();
    let x = gated_noise(11, 4);
    for (name, sets, ceil) in [
        ("noise, drive 12, auto", with(12.0, auto(0.0)), -0.2),
        ("noise, drive 18, manual 20 ms", with(18.0, manual(20.0)), -0.2),
        ("noise, drive 12, ceiling -1, auto Voice", { let mut v = with(12.0, auto(3.0)); v.push((P_CEILING, -1.0)); v }, -1.0),
        ("noise, drive 18, manual 0.01 ms", with(18.0, manual(0.01)), -0.2),
    ] {
        let mut m = fresh(48000.0, &sets);
        let (l, r) = run(&mut m, &x, &x.iter().rev().cloned().collect::<Vec<_>>());
        let sp = db(sample_peak(&l).max(sample_peak(&r)));
        c.cmp(&format!("{name}: sample peak over the ceiling (dB, <= 0 passes)"), (sp - ceil).max(0.0), 0.0, 0.0, 1e-4, false);
        c.note(&format!("{name}: sample peak (dBFS)"), sp, &format!("ceiling {ceil}"));
        c.note(&format!("{name}: true peak L (dBTP)"), db(true_peak(&l[..131072], 2000)), "not held on dense material, as G section 6");
    }
    c.finish("Contour: sample peak on dense material (G section 6)");
}

/// G section 5, stereo link bullet: a burst on the left only reduces the right channel by the
/// same amount. The original test file has no generator script; a left-only burst of the
/// timing signal's shape is used, so only the in-burst figures are compared.
#[test]
fn stereo_link() {
    let mut c = Check::new();
    let l = timing_signal(8.0, SR);
    let r: Vec<f64> = (0..l.len()).map(|i| lin(-30.0) * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect();
    let mut m = fresh(48000.0, &with(12.0, manual(200.0)));
    let (yl, yr) = run(&mut m, &l, &r);
    let (gl, gr) = (gain_trace(&yl, &l), gain_trace(&yr, &r));
    let a = (0.2 * SR as f64) as usize..(0.29 * SR as f64) as usize;
    c.cmp("left gain in the burst (dB)", median(&gl[a.clone()]), 5.21, 0.0, T_DB, false);
    c.cmp("right gain during the left burst (dB)", median(&gr[a]), 5.21, 0.0, T_DB, false);
    c.cmp("right gain 1 s after (dB)", median(&gr[(1.25 * SR as f64) as usize..(1.35 * SR as f64) as usize]), 12.0, 0.0, T_DB, false);
    c.finish("Contour: stereo link (G section 5)");
}

/// Gain-reduction metering through `Module::meters`.
#[test]
fn meter_reports_reduction() {
    let mut c = Check::new();
    let mut m = fresh(48000.0, &[(P_DRIVE, 12.0)]);
    let mut l: Vec<f32> = (0..48000).map(|i| (0.5 * (2.0 * PI * 1000.0 * i as f64 / 48000.0).sin()) as f32).collect();
    let mut r = l.clone();
    c.cmp("meter before any audio (dB)", m.meters().reduction_db as f64, 0.0, 0.0, 0.0, false);
    let mut last = 0.0;
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(a, b);
        last = m.meters().reduction_db as f64;
    }
    // 0.5 = -6.02 dBFS, +12 dB drive, held at -0.26 dBFS: 6.24 dB of reduction
    c.cmp("meter on a steady sine 6.24 dB over (dB)", last, 6.24, 0.0, T_DB, false);
    let mut m = fresh(48000.0, &[]);
    let mut l = vec![0.1f32; 512];
    let mut r = vec![0.1f32; 512];
    m.process(&mut l, &mut r);
    c.cmp("meter below the limit (dB)", m.meters().reduction_db as f64, 0.0, 0.0, 0.0, false);
    c.finish("Contour: gain-reduction meter");
}
