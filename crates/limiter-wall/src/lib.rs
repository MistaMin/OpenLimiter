//! Look-ahead wall limiter with manual and programme-dependent release.
//!
//! One module, two measured behaviours selected by the `style` parameter:
//!
//! * **Contour** follows source G of `spec/MASTER_BRIEF.md` (`spec/params/` table for G,
//!   sections 1, 3, 4, 5, 6): 36 samples of delay at 48 kHz, no knee, two-pole release in the
//!   linear gain domain, four programme-dependent automatic release profiles, a 0.06 dB
//!   margin on the reconstructed peak, an input soft clipper with three curves.
//! * **Leveller** follows source H (`spec/params/` table for H, sections 2 to 5): 64 samples
//!   of delay, gain below the limit exactly ceiling minus threshold, sample-peak limiting at
//!   exactly the ceiling, one-pole release, one automatic release.
//!
//! Own addition, not measured from any source: `strictTruePeak`, a second look-ahead stage
//! that holds the reconstructed (inter-sample) peak under the ceiling.
//!
//! Signal flow (G section 1 for the order of clipper and gain):
//! soft clip -> gain -> limiter -> [strict true-peak stage] -> output.
//!
//! Everything is allocated in `prepare`. `process`, `set_param` and `reset` do not allocate,
//! lock or panic: every buffer is a power of two indexed through a mask.

use olm_api::{Kind, Meters, Module, ParamDesc};

pub const P_STYLE: usize = 0;
pub const P_DRIVE: usize = 1;
pub const P_THRESHOLD: usize = 2;
pub const P_CEILING: usize = 3;
pub const P_AUTO_RELEASE: usize = 4;
pub const P_RELEASE: usize = 5;
pub const P_PROFILE: usize = 6;
pub const P_SOFT_CLIP: usize = 7;
pub const P_SOFT_CLIP_THRESHOLD: usize = 8;
pub const P_SOFT_CLIP_CURVE: usize = 9;
pub const P_STRICT_TRUE_PEAK: usize = 10;
const N_PARAMS: usize = 11;

pub const STYLE_CONTOUR: f32 = 0.0;
pub const STYLE_LEVELLER: f32 = 1.0;

static STYLE_LABELS: [&str; 2] = ["Contour", "Leveller"];
static PROFILE_LABELS: [&str; 4] = ["Universal", "Dense", "Natural", "Voice"];
static CURVE_LABELS: [&str; 3] = ["Wide knee", "Continuous", "Narrow knee"];

static PARAMS: [ParamDesc; N_PARAMS] = [
    ParamDesc::choice("style", "Style", &STYLE_LABELS, 0),
    ParamDesc::new("drive", "Drive", 0.0, 18.0, 0.0, Kind::Db),
    ParamDesc::new("threshold", "Threshold", -30.0, 0.0, 0.0, Kind::Db),
    ParamDesc::new("ceiling", "Ceiling", -30.0, 0.0, -0.2, Kind::Db),
    ParamDesc::toggle("autoRelease", "Auto Release", true),
    ParamDesc::new("release", "Release", 0.01, 3000.0, 200.0, Kind::Ms).log(),
    ParamDesc::choice("profile", "Auto Profile", &PROFILE_LABELS, 0),
    ParamDesc::toggle("softClip", "Soft Clip", false),
    ParamDesc::new("softClipThreshold", "Soft Clip Threshold", -20.0, 0.0, 0.0, Kind::Db),
    ParamDesc::choice("softClipCurve", "Soft Clip Curve", &CURVE_LABELS, 0),
    ParamDesc::toggle("strictTruePeak", "Strict True Peak", false),
];

// ---------------------------------------------------------------------------------------
// Model constants. Each one names the measurement it was fitted to.
// ---------------------------------------------------------------------------------------

/// The numbers that shape the release. Public only so the fitting test (feature `tuning`)
/// can move them; the plugin never touches them.
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    /// Contour, manual: time constant of each of the two cascaded poles as a share of the
    /// release setting. Fitted to G section 5 (63 % at 0.39 x, 90 % at 0.75 x the setting).
    pub contour_manual_ratio: f64,
    /// Contour: how long the gain is held after the last peak before the release starts, ms.
    /// Fitted to the depth 3 ms after the bursts of the burst-length table (G section 5).
    pub contour_hold_ms: f64,
    /// Contour, auto: pole time constant in ms of the slow stage, per profile. G section 5:
    /// "the shape of a manual release of about 2000 ms", the fourth profile exactly half.
    pub contour_slow_ms: [f64; 4],
    /// Contour, auto: pole time constant in ms of the fast stage (short peaks), per profile.
    pub contour_fast_ms: [f64; 4],
    /// Contour, auto: how long limiting must last before the slow stage starts to charge, ms.
    pub contour_qualify_ms: [f64; 4],
    /// Contour, auto: charge time constant of the slow stage's output pole, ms.
    pub contour_charge_out_ms: [f64; 4],
    /// Contour, auto: charge time constant of the slow stage's inner pole, ms.
    pub contour_charge_in_ms: [f64; 4],
    /// Leveller, manual: pole time constant as a share of the setting. H section 4
    /// (50 % at 0.19 x, 90 % at 0.79 x, 99 % at about 1.7 x).
    pub leveller_manual_ratio: f64,
    /// Leveller, auto: fast stage pole, ms.
    pub leveller_fast_ms: f64,
    /// Leveller, auto: qualification time, ms.
    pub leveller_qualify_ms: f64,
    /// Leveller, auto: charge time constant of the slow stage's inner pole, ms (a pole in dB).
    pub leveller_charge_ms: f64,
    /// Leveller, auto: charge time constant of the slow stage's output pole, ms (a pole in dB).
    pub leveller_charge_out_ms: f64,
    /// Leveller, auto: release time constant of the slow stage's output pole, ms.
    pub leveller_smooth_ms: f64,
    /// Leveller, auto: slow stage release time constant, ms.
    pub leveller_slow_ms: f64,
    /// Leveller, auto: the slow stage works on gain raised to this power.
    pub leveller_power: f64,
    /// Strict stage: pole time constant of its two-pole release, ms.
    pub strict_release_ms: f64,
    /// Strict stage: safety margin under the ceiling, dB (positive).
    pub strict_margin_db: f64,
}

