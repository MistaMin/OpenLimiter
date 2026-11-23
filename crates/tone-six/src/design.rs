//! Coefficient design for every filter shape. All in f64, no allocation, no panics.
//!
//! Spec references: source F in `spec/MASTER_BRIEF.md` (sections 2 and 4.3) and its parameter
//! file in `spec/params/` (section 6 for the low cut, section 7 for the EQ). The shape rules
//! below were fitted by this implementer to the stored multitone responses
//! (`measurements/srcF/round1_all.json` and `measurements/srcF/round5/jobs_s0.json`,
//! field `resp_L_db`); see REPORT.md for the fit and every free choice.

use core::f64::consts::{LN_2, PI};

/// One second-order section, `a[0]` is always 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sos {
    pub b: [f64; 3],
    pub a: [f64; 3],
}

pub const IDENTITY: Sos = Sos { b: [1.0, 0.0, 0.0], a: [1.0, 0.0, 0.0] };

/// Up to two sections in series (the 3 dB/oct shelf needs two first-order sections).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cascade {
    pub n: usize,
    pub s: [Sos; 2],
}

impl Cascade {
    pub const IDENTITY: Cascade = Cascade { n: 0, s: [IDENTITY, IDENTITY] };
    fn one(s: Sos) -> Self {
        Cascade { n: 1, s: [s, IDENTITY] }
    }
    fn two(a: Sos, b: Sos) -> Self {
        Cascade { n: 2, s: [a, b] }
    }
    /// Magnitude in dB at `f` Hz (for tests and displays).
    pub fn magnitude_db(&self, f: f64, fs: f64) -> f64 {
        let mut m = 1.0;
        for k in 0..self.n.min(2) {
            m *= sos_magnitude(&self.s[k], f, fs);
        }
        20.0 * m.max(1e-300).log10()
    }
}

pub fn sos_magnitude(s: &Sos, f: f64, fs: f64) -> f64 {
    let w = 2.0 * PI * f / fs;
    let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
    let nr = s.b[0] + s.b[1] * c1 + s.b[2] * c2;
    let ni = -(s.b[1] * s1 + s.b[2] * s2);
    let dr = s.a[0] + s.a[1] * c1 + s.a[2] * c2;
    let di = -(s.a[1] * s1 + s.a[2] * s2);
    ((nr * nr + ni * ni) / (dr * dr + di * di).max(1e-300)).sqrt()
}

/// The band shapes, in the order of the `Type` choice labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Bell,
    Reject,
    LowShelf3,
    LowShelf6,
    LowShelf12,
    HighShelf3,
    HighShelf6,
    HighShelf12,
    HighPass,
    LowPass,
}

pub const SHAPE_LABELS: &[&str] = &[
    "Bell",
    "Band reject",
    "Low shelf 3 dB/oct",
    "Low shelf 6 dB/oct",
    "Low shelf 12 dB/oct",
    "High shelf 3 dB/oct",
    "High shelf 6 dB/oct",
    "High shelf 12 dB/oct",
    "High-pass 12 dB/oct",
    "Low-pass 12 dB/oct",
];

impl Shape {
    pub fn from_index(i: usize) -> Shape {
        match i {
            0 => Shape::Bell,
            1 => Shape::Reject,
            2 => Shape::LowShelf3,
            3 => Shape::LowShelf6,
            4 => Shape::LowShelf12,
            5 => Shape::HighShelf3,
            6 => Shape::HighShelf6,
            7 => Shape::HighShelf12,
            8 => Shape::HighPass,
            _ => Shape::LowPass,
        }
    }
    /// Shapes whose effect is set by the band gain (identity at 0 dB).
    pub fn uses_gain(self) -> bool {
        !matches!(self, Shape::Reject | Shape::HighPass | Shape::LowPass)
    }
}

