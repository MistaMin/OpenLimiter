//! Ported from OpenEQ 1.0.1 `checkResonanceSuppressor`: reconstruction, level independence,
//! controls, focus, stereo, sidechain, timing and delta. Same signals and limits.

use olm_api::Module;
use reso_suppress::{engine, ResonanceSuppressor, NPARAMS, PARAMS};

const SR: f64 = 48000.0;
const LEN: usize = 48000 * 4;

fn noise(slope: f32, level_db: f32, seed: u64) -> Vec<f32> {
    // The linear congruential generator OpenEQ's test used (48-bit, multiplier 0x5DEECE66D).
    let mut state = seed as i64;
    let mut x: Vec<f32> = (0..LEN)
        .map(|_| {
            state = ((state as u64).wrapping_mul(0x5DEECE66D).wrapping_add(11) & 0xFFFF_FFFF_FFFF) as i64;
            let int = (state >> 16) as i32;
            let r = ((int as u32) as f64 / (u32::MAX as f64 + 1.0)) as f32;
            r.min(1.0 - f32::EPSILON / 2.0) * 2.0 - 1.0
        })
        .collect();
    if slope < -1.0 {
        let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
        for v in x.iter_mut() {
            let w = *v;
            b0 = 0.99886 * b0 + w * 0.0555179;
            b1 = 0.99332 * b1 + w * 0.0750759;
            b2 = 0.96900 * b2 + w * 0.1538520;
            b3 = 0.86650 * b3 + w * 0.3104856;
            b4 = 0.55000 * b4 + w * 0.5329522;
            b5 = -0.7616 * b5 - w * 0.0168980;
            *v = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362) * 0.11;
            b6 = w * 0.115926;
        }
    }
    let rms = (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / LEN as f64).sqrt();
    let k = 10f64.powf(level_db as f64 / 20.0) / rms;
    x.iter().map(|&v| (v as f64 * k) as f32).collect()
}

