//! `limiter-clear`: the transparent look-ahead peak limiter of OpenLimiter.
//!
//! Built from measured behaviour only (clean room). Section numbers in comments refer to the
//! measurement side's documents `SPEC_LIMITERS.md` and `HANDOFF_NEXT_TOOL.md`.
//!
//! Signal path, per sample, stereo linked:
//!
//! ```text
//! in -> input gain -> delay (148 samples at 48 kHz) -> x gain -> lift / peak guard -> clip at full scale -> trim
//!            |                                           ^             ^
//!            +-> peak (or true peak) -> sliding max -> static curve -> release -> attack -> slow hold
//! ```
//!
//! * The detector looks 20 samples (at 48 kHz) ahead of the delayed audio. Its sliding maximum
//!   makes the reduction exact on steady tones (HANDOFF 3.2 "Ceiling").
//! * Release is a hold followed by two one-pole stages working on the reduction in dB. Their
//!   times come from a table fitted to the measured recovery times (SPEC_LIMITERS.md 59, 64).
//! * Attack is two parallel one-pole stages in dB (SPEC_LIMITERS.md 54.2).
//! * The slow hold is a second release stage: three cascaded one-pole stages (SPEC_LIMITERS.md 71.5).
//! * Lift and the peak guard are one gain law on the limited signal, driven by a smooth
//!   envelope, and an instantaneous fold above the threshold (SPEC_LIMITERS.md 54.2, 56.1, 56.2).

#![forbid(unsafe_code)]

mod window;

use olm_api::{Kind, Meters, Module, ParamDesc};
use window::{BoxMean, SlidingMax};

pub const P_INPUT_GAIN: usize = 0;
pub const P_THRESHOLD: usize = 1;
pub const P_ATTACK: usize = 2;
pub const P_RELEASE: usize = 3;
pub const P_KNEE: usize = 4;
pub const P_LIFT: usize = 5;
pub const P_OUTPUT_TRIM: usize = 6;
pub const P_SLOW_HOLD: usize = 7;
pub const P_PEAK_GUARD: usize = 8;
pub const P_TRUE_PEAK: usize = 9;
const PARAM_COUNT: usize = 10;

// The displayed ends are 0.052 and 1.04 ms (attack) and 0.052 and 1038 ms (release). A
// geometric law from 0.0519 reproduces every displayed value in between (0.110, 0.232, 0.491;
// 0.179, 0.617, 2.13, 7.34, 25.3, 87.3, 301), which a law from 0.052 does not quite do.
const ATTACK_MIN_MS: f32 = 0.0519;
const ATTACK_MAX_MS: f32 = 1.038;
const RELEASE_MIN_MS: f32 = 0.0519;
const RELEASE_MAX_MS: f32 = 1038.0;

static PARAMS: [ParamDesc; PARAM_COUNT] = [
    ParamDesc::new("inputGain", "Input Gain", -18.0, 18.0, 0.0, Kind::Db),
    ParamDesc::new("threshold", "Threshold", -12.0, 0.0, 0.0, Kind::Db),
    ParamDesc::new("attack", "Attack", ATTACK_MIN_MS, ATTACK_MAX_MS, ATTACK_MIN_MS, Kind::Ms).log(),
    ParamDesc::new("release", "Release", RELEASE_MIN_MS, RELEASE_MAX_MS, 7.34, Kind::Ms).log(),
    ParamDesc::new("knee", "Knee", 0.0, 10.0, 0.0, Kind::Db),
    ParamDesc::new("lift", "Lift", 0.0, 125.0, 0.0, Kind::Percent),
    ParamDesc::new("outputTrim", "Output Trim", -18.0, 0.0, 0.0, Kind::Db),
    ParamDesc::toggle("slowHold", "Slow Hold", true),
    ParamDesc::toggle("peakGuard", "Peak Guard", true),
    ParamDesc::toggle("truePeak", "True Peak Hold", false),
];

