//! Small numeric helpers with the exact tie and floor rules of the original's framework helpers,
//! so the port can match the C++ bit for bit.

/// `jmax(a, b)`: `a < b ? b : a`.
#[inline]
pub fn jmax(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

/// `jmin(a, b)`: `b < a ? b : a`.
#[inline]
pub fn jmin(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// `jlimit(lo, hi, v)`: `v < lo ? lo : (hi < v ? hi : v)`.
#[inline]
pub fn jlimit(lo: f32, hi: f32, v: f32) -> f32 {
    if v < lo {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

/// `jmap(v, a, b)`: `a + v * (b - a)`.
#[inline]
pub fn jmap(v: f32, a: f32, b: f32) -> f32 {
    a + v * (b - a)
}

/// dB to gain with a -100 dB floor (below it: 0).
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    db_to_gain_floor(db, -100.0)
}

#[inline]
pub fn db_to_gain_floor(db: f32, minus_inf: f32) -> f32 {
    if db > minus_inf {
        10.0f32.powf(db * 0.05)
    } else {
        0.0
    }
}

/// Gain to dB with a -100 dB floor.
#[inline]
pub fn gain_to_db(g: f32) -> f32 {
    gain_to_db_floor(g, -100.0)
}

#[inline]
pub fn gain_to_db_floor(g: f32, minus_inf: f32) -> f32 {
    if g > 0.0 {
        jmax(minus_inf, g.log10() * 20.0)
    } else {
        minus_inf
    }
}

fn approximately_equal(a: f32, b: f32) -> bool {
    if !(a.is_finite() && b.is_finite()) {
        return a == b;
    }
    let diff = (a - b).abs();
    diff <= f32::MIN_POSITIVE || diff <= f32::EPSILON * a.abs().max(b.abs())
}

/// A linear or multiplicative parameter ramp, with the original framework's step rules.
#[derive(Clone, Copy, Debug)]
pub struct Smoothed {
    current: f32,
    target: f32,
    countdown: i32,
    steps_to_target: i32,
    step: f32,
    multiplicative: bool,
}

impl Smoothed {
    pub const fn linear(initial: f32) -> Self {
        Self { current: initial, target: initial, countdown: 0, steps_to_target: 0, step: 0.0, multiplicative: false }
    }

    pub const fn multiplicative(initial: f32) -> Self {
        Self { current: initial, target: initial, countdown: 0, steps_to_target: 0, step: 0.0, multiplicative: true }
    }

    /// Ramp length `seconds` at `sample_rate`; keeps the target as the current value.
    pub fn reset(&mut self, sample_rate: f64, seconds: f64) {
        self.steps_to_target = (seconds * sample_rate).floor() as i32;
        self.set_current_and_target(self.target);
    }

    pub fn set_current_and_target(&mut self, v: f32) {
        self.target = v;
        self.current = v;
        self.countdown = 0;
    }

    pub fn set_target(&mut self, v: f32) {
        if approximately_equal(v, self.target) {
            return;
        }
        if self.steps_to_target <= 0 {
            self.set_current_and_target(v);
            return;
        }
        self.target = v;
        self.countdown = self.steps_to_target;
        self.step = if self.multiplicative {
            ((self.target.abs().ln() - self.current.abs().ln()) / self.countdown as f32).exp()
        } else {
            (self.target - self.current) / self.countdown as f32
        };
    }

    #[inline]
    pub fn is_smoothing(&self) -> bool {
        self.countdown > 0
    }

    #[inline]
    pub fn current(&self) -> f32 {
        self.current
    }

    #[inline]
    pub fn target(&self) -> f32 {
        self.target
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if !self.is_smoothing() {
            return self.target;
        }
        self.countdown -= 1;
        if self.is_smoothing() {
            if self.multiplicative {
                self.current *= self.step;
            } else {
                self.current += self.step;
            }
        } else {
            self.current = self.target;
        }
        self.current
    }

    pub fn skip(&mut self, n: i32) -> f32 {
        if n >= self.countdown {
            self.set_current_and_target(self.target);
            return self.target;
        }
        if self.multiplicative {
            // pow(float, int) promotes to double in the original.
            self.current *= (self.step as f64).powf(n as f64) as f32;
        } else {
            self.current += self.step * n as f32;
        }
        self.countdown -= n;
        self.current
    }
}