impl Tuning {
    pub const DEFAULT: Tuning = Tuning {
        contour_manual_ratio: 0.2165,
        contour_hold_ms: 0.85,
        contour_slow_ms: [433.0, 433.0, 433.0, 216.5],
        contour_fast_ms: [0.8, 0.5, 3.8, 0.3],
        contour_qualify_ms: [5.0, 8.0, 7.0, 12.0],
        contour_charge_out_ms: [8.0, 4.0, 3.0, 3.0],
        contour_charge_in_ms: [26.0, 10.0, 12.0, 8.0],
        leveller_manual_ratio: 0.435,
        leveller_fast_ms: 0.2,
        leveller_qualify_ms: 14.0,
        leveller_charge_ms: 130.0,
        leveller_charge_out_ms: 40.0,
        leveller_smooth_ms: 15.0,
        leveller_slow_ms: 192.0,
        leveller_power: 0.55,
        strict_release_ms: 30.0,
        strict_margin_db: 0.02,
    };
}

/// Contour: delay in samples at 48 kHz (G section 1: 36 measured, 35 reported by the original).
const CONTOUR_DELAY_48K: f64 = 36.0;
/// Leveller: delay in samples at 48 kHz (H section 1 and 2).
const LEVELLER_DELAY_48K: f64 = 64.0;
/// Contour: margin of the reconstructed-peak path under the ceiling, dB (G section 3: a
/// steady sine is held at ceiling - 0.06 dB in every case).
const CONTOUR_TP_MARGIN_DB: f64 = 0.06;
/// Contour: droop of the reconstructed-peak detector at 12 kHz, dB (G section 6: the
/// inter-sample tone comes out 0.07 dB over the ceiling while the 1 kHz sine is 0.06 under).
const CONTOUR_TP_DROOP_12K_DB: f64 = 0.13;
/// Continuous soft-clip curve: scale of the error-function shaper relative to the threshold
/// (G section 4: -0.09 dB and -50 dB third harmonic for a sine at the threshold).
const CONTINUOUS_SCALE: f64 = 5.05;

/// Contour detector: 3 fractional phases (4x), 16-tap interpolator widened to 18 by the droop.
const DET_PHASES: usize = 3;
const DET_TAPS: usize = 18;
const DET_HALF: usize = 9;
/// Strict stage detector: 7 fractional phases (8x), 128 taps per phase.
const GRD_PHASES: usize = 7;
const GRD_TAPS: usize = 128;
const GRD_HALF: usize = 64;
/// Strict stage detector: Kaiser beta of the interpolator.
const GRD_BETA: f64 = 4.0;
/// Strict stage: the interpolator cannot reconstruct the top of the band (above about 0.49 x
/// the sample rate). A low-pass with its half-amplitude point here splits that part off; its
/// envelope is added to the peak estimate.
const GRD_TOP_EDGE: f64 = 0.488;
/// Strict stage: samples either side over which the top-of-band envelope is held.
const GRD_TOP_SPAN: usize = 32;
/// Strict stage: weight of the top-of-band envelope in the peak estimate.
const GRD_TOP_WEIGHT: f64 = 1.0;
/// Strict stage: samples on each side of a peak over which its gain is held flat.
const GRD_FLAT: usize = 16;
/// Strict stage: length of its gain ramp in samples at 48 kHz.
const GRD_RAMP_48K: f64 = 48.0;

// ---------------------------------------------------------------------------------------
// Small building blocks
// ---------------------------------------------------------------------------------------

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * (core::f64::consts::LN_10 / 20.0)).exp()
}

#[inline]
fn pole(ms: f64, sr: f64) -> f64 {
    let samples = ms * 0.001 * sr;
    if samples <= 1.0e-6 {
        1.0
    } else {
        1.0 - (-1.0 / samples).exp()
    }
}

