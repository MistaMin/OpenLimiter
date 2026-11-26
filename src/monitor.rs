//! Listening aids for the whole plugin, after the output meters: Autogain and Delta.
//!
//! Autogain matches what you hear to the loudness of the plugin's input (before the input
//! gain), so a louder master does not sound better only for being louder. Delta plays what the
//! plugin changed: the output minus the input after the input gain, delayed by the plugin's
//! latency so the two line up sample for sample. With both on, the difference is played at the
//! matched level.
//!
//! Both change what leaves the plugin, so they must be off when printing. The meters measure
//! ahead of this stage and keep showing the processed signal.

use crate::loudness::k_weighting;

/// The loudness compared, as an exponential average: about the short-term window (3 s).
const AVG_S: f64 = 3.0;
/// How fast the heard gain and the delta mix follow, seconds.
const GLIDE_S: f32 = 0.05;
/// In f32 a glide this slow stalls about 1.4e-4 short of its target, so it lands on the target
/// from within this (0.002 dB, or a delta residual 74 dB down).
const SNAP: f32 = 2e-4;
/// Autogain never goes further than this, dB.
const RANGE_DB: f32 = 24.0;
/// Below this K-weighted power (about -60 LUFS) the input counts as quiet: the gain is held.
const QUIET: f64 = 1e-6;

#[derive(Clone, Copy, Default)]
struct Biquad {
    c: [f64; 5],
    z: [f64; 2],
}

impl Biquad {
    fn run(&mut self, x: f64) -> f64 {
        let [b0, b1, b2, a1, a2] = self.c;
        let y = b0 * x + self.z[0];
        self.z[0] = b1 * x - a1 * y + self.z[1];
        self.z[1] = b2 * x - a2 * y;
        y
    }
}

/// K-weighted power of a stereo signal, averaged.
#[derive(Clone, Copy, Default)]
struct Power {
    filters: [[Biquad; 2]; 2],
    avg: f64,
}

impl Power {
    fn new(sr: f64) -> Self {
        let (s1, s2) = k_weighting(sr);
        let f = [Biquad { c: s1, z: [0.0; 2] }, Biquad { c: s2, z: [0.0; 2] }];
        Self { filters: [f, f], avg: 0.0 }
    }

    fn feed(&mut self, l: f32, r: f32, coef: f64) {
        let mut sum = 0.0;
        for (c, x) in [l, r].into_iter().enumerate() {
            let [a, b] = &mut self.filters[c];
            let y = b.run(a.run(f64::from(x)));
            sum += y * y;
        }
        self.avg += (sum - self.avg) * coef;
    }
}

pub struct Monitor {
    /// The input after the input gain, kept for Delta.
    ring: [Vec<f32>; 2],
    pos: usize,
    raw: Power,
    out: Power,
    avg_coef: f64,
    glide: f32,
    /// Heard gain now and where it is heading, linear.
    gain: f32,
    gain_target: f32,
    /// Delta mix now, 0 (off) to 1 (on).
    mix: f32,
    pub autogain: bool,
    pub delta: bool,
}

impl Monitor {
    /// A stage at rest with the given switches (Delta starts fully in).
    pub fn new(sample_rate: f32, autogain: bool, delta: bool) -> Self {
        let sr = f64::from(sample_rate);
        // Two seconds of latency at least.
        let len = ((sample_rate * 2.0) as usize).next_power_of_two();
        Self {
            ring: [vec![0.0; len], vec![0.0; len]],
            pos: 0,
            raw: Power::new(sr),
            out: Power::new(sr),
            avg_coef: 1.0 - (-1.0 / (AVG_S * sr)).exp(),
            glide: 1.0 - (-1.0 / (GLIDE_S * sample_rate)).exp(),
            gain: 1.0,
            gain_target: 1.0,
            mix: if delta { 1.0 } else { 0.0 },
            autogain,
            delta,
        }
    }

    /// Back to the state `new` gives with these switches, without allocating.
    pub fn reset(&mut self, autogain: bool, delta: bool) {
        for ring in &mut self.ring {
            ring.fill(0.0);
        }
        self.pos = 0;
        for p in [&mut self.raw, &mut self.out] {
            p.avg = 0.0;
            p.filters.iter_mut().flatten().for_each(|f| f.z = [0.0; 2]);
        }
        self.gain = 1.0;
        self.gain_target = 1.0;
        self.mix = if delta { 1.0 } else { 0.0 };
        self.autogain = autogain;
        self.delta = delta;
    }

    /// The gain Autogain applies now, dB (0 when it is off).
    pub fn gain_db(&self) -> f32 {
        20.0 * self.gain.max(1e-6).log10()
    }

    /// The plugin's input before the input gain: the loudness Autogain matches.
    pub fn feed_raw(&mut self, l: &[f32], r: &[f32]) {
        for (&a, &b) in l.iter().zip(r) {
            self.raw.feed(a, b, self.avg_coef);
        }
    }

    /// The input after the input gain: what Delta subtracts.
    pub fn feed_input(&mut self, l: &[f32], r: &[f32]) {
        let mask = self.ring[0].len() - 1;
        for (j, (&a, &b)) in l.iter().zip(r).enumerate() {
            let i = (self.pos + j) & mask;
            self.ring[0][i] = a;
            self.ring[1][i] = b;
        }
    }

    /// Turns the output (after the meters) into what is heard, in place. `latency` is the
    /// plugin's latency in samples. Both off and settled: the audio is not touched.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], latency: usize) {
        let mask = self.ring[0].len() - 1;
        let n = l.len().min(r.len());
        for (&a, &b) in l.iter().zip(r.iter()) {
            self.out.feed(a, b, self.avg_coef);
        }
        if self.autogain {
            if self.raw.avg > QUIET && self.out.avg > QUIET {
                let db = (10.0 * (self.raw.avg / self.out.avg).log10()) as f32;
                self.gain_target = 10f32.powf(db.clamp(-RANGE_DB, RANGE_DB) / 20.0);
            }
        } else {
            self.gain_target = 1.0;
        }
        let mix_target = if self.delta { 1.0 } else { 0.0 };
        let still = self.gain == self.gain_target && self.mix == mix_target;
        if !(still && self.gain == 1.0 && self.mix == 0.0) {
            let delay = latency.min(mask);
            for j in 0..n {
                self.gain += (self.gain_target - self.gain) * self.glide;
                if (self.gain - self.gain_target).abs() <= SNAP * self.gain_target {
                    self.gain = self.gain_target;
                }
                self.mix += (mix_target - self.mix) * self.glide;
                if (self.mix - mix_target).abs() <= SNAP {
                    self.mix = mix_target;
                }
                let i = (self.pos + j + mask + 1 - delay) & mask;
                l[j] = (l[j] - self.mix * self.ring[0][i]) * self.gain;
                r[j] = (r[j] - self.mix * self.ring[1][i]) * self.gain;
            }
        }
        self.pos = (self.pos + n) & mask;
    }
}
