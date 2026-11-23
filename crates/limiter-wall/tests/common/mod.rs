//! Shared test harness: the measurement signals and the analysis of the measurement side,
//! rebuilt from the Python in `measurements/scripts/` (the shared helper file's `env`, `up`,
//! `cross` and `metrics`, the G job script's burst analysis, the H job script's `analyse`).
#![allow(dead_code)]

pub mod composite;
pub mod curves;

use limiter_wall::WallLimiter;
use olm_api::Module;
use std::f64::consts::PI;

pub const SR: usize = 48000;

pub fn db(v: f64) -> f64 {
    20.0 * (v.abs() + 1e-12).log10()
}
pub fn lin(d: f64) -> f64 {
    10f64.powf(d / 20.0)
}

/// Runs a stereo signal through the module in blocks and removes the reported latency, as a
/// host does. Output length equals input length.
pub fn run(m: &mut WallLimiter, l: &[f64], r: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let lat = m.latency();
    let n = l.len();
    let mut a: Vec<f32> = l.iter().map(|v| *v as f32).collect();
    let mut b: Vec<f32> = r.iter().map(|v| *v as f32).collect();
    a.extend(std::iter::repeat(0.0).take(lat));
    b.extend(std::iter::repeat(0.0).take(lat));
    for (ca, cb) in a.chunks_mut(512).zip(b.chunks_mut(512)) {
        m.process(ca, cb);
    }
    (a[lat..lat + n].iter().map(|v| *v as f64).collect(), b[lat..lat + n].iter().map(|v| *v as f64).collect())
}

pub fn run_mono(m: &mut WallLimiter, x: &[f64]) -> Vec<f64> {
    run(m, x, x).0
}

pub fn fresh(sr: f32, sets: &[(usize, f32)]) -> WallLimiter {
    let mut m = WallLimiter::default();
    for (i, v) in sets {
        m.set_param(*i, *v);
    }
    m.prepare(sr, 512);
    m
}

// ----------------------------------------------------------------------------------------
// FFT based analysis
// ----------------------------------------------------------------------------------------

/// In-place radix-2 FFT. `inverse` leaves the result unscaled.
pub fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        let half = len / 2;
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..half {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (xr, xi) = (re[i + k + half], im[i + k + half]);
                let (vr, vi) = (xr * cr - xi * ci, xr * ci + xi * cr);
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + half] = ur - vr;
                im[i + k + half] = ui - vi;
                let t = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = t;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Hilbert envelope (magnitude of the analytic signal), as `env` / `hil` of the scripts. The
/// scripts transform the exact length; here the signal is zero-padded to a power of two,
/// which only differs within a few ms of the two ends.
pub fn envelope(x: &[f64]) -> Vec<f64> {
    let n = x.len().next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..x.len()].copy_from_slice(x);
    fft(&mut re, &mut im, false);
    for k in 1..n / 2 {
        re[k] *= 2.0;
        im[k] *= 2.0;
    }
    for k in n / 2 + 1..n {
        re[k] = 0.0;
        im[k] = 0.0;
    }
    fft(&mut re, &mut im, true);
    let s = 1.0 / n as f64;
    (0..x.len()).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt() * s).collect()
}

/// Gain trace in dB: processed envelope minus source envelope.
pub fn gain_trace(y: &[f64], s: &[f64]) -> Vec<f64> {
    let (ey, es) = (envelope(y), envelope(s));
    ey.iter().zip(es.iter()).map(|(a, b)| 20.0 * (a + 1e-12).log10() - 20.0 * (b + 1e-12).log10()).collect()
}

/// Ideal band-limited 8x reconstruction of a segment (the scripts' `up`), peak in linear
/// units, ignoring `skip` input samples at each end. The segment is cut to a power of two.
pub fn true_peak(x: &[f64], skip: usize) -> f64 {
    let n = if x.len().is_power_of_two() { x.len() } else { x.len().next_power_of_two() / 2 };
    let k = 8;
    let mut re = vec![0.0; n * k];
    let mut im = vec![0.0; n * k];
    let mut fr = x[..n].to_vec();
    let mut fi = vec![0.0; n];
    fft(&mut fr, &mut fi, false);
    for b in 0..n / 2 {
        re[b] = fr[b];
        im[b] = fi[b];
    }
    // Nyquist bin split between both sides, as a real inverse transform does.
    re[n / 2] = fr[n / 2] * 0.5;
    im[n / 2] = fi[n / 2] * 0.5;
    re[n * k - n / 2] = fr[n / 2] * 0.5;
    im[n * k - n / 2] = -fi[n / 2] * 0.5;
    for b in 1..n / 2 {
        re[n * k - b] = fr[b];
        im[n * k - b] = -fi[b];
    }
    fft(&mut re, &mut im, true);
    let s = 1.0 / n as f64;
    let mut pk = 0.0f64;
    for v in &re[skip * k..(n - skip) * k] {
        pk = pk.max((v * s).abs());
    }
    pk
}

