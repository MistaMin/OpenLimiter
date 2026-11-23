//! Shared helpers for the acceptance tests: rendering through the module, the measurement
//! code of `measurements/scripts/jobs2.py` re-expressed in Rust, and a small result table.
#![allow(dead_code)]

use comp_dual::CompDual;
use olm_api::Module;

pub const SR: usize = 48_000;
pub const SRF: f64 = 48_000.0;
pub const PI2: f64 = std::f64::consts::TAU;

pub fn lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn db(v: f64) -> f64 {
    20.0 * (v + 1.0e-12).log10()
}

pub fn fresh() -> CompDual {
    let mut m = CompDual::default();
    m.prepare(SRF as f32, 512);
    m
}

pub fn set(m: &mut CompDual, id: &str, v: f32) {
    let i = CompDual::param_index(id).unwrap_or_else(|| panic!("no parameter {id}"));
    m.set_param(i, v);
}

/// A module with the given settings, prepared at 48 kHz.
pub fn with(settings: &[(&str, f32)]) -> CompDual {
    let mut m = fresh();
    for (id, v) in settings {
        set(&mut m, id, *v);
    }
    m.reset();
    m
}

/// Renders a stereo signal in blocks of 512 and removes the reported latency, like an offline
/// host render does (spec section 18).
pub fn render2(m: &mut CompDual, l: &[f32], r: &[f32], key: Option<(&[f32], &[f32])>) -> (Vec<f32>, Vec<f32>) {
    m.reset();
    let lat = m.latency();
    let n = l.len();
    let mut a: Vec<f32> = l.to_vec();
    let mut b: Vec<f32> = r.to_vec();
    a.resize(n + lat, 0.0);
    b.resize(n + lat, 0.0);
    let (mut kl, mut kr) = (Vec::new(), Vec::new());
    if let Some((x, y)) = key {
        kl = x.to_vec();
        kr = y.to_vec();
        kl.resize(n + lat, 0.0);
        kr.resize(n + lat, 0.0);
    }
    let mut i = 0;
    while i < n + lat {
        let e = (i + 512).min(n + lat);
        let (la, ra) = (&mut a[i..e], &mut b[i..e]);
        if key.is_some() {
            m.process_keyed(la, ra, &kl[i..e], &kr[i..e]);
        } else {
            m.process(la, ra);
        }
        i = e;
    }
    (a[lat..].to_vec(), b[lat..].to_vec())
}

/// Same signal on both channels; returns the left output.
pub fn render(m: &mut CompDual, x: &[f32]) -> Vec<f32> {
    render2(m, x, x, None).0
}

pub fn sine(freq: f64, amp: f64, phase: f64, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (PI2 * freq * i as f64 / SRF + phase).sin()) as f32).collect()
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

pub fn peak(x: &[f32]) -> f64 {
    x.iter().fold(0.0f64, |m, v| m.max(v.abs() as f64))
}

/// RMS level change of `y` against `x` between two times in seconds (`lvl` in jobs2.py).
pub fn lvl(y: &[f32], x: &[f32], t0: f64, t1: f64) -> f64 {
    let (a, b) = ((t0 * SRF) as usize, (t1 * SRF) as usize);
    db(rms(&y[a..b])) - db(rms(&x[a..b]))
}

/// Amplitude of the component at `freq` in `y[a..b]` (quadrature correlation).
pub fn tone_amp(y: &[f32], freq: f64, a: usize, b: usize) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for i in a..b {
        let ph = PI2 * freq * i as f64 / SRF;
        s += y[i] as f64 * ph.sin();
        c += y[i] as f64 * ph.cos();
    }
    let n = (b - a) as f64;
    (2.0 * s / n).hypot(2.0 * c / n)
}

/// Envelope of a 1 kHz carrier, one value per sample: quadrature demodulation over one period
/// (48 samples) centred on the sample. Stands in for the analytic envelope of jobs2.py.
pub fn env_1k(y: &[f32]) -> Vec<f64> {
    let n = y.len();
    let mut cs = vec![0.0f64; n + 1];
    let mut cc = vec![0.0f64; n + 1];
    for i in 0..n {
        let ph = PI2 * 1000.0 * i as f64 / SRF;
        cs[i + 1] = cs[i] + y[i] as f64 * ph.sin();
        cc[i + 1] = cc[i] + y[i] as f64 * ph.cos();
    }
    (0..n)
        .map(|i| {
            let a = i.saturating_sub(24);
            let b = (i + 24).min(n);
            let w = (b - a) as f64;
            (2.0 * (cs[b] - cs[a]) / w).hypot(2.0 * (cc[b] - cc[a]) / w)
        })
        .collect()
}

pub fn gain_curve_1k(y: &[f32], x: &[f32]) -> Vec<f64> {
    let (ey, ex) = (env_1k(y), env_1k(x));
    ey.iter().zip(ex.iter()).map(|(a, b)| db(*a) - db(*b)).collect()
}

pub fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[s.len() / 2]
}

