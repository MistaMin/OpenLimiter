//! `process`, `process_keyed`, `set_param` and `reset` never allocate (counting global allocator).

use olm_api::Module;
use eq_parametric::{ParametricEq, NPARAMS, PARAMS};
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
    let mut m = ParametricEq::default();
    m.prepare(48000.0, 512);
    let x: Vec<f32> = (0..512).map(|i| ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.5).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    // Every mode once with time for kernels to arrive (the worker thread allocates them; the
    // audio thread only swaps pointers), the split path, and 2x oversampling.
    let mode = eq_parametric::P_PROCESSING_MODE;
    ARMED.with(|a| a.set(true));
    for (id, v) in [(mode, 1.0), (mode, 2.0), (mode, 3.0), (mode, 4.0), (mode, 0.0)] {
        m.set_param(id, v);
        m.set_param(eq_parametric::band_param(0, eq_parametric::F_GAIN), 6.0 + v);
        for _ in 0..60 {
            m.process(&mut l, &mut r);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    m.set_param(eq_parametric::band_param(0, eq_parametric::F_SPLIT), 1.0);
    m.set_param(eq_parametric::P_OVERSAMPLE2X, 1.0);
    for _ in 0..100 {
        m.process(&mut l, &mut r);
    }
    for k in 0..1500 {
        let idx = (k * 131) % NPARAMS;
        let p = PARAMS[idx];
        m.set_param(idx, p.min + (p.max - p.min) * ((k % 17) as f32 / 16.0));
        if k % 2 == 0 {
            m.process(&mut l, &mut r);
        } else {
            m.process(&mut l, &mut r);
        }
        let mut v = [0.0f32; 8];
        let mut c = [0.0f32; 4];
        m.response_db(&[100.0, 1000.0, 5000.0, 15000.0], &mut c);
        let (mut cl, mut cr) = ([0.0f32; 4], [0.0f32; 4]);
        m.response_channels_db(&[100.0, 1000.0, 5000.0, 15000.0], &mut cl, &mut cr);
        m.band_response_db(k % 32, None, &[100.0, 1000.0, 5000.0, 15000.0], &mut c);
        let _ = m.latency();
        m.view(&mut v);
        let _ = m.meters();
        if k % 500 == 0 {
            m.reset();
        }
    }
    ARMED.with(|a| a.set(false));
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
}
