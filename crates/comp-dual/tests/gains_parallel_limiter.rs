//! Gain laws, parallel mix laws and the safety limiter's three ceilings.
mod common;
use common::*;
use olm_api::Module;

const NEUTRAL: [(&str, f32); 4] = [("limiterOn", 0.0), ("threshold1", 0.0), ("ratio1", 1.0), ("softKnee1", 0.0)];

fn tone_gain(settings: &[(&str, f32)]) -> f64 {
    // measurements/scripts/gain_laws_float.py and srcB_parallel.py: 1 kHz at -40 dBFS, 2 s;
    // gain of the tone between 0.5 and 1.75 s.
    let x = sine(1000.0, 0.01, 0.0, 2 * SR);
    let mut all = NEUTRAL.to_vec();
    all.extend_from_slice(settings);
    let y = render(&mut with(&all), &x);
    let (a, b) = (SR / 2, 7 * SR / 4);
    db(tone_amp(&y, 1000.0, a, b)) - db(0.01)
}

#[test]
fn gains_are_plain_db() {
    // measurements/float_check/gain_laws.json (spec section 58). The source's tapers are replaced
    // by plain dB controls; the stored display values are used as the settings here.
    let mut t = Table::new("Gain laws (gain_laws.json)");
    for v in [-6.0, -1.5, -0.3, 0.0, 0.3, 1.5, 4.5, 12.0] {
        t.level(&format!("input gain {v} dB"), tone_gain(&[("inputGain", v)]), v as f64, 0.05);
        t.level(&format!("output gain {v} dB"), tone_gain(&[("outputGain", v)]), v as f64, 0.05);
    }
    for v in [0.0, 3.8, 7.5, 11.3, 15.0, 18.8, 22.5, 26.3, 30.0] {
        t.level(&format!("limiter drive {v} dB (limiter off)"), tone_gain(&[("limiterGain", v)]), v as f64, 0.05);
    }
    for v in [-60.0, -19.0, -7.0, -3.2, 0.0, 3.2, 7.0, 19.0, 60.0] {
        t.level(&format!("make-up {v} dB"), tone_gain(&[("makeup1", v)]), v as f64, 0.05);
    }
    // the bottom of the input and output ranges is silence ("-inf" in the source)
    let x = sine(1000.0, 0.01, 0.0, SR);
    for id in ["inputGain", "outputGain"] {
        let y = render(&mut with(&[(id, -60.0)]), &x);
        t.truth(&format!("{id} at its minimum is silent"), peak(&y) == 0.0, "exact zero");
    }
    t.finish();
}

#[test]
fn parallel_laws() {
    // measurements/float_check/srcB_parallel.json, "quiet tone, compressor neutral" (spec section 69)
    let rows = [(-60.0, 0.01), (-19.0, 0.92), (-7.0, 3.21), (-3.2, 4.57), (0.0, 6.02), (3.2, 7.77), (7.0, 10.21), (19.0, 19.92)];
    let mut t = Table::new("Parallel (srcB_parallel.json)");
    for (mk, want) in rows {
        t.level(&format!("sum, make-up {mk} dB"), tone_gain(&[("parallelMode", 1.0), ("makeup1", mk)]), want, 0.05);
    }
    // Blend (the sibling unit's mix control, source C params table, row 39):
    // 0 = dry, 100 = compressed. Checked against the law, with make-up -7 dB as the "compressed" level.
    let wet = lin(-7.0);
    for mix in [0.0f32, 25.0, 50.0, 75.0, 100.0] {
        let m = mix as f64 / 100.0;
        let want = db(1.0 - m + m * wet);
        t.level(&format!("blend, mix {mix} %"), tone_gain(&[("parallelMode", 2.0), ("parallelMix", mix), ("makeup1", -7.0)]), want, 0.05);
    }
    // Engaged on a hot signal: the sum output is exactly dry + (output with parallel off).
    // Stored fit: dry 0.997, compressed 1.001 to 1.005, residual about -33 dB; this build is exact.
    let x = hot_programme(4 * SR, 7);
    let s = [("limiterOn", 0.0), ("threshold1", -24.0), ("ratio1", 5.18), ("softKnee1", 0.0)];
    let c = render(&mut with(&s), &x);
    let mut sp = s.to_vec();
    sp.push(("parallelMode", 1.0));
    let p = render(&mut with(&sp), &x);
    let res: Vec<f32> = (0..x.len()).map(|i| p[i] - x[i] - c[i]).collect();
    let rel = db(rms(&res[SR..])) - db(rms(&p[SR..]));
    t.truth("sum = dry + compressed on programme", rel < -100.0, &format!("residual {rel:.1} dB (stored -33 dB)"));
    t.finish();
}

/// Bursts and gated noise, peak near -1 dBFS (own generator; the stored programme file is not used).
fn hot_programme(n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let mut lp = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / SRF;
            let gate = if (t * 3.0).fract() < 0.4 { 1.0 } else { 0.1 };
            lp += (rnd() - lp) * 0.35;
            let tone = 0.45 * (PI2 * 110.0 * t).sin() + 0.2 * (PI2 * 2500.0 * t).sin();
            ((gate * (0.9 * lp + tone)).clamp(-0.89, 0.89)) as f32
        })
        .collect()
}

/// 8x FFT-free true peak: windowed-sinc interpolation, 64 taps per phase (the stored figures use
/// an 8x band-limited reconstruction; `up` in srcB_safety_float.py).
fn true_peak(y: &[f32]) -> f64 {
    let half = 32isize;
    let mut pk = 0.0f64;
    for i in (half as usize + 1)..(y.len() - half as usize - 1) {
        for p in 0..8 {
            let frac = p as f64 / 8.0;
            let mut v = 0.0;
            for k in -half + 1..=half {
                let a = k as f64 - frac;
                let s = if a.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * a).sin() / (std::f64::consts::PI * a) };
                let w = 0.5 + 0.5 * (std::f64::consts::PI * a / (half as f64 + 1.0)).cos();
                v += y[(i as isize + k) as usize] as f64 * s * w;
            }
            pk = pk.max(v.abs());
        }
    }
    pk
}

