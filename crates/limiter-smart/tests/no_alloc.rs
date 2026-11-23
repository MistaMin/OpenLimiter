//! Spec 7.1: no allocation or free in `process`, `set_param`, `set_offline`, `reset`,
//! `latency`, `meters`, `view`, across engine / oversampling / crossover / channel-mode
//! switching; non-finite input and random-parameter fuzzing keep the output finite.

mod common;
use common::*;
use limiter_smart::{idx, view, SmartLimiter};
use olm_api::Module;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn note() {
    let _ = WATCH.try_with(|w| {
        if w.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        note();
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        note();
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        note();
        System.realloc(p, l, n)
    }
}

#[global_allocator]
static A: Counting = Counting;

fn watched<F: FnOnce()>(f: F) -> usize {
    COUNT.with(|c| c.set(0));
    WATCH.with(|w| w.set(true));
    f();
    WATCH.with(|w| w.set(false));
    COUNT.with(|c| c.get())
}

#[test]
fn audio_path_does_not_allocate() {
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        let mut m = SmartLimiter::default();
        m.prepare(sr, 512);
        let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        let mut vbuf = [0.0f32; view::LEN];
        let n_params = m.params().len();
        let mut rng = Rng(12345);
        let mut blocks = 0;
        let count = watched(|| {
            for b in 0..1200usize {
                for i in 0..512 {
                    let v = rng.next() * if b % 20 < 10 { 1.5 } else { 0.05 };
                    l[i] = v as f32;
                    r[i] = (-0.7 * v) as f32;
                }
                // structural switching: every engine, every oversampling factor, both
                // crossovers and both channel modes, offline on and off
                if b % 37 == 0 {
                    m.set_param(idx::ENGINE, ((b / 37) % 6) as f32);
                }
                if b % 53 == 0 {
                    m.set_param(idx::OVERSAMPLING, ((b / 53) % 3) as f32);
                }
                if b % 71 == 0 {
                    m.set_param(idx::CROSSOVER_MODE, ((b / 71) % 2) as f32);
                    m.set_param(idx::CHANNEL_MODE, ((b / 71 + 1) % 2) as f32);
                }
                if b % 89 == 0 {
                    m.set_offline((b / 89) % 2 == 1);
                }
                if b % 5 == 0 {
                    let i = (rng.next().abs() * n_params as f64) as usize % n_params;
                    let p = m.params()[i];
                    if i != idx::ENGINE && i != idx::OVERSAMPLING {
                        m.set_param(i, p.min + (p.max - p.min) * rng.next().abs() as f32);
                    }
                }
                if b == 600 {
                    m.reset();
                }
                m.process(&mut l, &mut r);
                let _ = m.meters();
                let _ = m.latency();
                let _ = m.view(&mut vbuf);
                let _ = m.lift_db();
                blocks += 1;
            }
        });
        println!("{sr} Hz: allocations or frees in {blocks} blocks with switching, offline, reset and parameter changes: {count}");
        assert_eq!(count, 0);
    }
}

#[test]
fn fuzz_non_finite_and_random_parameters() {
    let mut m = SmartLimiter::default();
    m.prepare(48000.0, 256);
    let n_params = m.params().len();
    let mut rng = Rng(777);
    let mut worst = 0.0f64;
    let mut non_finite = 0usize;
    let specials = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e30, -1e30, f32::MIN_POSITIVE, 0.0];
    for b in 0..3000usize {
        let mut l = vec![0.0f32; 1 + (b * 7919) % 300];
        let mut r = vec![0.0f32; l.len()];
        for i in 0..l.len() {
            let v = rng.next() * 2.0;
            l[i] = v as f32;
            r[i] = rng.next() as f32;
            if (b + i) % 97 == 0 {
                l[i] = specials[(b + i) % specials.len()];
            }
            if (b + i) % 89 == 0 {
                r[i] = specials[(b * 3 + i) % specials.len()];
            }
        }
        // random parameters, including out-of-range and non-finite values
        for _ in 0..3 {
            let i = (rng.next().abs() * (n_params + 2) as f64) as usize;
            let v = match b % 4 {
                0 => f32::NAN,
                1 => (rng.next() * 1e6) as f32,
                _ => {
                    let p = m.params()[i % n_params];
                    p.min + (p.max - p.min) * rng.next().abs() as f32
                }
            };
            if i != idx::DELTA {
                m.set_param(i, v);
            }
        }
        // delta outputs the removed signal, which is as large as the driven input; it is
        // fuzzed for finiteness only, in the last 500 blocks
        m.set_param(idx::DELTA, if b >= 2500 { 1.0 } else { 0.0 });
        if b % 500 == 0 {
            m.set_offline(b % 1000 == 0);
        }
        m.process(&mut l, &mut r);
        for v in l.iter().chain(r.iter()) {
            if !v.is_finite() {
                non_finite += 1;
            } else if b < 2500 {
                worst = worst.max(v.abs() as f64);
            }
        }
    }
    let ceiling = m.param(idx::CEILING);
    println!(
        "fuzz: 3000 blocks, non-finite outputs {non_finite}, largest |output| with delta off {:.4} ({:.3} dBFS; last ceiling {ceiling} dB)",
        worst,
        db(worst)
    );
    assert_eq!(non_finite, 0);
    // The ceiling and every other setting move at random between blocks (and the lookahead
    // pipeline then still holds gains computed for the old ceiling), so the bound here is
    // full scale with a little room; the ceiling itself is tested in `ceiling.rs`.
    assert!(worst < 1.12, "largest output {worst}");
}