fn add_resonance(x: &mut [f32], f: f64, gain_db: f64, q: f64) {
    let a = 10f64.powf(gain_db / 40.0);
    let w = 2.0 * std::f64::consts::PI * f / SR;
    let alpha = w.sin() / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = (1.0 + alpha * a, -2.0 * w.cos(), 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * w.cos(), 1.0 - alpha / a);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for v in x.iter_mut() {
        let y = (b0 * *v as f64 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0;
        x2 = x1;
        x1 = *v as f64;
        y2 = y1;
        y1 = y;
        *v = y as f32;
    }
}

fn band_level(x: &[f32], f: f64, delay: usize) -> f64 {
    let lo = f * 2f64.powf(-1.0 / 6.0);
    let hi = f * 2f64.powf(1.0 / 6.0);
    let start = LEN / 2;
    let mut power = 0.0;
    let mut g = lo;
    while g <= hi {
        let w = 2.0 * std::f64::consts::PI * g / SR;
        let (mut re, mut im) = (0.0, 0.0);
        let (c, s) = (w.cos(), w.sin());
        let (mut pr, mut pi) = ((w * start as f64).cos(), (w * start as f64).sin());
        for i in start..LEN - 4096 {
            let v = x[(i + delay).min(LEN - 1)] as f64;
            re += v * pr;
            im += v * pi;
            let t = pr * c - pi * s;
            pi = pr * s + pi * c;
            pr = t;
        }
        power += re * re + im * im;
        g += (hi - lo) / 12.0;
    }
    10.0 * (power + 1e-30).log10()
}

fn module(settings: &[(&str, f32)]) -> ResonanceSuppressor {
    let mut m = ResonanceSuppressor::default();
    for (id, v) in settings {
        m.set_param(ResonanceSuppressor::param_index(id).unwrap_or_else(|| panic!("{id}")), *v);
    }
    m.prepare(SR as f32, 512);
    m
}

fn run_stereo(l: &mut [f32], r: &mut [f32], settings: &[(&str, f32)], key: Option<(&[f32], &[f32])>) {
    let mut m = module(settings);
    let mut pos = 0;
    while pos < LEN {
        let e = (pos + 512).min(LEN);
        match key {
            Some((a, b)) => m.process_keyed(&mut l[pos..e], &mut r[pos..e], &a[pos..e], &b[pos..e]),
            None => m.process(&mut l[pos..e], &mut r[pos..e]),
        }
        pos = e;
    }
}

fn run(x: &[f32], settings: &[(&str, f32)]) -> Vec<f32> {
    let (mut l, mut r) = (x.to_vec(), x.to_vec());
    run_stereo(&mut l, &mut r, settings, None);
    l
}

const ON: (&str, f32) = ("depth", 80.0);

fn reduction_at(level: f32, gain: f64, q: f64, extra: &[(&str, f32)], probe: f64, peak: f64) -> f64 {
    let mut dry = noise(-3.0, level, 5);
    add_resonance(&mut dry, peak, gain, q);
    let mut s = vec![ON];
    s.extend_from_slice(extra);
    let wet = run(&dry, &s);
    band_level(&wet, probe, 2047) - band_level(&dry, probe, 0)
}

#[test]
fn latency_and_reconstruction() {
    let m = module(&[]);
    assert_eq!(m.latency(), 2047);
    let dry = noise(-3.0, -20.0, 3);
    let wet = run(&dry, &[]);
    let (mut err, mut refe) = (0.0, 0.0);
    for i in 4096..LEN - 2047 {
        let d = wet[i + 2047] as f64 - dry[i] as f64;
        err += d * d;
        refe += dry[i] as f64 * dry[i] as f64;
    }
    let snr = 10.0 * (refe / (err + 1e-30)).log10();
    println!("unity snr {snr:.1} dB");
    assert!(snr > 80.0);
}

#[test]
fn cuts_a_peak_independent_of_level() {
    let peak = reduction_at(-20.0, 12.0, 8.0, &[], 3000.0, 3000.0);
    let off = reduction_at(-20.0, 12.0, 8.0, &[], 600.0, 3000.0);
    let quiet = reduction_at(-45.0, 12.0, 8.0, &[], 3000.0, 3000.0);
    println!("peak {peak:.2} off {off:.2} quiet {quiet:.2}");
    assert!(peak < -4.0 && peak > -12.5);
    assert!(off.abs() < 0.6);
    assert!((quiet - peak).abs() < 0.5);
    for slope in [0.0f32, -3.0] {
        let dry = noise(slope, -20.0, 9);
        let wet = run(&dry, &[ON]);
        let worst = [100.0, 400.0, 1000.0, 3000.0, 8000.0].iter().map(|&f| (band_level(&wet, f, 2047) - band_level(&dry, f, 0)).abs()).fold(0.0, f64::max);
        println!("plain noise slope {slope}: worst {worst:.2}");
        assert!(worst < 1.0);
    }
}

#[test]
fn controls() {
    let peak = reduction_at(-20.0, 12.0, 8.0, &[], 3000.0, 3000.0);
    let low_depth = reduction_at(-20.0, 12.0, 8.0, &[("depth", 30.0)], 3000.0, 3000.0);
    let hard = reduction_at(-20.0, 12.0, 8.0, &[("depth", 100.0), ("hard", 1.0)], 3000.0, 3000.0);
    let soft = reduction_at(-20.0, 12.0, 8.0, &[("depth", 100.0)], 3000.0, 3000.0);
    let picky = reduction_at(-20.0, 5.0, 8.0, &[("selectivity", 100.0)], 3000.0, 3000.0);
    let open = reduction_at(-20.0, 5.0, 8.0, &[("selectivity", 0.0)], 3000.0, 3000.0);
    let outside = reduction_at(-20.0, 12.0, 8.0, &[("centre", 300.0), ("width", 1.0)], 3000.0, 3000.0);
    let half_mix = reduction_at(-20.0, 12.0, 8.0, &[("mix", 50.0)], 3000.0, 3000.0);
    let sharp = reduction_at(-20.0, 12.0, 30.0, &[("sharpness", 100.0)], 3000.0, 3000.0);
    let broad = reduction_at(-20.0, 12.0, 30.0, &[("sharpness", 0.0)], 3000.0, 3000.0);
    println!("depth30 {low_depth:.2} soft {soft:.2} hard {hard:.2} picky {picky:.2} open {open:.2} outside {outside:.2} mix {half_mix:.2} sharp {sharp:.2} broad {broad:.2}");
    assert!(low_depth > peak + 1.0);
    assert!(hard < soft - 2.0);
    assert!(picky > -0.5 && open < picky - 0.5);
    assert!(outside.abs() < 0.3);
    assert!(half_mix > peak + 1.0 && half_mix < -1.0);
    assert!(sharp < broad - 1.0);
}

#[test]
fn delta_plus_output_is_the_input() {
    let mut dry = noise(-3.0, -20.0, 5);
    add_resonance(&mut dry, 3000.0, 12.0, 8.0);
    let wet = run(&dry, &[ON]);
    let removed = run(&dry, &[ON, ("delta", 1.0)]);
    let (mut err, mut refe) = (0.0, 0.0);
    for i in 4096..LEN - 2047 {
        let d = wet[i + 2047] as f64 + removed[i + 2047] as f64 - dry[i] as f64;
        err += d * d;
        refe += dry[i] as f64 * dry[i] as f64;
    }
    let snr = 10.0 * (refe / (err + 1e-30)).log10();
    println!("delta sum snr {snr:.1}");
    assert!(snr > 70.0);
}

#[test]
fn focus_regions() {
    let peak = reduction_at(-20.0, 12.0, 8.0, &[], 3000.0, 3000.0);
    let more = reduction_at(-20.0, 12.0, 8.0, &[("focus1Amount", 100.0), ("focus1Freq", 3000.0), ("focus1Q", 1.0)], 3000.0, 3000.0);
    let less = reduction_at(-20.0, 12.0, 8.0, &[("focus1Amount", -100.0), ("focus1Freq", 3000.0), ("focus1Q", 1.0)], 3000.0, 3000.0);
    let away = reduction_at(-20.0, 12.0, 8.0, &[("focus1Amount", 100.0), ("focus1Freq", 3000.0)], 300.0, 300.0) - reduction_at(-20.0, 12.0, 8.0, &[], 300.0, 300.0);
    println!("focus +1 {more:.2} -1 {less:.2} plain {peak:.2} away {away:.2}");
    assert!(more < peak - 2.0 && less > peak + 2.0 && away.abs() < 0.3);
}

#[test]
fn stereo_link_and_mid_side() {
    let mut resonant = noise(-3.0, -20.0, 5);
    add_resonance(&mut resonant, 3000.0, 12.0, 8.0);
    let plain = noise(-3.0, -20.0, 11);
    let (mut l, mut r) = (resonant.clone(), plain.clone());
    run_stereo(&mut l, &mut r, &[ON, ("link", 0.0)], None);
    let ul = band_level(&l, 3000.0, 2047) - band_level(&resonant, 3000.0, 0);
    let ur = band_level(&r, 3000.0, 2047) - band_level(&plain, 3000.0, 0);
    let (mut l, mut r) = (resonant.clone(), plain.clone());
    run_stereo(&mut l, &mut r, &[ON], None);
    let lr = band_level(&r, 3000.0, 2047) - band_level(&plain, 3000.0, 0);
    let mut ml: Vec<f32> = resonant.iter().zip(&plain).map(|(a, b)| a + b).collect();
    let mut mr: Vec<f32> = resonant.iter().zip(&plain).map(|(a, b)| a - b).collect();
    run_stereo(&mut ml, &mut mr, &[ON, ("midSide", 1.0), ("link", 0.0)], None);
    let mid: Vec<f32> = ml.iter().zip(&mr).map(|(a, b)| 0.5 * (a + b)).collect();
    let side: Vec<f32> = ml.iter().zip(&mr).map(|(a, b)| 0.5 * (a - b)).collect();
    let mc = band_level(&mid, 3000.0, 2047) - band_level(&resonant, 3000.0, 0);
    let sc = band_level(&side, 3000.0, 2047) - band_level(&plain, 3000.0, 0);
    println!("unlinked L {ul:.2} R {ur:.2} linked R {lr:.2} mid {mc:.2} side {sc:.2}");
    assert!(ul < -3.0 && ur.abs() < 0.6 && lr < -3.0 && mc < -3.0 && sc.abs() < 0.6);
}

#[test]
fn sidechain_key() {
    let dry = noise(-3.0, -20.0, 5);
    let mut key = noise(-3.0, -20.0, 21);
    let plain_key = key.clone();
    add_resonance(&mut key, 2000.0, 15.0, 6.0);
    let keyed = |k: &[f32], sidechain: f32| {
        let (mut l, mut r) = (dry.clone(), dry.clone());
        run_stereo(&mut l, &mut r, &[ON, ("sidechain", sidechain)], Some((k, k)));
        l
    };
    let a = keyed(&key, 1.0);
    let b = keyed(&plain_key, 1.0);
    let at = band_level(&a, 2000.0, 2047) - band_level(&dry, 2000.0, 0);
    let away = band_level(&a, 500.0, 2047) - band_level(&dry, 500.0, 0);
    let plain_cut = band_level(&b, 2000.0, 2047) - band_level(&dry, 2000.0, 0);
    // Switch off: the key is ignored, the plain input has nothing to cut.
    let c = keyed(&key, 0.0);
    let ignored = band_level(&c, 2000.0, 2047) - band_level(&dry, 2000.0, 0);
    println!("key {at:.2} away {away:.2} plain key {plain_cut:.2} switch off {ignored:.2}");
    assert!(at < -3.0 && away.abs() < 0.6 && plain_cut.abs() < 0.8 && ignored.abs() < 0.8);
}

#[test]
fn attack_and_release_timing() {
    let mut gated = noise(-3.0, -30.0, 5);
    for (i, v) in gated.iter_mut().enumerate().take(120000).skip(48000) {
        *v += 0.1 * (2.0 * std::f64::consts::PI * 3000.0 * i as f64 / SR).sin() as f32;
    }
    let mut band = 0;
    for b in 0..engine::DISPLAY_BANDS {
        if (engine::display_band_hz(b) - 3000.0).abs() < (engine::display_band_hz(band) - 3000.0).abs() {
            band = b;
        }
    }
    let time_to = |settings: &[(&str, f32)], from: usize, rising: bool| {
        let mut s = vec![ON];
        s.extend_from_slice(settings);
        let mut m = module(&s);
        let (mut l, mut r) = (gated.clone(), gated.clone());
        let mut trace = Vec::new();
        let mut pos = 0;
        while pos < LEN {
            m.process(&mut l[pos..pos + 64], &mut r[pos..pos + 64]);
            trace.push(m.reduction_db(0, band).max(m.reduction_db(1, band)));
            pos += 64;
        }
        let full = trace[100000 / 64..118000 / 64].iter().fold(0.0f32, |a, &b| a.max(b));
        for (i, &t) in trace.iter().enumerate().skip(from / 64) {
            if if rising { t >= 0.5 * full } else { t <= 0.5 * full } {
                return (i * 64 - from) as f64 * 1000.0 / SR;
            }
        }
        1e9
    };
    let fa = time_to(&[("attack", 1.0)], 48000, true);
    let sa = time_to(&[("attack", 500.0)], 48000, true);
    let fr = time_to(&[("release", 20.0)], 120000, false);
    let sr = time_to(&[("release", 2000.0)], 120000, false);
    println!("attack fast {fa:.0} slow {sa:.0} ms, release fast {fr:.0} slow {sr:.0} ms");
    assert!(fa < 80.0 && sa > fa + 150.0 && sa < 1200.0 && fr < 150.0 && sr > fr + 600.0);
}

#[test]
fn view_and_meters() {
    let mut dry = noise(-3.0, -20.0, 5);
    add_resonance(&mut dry, 3000.0, 12.0, 8.0);
    let mut m = module(&[ON]);
    let (mut l, mut r) = (dry.clone(), dry.clone());
    for k in 0..LEN / 512 {
        m.process(&mut l[k * 512..k * 512 + 512], &mut r[k * 512..k * 512 + 512]);
    }
    let mut out = [0.0f32; reso_suppress::view::LEN];
    assert_eq!(m.view(&mut out), reso_suppress::view::LEN);
    let (deep, hz) = (out[reso_suppress::view::DEEPEST_DB], out[reso_suppress::view::DEEPEST_HZ]);
    println!("deepest {deep:.2} dB at {hz:.0} Hz, meter {:.2}", m.meters().reduction_db);
    assert!(deep > 3.0 && (hz / 3000.0).log2().abs() < 0.3);
    assert!((m.meters().reduction_db - deep).abs() < 1e-6);
}

#[test]
fn every_parameter_extreme_stays_bounded() {
    let x = noise(0.0, -20.0, 77);
    for i in 0..NPARAMS {
        for v in [PARAMS[i].min, PARAMS[i].max, PARAMS[i].default] {
            let mut m = ResonanceSuppressor::default();
            m.set_param(0, 100.0);
            m.set_param(i, v);
            m.prepare(48000.0, 512);
            let (mut l, mut r) = (x[..24000].to_vec(), x[24000..48000].to_vec());
            for k in 0..24000 / 500 {
                m.process(&mut l[k * 500..k * 500 + 500], &mut r[k * 500..k * 500 + 500]);
            }
            assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 100.0), "{} = {v}", PARAMS[i].id);
        }
    }
    let mut ids: Vec<&str> = PARAMS.iter().map(|p| p.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), NPARAMS);
    for p in PARAMS.iter() {
        assert!(p.default >= p.min && p.default <= p.max, "{}", p.id);
    }
}
