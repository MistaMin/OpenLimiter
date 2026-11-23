//! Spec 7.2: one constant `latency()` across engines, lookahead, true peak, oversampling,
//! offline, crossover and channel mode; impulse position; click-free 20 ms switch.

mod common;
use common::*;
use limiter_smart::{idx, module_latency, SmartLimiter};
use olm_api::Module;

fn impulse_peak(m: &mut SmartLimiter, amp: f64) -> (usize, f64, usize) {
    let n = 4096;
    let mut x = vec![0.0; n];
    x[100] = amp;
    let (y, _) = run(m, &x, &x);
    let (mut i, mut v) = (0, 0.0);
    for (k, s) in y.iter().enumerate() {
        if s.abs() > v {
            v = s.abs();
            i = k;
        }
    }
    let first = y.iter().position(|s| s.abs() > 1e-6 * amp).unwrap_or(n);
    (i, v, first)
}

#[test]
fn latency_is_constant_and_impulses_line_up() {
    for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
        let lat = module_latency(sr as f32);
        println!("latency at {sr} Hz: {lat} samples = {:.3} ms", lat as f64 * 1000.0 / sr);
    }
    let base = make(SR, &[]).latency();
    let mut checked = 0;
    let mut worst_peak_err = 0i64;
    for e in 0..6 {
        for os in 0..3 {
            for tp in [0.0f32, 1.0] {
                for la in [0.5f32, 1.5, 15.0] {
                    for (xo, ms) in [(0.0f32, 0.0f32), (1.0, 0.0), (0.0, 1.0)] {
                        for offline in [false, true] {
                            let mut m = make(SR, &[
                                engine(e),
                                (idx::OVERSAMPLING, os as f32),
                                (idx::TRUE_PEAK, tp),
                                (idx::LOOKAHEAD, la),
                                (idx::CROSSOVER_MODE, xo),
                                (idx::CHANNEL_MODE, ms),
                            ]);
                            m.set_offline(offline);
                            m.reset();
                            assert_eq!(m.latency(), base);
                            // below the ceiling: the output is the input, delayed by exactly latency()
                            let (i, v, first) = impulse_peak(&mut m, 0.25);
                            let lr4 = e == 4 && xo < 0.5;
                            if lr4 {
                                // the LR4 tree is all-pass: nothing comes out early
                                assert!(first >= 100, "LR4 output before the latency: {first}");
                            } else {
                                worst_peak_err = worst_peak_err.max((i as i64 - 100).abs());
                                assert_eq!(i, 100, "engine {e} os {os} tp {tp} la {la} xo {xo} ms {ms} offline {offline}: peak at {i}");
                                // a band-limited impulse through the halfbands keeps 96 % of its height
                                assert!(v > 0.9 * 0.25 && v < 1.0001 * 0.25, "impulse level {v}");
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    println!("{checked} settings: latency() = {base} everywhere; impulse peak offset 0 in all non-LR4 settings (worst {worst_peak_err}); LR4 never early");
    // the same position above the ceiling (the impulse is limited, not moved)
    for e in 0..6 {
        let mut m = make(SR, &[engine(e), (idx::CEILING, -6.0)]);
        let (i, v, _) = impulse_peak(&mut m, 1.0);
        println!("{:9} 0 dBFS impulse, ceiling -6 dB: peak at offset {} level {:.2} dB", ENGINE_NAMES[e], i as i64 - 100, db(v));
        if e != 4 {
            assert!((i as i64 - 100).abs() <= 1);
        }
    }
}

fn sweep_run(e: usize, sweep: bool) -> (f64, f64, usize) {
    let mut m = make(SR, &[engine(e), (idx::DRIVE, 6.0)]);
    let lat = m.latency();
    let n = 48000;
    let x = sine(997.0, 0.9, n, SR, 0.0);
    let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
    let mut r = l.clone();
    let mut worst_jump = 0.0f64;
    let mut prev = 0.0f64;
    let mut over = 0.0f64;
    for (b, (cl, cr)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
        if sweep {
            // 0.5 -> 15 -> 0.5 ms over the second, log scale
            let t = b as f64 / (n / 256) as f64;
            let la = 0.5 * (30f64).powf(1.0 - (2.0 * t - 1.0).abs());
            m.set_param(idx::LOOKAHEAD, la as f32);
        }
        m.process(cl, cr);
        assert_eq!(m.latency(), lat);
        for v in cl.iter() {
            let v = *v as f64;
            if b > 8 {
                worst_jump = worst_jump.max((v - prev).abs());
                over = over.max(v.abs());
            }
            prev = v;
        }
    }
    (worst_jump, over, lat)
}

#[test]
fn lookahead_sweep_glides_without_changing_latency() {
    for e in 0..6 {
        let (jump, over, lat) = sweep_run(e, true);
        let (jump0, over0, _) = sweep_run(e, false);
        println!(
            "{:9} lookahead sweep 0.5-15-0.5 ms: latency {} constant, largest step {:.4} (fixed 1.5 ms: {:.4}), peak {:.3} dBFS (fixed {:.3})",
            ENGINE_NAMES[e],
            lat,
            jump,
            jump0,
            db(over),
            db(over0)
        );
        assert!(jump < 1.05 * jump0, "sweep step {jump} vs {jump0}");
        assert!(db(over) <= -0.2 + 0.01);
    }
}

#[test]
fn engine_switch_is_a_click_free_20ms_crossfade() {
    // Switch through every engine (and the OS factor, crossover and channel mode) on a
    // limited 220 Hz sine, and compare the largest second difference during the switches
    // with the steady state. A click or a gap would show up as a jump.
    let n = 48000 * 6;
    let x = sine(220.0, 0.9, n, SR, 0.0);
    let mut m = make(SR, &[(idx::DRIVE, 6.0)]);
    let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
    let mut r = l.clone();
    let lat = m.latency();
    let mut changes = Vec::new();
    for (b, (cl, cr)) in l.chunks_mut(480).zip(r.chunks_mut(480)).enumerate() {
        let t = b * 480;
        if t > 0 && t % 24000 == 0 {
            let k = t / 24000;
            match k {
                1..=5 => m.set_param(idx::ENGINE, k as f32),
                6 => m.set_param(idx::CROSSOVER_MODE, 1.0),
                7 => m.set_param(idx::ENGINE, 0.0),
                8 => m.set_param(idx::OVERSAMPLING, 2.0),
                9 => m.set_param(idx::CHANNEL_MODE, 1.0),
                10 => m.set_offline(true),
                _ => {}
            }
            changes.push(t);
        }
        m.process(cl, cr);
        assert_eq!(m.latency(), lat);
    }
    let y: Vec<f64> = l.iter().map(|v| *v as f64).collect();
    let d2 = |i: usize| (y[i] - 2.0 * y[i - 1] + y[i - 2]).abs();
    // steady state of each setting: the last 4000 samples before the next change
    let mut bounds = vec![0usize];
    bounds.extend(changes.iter().cloned());
    bounds.push(n);
    let steady: Vec<f64> = bounds.windows(2).map(|w| (w[1] - 4000..w[1]).map(d2).fold(0.0, f64::max)).collect();
    let mut worst_ratio = 0.0f64;
    for (k, &c) in changes.iter().enumerate() {
        let allowed = steady[k].max(steady[k + 1]);
        let w = (c..(c + lat + 960 + 400).min(n)).map(d2).fold(0.0, f64::max);
        println!("  switch {} at {:.2} s: largest second difference {:.5}, steady before/after {:.5}/{:.5}", k + 1, c as f64 / SR, w, steady[k], steady[k + 1]);
        worst_ratio = worst_ratio.max(w / allowed);
    }
    let low = (24000..n).map(|i| y[i].abs()).fold(0.0, f64::max);
    println!("{} structural switches: worst ratio to the steady state on either side {:.3}; output peak {:.3} dBFS", changes.len(), worst_ratio, db(low));
    assert!(worst_ratio < 1.2, "switch click: ratio {worst_ratio}");
}