fn pow2_at_least(n: usize) -> usize {
    let mut p = 2;
    while p < n {
        p <<= 1;
    }
    p
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    let mut k = 1.0;
    while k < 60.0 {
        term *= q / (k * k);
        sum += term;
        if term < 1e-18 * sum {
            break;
        }
        k += 1.0;
    }
    sum
}

/// Kaiser-windowed sinc sampled at `j - frac`, j = -(taps/2 - 1) ..= taps/2, unit DC gain.
fn interp_kernel(out: &mut [f64], frac: f64, beta: f64) {
    let taps = out.len();
    let half = (taps / 2) as f64;
    let norm = bessel_i0(beta);
    let mut sum = 0.0;
    for (i, o) in out.iter_mut().enumerate() {
        let j = i as f64 - (half - 1.0);
        let t = j - frac;
        let s = if t.abs() < 1e-12 { 1.0 } else { (core::f64::consts::PI * t).sin() / (core::f64::consts::PI * t) };
        let r = t / half;
        let w = if r.abs() >= 1.0 { 0.0 } else { bessel_i0(beta * (1.0 - r * r).sqrt()) / norm };
        *o = s * w;
        sum += *o;
    }
    if sum.abs() > 1e-12 {
        for o in out.iter_mut() {
            *o /= sum;
        }
    }
}

/// erf by its power series (|z| < 3) and the asymptotic series beyond. Smooth and odd.
fn erf(z: f64) -> f64 {
    let a = z.abs();
    if a < 3.0 {
        let z2 = z * z;
        let mut term = z;
        let mut sum = z;
        let mut n = 0.0;
        while n < 80.0 {
            n += 1.0;
            term *= -z2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() < 1e-13 {
                break;
            }
        }
        sum * core::f64::consts::FRAC_2_SQRT_PI
    } else {
        let z2 = a * a;
        let c = (-z2).exp() / (a * core::f64::consts::PI.sqrt()) * (1.0 - 0.5 / z2 + 0.75 / (z2 * z2));
        let v = 1.0 - c;
        if z < 0.0 {
            -v
        } else {
            v
        }
    }
}

/// Audio delay line with history. Every sample is written twice, one buffer length apart, so
/// the most recent samples are always available as one contiguous slice.
#[derive(Default)]
struct Ring {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Ring {
    fn alloc(&mut self, min_len: usize) {
        let n = pow2_at_least(min_len);
        self.buf = vec![0.0; 2 * n];
        self.mask = n - 1;
        self.pos = 0;
    }
    fn clear(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
        self.pos = 0;
    }
    #[inline]
    fn push(&mut self, v: f32) {
        self.pos = (self.pos + 1) & self.mask;
        if let Some(s) = self.buf.get_mut(self.pos) {
            *s = v;
        }
        if let Some(s) = self.buf.get_mut(self.pos + self.mask + 1) {
            *s = v;
        }
    }
    /// `back` = 0 is the sample just pushed.
    #[inline]
    fn at(&self, back: usize) -> f32 {
        self.buf.get(self.pos.wrapping_sub(back) & self.mask).copied().unwrap_or(0.0)
    }
    /// `len` samples, oldest first, the last of them `back` samples before the newest.
    /// Empty when the request is longer than the history.
    #[inline]
    fn window(&self, len: usize, back: usize) -> &[f32] {
        let end = self.pos + self.mask + 2;
        if back + len > self.mask + 1 {
            return &[];
        }
        self.buf.get(end - back - len..end - back).unwrap_or(&[])
    }
}

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    // four accumulators so the compiler can vectorise without reordering concerns
    let mut acc = [0.0f32; 4];
    let mut ca = a.chunks_exact(4);
    let mut cb = b.chunks_exact(4);
    for (x, y) in (&mut ca).zip(&mut cb) {
        acc[0] += x[0] * y[0];
        acc[1] += x[1] * y[1];
        acc[2] += x[2] * y[2];
        acc[3] += x[3] * y[3];
    }
    let mut rest = 0.0f32;
    for (x, y) in ca.remainder().iter().zip(cb.remainder().iter()) {
        rest += x * y;
    }
    acc[0] + acc[1] + acc[2] + acc[3] + rest
}

/// Sliding minimum or maximum over the last `window` values (monotonic queue).
#[derive(Default)]
struct Wedge {
    val: Vec<f64>,
    idx: Vec<usize>,
    mask: usize,
    head: usize,
    tail: usize,
    n: usize,
    is_max: bool,
}

