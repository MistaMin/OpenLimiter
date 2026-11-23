//! Tests beyond the acceptance table: latency, metering, safety under random settings,
//! determinism, the sample-rate check of SPEC_LIMITERS.md 26, the peak-guard release figures of
//! SPEC_LIMITERS.md 54.1 (reported), and the processing cost.

mod common;

use common::*;
use limiter_clear::*;
use olm_api::Module;
use std::f64::consts::PI;

const SR: usize = 48_000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn signed(&mut self) -> f64 {
        2.0 * self.next() - 1.0
    }
}

/// Loud, uneven material: gated noise plus a low and a high tone.
fn programme(n: usize, rng: &mut Rng) -> (Vec<f64>, Vec<f64>) {
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / SR as f64;
        let gate = if (2.0 * PI * 3.0 * t).sin() > 0.3 { 1.0 } else { 0.1 };
        let common = 0.5 * (2.0 * PI * 55.0 * t).sin() + 0.2 * (2.0 * PI * 9000.0 * t).sin();
        l.push(gate * (0.6 * rng.signed() + common));
        r.push(gate * (0.6 * rng.signed() + 0.8 * common));
    }
    (l, r)
}

#[test]
fn latency_is_148_samples_at_48k_and_the_path_is_exact_below_threshold() {
    // HANDOFF 3.1: 148 samples at 48 kHz. HANDOFF 3.2: exactly unity below the threshold
    // (peak guard off, lift 0).
    let mut m = module(&[(P_PEAK_GUARD, 0.0), (P_SLOW_HOLD, 0.0)]);
    m.prepare(48_000.0, 256);
    assert_eq!(m.latency(), 148);
    let mut rng = Rng(7);
    let x: Vec<f32> = (0..20_000).map(|_| (0.9 * rng.signed()) as f32).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    l.resize(x.len() + 148, 0.0);
    r.resize(x.len() + 148, 0.0);
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        m.process(a, b);
    }
    assert!(l[..148].iter().all(|v| *v == 0.0));
    for i in 0..x.len() {
        assert_eq!(l[i + 148], x[i], "sample {i}");
        assert_eq!(r[i + 148], x[i], "sample {i}");
    }
    let mut m96 = ClearLimiter::default();
    m96.prepare(96_000.0, 256);
    assert_eq!(m96.latency(), 296);
    assert_eq!(m.params().len(), 10);
}

#[test]
fn meter_shows_true_reduction_rises_at_once_and_falls_20_db_per_second() {
    // SPEC_LIMITERS.md 62, 62.1: threshold -12 dB; tones at -12, -9, -6, -3, 0 dBFS read 0, 3, 6, 9, 12 dB.
    let mut m = module(&[(P_THRESHOLD, -12.0), (P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0)]);
    m.prepare(48_000.0, 480);
    let mut phase = 0usize;
    let mut run = |m: &mut ClearLimiter, level_db: f64, blocks: usize| {
        for _ in 0..blocks {
            let mut l: Vec<f32> = (0..480).map(|i| (lin(level_db) * (2.0 * PI * 1000.0 * (phase + i) as f64 / SR as f64).sin()) as f32).collect();
            let mut r = l.clone();
            m.process(&mut l, &mut r);
            phase += 480;
        }
    };
    for (level, want) in [(-12.0, 0.0), (-9.0, 3.0), (-6.0, 6.0), (-3.0, 9.0), (0.0, 12.0)] {
        run(&mut m, level, 100);
        let got = m.meters().reduction_db;
        println!("INFO | meter | tone {level} dBFS | mine {got:.3} | stored {want:.3}");
        assert!((got as f64 - want).abs() < 0.05, "{level}: {got}");
    }
    // Fall: silence for 0.3 s must leave 12 - 6 = 6 dB (release 7.34 ms is long gone).
    run(&mut m, -200.0, 30);
    let got = m.meters().reduction_db;
    println!("INFO | meter | 0.3 s after the tone stops | mine {got:.3} | stored 6.000");
    assert!((got - 6.0).abs() < 0.5, "{got}");
    run(&mut m, -200.0, 40);
    assert_eq!(m.meters().reduction_db, 0.0);
    // Rise: one block of a loud tone is enough.
    run(&mut m, 0.0, 1);
    assert!(m.meters().reduction_db > 11.5);
}

#[test]
fn static_gain_is_the_same_at_48_and_96_khz() {
    // SPEC_LIMITERS.md 26 (data: samplerate_96k.json): threshold -9.6 dB, slow hold off, the rest
    // default (peak guard on): gain for a 1 kHz sine at -48 ... 0 dBFS in 6 dB steps.
    let stored = [0.05, 0.10, 0.20, 0.37, 0.65, 0.97, 0.76, -3.60, -9.60];
    for sr in [48_000usize, 96_000] {
        for (k, want) in stored.iter().enumerate() {
            let level = -48.0 + 6.0 * k as f64;
            let x: Vec<f64> = (0..sr).map(|i| lin(level) * (2.0 * PI * 1000.0 * i as f64 / sr as f64).sin()).collect();
            let mut m = module(&[(P_THRESHOLD, -9.6), (P_SLOW_HOLD, 0.0)]);
            let y = render(&mut m, &x, sr as f64);
            let got = db(sine_level(&y[sr / 2..sr - sr / 10])) - level;
            println!("INFO | static gain {sr} Hz | input {level} dBFS | mine {got:.3} | stored {want:.3} | diff {:+.3}", got - want);
            assert!((got - want).abs() < 0.05, "{sr} {level}: {got}");
        }
    }
}

