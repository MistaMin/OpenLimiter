//! Ported from OpenEQ 1.0.1 `Tests/RegressionTests.cpp` and `Tests/FilterResponseTests.cpp`
//! (filter shapes, cut slopes, type rules, placement matrix, Digital+, 117-bit path), plus
//! checks of the FFT and the half-band oversampler.

use eq_core::band::EqBand;
use eq_core::cx::C32;
use eq_core::design::{cut_order, slope_norm_for_cut_db};
use eq_core::fft::Fft;
use eq_core::oversample::{round_trip_latency, HalfBand};
use eq_core::response::*;
use eq_core::types::*;
use eq_core::FilterType as T;

const SR: f64 = 48000.0;

fn db(d: &DesignedBand, f: f64) -> f64 {
    20.0 * d.magnitude(f, SR).max(1e-15).log10()
}

fn enc(s: f32) -> i32 {
    encode_slope_norm(s)
}

#[test]
fn cut_order_map_and_inverse() {
    for (s, o) in [(0.0, 0.0), (0.25, 1.0), (0.5, 2.0), (0.75, 8.0), (1.0, 16.0)] {
        assert!((cut_order(s) - o).abs() < 1e-12);
    }
    for dbo in [6.0f32, 12.0, 24.0, 36.0, 48.0, 72.0, 96.0] {
        let s = slope_norm_for_cut_db(dbo) as f64;
        assert!((cut_order(s) * 6.0 - dbo as f64).abs() < 1e-4, "{dbo}");
    }
    let e = enc(0.5);
    assert!(is_encoded_slope(e) && (decode_slope_norm(e) - 0.5).abs() < 1e-4 && !is_encoded_slope(12));
}

/// checkCuts: every slope is -3 dB at its cutoff, attenuates the named end, keeps the passband,
/// and the processed impulse matches the model.
#[test]
fn cut_slopes() {
    for ty in [T::LowCut, T::HighCut] {
        for dbo in [6.0f32, 12.0, 24.0, 36.0, 48.0, 72.0, 96.0] {
            let slope = enc(slope_norm_for_cut_db(dbo));
            let d = design_band(ty, 1000.0, 0.0, 1.0, slope, SR);
            let low = ty == T::LowCut;
            let stop = d.magnitude(if low { 100.0 } else { 10000.0 }, SR);
            let pass = d.magnitude(if low { 10000.0 } else { 100.0 }, SR);
            assert!(stop < 0.11, "{ty:?} {dbo}: stop {stop}");
            assert!(pass > 0.98, "{ty:?} {dbo}: pass {pass}");
            assert!((d.magnitude(1000.0, SR) - 0.5f64.sqrt()).abs() < 0.01, "{ty:?} {dbo}: cutoff {}", d.magnitude(1000.0, SR));
            let mut band = EqBand::default();
            band.prepare(SR);
            band.apply_if_changed(ty, 1000.0, 0.0, 1.0, slope);
            let mut x = vec![0.0f32; 32768];
            x[0] = 1.0;
            band.process(&mut x, 0);
            for f in [700.0, 1000.0, 1500.0] {
                let w = 2.0 * std::f64::consts::PI * f / SR;
                let (mut re, mut im) = (0.0, 0.0);
                for (n, &v) in x.iter().enumerate() {
                    re += v as f64 * (w * n as f64).cos();
                    im -= v as f64 * (w * n as f64).sin();
                }
                assert!((re.hypot(im) - d.magnitude(f, SR)).abs() < 0.002);
            }
        }
    }
    // Legacy dB/oct channel: Butterworth with an exact -3 dB cutoff.
    for slope in [6, 12, 24, 36, 48, 72, 96] {
        let lp = design_band(T::HighCut, 1000.0, 0.0, 1.0, slope, SR);
        let hp = design_band(T::LowCut, 1000.0, 0.0, 1.0, slope, SR);
        assert!((lp.magnitude(1000.0, SR) - 0.5f64.sqrt()).abs() < 0.002);
        assert!((hp.magnitude(1000.0, SR) - 0.5f64.sqrt()).abs() < 0.002);
        for f in [700.0f64, 1500.0] {
            let ratio = (std::f64::consts::PI * f / SR).tan() / (std::f64::consts::PI * 1000.0 / SR).tan();
            let want = 1.0 / (1.0 + (1.0 / ratio).powf(2.0 * slope as f64 / 6.0)).sqrt();
            assert!((hp.magnitude(f, SR) - want).abs() < 0.002, "hp {slope} {f}");
            let want_lp = 1.0 / (1.0 + (f / 1000.0).powf(2.0 * slope as f64 / 6.0)).sqrt();
            assert!((lp.magnitude(f, SR) - want_lp).abs() < 0.03, "lp {slope} {f}");
        }
    }
}

