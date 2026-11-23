//! Test helpers: the measurement signals and analysis of `measurements/scripts/` (the source F
//! signal, library and round scripts), ported to Rust.
#![allow(dead_code)]

use comp_bands::*;
use olm_api::Module;

pub const SR: f32 = 48000.0;
pub const SRU: usize = 48000;

pub fn db(v: f64) -> f64 {
    20.0 * (v.abs() + 1e-12).log10()
}

pub fn lin(d: f64) -> f64 {
    10f64.powf(d / 20.0)
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

pub fn peak(x: &[f32]) -> f64 {
    x.iter().fold(0.0f64, |m, v| m.max(v.abs() as f64))
}

/// A fresh module at 48 kHz with every band at the given settings left at default.
pub fn module() -> BandCompressor {
    let mut m = BandCompressor::default();
    m.prepare(SR, 512);
    m
}

pub fn set_band(m: &mut BandCompressor, band: usize, offset: usize, v: f32) {
    m.set_param(band_param(band, offset), v);
}

/// Thresholds of all bands to +20 dB: nothing compresses (the scripts' "NOC").
pub fn nothing_compresses(m: &mut BandCompressor) {
    for b in 0..BANDS {
        set_band(m, b, B_THRESHOLD, 20.0);
    }
}

/// Runs the signal through the module in blocks and removes the latency.
pub fn render(m: &mut BandCompressor, l: &[f32], r: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let lat = m.latency();
    let mut a: Vec<f32> = l.to_vec();
    let mut b: Vec<f32> = r.to_vec();
    a.extend(std::iter::repeat(0.0).take(lat));
    b.extend(std::iter::repeat(0.0).take(lat));
    for (x, y) in a.chunks_mut(512).zip(b.chunks_mut(512)) {
        m.process(x, y);
    }
    (a[lat..].to_vec(), b[lat..].to_vec())
}

pub fn render_mono(m: &mut BandCompressor, x: &[f32]) -> Vec<f32> {
    render(m, x, x).0
}

/// `fine_<f>.wav` of the signal script: -50..0 dBFS in 2 dB steps of 1 s.
pub fn fine_steps(freq: f64) -> Vec<f32> {
    steps(freq, -50, 2)
}

/// `steps_<f>.wav`: -48..0 dBFS in 6 dB steps of 1 s.
pub fn coarse_steps(freq: f64) -> Vec<f32> {
    steps(freq, -48, 6)
}

fn steps(freq: f64, first: i32, step: i32) -> Vec<f32> {
    let count = ((0 - first) / step + 1) as usize;
    let mut x = Vec::with_capacity(count * SRU);
    for k in 0..count {
        let a = lin((first + step * k as i32) as f64);
        for i in 0..SRU {
            let t = (k * SRU + i) as f64 / SR as f64;
            x.push((a * (2.0 * std::f64::consts::PI * freq * t).sin()) as f32);
        }
    }
    x
}

/// Gain per step, read like the scripts: RMS over `from..to` seconds of each 1 s step.
pub fn step_gains(y: &[f32], s: &[f32], from: f64, to: f64) -> Vec<f64> {
    let count = s.len() / SRU;
    (0..count)
        .map(|k| {
            let a = ((k as f64 + from) * SR as f64) as usize;
            let b = ((k as f64 + to) * SR as f64) as usize;
            db(rms(&y[a..b])) - db(rms(&s[a..b]))
        })
        .collect()
}

/// `burst_<f>.wav`: -30 dBFS with a -6 dBFS burst from 0.1 to 0.3 s in each 8 s period, 1 ms
/// raised-cosine edges. `seconds` long.
pub fn burst(freq: f64, seconds: usize) -> Vec<f32> {
    let n = seconds * SRU;
    let lo = lin(-30.0);
    let hi = lin(-6.0);
    let mut a = vec![lo; n];
    let e = 48usize;
    let mut st = 0usize;
    while st * SRU < n {
        let i0 = ((st as f64 + 0.1) * SR as f64) as usize;
        let i1 = ((st as f64 + 0.3) * SR as f64) as usize;
        if i1 <= n {
            for v in a[i0..i1].iter_mut() {
                *v = hi;
            }
            for k in 0..e {
                let r = 0.5 - 0.5 * (std::f64::consts::PI * k as f64 / e as f64).cos();
                a[i0 + k] = lo + (hi - lo) * r;
                a[i1 - e + k] = lo + (hi - lo) * (0.5 - 0.5 * (std::f64::consts::PI * (e - 1 - k) as f64 / e as f64).cos());
            }
        }
        st += 8;
    }
    (0..n).map(|i| (a[i] * (2.0 * std::f64::consts::PI * freq * i as f64 / SR as f64).sin()) as f32).collect()
}

// --- FFT and the analytic-signal envelope of the library script (`env`) -----------------------

pub fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    assert!(n.is_power_of_two());
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (xr, xi) = (re[i + k + len / 2], im[i + k + len / 2]);
                let (vr, vi) = (xr * cr - xi * ci, xr * ci + xi * cr);
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let t = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = t;
            }
            i += len;
        }
        len <<= 1;
    }
    if inverse {
        for v in re.iter_mut() {
            *v /= n as f64;
        }
        for v in im.iter_mut() {
            *v /= n as f64;
        }
    }
}

