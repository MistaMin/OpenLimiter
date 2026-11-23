//! Style "Leveller" against the stored measurements of source H (`spec/MASTER_BRIEF.md`
//! section 2; the H params file's sections are cited per test; the numbers come from
//! `results.json` in the H measurement folder). The composite test signal and the analysis
//! are rebuilt from the H job script in `measurements/scripts/`.
//! Tolerances: 10 % or 0.3 ms on times, 0.1 dB on levels (brief section 7).

mod common;
use common::composite::*;
use common::*;
use limiter_wall::*;
use olm_api::Module;

const T_REL: f64 = 0.10;
const T_MS: f64 = 0.3;
const T_DB: f64 = 0.1;
const BL: [usize; 5] = T_BL;

fn manual(ms: f32) -> Vec<(usize, f32)> {
    vec![(P_THRESHOLD, -12.0), (P_CEILING, 0.0), (P_AUTO_RELEASE, 0.0), (P_RELEASE, ms)]
}
fn auto(thr: f32, ceil: f32) -> Vec<(usize, f32)> {
    vec![(P_THRESHOLD, thr), (P_CEILING, ceil), (P_AUTO_RELEASE, 1.0), (P_RELEASE, 1.0)]
}

/// H sections 1 and 2: 64 samples, gain below the limit exactly ceiling minus threshold, output
/// exactly at the ceiling above it, impulse on the same sample after compensation.
#[test]
fn latency_and_static() {
    let sig = composite();
    let mut c = Check::new();
    let m = fresh(48000.0, &[(P_STYLE, STYLE_LEVELLER)]);
    c.cmp("latency at 48 kHz (samples); H section 1: 64", m.latency() as f64, 64.0, 0.0, 0.0, false);
    // (threshold, ceiling, gain below the limit, ceiling level, impulse peak)
    for (thr, ceil, imp) in [(0.0f32, 0.0f32, 0.25), (-12.0, 0.0, 0.99527), (-6.0, 0.0, 0.49882), (-18.0, 0.0, 1.0), (-12.0, -6.0, 0.49882), (-12.0, -0.3, 0.96148)] {
        let r = analyse(&sig, &auto(thr, ceil));
        let gain = (ceil - thr) as f64;
        let (mut worst_g, mut worst_c) = (0.0f64, 0.0f64);
        let mut d = -36;
        while d <= 0 {
            let (rms_db, peak_db) = r.statics[&d];
            // H section 2: below the threshold out = in + gain; above, the sine peak sits at the ceiling
            let want = if d as f64 <= thr as f64 { d as f64 + gain } else { ceil as f64 };
            worst_g = worst_g.max((rms_db - want).abs());
            worst_c = worst_c.max((peak_db - want).abs());
            d += 2;
        }
        c.cmp(&format!("threshold {thr}, ceiling {ceil}: largest level error over 19 steps, from rms (dB)"), worst_g, 0.0, 0.0, T_DB, false);
        c.cmp(&format!("threshold {thr}, ceiling {ceil}: largest level error over 19 steps, from peak (dB)"), worst_c, 0.0, 0.0, T_DB, false);
        c.cmp(&format!("threshold {thr}, ceiling {ceil}: impulse offset after compensation (samples)"), r.impulse_offset as f64, 0.0, 0.0, 0.0, false);
        c.cmp(&format!("threshold {thr}, ceiling {ceil}: impulse peak"), r.impulse_peak, imp, 0.0, 0.002, false);
    }
    // H section 2, third bullet: no added distortion on a steady sine 12 dB over the threshold
    let r = analyse(&sig, &auto(-12.0, 0.0));
    let (s, e) = sig.sec["step0"];
    c.note("harmonics 2 to 9 of the 0 dBFS step, threshold -12 (dB rel. fundamental)", thd_db(&r.out.0[s + SR / 8..e - SR / 8], SR), "stored: residual at the floor, -101 dB");
    c.finish("Leveller: latency and static behaviour (H sections 1, 2)");
}