/// FilterResponseTests: the retained measurement baselines.
#[test]
fn fidelity_baselines() {
    let s = enc(0.5);
    let lp = design_band(T::HighCut, 1000.0, 0.0, 1.0, s, SR);
    assert!(db(&lp, 100.0).abs() < 0.02 && (db(&lp, 1000.0) + 3.0).abs() < 0.1 && (db(&lp, 20000.0) + 53.1).abs() < 1.5);
    let bp = design_band(T::BandPass, 1000.0, 6.0, 1.0, s, SR);
    assert!(db(&bp, 1000.0).abs() < 0.1 && (db(&bp, 200.0) + 13.6).abs() < 1.5 && (db(&bp, 5000.0) + 13.6).abs() < 1.5);
    let notch = design_band(T::Notch, 1000.0, 0.0, 1.0, s, SR);
    assert!((db(&notch, 1010.5) + 67.2).abs() < 2.0 && (db(&notch, 976.0) + 52.6).abs() < 2.0);
    assert!(db(&notch, 100.0).abs() < 0.2 && db(&notch, 10000.0).abs() < 0.2);
    let flat = design_band(T::VtAtten, 1000.0, 0.0, 1.0, 12, SR);
    assert!(flat.stage_count == 1 && db(&flat, 1000.0).abs() < 0.01);
    let cut = design_band(T::VtAtten, 1000.0, -9.0, 1.0, 12, SR);
    assert!(db(&cut, 20.0).abs() < 0.2 && (db(&cut, 20000.0) + 9.69).abs() < 1.0);
    let tl = design_band(T::ToneLow, 1000.0, 6.0, 1.0, 12, SR);
    assert!((db(&tl, 20.0) - 6.0).abs() < 0.3 && db(&tl, 12000.0) < 5.0);
    let th = design_band(T::ToneHigh, 1000.0, 6.0, 1.0, 12, SR);
    assert!((db(&th, 20000.0) - 6.0).abs() < 0.5 && (db(&th, 487.0) - 0.94).abs() < 0.2 && db(&th, 20.0).abs() < 0.05);
    let legacy = design_band(T::HighCut, 1000.0, 0.0, 1.0, 12, SR);
    assert!((db(&legacy, 1000.0) + 3.0).abs() < 0.3);
}