/// Maps a control position 0..1 to the plain value, with the measured control laws
/// (HANDOFF 3.1; SPEC_LIMITERS.md 58, 59). The module itself only takes plain values; an
/// editor that wants the measured tapers uses this.
pub fn plain_from_position(index: usize, position: f32) -> f32 {
    let v = position.clamp(0.0, 1.0) as f64;
    let out = match index {
        P_INPUT_GAIN => -18.0 + 36.0 * v,
        P_THRESHOLD => -12.0 + 12.0 * v,
        P_ATTACK => ATTACK_MIN_MS as f64 * 20f64.powf(v),
        P_RELEASE => RELEASE_MIN_MS as f64 * 20000f64.powf(v),
        P_KNEE => 10.0 * v,
        P_LIFT => 125.0 * v,
        P_OUTPUT_TRIM => -18.0 * (1.0 - v) * (1.0 - v),
        P_SLOW_HOLD | P_PEAK_GUARD | P_TRUE_PEAK => {
            if v >= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    };
    out as f32
}

// ---------------------------------------------------------------------------------------------
// Fitted tables. All times in milliseconds.

/// HANDOFF 3.1: reported latency, samples at 48 kHz.
const LATENCY_AT_48K: f64 = 148.0;
/// How far the detector runs ahead of the delayed audio, samples at 48 kHz (SPEC_LIMITERS.md 54.2).
const DET_LEAD_AT_48K: f64 = 20.0;
/// Length of the detector's sliding maximum, samples at 48 kHz.
const DET_WINDOW_AT_48K: f64 = 44.0;

/// Release settings at which recovery was measured (SPEC_LIMITERS.md 59).
const REL_SET_MS: [f64; 9] = [0.052, 0.179, 0.617, 2.13, 7.34, 25.3, 87.3, 301.0, 1038.0];
/// Hold after the detector window has passed, before the release stages start.
const REL_HOLD_MS: [f64; 9] = [0.0, 0.43, 1.53, 2.73, 5.44, 4.28, 3.88, 4.08, 4.08];
/// First release stage (the long one; equals the setting from 25 ms upward).
const REL_STAGE1_MS: [f64; 9] = [0.60, 1.24, 2.08, 3.58, 7.04, 25.3, 87.3, 301.0, 1038.0];
/// Second release stage (rounds the start of the recovery).
const REL_STAGE2_MS: [f64; 9] = [0.054, 0.25, 0.52, 1.78, 3.31, 4.2, 4.2, 4.2, 4.2];

/// Attack settings at which the overshoot was measured (SPEC_LIMITERS.md 54.2).
const ATT_SET_MS: [f64; 5] = [0.052, 0.110, 0.232, 0.491, 1.04];
/// Time constant of the faster attack stage; the slower stage is 2.5 times as long.
const ATT_STAGE_MS: [f64; 5] = [0.0796, 0.1974, 0.4072, 0.8397, 1.7054];
const ATT_SLOW_RATIO: f64 = 2.5;
const ATT_FAST_SHARE: f64 = 0.75;

/// Slow hold (SPEC_LIMITERS.md 71.4, 71.5): build-up and the three recovery stages.
const SLOW_CHARGE_MS: f64 = 25.6;
const SLOW_STAGE_MS: [f64; 3] = [1400.0, 525.0, 525.0];
/// The slow stage leaves this much reduction to the fast stage: 0.79 dB at release 7.34 ms,
/// 0.42 dB at 87.3 ms (SPEC_LIMITERS.md 71.2). Straight line in log time between and beyond.
const SLOW_MARGIN_AT_7MS: f64 = 0.79;
const SLOW_MARGIN_PER_LN: f64 = 0.1494;
const SLOW_MARGIN_MAX: f64 = 1.5;

/// True-peak hold sits this far above the threshold (SPEC_LIMITERS.md 71.3: 0.16 dB).
const TRUE_PEAK_ALLOWANCE: f32 = 0.981_748; // 10^(-0.16/20)

/// Peak envelope for the lift law: hold, then two running means.
const ENV_HOLD_MS: f64 = 8.0;
const ENV_MEAN1_MS: f64 = 16.0;
const ENV_MEAN2_MS: f64 = 76.0;
const ENV_ONE: f64 = (1u32 << 30) as f64;

/// Extra within-cycle shaping at 125 % lift (SPEC_LIMITERS.md 56.1, 56.2: only the distortion
/// changes, and only for envelopes in the upper part of the range while nothing is limited).
const OVER_LIFT_SHAPE: f64 = 1.0e-3;
const OVER_LIFT_ENV_FROM: f64 = 0.3;
const OVER_LIFT_ENV_SPAN: f64 = 0.4;
const OVER_LIFT_REDUCTION_DB: f64 = 0.5;

const FIR_HALF: usize = 8;
const FIR_TAPS: usize = 2 * FIR_HALF;
const DB_FLOOR: f64 = 1.0e-7;
const LN10_OVER_20: f64 = std::f64::consts::LN_10 / 20.0;

fn table(xs: &[f64], ys: &[f64], x: f64, log_y: bool) -> f64 {
    let n = xs.len().min(ys.len());
    if n == 0 {
        return 0.0;
    }
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[n - 1] {
        return ys[n - 1];
    }
    for i in 1..n {
        if x <= xs[i] {
            let f = (x.ln() - xs[i - 1].ln()) / (xs[i].ln() - xs[i - 1].ln());
            return if log_y && ys[i - 1] > 0.0 && ys[i] > 0.0 {
                (ys[i - 1].ln() + f * (ys[i].ln() - ys[i - 1].ln())).exp()
            } else {
                ys[i - 1] + f * (ys[i] - ys[i - 1])
            };
        }
    }
    ys[n - 1]
}

fn one_pole(ms: f64, sr: f64) -> f64 {
    if ms <= 0.0 || sr <= 0.0 {
        1.0
    } else {
        1.0 - (-1000.0 / (ms * sr)).exp()
    }
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..40 {
        term *= q / (k * k) as f64;
        sum += term;
    }
    sum
}

/// Three quarter-sample interpolators (Kaiser-windowed sinc, 16 taps each) for the true-peak
/// detector. Row `r` gives the point `(r + 1) / 4` of the way from sample 7 to sample 8 of
/// the 16 it is given.
fn true_peak_taps() -> [[f32; FIR_TAPS]; 3] {
    let beta = 8.0;
    let mut out = [[0.0f32; FIR_TAPS]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        let frac = (r + 1) as f64 / 4.0;
        let mut h = [0.0f64; FIR_TAPS];
        let mut sum = 0.0;
        for (j, hj) in h.iter_mut().enumerate() {
            let t = j as f64 - (FIR_HALF as f64 - 1.0) - frac;
            let sinc = if t.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t) };
            let w = 1.0 - (t / FIR_HALF as f64).powi(2);
            let win = if w > 0.0 { bessel_i0(beta * w.sqrt()) / bessel_i0(beta) } else { 0.0 };
            *hj = sinc * win;
            sum += *hj;
        }
        for (j, hj) in h.iter().enumerate() {
            row[j] = (hj / sum) as f32;
        }
    }
    out
}

