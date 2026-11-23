//! Test signal and analysis, rebuilt from measurements/scripts/jobs2.py (functions `env`,
//! `cross`, `metrics`), srcD_mode_session.py (the mode timing pass) and curves.py.
#![allow(dead_code)]

use limiter_modes::LimiterModes;
use olm_api::Module;

pub const SR: usize = 48_000;
/// Analysis segment: the last 2^19 samples of the 16 s signal (the second burst is 3 s in).
pub const SEG: usize = 1 << 19;
pub const TOTAL: usize = 16 * SR;
pub const SEG_START: usize = TOTAL - SEG;
/// Start and end of the second burst, as in the scripts: base 8 s, on +0.1 s, off +0.3 s.
pub const ON: usize = 8 * SR + SR / 10 - SEG_START;
pub const OFF: usize = 8 * SR + 3 * SR / 10 - SEG_START;
pub const END: usize = 8 * SR + 79 * SR / 10 - SEG_START;

/// The burst signal: 1 kHz at -30 dBFS with a -6 dBFS burst from 0.1 to 0.3 s in each 8 s
/// period, 1 ms raised-cosine edges. The stored curves read +5.469 dB in the burst where
/// the output sits at 0.00 dBFS, so the burst peaks at -5.469 dBFS: the -6 dBFS burst is
/// added to the -30 dBFS tone (0.5012 + 0.0316 = 0.5328). See REPORT.md, ambiguities.
pub fn burst_signal() -> Vec<f32> {
    let lo = 10f64.powf(-30.0 / 20.0);
    let hi = lo + 10f64.powf(-6.0 / 20.0);
    let e = 48usize;
    let mut a = vec![lo; TOTAL];
    for st in [0usize, 8] {
        let (i0, i1) = (st * SR + SR / 10, st * SR + 3 * SR / 10);
        for v in a[i0..i1].iter_mut() {
            *v = hi;
        }
        for k in 0..e {
            let r = 0.5 - 0.5 * (core::f64::consts::PI * k as f64 / e as f64).cos();
            a[i0 + k] = lo + (hi - lo) * r;
            a[i1 - 1 - k] = lo + (hi - lo) * r;
        }
    }
    (0..TOTAL).map(|i| (a[i] * (2.0 * core::f64::consts::PI * 1000.0 * i as f64 / SR as f64).sin()) as f32).collect()
}

pub fn sine(freq: f64, dbfs: f64, phase: f64, len: usize) -> Vec<f32> {
    let a = 10f64.powf(dbfs / 20.0);
    (0..len).map(|i| (a * (2.0 * core::f64::consts::PI * freq * i as f64 / SR as f64 + phase).sin()) as f32).collect()
}

/// Runs a mono signal through both channels in blocks and removes the reported latency.
pub fn run(m: &mut LimiterModes, x: &[f32]) -> Vec<f32> {
    let (l, _) = run_stereo(m, x, x);
    l
}

pub fn run_stereo(m: &mut LimiterModes, xl: &[f32], xr: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let lat = m.latency();
    let mut l: Vec<f32> = xl.to_vec();
    let mut r: Vec<f32> = xr.to_vec();
    l.resize(xl.len() + lat, 0.0);
    r.resize(xr.len() + lat, 0.0);
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        m.process(a, b);
    }
    (l[lat..].to_vec(), r[lat..].to_vec())
}

pub fn make(style: usize, character: f32, drive: f32) -> LimiterModes {
    let mut m = LimiterModes::default();
    m.set_param(limiter_modes::P_STYLE, style as f32);
    m.set_param(limiter_modes::P_CHARACTER, character);
    m.set_param(limiter_modes::P_DRIVE, drive);
    m.prepare(SR as f32, 512);
    m
}

fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
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
        let ang = 2.0 * core::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (i + k, i + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let t = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = t;
            }
            i += len;
        }
        len <<= 1;
    }
    if inverse {
        for v in re.iter_mut().chain(im.iter_mut()) {
            *v /= n as f64;
        }
    }
}