/// The type checks, the steep-cut checks, and the shape checks of the roadmap and plan sections.
#[test]
fn type_rules_and_shapes() {
    for t in 0..FILTER_TYPE_COUNT {
        for g in [-12.0f32, 0.0, 12.0] {
            let d = design_band(T::from_index(t), 1000.0, g, 1.5, 24, SR);
            assert!(d.stage_count >= 1 && d.stage_count <= MAX_STAGES);
            for f in [20.0, 200.0, 1000.0, 8000.0, 20000.0] {
                let m = d.magnitude(f, SR);
                assert!(m.is_finite() && m >= 0.0);
            }
        }
    }
    assert_eq!(FILTER_TYPE_NAMES[13], "Steep HP");
    assert_eq!(FILTER_TYPE_NAMES[14], "Steep LP");
    assert!(!uses_q(T::ELowBell) && !uses_q(T::EHighBell) && !uses_q(T::Eq250LowShelf) && uses_q(T::Sword));
    assert!(uses_gain(T::VtBoost) && !uses_slope(T::EHighPass) && !uses_slope(T::SteepLowCut));
    assert!(is_all_pass(T::AllPassHigh) && !supported_in_linear_phase(T::AllPassHigh));
    assert!(supports_dynamics(T::VtBoost) && !supports_dynamics(T::EHighPass));
    assert!(!uses_q(T::SteepHighCut) && !uses_gain(T::SteepLowCut) && supported_in_linear_phase(T::SteepLowCut));

    let boost = design_band(T::VtBoost, 1000.0, -6.0, 1.0, 12, SR);
    assert!(db(&boost, 1000.0).abs() < 1e-3);
    let atten = design_band(T::VtAtten, 4000.0, 6.0, 1.0, 12, SR);
    assert!(db(&atten, 10000.0).abs() < 1e-3);
    let blue = design_band(T::BlueLow, 200.0, -6.0, 1.0, 12, SR);
    for f in [20.0, 200.0, 2000.0] {
        assert!(db(&blue, f) > -1e-3);
    }
    assert!((db(&blue, 200.0) - 0.43).abs() < 0.05);
    assert!(db(&design_band(T::VtBoost, 1000.0, 6.0, 1.0, 12, SR), 1000.0) > 5.5);

    let wrapped = |d: &DesignedBand, f: f64| {
        let p: f64 = d.raw[..d.stage_count].iter().map(|r| stage_phase(&normalize(r), f, SR)).sum();
        p.sin().atan2(p.cos())
    };
    let apl = design_band(T::AllPass, 1000.0, 0.0, 1.0, 12, SR);
    let aph = design_band(T::AllPassHigh, 1000.0, 0.0, 1.0, 12, SR);
    for f in [20.0, 1000.0, 15000.0] {
        assert!(db(&aph, f).abs() < 1e-3);
    }
    assert!(wrapped(&apl, 1000.0).abs() > 1.0 && wrapped(&aph, 1000.0).abs() < 0.2 && wrapped(&aph, 22000.0).abs() > 1.0);

    let sword = design_band(T::Sword, 1000.0, 6.0, 1.0, 12, SR);
    let bell2 = design_band(T::Bell, 1000.0, 6.0, 2.0, 12, SR);
    for k in 0..6 {
        assert!((sword.raw[0][k] - bell2.raw[0][k]).abs() < 1e-9);
    }

    let vt = design_band(T::VtLowShelf, 100.0, 6.0, 1.0, enc(0.0), SR);
    assert_eq!(vt.stage_count, 1);
    assert!((db(&vt, 20.0) - 5.98).abs() < 0.05 && db(&vt, 10000.0).abs() < 0.05);
    let vt_att = design_band(T::VtLowShelf, 100.0, 6.0, 1.0, enc(0.5), SR);
    assert!((db(&vt_att, 20.0) - 4.06).abs() < 0.05, "{}", db(&vt_att, 20.0));
    let vt_cut = design_band(T::VtLowShelf, 100.0, -6.0, 1.0, enc(0.0), SR);
    assert!(db(&vt_cut, 20.0).abs() < 1e-3);

    let tone_low = design_band(T::ToneLow, 100.0, 6.0, 1.0, 12, SR);
    let tone_high = design_band(T::ToneHigh, 1000.0, -12.0, 1.0, 12, SR);
    assert!(tone_low.stage_count == 2 && tone_high.stage_count == 2);
    assert!((db(&tone_low, 20.0) - 5.88).abs() < 0.1 && (db(&tone_low, 10000.0) + 2.08).abs() < 0.1);
    assert!((db(&tone_high, 10000.0) + 10.68).abs() < 0.15 && (db(&tone_high, 30.0) - 0.03).abs() < 0.1);

    let a = design_band(T::Eq250LowShelf, 300.0, 6.0, 3.0, 12, SR);
    let b = design_band(T::Eq250LowShelf, 300.0, 6.0, 0.5, 12, SR);
    assert!(a.stage_count == 3 && a.raw[..3] == b.raw[..3]);

    // Steep cuts: 15 sections, near unity at the frequency, deep stop band.
    let low = design_band(T::SteepLowCut, 1000.0, 0.0, 1.0, 12, SR);
    assert_eq!(low.stage_count, 15);
    assert!(db(&low, 1000.0).abs() < 2.0 && db(&low, 976.0) < -0.5 && db(&low, 976.0) > -4.0);
    assert!(db(&low, 2000.0) > -1.0 && db(&low, 500.0) < -120.0);
    let high = design_band(T::SteepHighCut, 1000.0, 0.0, 1.0, 12, SR);
    assert!(db(&high, 500.0) > -0.5 && db(&high, 2000.0) < -120.0);

    // Tilt, soft bell, flat tilt, flat top, narrow resonance.
    let tilt = design_band(T::TiltShelf, 1000.0, 6.0, 0.7, 12, SR);
    assert!(db(&tilt, 8000.0) > db(&tilt, 100.0) + 2.0);
    let soft = design_band(T::SoftBell, 1000.0, 12.0, 1.0, 12, SR);
    assert!((soft.q - 1.15).abs() < 0.02 && (db(&soft, 1000.0) - 12.0).abs() < 0.2);
    let ft = design_band(T::FlatTilt, 1000.0, 6.0, 0.7, 12, SR);
    assert!(db(&ft, 10000.0) > db(&ft, 80.0) + 2.0 && db(&ft, 1000.0).abs() < 0.4 && db(&ft, 80.0) < -1.5 && db(&ft, 10000.0) > 1.5);
    let flat_top = design_band(T::FlatTop, 1000.0, 12.0, 1.0, 12, SR);
    assert!(db(&flat_top, 1000.0) > 11.0);
    let narrow = design_band(T::NarrowResonance, 1000.0, 12.0, 1.0, 12, SR);
    let bell = design_band(T::Bell, 1000.0, 12.0, 1.0, 12, SR);
    assert!((db(&narrow, 1000.0) - 12.0).abs() < 0.3 && db(&narrow, 2000.0).abs() + 1.0 < db(&bell, 2000.0).abs());
}