#[test]
fn peak_guard_release_figures_are_reported() {
    // SPEC_LIMITERS.md 54.1 (data: recheck/results.json): same signal and analysis as the 1 kHz
    // release test, with the peak guard on. Not an acceptance row; the 63 % times are reported
    // only, and the settled gain is checked.
    let n = 20 * SR;
    let mut a = vec![lin(-30.0); n];
    let e = SR / 1000;
    for t0 in [1usize, 9] {
        let (i, j) = (t0 * SR, ((t0 as f64 + 0.2) * SR as f64) as usize);
        for v in a[i..j].iter_mut() {
            *v = lin(-6.0);
        }
        for k in 0..e {
            let rc = 0.5 - 0.5 * (PI * k as f64 / e as f64).cos();
            a[i - e + k] = lin(-30.0) + (lin(-6.0) - lin(-30.0)) * rc;
            a[j + k] = lin(-6.0) - (lin(-6.0) - lin(-30.0)) * rc;
        }
    }
    let x: Vec<f64> = (0..n).map(|i| a[i] * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect();
    let hx = hil(&x);
    let stored = [(0.0, 0.96), (0.25, 4.85), (0.5, 16.65), (0.75, 54.94), (1.0, 974.44)];
    for (p, t63) in stored {
        let mut m = module(&[(P_THRESHOLD, -9.6), (P_SLOW_HOLD, 0.0), (P_ATTACK, 0.052), (P_RELEASE, plain_from_position(P_RELEASE, p))]);
        let y = render(&mut m, &x, SR as f64);
        let g: Vec<f64> = hil(&y[..n]).iter().zip(hx.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect();
        let gb = median(&g[(9.1 * SR as f64) as usize..(9.19 * SR as f64) as usize]);
        let ge = median(&g[16 * SR..17 * SR]);
        let off = (9.2 * SR as f64) as usize;
        let hit = g[off..].iter().position(|v| *v >= gb + 0.63 * (ge - gb)).unwrap() as f64 / SR as f64 * 1000.0;
        let ms = plain_from_position(P_RELEASE, p);
        println!("INFO | peak guard on, release {ms:.3} ms | 63 % recovery ms | mine {hit:.3} | stored {t63:.3} | diff {:+.3}", hit - t63);
        println!("INFO | peak guard on, release {ms:.3} ms | gain at the end dB | mine {ge:.3} | stored 0.370 | diff {:+.3}", ge - 0.37);
        println!("INFO | peak guard on, release {ms:.3} ms | gain in burst dB | mine {gb:.3} | stored -3.600 | diff {:+.3}", gb + 3.6);
        assert!((ge - 0.37).abs() < 0.05);
    }
}

#[test]
fn random_settings_never_exceed_full_scale_and_the_guard_never_exceeds_the_threshold() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for case in 0..60 {
        let guard = rng.next() < 0.5;
        let thr = -12.0 * rng.next() as f32;
        let sets = [
            (P_INPUT_GAIN, (36.0 * rng.next() - 18.0) as f32),
            (P_THRESHOLD, thr),
            (P_ATTACK, plain_from_position(P_ATTACK, rng.next() as f32)),
            (P_RELEASE, plain_from_position(P_RELEASE, rng.next() as f32)),
            (P_KNEE, if rng.next() < 0.5 { 0.0 } else { 10.0 * rng.next() as f32 }),
            (P_LIFT, if rng.next() < 0.3 { 0.0 } else { 125.0 * rng.next() as f32 }),
            (P_OUTPUT_TRIM, 0.0),
            (P_SLOW_HOLD, (rng.next() < 0.5) as u8 as f32),
            (P_PEAK_GUARD, guard as u8 as f32),
            (P_TRUE_PEAK, (rng.next() < 0.5) as u8 as f32),
        ];
        let sr = [44_100.0, 48_000.0, 96_000.0][case % 3];
        let (l, r) = programme(SR, &mut rng);
        let mut m = module(&sets);
        let (yl, yr) = render_stereo(&mut m, &l, &r, sr);
        let peak = max_abs(&yl).max(max_abs(&yr));
        assert!(yl.iter().chain(yr.iter()).all(|v| v.is_finite()), "case {case}: {sets:?}");
        assert!(peak <= 1.0, "case {case}: peak {peak} with {sets:?}");
        if guard {
            let limit = lin(thr as f64) * (1.0 + 1e-6);
            assert!(peak <= limit, "case {case}: peak {peak} over the threshold {limit} with {sets:?}");
        }
        assert!(m.meters().reduction_db >= 0.0);
    }
}

#[test]
fn odd_inputs_do_not_panic_and_reset_repeats_exactly() {
    let mut m = ClearLimiter::default();
    // Before prepare: nothing happens.
    let (mut l, mut r) = (vec![0.5f32; 64], vec![0.5f32; 64]);
    m.process(&mut l, &mut r);
    assert!(l.iter().all(|v| *v == 0.5));
    m.set_param(99, 1.0);
    m.set_param(P_THRESHOLD, f32::NAN);
    m.set_param(P_THRESHOLD, -1.0e9);
    m.prepare(48_000.0, 64);
    // Unequal lengths, empty blocks, non-finite samples.
    m.process(&mut l[..10], &mut r[..7]);
    m.process(&mut l[..0], &mut r[..0]);
    let mut bad = vec![f32::NAN, f32::INFINITY, -f32::INFINITY, 1.0e30, 0.0, 0.1];
    let mut bad2 = bad.clone();
    m.process(&mut bad, &mut bad2);
    // After reset the module is deterministic and independent of the block size.
    let mut rng = Rng(99);
    let (xl, xr) = programme(30_000, &mut rng);
    let run = |m: &mut ClearLimiter, block: usize| {
        m.reset();
        let mut l: Vec<f32> = xl.iter().map(|v| *v as f32).collect();
        let mut r: Vec<f32> = xr.iter().map(|v| *v as f32).collect();
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            m.process(a, b);
        }
        (l, r)
    };
    let mut m = module(&[(P_INPUT_GAIN, 9.0), (P_THRESHOLD, -3.0), (P_LIFT, 60.0), (P_TRUE_PEAK, 1.0)]);
    m.prepare(48_000.0, 512);
    let a = run(&mut m, 512);
    let b = run(&mut m, 37);
    assert_eq!(a.0, b.0);
    assert_eq!(a.1, b.1);
}

