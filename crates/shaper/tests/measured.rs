//! Measured-against-stored tests. Reference numbers are hardcoded with the data file they come
//! from. Analysis follows `measurements/scripts/srcG_jobs.py` (gain = output RMS minus input
//! RMS over a steady window; distortion = root-sum-square of harmonics 2..9 relative to the
//! fundamental), done here by exact correlation over a whole number of cycles.

use olm_api::Module;
use shaper::*;

const SR: f32 = 48000.0;
const BLOCK: usize = 512;

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

fn make(curve: usize, thr: f32, os: bool) -> Shaper {
    let mut s = Shaper::default();
    s.set_param(P_CURVE, curve as f32);
    s.set_param(P_THRESHOLD, thr);
    s.set_param(P_OVERSAMPLE, if os { 1.0 } else { 0.0 });
    s.prepare(SR, BLOCK);
    s
}

/// Runs a mono signal on both channels in blocks; returns the left output.
fn run(s: &mut Shaper, x: &[f32]) -> Vec<f32> {
    let mut l = x.to_vec();
    let mut r = x.to_vec();
    let mut i = 0;
    while i < l.len() {
        let e = (i + BLOCK).min(l.len());
        s.process(&mut l[i..e], &mut r[i..e]);
        i = e;
    }
    assert_eq!(l, r);
    l
}

fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / SR as f64).sin()) as f32).collect()
}

struct Tone {
    gain_db: f64,
    thd_db: f64,
    h: [f64; 10], // h[k] = harmonic k relative to fundamental, dB (h[1] = 0)
    peak_db: f64,
}

/// Steady-window analysis of a sine of amplitude `amp` at `f0`.
fn analyse(y: &[f32], f0: f64, amp: f64, start: usize, len: usize) -> Tone {
    let w = &y[start..start + len];
    let rms = (w.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / len as f64).sqrt();
    let mut mag = [0.0f64; 10];
    for k in 1..10 {
        let om = 2.0 * std::f64::consts::PI * f0 * k as f64 / SR as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in w.iter().enumerate() {
            let ph = om * (start + i) as f64;
            re += *v as f64 * ph.cos();
            im -= *v as f64 * ph.sin();
        }
        mag[k] = (re * re + im * im).sqrt();
    }
    let mut h = [0.0; 10];
    for k in 1..10 {
        h[k] = db(mag[k] / mag[1]);
    }
    let thd = db((2..10).map(|k| mag[k] * mag[k]).sum::<f64>().sqrt() / mag[1]);
    let peak = w.iter().fold(0.0f64, |a, v| a.max((*v as f64).abs()));
    Tone { gain_db: db(rms / (amp / 2f64.sqrt())), thd_db: thd, h, peak_db: db(peak) }
}

/// 1 kHz sine at `level_dbfs` peak through `s`; window 0.1 s .. 0.5 s (400 whole cycles).
fn tone_1k(s: &mut Shaper, level_dbfs: f64) -> Tone {
    let amp = 10f64.powf(level_dbfs / 20.0);
    let x = sine(1000.0, amp, 24000);
    s.reset();
    let y = run(s, &x);
    analyse(&y, 1000.0, amp, 4800, 19200)
}

fn close(what: &str, got: f64, want: f64, tol: f64) -> bool {
    let ok = (got - want).abs() <= tol;
    println!("{:<58} measured {:>8.2}  stored {:>8.2}  tol {:>4.2}  {}", what, got, want, tol, if ok { "PASS" } else { "FAIL" });
    ok
}

const STEPS: [f64; 9] = [-48.0, -42.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, 0.0];

// ------------------------------------------------------------------ G: three curve types

/// measurements/srcG/jobs_s0.json, "softclip static | on, threshold -6, SOFT/HARD, gain 0",
/// last step (0 dBFS in): static_gain_db, thd_db_rel_fundamental, out_peak_dbfs.
#[test]
fn knee_types_9db_and_2db_against_stored() {
    for os in [false, true] {
        let mut all = true;
        for (curve, name, g, thd, pk) in [
            (CURVE_QUAD_9DB, "9 dB knee", -0.39, -32.4, -0.61),
            (CURVE_QUAD_2DB, "2 dB knee", -2.46, -15.7, -4.00),
        ] {
            let mut s = make(curve, -6.0, os);
            // below and at the threshold: gain 0.00 (all lower steps in the same rows)
            for lv in &STEPS[..8] {
                let t = tone_1k(&mut s, *lv);
                all &= close(&format!("{name} os={os} {lv} dBFS gain"), t.gain_db, 0.0, 0.1);
            }
            let t = tone_1k(&mut s, 0.0);
            all &= close(&format!("{name} os={os} 0 dBFS gain"), t.gain_db, g, 0.1);
            all &= close(&format!("{name} os={os} 0 dBFS THD"), t.thd_db, thd, 1.5);
            all &= close(&format!("{name} os={os} 0 dBFS out peak"), t.peak_db, pk, 0.1);
        }
        assert!(all);
    }
}

