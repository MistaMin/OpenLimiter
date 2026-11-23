//! CPU per engine and oversampling factor at 48 kHz (one core, release build). Printed, with
//! a loose bound only (the numbers depend on the machine).

mod common;
use common::*;
use limiter_smart::{idx, SmartLimiter};
use olm_api::Module;
use std::time::Instant;

fn measure(sets: &[(usize, f32)], secs: usize) -> f64 {
    let mut m = SmartLimiter::default();
    for (i, v) in sets {
        m.set_param(*i, *v);
    }
    m.prepare(48000.0, 512);
    let n = 48000 * secs;
    let x = noise(n, 0.3, 91);
    let y = noise(n, 0.3, 92);
    let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
    let mut r: Vec<f32> = y.iter().map(|v| *v as f32).collect();
    // warm up
    for (cl, cr) in l[..48000].chunks_mut(512).zip(r[..48000].chunks_mut(512)) {
        m.process(cl, cr);
    }
    let t = Instant::now();
    for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(cl, cr);
    }
    100.0 * t.elapsed().as_secs_f64() / secs as f64
}

#[test]
fn cpu_per_engine_and_factor() {
    let secs = 6;
    let mut worst = 0.0f64;
    for e in 0..6 {
        let factors: &[usize] = if matches!(e, 0 | 2 | 3) { &[0, 1, 2] } else { &[0] };
        for &os in factors {
            let base = [engine(e), (idx::DRIVE, 9.0), (idx::OVERSAMPLING, os as f32)];
            let a = measure(&base, secs);
            let mut lin = base.to_vec();
            lin.push((idx::CROSSOVER_MODE, 1.0));
            let mut ms = base.to_vec();
            ms.push((idx::CHANNEL_MODE, 1.0));
            let b = measure(&ms, secs);
            let extra = if e == 4 { format!(", linear-phase crossover {:.2} %", measure(&lin, secs)) } else { String::new() };
            println!(
                "{:9} {:>3}: {:.2} % of one core at 48 kHz stereo (Mid-Side {:.2} %{extra})",
                ENGINE_NAMES[e],
                if matches!(e, 0 | 2 | 3) { format!("{}x", [4, 8, 16][os]) } else { "1x".into() },
                a,
                b
            );
            worst = worst.max(a);
        }
    }
    // delta on (reference decimators run too), and Pure at 16x
    let d = measure(&[engine(0), (idx::DRIVE, 9.0), (idx::DELTA, 1.0), (idx::OVERSAMPLING, 2.0)], secs);
    println!("Pure 16x with delta: {d:.2} %");
    assert!(worst < 50.0);
}
