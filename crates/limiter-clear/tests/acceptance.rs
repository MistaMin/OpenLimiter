//! Acceptance tests of HANDOFF_NEXT_TOOL.md section 3.3 (all but the dither row, which belongs
//! to another module). Each test rebuilds the signal of the measurement script named in the
//! spec section, runs the module the way the measurement host did, repeats the script's
//! analysis and compares with the stored result.
//!
//! Data files are named by base name. "recheck/" is the source-A recheck folder under
//! `measurements/`, "float_check/" the folder of that name.
//!
//! Run with `-- --nocapture --test-threads=1` to read the figures.

mod common;

use common::reference::*;
use common::*;
use limiter_clear::*;
use std::f64::consts::PI;

const SR: usize = 48_000;

/// The sources written as 24-bit files: `round(x * 8388607)` read back as `/ 8388608`.
fn q24(x: &[f64]) -> Vec<f64> {
    x.iter().map(|v| (v * 8_388_607.0).round() / 8_388_608.0).collect()
}

fn pos(index: usize, position: f32) -> (usize, f32) {
    (index, plain_from_position(index, position))
}

fn tone(freq: f64, level_db: f64, n: usize, sr: usize, phase: f64) -> Vec<f64> {
    (0..n).map(|i| lin(level_db) * (2.0 * PI * freq * i as f64 / sr as f64 + phase).sin()).collect()
}

// ---------------------------------------------------------------------------------------------
// 1. Gain laws. Script gain_laws_float.py; data float_check/gain_laws.json (SPEC_LIMITERS.md 58).

