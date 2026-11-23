//! Ported from OpenEQ 1.0.1 checkSplitMode (transient/tonal split) and its latency-switch section
//! (drive, kernel modes, split: no clicks, latency back where it was).

mod common;
use common::*;
use olm_api::Module;

const LEN: usize = 24000;
const CLICK_A: usize = 9000;
const CLICK_B: usize = 15000;

fn input(tone_on: bool, clicks: bool) -> (Vec<f32>, Vec<f32>) {
    let mut l = if tone_on { tone(LEN, 1000.0, 0.25, 0, SR) } else { vec![0.0; LEN] };
    if clicks {
        for at in [CLICK_A, CLICK_B] {
            l[at] += 0.8;
        }
    }
    (l.clone(), l)
}

fn split_module(gain: f32, trans: f32, tail: f32, solo: f32) -> eq_parametric::ParametricEq {
    let mut b = bell(1000.0, gain, 1.0, 0.0);
    b.extend_from_slice(&[("b1Split", 1.0), ("b1SplitTrans", trans), ("b1SplitTail", tail)]);
    module_with(SR, &b, &[("splitSolo", solo)])
}

fn render(m: &mut eq_parametric::ParametricEq, tone_on: bool, clicks: bool) -> Vec<f32> {
    let (mut l, mut r) = input(tone_on, clicks);
    run(m, &mut l, &mut r, LEN);
    l
}

#[test]
fn split_latency_and_reconstruction() {
    let mut m = split_module(0.0, 0.0, 0.0, 0.0);
    let lat = m.latency();
    println!("split latency {lat}");
    assert_eq!(lat, 1791);
    let out = render(&mut m, true, true);
    let (dry, _) = input(true, true);
    let e = (0..LEN - lat).map(|i| (out[i + lat] - dry[i]).abs()).fold(0.0f32, f32::max);
    println!("reconstruction max {e:e}");
    assert!(e < 1e-5);
    set(&mut m, "b1Split", 0.0);
    assert_eq!(m.latency(), 0);
}

#[test]
fn split_null_against_plain_band() {
    let mut plain = module_with(SR, &bell(1000.0, 6.0, 1.0, 0.0), &[]);
    let mut split = split_module(6.0, 0.0, 0.0, 0.0);
    let a = render(&mut plain, true, true);
    let b = render(&mut split, true, true);
    let e = (0..LEN - 1791).map(|i| (a[i] - b[i + 1791]).abs()).fold(0.0f32, f32::max);
    println!("split null max {e:e}");
    assert!(e < 2e-4);
}

#[test]
fn split_separates_and_stream_gains_act() {
    let tone_rms = 0.25 / 2f64.sqrt();
    let lat = 1791;
    let peak_around = |x: &[f32], c: usize| x[c - 8..c + 8].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    let ts = render(&mut split_module(0.0, 0.0, 0.0, 1.0), true, true);
    let ss = render(&mut split_module(0.0, 0.0, 0.0, 2.0), true, true);
    let tone_in_t = rms(&ts[11000 + lat..13000 + lat]);
    let tone_in_s = rms(&ss[11000 + lat..13000 + lat]);
    let click_t = peak_around(&ts, CLICK_A + lat);
    let click_s = peak_around(&ss, CLICK_A + lat);
    println!("tone in transient {:.1} dB, in tonal {:.2} dB, click {click_t:.3} / {click_s:.3}", db(tone_in_t / tone_rms), db(tone_in_s / tone_rms));
    assert!(tone_in_t < tone_rms * 0.03 && db(tone_in_s / tone_rms).abs() < 0.3 && click_t > 0.5 && click_s < 0.45);

    let boosted = render(&mut split_module(0.0, 12.0, -12.0, 0.0), false, true);
    let cut = render(&mut split_module(0.0, -12.0, 12.0, 0.0), false, true);
    let cb = rms(&boosted[CLICK_A + lat - 256..CLICK_A + lat + 256]);
    let cc = rms(&cut[CLICK_A + lat - 256..CLICK_A + lat + 256]);
    let tu = rms(&render(&mut split_module(0.0, 12.0, -12.0, 0.0), true, true)[11000 + lat..13000 + lat]);
    let tb = rms(&render(&mut split_module(0.0, -12.0, 12.0, 0.0), true, true)[11000 + lat..13000 + lat]);
    println!("click {cb:.4}/{cc:.4} tone {:.2}/{:.2} dB", db(tu / tone_rms), db(tb / tone_rms));
    assert!(cb > cc * 1.25 && (db(tu / tone_rms) + 12.0).abs() < 1.0 && (db(tb / tone_rms) - 12.0).abs() < 1.0);
}

