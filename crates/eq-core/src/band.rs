//! Audio-side state of one band (OpenEQ `EQBand`): designed coefficients, two lanes of
//! transposed direct form II state in double precision, an optional double-double ("117-bit")
//! recursion, and Digital+ phase-compensating all-passes. Never allocates.

use crate::response::{design_band, digital_plus_all_pass, normalize, Norm, MAX_STAGES, NORM_UNITY};
use crate::types::*;

pub const CHUNK: usize = 512;

/// State magnitude snapped to zero at block ends (the IIR helper's denormal guard).
#[inline]
fn snap(x: f64) -> f64 {
    if !(x < -1.0e-8 || x > 1.0e-8) {
        0.0
    } else {
        x
    }
}

#[derive(Clone, Copy, Default)]
struct Dd {
    hi: f64,
    lo: f64,
}

#[inline]
fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd { hi: s, lo: b - (s - a) }
}
#[inline]
fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    Dd { hi: s, lo: (a - (s - bb)) + (b - bb) }
}
#[inline]
fn dd_add(a: Dd, b: Dd) -> Dd {
    let mut s = two_sum(a.hi, b.hi);
    s.lo += a.lo + b.lo;
    quick_two_sum(s.hi, s.lo)
}
#[inline]
fn dd_scale(a: Dd, b: f64) -> Dd {
    let p = a.hi * b;
    let e = a.hi.mul_add(b, -p);
    quick_two_sum(p, e + a.lo * b)
}

#[derive(Clone)]
pub struct EqBand {
    sample_rate: f64,
    coeffs: [Norm; MAX_STAGES],
    active_stages: usize,
    state: [[[f64; 2]; MAX_STAGES]; 2],
    dd_state: [[[Dd; 2]; MAX_STAGES]; 2],
    ultra: bool,
    digital_plus: usize,
    ready: bool,
    applied_type: FilterType,
    applied_frequency: f32,
    applied_gain: f32,
    applied_q: f32,
    requested_q: f32,
    applied_slope: i32,
    stereo_mode: StereoMode,
    pan: f32,
    designs: u64,
}

impl Default for EqBand {
    fn default() -> Self {
        EqBand {
            sample_rate: 44100.0,
            coeffs: [NORM_UNITY; MAX_STAGES],
            active_stages: 1,
            state: [[[0.0; 2]; MAX_STAGES]; 2],
            dd_state: [[[Dd::default(); 2]; MAX_STAGES]; 2],
            ultra: false,
            digital_plus: 0,
            ready: false,
            applied_type: FilterType::Bell,
            applied_frequency: -1.0,
            applied_gain: 0.0,
            applied_q: 1.0,
            requested_q: 1.0,
            applied_slope: 12,
            stereo_mode: StereoMode::Stereo,
            pan: 0.0,
            designs: 0,
        }
    }
}