/// measurements/srcG/jobs_s0.json, "softclip static | on, threshold -12 / -6, SMOOTH, gain 0".
/// The threshold 0 row is not used at 0 dBFS in: its -0.23 dB there is the limiter (ceiling -0.2,
/// output -0.26), not the clipper. THD cells more than 24 dB below the threshold are at the
/// method floor (about -97 to -122 dB, not on the 2 dB-per-dB line) and are skipped.
#[test]
fn continuous_knee_against_stored() {
    let rows: [(f32, [f64; 9], [f64; 9], [f64; 9]); 2] = [
        (
            -6.0,
            [0.0, 0.0, 0.0, 0.0, 0.0, -0.01, -0.02, -0.09, -0.33],
            [-105.4, -107.7, -107.2, -97.3, -85.6, -73.6, -61.6, -49.7, -37.9],
            [-48.0, -42.0, -36.0, -30.0, -24.0, -18.01, -12.03, -6.11, -0.44],
        ),
        (
            -12.0,
            [0.0, 0.0, 0.0, 0.0, -0.01, -0.02, -0.09, -0.33, -1.21],
            [-99.1, -121.9, -96.7, -85.5, -73.6, -61.6, -49.7, -37.9, -26.8],
            [-48.0, -42.0, -36.0, -30.0, -24.01, -18.03, -12.11, -6.44, -1.6],
        ),
    ];
    for os in [false, true] {
        let mut all = true;
        for (thr, gains, thds, pks) in rows.iter() {
            let mut s = make(CURVE_CONTINUOUS, *thr, os);
            for (i, lv) in STEPS.iter().enumerate() {
                let t = tone_1k(&mut s, *lv);
                all &= close(&format!("continuous thr {thr} os={os} {lv} dBFS gain"), t.gain_db, gains[i], 0.1);
                all &= close(&format!("continuous thr {thr} os={os} {lv} dBFS peak"), t.peak_db, pks[i], 0.1);
                if *lv - *thr as f64 >= -24.0 {
                    all &= close(&format!("continuous thr {thr} os={os} {lv} dBFS THD"), t.thd_db, thds[i], 1.5);
                }
            }
        }
        assert!(all);
    }
}

// ------------------------------------------------------------------ F: odd soft saturation

/// measurements/srcF/round1_all.json, "soft clip | L/St -6 dB, R same | steps 1 kHz":
/// static_gain_db last step -0.71, harmonics_at_0dBFS_db_rel_fund [h2, h3, h4, h5] =
/// [-171.7, -27.0, -156.6, -43.8], peak_at_0dBFS -1.15; all lower steps 0.0.
#[test]
fn odd_saturation_minus6_full_scale_against_stored() {
    for os in [false, true] {
        let mut all = true;
        let mut s = make(CURVE_QUAD_6DB, -6.0, os);
        for lv in &STEPS[..8] {
            let t = tone_1k(&mut s, *lv);
            all &= close(&format!("6 dB knee thr -6 os={os} {lv} dBFS gain"), t.gain_db, 0.0, 0.1);
        }
        let t = tone_1k(&mut s, 0.0);
        all &= close(&format!("6 dB knee thr -6 os={os} 0 dBFS gain"), t.gain_db, -0.71, 0.1);
        all &= close(&format!("6 dB knee thr -6 os={os} 0 dBFS 3rd harmonic"), t.h[3], -27.0, 1.5);
        all &= close(&format!("6 dB knee thr -6 os={os} 0 dBFS 5th harmonic"), t.h[5], -43.8, 1.5);
        all &= close(&format!("6 dB knee thr -6 os={os} 0 dBFS out peak"), t.peak_db, -1.15, 0.1);
        // even harmonics: stored below -150 dB; float32 I/O limits what we can show, so require < -100
        let ok_even = t.h[2] < -100.0 && t.h[4] < -100.0;
        println!("6 dB knee os={os} even harmonics h2 {:.1} h4 {:.1} dB (< -100 required) {}", t.h[2], t.h[4], if ok_even { "PASS" } else { "FAIL" });
        assert!(all && ok_even);
    }
}