impl Wedge {
    fn alloc(&mut self, max_window: usize, is_max: bool) {
        let n = pow2_at_least(max_window + 2);
        self.val = vec![1.0; n];
        self.idx = vec![0; n];
        self.mask = n - 1;
        self.is_max = is_max;
        self.clear();
    }
    fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.n = 0;
    }
    #[inline]
    fn push(&mut self, v: f64, window: usize) -> f64 {
        let window = window.clamp(1, self.mask);
        self.n = self.n.wrapping_add(1);
        while self.tail != self.head {
            let last = self.val.get(self.tail.wrapping_sub(1) & self.mask).copied().unwrap_or(v);
            let drop = if self.is_max { last <= v } else { last >= v };
            if drop {
                self.tail = self.tail.wrapping_sub(1);
            } else {
                break;
            }
        }
        let t = self.tail & self.mask;
        if let (Some(a), Some(b)) = (self.val.get_mut(t), self.idx.get_mut(t)) {
            *a = v;
            *b = self.n;
        }
        self.tail = self.tail.wrapping_add(1);
        while self.tail.wrapping_sub(self.head) > 1 {
            let first = self.idx.get(self.head & self.mask).copied().unwrap_or(self.n);
            if self.n.wrapping_sub(first) >= window {
                self.head = self.head.wrapping_add(1);
            } else {
                break;
            }
        }
        self.val.get(self.head & self.mask).copied().unwrap_or(v)
    }
}

/// Moving average with an exact running sum (the sum is rebuilt every `len` samples).
#[derive(Default)]
struct BoxAvg {
    buf: Vec<f64>,
    mask: usize,
    pos: usize,
    len: usize,
    run: f64,
    fresh: f64,
    count: usize,
}

impl BoxAvg {
    fn alloc(&mut self, max_len: usize) {
        let n = pow2_at_least(max_len + 2);
        self.buf = vec![1.0; n];
        self.mask = n - 1;
        self.len = 1;
        self.clear();
    }
    fn set_len(&mut self, len: usize) {
        self.len = len.clamp(1, self.mask);
        self.clear();
    }
    fn clear(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 1.0;
        }
        self.pos = 0;
        self.run = self.len as f64;
        self.fresh = 0.0;
        self.count = 0;
    }
    #[inline]
    fn push(&mut self, v: f64) -> f64 {
        self.pos = (self.pos + 1) & self.mask;
        let old = self.buf.get(self.pos.wrapping_sub(self.len) & self.mask).copied().unwrap_or(1.0);
        if let Some(s) = self.buf.get_mut(self.pos) {
            *s = v;
        }
        self.run += v - old;
        self.fresh += v;
        self.count += 1;
        if self.count >= self.len {
            self.run = self.fresh;
            self.fresh = 0.0;
            self.count = 0;
        }
        self.run / self.len as f64
    }
}

// ---------------------------------------------------------------------------------------
// Release envelope
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct EnvCfg {
    auto: bool,
    two_pole: bool,
    k_manual: f64,
    k_fast: f64,
    k_slow: f64,
    k_follow: f64,
    geometric: bool,
    k_charge_in: f64,
    k_charge_out: f64,
    use_power: bool,
    power: f64,
    inv_power: f64,
    qualify: usize,
}

#[derive(Clone, Copy)]
struct EnvState {
    y1: f64,
    y2: f64,
    s1: f64,
    s2: f64,
}

impl EnvState {
    const REST: EnvState = EnvState { y1: 1.0, y2: 1.0, s1: 1.0, s2: 1.0 };

    /// `held` is the look-ahead minimum of the required gain (<= 1); `qualified` is the
    /// largest `held` of the last `qualify` samples (only used in auto). Returns a gain that
    /// is never above `held`.
    #[inline]
    fn step(&mut self, c: &EnvCfg, held: f64, qualified: f64) -> f64 {
        let k = if c.auto { c.k_fast } else { c.k_manual };
        // Fast / manual stage: down at once, up through one or two poles (linear gain).
        self.y1 = held.min(self.y1 + k * (1.0 - self.y1));
        let fast = if c.two_pole {
            self.y2 = self.y1.min(self.y2 + k * (self.y1 - self.y2));
            self.y2
        } else {
            self.y1
        };
        if !c.auto {
            return fast;
        }
        // Slow stage: two cascaded poles on gain^power. Both charge only towards reduction
        // that has lasted; the output pole charges faster than the inner one, so a
        // medium-length peak leaves the pair already "in motion" when the release starts.
        // Contour: power 1, equal poles (critically damped), linear charge.
        // Leveller: power below 1, a long inner and a short output pole, charge as a pole in
        // dB (a geometric step is a pole in dB whatever the power).
        let q = if c.use_power { qualified.powf(c.power) } else { qualified };
        if q < self.s1 {
            self.s1 = charge(self.s1, q, c.k_charge_in, c.geometric);
        } else {
            self.s1 += c.k_slow * (1.0 - self.s1);
        }
        self.s2 += c.k_follow * (self.s1 - self.s2);
        if q < self.s2 {
            self.s2 = charge(self.s2, q, c.k_charge_out, c.geometric);
        }
        let slow = self.s2;
        let slow = if c.use_power { slow.max(0.0).powf(c.inv_power) } else { slow };
        fast.min(slow)
    }
}

#[inline]
fn charge(state: f64, target: f64, k: f64, geometric: bool) -> f64 {
    if geometric {
        state * (k * (target / state).ln()).exp()
    } else {
        state + k * (target - state)
    }
}