/// Magnitude of the analytic signal. The input length must be a power of two.
pub fn envelope(x: &[f32]) -> Vec<f64> {
    let n = x.len();
    let mut re: Vec<f64> = x.iter().map(|v| *v as f64).collect();
    let mut im = vec![0.0f64; n];
    fft(&mut re, &mut im, false);
    for i in 1..n / 2 {
        re[i] *= 2.0;
        im[i] *= 2.0;
    }
    for i in n / 2 + 1..n {
        re[i] = 0.0;
        im[i] = 0.0;
    }
    fft(&mut re, &mut im, true);
    re.iter().zip(im.iter()).map(|(a, b)| (a * a + b * b).sqrt()).collect()
}

pub struct Timing {
    /// Gain in dB per sample; index 0 is the start of the analysed (second) burst minus `pre`.
    pub gain: Vec<f64>,
    pub on: usize,
    pub bg: f64,
    pub end_of_burst: f64,
    pub recovered: f64,
    pub attack_63: Option<f64>,
    pub attack_90: Option<f64>,
    pub release_63: Option<f64>,
    pub release_90: Option<f64>,
    pub deepest: f64,
}

impl Timing {
    /// Gain at `ms` from the burst start, like the curve files.
    pub fn at(&self, ms: f64) -> f64 {
        self.gain[(self.on as f64 + (ms * SR as f64 / 1000.0).round()) as usize]
    }
}

fn cross(g: &[f64], start: usize, target: f64, rising: bool) -> Option<f64> {
    let seg = &g[start..];
    let hit = seg.iter().position(|v| if rising { *v >= target } else { *v <= target })?;
    if hit == 0 {
        return Some(0.0);
    }
    let (y0, y1) = (seg[hit - 1], seg[hit]);
    Some((hit as f64 - 1.0 + (target - y0) / (y1 - y0)) / SR as f64 * 1000.0)
}

/// `timing()` of the library script: the second burst of an 8 s period signal. `y` and `s` must
/// be 2^20 samples long (21.8 s), which covers the second period.
pub fn timing(y: &[f32], s: &[f32]) -> Timing {
    let ey = envelope(y);
    let es = envelope(s);
    let g: Vec<f64> = ey.iter().zip(es.iter()).map(|(a, b)| 20.0 * (a + 1e-9).log10() - 20.0 * (b + 1e-9).log10()).collect();
    let base = 8 * SRU;
    let on = base + (0.1 * SR as f64) as usize;
    let off = base + (0.3 * SR as f64) as usize;
    let end = base + (7.9 * SR as f64) as usize;
    let median = |a: usize, b: usize| {
        let mut v: Vec<f64> = g[a..b].to_vec();
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let n = v.len();
        if n % 2 == 1 { v[n / 2] } else { 0.5 * (v[n / 2 - 1] + v[n / 2]) }
    };
    let pre = median(on - (0.05 * SR as f64) as usize, on);
    let fin = median(off - (0.02 * SR as f64) as usize, off);
    let post = median(end - (0.25 * SR as f64) as usize, end);
    let deepest = g[on..off].iter().cloned().fold(f64::INFINITY, f64::min);
    Timing {
        attack_63: cross(&g, on, pre + 0.63 * (fin - pre), fin > pre),
        attack_90: cross(&g, on, pre + 0.9 * (fin - pre), fin > pre),
        release_63: cross(&g, off, fin + 0.63 * (post - fin), post > fin),
        release_90: cross(&g, off, fin + 0.9 * (post - fin), post > fin),
        gain: g,
        on,
        bg: pre,
        end_of_burst: fin,
        recovered: post,
        deepest,
    }
}

