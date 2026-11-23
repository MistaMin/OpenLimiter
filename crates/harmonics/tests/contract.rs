//! The module contract: null, latency, DC, auto gain, routing, parameters, read-outs.

mod common;

use common::*;
use harmonics::*;
use olm_api::{Kind, Module};

const MODES: [usize; 5] = [AA_ADAA, AA_ADAA2, AA_OS2, AA_OS4, AA_NONE];

fn mode_name(aa: usize) -> &'static str {
    ["ADAA", "ADAA2", "2x", "4x", "none"][aa]
}

#[test]
fn parameter_table_is_sound() {
    let m = Harmonics::default();
    let p = m.params();
    assert_eq!(p.len(), NPARAMS);
    for (i, d) in p.iter().enumerate() {
        assert!(d.min < d.max && d.default >= d.min && d.default <= d.max, "{}", d.id);
        assert!(d.id.chars().next().unwrap().is_ascii_lowercase() && d.id.chars().all(|c| c.is_ascii_alphanumeric()), "{}", d.id);
        assert!(p.iter().skip(i + 1).all(|o| o.id != d.id), "duplicate id {}", d.id);
        assert_eq!(m.param(i), d.default, "{}", d.id);
        if let Kind::Choice(l) = d.kind {
            assert_eq!(d.steps as usize, l.len());
        }
    }
    assert_eq!(PARAMS[P_TRIM].max, 0.0, "output trim is down only");
    assert!(m.param(NPARAMS).is_nan());
}

#[test]
fn latency_per_setting_is_the_dry_delay() {
    println!("latency at 48 kHz:");
    for aa in MODES {
        // harmonics off: the output is the input delayed by exactly the reported latency
        let mut m = module(&[(P_ANTI_ALIAS, aa as f32), (P_H2, -80.0), (P_H3, -80.0)]);
        let mut x = vec![0.0f32; 256];
        x[10] = 1.0;
        let y = run_mono(&mut m, &x);
        let at = y.iter().position(|v| *v != 0.0).unwrap();
        assert_eq!(at - 10, m.latency(), "{}", mode_name(aa));
        assert_eq!(y[at], 1.0);
        assert!(y.iter().enumerate().all(|(i, v)| i == at || *v == 0.0));
        println!("  {:5} {:2} samples", mode_name(aa), m.latency());
        assert_eq!(m.latency(), latency_of(aa));
    }
    // other settings do not change it
    let mut m = Harmonics::default();
    for i in 0..NPARAMS {
        if i != P_ANTI_ALIAS {
            m.set_param(i, PARAMS[i].max);
        }
    }
    assert_eq!(m.latency(), 1);
}

#[test]
fn added_signal_lines_up_with_the_dry_signal() {
    // Heavy saturation: the added part carries most of the fundamental with the opposite sign.
    // If it were late or early, the output fundamental would be lifted at high frequencies.
    println!("saturator +18 dB, -12 dBFS: output fundamental gain vs the static curve (dB) and phase error (deg)");
    for aa in MODES {
        let mut line = format!("  {:5}", mode_name(aa));
        for f in [100.0, 1000.0, 5000.0, 10000.0] {
            let n = 16384;
            let (k, f) = bin_freq(f, n, SR);
            let amp = 0.25;
            let x = sine(8192 + n, f, amp, SR);
            let mut m = module(&[(P_FAMILY, SATURATOR as f32), (P_DRIVE, 18.0), (P_ANTI_ALIAS, aa as f32)]);
            let y = run_mono(&mut m, &x);
            let seg: Vec<f64> = y[8192..].iter().map(|v| *v as f64).collect();
            let (re, im) = spectrum(&seg);
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() * 2.0 / n as f64;
            let shape = m.shape();
            let want = {
                let h = curve_harmonics(&shape.curve(), shape.drive() * amp);
                h[1] / shape.drive()
            };
            // phase relative to the input sine delayed by the latency (input sin starts at 0)
            let w = 2.0 * std::f64::consts::PI * f / SR;
            let expect = -std::f64::consts::FRAC_PI_2 + w * (8192.0 - m.latency() as f64);
            let mut dph = (im[k].atan2(re[k]) - expect).to_degrees() % 360.0;
            if dph > 180.0 {
                dph -= 360.0;
            }
            if dph < -180.0 {
                dph += 360.0;
            }
            let gdb = 20.0 * (mag / want).log10();
            line += &format!(" | {f:7.0} Hz {gdb:+6.2} dB {dph:+6.2} deg");
            if f < 2000.0 {
                assert!(gdb.abs() < 0.1 && dph.abs() < 1.0, "{} {f}: {gdb} dB {dph} deg", mode_name(aa));
            }
            if aa == AA_OS2 || aa == AA_OS4 || aa == AA_NONE {
                assert!(gdb.abs() < 0.1 && dph.abs() < 0.5, "{} {f}: {gdb} dB {dph} deg", mode_name(aa));
            }
        }
        println!("{line}");
    }
}

