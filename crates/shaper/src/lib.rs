//! Shaper: upward compression, transient shaping and a soft clipper with selectable curve
//! families (MASTER_BRIEF.md section 4.5, chain stages 7 and 8, build-order step 6).
//!
//! Signal flow, per stereo sample pair (dynamics are linked across the two channels):
//!
//! ```text
//! in -> upward compression (D, SPEC_LIMITERS.md 49 / 49.3)
//!    -> transient shaping (D, SPEC_LIMITERS.md 49 / 49.2 / 52)
//!    -> drive gain
//!    -> [optional 4x oversampling] curve(threshold) [decimate]   = wet
//!    -> out = (1 - mix) * dry + mix * wet, dry delayed to match the wet path
//! ```
//!
//! Curve families (all expressed in units of the threshold `t`, so `y = t * f(x / t)`):
//! * Dense light / medium / heavy: `f(u) = tanh(g u) / tanh(g)`. A peak at the threshold stays
//!   at the threshold and everything below it is lifted. `g` is calibrated so that a dense,
//!   peak-normalised programme proxy rises by 1.3, 1.9 and 2.2 dB at full mix
//!   (SPEC_LIMITERS.md 48.2). Own design; the spec gives only those level rises.
//! * Quadratic knees with a ceiling `L` dB above the threshold: identity up to the threshold, then
//!   `f(u) = u - (u - 1)^2 / (2 (U - 1))` with `U = 2 L - 1`, and `L` from `U` on. The slope is
//!   continuous at both joins. `L` = 6.02 dB (exactly 2x) reproduces the symmetrical soft
//!   saturation of `params/srcF.md` section 8; `L` = 9 dB and 2 dB reproduce the "9 dB knee" and
//!   "2 dB knee" types of `params/srcG.md` section 4.
//! * Continuous: `f(u) = tanh(k u) / k` with `k` = 0.2 (`params/srcG.md` section 4, the
//!   "continuous knee" type; fitted to all its gain, distortion and peak cells).
//!
//! The 4x oversampler is this module's own addition (not from any source); it adds
//! `OS_LATENCY` samples, reported by `latency()`.

use olm_api::{Kind, Meters, Module, ParamDesc};

// ---------------------------------------------------------------- parameters

pub const P_CURVE: usize = 0;
pub const P_THRESHOLD: usize = 1;
pub const P_DRIVE: usize = 2;
pub const P_MIX: usize = 3;
pub const P_OVERSAMPLE: usize = 4;
pub const P_UPWARD: usize = 5;
pub const P_UPWARD_THRESHOLD: usize = 6;
pub const P_TRANSIENT: usize = 7;

pub const CURVE_OFF: usize = 0;
pub const CURVE_DENSE_LIGHT: usize = 1;
pub const CURVE_DENSE_MEDIUM: usize = 2;
pub const CURVE_DENSE_HEAVY: usize = 3;
pub const CURVE_QUAD_6DB: usize = 4;
pub const CURVE_QUAD_9DB: usize = 5;
pub const CURVE_CONTINUOUS: usize = 6;
pub const CURVE_QUAD_2DB: usize = 7;

const CURVE_LABELS: &[&str] = &[
    "Off",
    "Dense light",
    "Dense medium",
    "Dense heavy",
    "Quadratic 6 dB",
    "Quadratic 9 dB",
    "Continuous",
    "Quadratic 2 dB",
];

static PARAMS: [ParamDesc; 8] = [
    ParamDesc::choice("curve", "Curve", CURVE_LABELS, CURVE_OFF),
    ParamDesc::new("threshold", "Threshold", -24.0, 6.0, 0.0, Kind::Db),
    ParamDesc::new("drive", "Drive", -12.0, 24.0, 0.0, Kind::Db),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("oversample", "Oversample 4x", true),
    ParamDesc::new("upward", "Upward amount", 0.0, 10.0, 0.0, Kind::Db),
    ParamDesc::new("upwardThreshold", "Upward threshold", -60.0, 0.0, -16.0, Kind::Db),
    ParamDesc::new("transient", "Transient amount", 0.0, 200.0, 0.0, Kind::Percent),
];

// ---------------------------------------------------------------- curve constants

