//! Small complex number types: `Cx` (f64) for design work, `C32` (f32) for FFT buffers.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cx {
    pub re: f64,
    pub im: f64,
}

impl Cx {
    pub const ZERO: Cx = Cx { re: 0.0, im: 0.0 };
    pub const ONE: Cx = Cx { re: 1.0, im: 0.0 };
    #[inline]
    pub const fn new(re: f64, im: f64) -> Self {
        Cx { re, im }
    }
    #[inline]
    pub const fn real(re: f64) -> Self {
        Cx { re, im: 0.0 }
    }
    #[inline]
    pub fn polar(r: f64, theta: f64) -> Self {
        Cx { re: r * theta.cos(), im: r * theta.sin() }
    }
    #[inline]
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    /// Squared magnitude (std::norm).
    #[inline]
    pub fn norm(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    #[inline]
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    #[inline]
    pub fn conj(self) -> Self {
        Cx { re: self.re, im: -self.im }
    }
    /// Principal square root, with the branch cut following the sign of the imaginary part
    /// (as std::sqrt of std::complex).
    pub fn sqrt(self) -> Self {
        let m = self.abs();
        if m == 0.0 {
            return Cx::new(0.0, self.im);
        }
        let re = ((m + self.re) * 0.5).max(0.0).sqrt();
        let im = ((m - self.re) * 0.5).max(0.0).sqrt();
        Cx::new(re, im.copysign(self.im))
    }
    pub fn exp(self) -> Self {
        let e = self.re.exp();
        Cx::new(e * self.im.cos(), e * self.im.sin())
    }
    pub fn ln(self) -> Self {
        Cx::new(self.abs().ln(), self.arg())
    }
    /// z^p for a real p, as exp(p log z) (std::pow (complex, double)).
    pub fn powf(self, p: f64) -> Self {
        if self.re == 0.0 && self.im == 0.0 {
            return if p == 0.0 { Cx::ONE } else { Cx::ZERO };
        }
        (self.ln() * p).exp()
    }
    #[inline]
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

fn logb(x: f64) -> i32 {
    let bits = x.abs().to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    if exp == 0 {
        (x.abs().log2()).floor() as i32
    } else {
        exp - 1023
    }
}

fn scalbn(x: f64, e: i32) -> f64 {
    if e == 0 {
        return x;
    }
    let mut v = x;
    let mut k = e;
    while k > 1000 {
        v *= 2f64.powi(1000);
        k -= 1000;
    }
    while k < -1000 {
        v *= 2f64.powi(-1000);
        k += 1000;
    }
    v * 2f64.powi(k)
}

impl Add for Cx {
    type Output = Cx;
    #[inline]
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
}
impl Sub for Cx {
    type Output = Cx;
    #[inline]
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.re - o.re, self.im - o.im)
    }
}
impl Mul for Cx {
    type Output = Cx;
    #[inline]
    fn mul(self, o: Cx) -> Cx {
        Cx::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}
impl Div for Cx {
    type Output = Cx;
    #[inline]
    fn div(self, o: Cx) -> Cx {
        // Scaled division in the same order as the C++ standard library the original was built
        // with, so near-ties in root grouping resolve the same way.
        let (a, b) = (self.re, self.im);
        let (mut c, mut d) = (o.re, o.im);
        let m = c.abs().max(d.abs());
        let mut e = 0i32;
        if m.is_finite() && m > 0.0 {
            e = logb(m);
            c = scalbn(c, -e);
            d = scalbn(d, -e);
        }
        let den = c * c + d * d;
        Cx::new(scalbn((a * c + b * d) / den, -e), scalbn((b * c - a * d) / den, -e))
    }
}
impl Neg for Cx {
    type Output = Cx;
    #[inline]
    fn neg(self) -> Cx {
        Cx::new(-self.re, -self.im)
    }
}
impl Add<f64> for Cx {
    type Output = Cx;
    #[inline]
    fn add(self, o: f64) -> Cx {
        Cx::new(self.re + o, self.im)
    }
}
impl Sub<f64> for Cx {
    type Output = Cx;
    #[inline]
    fn sub(self, o: f64) -> Cx {
        Cx::new(self.re - o, self.im)
    }
}
impl Mul<f64> for Cx {
    type Output = Cx;
    #[inline]
    fn mul(self, o: f64) -> Cx {
        Cx::new(self.re * o, self.im * o)
    }
}
impl Div<f64> for Cx {
    type Output = Cx;
    #[inline]
    fn div(self, o: f64) -> Cx {
        Cx::new(self.re / o, self.im / o)
    }
}
impl Add<Cx> for f64 {
    type Output = Cx;
    #[inline]
    fn add(self, o: Cx) -> Cx {
        Cx::new(self + o.re, o.im)
    }
}
impl Mul<Cx> for f64 {
    type Output = Cx;
    #[inline]
    fn mul(self, o: Cx) -> Cx {
        Cx::new(self * o.re, self * o.im)
    }
}
impl Div<Cx> for f64 {
    type Output = Cx;
    #[inline]
    fn div(self, o: Cx) -> Cx {
        Cx::real(self) / o
    }
}
impl AddAssign for Cx {
    #[inline]
    fn add_assign(&mut self, o: Cx) {
        *self = *self + o;
    }
}
impl SubAssign for Cx {
    #[inline]
    fn sub_assign(&mut self, o: Cx) {
        *self = *self - o;
    }
}
impl MulAssign for Cx {
    #[inline]
    fn mul_assign(&mut self, o: Cx) {
        *self = *self * o;
    }
}

/// Single-precision complex sample for FFT buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct C32 {
    pub re: f32,
    pub im: f32,
}

impl C32 {
    pub const ZERO: C32 = C32 { re: 0.0, im: 0.0 };
    #[inline]
    pub const fn new(re: f32, im: f32) -> Self {
        C32 { re, im }
    }
    #[inline]
    pub fn conj(self) -> Self {
        C32 { re: self.re, im: -self.im }
    }
    #[inline]
    pub fn norm(self) -> f32 {
        self.re * self.re + self.im * self.im
    }
    #[inline]
    pub fn abs(self) -> f32 {
        self.re.hypot(self.im)
    }
    #[inline]
    pub fn scale(self, k: f32) -> Self {
        C32 { re: self.re * k, im: self.im * k }
    }
    #[inline]
    pub fn mul(self, o: C32) -> Self {
        C32 { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
    #[inline]
    pub fn add(self, o: C32) -> Self {
        C32 { re: self.re + o.re, im: self.im + o.im }
    }
    #[inline]
    pub fn sub(self, o: C32) -> Self {
        C32 { re: self.re - o.re, im: self.im - o.im }
    }
}
