//! Kernel modes, ported from OpenEQ 1.0.1: the linear-phase parts of checkRoadmap,
//! checkLinearPhaseCrossfade, checkAnalogAndMixModes and checkPhaseModes.

mod common;
use common::*;
use olm_api::Module;

fn lp(band: &[(&str, f32)], settings: &[(&str, f32)]) -> eq_parametric::ParametricEq {
    let mut s = vec![("processingMode", 1.0), ("linearPhaseQuality", 0.0)];
    s.extend_from_slice(settings);
    module_with(SR, band, &s)
}

#[test]
fn flat_kernel_latency_and_unity() {
    for (q, fs, want) in [(0.0f32, 48000.0, 1024usize), (1.0, 48000.0, 2048), (0.0, 44100.0, 1024)] {
        let mut m = module_with(fs, &[], &[("processingMode", 1.0), ("linearPhaseQuality", q)]);
        assert_eq!(m.latency(), want);
        let (l, _) = settled_impulse(&mut m, 16384, true);
        let p = peak_index(&l);
        println!("quality {q} rate {fs}: latency {} peak {p} value {}", m.latency(), l[p]);
        assert!((p as i64 - want as i64).abs() <= 2 && l[p].abs() > 0.5 && l[p].abs() < 1.5);
        assert!(l.iter().all(|v| v.is_finite()));
    }
    // Bypassed: the dry signal keeps the reported delay.
    let mut m = module_with(SR, &[], &[("bypass", 1.0), ("processingMode", 1.0), ("linearPhaseQuality", 0.0)]);
    let (l, _) = settled_impulse(&mut m, 8192, true);
    let p = peak_index(&l);
    assert_eq!(m.latency(), 1024);
    assert!((p as i64 - 1024).abs() <= 2 && (l[p] - 1.0).abs() < 0.1, "bypass peak {p} {}", l[p]);
}

#[test]
fn left_band_keeps_the_shared_delay() {
    let mut m = lp(&bell(1000.0, 12.0, 0.5, 3.0), &[]);
    let (l, r) = settled_impulse(&mut m, 8192, true);
    let (pl, pr) = (peak_index(&l), peak_index(&r));
    println!("left peak {pl} {} right peak {pr} {}", l[pl], r[pr]);
    assert!((pl as i64 - 1024).abs() <= 4 && (pr as i64 - 1024).abs() <= 2);
    assert!(r[pr].abs() > 0.5 && r[pr].abs() < 1.5 && l[pl].abs() > r[pr].abs() + 0.05);
}

#[test]
fn magnitude_matches_the_band_and_all_pass_is_left_out() {
    let mut m = lp(&bell(300.0, 6.0, 1.0, 0.0), &[]);
    let (l, _) = settled_impulse(&mut m, 4096, false);
    let g = db(impulse_mag(&l, 300.0, SR));
    println!("linear phase 300 Hz {g:.2} dB");
    assert!((g - 6.0).abs() < 0.8);
    let mut m = lp(&[("b1On", 1.0), ("b1Type", 8.0), ("b1Freq", 1000.0), ("b1Q", 0.5)], &[]);
    let (l, _) = settled_impulse(&mut m, 4096, false);
    assert!(l[peak_index(&l)].abs() < 1.05);
}

fn asymmetry(h: &[f32]) -> f64 {
    let p = peak_index(h);
    let (mut d, mut e) = (0.0, 0.0);
    for k in 1..1500 {
        if p < k || p + k >= h.len() {
            break;
        }
        let (a, b) = (h[p + k] as f64, h[p - k] as f64);
        d += (a - b) * (a - b);
        e += a * a + b * b;
    }
    if e > 0.0 {
        d / e
    } else {
        0.0
    }
}

fn impulse_for(mode: f32, phase: f32, placement: f32, settings: &[(&str, f32)]) -> Vec<f32> {
    let mut b = bell(300.0, 6.0, 1.0, placement);
    b.push(("b1Phase", phase));
    let mut s = vec![("processingMode", mode), ("linearPhaseQuality", 0.0), ("mixCrossover", 1.0)];
    s.extend_from_slice(settings);
    let mut m = module_with(SR, &b, &s);
    settled_impulse(&mut m, 4096, false).0
}

#[test]
fn analog_and_mix() {
    let linear = impulse_for(1.0, 0.0, 0.0, &[]);
    let analog = impulse_for(2.0, 0.0, 0.0, &[]);
    let mixed = impulse_for(3.0, 0.0, 0.0, &[]);
    assert!((peak_index(&linear) as i64 - peak_index(&analog) as i64).abs() <= 1);
    for f in [100.0, 300.0, 1000.0] {
        assert!((db(impulse_mag(&linear, f, SR)) - db(impulse_mag(&analog, f, SR))).abs() < 0.5);
    }
    let (al, am) = (asymmetry(&linear), asymmetry(&mixed));
    println!("asymmetry linear {al:.2e} mixed {am:.3}");
    assert!(al < 1e-3 && am > 0.01);
    assert!((db(impulse_mag(&mixed, 300.0, SR)) - 6.0).abs() < 0.6 && db(impulse_mag(&mixed, 12000.0, SR)).abs() < 0.6);
    let left = impulse_for(3.0, 0.0, 3.0, &[]);
    assert!(asymmetry(&left) > 0.01 && (db(impulse_mag(&left, 300.0, SR)) - 6.0).abs() < 0.6);
}