/// Nyquist clamp and the frequency limits.
#[test]
fn nyquist_clamp() {
    assert!(clamp_frequency(20000.0, 22050.0) < 22050.0 * 0.49 + 0.05);
    let d = design_band(T::Bell, 20000.0, 18.0, 40.0, enc(0.0), 22050.0);
    for f in [100.0, 1000.0, 10000.0] {
        assert!(d.magnitude(f, 22050.0).is_finite());
    }
}

/// checkStereoPanAndMidSide: the placement matrix.
#[test]
fn placement_matrix() {
    let c = band_matrix(2.0, StereoMode::MidSide, 0.0);
    assert!((c.ll - 2.0).abs() < 1e-9 && c.lr.abs() < 1e-9);
    let full = band_matrix(2.0, StereoMode::Stereo, 1.0);
    assert!((full.ll - 1.0).abs() < 1e-9 && (full.rr - 2.0).abs() < 1e-9);
    let mid = band_matrix(4.0, StereoMode::Mid, 0.0);
    // A pure side input (1, -1) is untouched by a mid band.
    assert!(((mid.ll - mid.lr) - 1.0).abs() < 1e-12);
    let side = band_matrix(4.0, StereoMode::Side, 0.0);
    assert!(((side.ll + side.lr) - 1.0).abs() < 1e-12);
    let left = band_matrix(3.0, StereoMode::Left, 0.0);
    assert!(left.ll == 3.0 && left.rr == 1.0);
}

/// checkPhaseModes (Digital+): four all-pass steps bring the phase closer to a straight line.
#[test]
fn digital_plus_straightens_phase() {
    let dev = |steps: usize| {
        let mut band = EqBand::default();
        band.prepare(SR);
        band.set_digital_plus(steps);
        band.apply_if_changed(T::Bell, 1000.0, 6.0, 1.0, 12);
        let mut fr = [0.0f64; 5];
        let mut ph = [0.0f64; 5];
        for k in 0..5 {
            fr[k] = 1000.0 * (0.5 + 0.375 * k as f64);
            ph[k] = band.stages().iter().map(|c| stage_phase(c, fr[k], SR)).sum();
        }
        for k in 1..5 {
            while ph[k] - ph[k - 1] > std::f64::consts::PI {
                ph[k] -= 2.0 * std::f64::consts::PI;
            }
            while ph[k] - ph[k - 1] < -std::f64::consts::PI {
                ph[k] += 2.0 * std::f64::consts::PI;
            }
        }
        let (sf, sp) = (fr.iter().sum::<f64>(), ph.iter().sum::<f64>());
        let sff: f64 = fr.iter().map(|f| f * f).sum();
        let sfp: f64 = fr.iter().zip(ph.iter()).map(|(f, p)| f * p).sum();
        let slope = (5.0 * sfp - sf * sp) / (5.0 * sff - sf * sf);
        let icpt = (sp - slope * sf) / 5.0;
        (0..5).map(|k| (ph[k] - slope * fr[k] - icpt).powi(2)).sum::<f64>()
    };
    let (d0, d4) = (dev(0), dev(4));
    println!("digital+ deviation 0 steps {d0:.3e}, 4 steps {d4:.3e}");
    assert!(d4 < d0 * 0.9);
}

