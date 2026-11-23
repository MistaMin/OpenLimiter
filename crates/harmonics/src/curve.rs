//! The static curves and their antiderivatives.
//!
//! Every curve is written in its own input units `u` (the module's input times the drive) as
//! `phi(u) = u + r(u)`: unit small-signal slope, so `r` (the residual) is the only part that
//! is not linear and the only part the anti-aliasing kernels see. `r1` and `r2` are the first
//! and second antiderivatives of `r`, both zero at `u = 0`.
//!
//! `phi = u + rS(u) + G(u)`:
//! * `rS` is the base family: zero for the polynomial generator, `S_b(u) - u` for the soft
//!   saturator and the tape knee, with `S_b(u) = (S(u + b) - S(b)) / S'(b)` the base curve
//!   moved to the operating point `b` (bias) and renormalised to unit slope and zero offset.
//! * `G` is the harmonic generator: a sum of Chebyshev polynomials with their constant and
//!   linear terms removed, `C_n(u) = T_n(u) - T_n(0) - T_n'(0) u`. For `u = cos(theta)`,
//!   `T_n(u) = cos(n theta)` exactly, so a sine at the reference level (`|u| = 1`) gets exactly
//!   harmonic `n` from `C_n` plus a fundamental change from the removed linear term (odd orders
//!   only). Outside `[-1, 1]` the generator is continued with a quadratic taper that brings its
//!   slope to zero over one unit and then holds it constant (C1, bounded).

/// A static curve with unit small-signal slope, given by its residual `r = phi - u`.
pub trait Curve {
    /// `r(u) = phi(u) - u`.
    fn r(&self, u: f64) -> f64;
    /// First antiderivative of `r`, zero at 0.
    fn r1(&self, u: f64) -> f64;
    /// Second antiderivative of `r`, zero at 0.
    fn r2(&self, u: f64) -> f64;
    /// An upper bound on `|r'(u)|` over all `u` (used by the safety checks).
    fn lip(&self) -> f64;
}

pub const FAMILY_POLY: usize = 0;
pub const FAMILY_SAT: usize = 1;
pub const FAMILY_TAPE: usize = 2;

/// Tape knee: linear up to `TAPE_K`, cubic knee of width `TAPE_W` into the ceiling 1.
pub const TAPE_K: f64 = 0.3;
pub const TAPE_W: f64 = 1.5 * (1.0 - TAPE_K);
const TAPE_TOP: f64 = TAPE_K + TAPE_W;

/// Largest bias, in curve units, at bias = 100 %.
pub const BIAS_MAX: f64 = 0.4;

// ---------------------------------------------------------------- base families

/// asinh without the cancellation of the textbook formula for small arguments.
#[inline]
fn asinh(v: f64) -> f64 {
    let a = v.abs();
    let r = if a > 1e8 { a.ln() + std::f64::consts::LN_2 } else { (a + a * a / (1.0 + (1.0 + a * a).sqrt())).ln_1p() };
    r.copysign(v)
}

/// Soft saturator `S(v) = v / sqrt(1 + v^2)` and its antiderivatives (constants chosen so all
/// vanish at 0), plus `S'`.
#[inline]
fn sat(v: f64) -> (f64, f64, f64, f64) {
    let q = (1.0 + v * v).sqrt();
    let s = v / q;
    let s1 = v * v / (q + 1.0); // sqrt(1 + v^2) - 1
    let s2 = 0.5 * (v * q + asinh(v)) - v;
    let ds = 1.0 / (q * q * q);
    (s, s1, s2, ds)
}

/// Tape knee: identity to `K`, then `a - e^3 / (3 W^2)` (`e = a - K`) up to `K + W`, then 1.
#[inline]
fn tape(v: f64) -> (f64, f64, f64, f64) {
    let a = v.abs();
    let w2 = TAPE_W * TAPE_W;
    let (s, s1, s2, ds) = if a <= TAPE_K {
        (a, 0.5 * a * a, a * a * a / 6.0, 1.0)
    } else if a < TAPE_TOP {
        let e = a - TAPE_K;
        (a - e * e * e / (3.0 * w2), 0.5 * a * a - e.powi(4) / (12.0 * w2), a * a * a / 6.0 - e.powi(5) / (60.0 * w2), 1.0 - e * e / w2)
    } else {
        let e = TAPE_W;
        let s1t = 0.5 * TAPE_TOP * TAPE_TOP - e.powi(4) / (12.0 * w2);
        let s2t = TAPE_TOP.powi(3) / 6.0 - e.powi(5) / (60.0 * w2);
        let d = a - TAPE_TOP;
        (1.0, s1t + d, s2t + s1t * d + 0.5 * d * d, 0.0)
    };
    // s and s2 are odd, s1 and ds even
    (s.copysign(v), s1, s2.copysign(v), ds)
}