/// Envelope as in jobs2.py `env`: magnitude of the analytic signal. Length must be 2^k.
pub fn envelope(x: &[f32]) -> Vec<f64> {
    let n = x.len();
    let mut re: Vec<f64> = x.iter().map(|v| *v as f64).collect();
    let mut im = vec![0.0; n];
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
    re.iter().zip(im.iter()).map(|(a, b)| (a * a + b * b).sqrt()).collect()
}

/// Gain against time over the analysis segment: processed envelope minus source envelope, dB.
pub fn gain_curve(y: &[f32], x: &[f32]) -> Vec<f64> {
    let ey = envelope(&y[SEG_START..SEG_START + SEG]);
    let ex = envelope(&x[SEG_START..SEG_START + SEG]);
    ey.iter().zip(ex.iter()).map(|(a, b)| 20.0 * (a + 1e-12).log10() - 20.0 * (b + 1e-12).log10()).collect()
}

pub fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

/// `cross` of the scripts: first crossing of `target` from `start - back`, in ms relative
/// to `start`. `Some(f64::NEG_INFINITY)` stands for "already there at the window start"
/// (the scripts print 0.0, "before window" or "at window start").
pub fn cross(g: &[f64], start: usize, target: f64, rising: bool, back: usize) -> Option<f64> {
    let s0 = start - back;
    let hit = (s0..g.len()).find(|&i| if rising { g[i] >= target } else { g[i] <= target })?;
    if hit == s0 {
        return Some(f64::NEG_INFINITY);
    }
    let (y0, y1) = (g[hit - 1], g[hit]);
    Some(((hit - 1) as f64 + (target - y0) / (y1 - y0) - start as f64) / SR as f64 * 1000.0)
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub pre: f64,
    pub fin: f64,
    pub post: f64,
    pub attack: Option<f64>,
    pub release63: Option<f64>,
    pub release90: Option<f64>,
}

/// Mode timing as in srcD_mode_session.py: attack searched from 250 ms before the edge.
pub fn timing_modes(g: &[f64]) -> Timing {
    let pre = median(&g[ON - 4 * SR / 10..ON - 3 * SR / 10]);
    let fin = median(&g[OFF - SR / 20..OFF - SR / 200]);
    let post = median(&g[END - SR / 4..END]);
    Timing {
        pre,
        fin,
        post,
        attack: cross(g, ON, pre + 0.63 * (fin - pre), fin > pre, SR / 4),
        release63: cross(g, OFF, fin + 0.63 * (post - fin), post > fin, 0),
        release90: cross(g, OFF, fin + 0.9 * (post - fin), post > fin, 0),
    }
}

/// Type timing as in jobs2.py `metrics` (used by srcE_jobs.py): windows end at the edges.
pub fn timing_types(g: &[f64]) -> Timing {
    let pre = median(&g[ON - SR / 20..ON]);
    let fin = median(&g[OFF - SR / 20..OFF]);
    let post = median(&g[END - SR / 4..END]);
    Timing {
        pre,
        fin,
        post,
        attack: cross(g, ON, pre + 0.63 * (fin - pre), fin > pre, 0),
        release63: cross(g, OFF, fin + 0.63 * (post - fin), post > fin, 0),
        release90: cross(g, OFF, fin + 0.9 * (post - fin), post > fin, 0),
    }
}

/// Gain at `ms` from the start of the second burst (the time axis of spec/curves).
pub fn at_ms(g: &[f64], ms: f64) -> f64 {
    g[(ON as f64 + (ms * SR as f64 / 1000.0).round()) as usize]
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |a, b| a.max(b.abs()))
}

pub fn rms_db(x: &[f32]) -> f64 {
    let s: f64 = x.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    10.0 * (s / x.len() as f64 + 1e-30).log10()
}

