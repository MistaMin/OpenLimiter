//! The owner's decisions (design/smart-limiter-decisions.md), one test per item:
//! 1 Join Bend without the dense-noise ratchet, separate-burst join still exact;
//! 2 bandFocus default 100 %, band 0 post-hold 12 ms (50 Hz);
//! 4 79-tap stage-1 halfband: passband flat to 21 kHz (48k) / 19.5 kHz (44.1k), Punch 23 kHz alias;
//! 5 ceiling glides over 5 ms, the output never passes the gliding ceiling and does not click;
//! 6 Punch delta nulls exactly below threshold.

mod common;
use common::*;
use limiter_smart::lane::{join_bend, Lane};
use limiter_smart::{idx, SmartLimiter};
use olm_api::Module;

// ---------------------------------------------------------------------------------------------
// 1. Join Bend

/// Output level of Pure on dense noise at +12 dB drive: (largest sample peak, mean of the
/// 10 ms block peaks, RMS), dBFS, over 1..4 s.
fn dense(join: f32, x: &[f64]) -> (f64, f64, f64) {
    let mut m = make(SR, &[engine(0), (idx::DRIVE, 12.0), (idx::JOIN_WIDTH, join)]);
    let (y, _) = run(&mut m, x, x);
    let seg = &y[48000..4 * 48000];
    let blocks: Vec<f64> = seg.chunks(480).map(peak).collect();
    let mean_pk = blocks.iter().map(|v| db(*v)).sum::<f64>() / blocks.len() as f64;
    let rms = (seg.iter().map(|v| v * v).sum::<f64>() / seg.len() as f64).sqrt();
    (db(peak(seg)), mean_pk, db(rms))
}

#[test]
fn join_bend_does_not_ratchet_on_dense_noise() {
    let x = noise(4 * 48000, 0.25, 7);
    let (p6, b6, r6) = dense(6.0, &x);
    let (p0, b0, r0) = dense(0.0, &x);
    println!("Pure, dense noise +12 dB drive, ceiling -0.2: joinWidth 6 dB: peak {p6:.3} dBFS, mean 10 ms block peak {b6:.3} dBFS, RMS {r6:.2} dBFS");
    println!("                                           joinWidth 0 dB: peak {p0:.3} dBFS, mean 10 ms block peak {b0:.3} dBFS, RMS {r0:.2} dBFS");
    let tone = sine(3000.0, lin(-0.2 + 12.0), 4 * 48000, SR, 0.0);
    let t6 = dense(6.0, &tone);
    let t0 = dense(0.0, &tone);
    println!("3 kHz tone +12 dB over: joinWidth 6: mean block peak {:.3} dBFS, joinWidth 0: {:.3} dBFS", t6.1, t0.1);
    assert!(p6 > -0.5 && (p6 - p0).abs() < 0.1, "peaks must reach the ceiling");
    assert!((b6 - b0).abs() < 0.5, "Join Bend must not cost loudness on dense noise: {b6} vs {b0}");
    assert!((r6 - r0).abs() < 0.5);
}

#[test]
fn join_bend_on_separate_bursts_is_the_exact_integral() {
    // burst 1 holds the lane at -10 dB; 200 ms at -2 dB (the release is slow, 2 s, so the
    // released value is still far below); then burst 2 steps the hold down to -8 dB, in one
    // step or over 8 samples. Reference: the continuous join applied directly to the release
    // (the DualSone form, `h += join_bend(h, m_prev, m)` on every downward step).
    let fs = 48_000.0;
    let c_rel = limiter_smart::dsp::pole(2000.0, fs);
    let w = 6.0;
    let run = |steps: usize| -> (f64, f64) {
        let mut lane = Lane::default();
        lane.alloc(100, 0, 1, 0);
        lane.set_target(2, 0);
        lane.snap();
        let mut seq = vec![0.0; 40];
        seq.extend(vec![-10.0; 4]);
        seq.extend(vec![-2.0; 9600]);
        for i in 0..steps {
            seq.push(-2.0 - 6.0 * (i + 1) as f64 / steps as f64);
        }
        let mut out = 0.0;
        let (mut h, mut mp) = (0.0f64, 0.0f64);
        // the lane's hold is a sliding minimum over 4 samples: feed the reference the same
        let mut hist = vec![0.0f64; 4];
        for r in seq {
            out = lane.hold_release(r, c_rel, w);
            hist.remove(0);
            hist.push(r);
            let m = hist.iter().cloned().fold(f64::INFINITY, f64::min);
            if m <= h {
                h = m;
            } else {
                h = m + c_rel * (h - m);
                if m < mp {
                    h = (h + join_bend(h, mp, m, w)).min(m);
                }
            }
            mp = m;
        }
        (out, h)
    };
    let (a1, r1) = run(1);
    let (a8, r8) = run(8);
    println!("separate bursts, second burst in 1 step: lane {a1:.5} dB, continuous reference {r1:.5} dB (difference {:.2e})", (a1 - r1).abs());
    println!("separate bursts, second burst in 8 steps: lane {a8:.5} dB, reference {r8:.5} dB; 1 vs 8 steps differ by {:.4} dB", (a1 - a8).abs());
    // the bend is real (more than 0.5 dB below the plain release)
    assert!(a1 < -8.5);
    assert!((a1 - r1).abs() < 1e-3, "one step must give the exact closed form");
    assert!((a8 - r8).abs() < 0.03);
    // the step size changes the result no more than it changes the continuous reference
    assert!((a1 - a8).abs() <= (r1 - r8).abs() + 0.03);
}