#[test]
fn mix_zero_nulls_exactly() {
    for aa in MODES {
        for ag in [0.0, 1.0] {
            let mut m = module(&[(P_ANTI_ALIAS, aa as f32), (P_MIX, 0.0), (P_FAMILY, SATURATOR as f32), (P_DRIVE, 24.0), (P_H2, -12.0), (P_BIAS, 50.0), (P_AUTO_GAIN, ag), (P_CHANNELS, CH_MID_SIDE as f32)]);
            let l = noise(48000, 3, 0.9);
            let r = noise(48000, 4, 0.7);
            let (yl, yr) = run(&mut m, &l, &r, 333);
            let d = m.latency();
            for i in d..l.len() {
                assert_eq!(yl[i], l[i - d], "{} ag {ag} at {i}", mode_name(aa));
                assert_eq!(yr[i], r[i - d]);
            }
        }
        // all harmonics off on the polynomial curve is a null as well
        let mut m = module(&[(P_ANTI_ALIAS, aa as f32), (P_H2, -80.0), (P_H3, -80.0), (P_AUTO_GAIN, 1.0)]);
        let l = noise(24000, 5, 1.0);
        let (yl, _) = run(&mut m, &l, &l, 256);
        let d = m.latency();
        assert!((d..l.len()).all(|i| yl[i] == l[i - d]), "{}", mode_name(aa));
    }
}

#[test]
fn dc_is_blocked() {
    // strong even harmonics and bias make DC in the curve; the output carries none
    for (fam, bias) in [(POLYNOMIAL, 0.0f32), (SATURATOR, 100.0), (TAPE, -100.0)] {
        let mut m = module(&[(P_FAMILY, fam as f32), (P_H2, -12.0), (P_H4, -12.0), (P_BIAS, bias), (P_DRIVE, 12.0)]);
        let x = sine(96000, 440.0, 0.8, SR);
        let y = run_mono(&mut m, &x);
        let tail = &y[48000..];
        let mean = tail.iter().map(|v| *v as f64).sum::<f64>() / tail.len() as f64;
        // the curve's own DC for comparison
        let shape = m.shape();
        let h = curve_harmonics(&shape.curve(), shape.drive() * 0.8);
        let dc_curve = h[0] / shape.drive();
        println!("family {fam} bias {bias}: curve DC {dc_curve:.4}, output mean after 1 s {mean:.2e}");
        assert!(dc_curve > 1e-3);
        assert!(mean.abs() < 2e-5, "family {fam}: DC {mean}");
    }
}

fn kweighted_ms(x: &[f32]) -> f64 {
    let mut k = harmonics::filters::KWeight::default();
    k.set(SR);
    let v: Vec<f64> = x.iter().map(|s| k.run(*s as f64)).collect();
    let t = &v[v.len() / 2..];
    t.iter().map(|s| s * s).sum::<f64>() / t.len() as f64
}