#[test]
fn low_frequency_distortion_is_reported() {
    // Not measured on the original (all stored figures are at 1 kHz and above). Reported so the
    // behaviour of this build on low tones is known.
    let cases: [(&str, f64, f64, Vec<(usize, f32)>); 5] = [
        ("limiting 3.6 dB, defaults but slow hold off", 50.0, -6.0, vec![(P_THRESHOLD, -9.6), (P_SLOW_HOLD, 0.0)]),
        ("limiting 3.6 dB, defaults but slow hold off", 100.0, -6.0, vec![(P_THRESHOLD, -9.6), (P_SLOW_HOLD, 0.0)]),
        ("limiting 3.6 dB, peak guard off, release 0.052 ms", 100.0, -6.0, vec![(P_THRESHOLD, -9.6), (P_SLOW_HOLD, 0.0), (P_PEAK_GUARD, 0.0), (P_RELEASE, 0.052)]),
        ("lift 100 %, no limiting", 50.0, -12.0, vec![(P_LIFT, 100.0)]),
        ("lift 100 %, no limiting", 100.0, -12.0, vec![(P_LIFT, 100.0)]),
    ];
    for (name, freq, level, sets) in cases.iter() {
        let n = 3 * SR;
        let x: Vec<f64> = (0..n).map(|i| lin(*level) * (2.0 * PI * freq * i as f64 / SR as f64).sin()).collect();
        let mut m = module(sets);
        let y = render(&mut m, &x, SR as f64);
        let seg = &y[SR..3 * SR];
        let f = hann_spectrum(seg);
        let kk = (freq * seg.len() as f64 / SR as f64).round() as usize;
        let fund: f64 = f[kk - 3..kk + 4].iter().map(|v| v * v).sum();
        let all: f64 = f[4..].iter().map(|v| v * v).sum();
        println!("INFO | low tones | {name}, {freq} Hz at {level} dBFS | THD+N {:.1} dB | level out {:.2} dBFS", 10.0 * ((all - fund) / fund + 1e-30).log10(), db(max_abs(seg)));
    }
}

#[test]
fn processing_cost() {
    let mut rng = Rng(5);
    let seconds = 20;
    let (xl, xr) = programme(seconds * SR, &mut rng);
    let cases: [(&str, Vec<(usize, f32)>); 3] = [
        ("defaults, input +9 dB", vec![(P_INPUT_GAIN, 9.0)]),
        ("everything on: true-peak hold, lift 100 %, knee 5 dB, input +9 dB", vec![(P_INPUT_GAIN, 9.0), (P_TRUE_PEAK, 1.0), (P_LIFT, 100.0), (P_KNEE, 5.0)]),
        ("idle: signal far below the threshold", vec![(P_INPUT_GAIN, -18.0), (P_PEAK_GUARD, 0.0)]),
    ];
    for (name, sets) in cases.iter() {
        let mut m = module(sets);
        m.prepare(48_000.0, 512);
        let mut l: Vec<f32> = xl.iter().map(|v| *v as f32).collect();
        let mut r: Vec<f32> = xr.iter().map(|v| *v as f32).collect();
        let t = std::time::Instant::now();
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            m.process(a, b);
        }
        let dt = t.elapsed().as_secs_f64();
        println!("INFO | cost | {name} | {:.1} ns per stereo sample | {:.3} % of one core at 48 kHz", dt / (seconds * SR) as f64 * 1e9, dt / seconds as f64 * 100.0);
    }
}