pub const ANALYSIS_LEN: usize = 1 << 20;

/// Burst render and analysis in one step.
pub fn burst_timing(m: &mut BandCompressor, freq: f64) -> Timing {
    let s = burst(freq, 24);
    let y = render_mono(m, &s);
    timing(&y[..ANALYSIS_LEN], &s[..ANALYSIS_LEN])
}

// --- Multitone of the signal script (`multitone_dense.wav`) -----------------------------------

/// 100 log-spaced tones 20 Hz to 20 kHz on a 0.25 Hz grid (duplicates removed).
pub fn multitone_freqs() -> Vec<f64> {
    let mut f: Vec<f64> = (0..100)
        .map(|k| {
            let v = 20.0 * (1000.0f64).powf(k as f64 / 99.0);
            (v * 4.0).round() / 4.0
        })
        .collect();
    f.dedup();
    f
}

/// The tones at -45 dBFS each for 6 s. The phases come from a small generator of our own, not
/// from the script's random generator, which cannot be reproduced without its library.
pub fn multitone(freqs: &[f64]) -> Vec<f32> {
    let n = 6 * SRU;
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut x = vec![0.0f64; n];
    let a = lin(-45.0);
    for f in freqs {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ph = (seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 * std::f64::consts::PI;
        let w = 2.0 * std::f64::consts::PI * f / SR as f64;
        for (i, v) in x.iter_mut().enumerate() {
            *v += a * (w * i as f64 + ph).sin();
        }
    }
    x.iter().map(|v| *v as f32).collect()
}

/// Complex response at each tone: Hann-windowed single-bin transform over 1 s to 5 s, output
/// over input (the "cx" analysis of the round 2 script).
pub fn tone_response(y: &[f32], s: &[f32], freqs: &[f64]) -> Vec<(f64, f64)> {
    let a = SRU;
    let b = 5 * SRU;
    let n = b - a;
    freqs
        .iter()
        .map(|f| {
            let w = 2.0 * std::f64::consts::PI * f / SR as f64;
            let (mut yr, mut yi, mut sr, mut si) = (0.0, 0.0, 0.0, 0.0);
            for k in 0..n {
                let h = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / (n - 1) as f64).cos();
                let (c, sn) = ((w * k as f64).cos(), (w * k as f64).sin());
                yr += y[a + k] as f64 * h * c;
                yi -= y[a + k] as f64 * h * sn;
                sr += s[a + k] as f64 * h * c;
                si -= s[a + k] as f64 * h * sn;
            }
            let d = sr * sr + si * si;
            ((yr * sr + yi * si) / d, (yi * sr - yr * si) / d)
        })
        .collect()
}

pub fn mag_db(h: (f64, f64)) -> f64 {
    db((h.0 * h.0 + h.1 * h.1).sqrt())
}

/// Index of the tone nearest to `f`.
pub fn nearest(freqs: &[f64], f: f64) -> usize {
    let mut best = 0;
    for (i, v) in freqs.iter().enumerate() {
        if (v - f).abs() < (freqs[best] - f).abs() {
            best = i;
        }
    }
    best
}

// --- Pink noise like `noise_lr.wav` of the signal script ---------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let (a, b) = (self.next().max(1e-300), self.next());
        (-2.0 * a.ln()).sqrt() * (2.0 * std::f64::consts::PI * b).cos()
    }
}

/// White Gaussian noise shaped by 1/sqrt(f) in the frequency domain, RMS -20 dBFS, 2^19 samples
/// (the script uses 6 s and another random generator; the statistics are the same).
pub fn pink(seed: u64) -> Vec<f32> {
    let n = 1usize << 19;
    let mut rng = Rng(seed);
    let mut re: Vec<f64> = (0..n).map(|_| rng.normal()).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im, false);
    re[0] = 0.0;
    im[0] = 0.0;
    for k in 1..n {
        let f = (if k <= n / 2 { k } else { n - k }) as f64 * SR as f64 / n as f64;
        re[k] /= f.sqrt();
        im[k] /= f.sqrt();
    }
    fft(&mut re, &mut im, true);
    let r = (re.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
    re.iter().map(|v| (v / r * lin(-20.0)) as f32).collect()
}