#[inline]
fn base(family: usize, v: f64) -> (f64, f64, f64, f64) {
    match family {
        FAMILY_SAT => sat(v),
        FAMILY_TAPE => tape(v),
        _ => (v, 0.5 * v * v, v * v * v / 6.0, 1.0),
    }
}

// ---------------------------------------------------------------- the module's curve

/// Generator coefficients `a_n` (n = 2..5) for wanted harmonic ratios `rho_n` (amplitude of
/// harmonic `n` over the fundamental, for a sine peaking at the reference level).
///
/// Odd orders are compressive (`-C_3`, `+C_5`), so the fundamental at the reference level is
/// `1 - 3 a_3 - 5 a_5`; solving `a_n = rho_n (1 - 3 a_3 - 5 a_5)` gives
/// `a_n = rho_n / (1 + 3 rho_3 + 5 rho_5)`.
pub fn generator_coefficients(rho: [f64; 4]) -> [f64; 4] {
    let f = 1.0 / (1.0 + 3.0 * rho[1] + 5.0 * rho[3]);
    [rho[0] * f, rho[1] * f, rho[2] * f, rho[3] * f]
}

/// The module's curve: base family at a bias plus the harmonic generator.
#[derive(Clone, Copy, Debug)]
pub struct ShapeCurve {
    family: usize,
    /// Generator polynomial `P(u) = sum p[k] u^k`, k = 2..5 (p[0] = p[1] = 0).
    p: [f64; 6],
    // generator values at the taper joins u = +1 and u = -1: P, P', P1, P2
    jp: [f64; 4],
    jn: [f64; 4],
    // the same at the far end of the tapers (u = +2, -2): G, G1, G2
    ep: [f64; 3],
    en: [f64; 3],
    // base family at the bias: S(b), S1(b), S2(b), 1/S'(b)
    b: f64,
    sb: [f64; 3],
    inv_sd: f64,
    lip: f64,
    has_gen: bool,
}

impl ShapeCurve {
    /// `family` a `FAMILY_*`, `rho` the harmonic ratios for orders 2..5 (linear), `bias` in
    /// curve units (`-BIAS_MAX..BIAS_MAX`; ignored by the polynomial family).
    pub fn new(family: usize, rho: [f64; 4], bias: f64) -> Self {
        let mut c = Self {
            family: family.min(FAMILY_TAPE),
            p: [0.0; 6],
            jp: [0.0; 4],
            jn: [0.0; 4],
            ep: [0.0; 3],
            en: [0.0; 3],
            b: 0.0,
            sb: [0.0; 3],
            inv_sd: 1.0,
            lip: 1.0,
            has_gen: false,
        };
        c.set_generator(rho);
        c.set_bias(bias);
        c
    }

    pub fn set_generator(&mut self, rho: [f64; 4]) {
        let a = generator_coefficients(rho);
        // 2 u^2 ; -4 u^3 ; 8 u^4 - 8 u^2 ; 16 u^5 - 20 u^3
        let p = [0.0, 0.0, 2.0 * a[0] - 8.0 * a[2], -4.0 * a[1] - 20.0 * a[3], 8.0 * a[2], 16.0 * a[3]];
        self.p = p;
        self.has_gen = a.iter().any(|v| *v != 0.0);
        self.jp = Self::poly_all(&p, 1.0);
        self.jn = Self::poly_all(&p, -1.0);
        self.ep = Self::taper_end(&self.jp, 1.0);
        self.en = Self::taper_end(&self.jn, -1.0);
        self.update_lip();
    }

    pub fn set_bias(&mut self, bias: f64) {
        let b = if self.family == FAMILY_POLY { 0.0 } else { bias.clamp(-BIAS_MAX, BIAS_MAX) };
        self.b = b;
        let (s, s1, s2, ds) = base(self.family, b);
        self.sb = [s, s1, s2];
        self.inv_sd = 1.0 / ds.max(1e-6);
        self.update_lip();
    }

    pub fn bias(&self) -> f64 {
        self.b
    }