pub fn db(v: f64) -> f64 {
    20.0 * (v + 1e-12).log10()
}

/// Tolerance of the brief: 10 % or 0.3 ms, whichever is larger.
pub fn assert_time(name: &str, got: Option<f64>, want_ms: f64) {
    let g = got.unwrap_or_else(|| panic!("{name}: no crossing"));
    assert!(g.is_finite(), "{name}: read 'at window start', stored {want_ms} ms");
    let tol = (0.1 * want_ms.abs()).max(0.3);
    assert!((g - want_ms).abs() <= tol, "{name}: measured {g:.3} ms, stored {want_ms} ms, tolerance {tol:.2}");
}

/// The scripts print 0.0 or "before window" when the gain is already past the target at
/// the window start.
pub fn assert_at_start(name: &str, got: Option<f64>) {
    assert!(got == Some(f64::NEG_INFINITY), "{name}: measured {got:?}, stored 'below the method floor'");
}

pub fn assert_db(name: &str, got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "{name}: measured {got:.3} dB, stored {want} dB, tolerance {tol}");
}

/// static_steps.wav as described in srcF_signals.py ("like static_steps.wav"): 1 kHz at
/// -48..0 dBFS in 6 dB steps of 1 s.
pub fn static_steps() -> Vec<f32> {
    (0..9 * SR)
        .map(|i| {
            let a = 10f64.powf((-48.0 + 6.0 * (i / SR) as f64) / 20.0);
            (a * (2.0 * core::f64::consts::PI * 1000.0 * i as f64 / SR as f64).sin()) as f32
        })
        .collect()
}

/// `static_gain_db` of jobs2.py `metrics`: RMS difference over 0.5..0.9 s of each step.
pub fn static_gains(y: &[f32], x: &[f32]) -> Vec<f64> {
    (0..9).map(|k| {
        let (a, b) = (k * SR + SR / 2, k * SR + 9 * SR / 10);
        rms_db(&y[a..b]) - rms_db(&x[a..b])
    }).collect()
}

/// Programme-like stereo material (own design): sweep, noise bursts and pulses, L != R.
pub fn programme(seconds: usize) -> (Vec<f32>, Vec<f32>) {
    let n = seconds * SR;
    let mut seed = 0x2545_F491u32;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut phase = 0.0f64;
    for i in 0..n {
        let t = i as f64 / SR as f64;
        let f = 40.0 * (400.0f64).powf((t % 2.0) / 2.0);
        phase += 2.0 * core::f64::consts::PI * f / SR as f64;
        let sweep = 0.25 * phase.sin() as f32;
        let burst = if (i % (SR / 2)) < SR / 8 { 0.35 } else { 0.02 };
        let (nl, nr) = (noise() * burst, noise() * burst);
        let pulse = if i % (SR / 3) < 4 { 0.9 } else { 0.0 };
        l[i] = sweep + nl + pulse;
        r[i] = 0.8 * sweep + nr - if i % (SR / 5) < 2 { 0.7 } else { 0.0 };
    }
    (l, r)
}

/// True peak by 8x band-limited reconstruction (jobs2.py `up`), over the middle of `x`.
pub fn true_peak(x: &[f32]) -> f64 {
    let n = 1usize << 14;
    assert!(x.len() >= n);
    let mut re: Vec<f64> = x[..n].iter().map(|v| *v as f64).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im, false);
    let m = n * 8;
    let (mut ur, mut ui) = (vec![0.0; m], vec![0.0; m]);
    for k in 0..n / 2 {
        ur[k] = re[k];
        ui[k] = im[k];
        if k > 0 {
            ur[m - k] = re[n - k];
            ui[m - k] = im[n - k];
        }
    }
    fft(&mut ur, &mut ui, true);
    ur[m / 4..3 * m / 4].iter().fold(0.0f64, |a, b| a.max(b.abs() * 8.0))
}