/// The transparent peak limiter. See the crate documentation for the signal path.
pub struct ClearLimiter {
    values: [f32; PARAM_COUNT],
    sr: f64,
    prepared: bool,
    fresh: bool,

    // Derived from parameters.
    gain_in_target: f64,
    trim_target: f64,
    gain_smooth: f64,
    thr_db: f64,
    thr_lin: f64,
    knee_db: f64,
    knee_floor_lin: f32,
    lift: f64,
    over_lift: f64,
    guard: bool,
    slow_on: bool,
    true_peak: bool,
    att_k: [f64; 2],
    rel_hold: u32,
    rel_k: [f64; 2],
    slow_charge_k: f64,
    slow_k: [f64; 3],
    slow_margin: f64,

    // Sizes for this sample rate.
    latency: usize,
    det_delay: usize,

    // State.
    ring_l: Vec<f32>,
    ring_r: Vec<f32>,
    ring_mask: usize,
    write: usize,
    gain_in: f64,
    trim: f64,
    fir: [[f32; FIR_TAPS]; 3],
    det_max: SlidingMax<f32>,
    rel_q: [f64; 2],
    rel_count: u32,
    att_a: [f64; 2],
    slow_in: f64,
    slow_s: [f64; 3],
    env_hold: SlidingMax<u32>,
    env_mean1: BoxMean,
    env_mean2: BoxMean,
    meter_db: f32,
}