// ---------------------------------------------------------------------------------------
// Strict true-peak stage (own addition)
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Guard {
    l: Ring,
    r: Ring,
    hold: Wedge,
    top_hold: Wedge,
    ramp: BoxAvg,
    kernel: Vec<[f32; GRD_TAPS]>,
    low_pass: Vec<f32>,
    y1: f64,
    y2: f64,
    k_release: f64,
    window: usize,
    delay: usize,
}

impl Guard {
    fn prepare(&mut self, sr: f64) {
        let ramp = ((GRD_RAMP_48K * sr / 48000.0).round() as usize).max(2);
        // Index algebra (r[i] is the required gain for the interval between samples i and
        // i+1): the output sample j must carry a gain <= r[i] for every i in
        // [j-1-FLAT, j+FLAT]. With a minimum over `window` values followed by a mean over
        // `ramp` values this holds when delay >= ramp + FLAT - 1 and
        // window >= delay + FLAT + 2.
        self.delay = ramp + GRD_FLAT - 1;
        self.window = self.delay + GRD_FLAT + 2;
        let history = GRD_HALF + GRD_TOP_SPAN + 1 + self.delay + GRD_TAPS + 4;
        self.l.alloc(history);
        self.r.alloc(history);
        self.hold.alloc(self.window, false);
        self.top_hold.alloc(2 * GRD_TOP_SPAN + 4, true);
        self.ramp.alloc(ramp);
        self.ramp.set_len(ramp);
        self.kernel = vec![[0.0; GRD_TAPS]; GRD_PHASES];
        let mut k = [0.0f64; GRD_TAPS];
        for (p, row) in self.kernel.iter_mut().enumerate() {
            interp_kernel(&mut k, (p + 1) as f64 / (GRD_PHASES + 1) as f64, GRD_BETA);
            for (a, b) in row.iter_mut().zip(k.iter()) {
                *a = *b as f32;
            }
        }
        // Linear-phase low-pass, 2*HALF+1 taps, half amplitude at GRD_TOP_EDGE x the sample
        // rate. What it removes (the top of the band) is what the interpolator cannot see.
        let norm = bessel_i0(GRD_BETA);
        self.low_pass = vec![0.0; 2 * GRD_HALF + 1];
        let mut sum = 0.0;
        for (i, c) in self.low_pass.iter_mut().enumerate() {
            let t = i as f64 - GRD_HALF as f64;
            let x = 2.0 * GRD_TOP_EDGE * t;
            let sinc = if t == 0.0 { 1.0 } else { (core::f64::consts::PI * x).sin() / (core::f64::consts::PI * x) };
            let r = t / (GRD_HALF as f64 + 1.0);
            let w = bessel_i0(GRD_BETA * (1.0 - r * r).max(0.0).sqrt()) / norm;
            let v = 2.0 * GRD_TOP_EDGE * sinc * w;
            sum += v;
            *c = v as f32;
        }
        for c in self.low_pass.iter_mut() {
            *c = (*c as f64 / sum) as f32;
        }
        self.clear();
    }
    fn clear(&mut self) {
        self.l.clear();
        self.r.clear();
        self.hold.clear();
        self.top_hold.clear();
        self.ramp.clear();
        self.y1 = 1.0;
        self.y2 = 1.0;
    }
    fn latency(&self) -> usize {
        GRD_HALF + GRD_TOP_SPAN + 1 + self.delay
    }
    #[inline]
    fn tick(&mut self, l: f32, r: f32, limit: f64) -> (f32, f32, f64) {
        self.l.push(l);
        self.r.push(r);
        // Top-of-band part of the sample HALF back: x - lowpass(x).
        let ll = dot(&self.low_pass, self.l.window(2 * GRD_HALF + 1, 0));
        let lr = dot(&self.low_pass, self.r.window(2 * GRD_HALF + 1, 0));
        let top = (self.l.at(GRD_HALF) - ll).abs().max((self.r.at(GRD_HALF) - lr).abs()) as f64;
        // Largest top-of-band sample within TOP_SPAN samples either side of the interval.
        let top = self.top_hold.push(top, 2 * GRD_TOP_SPAN + 2);
        // Interval between samples m and m+1, m = now - HALF - TOP_SPAN - 1.
        let shift = GRD_TOP_SPAN + 1;
        let mut peak = self.l.at(GRD_HALF + shift).abs().max(self.r.at(GRD_HALF + shift).abs());
        peak = peak.max(self.l.at(GRD_HALF + shift - 1).abs()).max(self.r.at(GRD_HALF + shift - 1).abs());
        // tap i multiplies x[m + i - (HALF-1)]: the TAPS samples ending `shift` back, oldest first
        let (wl, wr) = (self.l.window(GRD_TAPS, shift), self.r.window(GRD_TAPS, shift));
        for row in self.kernel.iter() {
            peak = peak.max(dot(row, wl).abs()).max(dot(row, wr).abs());
        }
        // Upper estimate of the reconstructed peak: what the interpolator sees plus the
        // envelope of what it cannot see.
        let peak = peak as f64 + GRD_TOP_WEIGHT * top;
        let need = if peak > limit { (limit / peak).max(1.0e-9) } else { 1.0 };
        let held = self.hold.push(need, self.window);
        self.y1 = held.min(self.y1 + self.k_release * (1.0 - self.y1));
        self.y2 = self.y1.min(self.y2 + self.k_release * (self.y1 - self.y2));
        let g = self.ramp.push(self.y2);
        let back = GRD_HALF + shift + self.delay;
        ((self.l.at(back) as f64 * g) as f32, (self.r.at(back) as f64 * g) as f32, g)
    }
}

