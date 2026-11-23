//! `process`, `process_keyed`, `set_param` and `reset` never allocate (counting global allocator).

use olm_api::Module;
use reso_suppress::{ResonanceSuppressor, NPARAMS, PARAMS};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);
thread_local! { static ARMED: Cell<bool> = const { Cell::new(false) }; }

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn audio_path_does_not_allocate() {
    let mut m = ResonanceSuppressor::default();
    m.prepare(48000.0, 512);
    let x: Vec<f32> = (0..512).map(|i| ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.5).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    ARMED.with(|a| a.set(true));
    for k in 0..1500 {
        let idx = (k * 131) % NPARAMS;
        let p = PARAMS[idx];
        m.set_param(idx, p.min + (p.max - p.min) * ((k % 17) as f32 / 16.0));
        if k % 2 == 0 {
            m.process(&mut l, &mut r);
        } else {
            m.process_keyed(&mut l, &mut r, &x, &x);
        }
        let mut v = [0.0f32; 300];
        m.view(&mut v);
        let _ = m.meters();
        if k % 500 == 0 {
            m.reset();
        }
    }
    ARMED.with(|a| a.set(false));
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
}