/// Dense family strengths `g` in `tanh(g u) / tanh(g)`. Calibrated on the programme proxy in
/// `tests/measured.rs` against the level rises of SPEC_LIMITERS.md 48.2 (1.3, 1.9, 2.2 dB).
const DENSE_G: [f64; 3] = [1.43334, 2.01739, 2.39374];
/// Continuous-knee constant (fit to `params/srcG.md` section 4, SMOOTH rows).
const CONT_K: f64 = 0.2;
/// Quadratic-knee ceilings above the threshold, as linear factors.
const QUAD_L_6: f64 = 2.0; // 6.02 dB, params/srcF.md section 8
const QUAD_L_9: f64 = 2.818_382_931_264_454; // 10^(9/20), params/srcG.md section 4
const QUAD_L_2: f64 = 1.258_925_411_794_167_2; // 10^(2/20), params/srcG.md section 4

#[inline]
fn quad_knee(u: f64, l: f64) -> f64 {
    let a = u.abs();
    let o = if a <= 1.0 {
        a
    } else {
        let top = 2.0 * l - 1.0;
        if a >= top {
            l
        } else {
            let e = a - 1.0;
            a - e * e / (2.0 * (top - 1.0))
        }
    };
    o.copysign(u)
}

/// The static curve of family `kind` (a `CURVE_*` index), `x` and the result in linear
/// amplitude, `threshold` linear (> 0). The editor draws the transfer plot with this; the
/// audio path runs the same formula through [`curve`].
pub fn transfer(kind: usize, x: f64, threshold: f64) -> f64 {
    let t = if threshold.is_finite() && threshold > 1e-12 { threshold } else { 1.0 };
    curve(kind, x, t, 1.0 / t)
}

/// The upward boost in dB for a detector `level_db`, as the audio path computes it.
pub fn upward_boost_db(amount_db: f64, threshold_db: f64, level_db: f64) -> f64 {
    amount_db * ((threshold_db - level_db) / UP_RANGE_DB).clamp(0.0, 1.0)
}

/// The editor read-out, one block at a time ([`Module::view`]). Levels are dBFS and boosts dB,
/// all floored at [`view::FLOOR_DB`]; every slot is a block maximum.
pub mod view {
    /// The deepest upward boost in the block, dB (0 when off).
    pub const UPWARD_DB: usize = 0;
    /// The largest transient boost in the block, dB (0 when off).
    pub const TRANSIENT_DB: usize = 1;
    /// The peak at the curve's input (after upward, transient and drive), dBFS.
    pub const DRIVE_PEAK_DB: usize = 2;
    /// The output peak, dBFS.
    pub const OUT_PEAK_DB: usize = 3;
    /// The module's input peak, before anything, dBFS.
    pub const IN_PEAK_DB: usize = 4;
    pub const LEN: usize = 5;
    pub const FLOOR_DB: f32 = -120.0;
}

/// The static curve, `x` and result in absolute units, `t` the threshold (linear, > 0).
#[inline]
fn curve(kind: usize, x: f64, t: f64, inv_t: f64) -> f64 {
    let u = x * inv_t;
    let f = match kind {
        CURVE_DENSE_LIGHT | CURVE_DENSE_MEDIUM | CURVE_DENSE_HEAVY => {
            let g = DENSE_G[kind - CURVE_DENSE_LIGHT];
            (g * u).tanh() / g.tanh()
        }
        CURVE_QUAD_6DB => quad_knee(u, QUAD_L_6),
        CURVE_QUAD_9DB => quad_knee(u, QUAD_L_9),
        CURVE_QUAD_2DB => quad_knee(u, QUAD_L_2),
        CURVE_CONTINUOUS => (CONT_K * u).tanh() / CONT_K,
        _ => u,
    };
    f * t
}

// ---------------------------------------------------------------- oversampler (own design)

/// Linear-phase FIR length at the 4x rate. `(OS_TAPS - 1) / 4` base samples per filter.
const OS_TAPS: usize = 129;
/// Interpolator plus decimator group delay, in base-rate samples: `2 * 64 / 4`.
pub const OS_LATENCY: usize = 32;
const PHASE_LEN: usize = 33; // ceil(129 / 4)
const XH_LEN: usize = 64; // base-rate history ring, power of two >= PHASE_LEN
const ZH_LEN: usize = 256; // 4x-rate history ring, power of two >= OS_TAPS
const DRY_LEN: usize = 64; // dry delay ring, power of two > OS_LATENCY

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..60 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < 1e-17 * sum {
            break;
        }
    }
    sum
}

