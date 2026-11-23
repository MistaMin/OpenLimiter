//! Spec 7.3: ceiling and true-peak sweep (BS.1770 4x and 16x sinc reconstruction) across
//! tones, the 12 kHz quarter-rate inter-sample peak, a 21 + 22.5 kHz beat, a square wave and
//! gated noise; all six engines; Stereo and Mid-Side with mono, hard-panned and anti-phase
//! (side-only) input.

mod common;
use common::*;
use limiter_smart::idx;
use std::f64::consts::PI;

const C_DB: f64 = -1.0;
/// Sample peaks may exceed the ceiling by f32 rounding only.
const SAMPLE_TOL_DB: f64 = 0.001;
/// True-peak tolerance for content up to 20 kHz (16x reference).
const TP_TOL_DB: f64 = 0.1;
/// Content at the band edge (21 + 22.5 kHz beat, square wave, white noise): the 48-tap
/// reader is -0.55 dB at 22.5 kHz and blind above, so the 1x engines can pass a little of
/// what lies beyond. This is the measured limit (+0.27 dB here, +0.43 dB on heavier noise
/// in params.rs), not a target.
const TP_EDGE_LIMIT_DB: f64 = 0.45;

fn signals(n: usize) -> Vec<(&'static str, Vec<f64>, bool)> {
    let mut v = Vec::new();
    for f in [100.0, 1000.0, 5000.0, 10000.0, 15000.0, 19000.0] {
        let name: &'static str = Box::leak(format!("sine {f} Hz").into_boxed_str());
        v.push((name, sine(f, 0.5, n, SR, 0.3), false));
    }
    v.push(("12 kHz quarter-rate ISP", sine(12000.0, 0.5, n, SR, PI / 4.0), false));
    let beat: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            0.25 * (2.0 * PI * 21000.0 * t).sin() + 0.25 * (2.0 * PI * 22500.0 * t + 0.4).sin()
        })
        .collect();
    v.push(("21 + 22.5 kHz beat", beat, true));
    let sq: Vec<f64> = (0..n).map(|i| if (i / 24) % 2 == 0 { 0.5 } else { -0.5 }).collect();
    v.push(("square 1 kHz", sq, true));
    let nz = noise(n, 0.2, 99);
    let gated: Vec<f64> = nz.iter().enumerate().map(|(i, s)| if (i / 2400) % 3 == 0 { *s } else { 0.0 }).collect();
    v.push(("gated noise", gated, true));
    v
}

#[test]
fn ceiling_and_true_peak_sweep() {
    let n = 24000;
    let skip = 9600;
    let sigs = signals(n);
    let mut worst_sample = f64::NEG_INFINITY;
    let mut worst_tp_band = f64::NEG_INFINITY;
    let mut worst_tp_edge = f64::NEG_INFINITY;
    let mut worst_bs = f64::NEG_INFINITY;
    let mut rows = 0;
    for e in 0..6 {
        let mut line_band = f64::NEG_INFINITY;
        let mut line_edge = f64::NEG_INFINITY;
        for ms in [0.0f32, 1.0] {
            for (pan, pname) in [(0, "mono"), (1, "hard-left"), (2, "side-only")] {
                for (sname, x, edge) in sigs.iter() {
                    let mut m = make(SR, &[engine(e), (idx::DRIVE, 18.0), (idx::CEILING, C_DB as f32), (idx::CHANNEL_MODE, ms)]);
                    let (l, r): (Vec<f64>, Vec<f64>) = match pan {
                        0 => (x.clone(), x.clone()),
                        1 => (x.clone(), vec![0.0; n]),
                        _ => (x.clone(), x.iter().map(|v| -v).collect()),
                    };
                    let (yl, yr) = run(&mut m, &l, &r);
                    let (yl, yr) = (&yl[skip..], &yr[skip..]);
                    let sp = db(peak(yl).max(peak(yr))) - C_DB;
                    let (bs, s16) = true_peak_db(yl, yr);
                    let (bs, s16) = (bs - C_DB, s16 - C_DB);
                    worst_sample = worst_sample.max(sp);
                    worst_bs = worst_bs.max(bs);
                    if *edge {
                        worst_tp_edge = worst_tp_edge.max(s16);
                        line_edge = line_edge.max(s16);
                    } else {
                        worst_tp_band = worst_tp_band.max(s16);
                        line_band = line_band.max(s16);
                    }
                    if s16 > TP_TOL_DB || sp > SAMPLE_TOL_DB {
                        println!(
                            "  over: {:9} {} {:9} {:24} sample {:+.3} BS.1770 {:+.3} 16x {:+.3} dB re C",
                            ENGINE_NAMES[e],
                            if ms > 0.5 { "M/S" } else { "L/R" },
                            pname,
                            sname,
                            sp,
                            bs,
                            s16
                        );
                    }
                    rows += 1;
                }
            }
        }
        println!(
            "{:9} worst 16x true peak re ceiling: tones/ISP up to 19 kHz {:+.3} dB; beat/square/noise {:+.3} dB",
            ENGINE_NAMES[e],
            line_band,
            line_edge
        );
    }
    println!(
        "{rows} runs (18 dB drive, ceiling {C_DB} dB): worst sample peak {:+.4} dB, worst BS.1770 {:+.3} dB, worst 16x: band-limited {:+.3} dB, band-edge content {:+.3} dB",
        worst_sample,
        worst_bs,
        worst_tp_band,
        worst_tp_edge
    );
    assert!(worst_sample <= SAMPLE_TOL_DB);
    assert!(worst_tp_band <= TP_TOL_DB);
    assert!(worst_tp_edge <= TP_EDGE_LIMIT_DB);
}