/// Bell widths, octaves (brief section 4.3: 0.10 to 4.00).
pub const BELL_WIDTH: (f64, f64) = (0.10, 4.00);
/// Band-reject widths, octaves (F table section 7: 0.02 to 1.00 on the outer bands).
pub const REJECT_WIDTH: (f64, f64) = (0.02, 1.00);
/// Half the octave distance between the two first-order sections of the 3 dB/oct shelf.
/// Fitted: 2.78 octaves apart (2^1.39 each side of the set frequency), max error 0.013 dB on
/// the 50 Hz and 200 Hz low-shelf cases (F table section 7, round 5 bullet "LO1 shelf 3 dB/oct").
pub const SHELF3_HALF_SPAN_OCT: f64 = 1.39;
/// Pole Q of the 12 dB/oct shelf (fitted; reproduces the +0.55 dB overshoot and -0.55 dB dip).
pub const SHELF12_Q: f64 = 0.94;
/// Highest corner a bilinear (pre-warped) design may use, as a fraction of the sample rate.
pub const BLT_CORNER_LIMIT: f64 = 0.45;
/// Highest centre for the magnitude-fitted bell / reject design.
pub const FIT_CENTRE_LIMIT: f64 = 0.49;

/// Q from a bandwidth in octaves (between half-gain points for a bell).
pub fn q_from_octaves(bw: f64) -> f64 {
    1.0 / (2.0 * (LN_2 * 0.5 * bw).sinh())
}

/// Designs one band. `gain_db`, `width_oct` and `depth_db` are used only where they apply.
pub fn band(shape: Shape, fs: f64, freq: f64, gain_db: f64, width_oct: f64, depth_db: f64) -> Cascade {
    let fs = if fs.is_finite() && fs > 1000.0 { fs } else { 48000.0 };
    let freq = if freq.is_finite() { freq.max(1.0) } else { 1000.0 };
    let g = if gain_db.is_finite() { gain_db } else { 0.0 };
    match shape {
        Shape::Bell => {
            if g == 0.0 {
                return Cascade::IDENTITY;
            }
            Cascade::one(bell(fs, freq, g, clamp(width_oct, BELL_WIDTH)))
        }
        Shape::Reject => {
            let d = if depth_db.is_finite() { depth_db.min(0.0) } else { 0.0 };
            if d == 0.0 {
                return Cascade::IDENTITY;
            }
            Cascade::one(reject(fs, freq, d, clamp(width_oct, REJECT_WIDTH)))
        }
        Shape::LowShelf3 | Shape::HighShelf3 => {
            if g == 0.0 {
                return Cascade::IDENTITY;
            }
            let high = shape == Shape::HighShelf3;
            let r = (SHELF3_HALF_SPAN_OCT * LN_2).exp();
            Cascade::two(shelf1(fs, freq / r, g * 0.5, high), shelf1(fs, freq * r, g * 0.5, high))
        }
        Shape::LowShelf6 | Shape::HighShelf6 => {
            if g == 0.0 {
                return Cascade::IDENTITY;
            }
            Cascade::one(shelf1(fs, freq, g, shape == Shape::HighShelf6))
        }
        Shape::LowShelf12 | Shape::HighShelf12 => {
            if g == 0.0 {
                return Cascade::IDENTITY;
            }
            Cascade::one(shelf2(fs, freq, g, shape == Shape::HighShelf12))
        }
        Shape::HighPass => Cascade::one(butter2(fs, freq, false)),
        Shape::LowPass => Cascade::one(butter2(fs, freq, true)),
    }
}

/// First-order high-pass, -3 dB at `freq` (F table section 6), bilinear with pre-warping.
pub fn low_cut(fs: f64, freq: f64) -> Sos {
    let k = warp(fs, freq);
    let n = 1.0 / (1.0 + k);
    Sos { b: [n, -n, 0.0], a: [1.0, (k - 1.0) * n, 0.0] }
}

fn clamp(v: f64, r: (f64, f64)) -> f64 {
    if v.is_finite() {
        v.max(r.0).min(r.1)
    } else {
        r.1.min(2.0).max(r.0)
    }
}

