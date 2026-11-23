//! `process`, `set_param`, `reset` and `view` never allocate (counting global allocator); and
//! the cost per sample of each anti-alias choice.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use harmonics::*;
use olm_api::Module;

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
    let mut m = Harmonics::default();
    m.prepare(48000.0, 512);
    let x = noise(512, 2, 0.9);
    let (mut l, mut r) = (x.clone(), x.clone());
    let mut out = [0.0f32; view::LEN];
    ARMED.with(|a| a.set(true));
    for k in 0..4000 {
        let idx = (k * 131) % NPARAMS;
        let p = PARAMS[idx];
        m.set_param(idx, p.min + (p.max - p.min) * ((k % 17) as f32 / 16.0));
        m.process(&mut l, &mut r);
        m.view(&mut out);
        let _ = m.meters();
        let _ = m.latency();
        if k % 500 == 0 {
            m.reset();
        }
        if k % 7 == 0 {
            l.copy_from_slice(&x);
            r.copy_from_slice(&x);
        }
    }
    ARMED.with(|a| a.set(false));
    assert_eq!(COUNT.load(Ordering::SeqCst), 0);
}

#[test]
fn cost_per_sample() {
    let n = 48000 * 2;
    let x = noise(n, 9, 0.5);
    println!("stereo, ns per sample pair (release build, this machine):");
    for (aa, name) in [(AA_NONE, "none"), (AA_ADAA, "ADAA"), (AA_ADAA2, "ADAA2"), (AA_OS2, "2x"), (AA_OS4, "4x")] {
        let mut line = format!("  {name:6}");
        for fam in [POLYNOMIAL, SATURATOR, TAPE] {
            let mut m = module(&[(P_ANTI_ALIAS, aa as f32), (P_FAMILY, fam as f32), (P_DRIVE, 6.0), (P_AUTO_GAIN, 1.0)]);
            let (mut l, mut r) = (x.clone(), x.clone());
            let t = std::time::Instant::now();
            for c in 0..n / 512 {
                m.process(&mut l[c * 512..(c + 1) * 512], &mut r[c * 512..(c + 1) * 512]);
            }
            let ns = t.elapsed().as_nanos() as f64 / n as f64;
            line += &format!("  {}: {ns:6.1}", ["poly", "sat", "tape"][fam]);
            assert!(l.iter().all(|v| v.is_finite()));
        }
        println!("{line}");
    }
}