#[test]
fn t1_gain_laws() {
    let mut rep = Report::new("gain laws");
    let n = 2 * SR;
    let x = q24(&(0..n).map(|i| 0.01 * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect::<Vec<_>>());
    let tone_gain = |y: &[f64]| {
        let (mut a, mut c, mut cnt) = (0.0, 0.0, 0.0);
        for i in SR / 2..n - SR / 4 {
            let t = i as f64 / SR as f64;
            a += y[i] * (2.0 * PI * 1000.0 * t).sin();
            c += y[i] * (2.0 * PI * 1000.0 * t).cos();
            cnt += 1.0;
        }
        20.0 * ((2.0 * a / cnt).hypot(2.0 * c / cnt) / 0.01 + 1e-12).log10()
    };
    let positions = [0.0f32, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];
    // float_check/gain_laws.json: (display, measured gain dB) per position.
    let input = [(-18.0, -18.0), (-13.5, -13.5), (-9.0, -9.0), (-4.5, -4.5), (0.0, 0.0), (4.5, 4.5), (9.0, 9.0), (13.5, 13.5), (18.0, 18.0)];
    let trim = [(-18.0, -18.0), (-13.8, -13.78), (-10.1, -10.12), (-7.03, -7.03), (-4.5, -4.5), (-2.53, -2.53), (-1.12, -1.12), (-0.281, -0.28), (0.0, 0.0)];
    let thr = [(-12.0, 0.0), (-10.5, 0.0), (-9.0, 0.0), (-7.5, 0.0), (-6.0, 0.0), (-4.5, 0.0), (-3.0, 0.0), (-1.5, 0.0), (0.0, 0.0)];
    for (name, index, table) in [("input gain", P_INPUT_GAIN, input), ("output trim", P_OUTPUT_TRIM, trim), ("threshold", P_THRESHOLD, thr)] {
        for (p, (display, measured)) in positions.iter().zip(table.iter()) {
            let plain = plain_from_position(index, *p);
            // Script "pre": slow hold off, peak guard off, lift 0.
            let mut m = module(&[(P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), (P_LIFT, 0.0), (index, plain)]);
            let y = render(&mut m, &x, SR as f64);
            rep.row(&format!("{name} at position {p}: gain dB"), tone_gain(&y), *measured, 0.05);
            rep.row(&format!("{name} at position {p}: plain value against the stored display"), plain as f64, *display, 0.051);
        }
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 2. Hard-knee ceiling and soft-knee curves. Script *_knee_autogain.py part A;
//    data recheck/knee_autogain.json "knee" and "auto_gain_static" (SPEC_LIMITERS.md 71.1, 71.2).

fn level_steps() -> Vec<f64> {
    let h = SR / 2;
    let mut x = Vec::with_capacity(31 * h);
    for d in -30..=0 {
        x.extend(tone(1000.0, d as f64, h, SR, 0.0));
    }
    x
}

fn static_levels(y: &[f64]) -> Vec<f64> {
    let h = SR / 2;
    (0..31).map(|k| round2(db(sine_level(&y[k * h + h / 3..(k + 1) * h - h / 10])))).collect()
}

#[test]
fn t2_knee_curves() {
    let mut rep = Report::new("ceiling and knee");
    let x = level_steps();
    for (knee, thr, stored) in KNEE.iter() {
        // BASE of the script: slow hold off, peak guard off, lift 0, release 7.34 ms, attack minimum.
        let mut m = module(&[(P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), (P_LIFT, 0.0), pos(P_RELEASE, 0.5), pos(P_ATTACK, 0.0), (P_THRESHOLD, *thr), (P_KNEE, *knee)]);
        let got = static_levels(&render(&mut m, &x, SR as f64));
        for (k, (g, s)) in got.iter().zip(stored.iter()).enumerate() {
            rep.row(&format!("knee {knee} dB, threshold {thr} dB, input {} dBFS: output dBFS", k as i32 - 30), *g, *s, 0.05);
        }
    }
    for (slow, guard, stored) in SLOW_STATIC.iter() {
        let mut m = module(&[(P_PEAK_GUARD, *guard as u8 as f32), (P_LIFT, 0.0), pos(P_RELEASE, 0.5), pos(P_ATTACK, 0.0), pos(P_THRESHOLD, 0.5), (P_KNEE, 0.0), (P_SLOW_HOLD, *slow as u8 as f32)]);
        let got = static_levels(&render(&mut m, &x, SR as f64));
        for (k, (g, s)) in got.iter().zip(stored.iter()).enumerate() {
            rep.row(&format!("slow hold {slow}, peak guard {guard}, input {} dBFS: output dBFS", k as i32 - 30), *g, *s, 0.05);
        }
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 3. Release. Scripts *_timing_safe_off.py and *_release_short.py; data
//    recheck/results_safe_off.json and recheck/release_short.json (SPEC_LIMITERS.md 59, 64).

fn timing_signal() -> Vec<f64> {
    let n = 20 * SR;
    let mut a = vec![lin(-30.0); n];
    let e = SR / 1000;
    let rc: Vec<f64> = (0..e).map(|i| 0.5 - 0.5 * (PI * i as f64 / e as f64).cos()).collect();
    for t0 in [1usize, 9] {
        let (i, j) = (t0 * SR, ((t0 as f64 + 0.2) * SR as f64) as usize);
        for v in a[i..j].iter_mut() {
            *v = lin(-6.0);
        }
        for k in 0..e {
            a[i - e + k] = lin(-30.0) + (lin(-6.0) - lin(-30.0)) * rc[k];
            a[j + k] = lin(-6.0) - (lin(-6.0) - lin(-30.0)) * rc[k];
        }
    }
    for v in a[18 * SR..19 * SR].iter_mut() {
        *v = lin(-6.0);
    }
    q24(&(0..n).map(|i| a[i] * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect::<Vec<_>>())
}

struct Timing {
    gain_in_burst: f64,
    gain_end: f64,
    release_63_ms: f64,
    overshoot_db: f64,
}

fn timing_analyse(y: &[f64], x: &[f64], hx: &[f64]) -> Timing {
    let n = x.len();
    let env = hil(&y[..n]);
    let g: Vec<f64> = env.iter().zip(hx.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect();
    let off = (9.2 * SR as f64) as usize;
    let gb = median(&g[(9.1 * SR as f64) as usize..(9.19 * SR as f64) as usize]);
    let ge = median(&g[16 * SR..17 * SR]);
    let tgt = gb + 0.63 * (ge - gb);
    let hit = g[off..].iter().position(|v| *v >= tgt).unwrap_or(usize::MAX);
    let s = 18 * SR;
    let settled = median(&env[s + SR / 2..s + (0.9 * SR as f64) as usize]);
    let pk = max_abs(&y[s..s + SR / 20]);
    Timing { gain_in_burst: gb, gain_end: ge, release_63_ms: hit as f64 / SR as f64 * 1000.0, overshoot_db: 20.0 * (pk / settled).log10() }
}

fn tol_time(stored: f64) -> f64 {
    (0.1 * stored).max(0.3)
}

#[test]
fn t3_release_long_method() {
    let mut rep = Report::new("release, 1 kHz burst method");
    let x = timing_signal();
    let hx = hil(&x);
    // recheck/results_safe_off.json: (release position, 63 % recovery ms, gain in burst dB).
    let stored = [(0.0, 0.92, -3.55), (0.125, 2.02, -3.59), (0.25, 4.69, -3.6), (0.375, 8.79, -3.6), (0.5, 16.73, -3.6), (0.625, 34.02, -3.6), (0.75, 95.83, -3.6), (0.875, 308.54, -3.6), (1.0, 1039.54, -3.6)];
    for (p, t63, gb) in stored {
        let mut m = module(&[pos(P_THRESHOLD, 0.2), (P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), pos(P_ATTACK, 0.0), pos(P_RELEASE, p)]);
        let r = timing_analyse(&render(&mut m, &x, SR as f64), &x, &hx);
        let ms = plain_from_position(P_RELEASE, p);
        rep.row(&format!("release {ms:.3} ms: 63 % recovery ms"), r.release_63_ms, t63, tol_time(t63));
        rep.info(&format!("release {ms:.3} ms: gain in burst dB"), r.gain_in_burst, gb);
        rep.info(&format!("release {ms:.3} ms: gain at the end dB"), r.gain_end, 0.0);
    }
    // Same file, attack rows: step at a zero crossing, release 87.3 ms. Not an acceptance row
    // (the attack row of the brief names attack_peakstep.json); reported for SPEC_LIMITERS.md 59.
    let attack = [(0.0, 0.03, 95.83), (0.25, 0.38, 95.81), (0.5, 1.17, 96.62), (0.75, 2.06, 96.04), (1.0, 2.74, 96.04)];
    for (p, over, t63) in attack {
        let mut m = module(&[pos(P_THRESHOLD, 0.2), (P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), pos(P_RELEASE, 0.75), pos(P_ATTACK, p)]);
        let r = timing_analyse(&render(&mut m, &x, SR as f64), &x, &hx);
        let ms = plain_from_position(P_ATTACK, p);
        rep.info(&format!("attack {ms:.3} ms: overshoot dB, step at a zero crossing"), r.overshoot_db, over);
        rep.row(&format!("attack {ms:.3} ms: 63 % recovery ms"), r.release_63_ms, t63, tol_time(t63));
    }
    rep.finish();
}

#[test]
fn t3_release_short_method() {
    let mut rep = Report::new("release, 6 kHz step method");
    // recheck/release_short.json: per rate, (release position, 50 %, 63 %, 90 % recovery ms).
    let stored_96 = [(0.0, 0.917, 1.219, 1.75), (0.125, 2.042, 2.396, 3.729), (0.25, 3.896, 4.729, 7.229), (0.375, 7.542, 9.042, 13.729), (0.5, 14.208, 16.917, 26.583), (0.625, 27.698, 35.229, 68.229), (0.75, 71.531, 97.729, 211.875)];
    let stored_48 = [(0.0, 0.917, 1.083, 1.75), (0.125, 1.917, 2.396, 3.729), (0.25, 3.896, 4.729, 7.229), (0.375, 7.417, 8.917, 13.583), (0.5, 14.042, 16.875, 26.417), (0.625, 26.917, 34.562, 67.562), (0.75, 68.708, 94.917, 209.042)];
    for (sr, stored) in [(96_000usize, stored_96), (48_000usize, stored_48)] {
        let n = 4 * sr;
        let x: Vec<f64> = (0..n)
            .map(|i| {
                let a = if i >= sr && i < 2 * sr { lin(-6.0) } else { lin(-30.0) };
                a * (2.0 * PI * 6000.0 * i as f64 / sr as f64).sin()
            })
            .collect();
        let hx = hil(&x);
        for (p, t50, t63, t90) in stored {
            let mut m = module(&[pos(P_THRESHOLD, 0.2), (P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), (P_LIFT, 0.0), pos(P_ATTACK, 0.0), pos(P_RELEASE, p)]);
            let y = render(&mut m, &x, sr as f64);
            let ey = hil(&y[..n]);
            let g: Vec<f64> = ey.iter().zip(hx.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect();
            let off = 2 * sr;
            let gb = median(&g[off - sr / 10..off - sr / 100]);
            let ge = median(&g[off + sr..off + (1.5 * sr as f64) as usize]);
            let ms = plain_from_position(P_RELEASE, p);
            rep.row(&format!("{sr} Hz, release {ms:.3} ms: gain in burst dB"), gb, -3.6, 0.05);
            rep.row(&format!("{sr} Hz, release {ms:.3} ms: gain at the end dB"), ge, 0.0, 0.05);
            for (name, frac, st) in [("50 %", 0.5, t50), ("63 %", 0.63, t63), ("90 %", 0.9, t90)] {
                let hit = g[off + 2..].iter().position(|v| *v >= gb + frac * (ge - gb)).unwrap_or(usize::MAX - 2);
                let t = (hit + 2) as f64 / sr as f64 * 1000.0;
                rep.row(&format!("{sr} Hz, release {ms:.3} ms: {name} recovery ms"), t, st, tol_time(st));
            }
        }
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 4. Attack overshoot with the peak guard on and off. Script *_attack_peakstep.py; data
//    recheck/attack_peakstep.json (SPEC_LIMITERS.md 54.2).

fn half_cycle_peaks(y: &[f64], s: usize, count: usize) -> (f64, Vec<f64>, f64) {
    let settled = max_abs(&y[s + SR / 2..s + SR / 2 + 48]);
    let rel: Vec<f64> = (0..40).map(|i| round2(20.0 * (max_abs(&y[s + i * 24..s + (i + 1) * 24]) / settled).log10())).collect();
    let last = rel.iter().rposition(|v| *v > 0.1).map(|i| i as f64 + 1.0).unwrap_or(0.0);
    (settled, rel[..count].to_vec(), last * 0.5)
}

#[test]
fn t4_attack_peak_step() {
    let mut rep = Report::new("attack, step at a waveform peak");
    let n = 3 * SR;
    let s = SR;
    let x = q24(&(0..n)
        .map(|i| {
            let a = if i >= s && i < 2 * SR { lin(-6.0) } else { lin(-30.0) };
            a * (2.0 * PI * 1000.0 * i as f64 / SR as f64).cos()
        })
        .collect::<Vec<_>>());
    for (guard, attack_ms, settled_db, first, peaks, ms) in PEAK_STEP.iter() {
        // Script: threshold -9.6 dB, slow hold off, release 87.3 ms.
        let mut m = module(&[pos(P_THRESHOLD, 0.2), (P_SLOW_HOLD, 0.0), pos(P_RELEASE, 0.75), (P_PEAK_GUARD, *guard as u8 as f32), (P_ATTACK, *attack_ms)]);
        let y = render(&mut m, &x, SR as f64);
        let (settled, rel, within) = half_cycle_peaks(&y, s, 8);
        let tag = format!("peak guard {guard}, attack {attack_ms} ms");
        rep.row(&format!("{tag}: settled peak dBFS"), db(settled), *settled_db, 0.05);
        rep.row(&format!("{tag}: first loud sample, dB over settled"), 20.0 * (y[s].abs() / settled).log10(), *first, 0.3);
        for (k, (g, st)) in rel.iter().zip(peaks.iter()).enumerate() {
            rep.row(&format!("{tag}: half-cycle peak {k}, dB over settled"), *g, *st, 0.3);
        }
        rep.info(&format!("{tag}: ms until within 0.1 dB"), within, *ms);
        if *guard {
            let thr = lin(plain_from_position(P_THRESHOLD, 0.2) as f64);
            rep.check(&format!("{tag}: no sample above the threshold"), max_abs(&y) <= thr * (1.0 + 1e-6));
        }
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 5. Lift x peak guard grid. Script *_safe_dither.py part A; data recheck/safe_enhance.json
//    (SPEC_LIMITERS.md 56.1, 56.2).

#[test]
fn t5_lift_guard_grid() {
    let mut rep = Report::new("lift x peak guard grid");
    let steps = [-30.0, -24.0, -18.0, -12.0, -9.0, -6.0, -3.0, 0.0];
    let h = SR / 2;
    let mut x = Vec::new();
    for d in steps {
        x.extend(tone(1000.0, d, h, SR, 0.0));
    }
    x.extend(std::iter::repeat(0.0).take(SR));
    let s0 = steps.len() * h + SR + SR;
    for i in 0..3 * SR {
        let a = if i >= SR && i < 2 * SR { lin(-6.0) } else { lin(-30.0) };
        x.push(a * (2.0 * PI * 1000.0 * i as f64 / SR as f64).cos());
    }
    let x = q24(&x);
    for row in GRID.iter() {
        // Script: slow hold off, release 87.3 ms, attack maximum.
        let mut m = module(&[(P_THRESHOLD, row.thr), (P_SLOW_HOLD, 0.0), pos(P_RELEASE, 0.75), pos(P_ATTACK, 1.0), (P_PEAK_GUARD, row.guard as u8 as f32), (P_LIFT, row.lift)]);
        let y = render(&mut m, &x, SR as f64);
        let tag = format!("peak guard {}, lift {} %, threshold {} dB", row.guard, row.lift, row.thr);
        for (k, d) in steps.iter().enumerate() {
            let seg = &y[k * h + h / 4..(k + 1) * h - h / 8];
            rep.row(&format!("{tag}, input {d} dBFS: peak dBFS"), db(max_abs(seg)), row.peak[k], 0.2);
            rep.row(&format!("{tag}, input {d} dBFS: level from RMS dBFS"), db(sine_level(seg)), row.rms[k], 0.2);
            if *d == -12.0 || *d == 0.0 {
                let seg = &seg[..seg.len() / 48 * 48];
                let f = hann_spectrum(seg);
                let kk = (1000.0 * seg.len() as f64 / SR as f64).round() as usize;
                let fund: f64 = f[kk - 3..kk + 4].iter().map(|v| v * v).sum();
                let all: f64 = f[8..].iter().map(|v| v * v).sum();
                let thd = 10.0 * ((all - fund) / fund + 1e-30).log10();
                rep.info(&format!("{tag}, input {d} dBFS: THD+N dB"), thd, if *d == 0.0 { row.thd_0 } else { row.thd_m12 });
            }
        }
        let (settled, rel, within) = half_cycle_peaks(&y, s0, 6);
        rep.row(&format!("{tag}: step, settled peak dBFS"), db(settled), row.settled, 0.2);
        for (k, (g, st)) in rel.iter().zip(row.half_cycles.iter()).enumerate() {
            rep.row(&format!("{tag}: step, half-cycle peak {k}, dB over settled"), *g, *st, 0.3);
        }
        rep.info(&format!("{tag}: step, ms until within 0.1 dB"), within, row.ms);
        if row.guard {
            rep.check(&format!("{tag}: no sample above the threshold"), max_abs(&y) <= lin(row.thr as f64) * (1.0 + 1e-6));
        }
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 6. Slow hold and its recovery. Script *_autogain3.py; data recheck/autogain3.json
//    (SPEC_LIMITERS.md 71.5). Also autogain2.json (71.4) and the 71.2 table, reported.

fn bursts(n: usize, spans: &[(f64, f64, f64)]) -> Vec<f64> {
    let mut e = vec![lin(-30.0); n];
    for (a, b, level) in spans {
        for v in e[(a * SR as f64) as usize..(b * SR as f64) as usize].iter_mut() {
            *v = lin(*level);
        }
    }
    (0..n).map(|i| e[i] * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect()
}

fn slow_hold_sets(release_pos: f32, slow: bool) -> Vec<(usize, f32)> {
    vec![(P_PEAK_GUARD, 0.0), (P_LIFT, 0.0), pos(P_ATTACK, 0.0), pos(P_THRESHOLD, 0.2), (P_KNEE, 0.0), pos(P_RELEASE, release_pos), (P_SLOW_HOLD, slow as u8 as f32)]
}

fn window_median(g: &[f64], from_s: f64, len_s: f64) -> f64 {
    median(&g[(from_s * SR as f64) as usize..((from_s + len_s) * SR as f64) as usize])
}

#[test]
fn t6_slow_hold_against_reduction() {
    let mut rep = Report::new("slow hold against the amount of reduction");
    let levels = [-8.0, -6.0, -3.0, 0.0];
    let n = (2 + levels.len() * 10) * SR;
    let spans: Vec<(f64, f64, f64)> = levels.iter().enumerate().map(|(i, d)| (2.0 + i as f64 * 10.0, 3.0 + i as f64 * 10.0, *d)).collect();
    let x = bursts(n, &spans);
    let mut m = module(&slow_hold_sets(0.5, true));
    let g = gain_trace(&render(&mut m, &x, SR as f64), &x);
    // recheck/autogain3.json (both runs identical): reduction in the burst, gain at 0.1, 0.5, 1, 2, 3, 4, 6 s, s until within 0.1 dB.
    let stored = [
        (-1.6, [-0.82, -0.76, -0.58, -0.06, 0.0, 0.0, 0.0], 1.95),
        (-3.6, [-2.82, -2.67, -2.27, -1.1, -0.21, 0.0, 0.0], 3.21),
        (-6.6, [-5.81, -5.55, -4.81, -2.66, -1.03, -0.18, 0.0], 4.15),
        (-9.6, [-8.8, -8.42, -7.35, -4.22, -1.85, -0.61, 0.0], 4.74),
    ];
    for (i, (red, after, back)) in stored.iter().enumerate() {
        let end = 3.0 + i as f64 * 10.0;
        let tag = format!("burst {} dBFS", levels[i]);
        rep.row(&format!("{tag}: gain in the burst dB"), window_median(&g, end - 0.3, 0.2), *red, 0.3);
        for (dt, st) in [0.1, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0].iter().zip(after.iter()) {
            rep.row(&format!("{tag}: gain {dt} s after the burst dB"), window_median(&g, end + dt, 0.05), *st, 0.3);
        }
        let a = ((end + 0.05) * SR as f64) as usize;
        let b = ((end + 8.5) * SR as f64) as usize;
        let hit = g[a..b].iter().position(|v| *v > -0.1).map(|h| h as f64 / SR as f64 + 0.05).unwrap_or(f64::NAN);
        rep.info(&format!("{tag}: s until within 0.1 dB"), hit, *back);
    }
    rep.finish();
}

#[test]
fn t6_slow_hold_against_burst_length() {
    let mut rep = Report::new("slow hold against the burst length (SPEC_LIMITERS.md 71.4)");
    let lengths = [0.05, 0.2, 0.5, 1.0, 2.0, 5.0];
    let mut spans = Vec::new();
    let mut p = 2.0;
    for l in lengths {
        spans.push((p, p + l, -6.0));
        p += l + 9.0;
    }
    let n = (p * SR as f64) as usize;
    let x = bursts(n, &spans);
    let hx = hil(&x);
    for slow in [false, true] {
        let mut m = module(&slow_hold_sets(0.5, slow));
        let y = render(&mut m, &x, SR as f64);
        let g: Vec<f64> = hil(&y[..n]).iter().zip(hx.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect();
        for (i, (len, after, back)) in SLOW_LENGTH.iter().enumerate() {
            let end = spans[i].1;
            for (dt, st) in [0.1, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0].iter().zip(after.iter()) {
                // recheck/autogain2.json; with the slow hold off the stored gain is 0.00 dB from 0.1 s on.
                let st = if slow { *st } else { 0.0 };
                rep.row(&format!("slow hold {slow}, burst {len} s: gain {dt} s after dB"), window_median(&g, end + dt, 0.05), st, 0.3);
            }
            if slow {
                let a = ((end + 0.05) * SR as f64) as usize;
                let b = ((end + 8.5) * SR as f64) as usize;
                let hit = g[a..b].iter().position(|v| *v > -0.1).map(|h| h as f64 / SR as f64 + 0.05).unwrap_or(f64::NAN);
                rep.info(&format!("burst {len} s: s until within 0.1 dB"), hit, *back);
            }
        }
    }
    rep.finish();
}

#[test]
fn t6_slow_hold_and_release_setting() {
    let mut rep = Report::new("slow hold with two release settings (SPEC_LIMITERS.md 71.2)");
    let n = 26 * SR;
    let x = bursts(n, &[(3.0, 8.0, -6.0)]);
    let hx = hil(&x);
    for (slow, release_ms, stored) in SLOW_TIME.iter() {
        let mut sets = slow_hold_sets(0.5, *slow);
        sets.push((P_RELEASE, *release_ms));
        let mut m = module(&sets);
        let y = render(&mut m, &x, SR as f64);
        let g: Vec<f64> = hil(&y[..n]).iter().zip(hx.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect();
        for (t, st) in SLOW_TIME_AT_S.iter().zip(stored.iter()) {
            // recheck/knee_autogain.json "auto_gain_time". Same 0.3 dB as the slow-hold row of the brief.
            rep.row(&format!("slow hold {slow}, release {release_ms} ms: gain at {t} s dB"), window_median(&g, *t, 0.1), *st, 0.3);
        }
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 7. True-peak hold. Script *_knee_autogain.py part C; data recheck/knee_autogain.json
//    "auto_comp" (SPEC_LIMITERS.md 71.3).

/// One 3 s section of the composite hot signal: 1 kHz at -18 dBFS with a burst at -2 dBFS.
fn hot_burst(ms: f64) -> Vec<f64> {
    let n = 3 * SR;
    (0..n)
        .map(|i| {
            let a = if i >= SR / 2 && i < SR / 2 + (ms * SR as f64 / 1000.0) as usize { lin(-2.0) } else { lin(-18.0) };
            a * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()
        })
        .collect()
}

/// The composite hot programme signal of the measurement side (left, right), 34.5 s.
fn hot_signal() -> (Vec<f64>, Vec<f64>) {
    let mut l = vec![0.0; SR];
    let mut d = -36;
    while d <= 0 {
        l.extend(tone(1000.0, d as f64, SR / 2, SR, 0.0));
        d += 2;
    }
    for ms in [1.0, 5.0, 20.0, 100.0, 500.0] {
        l.extend(hot_burst(ms));
    }
    let mut r = l.clone();
    l.extend(hot_burst(100.0));
    r.extend(tone(1000.0, -18.0, 3 * SR, SR, 0.0));
    let mut imp = vec![0.0; SR];
    imp[SR / 2] = 0.25;
    l.extend(imp.iter());
    r.extend(imp.iter());
    // Gated noise: built with the script's own generator lines by tests/data/make_noise_fixture.py.
    let bytes = include_bytes!("data/noise_prog_f32le.bin");
    let noise: Vec<f64> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64).collect();
    assert_eq!(noise.len(), 4 * SR);
    l.extend(noise.iter());
    r.extend(noise.iter());
    (q24(&l), q24(&r))
}

#[test]
fn t7_true_peak_hold() {
    let mut rep = Report::new("true-peak hold");
    let levels = [-9.0, -6.0, -3.0, 0.0];
    let mut x = Vec::new();
    for d in levels {
        x.extend(tone(12_000.0, d, SR, SR, PI / 4.0));
    }
    let (hot_l, _) = hot_signal();
    x.extend(hot_l[20 * SR..28 * SR].iter());
    // recheck/knee_autogain.json "auto_comp": (true-peak hold, threshold dB, [sample peak dBFS, true peak dBTP] per tone, programme [peak, true peak, rms]).
    let stored = [
        (false, 0.0, [[-12.01, -9.0], [-9.01, -6.0], [-6.01, -3.0], [-3.01, 0.0]], [-2.0, -2.0, -14.88]),
        (false, -6.0, [[-12.01, -9.0], [-9.01, -6.0], [-6.01, -3.0], [-6.0, -2.99]], [-5.97, -5.97, -17.62]),
        (true, 0.0, [[-12.01, -9.0], [-9.01, -6.0], [-6.01, -3.0], [-3.01, 0.0]], [-2.0, -2.0, -14.88]),
        (true, -6.0, [[-12.01, -9.0], [-9.01, -6.0], [-8.85, -5.84], [-8.85, -5.84]], [-6.0, -6.0, -17.62]),
    ];
    for (tp, thr, tones, prog) in stored {
        let mut m = module(&[(P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), (P_LIFT, 0.0), pos(P_RELEASE, 0.5), pos(P_ATTACK, 0.0), (P_KNEE, 0.0), (P_THRESHOLD, thr), (P_TRUE_PEAK, tp as u8 as f32)]);
        let y = render(&mut m, &x, SR as f64);
        let tag = format!("true-peak hold {tp}, threshold {thr} dB");
        for (k, d) in levels.iter().enumerate() {
            let s = &y[k * SR + SR / 4..(k + 1) * SR - SR / 8];
            let u = up(s, 8);
            rep.row(&format!("{tag}, tone {d} dBTP: sample peak dBFS"), db(max_abs(s)), tones[k][0], 0.2);
            rep.row(&format!("{tag}, tone {d} dBTP: true peak dBTP"), db(max_abs(&u[4000..u.len() - 4000])), tones[k][1], 0.2);
        }
        let s = &y[4 * SR + SR / 4..12 * SR - SR / 4];
        let u = up(s, 8);
        rep.row(&format!("{tag}, programme: sample peak dBFS"), db(max_abs(s)), prog[0], 0.2);
        rep.row(&format!("{tag}, programme: true peak dBTP"), db(max_abs(&u[4000..u.len() - 4000])), prog[1], 0.2);
        rep.row(&format!("{tag}, programme: RMS dBFS"), db(rms(s)), prog[2], 0.2);
    }
    rep.finish();
}

// ---------------------------------------------------------------------------------------------
// 9. No sample above full scale. Script float_ceiling_check.py; data float_check/results.json
//    (SPEC_LIMITERS.md 57).

#[test]
fn t9_no_sample_above_full_scale() {
    let mut rep = Report::new("no sample above full scale");
    let (mut l, mut r) = hot_signal();
    l.resize(37 * SR, 0.0);
    r.resize(37 * SR, 0.0);
    // float_check/results.json: (case, settings on top of the defaults, sample peak dBFS, true peak dBTP 4x); samples above full scale 0 in all.
    let cases: [(&str, Vec<(usize, f32)>, f64, f64); 4] = [
        ("defaults, input +18 dB", vec![(P_INPUT_GAIN, 18.0)], -0.0, 2.95),
        ("slow hold off, peak guard off, input +18 dB", vec![(P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), (P_INPUT_GAIN, 18.0)], 0.0, 3.04),
        ("slow hold off, peak guard off, attack 1.04 ms, input +18 dB", vec![(P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), pos(P_ATTACK, 1.0), (P_INPUT_GAIN, 18.0)], 0.0, 5.8),
        ("true-peak hold on, input +18 dB", vec![(P_TRUE_PEAK, 1.0), (P_INPUT_GAIN, 18.0)], -0.0, 1.35),
    ];
    for (name, sets, peak, tp) in cases.iter() {
        let mut m = module(sets);
        let (yl, yr) = render_stereo(&mut m, &l, &r, SR as f64);
        let above = yl.iter().chain(yr.iter()).filter(|v| v.abs() > 1.0).count();
        rep.row(&format!("{name}: samples above full scale"), above as f64, 0.0, 0.0);
        rep.info(&format!("{name}: sample peak dBFS"), db(max_abs(&yl)), *peak);
        rep.info(&format!("{name}: true peak dBTP, 4x, 20 to 36 s"), db(max_abs(&up(&yl[20 * SR..36 * SR], 4))), *tp);
    }
    rep.finish();
}