/// tan(pi f / fs) with the corner limited below Nyquist.
fn warp(fs: f64, f: f64) -> f64 {
    let f = f.max(0.01).min(BLT_CORNER_LIMIT * fs);
    (PI * f / fs).tan()
}

/// Bilinear map of an analogue section c2 s^2 + c1 s + c0 (s normalised so the corner is 1).
fn blt2(k: f64, n: [f64; 3], d: [f64; 3]) -> Sos {
    let m = |c: [f64; 3]| [c[0] + c[1] * k + c[2] * k * k, 2.0 * (c[2] * k * k - c[0]), c[0] - c[1] * k + c[2] * k * k];
    let b = m(n);
    let a = m(d);
    let a0 = a[0];
    Sos { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [1.0, a[1] / a0, a[2] / a0] }
}

/// First-order shelf, half the gain (in dB) at `fc`.
fn shelf1(fs: f64, fc: f64, gain_db: f64, high: bool) -> Sos {
    let sg = 10f64.powf(gain_db / 40.0); // sqrt of the linear gain
    let k = warp(fs, fc);
    let (b0, b1, a0, a1) = if high {
        // H(s) = (s sg + 1) / (s / sg + 1)
        (sg + k, k - sg, 1.0 / sg + k, k - 1.0 / sg)
    } else {
        // H(s) = (s + sg) / (s + 1 / sg)
        (1.0 + k * sg, k * sg - 1.0, 1.0 + k / sg, k / sg - 1.0)
    };
    Sos { b: [b0 / a0, b1 / a0, 0.0], a: [1.0, a1 / a0, 0.0] }
}

/// Second-order shelf (12 dB/oct), half the gain at `fc`, pole Q `SHELF12_Q`.
fn shelf2(fs: f64, fc: f64, gain_db: f64, high: bool) -> Sos {
    let a = 10f64.powf(gain_db / 40.0);
    let sa = a.sqrt();
    let q = SHELF12_Q;
    let k = warp(fs, fc);
    if high {
        blt2(k, [a * a, a * sa / q, a], [1.0, sa / q, a])
    } else {
        blt2(k, [a, a * sa / q, a * a], [a, sa / q, 1.0])
    }
}

