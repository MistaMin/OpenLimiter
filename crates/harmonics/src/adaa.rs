//! Anti-aliasing kernels for the residual of a [`Curve`].
//!
//! Each engine takes the curve input `xs` (module units) and the drive `g`, forms `u = g xs`,
//! and returns the *added* signal `h` in module units, aligned so that the wet curve output is
//! `xs[n - D] + h[n]` with `D` the engine's integer delay. The dry signal is never touched:
//! the module adds `h` to its own delayed copy of the input.
//!
//! Every engine evaluates the antiderivatives at all its nodes with the curve of the current
//! sample, so the result is always the kernel applied to one fixed curve, even while drive,
//! bias or harmonic amounts move. That replaces the shipping `Hold` switch (see the report).

use crate::curve::Curve;

/// Close-sample switch for the first-order mean, relative to `max(1, |u|)` (shipping value).
pub const CLOSE1: f64 = 1e-5;
/// Close-sample switch for the triangle kernel, relative to `max(1, |c|)` (plugin port value).
pub const CLOSE2: f64 = 1e-3;
/// Blend knee: compression at which the linear path is half way to the averaged path.
pub const KNEE: f64 = 0.3;
/// The timing all-pass coefficient `(a + z^-1) / (1 + a z^-1)`.
pub const AP_A: f64 = 1.0 / 3.0;

/// Mean of `r` over the straight line from `a` to `b`, with the close-sample fallback and the
/// safety check. Returns the mean in curve units.
#[inline]
pub fn mean1<C: Curve>(c: &C, a: f64, b: f64) -> f64 {
    let d = b - a;
    let m = 0.5 * (a + b);
    let scale = 1f64.max(a.abs()).max(b.abs());
    if d.abs() <= CLOSE1 * scale {
        return c.r(m);
    }
    let q = (c.r1(b) - c.r1(a)) / d;
    // |mean(r) - r(m)| <= lip |d| / 4 for any curve with |r'| <= lip; a value outside twice
    // that is rounding noise from two huge, nearly equal antiderivatives.
    let rm = c.r(m);
    if q.is_finite() && (q - rm).abs() <= 0.5 * c.lip() * d.abs() + 1e-12 * (1.0 + rm.abs()) {
        q
    } else {
        rm
    }
}

const GL_T: [f64; 5] = [-0.906_179_845_938_664, -0.538_469_310_105_683, 0.0, 0.538_469_310_105_683, 0.906_179_845_938_664];
const GL_W: [f64; 5] = [0.236_926_885_056_189, 0.478_628_670_499_366, 0.568_888_888_888_889, 0.478_628_670_499_366, 0.236_926_885_056_189];

/// `W(c, e) = integral over s in [0, 1] of (1 - s) r(c + (e - c) s)`, with `r(c)`, `R1(c)`,
/// `R2(c)` given. Quotient for spread above `CLOSE2 max(1, |c|)`, 5-point Gauss-Legendre below
/// it or when the quotient fails the safety check.
#[inline]
pub fn tri_half<C: Curve>(cv: &C, c: f64, rc: f64, r1c: f64, r2c: f64, e: f64) -> f64 {
    let d = e - c;
    if d.abs() > CLOSE2 * 1f64.max(c.abs()) {
        let q = (cv.r2(e) - r2c - d * r1c) / (d * d);
        // 2W is a weighted mean of r with weight 2(1 - s) (centroid s = 1/3):
        // |2W - r(c)| <= lip |d| / 3.
        if q.is_finite() && (2.0 * q - rc).abs() <= (2.0 / 3.0) * cv.lip() * d.abs() + 1e-12 * (1.0 + rc.abs()) {
            return q;
        }
    }
    let mut acc = 0.0;
    for i in 0..5 {
        let s = 0.5 * (1.0 + GL_T[i]);
        acc += 0.5 * GL_W[i] * (1.0 - s) * cv.r(c + d * s);
    }
    acc
}

/// The shipping blend weight `w = c^8 / (c^8 + 0.3^8)` with the compression `c` taken from the
/// odd part of the residual, `r_odd = (r(u) - r(-u)) / 2` (identical to the shipping `1 - f/x`
/// for symmetric curves). For a biased curve the full residual differs between half cycles, and
/// a weight that moves at the fundamental times the linear part would make DC and even harmonics.
#[inline]
fn blend_weight<C: Curve>(cv: &C, r: f64, u: f64) -> f64 {
    if u.abs() <= 1e-9 {
        return 0.0;
    }
    let c = -0.5 * (r - cv.r(-u)) / u;
    let c2 = c * c;
    let c4 = c2 * c2;
    let c8 = c4 * c4;
    let k2 = KNEE * KNEE;
    let k8 = k2 * k2 * k2 * k2;
    c8 / (c8 + k8)
}

/// First-order all-pass with the residual ADAA timing.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllPass {
    x1: f64,
    y1: f64,
}