#[test]
fn auto_gain_matches_loudness() {
    // a programme proxy: three tones and noise
    let n = 48000 * 12;
    let mut x = noise(n, 11, 0.15);
    for (i, v) in x.iter_mut().enumerate() {
        let t = i as f64 / SR;
        *v += (0.3 * (2.0 * std::f64::consts::PI * 110.0 * t).sin() + 0.2 * (2.0 * std::f64::consts::PI * 880.0 * t).sin() + 0.1 * (2.0 * std::f64::consts::PI * 3300.0 * t).sin()) as f32;
    }
    println!("loudness change, output vs input (K-weighted, second half of 12 s):");
    for (fam, drive, label) in [(SATURATOR, 12.0f32, "saturator +12 dB"), (TAPE, 18.0, "tape +18 dB"), (POLYNOMIAL, 6.0, "polynomial H2 -12 H3 -12, +6 dB")] {
        let mut set = vec![(P_FAMILY, fam as f32), (P_DRIVE, drive)];
        if fam == POLYNOMIAL {
            set.push((P_H2, -12.0));
            set.push((P_H3, -12.0));
        }
        let mut off = module(&set);
        set.push((P_AUTO_GAIN, 1.0));
        let mut on = module(&set);
        let y_off = run_mono(&mut off, &x);
        let y_on = run_mono(&mut on, &x);
        let lin = kweighted_ms(&x);
        let d_off = 10.0 * (kweighted_ms(&y_off) / lin).log10();
        let d_on = 10.0 * (kweighted_ms(&y_on) / lin).log10();
        let mut v = [0.0f32; view::LEN];
        on.view(&mut v);
        println!("  {label:34} auto gain off {d_off:+6.2} dB, on {d_on:+6.3} dB (applied {:+.2} dB)", v[view::AUTO_GAIN_DB]);
        assert!(d_off.abs() > 0.5, "{label}: the test needs a level change to correct");
        assert!(d_on.abs() < 0.1, "{label}: {d_on} dB");
        assert!((v[view::AUTO_GAIN_DB] as f64 + d_off).abs() < 0.3);
        assert!((on.meters().reduction_db - (-v[view::AUTO_GAIN_DB]).max(0.0)).abs() < 1e-6);
    }
    // silence holds the gain (gate) and the gain stays within its range
    let mut m = module(&[(P_FAMILY, SATURATOR as f32), (P_DRIVE, 24.0), (P_AUTO_GAIN, 1.0)]);
    let y = run_mono(&mut m, &vec![0.0; 48000]);
    assert!(y.iter().all(|v| *v == 0.0));
    let mut v = [0.0f32; view::LEN];
    m.view(&mut v);
    assert_eq!(v[view::AUTO_GAIN_DB], 0.0);
    let _ = run_mono(&mut m, &sine(48000 * 4, 200.0, 0.001, SR));
    m.view(&mut v);
    assert!(v[view::AUTO_GAIN_DB].abs() <= AG_RANGE_DB as f32 + 1e-3);
}

#[test]
fn trim_is_down_only_and_exact() {
    let mut m = module(&[(P_TRIM, -6.0), (P_H2, -80.0), (P_H3, -80.0), (P_ANTI_ALIAS, AA_NONE as f32)]);
    let x = noise(4800, 2, 0.5);
    let y = run_mono(&mut m, &x);
    let g = 10f32.powf(-6.0 / 20.0);
    assert!((0..4800).all(|i| (y[i] - g * x[i]).abs() < 1e-6));
    let mut m = module(&[(P_TRIM, 12.0)]);
    assert_eq!(m.param(P_TRIM), 0.0, "clamped");
    m.set_param(P_TRIM, -100.0);
    assert_eq!(m.param(P_TRIM), -24.0);
}

