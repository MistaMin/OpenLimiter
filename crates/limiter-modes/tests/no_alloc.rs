//! The audio path does not allocate: `process`, `set_param`, `reset`, `latency`, `meters`
//! and `learn_drive` are run under a counting allocator. One test only, so no other thread
//! allocates while the counter is read.
use limiter_modes::*;
use olm_api::Module;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::SeqCst);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::SeqCst);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn audio_path_does_not_allocate() {
    let mut m = LimiterModes::default();
    m.prepare(48_000.0, 512);
    let mut l = vec![0.0f32; 512];
    let mut r = vec![0.0f32; 512];
    let before = COUNT.load(Ordering::SeqCst);
    let mut seed = 1u32;
    for block in 0..400usize {
        for p in 0..m.params().len() {
            let d = m.params()[p];
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = d.min + (d.max - d.min) * (seed >> 8) as f32 / (1u32 << 24) as f32;
            if (block + p) % 5 == 0 {
                m.set_param(p, v);
            }
        }
        for (i, (a, b)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *a = (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            *b = if i % 7 == 0 { -*a } else { 0.3 * *a };
        }
        m.process(&mut l, &mut r);
        let _ = (m.latency(), m.meters());
        if block % 50 == 49 {
            m.reset();
            m.learn_drive(-20.0);
        }
        assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()));
    }
    assert_eq!(COUNT.load(Ordering::SeqCst), before, "allocation in the audio path");
}