/// H section 3: no overshoot at any burst length, gain in the burst exactly ceiling minus the
/// input level, reduction starts before the burst. H section 5: full stereo link.
#[test]
fn attack_and_link() {
    let sig = composite();
    let mut c = Check::new();
    let r = analyse(&sig, &auto(-12.0, 0.0));
    // stored: gain 1 ms before the burst and 5 ms before, per burst length (auto, threshold -12)
    let stored = [(1usize, 10.22, 11.26), (5, 10.43, 11.85), (20, 10.29, 11.76), (100, 10.25, 11.71), (500, 10.24, 11.70)];
    for (ms, m1, m5) in stored {
        let b = &r.burst[&ms];
        c.cmp(&format!("{ms} ms burst: peak in the burst (dBFS)"), b.peak, 0.0, 0.0, 0.005, false);
        c.cmp(&format!("{ms} ms burst: gain in the burst (dB)"), b.in_burst, if ms == 1 { 1.99 } else { 2.0 }, 0.0, T_DB, false);
        c.cmp(&format!("{ms} ms burst: gain at the end of the 3 s (dB)"), b.end, 12.0, 0.0, T_DB, false);
        // the look-ahead ramp: these two readings fixed its shape (triangular weights)
        c.cmp(&format!("{ms} ms burst: gain 1 ms before the burst (dB)"), b.minus_1ms, m1, 0.0, T_DB, false);
        c.cmp(&format!("{ms} ms burst: gain 5 ms before the burst (dB)"), b.minus_5ms, m5, 0.0, T_DB, false);
    }
    c.cmp("left gain in a left-only burst (dB)", r.link.0, 2.0, 0.0, T_DB, false);
    c.cmp("right gain during the left burst (dB)", r.link.1, 2.0, 0.0, T_DB, false);
    c.cmp("right gain before the burst (dB)", r.link.2, 12.0, 0.0, T_DB, false);
    let r = analyse(&sig, &auto(-12.0, -6.0));
    c.cmp("ceiling -6: peak in the 100 ms burst (dBFS)", r.burst[&100].peak, -6.0, 0.0, 0.005, false);
    c.cmp("ceiling -6: gain in the 100 ms burst (dB)", r.burst[&100].in_burst, -4.0, 0.0, T_DB, false);
    c.finish("Leveller: attack and stereo link (H sections 3, 5)");
}

/// H section 4, first table: manual release. Stored per setting and burst length
/// (50 %, 90 %, 99 % in ms) from `results.json`, the manual-release jobs at threshold -12.
#[test]
fn manual_release_table() {
    let sig = composite();
    let mut c = Check::new();
    type Row = [(f64, f64, f64); 5];
    let rows: [(f32, Row); 6] = [
        (0.01, [(0.0, 0.52, 0.75), (0.0, 0.60, 0.77), (0.0, 0.62, 0.77), (0.0, 0.0, 0.77), (0.0, 0.0, 0.77)]),
        (0.1, [(0.0, 0.54, 0.75), (0.0, 0.62, 0.77), (0.0, 0.65, 0.77), (0.0, 0.65, 0.77), (0.0, 0.65, 0.77)]),
        (1.0, [(0.27, 0.85, 1.79), (0.29, 0.83, 0.96), (0.31, 0.83, 0.94), (0.31, 0.83, 0.92), (0.31, 0.83, 0.92)]),
        (10.0, [(1.92, 7.94, 17.77), (1.88, 7.88, 16.98), (1.85, 7.83, 15.98), (1.85, 7.81, 15.88), (1.85, 7.81, 14.96)]),
        (100.0, [(19.38, 79.79, 177.33), (19.5, 79.83, 177.6), (19.58, 79.81, 176.98), (19.62, 79.79, 176.0), (19.65, 79.79, 174.92)]),
        (1000.0, [(192.19, 784.21, 1638.77), (192.58, 784.65, 1638.62), (192.56, 784.04, 1633.94), (192.23, 781.21, 1609.81), (189.83, 756.73, 1438.88)]),
    ];
    for (ms, want) in rows {
        let r = analyse(&sig, &manual(ms));
        for (k, len) in BL.iter().enumerate() {
            let b = &r.burst[len];
            let w = want[k];
            let known = declared_miss_manual(ms, *len);
            c.cmp(&format!("release {ms} ms, {len} ms burst: 50 % (ms)"), b.rec50, w.0, T_REL, T_MS, known);
            c.cmp(&format!("release {ms} ms, {len} ms burst: 90 % (ms)"), b.rec90, w.1, T_REL, T_MS, known);
            c.cmp(&format!("release {ms} ms, {len} ms burst: 99 % (ms)"), b.rec99, w.2, T_REL, T_MS, known);
            c.cmp(&format!("release {ms} ms, {len} ms burst: peak in the burst (dBFS)"), b.peak, 0.0, 0.0, 0.005, false);
        }
    }
    c.finish("Leveller: manual release (H section 4)");
}