/// Kaiser-windowed sinc low-pass at the 4x rate: cutoff 0.12 cycles per 4x sample (23 kHz at a
/// 48 kHz base rate), beta 7.86 (about 80 dB stop band). Normalised to unity DC gain.
fn design_os_filter() -> [f64; OS_TAPS] {
    let mut h = [0.0; OS_TAPS];
    let fc = 0.12;
    let beta = 7.857;
    let m = (OS_TAPS - 1) as f64;
    let i0b = bessel_i0(beta);
    let mut sum = 0.0;
    for (i, v) in h.iter_mut().enumerate() {
        let n = i as f64 - m / 2.0;
        let sinc = if n == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * n).sin() / (std::f64::consts::PI * n) };
        let r = 2.0 * i as f64 / m - 1.0;
        let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b;
        *v = sinc * w;
        sum += *v;
    }
    for v in h.iter_mut() {
        *v /= sum;
    }
    h
}

#[derive(Clone)]
struct OsChannel {
    xh: [f64; XH_LEN],
    xpos: usize,
    zh: [f64; ZH_LEN],
    zpos: usize,
}

impl OsChannel {
    fn new() -> Self {
        Self { xh: [0.0; XH_LEN], xpos: 0, zh: [0.0; ZH_LEN], zpos: 0 }
    }
    fn clear(&mut self) {
        *self = Self::new();
    }
}

// ---------------------------------------------------------------- dynamics constants (own design)

/// Upward compression: RMS detector times and fixed ratio (the source gives only an amount
/// in dB; SPEC_LIMITERS.md 49, 49.3). Below the upward threshold the boost is
/// `amount * clamp((threshold - level) / UP_RANGE_DB, 0, 1)` dB: it fades in over the first
/// `UP_RANGE_DB` below the threshold and holds the full amount further down.
const UP_ATTACK_S: f64 = 0.010;
const UP_RELEASE_S: f64 = 0.150;
pub const UP_RANGE_DB: f64 = 10.0;
/// Transient shaping: a fast and a slow peak follower; the boost is
/// `amount/100 * TR_SLOPE * (fast_dB - slow_dB)` dB, capped at `TR_MAX_DB` per 100 %.
const TR_FAST_ATTACK_S: f64 = 0.0002;
const TR_FAST_RELEASE_S: f64 = 0.010;
const TR_SLOW_ATTACK_S: f64 = 0.015;
const TR_SLOW_RELEASE_S: f64 = 0.150;
const TR_SLOPE: f64 = 0.052;
const TR_MAX_DB: f64 = 3.0;
/// Parameter smoothing time for drive, threshold and mix.
const SMOOTH_S: f64 = 0.005;

const LN10_20: f64 = std::f64::consts::LN_10 / 20.0;

#[inline]
fn coef(seconds: f64, sr: f64) -> f64 {
    (-1.0 / (seconds * sr)).exp()
}

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * LN10_20).exp()
}

/// Soft clipper with selectable curve families plus upward compression and transient shaping.
pub struct Shaper {
    // parameter targets (plain values)
    curve: usize,
    threshold_db: f64,
    drive_db: f64,
    mix: f64, // 0..1
    oversample: bool,
    upward_db: f64,
    upward_thr_db: f64,
    transient: f64, // 0..2

    // smoothed values
    s_thr: f64, // linear
    s_drive: f64,
    s_mix: f64,
    k_smooth: f64,

    sr: f64,
    // upward detector
    up_env: f64,
    up_att: f64,
    up_rel: f64,
    // transient detectors
    tr_fast: f64,
    tr_slow: f64,
    tr_fa: f64,
    tr_fr: f64,
    tr_sa: f64,
    tr_sr: f64,

    h: [f64; OS_TAPS],
    os: [OsChannel; 2],
    dry: [[f64; DRY_LEN]; 2],
    dry_pos: usize,

    meters: Meters,
    /// The last block's read-out, `view::LEN` values.
    shown: [f32; view::LEN],
}

impl Default for Shaper {
    fn default() -> Self {
        let mut s = Self {
            curve: CURVE_OFF,
            threshold_db: 0.0,
            drive_db: 0.0,
            mix: 1.0,
            oversample: true,
            upward_db: 0.0,
            upward_thr_db: -16.0,
            transient: 0.0,
            s_thr: 1.0,
            s_drive: 1.0,
            s_mix: 1.0,
            k_smooth: 0.0,
            sr: 48000.0,
            up_env: 0.0,
            up_att: 0.0,
            up_rel: 0.0,
            tr_fast: 0.0,
            tr_slow: 0.0,
            tr_fa: 0.0,
            tr_fr: 0.0,
            tr_sa: 0.0,
            tr_sr: 0.0,
            h: design_os_filter(),
            os: [OsChannel::new(), OsChannel::new()],
            dry: [[0.0; DRY_LEN]; 2],
            dry_pos: 0,
            meters: Meters::default(),
            shown: [view::FLOOR_DB; view::LEN],
        };
        s.prepare(48000.0, 0);
        s
    }
}