impl Default for ClearLimiter {
    fn default() -> Self {
        let mut values = [0.0f32; PARAM_COUNT];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        let mut s = Self {
            values,
            sr: 48_000.0,
            prepared: false,
            fresh: true,
            gain_in_target: 1.0,
            trim_target: 1.0,
            gain_smooth: 1.0,
            thr_db: 0.0,
            thr_lin: 1.0,
            knee_db: 0.0,
            knee_floor_lin: 1.0,
            lift: 0.0,
            over_lift: 0.0,
            guard: true,
            slow_on: true,
            true_peak: false,
            att_k: [1.0; 2],
            rel_hold: 0,
            rel_k: [1.0; 2],
            slow_charge_k: 1.0,
            slow_k: [1.0; 3],
            slow_margin: SLOW_MARGIN_AT_7MS,
            latency: 148,
            det_delay: 128,
            ring_l: Vec::new(),
            ring_r: Vec::new(),
            ring_mask: 0,
            write: 0,
            gain_in: 1.0,
            trim: 1.0,
            fir: true_peak_taps(),
            det_max: SlidingMax::empty(0.0),
            rel_q: [0.0; 2],
            rel_count: 0,
            att_a: [0.0; 2],
            slow_in: 0.0,
            slow_s: [0.0; 3],
            env_hold: SlidingMax::empty(0),
            env_mean1: BoxMean::empty(),
            env_mean2: BoxMean::empty(),
            meter_db: 0.0,
        };
        s.update();
        s
    }
}

impl ClearLimiter {
    /// Recomputes everything derived from the parameter values. No allocation.
    fn update(&mut self) {
        let v = self.values;
        let sr = self.sr;
        self.gain_in_target = (v[P_INPUT_GAIN] as f64 * LN10_OVER_20).exp();
        self.trim_target = (v[P_OUTPUT_TRIM] as f64 * LN10_OVER_20).exp();
        self.gain_smooth = one_pole(2.0, sr);
        self.thr_db = v[P_THRESHOLD] as f64;
        self.thr_lin = (self.thr_db * LN10_OVER_20).exp();
        self.knee_db = v[P_KNEE] as f64;
        // Below this level the static curve is unity and no logarithm is needed.
        self.knee_floor_lin = (((self.thr_db - self.knee_db) * LN10_OVER_20).exp() * (1.0 - 1e-6)) as f32;
        let lift = v[P_LIFT] as f64 / 100.0;
        self.lift = lift.clamp(0.0, 1.0);
        self.over_lift = ((lift - 1.0) / 0.25).clamp(0.0, 1.0) * OVER_LIFT_SHAPE;
        self.slow_on = v[P_SLOW_HOLD] >= 0.5;
        self.guard = v[P_PEAK_GUARD] >= 0.5;
        self.true_peak = v[P_TRUE_PEAK] >= 0.5;

        let att = table(&ATT_SET_MS, &ATT_STAGE_MS, v[P_ATTACK] as f64, true);
        self.att_k = [one_pole(att, sr), one_pole(ATT_SLOW_RATIO * att, sr)];

        let rel = v[P_RELEASE] as f64;
        let hold_ms = table(&REL_SET_MS, &REL_HOLD_MS, rel, false);
        self.rel_hold = (hold_ms * sr / 1000.0).round().max(0.0) as u32;
        self.rel_k = [
            one_pole(table(&REL_SET_MS, &REL_STAGE1_MS, rel, true), sr),
            one_pole(table(&REL_SET_MS, &REL_STAGE2_MS, rel, true), sr),
        ];

        self.slow_charge_k = one_pole(SLOW_CHARGE_MS, sr);
        for (k, ms) in self.slow_k.iter_mut().zip(SLOW_STAGE_MS.iter()) {
            *k = one_pole(*ms, sr);
        }
        self.slow_margin =
            (SLOW_MARGIN_AT_7MS - SLOW_MARGIN_PER_LN * (rel / 7.34).ln()).clamp(0.0, SLOW_MARGIN_MAX);

        if self.fresh {
            self.gain_in = self.gain_in_target;
            self.trim = self.trim_target;
        }
    }

    /// Reduction in dB (positive) for a peak level, hard or soft knee (SPEC_LIMITERS.md 71.1).
    #[inline]
    fn static_reduction(&self, peak: f32) -> f64 {
        if peak <= self.knee_floor_lin {
            return 0.0;
        }
        let over = 20.0 * (peak as f64).log10() - self.thr_db;
        let k = self.knee_db;
        if k < 1e-6 {
            over.max(0.0)
        } else if over <= -k {
            0.0
        } else if over >= k {
            over
        } else {
            (over + k) * (over + k) / (4.0 * k)
        }
    }

    /// Largest inter-sample magnitude between ring positions `c` and `c + 1`.
    #[inline]
    fn inter_sample_peak(&self, ring: &[f32], c: usize) -> f32 {
        let mask = self.ring_mask;
        let start = c.wrapping_sub(FIR_HALF - 1);
        let mut acc = [0.0f32; 3];
        for j in 0..FIR_TAPS {
            let s = ring[start.wrapping_add(j) & mask];
            acc[0] += s * self.fir[0][j];
            acc[1] += s * self.fir[1][j];
            acc[2] += s * self.fir[2][j];
        }
        acc[0].abs().max(acc[1].abs()).max(acc[2].abs())
    }

