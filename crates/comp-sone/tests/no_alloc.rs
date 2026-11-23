//! No allocation or free in `process`, `set_param`, `reset`, `latency` or `meters`
//! (port of the plugin test "no allocation or free in the audio callback").

use comp_sone::DualSone;
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
    let mut m = DualSone::default();
    m.prepare(48000.0, 512);
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let n_params = m.params().len();
    WATCH.with(|w| w.set(true));
    let mut seed = 1u32;
    for b in 0..400 {
        for i in 0..512 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * if b % 20 < 10 { 1.0 } else { 0.1 };
            l[i] = v;
            r[i] = -v;
        }
        if b % 7 == 0 {
            let p = m.params()[b % n_params];
            m.set_param(b % n_params, p.min + (p.max - p.min) * ((b % 5) as f32 / 4.0));
        }
        if b == 300 {
            m.reset();
        }
        m.process(&mut l, &mut r);
        let _ = m.meters();
        let _ = m.latency();
    }
    WATCH.with(|w| w.set(false));
    let count = COUNT.with(|c| c.get());
    println!("allocations or frees in 400 blocks with parameter changes and a reset: {count}");
    assert_eq!(count, 0);
}