// ---------------------------------------------------------------------------------------
// The module
// ---------------------------------------------------------------------------------------

/// The limiter. See the crate documentation and `REPORT.md`.
pub struct WallLimiter {
    params: [f32; N_PARAMS],
    tuning: Tuning,
    sr: f64,
    ready: bool,

    // derived from parameters
    leveller: bool,
    strict: bool,
    pre_gain: f64,
    ceiling: f64,
    tp_limit: f64,
    guard_limit: f64,
    clip_on: bool,
    clip_curve: usize,
    clip_threshold: f64,
    clip_max: f64,
    clip_end: f64,
    env_cfg: EnvCfg,
    delay: usize,
    window: usize,
    hold_window: usize,
    det_half: usize,

    // sizes fixed by the sample rate
    contour_delay: usize,
    leveller_delay: usize,

    // state
    in_l: Ring,
    in_r: Ring,
    hold: Wedge,
    qual: Wedge,
    ramp: BoxAvg,
    ramp_b: BoxAvg,
    env: EnvState,
    det_kernel: [[f32; DET_TAPS]; DET_PHASES],
    prev_tp_need: f64,
    guard: Guard,
    meter_min_gain: f64,
    meters: Meters,
}

impl Default for WallLimiter {
    fn default() -> Self {
        let mut params = [0.0; N_PARAMS];
        for (p, d) in params.iter_mut().zip(PARAMS.iter()) {
            *p = d.default;
        }
        Self {
            params,
            tuning: Tuning::DEFAULT,
            sr: 48000.0,
            ready: false,
            leveller: false,
            strict: false,
            pre_gain: 1.0,
            ceiling: 1.0,
            tp_limit: 1.0,
            guard_limit: 1.0,
            clip_on: false,
            clip_curve: 0,
            clip_threshold: 1.0,
            clip_max: 1.0,
            clip_end: 1.0,
            env_cfg: EnvCfg::default(),
            delay: 36,
            window: 28,
            hold_window: 28,
            det_half: DET_HALF,
            contour_delay: 36,
            leveller_delay: 64,
            in_l: Ring::default(),
            in_r: Ring::default(),
            hold: Wedge::default(),
            qual: Wedge::default(),
            ramp: BoxAvg::default(),
            ramp_b: BoxAvg::default(),
            env: EnvState::REST,
            det_kernel: [[0.0; DET_TAPS]; DET_PHASES],
            prev_tp_need: 1.0,
            guard: Guard::default(),
            meter_min_gain: 1.0,
            meters: Meters::default(),
        }
    }
}