impl EqBand {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.coeffs = [NORM_UNITY; MAX_STAGES];
        self.active_stages = 1;
        self.ready = false;
        self.applied_frequency = -1.0;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.state = [[[0.0; 2]; MAX_STAGES]; 2];
        self.dd_state = [[[Dd::default(); 2]; MAX_STAGES]; 2];
    }

    /// Installs this design unless it matches the installed one. Returns true when it redesigned.
    pub fn apply_if_changed(&mut self, ty: FilterType, frequency: f32, gain_db: f32, q: f32, slope_db: i32) -> bool {
        if self.sample_rate <= 0.0 {
            return false;
        }
        let f = clamp_frequency(frequency, self.sample_rate);
        let q = clamp_q(q);
        if self.ready
            && ty == self.applied_type
            && slope_db == self.applied_slope
            && f.to_bits() == self.applied_frequency.to_bits()
            && gain_db.to_bits() == self.applied_gain.to_bits()
            && q.to_bits() == self.requested_q.to_bits()
        {
            return false;
        }
        let mut d = design_band(ty, f, gain_db, q, slope_db, self.sample_rate);
        let mut to_write = d.stage_count.clamp(1, MAX_STAGES);
        if self.digital_plus > 0 && to_write < MAX_STAGES {
            let base = d.stage_count;
            for i in 0..base {
                if to_write >= MAX_STAGES {
                    break;
                }
                let mut existing = [(0.0f64, 0.0f64); MAX_STAGES];
                let mut ec = 0;
                for _ in 0..self.digital_plus {
                    if to_write >= MAX_STAGES {
                        break;
                    }
                    let (d1, d2) = digital_plus_all_pass(&d.raw[i], d.frequency, self.sample_rate, 0.5, 2.0, &existing[..ec]);
                    existing[ec] = (d1, d2);
                    ec += 1;
                    d.raw[to_write] = [d2, d1, 1.0, 1.0, d1, d2];
                    to_write += 1;
                }
            }
        }
        let topology = !self.ready
            || ty != self.applied_type
            || to_write != self.active_stages
            || (is_cut(ty) && slope_db != self.applied_slope);
        for i in 0..to_write {
            self.coeffs[i] = normalize(&d.raw[i]);
        }
        self.active_stages = to_write;
        self.applied_type = ty;
        self.applied_slope = slope_db;
        self.applied_frequency = d.frequency;
        self.applied_gain = gain_db;
        self.applied_q = d.q;
        self.requested_q = q;
        self.ready = true;
        self.designs += 1;
        if topology {
            self.reset();
        }
        true
    }

    pub fn set_stereo_mode(&mut self, m: StereoMode) {
        self.stereo_mode = m;
    }
    pub fn stereo_mode(&self) -> StereoMode {
        self.stereo_mode
    }
    pub fn set_pan(&mut self, p: f32) {
        self.pan = if p.is_nan() { 0.0 } else { p.clamp(-1.0, 1.0) };
    }
    pub fn pan(&self) -> f32 {
        self.pan
    }
    pub fn set_ultra(&mut self, on: bool) {
        if on != self.ultra {
            self.ultra = on;
            self.reset();
        }
    }
    pub fn set_digital_plus(&mut self, steps: usize) {
        self.digital_plus = steps.min(4);
    }
    pub fn ready(&self) -> bool {
        self.ready
    }
    pub fn frequency(&self) -> f32 {
        self.applied_frequency
    }
    pub fn gain(&self) -> f32 {
        self.applied_gain
    }
    pub fn q(&self) -> f32 {
        self.applied_q
    }
    pub fn stage_count(&self) -> usize {
        self.active_stages
    }
    pub fn stages(&self) -> &[Norm] {
        &self.coeffs[..self.active_stages]
    }
    /// Number of designs installed since prepare (for tests: a settled band stops redesigning).
    pub fn design_count(&self) -> u64 {
        self.designs
    }

    pub fn magnitude(&self, f: f64) -> f64 {
        if !self.ready {
            return 1.0;
        }
        let mut m = 1.0;
        for c in self.stages() {
            m *= crate::response::stage_magnitude(c, f, self.sample_rate);
        }
        m
    }

    /// Processes one lane (0 or 1) in place.
    pub fn process(&mut self, data: &mut [f32], lane: usize) {
        if data.is_empty() || !self.ready {
            return;
        }
        let lane = lane.min(1);
        if self.ultra {
            self.process_ultra(data, lane);
            return;
        }
        let mut work = [0.0f64; CHUNK];
        for chunk in data.chunks_mut(CHUNK) {
            let n = chunk.len();
            for (w, &x) in work.iter_mut().zip(chunk.iter()) {
                *w = x as f64;
            }
            for s in 0..self.active_stages {
                let [b0, b1, b2, a1, a2] = self.coeffs[s];
                let z = &mut self.state[lane][s];
                let (mut s1, mut s2) = (z[0], z[1]);
                for w in work[..n].iter_mut() {
                    let x = *w;
                    let y = x * b0 + s1;
                    *w = y;
                    s1 = x * b1 - y * a1 + s2;
                    s2 = x * b2 - y * a2;
                }
                z[0] = snap(s1);
                z[1] = snap(s2);
            }
            for (o, w) in chunk.iter_mut().zip(work.iter()) {
                *o = *w as f32;
            }
        }
    }

    /// Processes lane 0 on `a` and lane 1 on `b`.
    pub fn process_pair(&mut self, a: &mut [f32], b: &mut [f32]) {
        self.process(a, 0);
        self.process(b, 1);
    }

    fn process_ultra(&mut self, data: &mut [f32], lane: usize) {
        for v in data.iter_mut() {
            let mut x = Dd { hi: *v as f64, lo: 0.0 };
            for s in 0..self.active_stages {
                let c = self.coeffs[s];
                let st = &mut self.dd_state[lane][s];
                let y = dd_add(dd_scale(x, c[0]), st[0]);
                st[0] = dd_add(dd_add(dd_scale(x, c[1]), dd_scale(y, -c[3])), st[1]);
                st[1] = dd_add(dd_scale(x, c[2]), dd_scale(y, -c[4]));
                x = y;
            }
            *v = (x.hi + x.lo) as f32;
        }
    }
}
