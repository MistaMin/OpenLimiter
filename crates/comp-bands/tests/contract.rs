//! The `Module` contract: parameters, latency, block-size independence, no allocation in the
//! audio path, reset, keyed processing, meters, other sample rates.
mod common;
use common::*;
use comp_bands::*;
use olm_api::{Kind, Module};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static WATCH: AtomicBool = AtomicBool::new(false);
thread_local! { static ON_THIS_THREAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if WATCH.load(Ordering::Relaxed) && ON_THIS_THREAD.try_with(|v| v.get()).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn parameter_table_is_sound() {
    let m = module();
    let p = m.params();
    assert_eq!(p.len(), PARAM_COUNT);
    for (i, d) in p.iter().enumerate() {
        assert!(d.min < d.max && d.default >= d.min && d.default <= d.max, "{}", d.id);
        assert!(d.id.chars().next().unwrap().is_ascii_lowercase() && d.id.chars().all(|c| c.is_ascii_alphanumeric()), "{}", d.id);
        assert!(p.iter().skip(i + 1).all(|o| o.id != d.id), "duplicate id {}", d.id);
        assert_eq!(m.param(i), d.default);
        if d.log {
            assert!(d.min > 0.0);
        }
    }
    // The defaults the measurements name (section 2 and section 3 of the source F table file).
    assert_eq!(p[P_REFERENCE].default, -8.0);
    assert_eq!(p[P_LOOKAHEAD].default, 10.0);
    assert_eq!([p[P_SPLIT_1].default, p[P_SPLIT_2].default, p[P_SPLIT_3].default, p[P_SPLIT_4].default], [125.0, 500.0, 2000.0, 8000.0]);
    assert!(matches!(p[P_MODE].kind, Kind::Choice(l) if l.len() == 4));
    assert!(matches!(p[P_DETECTOR].kind, Kind::Choice(l) if l.len() == 9));
    let thr: Vec<f32> = (0..BANDS).map(|b| p[band_param(b, B_THRESHOLD)].default).collect();
    assert_eq!(thr, [-7.0, -7.0, -8.0, -9.0, -10.0]);
    // All five bands are real parameters, the middle one included.
    assert_eq!(p[band_param(2, B_THRESHOLD)].id, "midThreshold");
}

#[test]
fn set_param_clamps_and_ignores_unknown() {
    let mut m = module();
    m.set_param(P_REFERENCE, 99.0);
    assert_eq!(m.param(P_REFERENCE), 0.0);
    m.set_param(band_param(1, B_RATIO), -3.0);
    assert_eq!(m.param(band_param(1, B_RATIO)), 1.0);
    m.set_param(P_MODE, 2.4);
    assert_eq!(m.param(P_MODE), 2.0);
    m.set_param(P_LOOKAHEAD, f32::NAN);
    assert_eq!(m.param(P_LOOKAHEAD), 10.0);
    m.set_param(PARAM_COUNT, 1.0);
    m.set_param(usize::MAX, 1.0);
    // Parameters survive prepare and reset.
    m.set_param(band_param(4, B_ATTACK), 3.0);
    m.prepare(96000.0, 64);
    m.reset();
    assert_eq!(m.param(band_param(4, B_ATTACK)), 3.0);
}

/// Section 1 / 4.6: the delay does not depend on any compressor setting, the look-ahead
/// included (the original: 769 samples for the whole unit; this module: 15 ms).
#[test]
fn latency_is_constant_and_exact() {
    let mut m = module();
    assert_eq!(m.latency(), 720);
    for look in [0.0, 1.0, 8.5, 15.0] {
        m.set_param(P_LOOKAHEAD, look);
        assert_eq!(m.latency(), 720);
    }
    for (sr, want) in [(44100.0, 662), (96000.0, 1440)] {
        let mut m = BandCompressor::default();
        m.prepare(sr, 256);
        assert_eq!(m.latency(), want);
    }
    // An impulse comes out exactly `latency` samples later, whatever the look-ahead.
    for look in [0.0, 10.0, 15.0] {
        let mut m = module();
        nothing_compresses(&mut m);
        m.set_param(P_LOOKAHEAD, look);
        let mut l = vec![0.0f32; 2048];
        l[100] = 0.5;
        let mut r = l.clone();
        m.process(&mut l, &mut r);
        let at = (0..l.len()).max_by(|a, b| l[*a].abs().partial_cmp(&l[*b].abs()).unwrap()).unwrap();
        assert_eq!(at, 100 + m.latency());
        assert!((l[at] - 0.5).abs() < 1e-6);
    }
}

#[test]
fn block_size_does_not_change_the_result() {
    let x = pink(21);
    let (l, r) = (&x[..SRU], &x[SRU..2 * SRU]);
    let run = |sizes: &[usize]| {
        let mut m = module();
        m.set_param(P_MODE, MODE_PARALLEL_UNLINKED);
        m.set_param(P_BAND_LIMITER, 1.0);
        m.set_param(P_REFERENCE, -24.0);
        let (mut a, mut b) = (l.to_vec(), r.to_vec());
        let (mut i, mut k) = (0, 0);
        while i < a.len() {
            let n = sizes[k % sizes.len()].min(a.len() - i);
            m.process(&mut a[i..i + n], &mut b[i..i + n]);
            i += n;
            k += 1;
        }
        (a, b)
    };
    let one = run(&[512]);
    let two = run(&[1, 7, 64, 3, 500, 512, 33]);
    assert!(one.0 == two.0 && one.1 == two.1);
    // Empty and unequal slices do not panic.
    let mut m = module();
    m.process(&mut [], &mut []);
    let (mut a, mut b) = ([0.1f32; 8], [0.1f32; 5]);
    m.process(&mut a, &mut b);
}

#[test]
fn reset_clears_state_and_default_is_usable_without_prepare() {
    let x = pink(4);
    let x = &x[..SRU / 2];
    let mut m = BandCompressor::default(); // no prepare call
    m.set_param(P_REFERENCE, -24.0);
    let first = render_mono(&mut m, x);
    m.reset();
    let second = render_mono(&mut m, x);
    assert!(first == second);
    assert!(first.iter().all(|v| v.is_finite()));
}

#[test]
fn no_allocation_in_process_set_param_or_reset() {
    let x = pink(9);
    let mut m = module();
    let mut l = x[..4096].to_vec();
    let mut r = x[4096..8192].to_vec();
    let key = x[8192..12288].to_vec();
    ON_THIS_THREAD.with(|v| v.set(true));
    ALLOCATIONS.store(0, Ordering::SeqCst);
    WATCH.store(true, Ordering::SeqCst);
    for round in 0..4 {
        m.set_param(P_MODE, round as f32);
        m.set_param(P_BAND_LIMITER, (round % 2) as f32);
        m.set_param(P_LOOKAHEAD, 3.0 * round as f32);
        m.set_param(P_SPLIT_2, 300.0 + 100.0 * round as f32);
        m.set_param(band_param(round, B_ATTACK), 1.0 + round as f32);
        for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            m.process(a, b);
        }
        m.process_keyed(&mut l[..512], &mut r[..512], &key[..512], &key[512..1024]);
        let _ = m.meters();
        let _ = m.latency();
        m.reset();
    }
    let in_audio_path = ALLOCATIONS.load(Ordering::SeqCst);
    // The counter does see allocations on this thread.
    let probe = vec![0u8; 64];
    assert!(ALLOCATIONS.load(Ordering::SeqCst) > in_audio_path && probe.len() == 64);
    WATCH.store(false, Ordering::SeqCst);
    ON_THIS_THREAD.with(|v| v.set(false));
    assert_eq!(in_audio_path, 0);
}