// ---------------------------------------------------------------------------------------------
// 2. Multiband

#[test]
fn band_focus_defaults_to_100_percent() {
    let m = SmartLimiter::default();
    let i = SmartLimiter::index_of("bandFocus").unwrap();
    println!("bandFocus default {} %", m.params()[i].default);
    assert_eq!(i, idx::BAND_FOCUS);
    assert_eq!(m.params()[i].default, 100.0);
    assert_eq!(m.param(idx::BAND_FOCUS), 100.0);
    assert_eq!((m.params()[i].min, m.params()[i].max), (0.0, 100.0));
    // out of the box Multiband now keeps the 5 kHz line off the 50 Hz bass
    let n = 48000 * 3;
    let x: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            lin(-0.2 + 6.0) * (2.0 * std::f64::consts::PI * 50.0 * t).sin() + lin(-18.0) * (2.0 * std::f64::consts::PI * 5000.0 * t).sin()
        })
        .collect();
    for xo in [0.0f32, 1.0] {
        let mut m = make(SR, &[engine(4), (idx::CROSSOVER_MODE, xo)]);
        let (y, _) = run(&mut m, &x, &x);
        let seg = &y[48000..96000];
        let c = dft_amp(seg, 5000.0, SR);
        let mut p = 0.0;
        for k in 1..=6 {
            for s in [-1.0, 1.0] {
                let a = dft_amp(seg, 5000.0 + s * 100.0 * k as f64, SR);
                p += a * a;
            }
        }
        let side = 10.0 * (p / (c * c)).log10();
        println!("Multiband {} at the default focus: 5 kHz carrier {:+.2} dB re input, 100 Hz sidebands {side:.1} dBc", if xo > 0.5 { "lin-phase" } else { "LR4" }, db(c) + 18.0);
        assert!(side < -60.0);
    }
}

#[test]
fn multiband_50hz_with_the_12ms_band_0_hold() {
    let mut worst = f64::NEG_INFINITY;
    for xo in [0.0f32, 1.0] {
        for (f, over) in [(50.0, 6.0), (50.0, 12.0), (40.0, 6.0), (60.0, 6.0)] {
            let mut m = make(SR, &[engine(4), (idx::CROSSOVER_MODE, xo)]);
            let x = sine(f, lin(-0.2 + over), 48000 * 4, SR, 0.0);
            let (y, _) = run(&mut m, &x, &x);
            let seg = &y[2 * 48000..3 * 48000];
            let t = thd_n(seg, f, SR);
            println!("Multiband {} {f} Hz +{over} dB: THD+N {t:.1} dB, peak {:.3} dBFS", if xo > 0.5 { "lin-phase" } else { "LR4" }, db(peak(seg)));
            if f == 50.0 {
                worst = worst.max(t);
            }
        }
    }
    println!("worst 50 Hz THD+N {worst:.1} dB");
    assert!(worst < -60.0);
}

// ---------------------------------------------------------------------------------------------
// 4. Stage-1 halfband

fn below_threshold_gain(e: usize, os: f32, sr: f64, f: f64) -> f64 {
    let mut m = make(sr, &[engine(e), (idx::OVERSAMPLING, os)]);
    let n = sr as usize;
    let x = sine(f, 0.3, n, sr, 0.0);
    let (y, _) = run(&mut m, &x, &x);
    db(dft_amp(&y[n / 4..3 * n / 4], f, sr) / 0.3)
}

