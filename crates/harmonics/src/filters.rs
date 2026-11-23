//! Small filters: second-order Butterworth low and high pass, the DC blocker, and the
//! BS.1770 K-weighting the auto gain measures with.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }
}

impl Biquad {
    /// Butterworth (Q = 1/sqrt 2) low pass (`high = false`) or high pass at `hz`.
    pub fn set_butter(&mut self, high: bool, hz: f64, sr: f64) {
        let f = hz.clamp(1.0, 0.45 * sr);
        let w = 2.0 * PI * f / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / std::f64::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high { ((1.0 + c) * 0.5, -(1.0 + c)) } else { ((1.0 - c) * 0.5, 1.0 - c) };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b0 / a0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    /// General second-order section from analogue-matched coefficients (already normalised).
    pub fn set(&mut self, b: [f64; 3], a: [f64; 2]) {
        self.b0 = b[0];
        self.b1 = b[1];
        self.b2 = b[2];
        self.a1 = a[0];
        self.a2 = a[1];
    }

    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn clear(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    pub fn flush(&mut self) {
        if self.z1.abs() < 1e-30 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-30 {
            self.z2 = 0.0;
        }
    }
}

/// First-order DC blocker `y = x - x1 + R y1`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DcBlock {
    r: f64,
    x1: f64,
    y1: f64,
}

impl DcBlock {
    pub fn set(&mut self, hz: f64, sr: f64) {
        self.r = (-2.0 * PI * hz / sr).exp();
    }
    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
    pub fn clear(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
    pub fn flush(&mut self) {
        if self.y1.abs() < 1e-30 {
            self.y1 = 0.0;
        }
        if self.x1.abs() < 1e-30 {
            self.x1 = 0.0;
        }
    }
}

/// BS.1770 K-weighting (high shelf then RLB high pass) for any sample rate, by the usual
/// analogue-prototype bilinear design (identical to the standard 48 kHz coefficients).
#[derive(Clone, Copy, Debug, Default)]
pub struct KWeight {
    shelf: Biquad,
    hp: Biquad,
}

impl KWeight {
    pub fn set(&mut self, sr: f64) {
        // shelf
        let f0 = 1681.974450955533;
        let g = 3.999843853973347;
        let q = 0.7071752369554196;
        let k = (PI * f0 / sr).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        self.shelf.set([(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0], [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0]);
        // RLB high pass
        let f0 = 38.13547087602444;
        let q = 0.5003270373238773;
        let k = (PI * f0 / sr).tan();
        let a0 = 1.0 + k / q + k * k;
        self.hp.set([1.0, -2.0, 1.0], [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0]);
    }
    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        self.hp.run(self.shelf.run(x))
    }
    pub fn clear(&mut self) {
        self.shelf.clear();
        self.hp.clear();
    }
    pub fn flush(&mut self) {
        self.shelf.flush();
        self.hp.flush();
    }
}