impl WallLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Model constants, for the fitting test only.
    #[cfg(feature = "tuning")]
    pub fn tuning(&self) -> Tuning {
        self.tuning
    }

    /// Replaces the model constants, for the fitting test only.
    #[cfg(feature = "tuning")]
    pub fn set_tuning(&mut self, t: Tuning) {
        self.tuning = t;
        self.update(true);
    }

    fn max_qualify_samples(&self) -> usize {
        // Room for qualification times up to 60 ms.
        (0.060 * self.sr).ceil() as usize + 2
    }

    /// Recomputes everything derived from the parameters. `structural` clears the state
    /// (style or strict switch: the delay changes).
    fn update(&mut self, structural: bool) {
        let p = &self.params;
        let t = &self.tuning;
        let sr = self.sr;
        self.leveller = p[P_STYLE] >= 0.5;
        self.strict = p[P_STRICT_TRUE_PEAK] >= 0.5;
        let ceiling_db = p[P_CEILING] as f64;
        self.ceiling = db_to_lin(ceiling_db);
        // Contour: gain = drive (G section 3; threshold, a Leveller control, also acts so that
        // no control is dead). Leveller: gain below the limit = ceiling - threshold (H section 2).
        let mut gain_db = p[P_DRIVE] as f64 - p[P_THRESHOLD] as f64;
        if self.leveller {
            gain_db += ceiling_db;
        }
        self.pre_gain = db_to_lin(gain_db);
        self.tp_limit = self.ceiling * db_to_lin(-CONTOUR_TP_MARGIN_DB);

        self.clip_on = p[P_SOFT_CLIP] >= 0.5;
        self.clip_curve = (p[P_SOFT_CLIP_CURVE].round() as usize).min(2);
        self.clip_threshold = db_to_lin(p[P_SOFT_CLIP_THRESHOLD] as f64);
        // G section 4: knee starts at the threshold, output tops out 9 dB (wide) or 2 dB
        // (narrow) above it; quadratic in the linear domain.
        let knee_db = if self.clip_curve == 2 { 2.0 } else { 9.0 };
        self.clip_max = self.clip_threshold * db_to_lin(knee_db);
        self.clip_end = 2.0 * self.clip_max - self.clip_threshold;

        let auto = p[P_AUTO_RELEASE] >= 0.5;
        let release_ms = (p[P_RELEASE] as f64).clamp(0.01, 3000.0);
        let prof = (p[P_PROFILE].round() as usize).min(3);
        let mut c = EnvCfg { auto, ..EnvCfg::default() };
        if self.leveller {
            c.two_pole = false;
            c.k_manual = pole(release_ms * t.leveller_manual_ratio, sr);
            c.k_fast = pole(t.leveller_fast_ms, sr);
            c.k_slow = pole(t.leveller_slow_ms, sr);
            c.k_follow = pole(t.leveller_smooth_ms, sr);
            c.geometric = true;
            c.k_charge_in = pole(t.leveller_charge_ms, sr);
            c.k_charge_out = pole(t.leveller_charge_out_ms, sr);
            c.power = t.leveller_power.clamp(0.05, 4.0);
            c.inv_power = 1.0 / c.power;
            c.use_power = (c.power - 1.0).abs() > 1e-9;
            c.qualify = (t.leveller_qualify_ms * 0.001 * sr).round() as usize;
            self.delay = self.leveller_delay;
            self.det_half = 0;
        } else {
            c.two_pole = true;
            c.k_manual = pole(release_ms * t.contour_manual_ratio, sr);
            c.k_fast = pole(t.contour_fast_ms[prof], sr);
            c.k_slow = pole(t.contour_slow_ms[prof], sr);
            c.k_follow = c.k_slow;
            c.geometric = false;
            c.k_charge_in = pole(t.contour_charge_in_ms[prof], sr);
            c.k_charge_out = pole(t.contour_charge_out_ms[prof], sr);
            c.power = 1.0;
            c.inv_power = 1.0;
            c.use_power = false;
            c.qualify = (t.contour_qualify_ms[prof] * 0.001 * sr).round() as usize;
            self.delay = self.contour_delay;
            self.det_half = DET_HALF;
        }
        c.qualify = c.qualify.clamp(1, self.max_qualify_samples());
        self.env_cfg = c;
        // Minimum window = ramp length = delay - detector look-ahead + 1, so the smoothed gain
        // is never above the gain the delayed sample needs.
        let window = (self.delay + 1).saturating_sub(self.det_half).max(1);
        if window != self.window || structural {
            self.window = window;
            // Contour: one moving average (linear ramp). Leveller: two in series whose
            // lengths add up to window + 1 (triangular weights over the same span), which
            // starts the ramp more gently (H section 3: gain 1 ms before a burst).
            let first = if self.leveller { (window + 1) / 2 } else { window };
            self.ramp.set_len(first);
            self.ramp_b.set_len(window + 1 - first);
        }
        let extra = if self.leveller { 0 } else { (t.contour_hold_ms.clamp(0.0, 5.0) * 0.001 * sr).round() as usize };
        self.hold_window = self.window + extra;
        self.guard.k_release = pole(t.strict_release_ms, sr);
        self.guard_limit = self.ceiling * db_to_lin(-t.strict_margin_db);
        if structural {
            self.clear_state();
        }
    }

    fn clear_state(&mut self) {
        self.in_l.clear();
        self.in_r.clear();
        self.hold.clear();
        self.qual.clear();
        self.ramp.clear();
        self.ramp_b.clear();
        self.env = EnvState::REST;
        self.prev_tp_need = 1.0;
        self.guard.clear();
        self.meter_min_gain = 1.0;
        self.meters = Meters::default();
    }

    /// The input soft clipper (G section 4). Instant, no memory.
    #[inline]
    fn soft_clip(&self, x: f64) -> f64 {
        let t = self.clip_threshold;
        if self.clip_curve == 1 {
            // Continuous: acts from far below the threshold, one shape that moves with it.
            let s = t * CONTINUOUS_SCALE;
            return s * 0.886_226_925_452_758 * erf(x / s);
        }
        let a = x.abs();
        if a <= t {
            return x;
        }
        let y = if a < self.clip_end {
            let d = a - t;
            a - d * d / (2.0 * (self.clip_end - t))
        } else {
            self.clip_max
        };
        if x < 0.0 {
            -y
        } else {
            y
        }
    }

    #[inline]
    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mut xl = if l.is_finite() { l as f64 } else { 0.0 };
        let mut xr = if r.is_finite() { r as f64 } else { 0.0 };
        if self.clip_on {
            xl = self.soft_clip(xl);
            xr = self.soft_clip(xr);
        }
        xl = (xl * self.pre_gain).clamp(-1.0e6, 1.0e6);
        xr = (xr * self.pre_gain).clamp(-1.0e6, 1.0e6);
        self.in_l.push(xl as f32);
        self.in_r.push(xr as f32);

        // Required gain for the sample `det_half` back (linked: the larger channel decides).
        let half = self.det_half;
        let sp = self.in_l.at(half).abs().max(self.in_r.at(half).abs()) as f64;
        let mut need = if sp > self.ceiling { (self.ceiling / sp).max(1.0e-9) } else { 1.0 };
        if !self.leveller {
            // Reconstructed peak between this sample and the next, through the drooping
            // interpolator, held 0.06 dB under the ceiling (G sections 3 and 6).
            let mut tp = 0.0f32;
            let (wl, wr) = (self.in_l.window(DET_TAPS, 0), self.in_r.window(DET_TAPS, 0));
            for row in self.det_kernel.iter() {
                tp = tp.max(dot(row, wl).abs()).max(dot(row, wr).abs());
            }
            let tp = tp as f64;
            let tp_need = if tp > self.tp_limit { (self.tp_limit / tp).max(1.0e-9) } else { 1.0 };
            // The interval before this sample also touches it.
            need = need.min(tp_need).min(self.prev_tp_need);
            self.prev_tp_need = tp_need;
        }

        let held = self.hold.push(need, self.hold_window);
        let qualified = if self.env_cfg.auto { self.qual.push(held, self.env_cfg.qualify) } else { 1.0 };
        let e = self.env.step(&self.env_cfg, held, qualified);
        let g = self.ramp_b.push(self.ramp.push(e));

        let c = self.ceiling;
        let mut ol = (self.in_l.at(self.delay) as f64 * g).clamp(-c, c) as f32;
        let mut or = (self.in_r.at(self.delay) as f64 * g).clamp(-c, c) as f32;
        let mut total = g;
        if self.strict {
            let (gl, gr, gg) = self.guard.tick(ol, or, self.guard_limit);
            ol = gl;
            or = gr;
            total *= gg;
        }
        if total < self.meter_min_gain {
            self.meter_min_gain = total;
        }
        (ol, or)
    }
}

