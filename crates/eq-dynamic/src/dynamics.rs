//! Per-band dynamic gain (OpenEQ `DynamicsEngine`).
//!
//! Detect: a band-pass of the signal measured as linked stereo energy (no L+R cancellation), or
//! left, right, mid or side. Relative: the level is compared against a reference, the broadband
//! level or its own band-pass ("Relative Free"). Gain: the Below and Above sections (shared
//! threshold, each with range and ratio), or the single-action path (action + range) when both
//! sections are off. The envelope advances one sample at a time, so the host block size does not
//! change the result.

use eq_core::jfilt::band_pass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mix {
    Energy,
    Left,
    Right,
    Mid,
    Side,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub threshold_db: f32,
    pub knee_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub transient_percent: f32,
    pub automatic: bool,
    pub relative_percent: f32,
    pub relative_free: bool,
    pub modern: bool,
    pub below_on: bool,
    pub below_range_db: f32,
    pub below_ratio: f32,
    pub above_on: bool,
    pub above_range_db: f32,
    pub above_ratio: f32,
    pub action: u8,
    pub range_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            threshold_db: -24.0,
            knee_db: 0.0,
            attack_ms: 15.0,
            release_ms: 150.0,
            transient_percent: 0.0,
            automatic: false,
            relative_percent: 0.0,
            relative_free: false,
            modern: false,
            below_on: false,
            below_range_db: 6.0,
            below_ratio: 1.0,
            above_on: false,
            above_range_db: -6.0,
            above_ratio: 1.0,
            action: 0,
            range_db: 6.0,
        }
    }
}

pub const MAX_SLICE: usize = 512;

#[inline]
fn snap(x: f32) -> f32 {
    if !(x < -1.0e-8 || x > 1.0e-8) {
        0.0
    } else {
        x
    }
}

/// Single-precision band-pass biquad (transposed direct form II), as the original detector.
#[derive(Clone, Copy, Default)]
struct Bp {
    c: [f32; 5],
    z: [f32; 2],
}

impl Bp {
    fn set(&mut self, sr: f64, f: f32, q: f32) {
        let r = band_pass(sr, f as f64, q as f64);
        let inv = 1.0 / r[3];
        self.c = [(r[0] * inv) as f32, (r[1] * inv) as f32, (r[2] * inv) as f32, (r[4] * inv) as f32, (r[5] * inv) as f32];
    }
    fn run(&mut self, x: &mut [f32]) {
        let [b0, b1, b2, a1, a2] = self.c;
        let (mut s1, mut s2) = (self.z[0], self.z[1]);
        for v in x.iter_mut() {
            let i = *v;
            let o = i * b0 + s1;
            *v = o;
            s1 = i * b1 - o * a1 + s2;
            s2 = i * b2 - o * a2;
        }
        self.z = [snap(s1), snap(s2)];
    }
}

#[derive(Clone, Copy)]
struct Bank {
    f: [Bp; 2],
    freq: f32,
    q: f32,
    ready: bool,
}

impl Default for Bank {
    fn default() -> Self {
        Bank { f: [Bp::default(); 2], freq: -1.0, q: -1.0, ready: false }
    }
}

impl Bank {
    fn set(&mut self, sr: f64, f: f32, q: f32) {
        if self.ready && f.to_bits() == self.freq.to_bits() && q.to_bits() == self.q.to_bits() {
            return;
        }
        for b in self.f.iter_mut() {
            b.set(sr, f, q);
            b.z = [0.0; 2];
        }
        self.freq = f;
        self.q = q;
        self.ready = true;
    }

