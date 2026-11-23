//! Harmonic stage: a polynomial shaper adding 2nd to 5th harmonics, a smooth rail limiter, both
//! with first-order antiderivative anti-aliasing (ADAA), and a 3.5 Hz DC blocker.
//!
//! A port of `DSP/Adaa.h` and `HarmonicsEngine` in `CompressorCore.h`. The ADAA works on the
//! residual r(x) = f(x) - slope * x only, so the linear part stays unaveraged: with all four
//! harmonics at 0 % the stage returns its input bit for bit.

use std::sync::OnceLock;

const RAIL_SIZE: usize = 4096;
const RAIL_MAX: f64 = 8.0;
const RAIL_STEP: f64 = RAIL_MAX / RAIL_SIZE as f64;

/// The rail limiter f(v) = v / (1 + |v|^8)^(1/8) and a Simpson-integrated, cubic-Hermite
/// antiderivative table.
pub struct RailTable {
    g: [f64; RAIL_SIZE + 1],
    f: [f64; RAIL_SIZE + 1],
}

impl RailTable {
    fn build() -> Self {
        let mut t = RailTable { g: [0.0; RAIL_SIZE + 1], f: [0.0; RAIL_SIZE + 1] };
        for i in 0..=RAIL_SIZE {
            t.f[i] = Self::value(i as f64 * RAIL_STEP);
        }
        t.g[0] = 0.0;
        for i in 0..RAIL_SIZE {
            let mut acc = 0.0;
            let t0 = i as f64 * RAIL_STEP;
            let h = RAIL_STEP / 16.0;
            for k in 0..16 {
                let a = t0 + k as f64 * h;
                let m = a + 0.5 * h;
                let b = a + h;
                acc += (h / 6.0) * (Self::value(a) + 4.0 * Self::value(m) + Self::value(b));
            }
            t.g[i + 1] = t.g[i] + acc;
        }
        t
    }

    #[inline]
    pub fn value(t: f64) -> f64 {
        t / (1.0 + t.abs().powf(8.0)).powf(0.125)
    }

    /// Integral of `value` over [0, t], t >= 0.
    #[inline]
    pub fn integral(&self, t: f64) -> f64 {
        if t >= RAIL_MAX {
            return self.g[RAIL_SIZE] + (t - RAIL_MAX);
        }
        let u = t / RAIL_STEP;
        // `as usize` saturates (NaN and negatives give 0), so the index cannot run off.
        let i = (u as usize).min(RAIL_SIZE - 1);
        let s = u - i as f64;
        let s2 = s * s;
        let s3 = s2 * s;
        let (g0, g1, f0, f1) = (self.g[i], self.g[i + 1], self.f[i], self.f[i + 1]);
        (2.0 * s3 - 3.0 * s2 + 1.0) * g0
            + (s3 - 2.0 * s2 + s) * RAIL_STEP * f0
            + (-2.0 * s3 + 3.0 * s2) * g1
            + (s3 - s2) * RAIL_STEP * f1
    }
}

/// The shared table, built once (on construction of the first module, never on the audio path).
pub fn rail_table() -> &'static RailTable {
    static T: OnceLock<Box<RailTable>> = OnceLock::new();
    T.get_or_init(|| Box::new(RailTable::build()))
}

#[inline]
pub fn rail_limit(v: f64) -> f64 {
    RailTable::value(v)
}

#[inline]
pub fn rail_limit_integral(table: &RailTable, v: f64) -> f64 {
    table.integral(v.abs())
}

/// Zero-delay residual ADAA: the linear term `slope * x` passes as is, the residual is averaged
/// over the segment between consecutive inputs.
#[derive(Clone, Copy, Debug, Default)]
pub struct ZeroDelayResidual {
    x1: f64,
    fr1: f64,
    r1: f64,
    primed: bool,
}

impl ZeroDelayResidual {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn step(&mut self, x: f64, f: f64, big_f: f64, slope: f64) -> f64 {
        let lin = slope * x;
        let r = f - lin;
        let fr = big_f - 0.5 * slope * x * x;
        if !self.primed {
            self.x1 = x;
            self.fr1 = fr;
            self.r1 = r;
            self.primed = true;
            return f;
        }
        let dx = x - self.x1;
        let mut mean = 0.5 * (r + self.r1);
        let scale = max3(1.0, x.abs(), self.x1.abs());
        if dx.abs() > 1.0e-5 * scale {
            mean = (fr - self.fr1) / dx;
            let lo = if self.r1 < r { self.r1 } else { r };
            let hi = if r < self.r1 { self.r1 } else { r };
            let tol = (hi - lo) + 1.0e-6 * (1.0 + r.abs());
            if !(mean >= lo - tol && mean <= hi + tol) {
                mean = 0.5 * (r + self.r1);
            }
        }
        self.x1 = x;
        self.fr1 = fr;
        self.r1 = r;
        lin + mean
    }
}

