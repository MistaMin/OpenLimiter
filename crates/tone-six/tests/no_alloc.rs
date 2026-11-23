//! `process`, `set_param` and `reset` never allocate (counting global allocator).

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use olm_api::Module;
use tone_six::{ToneSix, NPARAMS, PARAMS};

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
    let mut m = ToneSix::default();
    m.prepare(48000.0, 512);
    let x = noise(512, 2, 0.5);
    let (mut l, mut r) = (x.clone(), x.clone());
    ARMED.with(|a| a.set(true));
    for k in 0..2000 {
        let idx = (k * 131) % NPARAMS;
        let p = PARAMS[idx];
        m.set_param(idx, p.min + (p.max - p.min) * ((k % 17) as f32 / 16.0));
        m.process(&mut l, &mut r);
        if k % 500 == 0 {
            m.reset();
        }
    }
    ARMED.with(|a| a.set(false));
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
}