    fn update_lip(&mut self) {
        let lg: f64 = (2..6).map(|k| k as f64 * self.p[k].abs()).sum();
        let ls = if self.family == FAMILY_POLY { 0.0 } else { self.inv_sd.max(1.0) };
        self.lip = 1e-9 + lg + ls;
    }

    #[inline]
    fn poly_all(p: &[f64; 6], u: f64) -> [f64; 4] {
        let (mut v, mut d, mut i1, mut i2) = (0.0, 0.0, 0.0, 0.0);
        for k in (2..6).rev() {
            let pk = p[k];
            let kf = k as f64;
            v = v * u + pk;
            d = d * u + kf * pk;
            i1 = i1 * u + pk / (kf + 1.0);
            i2 = i2 * u + pk / ((kf + 1.0) * (kf + 2.0));
        }
        // v holds sum p_k u^(k-2): shift to the right powers
        let u2 = u * u;
        [v * u2, d * u, i1 * u2 * u, i2 * u2 * u2]
    }

    /// Taper `G = P(s) + P'(s) (t - s t^2 / 2)` (`t = u - s`, `s = +-1`) evaluated at `t = s`.
    #[inline]
    fn taper_end(j: &[f64; 4], s: f64) -> [f64; 3] {
        let [p, d, i1, i2] = *j;
        let t = s;
        [p + d * (t - s * t * t / 2.0), i1 + p * t + d * (t * t / 2.0 - s * t * t * t / 6.0), i2 + i1 * t + p * t * t / 2.0 + d * (t * t * t / 6.0 - s * t.powi(4) / 24.0)]
    }

    /// Generator `G`, `G1`, `G2` at `u`.
    #[inline]
    fn gen(&self, u: f64) -> (f64, f64, f64) {
        if !self.has_gen {
            return (0.0, 0.0, 0.0);
        }
        if (-1.0..=1.0).contains(&u) {
            let a = Self::poly_all(&self.p, u);
            return (a[0], a[2], a[3]);
        }
        let (s, j, e) = if u > 1.0 { (1.0, &self.jp, &self.ep) } else { (-1.0, &self.jn, &self.en) };
        let t = u - s;
        if t.abs() <= 1.0 {
            let [p, d, i1, i2] = *j;
            let t2 = t * t;
            (p + d * (t - s * t2 / 2.0), i1 + p * t + d * (t2 / 2.0 - s * t2 * t / 6.0), i2 + i1 * t + p * t2 / 2.0 + d * (t2 * t / 6.0 - s * t2 * t2 / 24.0))
        } else {
            let dt = t - s;
            (e[0], e[1] + e[0] * dt, e[2] + e[1] * dt + 0.5 * e[0] * dt * dt)
        }
    }

    /// Base residual `rS`, `RS1`, `RS2` at `u`.
    #[inline]
    fn base_res(&self, u: f64) -> (f64, f64, f64) {
        if self.family == FAMILY_POLY {
            return (0.0, 0.0, 0.0);
        }
        let (s, s1, s2, _) = base(self.family, u + self.b);
        let [sb, s1b, s2b] = self.sb;
        let k = self.inv_sd;
        let phi = (s - sb) * k;
        let phi1 = (s1 - s1b - sb * u) * k;
        let phi2 = (s2 - s2b - s1b * u - 0.5 * sb * u * u) * k;
        (phi - u, phi1 - 0.5 * u * u, phi2 - u * u * u / 6.0)
    }

    /// `r`, `r1`, `r2` together.
    #[inline]
    pub fn all(&self, u: f64) -> (f64, f64, f64) {
        let (a, b, c) = self.base_res(u);
        let (d, e, f) = self.gen(u);
        (a + d, b + e, c + f)
    }

    /// The full curve `phi(u)`.
    #[inline]
    pub fn phi(&self, u: f64) -> f64 {
        u + self.r(u)
    }
}

impl Curve for ShapeCurve {
    #[inline]
    fn r(&self, u: f64) -> f64 {
        let a = if self.family == FAMILY_POLY {
            0.0
        } else {
            (base(self.family, u + self.b).0 - self.sb[0]) * self.inv_sd - u
        };
        a + self.gen(u).0
    }
    #[inline]
    fn r1(&self, u: f64) -> f64 {
        self.all(u).1
    }
    #[inline]
    fn r2(&self, u: f64) -> f64 {
        self.all(u).2
    }
    #[inline]
    fn lip(&self) -> f64 {
        self.lip
    }
}