    fn run_mix(&mut self, mix: Mix, l: &[f32], r: &[f32], a: &mut [f32], b: &mut [f32]) {
        let n = a.len();
        match mix {
            Mix::Mid | Mix::Side => {
                for i in 0..n {
                    a[i] = if mix == Mix::Mid { 0.5 * (l[i] + r[i]) } else { 0.5 * (l[i] - r[i]) };
                }
                self.f[0].run(a);
            }
            Mix::Left => {
                a.copy_from_slice(&l[..n]);
                self.f[0].run(a);
            }
            Mix::Right => {
                a.copy_from_slice(&r[..n]);
                self.f[0].run(a);
            }
            Mix::Energy => {
                a.copy_from_slice(&l[..n]);
                b.copy_from_slice(&r[..n]);
                self.f[0].run(a);
                self.f[1].run(b);
            }
        }
    }
}

#[inline]
fn level_of(mix: Mix, a: f32, b: f32) -> f32 {
    if mix == Mix::Energy {
        (0.5 * (a * a + b * b)).sqrt()
    } else {
        a.abs()
    }
}

#[inline]
fn broadband_level(mix: Mix, l: f32, r: f32) -> f32 {
    match mix {
        Mix::Left => l.abs(),
        Mix::Right => r.abs(),
        Mix::Mid => (0.5 * (l + r)).abs(),
        Mix::Side => (0.5 * (l - r)).abs(),
        Mix::Energy => (0.5 * (l * l + r * r)).sqrt(),
    }
}

#[inline]
fn gain_to_db(g: f32) -> f32 {
    if g > 0.0 {
        (20.0 * g.log10()).max(-100.0)
    } else {
        -100.0
    }
}

fn soft_knee(region: f32, knee_db: f32) -> f32 {
    let knee = knee_db.max(0.0);
    if knee <= 0.05 {
        return region.max(0.0);
    }
    let half = 0.5 * knee;
    if region <= -half {
        return 0.0;
    }
    if region >= half {
        return region;
    }
    let t = (region + half) / knee;
    t * t * half
}

fn limited_offset(depth: f32, range: f32) -> f32 {
    let m = depth.max(0.0).min(range.abs());
    if range < 0.0 {
        -m
    } else {
        m
    }
}

#[derive(Clone, Copy)]
pub struct Dynamics {
    sample_rate: f64,
    envelope: f32,
    fast_envelope: f32,
    gain_level: f32,
    last_threshold_db: f32,
    reference: f32,
    trigger: Bank,
    relative: Bank,
}

impl Default for Dynamics {
    fn default() -> Self {
        Dynamics {
            sample_rate: 44100.0,
            envelope: 0.0,
            fast_envelope: 0.0,
            gain_level: 0.0,
            last_threshold_db: -24.0,
            reference: 0.0,
            trigger: Bank::default(),
            relative: Bank::default(),
        }
    }
}