#[inline]
fn max3(a: f64, b: f64, c: f64) -> f64 {
    let m = if a < b { b } else { a };
    if m < c {
        c
    } else {
        m
    }
}

/// Polynomial weights of the four harmonic controls (each 0..1).
#[inline]
pub fn harmonic_weights(h2: f32, h3: f32, h4: f32, h5: f32) -> [f64; 4] {
    [0.50 * h2 as f64, 0.50 * h3 as f64, 0.25 * h4 as f64, 0.20 * h5 as f64]
}

/// The shaper p(x) = x + g(x) without anti-aliasing (for measurements):
/// g(x) = c2 x^2 + c3 (x^3 - x) + c4 x^4 + c5 (x^5 - x) inside [-1, 1], held outside.
pub fn shaper_naive(x: f64, c: [f64; 4]) -> f64 {
    let [c2, c3, c4, c5] = c;
    if x > 1.0 || x < -1.0 {
        return x + c2 + c4;
    }
    let x2 = x * x;
    let x3 = x2 * x;
    x + c2 * x2 + c3 * (x3 - x) + c4 * (x2 * x2) + c5 * (x3 * x2 - x)
}

/// Stereo harmonic stage.
pub struct Harmonics {
    rail: &'static RailTable,
    dc_r: f32,
    shaper: [ZeroDelayResidual; 2],
    rail_adaa: [ZeroDelayResidual; 2],
    dc_x1: [f32; 2],
    dc_y1: [f32; 2],
}

impl Default for Harmonics {
    fn default() -> Self {
        Self {
            rail: rail_table(),
            dc_r: 0.9995,
            shaper: [ZeroDelayResidual::default(); 2],
            rail_adaa: [ZeroDelayResidual::default(); 2],
            dc_x1: [0.0; 2],
            dc_y1: [0.0; 2],
        }
    }
}

impl Harmonics {
    pub fn prepare(&mut self, sr: f64) {
        self.dc_r = (-2.0 * std::f64::consts::PI * 3.5 / sr).exp() as f32;
        self.reset();
    }

    pub fn reset(&mut self) {
        for s in &mut self.shaper {
            s.reset();
        }
        for s in &mut self.rail_adaa {
            s.reset();
        }
        self.dc_x1 = [0.0; 2];
        self.dc_y1 = [0.0; 2];
    }

    /// One sample of channel `ch` (0 or 1). `h2..h5` are 0..1.
    #[inline]
    pub fn process(&mut self, x: f32, h2: f32, h3: f32, h4: f32, h5: f32, ch: usize) -> f32 {
        let ch = ch & 1;
        let xd = x as f64;
        let [c2, c3, c4, c5] = harmonic_weights(h2, h3, h4, h5);
        let active = (c2 + c3 + c4 + c5) > 1.0e-7;
        let slope = 1.0 - c3 - c5;
        let g0_int = |u: f64| {
            let u2 = u * u;
            let u4 = u2 * u2;
            (c2 / 3.0) * (u2 * u) + c3 * (0.25 * u4 - 0.5 * u2) + (c4 / 5.0) * (u4 * u) + c5 * ((u4 * u2) / 6.0 - 0.5 * u2)
        };
        let mut g_val = 0.0;
        let mut g_int = 0.0;
        if active {
            if xd > 1.0 {
                g_val = c2 + c4;
                g_int = g0_int(1.0) + (c2 + c4) * (xd - 1.0);
            } else if xd < -1.0 {
                g_val = c2 + c4;
                g_int = g0_int(-1.0) + (c2 + c4) * (xd + 1.0);
            } else {
                let x2 = xd * xd;
                let x3 = x2 * xd;
                g_val = c2 * x2 + c3 * (x3 - xd) + c4 * (x2 * x2) + c5 * (x3 * x2 - xd);
                g_int = g0_int(xd);
            }
        }
        let p_val = xd + g_val;
        let p_int = 0.5 * xd * xd + g_int;
        let shaped = self.shaper[ch].step(xd, p_val, p_int, slope);
        // Kept primed while the harmonics are off so automation transitions are click-free.
        let limited =
            self.rail_adaa[ch].step(shaped, rail_limit(shaped), rail_limit_integral(self.rail, shaped), 1.0);
        let ins = limited as f32;
        let out = ins - self.dc_x1[ch] + self.dc_r * self.dc_y1[ch];
        self.dc_x1[ch] = ins;
        self.dc_y1[ch] = out;
        if active {
            out
        } else {
            x
        }
    }
}