/// Second-order Butterworth cut (F table section 7: "Cut: Butterworth").
fn butter2(fs: f64, fc: f64, low_pass: bool) -> Sos {
    let k = warp(fs, fc);
    let n = if low_pass { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    blt2(k, n, [1.0, core::f64::consts::SQRT_2, 1.0])
}

/// Bell: analogue prototype (s^2 + s A/Q + 1) / (s^2 + s/(A Q) + 1), A = 10^(G/40), made
/// digital by magnitude fitting (poles by impulse invariance, numerator matched at 0 Hz, at the
/// centre and at Nyquist). A cut is the exact inverse of the boost of the same size, so
/// "-12 dB is the exact mirror" (F table section 7) holds.
fn bell(fs: f64, f0: f64, gain_db: f64, bw: f64) -> Sos {
    let boost = bell_boost(fs, f0, gain_db.abs(), bw);
    if gain_db >= 0.0 {
        boost
    } else {
        let b0 = boost.b[0];
        Sos {
            b: [1.0 / b0, boost.a[1] / b0, boost.a[2] / b0],
            a: [1.0, boost.b[1] / b0, boost.b[2] / b0],
        }
    }
}

fn bell_boost(fs: f64, f0: f64, gain_db: f64, bw: f64) -> Sos {
    let f0 = f0.min(FIT_CENTRE_LIMIT * fs);
    let a = 10f64.powf(gain_db / 40.0);
    let q = q_from_octaves(bw);
    let h2 = |f: f64| {
        let x = f / f0;
        let re = 1.0 - x * x;
        let num = re * re + (x * a / q) * (x * a / q);
        let den = re * re + (x / (a * q)) * (x / (a * q));
        num / den
    };
    fit_section(fs, f0, 1.0 / (2.0 * a * q), &h2)
}

/// Band reject with its own depth: (s^2 + s D/Q + 1) / (s^2 + s/Q + 1), D = 10^(depth/20).
/// Q = Q(width) / sqrt(2): fitted to the five stored reject cases (ratio 0.70 to 0.73).
fn reject(fs: f64, f0: f64, depth_db: f64, bw: f64) -> Sos {
    let f0 = f0.min(FIT_CENTRE_LIMIT * fs);
    let d = 10f64.powf(depth_db / 20.0);
    let q = q_from_octaves(bw) / core::f64::consts::SQRT_2;
    let h2 = |f: f64| {
        let x = f / f0;
        let re = 1.0 - x * x;
        (re * re + (x * d / q) * (x * d / q)) / (re * re + (x / q) * (x / q))
    };
    fit_section(fs, f0, 1.0 / (2.0 * q), &h2)
}

/// Magnitude-fitted second-order section. Poles: impulse invariant from the analogue poles
/// (natural frequency `f0`, damping `zeta`). Numerator: |H|^2 equal to the analogue value at
/// 0 Hz, at `f0` and at Nyquist. Zeros outside the unit disc are reflected inside so the
/// section stays minimum phase (the magnitude is unchanged).
fn fit_section(fs: f64, f0: f64, zeta: f64, h2: &dyn Fn(f64) -> f64) -> Sos {
    let w0 = 2.0 * PI * f0 / fs;
    let e = (-zeta * w0).exp();
    let a1 = if zeta <= 1.0 {
        -2.0 * e * ((1.0 - zeta * zeta).sqrt() * w0).cos()
    } else {
        -2.0 * e * ((zeta * zeta - 1.0).sqrt() * w0).cosh()
    };
    let a2 = e * e;
    let big_a0 = (1.0 + a1 + a2) * (1.0 + a1 + a2);
    let big_a1 = (1.0 - a1 + a2) * (1.0 - a1 + a2);
    let big_a2 = -4.0 * a2;
    let p0 = (w0 * 0.5).cos().powi(2);
    let p1 = (w0 * 0.5).sin().powi(2);
    let p2 = 4.0 * p0 * p1;
    let big_b0 = big_a0 * h2(0.0);
    let big_b1 = big_a1 * h2(fs * 0.5);
    let big_b2 = if p2 > 0.0 {
        (h2(f0) * (big_a0 * p0 + big_a1 * p1 + big_a2 * p2) - big_b0 * p0 - big_b1 * p1) / p2
    } else {
        0.0
    };
    let r0 = big_b0.max(0.0).sqrt();
    let r1 = big_b1.max(0.0).sqrt();
    let w = 0.5 * (r0 + r1);
    let b0 = 0.5 * (w + (w * w + big_b2).max(0.0).sqrt());
    let b1 = 0.5 * (r0 - r1);
    let b2 = if b0 != 0.0 { -big_b2 / (4.0 * b0) } else { 0.0 };
    let mut s = Sos { b: [b0, b1, b2], a: [1.0, a1, a2] };
    reflect_zeros(&mut s);
    s
}

/// Keeps the zeros of a section inside (or on) the unit disc without changing |H|.
pub fn reflect_zeros(s: &mut Sos) {
    let [b0, b1, b2] = s.b;
    if b0 == 0.0 || b2 == 0.0 {
        return;
    }
    let disc = b1 * b1 - 4.0 * b0 * b2;
    if disc < 0.0 {
        // complex pair, radius^2 = b2 / b0
        if b2 / b0 > 1.0 {
            s.b = [b2, b1, b0];
        }
    } else {
        let sq = disc.sqrt();
        let mut r1 = (-b1 + sq) / (2.0 * b0);
        let mut r2 = (-b1 - sq) / (2.0 * b0);
        let mut g = b0;
        if r1.abs() > 1.0 {
            g *= r1.abs();
            r1 = 1.0 / r1;
        }
        if r2.abs() > 1.0 {
            g *= r2.abs();
            r2 = 1.0 / r2;
        }
        s.b = [g, -g * (r1 + r2), g * r1 * r2];
    }
}