impl Shaper {
    fn snap(&mut self) {
        self.s_thr = db_to_lin(self.threshold_db);
        self.s_drive = db_to_lin(self.drive_db);
        self.s_mix = self.mix;
    }

    #[inline]
    fn smooth(cur: &mut f64, target: f64, k: f64) {
        let d = target - *cur;
        if d.abs() < 1e-9 * (1.0 + target.abs()) {
            *cur = target;
        } else {
            *cur = target - d * k;
        }
    }

    /// Runs the curve at 4x for one base-rate sample of channel `c`; returns the wet sample,
    /// delayed by `OS_LATENCY`.
    #[inline]
    fn os_sample(&mut self, c: usize, x: f64, t: f64, inv_t: f64) -> f64 {
        let kind = self.curve;
        let h = &self.h;
        let ch = &mut self.os[c];
        ch.xpos = (ch.xpos + 1) & (XH_LEN - 1);
        ch.xh[ch.xpos] = x;
        let mut out = 0.0;
        for p in 0..4 {
            // interpolate phase p: y[4n+p] = 4 * sum_j h[4j+p] x[n-j]
            let mut acc = 0.0;
            let mut j = 0;
            while j < PHASE_LEN {
                let i = 4 * j + p;
                if i >= OS_TAPS {
                    break;
                }
                acc += h[i] * ch.xh[(ch.xpos.wrapping_sub(j)) & (XH_LEN - 1)];
                j += 1;
            }
            let y = curve(kind, 4.0 * acc, t, inv_t);
            ch.zpos = (ch.zpos + 1) & (ZH_LEN - 1);
            ch.zh[ch.zpos] = y;
            if p == 0 {
                // decimate at the phase-0 instant so the total delay is a whole number of samples
                let mut d = 0.0;
                for (i, hv) in h.iter().enumerate() {
                    d += hv * ch.zh[(ch.zpos.wrapping_sub(i)) & (ZH_LEN - 1)];
                }
                out = d;
            }
        }
        out
    }
}

