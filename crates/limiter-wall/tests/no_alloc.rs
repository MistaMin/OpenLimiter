//! The api crate's rule: `process`, `set_param` and `reset` never allocate. A counting global
//! allocator checks it (single test in this binary, so no other thread allocates meanwhile).

use limiter_wall::*;
use olm_api::Module;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::SeqCst);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::SeqCst);
        System.realloc(p, l, n)
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn audio_path_does_not_allocate() {
    let mut m = WallLimiter::default();
    m.prepare(48000.0, 512);
    let mut l = vec![0.0f32; 512];
    let mut r = vec![0.0f32; 512];
    let mut seed = 1u32;
    let before = ALLOCS.load(Ordering::SeqCst);
    for k in 0..400usize {
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *a = (seed >> 8) as f32 / 8388608.0 - 1.0;
            *b = -*a * 0.7;
        }
        // walk through every parameter, including the two that change the delay
        m.set_param(P_STYLE, ((k / 40) % 2) as f32);
        m.set_param(P_STRICT_TRUE_PEAK, ((k / 80) % 2) as f32);
        m.set_param(P_DRIVE, (k % 19) as f32);
        m.set_param(P_THRESHOLD, -((k % 31) as f32));
        m.set_param(P_CEILING, -((k % 7) as f32));
        m.set_param(P_AUTO_RELEASE, ((k / 3) % 2) as f32);
        m.set_param(P_RELEASE, 0.01 + (k % 50) as f32 * 60.0);
        m.set_param(P_PROFILE, (k % 4) as f32);
        m.set_param(P_SOFT_CLIP, ((k / 5) % 2) as f32);
        m.set_param(P_SOFT_CLIP_THRESHOLD, -((k % 21) as f32));
        m.set_param(P_SOFT_CLIP_CURVE, (k % 3) as f32);
        m.process(&mut l, &mut r);
        let _ = m.meters();
        let _ = m.latency();
        if k % 97 == 0 {
            m.reset();
        }
    }
    let after = ALLOCS.load(Ordering::SeqCst);
    assert_eq!(after - before, 0, "allocations in the audio path");
    assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()));
}