#[test]
fn listen_plays_exactly_what_is_added() {
    for aa in [AA_ADAA, AA_OS4] {
        let set = vec![(P_FAMILY, SATURATOR as f32), (P_DRIVE, 6.0), (P_H2, -20.0), (P_MIX, 70.0), (P_TRIM, -3.0), (P_ANTI_ALIAS, aa as f32)];
        let mut a = module(&set);
        let mut b = module(&[set.clone(), vec![(P_LISTEN, 1.0)]].concat());
        let l = noise(24000, 8, 0.6);
        let r = sine(24000, 300.0, 0.5, SR);
        let (al, ar) = run(&mut a, &l, &r, 256);
        let (bl, br) = run(&mut b, &l, &r, 256);
        let d = a.latency();
        let g = 10f64.powf(-3.0 / 20.0);
        for i in d..l.len() {
            assert!((al[i] as f64 - (g * l[i - d] as f64 + bl[i] as f64)).abs() < 2e-6);
            assert!((ar[i] as f64 - (g * r[i - d] as f64 + br[i] as f64)).abs() < 2e-6);
        }
    }
}

#[test]
fn channel_modes_route_as_labelled() {
    let n = 24000;
    let s = sine(n, 500.0, 0.7, SR);
    let neg: Vec<f32> = s.iter().map(|v| -v).collect();
    let set = |ch: usize| vec![(P_CHANNELS, ch as f32), (P_H2, -12.0), (P_H3, -12.0), (P_ANTI_ALIAS, AA_NONE as f32)];
    // Mid only: a pure side signal (L = -R) gets nothing added
    let mut m = module(&set(CH_MID));
    let (yl, yr) = run(&mut m, &s, &neg, 256);
    assert!((0..n).all(|i| yl[i] == s[i] && yr[i] == neg[i]));
    // Side only: a mono signal gets nothing added
    let mut m = module(&set(CH_SIDE));
    let (yl, yr) = run(&mut m, &s, &s, 256);
    assert!((0..n).all(|i| yl[i] == s[i] && yr[i] == s[i]));
    // Mid only on mono: both sides get the same harmonics; Side only on side: opposite
    let mut m = module(&set(CH_MID));
    let (yl, yr) = run(&mut m, &s, &s, 256);
    assert!((0..n).all(|i| yl[i] == yr[i]));
    assert!((1000..n).any(|i| yl[i] != s[i]));
    let mut m = module(&set(CH_SIDE));
    let (yl, yr) = run(&mut m, &s, &neg, 256);
    assert!((1000..n).all(|i| (yl[i] + yr[i]).abs() < 1e-6));
    assert!((1000..n).any(|i| yl[i] != s[i]));
    // Stereo: a signal on the left only leaves the right untouched
    let zero = vec![0.0f32; n];
    let mut m = module(&set(CH_STEREO));
    let (yl, yr) = run(&mut m, &s, &zero, 256);
    assert!(yr.iter().all(|v| *v == 0.0));
    assert!((1000..n).any(|i| yl[i] != s[i]));
    // Mid-Side on a mono signal equals Stereo on it (side is zero)
    let mut a = module(&set(CH_MID_SIDE));
    let mut b = module(&set(CH_STEREO));
    let (al, _) = run(&mut a, &s, &s, 256);
    let (bl, _) = run(&mut b, &s, &s, 256);
    assert!((0..n).all(|i| (al[i] - bl[i]).abs() < 1e-6));
}

fn harmonic_db(y: &[f32], k: usize, n: usize, order: usize) -> f64 {
    let seg: Vec<f64> = y[y.len() - n..].iter().map(|v| *v as f64).collect();
    let p = power(&seg);
    10.0 * (p[order * k] / p[k]).log10()
}

