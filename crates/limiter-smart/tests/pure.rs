//! Spec 7.4 (Pure): THD+N of 40/50/100 Hz at +6/+12 dB; Join Bend step invariance; Pause Hold
//! freezes the slow gain in a -30 LUFS tail; Peak Body against Loudness Crest.

mod common;
use common::*;
use limiter_smart::lane::{join_bend, Lane};
use limiter_smart::{body_split_db, idx, loudness_crest_gain_db, view};
use olm_api::Module;

const C_DB: f64 = -0.2;

#[test]
fn low_tones_thd_n() {
    let mut worst = f64::NEG_INFINITY;
    for os in [0.0f32, 2.0] {
        for f in [40.0, 50.0, 100.0] {
            for drive in [6.0f32, 12.0] {
                let mut m = make(SR, &[engine(0), (idx::DRIVE, drive), (idx::OVERSAMPLING, os)]);
                // 10 s; analyse 8..9 s: the slow engine has settled, and the end of the signal
                // (where the filters and the lookahead see the zeros after it) stays out
                let n = 48000 * 10;
                let x = sine(f, lin(C_DB), n, SR, 0.0);
                let (y, _) = run(&mut m, &x, &x);
                let tail = &y[8 * 48000..9 * 48000];
                let t = thd_n(tail, f, SR);
                let mut v = [0f32; view::LEN];
                m.view(&mut v);
                println!(
                    "Pure {}x {f:5} Hz +{drive:4} dB: THD+N {t:7.1} dB, output peak {:.4} dBFS, gain {:.2} dB (slow {:.2}, post-hold {:.1} ms)",
                    [4, 8, 16][os as usize],
                    db(peak(tail)),
                    v[view::TOTAL_GAIN],
                    v[view::SLOW_GAIN],
                    v[view::POST_HOLD_MS]
                );
                worst = worst.max(t);
            }
        }
    }
    println!("worst THD+N {worst:.1} dB (spec: < -125 dB)");
    // for context: the same 40 Hz tone without the bass-adaptive post-hold
    let mut m = make(SR, &[engine(0), (idx::DRIVE, 12.0), (idx::BASS_HOLD_MAX, 0.0)]);
    let n = 48000 * 10;
    let x = sine(40.0, lin(C_DB), n, SR, 0.0);
    let (y, _) = run(&mut m, &x, &x);
    println!("(context, not a target) 40 Hz +12 dB with bassHoldMax = 0: THD+N {:.1} dB", thd_n(&y[8 * 48000..9 * 48000], 40.0, SR));
    assert!(worst < -125.0);
}

#[test]
fn join_bend_is_step_size_invariant() {
    // 1) the closed form is additive: one step m0 -> m2 equals m0 -> m1 -> m2 at fixed h.
    let mut worst_add = 0.0f64;
    for (h, m0, m2) in [(-10.0, -2.0, -9.0), (-6.0, -1.0, -5.5), (-3.0, 0.0, -2.9), (-12.0, -1.0, -11.0)] {
        for w in [1.0, 6.0, 12.0] {
            let one = join_bend(h, m0, m2, w);
            let mut many = 0.0;
            let k = 64;
            for i in 0..k {
                let a = m0 + (m2 - m0) * i as f64 / k as f64;
                let b = m0 + (m2 - m0) * (i + 1) as f64 / k as f64;
                many += join_bend(h, a, b, w);
            }
            worst_add = worst_add.max((one - many).abs());
        }
    }
    println!("closed-form Join Bend: one step vs 64 sub-steps at fixed h, largest difference {worst_add:.2e} dB");
    assert!(worst_add < 1e-12);

    // 2) dual burst through the lane: the second burst's requirement arrives as one step or
    //    spread over 8 samples (the way an oversampled peak can arrive). With no release, the
    //    bend alone moves the held value; compare with the discrete form of the original
    //    (closeness^2 x fall, closeness at the new level).
    let run = |steps: usize, exact: bool| -> f64 {
        let mut lane = Lane::default();
        lane.alloc(100, 0, 1, 0);
        lane.set_target(2, 0);
        lane.snap();
        let mut seq = vec![0.0; 40];
        seq.extend(vec![-10.0; 4]); // first burst
        // window is short (lookahead 2): the hold releases at once, then the second burst
        // comes down from -2 to -8 while held is still at -10 (c_rel = 1: no release)
        seq.extend(vec![-2.0; 20]);
        for i in 0..steps {
            seq.push(-2.0 - 6.0 * (i + 1) as f64 / steps as f64);
        }
        seq.extend(vec![-8.0; 20]);
        let mut held = 0.0;
        let mut m_prev = 0.0;
        let mut h_disc = 0.0f64;
        for r in seq {
            if exact {
                held = lane.hold_release(r, 1.0, 6.0);
            } else {
                // discrete original form on the same hold
                let m = r;
                if m <= h_disc {
                    h_disc = m;
                } else {
                    let fall = (m - m_prev).min(0.0);
                    let close = (1.0 - (m - h_disc) / 6.0).max(0.0);
                    h_disc = m.min(h_disc + close * close * fall);
                }
                m_prev = m;
                held = h_disc;
            }
        }
        held
    };
    let (a, b) = (run(1, true), run(8, true));
    let (c, d) = (run(1, false), run(8, false));
    println!("dual burst, second burst in 1 vs 8 steps: exact integral held {a:.4} / {b:.4} dB (difference {:.4}); discrete original {c:.4} / {d:.4} dB (difference {:.4})", (a - b).abs(), (c - d).abs());
    assert!((a - b).abs() < 0.05 * (a + 8.0).abs().max(0.05));
    assert!((a - b).abs() < (c - d).abs());
}