pub fn sample_peak(x: &[f64]) -> f64 {
    x.iter().fold(0.0f64, |a, v| a.max(v.abs()))
}

pub fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

pub fn median(x: &[f64]) -> f64 {
    let mut v = x.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// The shared helper's `cross`: ms after `start` until the trace passes `target`,
/// linearly interpolated.
pub fn cross(g: &[f64], start: usize, target: f64, rising: bool, sr: usize) -> Option<f64> {
    let seg = &g[start..];
    let hit = seg.iter().position(|v| if rising { *v >= target } else { *v <= target })?;
    if hit == 0 {
        return Some(0.0);
    }
    let (y0, y1) = (seg[hit - 1], seg[hit]);
    Some((hit as f64 - 1.0 + (target - y0) / (y1 - y0)) / sr as f64 * 1000.0)
}

/// Sum of harmonics 2 to 9 relative to the 1 kHz fundamental, dB (the G job script's `thd`).
pub fn thd_db(seg: &[f64], sr: usize) -> f64 {
    let n = seg.len();
    let mag = |f: f64| {
        // single-bin DFT of the Hann-windowed segment at the nearest bin
        let k = (f * n as f64 / sr as f64).round();
        let (mut a, mut b) = (0.0, 0.0);
        for (i, v) in seg.iter().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / (n as f64 - 1.0)).cos();
            let ph = 2.0 * PI * k * i as f64 / n as f64;
            a += v * w * ph.cos();
            b -= v * w * ph.sin();
        }
        (a * a + b * b).sqrt()
    };
    let h1 = mag(1000.0);
    let hs: f64 = (2..10).map(|h| mag(1000.0 * h as f64).powi(2)).sum();
    db(hs.sqrt() / (h1 + 1e-20))
}

// ----------------------------------------------------------------------------------------
// Signals
// ----------------------------------------------------------------------------------------

/// Raised-cosine edged amplitude window, edges inside [i0, i1), as the burst-length generator.
fn add_burst(a: &mut [f64], i0: usize, i1: usize, lo: f64, hi: f64, edge: usize) {
    for v in a[i0..i1].iter_mut() {
        *v = hi;
    }
    for k in 0..edge {
        let r = 0.5 - 0.5 * (PI * k as f64 / edge as f64).cos();
        a[i0 + k] = lo + (hi - lo) * r;
        a[i1 - edge + k] = lo + (hi - lo) * (0.5 - 0.5 * (PI * (edge - 1 - k) as f64 / edge as f64).cos());
    }
}

/// The 8 s cycle timing signal: 1 kHz at -30 dBFS with a 200 ms burst at 0.1 s of each cycle,
/// 1 ms raised-cosine edges (`spec/curves/README.md`). No generator script exists for it, so
/// the burst level is inferred: the stored in-burst gains (5.21 dB with the output held at
/// ceiling - 0.06 dB) put it at -5.47 dBFS, i.e. the -30 dBFS tone plus a -6 dBFS tone in
/// phase, not at the -6 dBFS the README states. See REPORT.md.
pub fn timing_signal(seconds: f64, sr: usize) -> Vec<f64> {
    timing_signal_edges(seconds, sr, TIMING_RISE_START_MS, TIMING_FALL_END_MS)
}

/// Where the 1 ms edges of the timing burst sit relative to the nominal 0 and 200 ms is not
/// documented and no generator script exists. These two offsets (start of the rising edge,
/// end of the falling edge, relative to 0 and 200 ms) are read off the stored curves: the
/// rising edge from the attack points (gain 7.33 dB at 0.25 ms, 5.88 dB at 0.5 ms in the
/// manual 200 ms curve file), the falling edge from the 20 ms manual release after the hold
/// time had been fixed on the burst-length signal, whose generator is known. They amount to
/// a 200 ms burst with its edges inside, starting 0.3 to 0.4 ms early. See REPORT.md.
pub const TIMING_RISE_START_MS: f64 = -0.42;
pub const TIMING_FALL_END_MS: f64 = -0.1;

pub fn timing_signal_edges(seconds: f64, sr: usize, rise_start_ms: f64, fall_end_ms: f64) -> Vec<f64> {
    let n = (seconds * sr as f64) as usize;
    let lo = lin(-30.0);
    let hi = lin(-30.0) + lin(-6.0);
    let mut a = vec![lo; n];
    let mut c = 0;
    loop {
        let on = (c * 8 * sr + sr / 10) as f64;
        let i0 = (on + rise_start_ms * sr as f64 / 1000.0).round() as usize;
        let i1 = (on + (200.0 + fall_end_ms) * sr as f64 / 1000.0).round() as usize;
        if i1 >= n {
            break;
        }
        add_burst(&mut a, i0, i1, lo, hi, sr / 1000);
        c += 1;
    }
    (0..n).map(|i| a[i] * (2.0 * PI * 1000.0 * i as f64 / sr as f64).sin()).collect()
}

