//! Look-ahead limiter engine.
//!
//! Written from two descriptions of behaviour only: the first clean-room package (parameters and
//! presets) and a published description of a comparable limiter (what the
//! controls do). No vendor code was read.
//!
//! Signal path per channel:
//! input gain -> phase invert -> soft clip -> look-ahead peak limiter (optional 5x
//! inter-sample peak detection) -> output gain -> dither -> alignment delay.
//!
//! What the manual fixes:
//! * The look-ahead delay is about 0.8 ms: 37, 39, 64 and 69 samples at 44.1, 48, 88.2 and 96 kHz.
//! * The top of the Threshold range is Off.
//! * Soft Clip works on the level after the input gain, per channel, and its top value is Off.
//! * With Auto Release on, the manual Release time is not used. The limiter recovers quickly
//!   from an occasional peak and slowly under sustained limiting.
//! * Gains at multiples of 6 dB are bit shifts, so below the threshold the path is bit-transparent.
//! * The meters always read inter-sample peaks, whatever Upsample says.
//!
//! The gain envelope is computed on a look-ahead window, so no sample above the ceiling gets
//! through. With Upsample the window holds inter-sample peaks, so the reconstructed waveform
//! stays under the ceiling too.

use crate::shared::Shared;
use std::sync::Arc;

const RING: usize = 8192; // power of two, larger than look-ahead + the interpolator
const RMASK: usize = RING - 1;

/// The inter-sample interpolator: four points between each pair of samples (5x), each from
/// `TAPS` samples. It needs `ISP_DELAY` samples after the one it measures.
pub(crate) const TAPS: usize = 8;
pub(crate) const PHASES: usize = 4;
const ISP_DELAY: usize = 4;

/// Auto Release time constants at the Normal step, in milliseconds.
const AUTO_FAST_MS: f32 = 30.0;
const AUTO_SLOW_MS: f32 = 700.0;
/// How fast the slow stage sinks towards sustained reduction.
const AUTO_SINK_MS: f32 = 80.0;

/// Plain parameter values, already clamped to their ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub gain_l_db: f32,
    pub gain_r_db: f32,
    pub threshold_db: f32,  // above 0 = off
    pub soft_clip_db: f32,  // 24 = off
    pub release_step: i32,  // -3..3, only with auto release; negative = slower
    pub release_ms: f32,    // only without auto release
    pub auto_release: bool,
    pub output_db: f32,     // <= -120 = silence
    pub delay_l_ms: f32,
    pub delay_r_ms: f32,
    pub fine_l: u32,
    pub fine_r: u32,
    pub phase_l: bool,
    pub phase_r: bool,
    pub upsample: bool,
    pub link: bool,
    pub dither_bits: i32,   // 8..24 dither; 25 and above = off
    pub no_latency: bool,
}

#[derive(Clone)]
struct Chan {
    x: Vec<f32>,        // conditioned input history
    dq_idx: Vec<i64>,   // monotonic max deque over the look-ahead window
    dq_val: Vec<f32>,
    dq_head: u64,
    dq_tail: u64,
    delay: Vec<f32>,    // alignment delay ring
    recent_out: [f32; TAPS],
    env: f32,
    slow: f32,
    rng: u32,
}

pub struct Limiter {
    sr: f32,
    s: Settings,
    chans: [Chan; 2],
    kernel: [[f32; TAPS]; PHASES],
    delay_mask: usize,
    counter: u64,
    lookahead: usize,
    peak_delay: usize,
    delay_samples: [usize; 2],
    limiting: bool,
    ceil: f32,
    soft_clip: Option<f32>,
    out_lin: f32,
    in_gain: [f32; 2],
    sign: [f32; 2],
    release: Release,
    /// Meters and history for the editor. `None` in tests and headless use.
    shared: Option<Arc<Shared>>,
    /// The lowest gain of the last block (1 = no reduction).
    block_gain: f32,
}

/// Release coefficients per sample.
#[derive(Clone, Copy)]
struct Release {
    auto: bool,
    manual: f32,
    fast: f32,
    slow: f32,
    sink: f32,
}