#[test]
fn band_filters_shape_the_added_harmonics() {
    let n = 16384;
    let (k, f) = bin_freq(500.0, n, SR);
    let x = sine(24000 + n, f, 1.0, SR);
    let base = vec![(P_H2, -20.0f32), (P_H4, -30.0), (P_ANTI_ALIAS, AA_OS4 as f32)];
    let mut m = module(&base);
    let y0 = run_mono(&mut m, &x);
    // harmonics low cut at 2 kHz: H2 (1 kHz) loses about 12 dB (2nd order, an octave below)
    let mut m = module(&[base.clone(), vec![(P_HARM_LOW, 2000.0)]].concat());
    let y1 = run_mono(&mut m, &x);
    let d2 = harmonic_db(&y1, k, n, 2) - harmonic_db(&y0, k, n, 2);
    let d4 = harmonic_db(&y1, k, n, 4) - harmonic_db(&y0, k, n, 4);
    println!("harmonics low cut 2 kHz: H2 {d2:+.2} dB, H4 {d4:+.2} dB");
    assert!((d2 + 12.3).abs() < 0.5 && (d4 + 3.0).abs() < 0.3, "{d2} {d4}");
    // harmonics high cut at 1 kHz: H4 (2 kHz) loses about 12 dB, H2 3 dB
    let mut m = module(&[base.clone(), vec![(P_HARM_HIGH, 1000.0)]].concat());
    let y2 = run_mono(&mut m, &x);
    let d2 = harmonic_db(&y2, k, n, 2) - harmonic_db(&y0, k, n, 2);
    let d4 = harmonic_db(&y2, k, n, 4) - harmonic_db(&y0, k, n, 4);
    println!("harmonics high cut 1 kHz: H2 {d2:+.2} dB, H4 {d4:+.2} dB");
    assert!((d2 + 3.0).abs() < 0.3 && (d4 + 12.3).abs() < 0.5, "{d2} {d4}");
    // source band: with a 5 kHz source low cut, a 500 Hz tone makes (almost) no harmonics,
    // but the dry tone passes untouched
    let mut m = module(&[base.clone(), vec![(P_SRC_LOW, 5000.0)]].concat());
    let y3 = run_mono(&mut m, &x);
    let h2 = harmonic_db(&y3, k, n, 2);
    let h2_0 = harmonic_db(&y0, k, n, 2);
    println!("source low cut 5 kHz: H2 {h2:.1} dB (without {h2_0:.1})");
    assert!(h2 < h2_0 - 60.0);
    let seg: Vec<f64> = y3[y3.len() - n..].iter().map(|v| *v as f64).collect();
    let p = power(&seg);
    assert!((10.0 * (p[k] / (n as f64 / 2.0).powi(2)).log10()).abs() < 0.01, "dry fundamental untouched");
}

#[test]
fn every_parameter_at_min_max_default_is_finite() {
    let x = {
        let mut v = noise(9600, 21, 0.8);
        for (i, s) in v.iter_mut().enumerate() {
            *s += (0.5 * (i as f64 * 0.37).sin()) as f32;
        }
        v
    };
    let mut count = 0;
    for sr in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        for i in 0..NPARAMS {
            for which in 0..3 {
                let d = PARAMS[i];
                let v = [d.min, d.max, d.default][which];
                for aa in MODES {
                    let mut m = Harmonics::default();
                    m.prepare(sr, 512);
                    m.set_param(P_ANTI_ALIAS, aa as f32);
                    m.set_param(i, v);
                    // and the rest at an extreme corner too
                    let (l, r) = run(&mut m, &x, &x.iter().map(|s| -0.5 * s).collect::<Vec<_>>(), 177);
                    assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 20.0), "{} = {v} at {sr} ({})", d.id, mode_name(aa));
                    let mut out = [0.0f32; view::LEN];
                    assert_eq!(m.view(&mut out), view::LEN);
                    assert!(out.iter().all(|s| s.is_finite()), "{} view", d.id);
                    count += 1;
                }
            }
        }
        // everything at max, then everything at min
        for corner in [true, false] {
            let mut m = Harmonics::default();
            m.prepare(sr, 512);
            for i in 0..NPARAMS {
                m.set_param(i, if corner { PARAMS[i].max } else { PARAMS[i].min });
            }
            for aa in MODES {
                m.set_param(P_ANTI_ALIAS, aa as f32);
                let (l, r) = run(&mut m, &x, &x, 64);
                assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 50.0), "corner {corner} {sr} {}", mode_name(aa));
            }
        }
    }
    // invalid values are ignored or clamped
    let mut m = Harmonics::default();
    m.set_param(P_DRIVE, f32::NAN);
    assert_eq!(m.param(P_DRIVE), 0.0);
    m.set_param(P_FAMILY, 99.0);
    assert_eq!(m.param(P_FAMILY), 2.0);
    m.set_param(999, 1.0);
    println!("{count} single-parameter runs");
}