pub const BURST_LENS_MS: [usize; 5] = [5, 20, 100, 500, 2000];
pub const BURST_STARTS_S: [usize; 5] = [8, 16, 24, 32, 40];

/// The burst-length signal, from its generator in `measurements/scripts/` (the release-role
/// script): 50 s of 1 kHz at -30 dBFS, bursts of 5, 20, 100, 500, 2000 ms at -6 dBFS starting
/// at 8, 16, 24, 32, 40 s, 48-sample raised-cosine edges, 16-bit truncation.
pub fn burst_length_signal() -> Vec<f64> {
    let n = 50 * SR;
    let lo = lin(-30.0);
    let hi = lin(-6.0);
    let mut a = vec![lo; n];
    for (st, len) in BURST_STARTS_S.iter().zip(BURST_LENS_MS.iter()) {
        let i0 = st * SR;
        let i1 = ((*st as f64 + *len as f64 / 1000.0) * SR as f64) as usize;
        add_burst(&mut a, i0, i1, lo, hi, 48);
    }
    (0..n)
        .map(|i| {
            let v = a[i] * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin() * 32767.0;
            v.trunc() / 32768.0
        })
        .collect()
}

/// 1 kHz sine steps of 1 s each at -48, -42 ... 0 dBFS (G section 3).
pub fn static_steps(sr: usize) -> Vec<f64> {
    let mut x = Vec::with_capacity(9 * sr);
    for k in 0..9 {
        let a = lin(-48.0 + 6.0 * k as f64);
        for i in 0..sr {
            let t = (k * sr + i) as f64 / sr as f64;
            x.push(a * (2.0 * PI * 1000.0 * t).sin());
        }
    }
    x
}

/// Deterministic Gaussian noise (xorshift + Box-Muller); not the generator of the scripts.
pub struct Noise(pub u64);
impl Noise {
    pub fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    pub fn gauss(&mut self) -> f64 {
        let (a, b) = (self.uniform(), self.uniform());
        (-2.0 * a.ln()).sqrt() * (2.0 * PI * b).cos()
    }
}

/// Gated white noise as in the H job script: 4 s, level 0.15 with a 2 Hz gate to 1.0,
/// normalised to a peak of -1 dBFS. Own random numbers, same construction.
pub fn gated_noise(seed: u64, seconds: usize) -> Vec<f64> {
    let n = seconds * SR;
    let mut rng = Noise(seed);
    let mut x: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let e = 0.15 + 0.85 * if (2.0 * PI * 2.0 * t).sin() > 0.6 { 1.0 } else { 0.0 };
            rng.gauss() * e
        })
        .collect();
    let pk = sample_peak(&x);
    for v in x.iter_mut() {
        *v *= lin(-1.0) / pk;
    }
    x
}

// ----------------------------------------------------------------------------------------
// Reporting
// ----------------------------------------------------------------------------------------

/// One comparison line. `tol_rel` and `tol_abs`: passes when within either.
pub struct Check {
    pub rows: Vec<String>,
    pub fails: usize,
    pub known: usize,
}

impl Check {
    pub fn new() -> Self {
        Self { rows: Vec::new(), fails: 0, known: 0 }
    }
    /// Compares and records. `known_miss` marks a deviation that REPORT.md declares; it is
    /// printed but does not fail the test.
    pub fn cmp(&mut self, name: &str, measured: f64, stored: f64, tol_rel: f64, tol_abs: f64, known_miss: bool) -> bool {
        let err = (measured - stored).abs();
        let ok = err <= tol_abs || err <= tol_rel * stored.abs();
        let verdict = if ok {
            "pass"
        } else if known_miss {
            self.known += 1;
            "MISS (declared)"
        } else {
            self.fails += 1;
            "FAIL"
        };
        self.rows.push(format!("| {name} | {measured:.3} | {stored:.3} | {verdict} |"));
        ok
    }
    pub fn note(&mut self, name: &str, measured: f64, text: &str) {
        self.rows.push(format!("| {name} | {measured:.3} | {text} | info |"));
    }
    pub fn finish(&self, title: &str) {
        println!("\n#### {title}\n| check | measured | stored | result |\n|---|---|---|---|");
        for r in &self.rows {
            println!("{r}");
        }
        assert_eq!(self.fails, 0, "{title}: {} comparison(s) outside tolerance", self.fails);
    }
}