#[test]
fn analog_zl_free_phase_and_window() {
    let m = module_with(SR, &bell(300.0, 6.0, 1.0, 0.0), &[("processingMode", 4.0)]);
    assert_eq!(m.latency(), 0);
    let zl = impulse_for(4.0, 0.0, 0.0, &[]);
    let lin = impulse_for(1.0, 0.0, 0.0, &[]);
    println!("zl peak {} linear peak {}", peak_index(&zl), peak_index(&lin));
    assert!(peak_index(&zl) < 512 && peak_index(&lin) > 900 && peak_index(&lin) < 1200);
    assert!((db(impulse_mag(&zl, 300.0, SR)) - 6.0).abs() < 0.8 && (db(impulse_mag(&lin, 300.0, SR)) - 6.0).abs() < 0.8);

    let min = impulse_for(1.0, 100.0, 0.0, &[]);
    let inv = impulse_for(1.0, -100.0, 0.0, &[]);
    let (al, amin, ainv) = (asymmetry(&lin), asymmetry(&min), asymmetry(&inv));
    println!("asymmetry linear {al:.2e} min {amin:.3} inv {ainv:.3}");
    assert!(al < 1e-3 && amin > 0.1 && ainv > 0.1);
    let energy = |h: &[f32]| h.iter().map(|&v| v as f64 * v as f64).sum::<f64>();
    assert!((energy(&min) - energy(&lin)).abs() < 0.05 * energy(&lin));
    assert!((energy(&inv) - energy(&lin)).abs() < 0.05 * energy(&lin));
    let bias = |h: &[f32]| {
        let p = peak_index(h);
        let (mut before, mut after) = (0.0, 0.0);
        for k in 1..=1000 {
            if p >= k {
                before += (h[p - k] as f64).powi(2);
            }
            if p + k < h.len() {
                after += (h[p + k] as f64).powi(2);
            }
        }
        if before > 1e-12 {
            after / before
        } else {
            0.0
        }
    };
    assert!(bias(&inv) > bias(&min));

    let kaiser = impulse_for(1.0, 0.0, 0.0, &[("windowShape", 1.0), ("windowParameter", 100.0)]);
    assert!((db(impulse_mag(&kaiser, 300.0, SR)) - 6.0).abs() < 0.8);
    for w in 0..6 {
        let h = impulse_for(1.0, 0.0, 0.0, &[("windowShape", w as f32), ("windowParameter", 50.0)]);
        assert!(h.iter().all(|v| v.is_finite()) && (db(impulse_mag(&h, 300.0, SR)) - 6.0).abs() < 0.9, "window {w}");
    }
}

#[test]
fn same_length_edits_crossfade() {
    let max_step = |x: &[f32]| x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let mut m = lp(&bell(1000.0, 0.0, 1.0, 0.0), &[]);
    let mut phase = 0;
    let mut render = |m: &mut eq_parametric::ParametricEq, n: usize| {
        let (mut a, mut b) = (tone(n, 1000.0, 0.2, phase, SR), tone(n, 1000.0, 0.2, phase, SR));
        phase += n;
        run(m, &mut a, &mut b, 512);
        a
    };
    settle_kernel(&mut m);
    let settled = render(&mut m, 16384);
    let quiet = rms(&settled[14336..]);
    let mut prev = *settled.last().unwrap();
    let mut worst = max_step(&settled[14336..]);
    let before = m.kernel_designs();
    set(&mut m, "b1Gain", 12.0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while m.kernel_designs() == before && std::time::Instant::now() < deadline {
        let b = render(&mut m, 512);
        worst = worst.max((b[0] - prev).abs()).max(max_step(&b));
        prev = *b.last().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let hand = render(&mut m, 2048 + 512 + 1024);
    worst = worst.max((hand[0] - prev).abs()).max(max_step(&hand));
    prev = *hand.last().unwrap();
    let loud = render(&mut m, 4096);
    worst = worst.max((loud[0] - prev).abs()).max(max_step(&loud));
    let loud_rms = rms(&loud[2048..]);
    let loud_step = max_step(&loud[2048..]);
    println!("quiet {quiet:.4} loud {loud_rms:.4} transition step {worst:.4} loud step {loud_step:.4}");
    assert!(loud_rms > quiet * 2.0 && worst < (loud_step * 3.0).max(0.05));
    set(&mut m, "linearPhaseQuality", 1.0);
    assert_eq!(m.latency(), 2048);
}