/// One step of the gain envelope towards the gain `req` the window asks for. Takes and
/// returns `(env, slow)`. `env` never exceeds `req`, which is what keeps the ceiling.
fn step_env(r: &Release, env: f32, slow: f32, req: f32) -> (f32, f32) {
    if !r.auto {
        return (if req < env { req } else { req + (env - req) * r.manual }, slow);
    }
    // Two stages. The slow one sinks only under sustained reduction and recovers slowly;
    // the fast one recovers quickly, but only as far as the slow one allows. An isolated
    // peak barely moves the slow stage, so it is forgotten at once.
    let slow = req + (slow - req) * if req < slow { r.sink } else { r.slow };
    let target = req.min(slow);
    (if target < env { target } else { target + (env - target) * r.fast }, slow)
}


/// Decibels to a linear factor. A multiple of 6 dB is a bit shift (an exact power of two),
/// which keeps the path bit-transparent at those settings.
pub fn db_to_lin(db: f32) -> f32 {
    if db <= -120.0 {
        return 0.0;
    }
    let shifts = db / 6.0;
    if (shifts - shifts.round()).abs() < 1e-4 {
        2f32.powi(shifts.round() as i32)
    } else {
        10f32.powf(db / 20.0)
    }
}

/// The look-ahead delay in samples: the manual's figures, and 0.8 ms at other rates.
pub fn lookahead_samples(sample_rate: f32) -> usize {
    match sample_rate.round() as u32 {
        44_100 => 37,
        48_000 => 39,
        88_200 => 64,
        96_000 => 69,
        _ => (0.0008 * sample_rate).round().max(ISP_DELAY as f32 + 1.0) as usize,
    }
}

fn coef(ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * sample_rate)).exp()
}

fn bessel_i0(x: f32) -> f32 {
    let (mut sum, mut term) = (1.0f32, 1.0f32);
    for n in 1..20 {
        term *= (x / (2.0 * n as f32)).powi(2);
        sum += term;
    }
    sum
}

/// Kaiser-windowed sinc, one row per point between two samples.
pub(crate) fn make_kernel() -> [[f32; TAPS]; PHASES] {
    let mut k = [[0.0f32; TAPS]; PHASES];
    let beta = 6.0f32;
    for (p, row) in k.iter_mut().enumerate() {
        let frac = (p + 1) as f32 / (PHASES + 1) as f32;
        for (j, tap) in row.iter_mut().enumerate() {
            let t = j as f32 - (TAPS / 2 - 1) as f32 - frac;
            let sinc = if t.abs() < 1e-6 { 1.0 } else { (std::f32::consts::PI * t).sin() / (std::f32::consts::PI * t) };
            let span = (t / (TAPS / 2) as f32).clamp(-1.0, 1.0);
            *tap = sinc * bessel_i0(beta * (1.0 - span * span).sqrt()) / bessel_i0(beta);
        }
        let sum: f32 = row.iter().sum();
        row.iter_mut().for_each(|t| *t /= sum);
    }
    k
}

/// The peak around the sample `ISP_DELAY` before the newest of `window`: the sample itself
/// and the four points between it and the next. `window` is oldest first.
pub(crate) fn true_peak(kernel: &[[f32; TAPS]; PHASES], window: &[f32; TAPS]) -> f32 {
    let mut pk = window[TAPS / 2 - 1].abs();
    for row in kernel {
        let mut v = 0.0f32;
        for (h, x) in row.iter().zip(window) {
            v += h * x;
        }
        pk = pk.max(v.abs());
    }
    pk
}

impl Limiter {
    /// Allocates everything. Call from `prepare`, never from the audio path.
    pub fn new(sample_rate: f32, s: Settings, shared: Option<Arc<Shared>>) -> Self {
        let max_delay = (0.3 * sample_rate).ceil() as usize + 128;
        let delay_len = max_delay.next_power_of_two();
        let chan = || Chan {
            x: vec![0.0; RING],
            dq_idx: vec![0; RING],
            dq_val: vec![0.0; RING],
            dq_head: 0,
            dq_tail: 0,
            delay: vec![0.0; delay_len],
            recent_out: [0.0; TAPS],
            env: 1.0,
            slow: 1.0,
            rng: 0x1234_5678,
        };
        let mut l = Self {
            sr: sample_rate,
            s,
            chans: [chan(), chan()],
            kernel: make_kernel(),
            delay_mask: delay_len - 1,
            counter: 0,
            lookahead: 0,
            peak_delay: 0,
            delay_samples: [0; 2],
            limiting: true,
            ceil: 1.0,
            soft_clip: None,
            out_lin: 1.0,
            in_gain: [1.0; 2],
            sign: [1.0; 2],
            release: Release { auto: true, manual: 0.0, fast: 0.0, slow: 0.0, sink: 0.0 },
            shared,
            block_gain: 1.0,
        };
        l.set_settings(s);
        l
    }