/// params/srcF.md section 8 (measurements/srcF/round2_all.json, "soft clip curve |
/// normalizer +12, soft clip 0, output -12", gain_L_db with the +0.10 dB normaliser/output read
/// offset removed): gain at +2, +4, +6, +8, +10, +12 dB over the curve point.
#[test]
fn odd_saturation_curve_shape_against_stored() {
    let stored = [(2.0, -0.05), (4.0, -0.26), (6.0, -0.70), (8.0, -1.44), (10.0, -2.62), (12.0, -4.16)];
    for os in [false, true] {
        let mut all = true;
        let mut s = make(CURVE_QUAD_6DB, 0.0, os);
        let t0 = tone_1k(&mut s, 0.0);
        all &= close(&format!("6 dB knee thr 0 os={os} 0 dBFS gain"), t0.gain_db, 0.0, 0.1);
        for (lv, g) in stored {
            let t = tone_1k(&mut s, lv);
            all &= close(&format!("6 dB knee thr 0 os={os} +{lv} dBFS gain"), t.gain_db, g, 0.1);
        }
        // the -6 setting is the same curve 6 dB lower (round2_all.json, soft clip -6 row)
        let mut s6 = make(CURVE_QUAD_6DB, -6.0, os);
        for (lv, g) in stored {
            let t = tone_1k(&mut s6, lv - 6.0);
            all &= close(&format!("6 dB knee thr -6 os={os} {} dBFS gain", lv - 6.0), t.gain_db, g, 0.1);
        }
        assert!(all);
    }
}

/// measurements/srcF/round3_all.json, "M/S, hot, soft clip L/St -6 only": 4 kHz at -1 dBFS peak,
/// left-only part: L_peak -1.77 dBFS, L_dbfs -4.47 (dry -4.01, so -0.46 dB).
#[test]
fn odd_saturation_4k_against_stored() {
    let mut all = true;
    for os in [false, true] {
        let mut s = make(CURVE_QUAD_6DB, -6.0, os);
        let amp = 10f64.powf(-1.0 / 20.0);
        let x = sine(4000.0, amp, 24000);
        let y = run(&mut s, &x);
        let w = &y[4800..4800 + 19200];
        let rms = (w.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w.len() as f64).sqrt();
        let pk = w.iter().fold(0.0f64, |a, v| a.max((*v as f64).abs()));
        all &= close(&format!("6 dB knee 4 kHz -1 dBFS os={os} RMS change"), db(rms) - db(amp / 2f64.sqrt()), -0.46, 0.1);
        all &= close(&format!("6 dB knee 4 kHz -1 dBFS os={os} out peak"), db(pk), -1.77, 0.1);
    }
    assert!(all);
}

// ------------------------------------------------------------------ D: dense curves and mix

/// Programme proxy for SPEC_LIMITERS.md 48.2 (the stored programme signal is a .wav, which this
/// implementation may not read). A 40-tone pinkish multitone, densified with tanh to the stored
/// "Soft Clip off" state: RMS -3.87 dBFS at sample peak 0.0 dBFS.
pub fn dense_proxy() -> Vec<f32> {
    let n = 96000;
    let mut st: u64 = 0x1234567;
    let mut x = vec![0.0f64; n];
    for i in 0..40 {
        st = st.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ph = 2.0 * std::f64::consts::PI * ((st >> 11) as f64 / (1u64 << 53) as f64);
        let f = 50.0 * (10000.0f64 / 50.0).powf(i as f64 / 39.0);
        for (k, v) in x.iter_mut().enumerate() {
            *v += (2.0 * std::f64::consts::PI * f * k as f64 / SR as f64 + ph).sin() / f.sqrt();
        }
    }
    let pk = x.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    let a = 3.652_236_764_5; // tanh drive giving -3.87 dBFS RMS (solved by bisection offline)
    let y: Vec<f64> = x.iter().map(|v| (a * v / pk).tanh()).collect();
    let pk2 = y.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    y.iter().map(|v| (v / pk2) as f32).collect()
}