#[test]
fn oversampled_passband_is_flat_past_21_khz() {
    // Pure and Precise carry the halfband cascade alone. 4x is stage 1 + stage 2; 8x and 16x
    // add the 7-tap maximally flat stages (about -0.02 dB per pass at 20 kHz, unchanged).
    // Punch is printed too: it adds the ADAA1 droop, left uncompensated by decision.
    let mut worst4 = 0.0f64;
    let mut worst = 0.0f64;
    for (sr, fs) in [(48000.0, [20000.0, 21000.0, 22000.0]), (44100.0, [18000.0, 19500.0, 20000.0])] {
        for e in [0usize, 2, 3] {
            for os in 0..3 {
                let g: Vec<f64> = fs.iter().map(|f| below_threshold_gain(e, os as f32, sr, *f)).collect();
                println!(
                    "{:8} {:>3}x at {sr} Hz, below threshold: {:.0} Hz {:+.4} dB, {:.0} Hz {:+.4} dB, {:.0} Hz {:+.4} dB",
                    ENGINE_NAMES[e],
                    [4, 8, 16][os],
                    fs[0],
                    g[0],
                    fs[1],
                    g[1],
                    fs[2],
                    g[2]
                );
                if e != 3 {
                    let d = g[1].abs().max(g[0].abs());
                    worst = worst.max(d);
                    if os == 0 {
                        worst4 = worst4.max(d);
                    }
                }
            }
        }
    }
    println!("Pure/Precise, worst deviation up to 21 kHz (48k) / 19.5 kHz (44.1k): 4x {worst4:.4} dB, any factor {worst:.4} dB");
    assert!(worst4 < 0.02);
    assert!(worst < 0.07);
}

#[test]
fn punch_23khz_alias() {
    let n = 48000 * 2;
    let mut worst = f64::NEG_INFINITY;
    for os in 0..3 {
        let mut m = make(SR, &[engine(3), (idx::DRIVE, 3.0), (idx::OVERSAMPLING, os as f32)]);
        let x = sine(5000.0, lin(-0.2), n, SR, 0.0);
        let (y, _) = run(&mut m, &x, &x);
        let seg = &y[24000..72000];
        let a1 = dft_amp(seg, 5000.0, SR);
        let a23 = db(dft_amp(seg, 23000.0, SR) / a1);
        println!("Punch 5 kHz +3 dB {:>3}x: 23 kHz alias (5th harmonic) {a23:.1} dBc", [4, 8, 16][os]);
        worst = worst.max(a23);
    }
    // The 47-tap stage passed 25 kHz at -11.1 dB (-51.0 dBc alias); the 79-tap stage passes
    // it at -15.0 dB. A halfband is -6 dB at 24 kHz by construction, so 1 kHz above it no
    // halfband stops much more.
    assert!(worst < -54.0);
}

// ---------------------------------------------------------------------------------------------
// 5. Ceiling glide