/// checkUltraPrecision: the double-double recursion agrees with the double path.
#[test]
fn ultra_precision_agrees() {
    for (ty, slope) in [(T::Bell, enc(0.0)), (T::LowCut, enc(0.5))] {
        let mut a = EqBand::default();
        let mut b = EqBand::default();
        a.prepare(SR);
        b.prepare(SR);
        a.apply_if_changed(ty, 60.0, 6.0, 6.0, slope);
        b.apply_if_changed(ty, 60.0, 6.0, 6.0, slope);
        b.set_ultra(true);
        let x: Vec<f32> = (0..20480).map(|i| (0.3 * (2.0 * std::f64::consts::PI * 137.0 * i as f64 / SR).sin()) as f32).collect();
        let (mut xa, mut xb) = (x.clone(), x.clone());
        a.process(&mut xa, 0);
        b.process(&mut xb, 0);
        let e = xa.iter().zip(&xb).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
        assert!(xb.iter().all(|v| v.is_finite()) && e < 1e-5, "{ty:?} {e}");
    }
}

/// A settled band stops redesigning, also for types whose designed Q differs from the control.
#[test]
fn settled_band_does_not_redesign() {
    for t in [T::Bell, T::SoftBell, T::NarrowResonance, T::FlatTop, T::VtAtten, T::Sword] {
        let mut b = EqBand::default();
        b.prepare(SR);
        assert!(b.apply_if_changed(t, 1000.0, 6.0, 1.0, enc(0.0)));
        assert!(!b.apply_if_changed(t, 1000.0, 6.0, 1.0, enc(0.0)), "{t:?} redesigned");
    }
}

#[test]
fn fft_round_trip() {
    let fft = Fft::new(10);
    let mut x: Vec<C32> = (0..1024).map(|i| C32::new((i as f32 * 0.37).sin(), (i as f32 * 0.11).cos())).collect();
    let orig = x.clone();
    fft.forward(&mut x);
    // A delta's spectrum is flat.
    let mut d = vec![C32::ZERO; 1024];
    d[0] = C32::new(1.0, 0.0);
    fft.forward(&mut d);
    assert!(d.iter().all(|v| (v.re - 1.0).abs() < 1e-6 && v.im.abs() < 1e-6));
    fft.inverse(&mut x);
    let e = x.iter().zip(&orig).map(|(a, b)| (a.re - b.re).abs().max((a.im - b.im).abs())).fold(0.0f32, f32::max);
    assert!(e < 1e-5);
}

/// 2x oversampler: flat pass band, rejected images, a measured latency.
#[test]
fn half_band_oversampler() {
    let lat = round_trip_latency();
    println!("half-band round trip latency {lat:.3} samples");
    assert!(lat > 0.5 && lat < 20.0);
    for (f, want_db, tol) in [(100.0, 0.0, 0.01), (1000.0, 0.0, 0.01), (10000.0, 0.0, 0.05), (18000.0, 0.0, 0.2)] {
        let mut h = HalfBand::new();
        let n = 48000;
        let (mut e_in, mut e_out) = (0.0, 0.0);
        for i in 0..n {
            let x = (2.0 * std::f64::consts::PI * f * i as f64 / SR).sin() as f32 * 0.5;
            let u = h.up(x);
            let y = h.down(u);
            if i > 4000 {
                e_in += (x as f64).powi(2);
                e_out += (y as f64).powi(2);
            }
        }
        let g = 10.0 * (e_out / e_in).log10();
        assert!((g - want_db).abs() < tol, "{f} Hz: {g} dB");
    }
    // An image: the upsampled stream of a 5 kHz tone carries little at 48k - 5k.
    let mut h = HalfBand::new();
    let mut up = Vec::new();
    for i in 0..8192 {
        let x = (2.0 * std::f64::consts::PI * 5000.0 * i as f64 / SR).sin() as f32;
        up.extend_from_slice(&h.up(x));
    }
    let tone = |f: f64| {
        let w = 2.0 * std::f64::consts::PI * f / (2.0 * SR);
        let (mut c, mut s) = (0.0, 0.0);
        for (i, &v) in up.iter().enumerate().skip(2000) {
            c += v as f64 * (w * i as f64).cos();
            s += v as f64 * (w * i as f64).sin();
        }
        (c * c + s * s).sqrt()
    };
    let ratio = 20.0 * (tone(43000.0) / tone(5000.0)).log10();
    println!("image rejection {ratio:.1} dB");
    assert!(ratio < -70.0);
}