fn rms_db(x: &[f32]) -> f64 {
    db((x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt())
}

/// SPEC_LIMITERS.md 48.2, measurements/srcD_softclip_m/results.json: off -3.87 dBFS; level at
/// mix 33 / 67 / 100 %: L -3.43 / -2.97 / -2.54, M -3.24 / -2.59 / -2.0, H -3.14 / -2.37 / -1.71.
#[test]
fn dense_curves_level_rise_against_stored() {
    let x = dense_proxy();
    let off = rms_db(&x);
    let mut all = close("proxy RMS (stored off state)", off, -3.87, 0.02);
    let stored = [
        (CURVE_DENSE_LIGHT, "light", [-3.43, -2.97, -2.54]),
        (CURVE_DENSE_MEDIUM, "medium", [-3.24, -2.59, -2.0]),
        (CURVE_DENSE_HEAVY, "heavy", [-3.14, -2.37, -1.71]),
    ];
    for os in [false, true] {
        let lat = if os { OS_LATENCY } else { 0 };
        for (curve, name, lv) in stored {
            for (i, mix) in [33.0f32, 67.0, 100.0].iter().enumerate() {
                let mut s = make(curve, 0.0, os);
                s.set_param(P_MIX, *mix);
                s.reset();
                let y = run(&mut s, &x);
                let rise = rms_db(&y[lat..]) - rms_db(&x[..x.len() - lat]);
                all &= close(&format!("dense {name} os={os} mix {mix} % level rise"), rise, lv[i] - (-3.87), 0.1);
                if !os {
                    // a peak at the threshold stays at the threshold (sample peak 0.0 dBFS in 48.2)
                    let pk = y.iter().fold(0.0f64, |a, v| a.max((*v as f64).abs()));
                    all &= pk <= 1.0 + 1e-6;
                }
            }
        }
    }
    assert!(all);
}

// ------------------------------------------------------------------ pass-through

fn noise(n: usize) -> Vec<f32> {
    let mut st: u64 = 99;
    (0..n)
        .map(|_| {
            st = st.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (((st >> 40) as f64 / (1u64 << 24) as f64) * 2.0 - 1.0) as f32 * 1.5
        })
        .collect()
}

#[test]
fn off_and_mix_zero_are_exact_pass_through() {
    let x = noise(20000);
    for os in [false, true] {
        let lat = if os { OS_LATENCY } else { 0 };
        // curve off
        let mut s = make(CURVE_OFF, -12.0, os);
        assert_eq!(s.latency(), lat);
        let y = run(&mut s, &x);
        assert!(y[lat..].iter().zip(&x).all(|(a, b)| a.to_bits() == b.to_bits()), "off os={os}");
        assert!(y[..lat].iter().all(|v| *v == 0.0));
        // every curve at mix 0
        for c in 1..8 {
            let mut s = make(c, -12.0, os);
            s.set_param(P_MIX, 0.0);
            s.reset();
            let y = run(&mut s, &x);
            assert!(y[lat..].iter().zip(&x).all(|(a, b)| a.to_bits() == b.to_bits()), "curve {c} mix 0 os={os}");
        }
        // upward and transient at 0 (their neutral ends) with the curve off: still exact
        let mut s = make(CURVE_OFF, 0.0, os);
        s.set_param(P_UPWARD, 0.0);
        s.set_param(P_TRANSIENT, 0.0);
        let y = run(&mut s, &x);
        assert!(y[lat..].iter().zip(&x).all(|(a, b)| a.to_bits() == b.to_bits()));
    }
    println!("pass-through: bit-exact (delayed by the reported latency when oversampling)  PASS");
}

#[test]
fn oversampling_latency_alignment_below_threshold() {
    // A knee curve below its threshold is the identity, so the oversampled path must line up with
    // the reported latency: 1 kHz at -20 dBFS, threshold 0.
    let mut s = make(CURVE_QUAD_9DB, 0.0, true);
    let x = sine(1000.0, 0.1, 9600);
    let y = run(&mut s, &x);
    let err = (4800..9600).map(|i| (y[i] - x[i - OS_LATENCY]).abs() as f64).fold(0.0, f64::max);
    println!("oversampled path vs input delayed by {OS_LATENCY}: max error {:.1} dB re signal", db(err / 0.1));
    assert!(db(err / 0.1) < -60.0);
}

/// Own addition: aliasing check. 7 kHz at 0 dBFS through the 2 dB knee at -6: harmonics 7 and 5
/// (49 and 35 kHz) fold to 1 and 13 kHz without oversampling.
#[test]
fn oversampling_reduces_aliasing() {
    let x = sine(7000.0, 1.0, 14400);
    let mut res = [0.0; 2];
    for (j, os) in [false, true].iter().enumerate() {
        let mut s = make(CURVE_QUAD_2DB, -6.0, *os);
        let y = run(&mut s, &x);
        let w = &y[4800..9600];
        let bin = |f: f64| {
            let om = 2.0 * std::f64::consts::PI * f / SR as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, v) in w.iter().enumerate() {
                re += *v as f64 * (om * i as f64).cos();
                im += *v as f64 * (om * i as f64).sin();
            }
            (re * re + im * im).sqrt()
        };
        let f1 = bin(7000.0);
        let alias = (bin(1000.0).powi(2) + bin(13000.0).powi(2)).sqrt();
        res[j] = db(alias / f1);
    }
    println!("aliases at 1 and 13 kHz re 7 kHz: os off {:.1} dB, os on {:.1} dB", res[0], res[1]);
    assert!(res[1] < res[0] - 20.0);
}

