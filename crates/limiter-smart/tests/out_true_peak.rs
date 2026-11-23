//! The output true-peak read-out (`view::OUT_TRUE_PEAK`): it reads the final output, after
//! the trim, as a reconstruction meter does, and it changes nothing in the audio.

mod common;
use common::*;
use limiter_smart::{idx, view, SmartLimiter};
use olm_api::Module;

fn read_out(m: &mut SmartLimiter, x: &[f64]) -> (f32, Vec<f64>) {
    // the last block's read-out after a settled run, deepest over the last 20 blocks
    let lat = m.latency();
    let mut a: Vec<f32> = x.iter().map(|v| *v as f32).collect();
    a.extend(std::iter::repeat(0.0).take(lat));
    let mut b = a.clone();
    let mut tp = -200.0f32;
    let blocks = a.len().div_ceil(512);
    for (k, (ca, cb)) in a.chunks_mut(512).zip(b.chunks_mut(512)).enumerate() {
        m.process(ca, cb);
        let mut v = [0f32; view::LEN];
        m.view(&mut v);
        if k + 30 >= blocks && k + 10 < blocks {
            tp = tp.max(v[view::OUT_TRUE_PEAK]);
        }
    }
    (tp, a[lat..lat + x.len()].iter().map(|v| *v as f64).collect())
}

#[test]
fn output_true_peak_reads_after_the_trim() {
    // A quarter-rate tone at 45 degrees: its samples sit 3.01 dB under its true peak.
    let n = 48_000;
    let x = sine(12_000.0, 0.9, n, SR, std::f64::consts::FRAC_PI_4);
    let mid = n / 2..n - 4096;
    // true peak off: the samples are held at the ceiling, the true peak is 3 dB above it
    let mut m = make(SR, &[engine(1), (idx::DRIVE, 6.0), (idx::CEILING, -3.0), (idx::TRUE_PEAK, 0.0)]);
    let (tp, y) = read_out(&mut m, &x);
    let sample = db(peak(&y[mid.clone()]));
    let reference = db(tp_sinc16(&y[mid.clone()]));
    println!("TP off: sample {sample:.3} dBFS, read-out {tp:.3} dBTP, 16x reference {reference:.3} dBTP");
    assert!((sample + 3.0).abs() < 0.05, "{sample}");
    assert!((tp as f64 - reference).abs() < 0.1, "read-out {tp} vs reference {reference}");
    assert!(tp as f64 > sample + 2.5);
    // true peak on: the trim holds the true peak, and the read-out says so
    let mut m = make(SR, &[engine(1), (idx::DRIVE, 6.0), (idx::CEILING, -3.0)]);
    let (tp, y) = read_out(&mut m, &x);
    let reference = db(tp_sinc16(&y[mid.clone()]));
    println!("TP on: read-out {tp:.3} dBTP, 16x reference {reference:.3} dBTP");
    assert!((tp as f64 - reference).abs() < 0.1, "read-out {tp} vs reference {reference}");
    assert!(tp < -2.9);
}

#[test]
fn the_read_out_does_not_touch_the_audio() {
    // the same run twice, one reading the view after every block: identical output
    let x = noise(24_000, 0.4, 7);
    for e in 0..6 {
        let mut a = make(SR, &[engine(e), (idx::DRIVE, 9.0)]);
        let mut b = make(SR, &[engine(e), (idx::DRIVE, 9.0)]);
        let (ya, _) = run(&mut a, &x, &x);
        let (_, yb) = read_out(&mut b, &x);
        assert_eq!(ya, yb, "engine {e}");
    }
}