/// Cells REPORT.md declares as not reached (all are "0.0 ms against a few tenths of a ms"
/// readings that sit on the ripple of the envelope right after the hard burst edge).
fn declared_miss_manual(ms: f32, len: usize) -> bool {
    (ms == 0.01 && len >= 100) || (ms == 1.0 && len >= 20)
}

/// H section 4, second table: automatic release. Stored from `results.json` "thr -12",
/// "thr -6", "thr -18" (50 %, 90 %, 99 % in ms per burst length) and the gain before each burst.
#[test]
fn auto_release_table() {
    let sig = composite();
    let mut c = Check::new();
    type Row = [(f64, f64, f64, f64); 5];
    let rows: [(f32, Row); 3] = [
        (-12.0, [(0.06, 0.79, 439.54, 10.59), (0.0, 0.73, 0.83, 12.0), (0.0, 0.83, 511.73, 12.0), (15.81, 290.62, 777.92, 12.0), (110.77, 396.73, 878.88, 12.0)]),
        (-6.0, [(0.04, 0.83, 541.42, 5.09), (0.0, 0.73, 0.83, 6.0), (0.0, 0.83, 1.0, 6.0), (16.0, 347.77, 841.88, 6.0), (124.88, 453.79, 939.98, 6.0)]),
        (-18.0, [(0.02, 0.77, 375.12, 16.33), (0.0, 0.71, 0.85, 18.0), (0.0, 0.81, 455.88, 18.0), (15.81, 240.88, 722.0, 18.0), (108.9, 346.92, 823.92, 18.0)]),
    ];
    for (thr, want) in rows {
        let r = analyse(&sig, &auto(thr, 0.0));
        for (k, len) in BL.iter().enumerate() {
            let b = &r.burst[len];
            let w = want[k];
            c.cmp(&format!("threshold {thr}, {len} ms burst: gain before (dB)"), b.before, w.3, 0.0, T_DB, declared_miss_auto(thr, *len, 0));
            c.cmp(&format!("threshold {thr}, {len} ms burst: 50 % (ms)"), b.rec50, w.0, T_REL, T_MS, declared_miss_auto(thr, *len, 50));
            c.cmp(&format!("threshold {thr}, {len} ms burst: 90 % (ms)"), b.rec90, w.1, T_REL, T_MS, declared_miss_auto(thr, *len, 90));
            c.cmp(&format!("threshold {thr}, {len} ms burst: 99 % (ms)"), b.rec99, w.2, T_REL, T_MS, declared_miss_auto(thr, *len, 99));
            c.cmp(&format!("threshold {thr}, {len} ms burst: peak in the burst (dBFS)"), b.peak, 0.0, 0.0, 0.005, false);
        }
    }
    // H section 4: with auto on, the release control has no effect.
    let a = analyse(&sig, &[(P_THRESHOLD, -12.0), (P_CEILING, 0.0), (P_AUTO_RELEASE, 1.0), (P_RELEASE, 0.1)]);
    let b = analyse(&sig, &[(P_THRESHOLD, -12.0), (P_CEILING, 0.0), (P_AUTO_RELEASE, 1.0), (P_RELEASE, 1000.0)]);
    let d = a.out.0.iter().zip(b.out.0.iter()).fold(0.0f64, |m, (p, q)| m.max((p - q).abs()));
    c.cmp("auto on: largest output difference between release 0.1 ms and 1000 ms", d, 0.0, 0.0, 0.0, false);
    // H section 4: auto times do not change with the ceiling.
    let e = analyse(&sig, &auto(-12.0, -6.0));
    c.cmp("ceiling -6, 100 ms burst: 90 % (ms)", e.burst[&100].rec90, 290.62, T_REL, T_MS, declared_miss_auto(-12.0, 100, 90));
    c.cmp("ceiling -6, 500 ms burst: 90 % (ms)", e.burst[&500].rec90, 396.73, T_REL, T_MS, declared_miss_auto(-12.0, 500, 90));
    c.finish("Leveller: automatic release (H section 4)");
}

