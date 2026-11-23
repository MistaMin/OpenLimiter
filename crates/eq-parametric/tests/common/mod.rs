#![allow(dead_code)]
use eq_parametric::ParametricEq;
use olm_api::Module;

pub const SR: f64 = 48000.0;

pub fn set(m: &mut ParametricEq, id: &str, v: f32) {
    let i = ParametricEq::param_index(id).unwrap_or_else(|| panic!("unknown {id}"));
    m.set_param(i, v);
}

/// OpenEQ's silenceBands + configureBand on band 1 (slope: cuts get the dB/oct of `cut_db`).
pub fn module_with(fs: f64, band: &[(&str, f32)], settings: &[(&str, f32)]) -> ParametricEq {
    let mut m = ParametricEq::default();
    for b in 1..=32 {
        set(&mut m, &format!("b{b}On"), 0.0);
    }
    for (id, v) in band {
        set(&mut m, id, *v);
    }
    for (id, v) in settings {
        set(&mut m, id, *v);
    }
    m.prepare(fs as f32, 512);
    m
}

pub fn bell(f: f32, g: f32, q: f32, placement: f32) -> Vec<(&'static str, f32)> {
    vec![("b1On", 1.0), ("b1Type", 0.0), ("b1Freq", f), ("b1Gain", g), ("b1Q", q), ("b1Placement", placement)]
}

pub fn tone(n: usize, f: f64, a: f32, phase: usize, fs: f64) -> Vec<f32> {
    (0..n).map(|i| (a as f64 * (2.0 * std::f64::consts::PI * f * (phase + i) as f64 / fs).sin()) as f32).collect()
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

pub fn db(r: f64) -> f64 {
    20.0 * r.max(1e-12).log10()
}

pub fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

pub fn impulse_mag(h: &[f32], f: f64, fs: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f / fs;
    let (mut re, mut im) = (0.0, 0.0);
    for (n, &v) in h.iter().enumerate() {
        re += v as f64 * (w * n as f64).cos();
        im -= v as f64 * (w * n as f64).sin();
    }
    re.hypot(im)
}

pub fn peak_index(x: &[f32]) -> usize {
    let mut best = (0, 0.0f32);
    for (i, &v) in x.iter().enumerate() {
        if v.abs() > best.1 {
            best = (i, v.abs());
        }
    }
    best.0
}

/// Processes in blocks of `block`.
pub fn run(m: &mut ParametricEq, l: &mut [f32], r: &mut [f32], block: usize) {
    let n = l.len();
    let mut i = 0;
    while i < n {
        let e = (i + block).min(n);
        m.process(&mut l[i..e], &mut r[i..e]);
        i = e;
    }
}

/// Waits for the kernel (the worker thread designs it), then flushes the convolution with silence.
pub fn settle_kernel(m: &mut ParametricEq) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let (mut a, mut b) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        m.process(&mut a, &mut b);
        if m.kernel_ready() || std::time::Instant::now() > deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let n = 4096.max(2 * m.latency() + 1024) + 48000;
    let (mut a, mut b) = (vec![0.0f32; n], vec![0.0f32; n]);
    run(m, &mut a, &mut b, 512);
}

/// Left-channel impulse response after the kernel has settled.
pub fn settled_impulse(m: &mut ParametricEq, len: usize, both: bool) -> (Vec<f32>, Vec<f32>) {
    settle_kernel(m);
    let (mut l, mut r) = (vec![0.0f32; len], vec![0.0f32; len]);
    l[0] = 1.0;
    if both {
        r[0] = 1.0;
    }
    run(m, &mut l, &mut r, 512);
    (l, r)
}

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