#[test]
fn view_reports_the_block() {
    let mut m = module(&[(P_H2, -20.0), (P_H3, -30.0), (P_H4, -40.0), (P_H5, -50.0), (P_ANTI_ALIAS, AA_OS4 as f32)]);
    let mut out = [0.0f32; 16];
    assert_eq!(m.view(&mut out), view::LEN);
    assert_eq!(out[view::IN_PEAK_DB], view::FLOOR_DB);
    let x = sine(4800, 1000.0, 1.0, SR);
    let _ = run_mono(&mut m, &x);
    let mut l = sine(480, 1000.0, 1.0, SR);
    let mut r = l.clone();
    use olm_api::Module as _;
    m.process(&mut l, &mut r);
    m.view(&mut out);
    // the block's peak sits at the reference level: the estimates are the settings
    assert!(out[view::DRIVE_DB].abs() < 0.01, "drive {}", out[view::DRIVE_DB]);
    for (j, want) in [-20.0, -30.0, -40.0, -50.0].iter().enumerate() {
        assert!((out[view::H2_DB + j] - want).abs() < 0.05, "H{} {}", j + 2, out[view::H2_DB + j]);
    }
    let thd = 10.0 * (10f64.powf(-2.0) + 10f64.powf(-3.0) + 10f64.powf(-4.0) + 10f64.powf(-5.0)).log10();
    assert!((out[view::THD_DB] as f64 - thd).abs() < 0.05);
    assert!(out[view::ADDED_DB] > -30.0 && out[view::ADDED_DB] < -15.0, "added {}", out[view::ADDED_DB]);
    assert!(out[view::IN_PEAK_DB].abs() < 0.01);
    assert_eq!(out[view::AUTO_GAIN_DB], 0.0);
    // drive moves the reading
    m.set_param(P_DRIVE, -12.0);
    let _ = run_mono(&mut m, &sine(4800, 1000.0, 1.0, SR));
    m.view(&mut out);
    assert!((out[view::DRIVE_DB] + 12.0).abs() < 0.01);
    assert!(out[view::H2_DB] < -30.0);
}

#[test]
fn other_sample_rates_keep_the_levels() {
    for sr in [44100.0f64, 96000.0, 192000.0] {
        let mut m = Harmonics::default();
        m.prepare(sr as f32, 512);
        for (i, v) in [(P_AUTO_GAIN, 0.0), (P_H2, -20.0), (P_H3, -30.0), (P_ANTI_ALIAS, AA_ADAA as f32)] {
            m.set_param(i, v);
        }
        m.reset();
        let n = 16384;
        let (k, f) = bin_freq(300.0, n, sr);
        let x = sine(24000 + n, f, 1.0, sr);
        let y = run_mono(&mut m, &x);
        let h2 = harmonic_db(&y, k, n, 2);
        let h3 = harmonic_db(&y, k, n, 3);
        assert!((h2 + 20.0).abs() < 0.1 && (h3 + 30.0).abs() < 0.1, "{sr}: {h2} {h3}");
    }
}
