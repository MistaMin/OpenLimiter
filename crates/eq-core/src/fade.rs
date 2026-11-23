//! Output fade around a latency change (OpenEQ `applyLatencyTransition`): fade down, switch in
//! silence, stay silent for the new delay, fade back up. 5 ms ramps.

pub const FADE_SECONDS: f64 = 0.005;

#[derive(Clone, Copy, Debug)]
pub struct LatencyFade {
    /// A change is waiting for the fade-down to finish.
    pub pending: bool,
    pub gain: f32,
    pub mute: usize,
    step: f32,
}

impl Default for LatencyFade {
    fn default() -> Self {
        LatencyFade { pending: false, gain: 1.0, mute: 0, step: 1.0 / 240.0 }
    }
}

impl LatencyFade {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.step = 1.0 / ((FADE_SECONDS * sample_rate).round().max(1.0) as f32);
        self.pending = false;
        self.gain = 1.0;
        self.mute = 0;
    }

    /// True once the output is fully down and a pending change may be applied.
    pub fn ready_to_switch(&self) -> bool {
        self.pending && self.gain <= 0.0
    }

    /// Applied in silence: mute for `delay` samples, then fade up.
    pub fn switched(&mut self, delay: usize) {
        self.pending = false;
        self.gain = 0.0;
        self.mute = delay;
    }

    pub fn idle(&self) -> bool {
        !self.pending && self.mute == 0 && self.gain >= 1.0
    }

    pub fn apply(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.idle() {
            return;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.pending {
                self.gain = (self.gain - self.step).max(0.0);
            } else if self.mute > 0 {
                self.mute -= 1;
            } else {
                self.gain = (self.gain + self.step).min(1.0);
            }
            *l *= self.gain;
            *r *= self.gain;
        }
    }
}