impl Dynamics {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.trigger = Bank::default();
        self.relative = Bank::default();
        self.trigger.set(sample_rate, 1000.0, 0.7);
        self.relative.set(sample_rate, 1000.0, 0.7);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.fast_envelope = 0.0;
        self.gain_level = 0.0;
        self.reference = 0.0;
        for b in self.trigger.f.iter_mut().chain(self.relative.f.iter_mut()) {
            b.z = [0.0; 2];
        }
    }

    pub fn set_trigger_filter(&mut self, f: f32, q: f32) {
        self.trigger.set(self.sample_rate, f, q);
    }

    pub fn set_relative_filter(&mut self, f: f32, q: f32) {
        self.relative.set(self.sample_rate, f, q);
    }

    fn coeff_for_ms(&self, ms: f32) -> f32 {
        let samples = (self.sample_rate as f32 * ms.max(0.05) / 1000.0).max(1.0);
        (-1.0 / samples).exp()
    }

    pub fn envelope_db(&self) -> f32 {
        gain_to_db(self.envelope)
    }

    pub fn reference_db(&self) -> f32 {
        gain_to_db(self.reference).max(-80.0)
    }

    pub fn last_threshold_db(&self) -> f32 {
        self.last_threshold_db
    }

    /// Advances by the slice (at most `MAX_SLICE` samples) and returns the gain offset in dB.
    /// `rel` is the relative path's own input (External); `None` uses the detector input.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(&mut self, l: &[f32], r: &[f32], mix: Mix, rel: Option<(&[f32], &[f32])>, rel_mix: Mix, s: &Settings) -> f32 {
        let (mut attack, mut release, mut threshold) = (s.attack_ms, s.release_ms, s.threshold_db);
        if s.automatic {
            attack = 8.0;
            release = 140.0;
        }
        let n = l.len().min(r.len()).min(MAX_SLICE);
        if n == 0 || self.sample_rate <= 0.0 {
            self.gain_level = self.envelope;
            if s.automatic {
                threshold = (self.reference_db() + 8.0).clamp(-60.0, 0.0);
            }
            self.last_threshold_db = threshold;
            return self.gain_for(threshold, s);
        }
        let mut fa = [0.0f32; MAX_SLICE];
        let mut fb = [0.0f32; MAX_SLICE];
        self.trigger.run_mix(mix, l, r, &mut fa[..n], &mut fb[..n]);
        let (rl, rr) = match rel {
            Some((a, b)) if a.len() >= n && b.len() >= n => (a, b),
            _ => (l, r),
        };
        let mut ra = [0.0f32; MAX_SLICE];
        let mut rb = [0.0f32; MAX_SLICE];
        if s.relative_free {
            self.relative.run_mix(rel_mix, rl, rr, &mut ra[..n], &mut rb[..n]);
        }
        let ac = self.coeff_for_ms(attack);
        let rc = self.coeff_for_ms(release);
        let fac = self.coeff_for_ms(1.0);
        let refc = self.coeff_for_ms(80.0);
        for i in 0..n {
            let level = level_of(mix, fa[i], fb[i]);
            let follow = if level > self.envelope { ac } else { rc };
            self.envelope = level + follow * (self.envelope - level);
            let ff = if level > self.fast_envelope { fac } else { rc };
            self.fast_envelope = level + ff * (self.fast_envelope - level);
            let rlv = if s.relative_free { level_of(rel_mix, ra[i], rb[i]) } else { broadband_level(rel_mix, rl[i], rr[i]) };
            self.reference = rlv + refc * (self.reference - rlv);
        }
        self.gain_level = self.envelope;
        if s.transient_percent > 0.05 {
            let blend = (s.transient_percent / 100.0).clamp(0.0, 1.0);
            self.gain_level = (self.envelope + blend * (self.fast_envelope - self.envelope)).max(0.0);
        }
        if s.automatic {
            threshold = (self.reference_db() + 8.0).clamp(-60.0, 0.0);
        }
        self.last_threshold_db = threshold;
        self.gain_for(threshold, s)
    }

    fn gain_for(&self, threshold: f32, s: &Settings) -> f32 {
        let band_db = gain_to_db(self.gain_level);
        let raw_ref = gain_to_db(self.reference);
        // Silence floor: relative detection must not expand a quiet passage.
        if raw_ref < -70.0 && band_db < -70.0 {
            return 0.0;
        }
        let blend = (s.relative_percent / 100.0).clamp(0.0, 1.0);
        let relative_db = band_db - self.reference_db();
        let used = band_db + blend * (relative_db - band_db);
        if s.modern {
            let signed = soft_knee(used - threshold, s.knee_db) - soft_knee(threshold - used, s.knee_db);
            if signed > 0.0 {
                return if s.above_on { limited_offset(signed * s.above_ratio, s.above_range_db) } else { 0.0 };
            }
            if signed < 0.0 {
                return if s.below_on { limited_offset(-signed * s.below_ratio, s.below_range_db) } else { 0.0 };
            }
            return 0.0;
        }
        let region = if s.action == 0 { used - threshold } else { threshold - used };
        let depth = soft_knee(region, s.knee_db);
        if depth <= 0.0 {
            return 0.0;
        }
        if s.range_db >= 0.0 {
            depth.min(s.range_db)
        } else {
            (-depth).max(s.range_db)
        }
    }
}