/// Cells REPORT.md declares as not reached by the automatic release model.
/// `what`: 0 = gain before the burst, 50 / 90 / 99 = recovery share.
fn declared_miss_auto(thr: f32, len: usize, what: u32) -> bool {
    match (thr as i32, len, what) {
        // recovery from the long 0 dBFS step that precedes the 1 ms burst
        (-12, 1, 0) | (-12, 1, 99) | (-6, 1, 99) | (-18, 1, 0) | (-18, 1, 99) => true,
        // the small slow tail after a 20 ms burst
        (-12, 20, 99) | (-18, 20, 99) => true,
        // 100 ms burst, depth dependence
        (-6, 100, 90) | (-6, 100, 99) | (-18, 100, 90) => true,
        _ => false,
    }
}

/// H section 5: sample peak exactly at the ceiling on driven gated noise; true peak not
/// controlled (+4.07 dBTP stored for the original's own noise). The noise here is built the
/// same way from different random numbers, so levels are reported next to the stored ones.
#[test]
fn noise_peaks() {
    let sig = composite();
    let mut c = Check::new();
    for (name, sets, ceil, stored_tp, stored_rms) in [
        ("auto, threshold -12, ceiling 0", auto(-12.0, 0.0), 0.0, 4.07, -13.17),
        ("auto, threshold -12, ceiling -0.3", auto(-12.0, -0.3), -0.3, 3.77, -13.47),
        ("auto, threshold -12, ceiling -6", auto(-12.0, -6.0), -6.0, -1.93, -19.17),
        ("manual 0.01 ms", manual(0.01), 0.0, 4.08, -12.63),
        ("manual 1 ms", manual(1.0), 0.0, 4.07, -12.85),
        ("manual 10 ms", manual(10.0), 0.0, 3.37, -13.44),
        ("manual 100 ms", manual(100.0), 0.0, 2.5, -14.39),
        ("manual 1000 ms", manual(1000.0), 0.0, 2.22, -15.90),
    ] {
        let r = analyse(&sig, &sets);
        c.cmp(&format!("{name}: sample peak of the noise (dBFS)"), r.noise_sample_peak, ceil, 0.0, 0.002, false);
        c.note(&format!("{name}: true peak (dBTP)"), r.noise_true_peak, &format!("stored {stored_tp} on the original's noise"));
        c.note(&format!("{name}: output rms (dBFS), input rms {:.2}", r.noise_in_rms), r.noise_rms, &format!("stored {stored_rms} with input rms -18.95"));
    }
    c.finish("Leveller: peaks on gated noise (H section 5)");
}