#[test]
fn ceiling_drop_glides_never_passes_the_gliding_ceiling_and_does_not_click() {
    let k = limiter_smart::dsp::pole(5.0, SR);
    let mut worst_over_glide = f64::NEG_INFINITY;
    let mut worst_over_target = f64::NEG_INFINITY;
    let mut worst_ratio = 0.0f64;
    let mut clamped_total = 0usize;
    for e in 0..6 {
        for (name, x) in [("sine 1 kHz", sine(1000.0, 0.9, 48000, SR, 0.0)), ("noise", noise(48000, 0.3, 17))] {
            let mut m = make(SR, &[engine(e), (idx::DRIVE, 6.0), (idx::CEILING, -1.0)]);
            let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
            let mut r = l.clone();
            // the ceiling target per processed sample (changes at block boundaries)
            let (t0, t1, t2) = (12000usize, 24000usize, 36000usize);
            let mut target = vec![0.0f64; l.len()];
            for (b, (cl, cr)) in l.chunks_mut(480).zip(r.chunks_mut(480)).enumerate() {
                let s = b * 480;
                let c = if s >= t2 {
                    -1.0
                } else if s >= t1 {
                    -12.0
                } else if s >= t0 {
                    -7.0
                } else {
                    -1.0
                };
                m.set_param(idx::CEILING, c as f32);
                for v in target[s..s + cl.len()].iter_mut() {
                    *v = lin(c);
                }
                m.process(cl, cr);
            }
            // the gliding ceiling travels with the audio: output sample i carries the value
            // that entered with it, one latency earlier
            let lat = m.latency();
            let mut glide = vec![0.0f64; l.len()];
            let mut g = target[0];
            for i in 0..l.len() {
                g = target[i] + k * (g - target[i]);
                glide[i] = g;
            }
            let go = |i: usize| glide[i.saturating_sub(lat)];
            let to = |i: usize| target[i.saturating_sub(lat)];
            let y: Vec<f64> = l.iter().zip(r.iter()).map(|(a, b)| (*a as f64).abs().max((*b as f64).abs())).collect();
            let yl: Vec<f64> = l.iter().map(|v| *v as f64).collect();
            let mut over_glide = f64::NEG_INFINITY;
            let mut over_target = f64::NEG_INFINITY;
            let mut clamped = 0;
            for i in 4800..y.len() {
                over_glide = over_glide.max(db(y[i]) - db(go(i)));
                over_target = over_target.max(db(y[i]) - db(to(i)));
                // a sample sitting exactly on the gliding ceiling while it moves: the clamp
                let moving = (go(i) / to(i) - 1.0).abs() > 1e-4;
                if moving && (l[i].abs() == go(i) as f32 || r[i].abs() == go(i) as f32) {
                    clamped += 1;
                }
            }
            // click: largest second difference from the change until 30 ms after it reaches
            // the output, against the steady state before the change and after the glide
            let d2 = |i: usize| (yl[i] - 2.0 * yl[i - 1] + yl[i - 2]).abs();
            let mut ratio = 0.0f64;
            for &c in [t0, t1, t2].iter() {
                let steady = (c - 2400..c).map(d2).fold(0.0, f64::max).max((c + lat + 4800..c + lat + 7200).map(d2).fold(0.0, f64::max));
                let w = (c..c + lat + 1440).map(d2).fold(0.0, f64::max);
                ratio = ratio.max(w / steady);
            }
            println!(
                "{:9} {name:10}: ceiling -1 -> -7 -> -12 -> -1 dB: over the gliding ceiling {over_glide:+.4} dB, over the new target while gliding {over_target:+.3} dB, clamped samples {clamped}, transient second-difference ratio {ratio:.2}",
                ENGINE_NAMES[e]
            );
            worst_over_glide = worst_over_glide.max(over_glide);
            worst_over_target = worst_over_target.max(over_target);
            worst_ratio = worst_ratio.max(ratio);
            clamped_total += clamped;
        }
    }
    println!("worst: over the gliding ceiling {worst_over_glide:+.5} dB, over the target {worst_over_target:+.3} dB, clamped samples {clamped_total}, second-difference ratio {worst_ratio:.2}");
    assert!(worst_over_glide <= 1e-5, "the output must stay under the gliding ceiling");
    assert_eq!(clamped_total, 0, "the trim keeps the glide; the clamp is only a safety net");
    assert!(worst_ratio < 1.3, "no click on a ceiling change");
}

// ---------------------------------------------------------------------------------------------
// 6. Punch delta

#[test]
fn punch_delta_nulls_exactly_below_threshold() {
    let mut worst = 0.0f64;
    for os in 0..3 {
        for ms in [0.0f32, 1.0] {
            let mut m = make(SR, &[engine(3), (idx::DELTA, 1.0), (idx::OVERSAMPLING, os as f32), (idx::CHANNEL_MODE, ms)]);
            let x = noise(48000, 0.08, 31);
            let r = noise(48000, 0.08, 32);
            let (yl, yr) = run(&mut m, &x, &r);
            let w = peak(&yl[4000..]).max(peak(&yr[4000..]));
            println!("Punch {:>3}x {} delta below threshold: largest output sample {w:e}", [4, 8, 16][os], if ms > 0.5 { "M/S" } else { "L/R" });
            worst = worst.max(w);
        }
    }
    // above threshold the delta carries what was removed
    let mut m = make(SR, &[engine(3), (idx::DELTA, 1.0), (idx::DRIVE, 12.0)]);
    let x = noise(48000, 0.08, 31);
    let (yl, _) = run(&mut m, &x, &x);
    println!("  with 12 dB drive the delta peaks at {:.2} dBFS", db(peak(&yl[4000..])));
    assert_eq!(worst, 0.0, "Punch delta must null to exact zero");
    assert!(peak(&yl[4000..]) > 0.01);
}