#[test]
fn split_out_of_band_and_side() {
    let mut b = bell(1000.0, 0.0, 2.0, 0.0);
    b.extend_from_slice(&[("b1Split", 1.0), ("b1SplitTrans", 12.0), ("b1SplitTail", -12.0)]);
    let mut m = module_with(SR, &b, &[]);
    let x = tone(12000, 100.0, 0.3, 40, SR);
    let (mut l, mut r) = (x.clone(), x.clone());
    run(&mut m, &mut l, &mut r, 512);
    let d = db(rms(&l[6000..10800]) / rms(&x[6000..10800])).abs();
    println!("out of band delta {d:.3} dB");
    assert!(d < 0.1);

    let mut b = bell(1000.0, 0.0, 1.0, 2.0);
    b.push(("b1Split", 1.0));
    let mut m = module_with(SR, &b, &[]);
    let (l0, mut r0) = input(true, true);
    for v in r0.iter_mut() {
        *v *= 0.5;
    }
    let (mut l, mut r) = (l0.clone(), r0.clone());
    run(&mut m, &mut l, &mut r, 512);
    let e = (0..LEN - 1791).map(|i| (l[i + 1791] - l0[i]).abs().max((r[i + 1791] - r0[i]).abs())).fold(0.0f32, f32::max);
    assert!(e < 1e-5, "side split reconstruction {e}");
}

/// Streams a 1 kHz tone in 256-sample blocks; returns the largest step between neighbours.
fn stream(m: &mut eq_parametric::ParametricEq, blocks: usize, phase: &mut usize, last: &mut f32) -> f64 {
    let mut step = 0.0f64;
    for _ in 0..blocks {
        let (mut a, mut b) = (tone(256, 1000.0, 0.25, *phase, SR), tone(256, 1000.0, 0.25, *phase, SR));
        *phase += 256;
        m.process(&mut a, &mut b);
        for &v in &a {
            step = step.max((v - *last).abs() as f64);
            *last = v;
        }
    }
    step
}

#[test]
fn latency_changes_do_not_click() {
    let switches: [(&str, f32, f32); 5] = [("b1Split", 1.0, 0.0), ("drive", 30.0, 0.0), ("processingMode", 1.0, 0.0), ("processingMode", 4.0, 0.0), ("oversample2x", 1.0, 0.0)];
    for (id, on, off) in switches {
        let mut m = module_with(SR, &bell(1000.0, 3.0, 1.0, 0.0), &[]);
        let (mut phase, mut last) = (0usize, 0.0f32);
        let tone_step = stream(&mut m, 40, &mut phase, &mut last);
        let before = m.latency();
        set(&mut m, id, on);
        let on_step = stream(&mut m, 200, &mut phase, &mut last);
        let lat_on = m.latency();
        set(&mut m, id, off);
        let off_step = stream(&mut m, 200, &mut phase, &mut last);
        println!("{id}: latency {before} -> {lat_on} -> {} steps tone {tone_step:.3} on {on_step:.3} off {off_step:.3}", m.latency());
        assert_eq!(m.latency(), before);
        assert!(on_step < 0.08 && off_step < 0.08, "{id} clicked");
    }
}
