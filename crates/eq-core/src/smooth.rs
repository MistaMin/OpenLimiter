//! Linear ramp with a fixed number of steps per target change (the smoothed-value helper the
//! original used).

#[derive(Clone, Copy, Debug)]
pub struct Smoothed {
    current: f32,
    target: f32,
    step: f32,
    countdown: i32,
    steps: i32,
}

impl Default for Smoothed {
    fn default() -> Self {
        Smoothed { current: 0.0, target: 0.0, step: 0.0, countdown: 0, steps: 0 }
    }
}

impl Smoothed {
    /// Sets the ramp length and stops any ramp in progress.
    pub fn reset(&mut self, sample_rate: f64, seconds: f64) {
        self.steps = (seconds * sample_rate).floor() as i32;
        self.set_current_and_target(self.target);
    }
    pub fn set_current_and_target(&mut self, v: f32) {
        self.current = v;
        self.target = v;
        self.countdown = 0;
    }
    pub fn set_target(&mut self, v: f32) {
        if v == self.target {
            return;
        }
        if self.steps <= 0 {
            self.set_current_and_target(v);
            return;
        }
        self.target = v;
        self.countdown = self.steps;
        self.step = (self.target - self.current) / self.countdown as f32;
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
        if self.countdown > 0 {
            self.current += self.step;
        } else {
            self.current = self.target;
        }
        self.current
    }
    pub fn skip(&mut self, n: usize) {
        if n as i32 >= self.countdown {
            self.set_current_and_target(self.target);
            return;
        }
        self.current += self.step * n as f32;
        self.countdown -= n as i32;
    }
}