impl Module for WallLimiter {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 8000.0 { sample_rate as f64 } else { 48000.0 };
        self.sr = sr;
        // Free choice: the look-ahead is kept constant in time, so the delay scales with the rate.
        self.contour_delay = ((CONTOUR_DELAY_48K * sr / 48000.0).round() as usize).max(DET_HALF + 1);
        self.leveller_delay = ((LEVELLER_DELAY_48K * sr / 48000.0).round() as usize).max(1);
        let max_delay = self.contour_delay.max(self.leveller_delay);
        self.in_l.alloc(max_delay + DET_TAPS + 4);
        self.in_r.alloc(max_delay + DET_TAPS + 4);
        self.hold.alloc(max_delay + (0.005 * sr) as usize + 4, false);
        self.qual.alloc(self.max_qualify_samples() + 2, true);
        self.ramp.alloc(max_delay + 2);
        self.ramp_b.alloc(max_delay + 2);

        // Contour detector: 4x Kaiser-windowed sinc (16 taps) convolved with a three-tap droop
        // [a, 1-2a, a] that is 0.13 dB down at 12 kHz.
        let w = 2.0 * core::f64::consts::PI * 12000.0 / sr;
        let a = if w < core::f64::consts::PI {
            ((1.0 - db_to_lin(-CONTOUR_TP_DROOP_12K_DB)) / (2.0 * (1.0 - w.cos()))).min(0.02)
        } else {
            0.0
        };
        let mut base = [0.0f64; DET_TAPS - 2];
        for (p, row) in self.det_kernel.iter_mut().enumerate() {
            interp_kernel(&mut base, (p + 1) as f64 / (DET_PHASES + 1) as f64, 7.0);
            let mut wide = [0.0f64; DET_TAPS];
            for (i, b) in base.iter().enumerate() {
                wide[i] += a * b;
                wide[i + 1] += (1.0 - 2.0 * a) * b;
                wide[i + 2] += a * b;
            }
            for (o, v) in row.iter_mut().zip(wide.iter()) {
                *o = *v as f32;
            }
        }
        self.guard.prepare(sr);
        self.ready = true;
        self.update(true);
    }

    fn reset(&mut self) {
        self.clear_state();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(d) = PARAMS.get(index) else { return };
        let mut v = if value.is_finite() { value.clamp(d.min, d.max) } else { d.default };
        if d.steps > 0 {
            v = v.round();
        }
        let Some(slot) = self.params.get_mut(index) else { return };
        if *slot == v {
            return;
        }
        *slot = v;
        if self.ready {
            self.update(index == P_STYLE || index == P_STRICT_TRUE_PEAK);
        }
    }

    fn latency(&self) -> usize {
        let base = if self.params[P_STYLE] >= 0.5 { self.leveller_delay } else { self.contour_delay };
        if self.params[P_STRICT_TRUE_PEAK] >= 0.5 {
            base + self.guard.latency()
        } else {
            base
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.ready {
            return;
        }
        self.meter_min_gain = 1.0;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (a, b) = self.tick(*l, *r);
            *l = a;
            *r = b;
        }
        let g = self.meter_min_gain.max(1e-10);
        self.meters.reduction_db = if g < 1.0 { (-20.0 * g.log10()) as f32 } else { 0.0 };
    }

    fn meters(&self) -> Meters {
        self.meters
    }
}