/// The detectors listen to the key; with the key equal to the input the result is the same as
/// without a key, and a silent key leaves the audio uncompressed.
#[test]
fn keyed_processing() {
    let x = fine_steps(4000.0);
    let x = &x[20 * SRU..22 * SRU]; // -10 and -8 dBFS
    let run = |key: Option<&[f32]>| {
        let mut m = module();
        set_band(&mut m, 3, B_RATIO, RATIO_INFINITE);
        // The key is used only with Key Source on External.
        m.set_param(P_KEY_SOURCE, 1.0);
        let lat = m.latency();
        let mut a: Vec<f32> = x.to_vec();
        a.extend(std::iter::repeat(0.0).take(lat));
        let mut b = a.clone();
        match key {
            Some(k) => {
                let mut kk = k.to_vec();
                kk.extend(std::iter::repeat(0.0).take(lat));
                m.process_keyed(&mut a, &mut b, &kk, &kk)
            }
            None => m.process(&mut a, &mut b),
        }
        a[lat..].to_vec()
    };
    let plain = run(None);
    let same_key = run(Some(x));
    assert!(plain == same_key);
    let silent = run(Some(&vec![0.0; x.len()]));
    let g = db(rms(&silent[SRU / 2..SRU])) - db(rms(&x[SRU / 2..SRU]));
    let gp = db(rms(&plain[SRU / 2..SRU])) - db(rms(&x[SRU / 2..SRU]));
    println!("4 kHz at -10 dBFS: {gp:.2} dB with its own detector, {g:.3} dB with a silent key");
    assert!(g.abs() < 0.001 && gp < -3.0);
}

