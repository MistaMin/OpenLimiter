//! The transient designer and the loudness matcher behind Auto Gain.
//!
//! Ports of `DSP/TransientDesigner.h` and `DSP/LoudnessMatch.h`.

/// `std::clamp(v, lo, hi)`.
#[inline]
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

#[inline]
fn smax(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

#[inline]
fn smin(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// Stereo-linked, level-independent differential-envelope transient designer.
#[derive(Clone, Debug)]
pub struct TransientDesigner {
    sr: f64,
    fast: f32,
    med: f32,
    slow: f32,
    /// The gain applied to the last sample, dB.
    pub gain_db: f32,
}

impl Default for TransientDesigner {
    fn default() -> Self {
        Self { sr: 48000.0, fast: 0.0, med: 0.0, slow: 0.0, gain_db: 0.0 }
    }
}

impl TransientDesigner {
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr.max(8000.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.med = 0.0;
        self.slow = 0.0;
        self.gain_db = 0.0;
    }

    #[inline]
    fn coeff(&self, ms: f32) -> f32 {
        (-1.0f32 / (0.001f32 * smax(0.05, ms) * self.sr as f32)).exp()
    }

    #[inline]
    fn step(env: f32, x: f32, att: f32, rel: f32) -> f32 {
        let c = if x > env { att } else { rel };
        c * env + (1.0 - c) * x
    }

    /// Processes a block in place. `attack_pct`/`sustain_pct` are -100..100, `speed_ms` 0.5..50.
    /// Returns the extreme (largest magnitude) gain applied in the block, dB.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], enabled: bool, attack_pct: f32, sustain_pct: f32, speed_ms: f32) -> f32 {
        let n = l.len().min(r.len());
        let scale = clamp(speed_ms / 10.0, 0.05, 5.0);
        let fast_att = self.coeff(0.35 * scale);
        let fast_rel = self.coeff(11.0 * scale);
        let med_att = self.coeff(14.0 * scale);
        let med_rel = self.coeff(58.0 * scale);
        let slow_att = self.coeff(24.0 * scale);
        let slow_rel = self.coeff(320.0 * scale);
        let smooth = self.coeff(0.6);
        let att_amount = clamp(attack_pct * 0.01, -1.0, 1.0);
        let sus_amount = clamp(sustain_pct * 0.01, -1.0, 1.0);
        let mut extreme = 0.0f32;
        for s in 0..n {
            let det = smax(l[s].abs(), r[s].abs());
            self.fast = Self::step(self.fast, det, fast_att, fast_rel);
            self.med = Self::step(self.med, det, med_att, med_rel);
            self.slow = Self::step(self.slow, det, slow_att, slow_rel);
            let mut target = 0.0;
            if enabled {
                let fast_db = 20.0 * (self.fast + 1.0e-6).log10();
                let med_db = 20.0 * (self.med + 1.0e-6).log10();
                let slow_db = 20.0 * (self.slow + 1.0e-6).log10();
                // Smooth floor gate (-66..-50 dBFS) so silence and low hiss stay untouched.
                let gate = clamp((slow_db + 66.0) / 16.0, 0.0, 1.0);
                let att_diff = smax(0.0, fast_db - med_db);
                let sus_diff = smax(0.0, slow_db - fast_db);
                let att_gain = att_amount * smin(15.0, att_diff * 1.65);
                let sus_gain = sus_amount * smin(15.0, sus_diff * 1.85);
                target = clamp(gate * (att_gain + sus_gain), -18.0, 18.0);
            }
            self.gain_db = smooth * self.gain_db + (1.0 - smooth) * target;
            if enabled || self.gain_db.abs() > 1.0e-4 {
                let g = crate::util::db_to_gain(self.gain_db);
                l[s] *= g;
                r[s] *= g;
                if self.gain_db.abs() > extreme.abs() {
                    extreme = self.gain_db;
                }
            }
        }
        extreme
    }
}

// ---------------------------------------------------------------------------------------------

/// BS.1770 K-weighting coefficients (high shelf, then the RLB high-pass), for any sample rate.
#[derive(Clone, Copy, Debug, Default)]
pub struct KWeighting {
    shelf_b: [f64; 3],
    shelf_a: [f64; 3],
    hp_b: [f64; 3],
    hp_a: [f64; 3],
}

impl KWeighting {
    pub fn design(sr: f64) -> Self {
        let pi = 3.14159265358979323846f64;
        let mut c = KWeighting::default();
        {
            let f0 = 1681.974450955533;
            let gain_db = 3.999843853973347;
            let q = 0.7071752369554196;
            let k = (pi * f0 / sr).tan();
            let vh = 10.0f64.powf(gain_db / 20.0);
            let vb = vh.powf(0.4996667741545416);
            let a0 = 1.0 + k / q + k * k;
            c.shelf_b = [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0];
            c.shelf_a = [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];
        }
        {
            let f0 = 38.13547087602444;
            let q = 0.5003270373238773;
            let k = (pi * f0 / sr).tan();
            let a0 = 1.0 + k / q + k * k;
            c.hp_b = [1.0, -2.0, 1.0];
            c.hp_a = [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0];
        }
        c
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    z1: f64,
    z2: f64,
}

impl Biquad {
    #[inline]
    fn run(&mut self, x: f64, b: &[f64; 3], a: &[f64; 3]) -> f64 {
        let y = b[0] * x + self.z1;
        self.z1 = b[1] * x - a[1] * y + self.z2;
        self.z2 = b[2] * x - a[2] * y;
        y
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct KFilter {
    shelf: Biquad,
    hp: Biquad,
}

impl KFilter {
    #[inline]
    fn process(&mut self, x: f32, c: &KWeighting) -> f64 {
        let s = self.shelf.run(x as f64, &c.shelf_b, &c.shelf_a);
        self.hp.run(s, &c.hp_b, &c.hp_a)
    }
}

const WINDOW_CHUNKS: usize = 30;
const ABS_GATE_LUFS: f64 = -70.0;
const MAX_GAIN_DB: f32 = 24.0;

/// Matches short-term (3 s) K-weighted loudness of the processed signal to a reference.
/// Feed-forward, in 100 ms chunks; chunks with a reference below -70 LUFS are skipped.
#[derive(Clone, Debug)]
pub struct LoudnessMatcher {
    coeffs: KWeighting,
    chunk_len: i32,
    ref_f: [KFilter; 2],
    proc_f: [KFilter; 2],
    ref_ring: [f64; WINDOW_CHUNKS],
    proc_ring: [f64; WINDOW_CHUNKS],
    ring_count: usize,
    ring_pos: usize,
    chunk_ref: f64,
    chunk_proc: f64,
    chunk_fill: i32,
    target_db: f32,
}

impl Default for LoudnessMatcher {
    fn default() -> Self {
        Self {
            coeffs: KWeighting::design(48000.0),
            chunk_len: 4800,
            ref_f: [KFilter::default(); 2],
            proc_f: [KFilter::default(); 2],
            ref_ring: [0.0; WINDOW_CHUNKS],
            proc_ring: [0.0; WINDOW_CHUNKS],
            ring_count: 0,
            ring_pos: 0,
            chunk_ref: 0.0,
            chunk_proc: 0.0,
            chunk_fill: 0,
            target_db: 0.0,
        }
    }
}

pub fn mean_square_to_lufs(ms: f64) -> f64 {
    -0.691 + 10.0 * ms.max(1.0e-20).log10()
}

impl LoudnessMatcher {
    pub fn prepare(&mut self, sr: f64) {
        let sr = sr.max(8000.0);
        self.coeffs = KWeighting::design(sr);
        self.chunk_len = ((0.1 * sr).round() as i32).max(1);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ref_f = [KFilter::default(); 2];
        self.proc_f = [KFilter::default(); 2];
        self.ref_ring = [0.0; WINDOW_CHUNKS];
        self.proc_ring = [0.0; WINDOW_CHUNKS];
        self.ring_count = 0;
        self.ring_pos = 0;
        self.chunk_ref = 0.0;
        self.chunk_proc = 0.0;
        self.chunk_fill = 0;
        self.target_db = 0.0;
    }

    pub fn target_gain_db(&self) -> f32 {
        self.target_db
    }

    pub fn process(&mut self, reference: [&[f32]; 2], processed: [&[f32]; 2], n: usize) {
        for s in 0..n {
            for ch in 0..2 {
                let r = self.ref_f[ch].process(reference[ch][s], &self.coeffs);
                let p = self.proc_f[ch].process(processed[ch][s], &self.coeffs);
                self.chunk_ref += r * r;
                self.chunk_proc += p * p;
            }
            self.chunk_fill += 1;
            if self.chunk_fill >= self.chunk_len {
                self.finish_chunk();
            }
        }
    }

    fn finish_chunk(&mut self) {
        let ref_ms = self.chunk_ref / self.chunk_fill as f64;
        let proc_ms = self.chunk_proc / self.chunk_fill as f64;
        self.chunk_ref = 0.0;
        self.chunk_proc = 0.0;
        self.chunk_fill = 0;
        if mean_square_to_lufs(ref_ms) < ABS_GATE_LUFS {
            return;
        }
        self.ref_ring[self.ring_pos] = ref_ms;
        self.proc_ring[self.ring_pos] = proc_ms;
        self.ring_pos = (self.ring_pos + 1) % WINDOW_CHUNKS;
        self.ring_count = (self.ring_count + 1).min(WINDOW_CHUNKS);
        let mut rs = 0.0;
        let mut ps = 0.0;
        for i in 0..self.ring_count {
            rs += self.ref_ring[i];
            ps += self.proc_ring[i];
        }
        let rl = mean_square_to_lufs(rs / self.ring_count as f64) as f32;
        let pl = mean_square_to_lufs(ps / self.ring_count as f64) as f32;
        self.target_db = clamp(rl - pl, -MAX_GAIN_DB, MAX_GAIN_DB);
    }
}