// ------------------------------------------------------------------ D: upward and transient

/// Dynamic programme proxy (own design; the stored programme .wav may not be read): the same
/// multitone under a 0.5 s level pattern 0 / -10 / -4 / -18 dB and a 125 ms percussive envelope,
/// scaled to -1 dBFS peak.
pub fn dynamic_proxy() -> Vec<f32> {
    let base: Vec<f32> = {
        let n = 192000;
        let mut st: u64 = 0x1234567;
        let mut x = vec![0.0f64; n];
        for i in 0..40 {
            st = st.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let ph = 2.0 * std::f64::consts::PI * ((st >> 11) as f64 / (1u64 << 53) as f64);
            let f = 50.0 * (10000.0f64 / 50.0).powf(i as f64 / 39.0);
            for (k, v) in x.iter_mut().enumerate() {
                *v += (2.0 * std::f64::consts::PI * f * k as f64 / SR as f64 + ph).sin() / f.sqrt();
            }
        }
        x.iter().map(|v| *v as f32).collect()
    };
    let levels = [0.0, -10.0, -4.0, -18.0];
    let y: Vec<f64> = base
        .iter()
        .enumerate()
        .map(|(k, v)| {
            let t = k as f64 / SR as f64;
            let sec = ((t / 0.5) as usize) % 4;
            let lv = 10f64.powf(levels[sec] / 20.0);
            let ph = t % 0.125;
            let perc = (1.0 + 2.0 * (-ph / 0.02).exp()) / 3.0;
            *v as f64 * lv * perc
        })
        .collect();
    let pk = y.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    y.iter().map(|v| (v / pk * 10f64.powf(-1.0 / 20.0)) as f32).collect()
}

fn dyn_run(upward: f32, transient: f32) -> f64 {
    let x = dynamic_proxy();
    let mut s = make(CURVE_OFF, 0.0, false);
    // calibration point of the upward stage on this proxy (own choice, see REPORT.md)
    s.set_param(P_UPWARD_THRESHOLD, -15.5);
    s.set_param(P_UPWARD, upward);
    s.set_param(P_TRANSIENT, transient);
    s.reset();
    let y = run(&mut s, &x);
    rms_db(&y[24000..]) - rms_db(&x[24000..])
}

/// SPEC_LIMITERS.md 49 table, "+8 dB, Character 5" row (measurements/srcD_upward_transient_grid.json):
/// off -7.12, amount 2.5 / 5 / 10 dB: -7.01 / -6.88 / -6.47, so rises of +0.11 / +0.24 / +0.65 dB.
/// The stored values are the whole maximizer's output; here the stage alone on the proxy.
#[test]
fn upward_compression_small_effect() {
    let mut all = true;
    for (amt, rise) in [(2.5f32, 0.11), (5.0, 0.24), (10.0, 0.65)] {
        all &= close(&format!("upward {amt} dB, proxy RMS rise"), dyn_run(amt, 0.0), rise, 0.1);
    }
    assert!(all);
}

/// SPEC_LIMITERS.md 49 table, "+8 dB, Character 5" row: off -7.12, 50 / 150 / 200 %:
/// -7.02 / -6.82 / -6.73, so rises of +0.10 / +0.30 / +0.39 dB; 0 % neutral (52).
#[test]
fn transient_shaping_small_effect() {
    let mut all = true;
    for (amt, rise) in [(50.0f32, 0.10), (150.0, 0.30), (200.0, 0.39)] {
        all &= close(&format!("transient {amt} %, proxy RMS rise"), dyn_run(0.0, amt), rise, 0.1);
    }
    assert!(all);
}

// ------------------------------------------------------------------ robustness

#[test]
fn parameters_clamp_and_no_nan() {
    let mut s = Shaper::default();
    assert_eq!(s.params().len(), 8);
    for (i, p) in s.params().iter().enumerate() {
        s.set_param(i, p.max + 1000.0);
        s.set_param(i, f32::NAN);
    }
    s.set_param(99, 1.0);
    s.prepare(SR, BLOCK);
    let mut x = noise(4096);
    x[100] = 1e30;
    let y = run(&mut s, &x);
    assert!(y.iter().all(|v| v.is_finite()));
    let mut z = vec![0.0f32; 1024];
    let mut z2 = z.clone();
    s.process(&mut z, &mut z2);
    assert!(z.iter().all(|v| v.is_finite()));
}