#[test]
fn limiter_ceilings() {
    // measurements/float_check/srcB_safety_float.json (spec section 57.3): compressor neutral,
    // a 0 dBFS 1 kHz sine and the inter-sample tone (12 kHz at 45 degrees), input gain +1.5 and +6 dB.
    // Cells: sample peak dBFS / true peak dBTP, read between 1 and 3 s.
    let sine_x = sine(1000.0, 1.0, 0.0, 4 * SR);
    let isp_x = sine(12000.0, 1.0, std::f64::consts::FRAC_PI_4, 4 * SR);
    // (type, on), then [sine +1.5, sine +6, isp +1.5, isp +6] as (sample, true)
    let rows: [(&str, f32, f32, [(f64, f64); 4]); 4] = [
        ("sample -0.05", 0.0, 1.0, [(-0.05, -0.05), (-0.05, -0.05), (-1.49, 1.52), (-0.04, 2.97)]),
        ("sample -0.01", 1.0, 1.0, [(-0.01, -0.01), (-0.01, -0.01), (-1.51, 1.50), (0.0, 3.01)]),
        ("true peak", 2.0, 1.0, [(-0.18, -0.18), (-0.18, -0.18), (-3.23, -0.22), (-3.23, -0.22)]),
        ("off", 1.0, 0.0, [(1.50, 1.50), (6.00, 6.00), (-1.51, 1.50), (2.99, 6.00)]),
    ];
    let mut t = Table::new("Safety limiter ceilings (srcB_safety_float.json)");
    for (name, ty, on, cells) in rows.iter() {
        for (j, (sig, gain)) in [("sine", 1.5f32), ("sine", 6.0), ("inter-sample tone", 1.5), ("inter-sample tone", 6.0)].iter().enumerate() {
            let x = if *sig == "sine" { &sine_x } else { &isp_x };
            let mut m = with(&[("threshold1", 0.0), ("ratio1", 1.0), ("softKnee1", 0.0), ("inputGain", *gain), ("limiterOn", *on), ("limiterType", *ty)]);
            let y = render(&mut m, x);
            let seg = &y[SR..3 * SR];
            t.level(&format!("{name}, {sig} +{gain}: sample peak"), db(peak(seg)), cells[j].0, 0.05);
            t.level(&format!("{name}, {sig} +{gain}: true peak"), db(true_peak(&seg[..SR / 4])), cells[j].1, 0.1);
        }
    }
    t.finish();
}

#[test]
fn limiter_on_programme() {
    // measurements/float_check/results.json (spec section 57): with the limiter on no sample is
    // above full scale; off, nothing is limited or clipped; output gain is after the limiter.
    let x = hot_programme(6 * SR, 3);
    let src_peak = db(peak(&x));
    let mut t = Table::new("Safety limiter on programme (results.json, spec 57)");
    for (name, ty, ceil) in [("sample -0.05", 0.0f32, -0.05f64), ("sample -0.01", 1.0, -0.01), ("true peak", 2.0, -0.20)] {
        let y = render(&mut with(&[("inputGain", 12.0), ("limiterType", ty)]), &x);
        let over = y.iter().filter(|v| v.abs() > 1.0).count();
        t.truth(&format!("{name}, input +12: samples above full scale"), over == 0, &format!("{over} (stored 0)"));
        t.truth(&format!("{name}, input +12: sample peak <= ceiling"), db(peak(&y)) <= ceil + 1.0e-4, &format!("{:.3} dBFS", db(peak(&y))));
    }
    // On dense programme the source's true-peak type lets the true peak through (stored +1.74 dBTP
    // against +5.01 dBTP for the sample type); only tones are held at the ceiling (57.3). Same here.
    let y = render(&mut with(&[("inputGain", 12.0), ("limiterType", 2.0)]), &x);
    let tp = db(true_peak(&y[SR..2 * SR]));
    let y1 = render(&mut with(&[("inputGain", 12.0), ("limiterType", 1.0)]), &x);
    let tp1 = db(true_peak(&y1[SR..2 * SR]));
    t.truth("true peak type, input +12: true peak on programme at most the stored +1.74 dBTP", tp < 1.74, &format!("{tp:.2} dBTP (own programme signal)"));
    t.truth("true peak type holds the true peak lower than the sample type", tp < tp1, &format!("{tp:.2} against {tp1:.2} dBTP (stored +1.74 against +5.01)"));
    let y = render(&mut with(&[("inputGain", 12.0), ("limiterOn", 0.0)]), &x);
    t.level("off, input +12: sample peak (not clipped)", db(peak(&y)), src_peak + 12.0, 0.05);
    let y = render(&mut with(&[("limiterGain", 15.0)]), &x);
    t.truth("limiter drive 15 dB is limited", db(peak(&y)) <= -0.01 + 1.0e-4, &format!("{:.3} dBFS", db(peak(&y))));
    let y = render(&mut with(&[("inputGain", 12.0), ("outputGain", 12.0)]), &x);
    t.level("output +12 after the limiter is not limited", db(peak(&y)), 12.0 - 0.01, 0.05);
    // gain-reduction meter of the limiter
    let mut m = with(&[("inputGain", 12.0)]);
    let _ = render(&mut m, &x[..SR]);
    t.truth("limiter reduction shows on the meter", m.meters().band_reduction_db[2] >= 0.0, "band_reduction_db[2]");
    t.finish();
}