impl AllPass {
    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = AP_A * (x - self.y1) + self.x1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    fn flush(&mut self) {
        if self.y1.abs() < 1e-30 {
            self.y1 = 0.0;
        }
        if self.x1.abs() < 1e-30 {
            self.x1 = 0.0;
        }
    }
}

/// Residual first-order ADAA, delay 1.
///
/// The shipping method computes `y = AP(slope x) + mean(r) + w (avg2(x) - AP(slope x))`. Here
/// the linear part is the module's dry signal delayed by one whole sample, and the all-pass
/// moves onto the residual: `h = AP(mean(r)) + w (AP(avg2(xs)) - xs[n-1])`. At low
/// frequencies the all-pass delays by half a sample, the mean is centred half a sample back,
/// so the residual lines up with the one-sample dry delay; in deep saturation (`w = 1`) the
/// wet output `xs[n-1] + h` is `AP(plain ADAA)`, as in the shipping method.
#[derive(Clone, Copy, Debug, Default)]
pub struct Adaa1 {
    x1: f64,
    w1: f64,
    ap_r: AllPass,
    ap_l: AllPass,
    /// Use the blend weight (shipping behaviour). Off only for measurements.
    pub no_blend: bool,
}

impl Adaa1 {
    pub const DELAY: usize = 1;

    pub fn reset(&mut self) {
        let nb = self.no_blend;
        *self = Self::default();
        self.no_blend = nb;
    }

    #[inline]
    pub fn step<C: Curve>(&mut self, c: &C, xs: f64, g: f64) -> f64 {
        let u0 = g * xs;
        let u1 = g * self.x1;
        let mean = mean1(c, u1, u0) / g;
        let ar = self.ap_r.run(mean);
        let al = self.ap_l.run(0.5 * (xs + self.x1));
        let w = if self.no_blend { 0.0 } else { self.w1 };
        let h = ar + w * (al - self.x1);
        // the weight for the next output, from the sample it will be centred on
        self.w1 = if self.no_blend { 0.0 } else { blend_weight(c, c.r(u0), u0) };
        self.x1 = xs;
        h
    }

    pub fn flush(&mut self) {
        self.ap_r.flush();
        self.ap_l.flush();
        if self.x1.abs() < 1e-30 {
            self.x1 = 0.0;
        }
    }
}

/// Residual triangle ADAA2, delay 1: the triangle kernel on the residual only, an integer
/// one-sample delay on the linear part, no all-pass and no inverse filter (the experiment the
/// November 2026 page lists as untested). With the same blend:
/// `h = W(u1, u0) + W(u1, u2) + w (tri(xs) - xs[n-1])`, `tri = (x0 + 4 x1 + x2) / 6`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Adaa2 {
    x1: f64,
    x2: f64,
    pub no_blend: bool,
}

impl Adaa2 {
    pub const DELAY: usize = 1;

    pub fn reset(&mut self) {
        let nb = self.no_blend;
        *self = Self::default();
        self.no_blend = nb;
    }

    #[inline]
    pub fn step<C: Curve>(&mut self, cv: &C, xs: f64, g: f64) -> f64 {
        let u0 = g * xs;
        let u1 = g * self.x1;
        let u2 = g * self.x2;
        let rc = cv.r(u1);
        let r1c = cv.r1(u1);
        let r2c = cv.r2(u1);
        let tri = (tri_half(cv, u1, rc, r1c, r2c, u0) + tri_half(cv, u1, rc, r1c, r2c, u2)) / g;
        let w = if self.no_blend { 0.0 } else { blend_weight(cv, rc, u1) };
        let lin_tri = (xs + 4.0 * self.x1 + self.x2) / 6.0;
        let h = tri + w * (lin_tri - self.x1);
        self.x2 = self.x1;
        self.x1 = xs;
        h
    }

    pub fn flush(&mut self) {
        if self.x1.abs() < 1e-30 {
            self.x1 = 0.0;
        }
        if self.x2.abs() < 1e-30 {
            self.x2 = 0.0;
        }
    }
}

/// No anti-aliasing: `h = r(g xs) / g`, delay 0.
#[inline]
pub fn naive<C: Curve>(c: &C, xs: f64, g: f64) -> f64 {
    c.r(g * xs) / g
}

/// The even part of a curve's residual, `(r(u) + r(-u)) / 2`, with its antiderivatives. The
/// module DC-blocks only this part: DC comes from the even part, while the odd part carries
/// the compression of the fundamental, which must stay in phase with the dry signal down to
/// the lowest frequencies.
pub struct EvenPart<'a, C: Curve>(pub &'a C);

impl<C: Curve> Curve for EvenPart<'_, C> {
    #[inline]
    fn r(&self, u: f64) -> f64 {
        0.5 * (self.0.r(u) + self.0.r(-u))
    }
    #[inline]
    fn r1(&self, u: f64) -> f64 {
        0.5 * (self.0.r1(u) - self.0.r1(-u))
    }
    #[inline]
    fn r2(&self, u: f64) -> f64 {
        0.5 * (self.0.r2(u) + self.0.r2(-u))
    }
    #[inline]
    fn lip(&self) -> f64 {
        self.0.lip()
    }
}