/// Per-band gain reduction on the meters: a 4 kHz tone at -6 dBFS with the high-mid band at
/// threshold -10, Inf:1 puts about 10.8 dB on the high-mid band (band level -7.85 dBFS, detector
/// weighting +0.7 dB, threshold -18 dBFS), less on its neighbours, none on the lowest band.
#[test]
fn meters_report_per_band_reduction() {
    let mut m = module();
    set_band(&mut m, 3, B_THRESHOLD, -10.0);
    set_band(&mut m, 3, B_RATIO, RATIO_INFINITE);
    assert_eq!(m.meters().reduction_db, 0.0);
    let x: Vec<f32> = (0..SRU).map(|i| (lin(-6.0) * (2.0 * std::f64::consts::PI * 4000.0 * i as f64 / SR as f64).sin()) as f32).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(a, b);
    }
    let mt = m.meters();
    println!("band reduction {:?}, deepest {:.2}", &mt.band_reduction_db[..5], mt.reduction_db);
    assert!((mt.band_reduction_db[3] - 10.85).abs() < 0.3);
    assert!(mt.band_reduction_db[0] == 0.0 && mt.band_reduction_db[2] > 0.5 && mt.band_reduction_db[4] > 0.5);
    assert!(mt.band_reduction_db[5..].iter().all(|v| *v == 0.0));
    assert_eq!(mt.reduction_db, mt.band_reduction_db[3]);
}

/// Section 10: at 96 kHz the middle band's static curve at 1 kHz is the same as at 48 kHz
/// ("within 0.01 dB"; here within 0.1 dB, because the detector weighting bell is designed per
/// sample rate), and the bands still sum flat.
#[test]
fn works_at_96_khz() {
    let sr = 96000.0f64;
    let mut m = BandCompressor::default();
    m.prepare(sr as f32, 512);
    let lat = m.latency();
    let levels = [-12.0, -6.0, 0.0];
    let want = [-0.73, -3.20, -6.27];
    for (lv, w) in levels.iter().zip(want.iter()) {
        m.reset();
        let n = sr as usize;
        let mut l: Vec<f32> = (0..n + lat).map(|i| (lin(*lv) * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / sr).sin()) as f32).collect();
        let src = l.clone();
        let mut r = l.clone();
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            m.process(a, b);
        }
        let g = db(rms(&l[lat + n / 2..lat + n * 9 / 10])) - db(rms(&src[n / 2..n * 9 / 10]));
        println!("96 kHz, 1 kHz at {lv} dBFS: {g:.2} dB (measured at 48 and 96 kHz: {w})");
        assert!((g - w).abs() < 0.1);
    }
    let mut m = BandCompressor::default();
    m.prepare(sr as f32, 512);
    nothing_compresses(&mut m);
    let x = pink(2);
    let lat = m.latency();
    let mut l = x[..40000].to_vec();
    let mut r = l.clone();
    m.process(&mut l, &mut r);
    let err: Vec<f32> = (lat..l.len()).map(|i| l[i] - x[i - lat]).collect();
    assert!(db(rms(&err)) - db(rms(&x[..40000])) < -120.0);
}