    /// Clears histories and state, keeps the settings. No allocation.
    pub fn reset(&mut self) {
        for c in &mut self.chans {
            c.x.fill(0.0);
            c.delay.fill(0.0);
            c.recent_out = [0.0; TAPS];
            c.dq_head = 0;
            c.dq_tail = 0;
            c.env = 1.0;
            c.slow = 1.0;
        }
        self.counter = 0;
    }

    /// Recomputes derived values. No allocation.
    pub fn set_settings(&mut self, s: Settings) {
        self.s = s;
        // The total look-ahead delay is the same with and without Upsample: the interpolator's
        // own delay comes out of it.
        self.peak_delay = if s.upsample && !s.no_latency { ISP_DELAY } else { 0 };
        self.lookahead = if s.no_latency { 0 } else { lookahead_samples(self.sr).saturating_sub(self.peak_delay) };
        self.limiting = s.threshold_db <= 0.05;
        self.ceil = 10f32.powf(s.threshold_db.min(0.0) / 20.0);
        self.soft_clip = (s.soft_clip_db < 24.0).then(|| 10f32.powf(s.soft_clip_db / 20.0));
        self.out_lin = db_to_lin(s.output_db);
        self.in_gain = [db_to_lin(s.gain_l_db), db_to_lin(s.gain_r_db)];
        self.sign = [if s.phase_l { -1.0 } else { 1.0 }, if s.phase_r { -1.0 } else { 1.0 }];
        let max_delay = self.delay_mask;
        self.delay_samples = [
            ((s.delay_l_ms * 0.001 * self.sr).round() as usize + s.fine_l as usize).min(max_delay),
            ((s.delay_r_ms * 0.001 * self.sr).round() as usize + s.fine_r as usize).min(max_delay),
        ];
        // Release. The Slow/Fast step scales both auto stages (negative = slower).
        let step = 2f32.powf(-0.5 * s.release_step as f32);
        self.release = Release {
            auto: s.auto_release,
            manual: coef(s.release_ms, self.sr),
            fast: coef(AUTO_FAST_MS * step, self.sr),
            slow: coef(AUTO_SLOW_MS * step, self.sr),
            sink: coef(AUTO_SINK_MS, self.sr),
        };
    }

    /// Samples between input and output: the look-ahead delay plus the longer alignment delay.
    pub fn latency(&self) -> usize {
        self.lookahead + self.peak_delay + self.delay_samples[0].max(self.delay_samples[1])
    }