impl Module for Shaper {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate > 1000.0 { sample_rate as f64 } else { 48000.0 };
        self.sr = sr;
        self.k_smooth = coef(SMOOTH_S, sr);
        self.up_att = coef(UP_ATTACK_S, sr);
        self.up_rel = coef(UP_RELEASE_S, sr);
        self.tr_fa = coef(TR_FAST_ATTACK_S, sr);
        self.tr_fr = coef(TR_FAST_RELEASE_S, sr);
        self.tr_sa = coef(TR_SLOW_ATTACK_S, sr);
        self.tr_sr = coef(TR_SLOW_RELEASE_S, sr);
        self.reset();
    }

    fn reset(&mut self) {
        self.up_env = 0.0;
        self.tr_fast = 0.0;
        self.tr_slow = 0.0;
        self.os[0].clear();
        self.os[1].clear();
        self.dry = [[0.0; DRY_LEN]; 2];
        self.dry_pos = 0;
        self.meters = Meters::default();
        self.shown = [view::FLOOR_DB; view::LEN];
        self.snap();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(d) = PARAMS.get(index) else { return };
        if !value.is_finite() {
            return;
        }
        let v = value.clamp(d.min, d.max) as f64;
        match index {
            P_CURVE => self.curve = (v.round() as usize).min(CURVE_LABELS.len() - 1),
            P_THRESHOLD => self.threshold_db = v,
            P_DRIVE => self.drive_db = v,
            P_MIX => self.mix = v / 100.0,
            P_OVERSAMPLE => {
                let on = v >= 0.5;
                if on != self.oversample {
                    self.oversample = on;
                    self.os[0].clear();
                    self.os[1].clear();
                    self.dry = [[0.0; DRY_LEN]; 2];
                }
            }
            P_UPWARD => self.upward_db = v,
            P_UPWARD_THRESHOLD => self.upward_thr_db = v,
            P_TRANSIENT => self.transient = v / 100.0,
            _ => {}
        }
    }

    fn latency(&self) -> usize {
        if self.oversample {
            OS_LATENCY
        } else {
            0
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let thr_target = db_to_lin(self.threshold_db);
        let drive_target = db_to_lin(self.drive_db);
        let mix_target = self.mix;
        let k = self.k_smooth;
        let up_on = self.upward_db > 0.0;
        let tr_on = self.transient > 0.0;
        let mut pk_in: f64 = 0.0;
        let mut pk_out: f64 = 0.0;
        // Read-out only: nothing below depends on these.
        let mut pk_raw: f64 = 0.0;
        let mut up_max: f64 = 0.0;
        let mut tr_max: f64 = 0.0;

        for i in 0..n {
            let mut l = left[i] as f64;
            let mut r = right[i] as f64;
            pk_raw = pk_raw.max(l.abs()).max(r.abs());

            // stage 7a: upward compression (linked, mean-square detector)
            if up_on {
                let p = 0.5 * (l * l + r * r);
                let c = if p > self.up_env { self.up_att } else { self.up_rel };
                self.up_env = p + c * (self.up_env - p);
                let lvl_db = 10.0 * (self.up_env + 1e-20).log10();
                let boost = self.upward_db * ((self.upward_thr_db - lvl_db) / UP_RANGE_DB).clamp(0.0, 1.0);
                up_max = up_max.max(boost);
                let g = db_to_lin(boost);
                l *= g;
                r *= g;
            } else {
                self.up_env = 0.0;
            }

            // stage 7b: transient shaping (fast minus slow peak envelope)
            if tr_on {
                let a = l.abs().max(r.abs());
                let cf = if a > self.tr_fast { self.tr_fa } else { self.tr_fr };
                self.tr_fast = a + cf * (self.tr_fast - a);
                let cs = if a > self.tr_slow { self.tr_sa } else { self.tr_sr };
                self.tr_slow = a + cs * (self.tr_slow - a);
                let diff_db = 20.0 * ((self.tr_fast + 1e-6) / (self.tr_slow + 1e-6)).log10();
                let boost = self.transient * (TR_SLOPE * diff_db).clamp(0.0, TR_MAX_DB);
                tr_max = tr_max.max(boost);
                let g = db_to_lin(boost);
                l *= g;
                r *= g;
            } else {
                self.tr_fast = 0.0;
                self.tr_slow = 0.0;
            }

            // stage 8: drive and curve
            Self::smooth(&mut self.s_thr, thr_target, k);
            Self::smooth(&mut self.s_drive, drive_target, k);
            Self::smooth(&mut self.s_mix, mix_target, k);
            let t = self.s_thr;
            let inv_t = 1.0 / t;
            let dg = self.s_drive;
            let m = self.s_mix;
            let xl = l * dg;
            let xr = r * dg;
            pk_in = pk_in.max(xl.abs()).max(xr.abs());

            let (dl, dr, wl, wr);
            if self.oversample {
                self.dry_pos = (self.dry_pos + 1) & (DRY_LEN - 1);
                self.dry[0][self.dry_pos] = xl;
                self.dry[1][self.dry_pos] = xr;
                let rp = self.dry_pos.wrapping_sub(OS_LATENCY) & (DRY_LEN - 1);
                dl = self.dry[0][rp];
                dr = self.dry[1][rp];
                if self.curve == CURVE_OFF {
                    wl = dl;
                    wr = dr;
                } else {
                    wl = self.os_sample(0, xl, t, inv_t);
                    wr = self.os_sample(1, xr, t, inv_t);
                }
            } else {
                dl = xl;
                dr = xr;
                wl = curve(self.curve, xl, t, inv_t);
                wr = curve(self.curve, xr, t, inv_t);
            }

            let (ol, or) = if self.curve == CURVE_OFF || m == 0.0 {
                (dl, dr)
            } else if m == 1.0 {
                (wl, wr)
            } else {
                (dl + m * (wl - dl), dr + m * (wr - dr))
            };
            pk_out = pk_out.max(ol.abs()).max(or.abs());
            left[i] = ol as f32;
            right[i] = or as f32;
        }

        let red = if pk_in > 1e-9 && pk_out > 1e-12 { 20.0 * (pk_in / pk_out).log10() } else { 0.0 };
        self.meters.reduction_db = red.max(0.0) as f32;
        let level = |x: f64| if x > 1e-6 { (20.0 * x.log10()) as f32 } else { view::FLOOR_DB };
        self.shown = [up_max as f32, tr_max as f32, level(pk_in), level(pk_out), level(pk_raw)];
    }

    fn meters(&self) -> Meters {
        self.meters
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&self.shown[..n]);
        n
    }
}