fn slow_trace(pause_hold: bool) -> Vec<f32> {
    let mut m = make(SR, &[engine(0), (idx::DRIVE, 12.0), (idx::PAUSE_HOLD, if pause_hold { 1.0 } else { 0.0 })]);
    // 6 s of dense programme (about +2 LUFS at the engine after 12 dB drive), then a tail
    // 34.3 dB lower (-30 LUFS)
    let n1 = 48000 * 6;
    let n2 = 48000 * 2;
    let a = noise(n1, 0.25, 5);
    let tail_gain = lin(-34.3);
    let b: Vec<f64> = noise(n2, 0.25, 6).iter().map(|v| v * tail_gain).collect();
    let x: Vec<f64> = a.into_iter().chain(b).collect();
    let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
    let mut r = l.clone();
    let mut out = Vec::new();
    let mut v = [0f32; view::LEN];
    for (cl, cr) in l.chunks_mut(480).zip(r.chunks_mut(480)) {
        m.process(cl, cr);
        m.view(&mut v);
        out.push(v[view::SLOW_GAIN]);
        out.push(v[view::SLOW_LOUDNESS]);
        out.push(v[view::PAUSE_HOLD]);
    }
    out
}

#[test]
fn pause_hold_freezes_the_slow_gain_in_a_quiet_tail() {
    let on = slow_trace(true);
    let off = slow_trace(false);
    let at = |t: &Vec<f32>, s: f64| t[3 * (s * 100.0) as usize];
    let hold = |t: &Vec<f32>, s: f64| t[3 * (s * 100.0) as usize + 2];
    let loud = on[3 * 590 + 1];
    let (a0, a1) = (at(&on, 6.1), at(&on, 7.9));
    let (b0, b1) = (at(&off, 6.1), at(&off, 7.9));
    println!(
        "programme {:.1} LUFS (tail 34.3 dB lower: {:.1} LUFS); slow gain before the tail {:.3} dB; tail 6.1 -> 7.9 s: pause hold on {:.3} -> {:.3} dB (held {}), off {:.3} -> {:.3} dB",
        loud,
        loud - 34.3,
        at(&on, 5.9),
        a0,
        a1,
        hold(&on, 7.0),
        b0,
        b1
    );
    assert!(at(&on, 5.9) < -1.0, "the programme must drive the slow engine");
    assert!((a1 - a0).abs() < 0.02, "pause hold must freeze the slow gain");
    assert!(b1 - b0 > 1.0, "without pause hold the slow gain recovers");
}

#[test]
fn peak_body_against_loudness_crest() {
    // static: a steady 1 kHz sine 6 dB over the ceiling
    let (s, f) = body_split_db(6.0, 0.5);
    println!("static Peak Body split at +6 dB, body 50 %: slow {s:.2} dB, fast {f:.2} dB");
    let measure = |mode: f32, body: f32, x: &[f64]| -> (f32, f32, f32) {
        let mut m = make(SR, &[engine(0), (idx::SLOW_MODE, mode), (idx::BODY_SHARE, body)]);
        let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
        let mut r = l.clone();
        let mut v = [0f32; view::LEN];
        for (cl, cr) in l.chunks_mut(480).zip(r.chunks_mut(480)) {
            m.process(cl, cr);
            m.view(&mut v);
        }
        (v[view::SLOW_GAIN], v[view::TOTAL_GAIN], v[view::SLOW_LOUDNESS])
    };
    let n = 48000 * 8;
    let tone = sine(1000.0, lin(C_DB + 6.0), n, SR, 0.0);
    let (pb, pt, _) = measure(0.0, 50.0, &tone);
    let (lc, lt, ll) = measure(1.0, 50.0, &tone);
    let expect_lc = loudness_crest_gain_db(ll as f64, C_DB, 10.0);
    println!("sine +6 dB: Peak Body slow {pb:.3} dB (total {pt:.3}); Loudness Crest slow {lc:.3} dB (total {lt:.3}, loudness {ll:.2} LUFS, static curve {expect_lc:.3})");
    assert!((pb as f64 + 3.0).abs() < 0.05, "Peak Body takes exactly bodyShare of a steady excess");
    assert!((pt as f64 + 6.0).abs() < 0.05);
    assert!((lc as f64 - expect_lc).abs() < 0.2);

    // isolated clicks on a quiet bed: Peak Body ignores them, Loudness Crest follows loudness
    let mut clicks = noise(n, 0.01, 3);
    for i in (0..n).step_by(24000) {
        for k in 0..24 {
            clicks[i + k] += if k % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    let (pbc, _, _) = measure(0.0, 50.0, &clicks);
    let (lcc, _, lcl) = measure(1.0, 50.0, &clicks);
    // dense programme at the same peak level
    let dense: Vec<f64> = noise(n, 1.0, 4);
    let (pbd, _, _) = measure(0.0, 50.0, &dense);
    let (lcd, _, lcdl) = measure(1.0, 50.0, &dense);
    println!("click train (1 per 0.5 s on a -40 dBFS bed): Peak Body slow {pbc:.3} dB, Loudness Crest slow {lcc:.3} dB (loudness {lcl:.1})");
    println!("dense noise (0.82 RMS, peaks up to +10 dBFS): Peak Body slow {pbd:.3} dB, Loudness Crest slow {lcd:.3} dB (loudness {lcdl:.1})");
    assert!(pbc > -0.6, "Peak Body ignores isolated clicks");
    assert!(pbd < -1.0);
    assert!(lcd < pbd, "on dense programme the crest leveller works harder");
}
