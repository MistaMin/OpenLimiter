//! No allocation or free in `process`, `process_keyed`, `set_param`, `reset`, `latency`,
//! `meters` or `view`, across modes and oversampling changes.

use comp_harmonic::{idx, view, HarmonicCompressor};
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

#[test]
fn audio_path_does_not_allocate() {
    let mut m = HarmonicCompressor::default();
    m.set_param(idx::H3, 40.0);
    m.prepare(48000.0, 512);
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let (mut kl, mut kr) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let mut vb = [0.0f32; view::LEN];
    let n_params = m.params().len();
    WATCH.with(|w| w.set(true));
    let mut seed = 1u32;
    for b in 0..1200 {
        for i in 0..512 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * if b % 20 < 10 { 1.5 } else { 0.1 };
            l[i] = v;
            r[i] = -v;
            kl[i] = v * 0.5;
            kr[i] = v;
        }
        if b % 7 == 0 {
            let p = m.params()[b % n_params];
            m.set_param(b % n_params, p.min + (p.max - p.min) * ((b % 5) as f32 / 4.0));
        }
        if b % 50 == 0 {
            m.set_param(idx::OS_MODE, ((b / 50) % 4) as f32);
            m.set_param(idx::OS_FILTER, ((b / 200) % 2) as f32);
            m.set_param(idx::MODE, ((b / 50) % 7) as f32);
            m.set_param(idx::KEY_SOURCE, ((b / 100) % 2) as f32);
        }
        if b == 700 {
            m.reset();
        }
        let len = [512usize, 1, 77, 300][b % 4];
        if b % 3 == 0 {
            m.process_keyed(&mut l[..len], &mut r[..len], &kl[..len], &kr[..len]);
        } else {
            m.process(&mut l[..len], &mut r[..len]);
        }
        let _ = m.meters();
        let _ = m.latency();
        let _ = m.view(&mut vb);
    }
    WATCH.with(|w| w.set(false));
    let count = COUNT.with(|c| c.get());
    println!("allocations or frees in 1200 blocks with parameter, mode and oversampling changes and a reset: {count}");
    assert_eq!(count, 0);
    assert!(l.iter().chain(&r).all(|v| v.is_finite()));
}