/// First crossing of `target` after `start`, in ms, linearly interpolated (`cross` in jobs2.py).
pub fn cross(g: &[f64], start: usize, target: f64, rising: bool) -> Option<f64> {
    for i in start..g.len() {
        let hit = if rising { g[i] >= target } else { g[i] <= target };
        if hit {
            if i == start {
                return Some(0.0);
            }
            let (y0, y1) = (g[i - 1], g[i]);
            return Some(((i - start) as f64 - 1.0 + (target - y0) / (y1 - y0)) / SRF * 1000.0);
        }
    }
    None
}

/// The timing stimulus (`release_8s.wav`, spec section 22 and `spec/curves/README.md`): 1 kHz at
/// -30 dBFS throughout, with a 0.2 s burst every 8 s starting at 0.1 s, 1 ms raised-cosine
/// edges. The burst adds 0.5 to the amplitude (the stored curves read -5.49 dBFS in the burst
/// with the fastest attack, which is 0.5 + 0.0316).
pub fn release_8s() -> Vec<f32> {
    let n = 16 * SR;
    (0..n)
        .map(|i| {
            let t = i as f64 / SRF;
            let tb = ((t - 0.1).rem_euclid(8.0)) * 1000.0;
            let e = if tb < 1.0 {
                0.5 * (1.0 - (std::f64::consts::PI * tb).cos())
            } else if tb < 199.0 {
                1.0
            } else if tb < 200.0 {
                0.5 * (1.0 + (std::f64::consts::PI * (tb - 199.0)).cos())
            } else {
                0.0
            };
            ((lin(-30.0) + 0.5 * e) * (PI2 * 1000.0 * t).sin()) as f32
        })
        .collect()
}

pub struct Timing {
    pub pre: f64,
    pub fin: f64,
    pub post: f64,
    pub attack_63: f64,
    pub release_63: f64,
    pub release_90: f64,
    pub gain: Vec<f64>,
}

pub const ON: usize = 8 * SR + SR / 10;
pub const OFF: usize = 8 * SR + 3 * SR / 10;

/// The metrics of jobs2.py for the timing stimulus, taken on the second burst.
pub fn timing(m: &mut CompDual) -> Timing {
    let x = release_8s();
    let y = render(m, &x);
    let g = gain_curve_1k(&y, &x);
    let end = 8 * SR + (7.9 * SRF) as usize;
    let w = SR / 20;
    let pre = median(&g[ON - w..ON]);
    let fin = median(&g[OFF - w..OFF]);
    let post = median(&g[end - SR / 4..end]);
    Timing {
        pre,
        fin,
        post,
        attack_63: cross(&g, ON, pre + 0.63 * (fin - pre), fin > pre).unwrap_or(f64::NAN),
        release_63: cross(&g, OFF, fin + 0.63 * (post - fin), post > fin).unwrap_or(f64::NAN),
        release_90: cross(&g, OFF, fin + 0.9 * (post - fin), post > fin).unwrap_or(f64::NAN),
        gain: g,
    }
}

/// Result table: prints every comparison and fails the test at the end if any is out.
pub struct Table {
    title: String,
    rows: Vec<String>,
    failed: usize,
}

impl Table {
    pub fn new(title: &str) -> Self {
        Self { title: title.to_string(), rows: Vec::new(), failed: 0 }
    }

    /// A level in dB against a stored one.
    pub fn level(&mut self, name: &str, got: f64, want: f64, tol: f64) {
        let ok = (got - want).abs() <= tol;
        self.push(name, format!("{got:9.2}"), format!("{want:9.2}"), format!("{:+.2} dB (tol {tol})", got - want), ok);
    }

    /// A time in ms against a stored one: within `pct` percent or `floor` ms, whichever is larger.
    pub fn time(&mut self, name: &str, got: f64, want: f64, pct: f64, floor: f64) {
        let ok = (got - want).abs() <= (want.abs() * pct / 100.0).max(floor);
        let rel = if want.abs() > 1.0e-9 { format!("{:+.1} %", 100.0 * (got - want) / want) } else { "n/a".to_string() };
        self.push(name, format!("{got:9.2}"), format!("{want:9.2}"), format!("{rel} (tol {pct} % or {floor} ms)"), ok);
    }

    pub fn truth(&mut self, name: &str, ok: bool, note: &str) {
        self.push(name, String::new(), String::new(), note.to_string(), ok);
    }

    fn push(&mut self, name: &str, got: String, want: String, note: String, ok: bool) {
        if !ok {
            self.failed += 1;
        }
        self.rows.push(format!("| {name} | {got} | {want} | {note} | {} |", if ok { "pass" } else { "FAIL" }));
    }

    pub fn finish(self) {
        println!("\n### {}\n| test | measured | stored | difference | result |\n|---|---|---|---|---|", self.title);
        for r in &self.rows {
            println!("{r}");
        }
        assert_eq!(self.failed, 0, "{}: {} comparison(s) out of tolerance", self.title, self.failed);
    }
}