    fn next_random(c: &mut Chan) -> f32 {
        let mut v = c.rng;
        v ^= v << 13;
        v ^= v >> 17;
        v ^= v << 5;
        c.rng = v;
        (v >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// The last `TAPS` conditioned inputs up to sample `g`, oldest first.
    fn recent_in(chan: &Chan, g: u64) -> [f32; TAPS] {
        std::array::from_fn(|j| chan.x[(g.wrapping_sub((TAPS - 1 - j) as u64) as usize) & RMASK])
    }

    fn push_peak(chan: &mut Chan, k: i64, pk: f32, lookahead: i64) {
        while chan.dq_tail > chan.dq_head && chan.dq_val[((chan.dq_tail - 1) as usize) & RMASK] <= pk {
            chan.dq_tail -= 1;
        }
        let slot = (chan.dq_tail as usize) & RMASK;
        chan.dq_idx[slot] = k;
        chan.dq_val[slot] = pk;
        chan.dq_tail += 1;
        while chan.dq_tail > chan.dq_head && chan.dq_idx[(chan.dq_head as usize) & RMASK] < k - lookahead {
            chan.dq_head += 1;
        }
    }

    fn post(&mut self, c: usize, y: f32) -> f32 {
        let mut y = y * self.out_lin;
        let bits = self.s.dither_bits;
        if (8..=24).contains(&bits) {
            let q = 2f32.powi(bits - 1);
            let r1 = Self::next_random(&mut self.chans[c]);
            let r2 = Self::next_random(&mut self.chans[c]);
            let v = (y * q + 0.5 + (r1 - r2)).floor() / q;
            y = v.clamp(-1.0, (q - 1.0) / q);
        }
        y
    }

    /// Processes one block in place. `right` is `None` for mono.
    pub fn process(&mut self, left: &mut [f32], mut right: Option<&mut [f32]>) {
        let stereo = right.is_some();
        let nch = if stereo { 2 } else { 1 };
        let p = self.peak_delay;
        let la = self.lookahead;
        let lat = (la + p) as u64;
        let linked = stereo && self.s.link;
        let rel = self.release;

        let mut clipped = false;

        self.block_gain = 1.0;

        for i in 0..left.len() {
            let g = self.counter;
            let raw = [
                left[i],
                match right.as_deref() {
                    Some(r) => r[i],
                    None => 0.0,
                },
            ];

            // 1) Input gain, phase and soft clip. Then the inter-sample peak around the sample
            //    ISP_DELAY back, which the meters always show and Upsample limits on.
            let mut isp = [0.0f32; 2];
            for c in 0..nch {
                let mut v = raw[c] * self.in_gain[c] * self.sign[c];
                if let Some(knee) = self.soft_clip {
                    if v.abs() > knee {
                        // Saturates towards 6 dB above the Soft Clip level.
                        let over = knee + knee * ((v.abs() - knee) / knee).tanh();
                        v = if v < 0.0 { -over } else { over };
                        clipped = true;
                    }
                }
                self.chans[c].x[(g as usize) & RMASK] = v;
                isp[c] = true_peak(&self.kernel, &Self::recent_in(&self.chans[c], g));
            }

            // 2) The peak of sample k = g - peak_delay joins the look-ahead window.
            let k = g as i64 - p as i64;
            let have_out = g >= lat;
            let mut req = [1.0f32; 2];
            if k >= 0 {
                for c in 0..nch {
                    let pk = if p == ISP_DELAY { isp[c] } else { self.chans[c].x[(g as usize) & RMASK].abs() };
                    Self::push_peak(&mut self.chans[c], k, pk, la as i64);
                    if self.limiting && have_out && self.chans[c].dq_tail > self.chans[c].dq_head {
                        let mw = self.chans[c].dq_val[(self.chans[c].dq_head as usize) & RMASK];
                        req[c] = if mw > self.ceil { self.ceil / mw } else { 1.0 };
                    }
                }
            }

            // 3) Gain envelope and gain for output sample t = g - lat.
            let mut y = [0.0f32; 2];
            if have_out {
                let t = (g - lat) as usize & RMASK;
                if linked {
                    let (env, slow) = step_env(&rel, self.chans[0].env, self.chans[0].slow, req[0].min(req[1]));
                    for chan in &mut self.chans {
                        chan.env = env;
                        chan.slow = slow;
                    }
                } else {
                    for chan in self.chans.iter_mut().take(nch).zip(req) {
                        let (chan, req) = chan;
                        (chan.env, chan.slow) = step_env(&rel, chan.env, chan.slow, req);
                    }
                }
                for (y, chan) in y.iter_mut().zip(&self.chans).take(nch) {
                    *y = chan.x[t] * chan.env;
                    self.block_gain = self.block_gain.min(chan.env);
                }
            }

            // 4) Output stage, output meter and per-channel alignment delay.
            let dm = self.delay_mask;
            let mut out = [0.0f32; 2];
            for c in 0..nch {
                let v = if have_out { self.post(c, y[c]) } else { 0.0 };
                let chan = &mut self.chans[c];
                chan.delay[(g as usize) & dm] = v;
                out[c] = chan.delay[(g as usize).wrapping_sub(self.delay_samples[c]) & dm];
            }

            left[i] = out[0];
            if let Some(r) = right.as_deref_mut() {
                r[i] = out[1];
            }
            self.counter += 1;
        }

        if clipped {
            if let Some(shared) = &self.shared {
                shared.mark_soft_clip();
            }
        }
    }

    /// The deepest reduction of the last block, in dB (positive).
    pub fn reduction_db(&self) -> f32 {
        -20.0 * self.block_gain.max(1e-6).log10()
    }
}
