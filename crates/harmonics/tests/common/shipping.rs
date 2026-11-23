//! Line-for-line port of the shipping residual ADAA1 (`dsp::adaa::Residual` in the preamp and
//! EQ products), used only as a reference in the tests.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Off,
    Hold,
    On,
}

#[derive(Default, Clone, Copy)]
pub struct Shipping {
    x1: f64,
    f1: f64,
    r1: f64,
    apx: f64,
    apy: f64,
    primed: bool,
}

impl Shipping {
    pub fn step(&mut self, x: f64, f: f64, ff: f64, slope: f64, mode: Mode) -> f64 {
        if mode == Mode::Off {
            self.primed = false;
            return f;
        }
        let lin_in = slope * x;
        if !self.primed {
            self.apx = lin_in;
            self.apy = lin_in;
        }
        let lin = (1.0 / 3.0) * (lin_in - self.apy) + self.apx;
        self.apx = lin_in;
        self.apy = lin;
        let r = f - lin_in;
        let fr = ff - 0.5 * slope * x * x;
        let mut result = lin + r;
        if mode == Mode::On && self.primed {
            let dx = x - self.x1;
            if dx.abs() > 1e-5 {
                let mut mean_r = (fr - self.f1) / dx;
                let lo = r.min(self.r1);
                let hi = r.max(self.r1);
                let tol = (hi - lo) + 1e-6 * (1.0 + r.abs());
                if !(mean_r >= lo - tol && mean_r <= hi + tol) {
                    mean_r = r;
                }
                let c = if x.abs() > 1e-9 { 1.0 - f / lin_in } else { 0.0 };
                let c8 = c.powi(8);
                let w = c8 / (c8 + 0.3f64.powi(8));
                result = lin + mean_r + w * (0.5 * slope * (x + self.x1) - lin);
            }
        }
        self.x1 = x;
        self.f1 = fr;
        self.r1 = r;
        self.primed = true;
        result
    }
}