    /// Output for one limited sample `v`, given the smooth envelope `env` (peak of the limited
    /// signal over the threshold, 0..1) and the amount of within-cycle shaping `over`.
    /// SPEC_LIMITERS.md 54.2, 56.1, 56.2.
    #[inline]
    fn shape(&self, v: f64, env: f64, over: f64) -> f64 {
        let a = v.abs() / self.thr_lin;
        let e = self.lift;
        if a <= 1.0 {
            let env = env.max(a);
            let mut g = if self.guard {
                (1.0 - e) * (1.0 + 0.5 * env * (1.0 - env)) + e * (2.0 - env)
            } else {
                1.0 + e * (1.0 - env)
            };
            g += over * (env - a);
            v * g
        } else {
            // A peak the attack let through: fold it back instead of passing it.
            let d = (a - 1.0).min(0.5);
            let fold = 1.0 - d * d;
            let y = if self.guard { fold } else { (1.0 - e) * a + e * fold };
            (y * self.thr_lin).copysign(v)
        }
    }
}

impl Module for ClearLimiter {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 1000.0 { sample_rate as f64 } else { 48_000.0 };
        self.sr = sr;
        let scale = sr / 48_000.0;
        // HANDOFF 3.1: 148 samples at 48 kHz. Kept constant in time at other rates.
        let latency = ((LATENCY_AT_48K * scale).round() as usize).max(FIR_HALF + 9);
        let lead = ((DET_LEAD_AT_48K * scale).round() as usize).clamp(1, latency - FIR_HALF);
        let window = ((DET_WINDOW_AT_48K * scale).round() as usize).max(lead + 1);
        self.latency = latency;
        self.det_delay = latency - lead;
        let ring = (latency + FIR_TAPS + 2).next_power_of_two();
        self.ring_l = vec![0.0; ring];
        self.ring_r = vec![0.0; ring];
        self.ring_mask = ring - 1;
        self.det_max = SlidingMax::with_window(window, 0.0);
        self.env_hold = SlidingMax::with_window((ENV_HOLD_MS * sr / 1000.0).round() as usize, 0);
        self.env_mean1 = BoxMean::with_len((ENV_MEAN1_MS * sr / 1000.0).round() as usize);
        self.env_mean2 = BoxMean::with_len((ENV_MEAN2_MS * sr / 1000.0).round() as usize);
        self.prepared = true;
        self.reset();
        self.update();
    }

    fn reset(&mut self) {
        for v in self.ring_l.iter_mut() {
            *v = 0.0;
        }
        for v in self.ring_r.iter_mut() {
            *v = 0.0;
        }
        self.write = 0;
        self.det_max.clear();
        self.rel_q = [0.0; 2];
        self.rel_count = 0;
        self.att_a = [0.0; 2];
        self.slow_in = 0.0;
        self.slow_s = [0.0; 3];
        self.env_hold.clear();
        self.env_mean1.clear();
        self.env_mean2.clear();
        self.meter_db = 0.0;
        self.fresh = true;
        self.gain_in = self.gain_in_target;
        self.trim = self.trim_target;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index >= PARAM_COUNT || !value.is_finite() {
            return;
        }
        let p = &PARAMS[index];
        self.values[index] = value.clamp(p.min, p.max);
        self.update();
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        if !self.prepared || n == 0 {
            return;
        }
        self.fresh = false;
        let mask = self.ring_mask;
        let thr = self.thr_lin;
        let mut deepest = 0.0f64;

        for i in 0..n {
            // Input gain, smoothed against zipper noise.
            self.gain_in += (self.gain_in_target - self.gain_in) * self.gain_smooth;
            self.trim += (self.trim_target - self.trim) * self.gain_smooth;
            let w = self.write;
            self.ring_l[w & mask] = (left[i] as f64 * self.gain_in) as f32;
            self.ring_r[w & mask] = (right[i] as f64 * self.gain_in) as f32;

            // Detector: peak of both channels, `lead` samples ahead of the delayed audio.
            let c = w.wrapping_sub(self.det_delay);
            let mut peak = self.ring_l[c & mask].abs().max(self.ring_r[c & mask].abs());
            if self.true_peak {
                let between = self
                    .inter_sample_peak(&self.ring_l, c.wrapping_sub(1))
                    .max(self.inter_sample_peak(&self.ring_r, c.wrapping_sub(1)));
                peak = peak.max(between * TRUE_PEAK_ALLOWANCE);
            }
            if !peak.is_finite() {
                peak = 0.0;
            }
            let held = self.det_max.push(peak);
            let need = self.static_reduction(held);

            // Release: hold, then two one-pole stages on the reduction in dB.
            if need >= self.rel_q[0] {
                self.rel_q[0] = need;
                self.rel_count = self.rel_hold;
            } else if self.rel_count > 0 {
                self.rel_count -= 1;
            } else {
                self.rel_q[0] += (need - self.rel_q[0]) * self.rel_k[0];
                if self.rel_q[0] < DB_FLOOR {
                    self.rel_q[0] = 0.0;
                }
            }
            if self.rel_q[0] >= self.rel_q[1] {
                self.rel_q[1] = self.rel_q[0];
            } else {
                self.rel_q[1] += (self.rel_q[0] - self.rel_q[1]) * self.rel_k[1];
                if self.rel_q[1] < DB_FLOOR {
                    self.rel_q[1] = 0.0;
                }
            }
            let q = self.rel_q[1];

            // Attack: two parallel one-pole stages, instant on the way back.
            for (a, k) in self.att_a.iter_mut().zip(self.att_k.iter()) {
                if q > *a {
                    *a += (q - *a) * *k;
                } else {
                    *a = q;
                }
            }
            let fast = ATT_FAST_SHARE * self.att_a[0] + (1.0 - ATT_FAST_SHARE) * self.att_a[1];

            // Slow hold: a second, slow release that keeps the reduction less a margin.
            let mut reduction = fast;
            if self.slow_on {
                if fast > self.slow_in {
                    self.slow_in += (fast - self.slow_in) * self.slow_charge_k;
                } else {
                    self.slow_in = fast;
                }
                let mut feed = self.slow_in;
                for (s, k) in self.slow_s.iter_mut().zip(self.slow_k.iter()) {
                    if feed >= *s {
                        *s = feed;
                    } else {
                        *s += (feed - *s) * *k;
                        if *s < DB_FLOOR {
                            *s = 0.0;
                        }
                    }
                    feed = *s;
                }
                reduction = reduction.max(feed - self.slow_margin);
            } else {
                self.slow_in = 0.0;
                self.slow_s = [0.0; 3];
            }
            if reduction > deepest {
                deepest = reduction;
            }
            let gain = if reduction > 0.0 { (-reduction * LN10_OVER_20).exp() } else { 1.0 };

            // Envelope of the limited signal for the lift law: instant up, smooth and slow down.
            let now = (held as f64 * gain / thr).min(1.0);
            let quant = (now * ENV_ONE) as u32;
            let smooth = self.env_mean2.push(self.env_mean1.push(self.env_hold.push(quant)));
            let env = now.max(smooth as f64 / ENV_ONE);

            let r = w.wrapping_sub(self.latency);
            let vl = self.ring_l[r & mask] as f64 * gain;
            let vr = self.ring_r[r & mask] as f64 * gain;
            let over = if self.over_lift > 0.0 {
                let t = ((env - OVER_LIFT_ENV_FROM) / OVER_LIFT_ENV_SPAN).clamp(0.0, 1.0);
                self.over_lift * t * t * (3.0 - 2.0 * t) * (1.0 - (reduction / OVER_LIFT_REDUCTION_DB).min(1.0))
            } else {
                0.0
            };
            let yl = self.shape(vl, env, over).clamp(-1.0, 1.0) * self.trim;
            let yr = self.shape(vr, env, over).clamp(-1.0, 1.0) * self.trim;
            left[i] = yl as f32;
            right[i] = yr as f32;
            self.write = w.wrapping_add(1);
        }

        // Meter: instant rise, linear fall of 20 dB per second (SPEC_LIMITERS.md 62, 62.1).
        let fall = 20.0 * n as f32 / self.sr as f32;
        self.meter_db = (self.meter_db - fall).max(deepest as f32).max(0.0);
    }

    fn meters(&self) -> Meters {
        Meters { reduction_db: self.meter_db, ..Meters::default() }
    }
}
