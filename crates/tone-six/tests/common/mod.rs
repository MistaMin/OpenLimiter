#![allow(dead_code)]
use olm_api::Module;
use tone_six::ToneSix;

pub fn module(fs: f32, settings: &[(&str, f32)]) -> ToneSix {
    let mut m = ToneSix::default();
    for (id, v) in settings {
        let i = ToneSix::param_index(id).unwrap_or_else(|| panic!("unknown id {id}"));
        m.set_param(i, *v);
    }
    m.prepare(fs, 512);
    m
}

pub fn set(m: &mut ToneSix, id: &str, v: f32) {
    let i = ToneSix::param_index(id).unwrap_or_else(|| panic!("unknown id {id}"));
    m.set_param(i, v);
}

/// Left-channel impulse response through `process`, `n` samples, in blocks of 512.
pub fn impulse(m: &mut ToneSix, n: usize) -> Vec<f64> {
    let mut l = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];
    l[0] = 1.0;
    for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(cl, cr);
    }
    l.iter().map(|&x| x as f64).collect()
}

/// |DFT| of `h` at `f` Hz, in dB (rotating phasor, f64).
pub fn dft_db(h: &[f64], f: f64, fs: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f / fs;
    let (c, s) = (w.cos(), -w.sin());
    let (mut pr, mut pi) = (1.0f64, 0.0f64);
    let (mut re, mut im) = (0.0, 0.0);
    for (n, &x) in h.iter().enumerate() {
        re += x * pr;
        im += x * pi;
        let t = pr * c - pi * s;
        pi = pr * s + pi * c;
        pr = t;
        if n % 4096 == 4095 {
            let g = 1.0 / (pr * pr + pi * pi).sqrt();
            pr *= g;
            pi *= g;
        }
    }
    10.0 * (re * re + im * im).max(1e-300).log10()
}

/// Deterministic white noise in -a..a.
pub fn noise(n: usize, seed: u64, a: f32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * a
        })
        .collect()
}

pub fn sine(n: usize, f: f64, fs: f64, a: f64) -> Vec<f32> {
    (0..n).map(|i| (a * (2.0 * std::f64::consts::PI * f * i as f64 / fs).sin()) as f32).collect()
}

/// Amplitude (dB re 1) of a steady tone at `f` in `x[a..b]`, by projection on sin and cos.
pub fn tone_db(x: &[f32], f: f64, fs: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f / fs;
    let (mut c, mut s) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        c += v as f64 * (w * i as f64).cos();
        s += v as f64 * (w * i as f64).sin();
    }
    let amp = 2.0 * (c * c + s * s).sqrt() / x.len() as f64;
    20.0 * amp.max(1e-300).log10()
}

/// Largest |second difference| of a signal.
pub fn max_d2(x: &[f32]) -> f64 {
    x.windows(3).map(|w| (w[2] as f64 - 2.0 * w[1] as f64 + w[0] as f64).abs()).fold(0.0, f64::max)
}
