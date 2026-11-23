//! Harmonics: controlled harmonic coloration for mastering.
//!
//! Signal flow per stereo sample (all in f64):
//!
//! ```text
//! in ──┬─────────────── delay D ─────────────── dry ──┐
//!      │                                              + ── x auto gain x trim ── out
//!      └ [L/R or M/S] ─ source HP/LP ─ curve residual  │
//!                         (ADAA1 | ADAA2 | 2x | 4x | none)
//!                         ─ DC block (even part) ─ harmonics HP/LP ─ x mix ┘  (listen: this part only)
//! ```
//!
//! The curve is `y = phi(g x) / g` with `g` the drive and `phi(u) = u + r(u)` (unit slope; see
//! [`curve`]). Only the residual `r` (what the curve adds) is anti-aliased, filtered and mixed:
//! the dry signal is a plain integer delay, so mix 0 nulls exactly, the harmonics-only listen
//! is exactly what is added, and the band filters act on the added part alone.
//!
//! Families: polynomial generator (H2-H5 exact at the reference level), soft saturator
//! `v / sqrt(1 + v^2)`, tape knee (linear, cubic knee, ceiling). The H2-H5 generator adds on
//! top of every family; bias moves the saturator and tape operating point.

pub mod adaa;
pub mod curve;
pub mod filters;
pub mod os;

use adaa::{Adaa1, Adaa2, EvenPart};
use curve::{ShapeCurve, BIAS_MAX, FAMILY_POLY};
use filters::{Biquad, DcBlock, KWeight};
use olm_api::{Kind, Meters, Module, ParamDesc};
use os::{Oversampler, MAX_TAPS, OS_LATENCY};

// ---------------------------------------------------------------- parameters

pub const P_FAMILY: usize = 0;
pub const P_H2: usize = 1;
pub const P_H3: usize = 2;
pub const P_H4: usize = 3;
pub const P_H5: usize = 4;
pub const P_DRIVE: usize = 5;
pub const P_BIAS: usize = 6;
pub const P_SRC_LOW: usize = 7;
pub const P_SRC_HIGH: usize = 8;
pub const P_HARM_LOW: usize = 9;
pub const P_HARM_HIGH: usize = 10;
pub const P_MIX: usize = 11;
pub const P_LISTEN: usize = 12;
pub const P_AUTO_GAIN: usize = 13;
pub const P_TRIM: usize = 14;
pub const P_CHANNELS: usize = 15;
pub const P_ANTI_ALIAS: usize = 16;
pub const NPARAMS: usize = 17;

pub use curve::{FAMILY_POLY as POLYNOMIAL, FAMILY_SAT as SATURATOR, FAMILY_TAPE as TAPE};

pub const CH_STEREO: usize = 0;
pub const CH_MID_SIDE: usize = 1;
pub const CH_MID: usize = 2;
pub const CH_SIDE: usize = 3;

pub const AA_ADAA: usize = 0;
pub const AA_ADAA2: usize = 1;
pub const AA_OS2: usize = 2;
pub const AA_OS4: usize = 3;
pub const AA_NONE: usize = 4;

const FAMILY_LABELS: &[&str] = &["Polynomial", "Soft saturator", "Tape knee"];
const CHANNEL_LABELS: &[&str] = &["Stereo", "Mid-Side", "Mid only", "Side only"];
const AA_LABELS: &[&str] = &["ADAA", "ADAA2", "Oversample 2x", "Oversample 4x", "None"];

/// Harmonic amounts at or below this are off.
pub const H_OFF_DB: f32 = -80.0;
pub const H_MAX_DB: f32 = -12.0;
/// Filter ends that mean "off".
pub const LOW_OFF_HZ: f32 = 10.0;
pub const HIGH_OFF_HZ: f32 = 24000.0;

pub static PARAMS: [ParamDesc; NPARAMS] = [
    ParamDesc::choice("family", "Curve", FAMILY_LABELS, FAMILY_POLY),
    ParamDesc::new("h2", "2nd harmonic", H_OFF_DB, H_MAX_DB, -30.0, Kind::Db),
    ParamDesc::new("h3", "3rd harmonic", H_OFF_DB, H_MAX_DB, -40.0, Kind::Db),
    ParamDesc::new("h4", "4th harmonic", H_OFF_DB, H_MAX_DB, H_OFF_DB, Kind::Db),
    ParamDesc::new("h5", "5th harmonic", H_OFF_DB, H_MAX_DB, H_OFF_DB, Kind::Db),
    ParamDesc::new("drive", "Drive", -24.0, 24.0, 0.0, Kind::Db),
    ParamDesc::new("bias", "Bias", -100.0, 100.0, 0.0, Kind::Percent),
    ParamDesc::new("srcLow", "Source low cut", LOW_OFF_HZ, 5000.0, LOW_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("srcHigh", "Source high cut", 500.0, HIGH_OFF_HZ, HIGH_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("harmLow", "Harmonics low cut", LOW_OFF_HZ, 5000.0, LOW_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("harmHigh", "Harmonics high cut", 500.0, HIGH_OFF_HZ, HIGH_OFF_HZ, Kind::Hz).log(),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("listen", "Harmonics only", false),
    ParamDesc::toggle("autoGain", "Auto gain", true),
    ParamDesc::new("trim", "Output trim", -24.0, 0.0, 0.0, Kind::Db),
    ParamDesc::choice("channels", "Channels", CHANNEL_LABELS, CH_STEREO),
    ParamDesc::choice("antiAlias", "Anti-alias", AA_LABELS, AA_ADAA),
];

// ---------------------------------------------------------------- constants (own choices)

/// Parameter smoothing (drive, bias, harmonic amounts, mix, trim).
const SMOOTH_S: f64 = 0.005;
/// Filter frequency smoothing (log domain); coefficients follow every `FILTER_EVERY` samples while it glides.
const FREQ_SMOOTH_S: f64 = 0.02;
const FILTER_EVERY: u32 = 1;
/// Cross-fade of the added signal when the curve family or channel mode changes.
const FADE_S: f64 = 0.005;
/// DC blocker on the added signal (house value).
pub const DC_HZ: f64 = 3.5;
/// Auto gain: K-weighted mean-square time constant, applied-gain smoothing, range, gate.
pub const AG_TAU_S: f64 = 1.5;
const AG_GAIN_S: f64 = 0.05;
pub const AG_RANGE_DB: f64 = 12.0;
const AG_GATE_MS: f64 = 1.17e-7; // -70 LUFS summed over two channels
const DRY_LEN: usize = 64;

const LN10_20: f64 = std::f64::consts::LN_10 / 20.0;

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * LN10_20).exp()
}

#[inline]
fn rho_of(db: f64) -> f64 {
    if db <= H_OFF_DB as f64 + 1e-6 {
        0.0
    } else {
        db_to_lin(db)
    }
}

// ---------------------------------------------------------------- public curve description

/// The static shaping, in the parameter units: what the editor draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// A family index (`POLYNOMIAL`, `SATURATOR`, `TAPE`).
    pub family: usize,
    /// Harmonics 2..5 in dB re the fundamental at the reference level (`H_OFF_DB` = off).
    pub harmonics_db: [f64; 4],
    pub drive_db: f64,
    /// -100..100 %.
    pub bias_pct: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Self { family: FAMILY_POLY, harmonics_db: [-30.0, -40.0, H_OFF_DB as f64, H_OFF_DB as f64], drive_db: 0.0, bias_pct: 0.0 }
    }
}

impl Shape {
    pub fn curve(&self) -> ShapeCurve {
        let h = self.harmonics_db;
        ShapeCurve::new(self.family, [rho_of(h[0]), rho_of(h[1]), rho_of(h[2]), rho_of(h[3])], BIAS_MAX * self.bias_pct.clamp(-100.0, 100.0) / 100.0)
    }
    pub fn drive(&self) -> f64 {
        db_to_lin(self.drive_db.clamp(-24.0, 24.0))
    }
}

/// The wet curve at mix 100 % before the filters: `y = phi(g x) / g`. `x` and `y` in the
/// module's linear units. Pure; the audio path runs the same formula (exactly so with
/// anti-alias "None" and the filters off).
pub fn transfer(shape: &Shape, x: f64) -> f64 {
    let g = shape.drive();
    shape.curve().phi(g * x) / g
}

/// Amplitudes of harmonics 0..=5 (0 = DC) of `phi(a cos t)` for a curve input amplitude `a`
/// (curve units), by a 256-point DFT of one period.
pub fn curve_harmonics(c: &ShapeCurve, a: f64) -> [f64; 6] {
    const N: usize = 256;
    let mut re = [0.0f64; 6];
    let mut im = [0.0f64; 6];
    for k in 0..N {
        let t = 2.0 * std::f64::consts::PI * k as f64 / N as f64;
        let (s1, c1) = t.sin_cos();
        let y = c.phi(a * c1);
        // cos(n t), sin(n t) by the Chebyshev recurrence
        let (mut cp, mut sp, mut cn, mut sn) = (1.0, 0.0, c1, s1);
        re[0] += y;
        for n in 1..6 {
            re[n] += y * cn;
            im[n] -= y * sn;
            let (cq, sq) = (2.0 * c1 * cn - cp, 2.0 * c1 * sn - sp);
            cp = cn;
            sp = sn;
            cn = cq;
            sn = sq;
        }
    }
    let mut out = [0.0; 6];
    for n in 0..6 {
        let scale = if n == 0 { 1.0 / N as f64 } else { 2.0 / N as f64 };
        out[n] = scale * (re[n] * re[n] + im[n] * im[n]).sqrt();
    }
    out
}

/// Levels of harmonics 2..5 in dB re the fundamental for a sine peaking at `peak` (module
/// linear units, so 1.0 is 0 dBFS), floored at `view::FLOOR_DB`.
pub fn harmonic_levels_db(shape: &Shape, peak: f64) -> [f64; 4] {
    let h = curve_harmonics(&shape.curve(), shape.drive() * peak.abs());
    levels_from(&h)
}

fn levels_from(h: &[f64; 6]) -> [f64; 4] {
    let mut o = [view::FLOOR_DB as f64; 4];
    if h[1] > 1e-300 {
        for n in 0..4 {
            let r = h[n + 2] / h[1];
            if r > 1e-6 {
                o[n] = (20.0 * r.log10()).max(view::FLOOR_DB as f64);
            }
        }
    }
    o
}

/// The editor read-out ([`Module::view`]), one block at a time. dB values are floored at
/// [`view::FLOOR_DB`].
pub mod view {
    /// Estimated level of harmonics 2..5 re the fundamental, for a sine peaking at this
    /// block's curve-input peak (from the current curve, [`crate::curve_harmonics`]), dB.
    pub const H2_DB: usize = 0;
    pub const H3_DB: usize = 1;
    pub const H4_DB: usize = 2;
    pub const H5_DB: usize = 3;
    /// THD estimate from the same four orders, dB re the fundamental.
    pub const THD_DB: usize = 4;
    /// Measured power of the added signal (after filters and mix) re the dry power, dB.
    pub const ADDED_DB: usize = 5;
    /// Curve-input peak in dB re the reference level (0 dB = the level the H settings hold at).
    pub const DRIVE_DB: usize = 6;
    /// Auto gain applied at the end of the block, dB.
    pub const AUTO_GAIN_DB: usize = 7;
    pub const IN_PEAK_DB: usize = 8;
    pub const OUT_PEAK_DB: usize = 9;
    pub const LEN: usize = 10;
    pub const FLOOR_DB: f32 = -120.0;
}

// ---------------------------------------------------------------- per-channel processing

#[derive(Clone, Copy, Default)]
struct Unit {
    src_hp: Biquad,
    src_lp: Biquad,
    a1: Adaa1,
    a2: Adaa2,
    os: Oversampler,
    // the same kernels on the even part only (no blend), for the DC blocker
    e1: Adaa1,
    e2: Adaa2,
    eos: Oversampler,
    dc: DcBlock,
    h_hp: Biquad,
    h_lp: Biquad,
}

struct Ctx<'a> {
    curve: &'a ShapeCurve,
    g: f64,
    aa: usize,
    /// Filter in/out weights (0 = bypassed, 1 = in), smoothed: src HP, src LP, harm HP, harm LP.
    fw: [f64; 4],
    os2: &'a [f64; MAX_TAPS],
    n2: usize,
    os4: &'a [f64; MAX_TAPS],
    n4: usize,
}

impl Unit {
    fn clear(&mut self) {
        self.src_hp.clear();
        self.src_lp.clear();
        self.a1.reset();
        self.a2.reset();
        self.os.reset();
        self.reset_even();
        self.dc.clear();
        self.h_hp.clear();
        self.h_lp.clear();
    }

    fn flush(&mut self) {
        self.src_hp.flush();
        self.src_lp.flush();
        self.a1.flush();
        self.a2.flush();
        self.os.flush();
        self.e1.flush();
        self.e2.flush();
        self.eos.flush();
        self.dc.flush();
        self.h_hp.flush();
        self.h_lp.flush();
    }

    fn reset_even(&mut self) {
        self.e1.no_blend = true;
        self.e1.reset();
        self.e2.no_blend = true;
        self.e2.reset();
        self.eos.reset();
    }

    /// Returns the added signal and the curve-input magnitude.
    #[inline]
    fn run(&mut self, c: &Ctx, x: f64) -> (f64, f64) {
        // the filters always run, so switching one in or out is a crossfade from a warm state
        let mut xs = x;
        let f = self.src_hp.run(xs);
        xs += c.fw[0] * (f - xs);
        let f = self.src_lp.run(xs);
        xs += c.fw[1] * (f - xs);
        let ev = EvenPart(c.curve);
        let (mut h, he) = match c.aa {
            AA_ADAA2 => (self.a2.step(c.curve, xs, c.g), self.e2.step(&ev, xs, c.g)),
            AA_OS2 => (self.os.step(c.curve, c.os2, c.n2, 2, xs, c.g), self.eos.step(&ev, c.os2, c.n2, 2, xs, c.g)),
            AA_OS4 => (self.os.step(c.curve, c.os4, c.n4, 4, xs, c.g), self.eos.step(&ev, c.os4, c.n4, 4, xs, c.g)),
            AA_NONE => (adaa::naive(c.curve, xs, c.g), adaa::naive(&ev, xs, c.g)),
            _ => (self.a1.step(c.curve, xs, c.g), self.e1.step(&ev, xs, c.g)),
        };
        // DC block the even part only: h = odd + DC(even)
        h += self.dc.run(he) - he;
        let f = self.h_hp.run(h);
        h += c.fw[2] * (f - h);
        let f = self.h_lp.run(h);
        h += c.fw[3] * (f - h);
        (h, (c.g * xs).abs())
    }
}

/// Latency in samples for an anti-alias choice.
pub fn latency_of(aa: usize) -> usize {
    match aa {
        AA_ADAA | AA_ADAA2 => Adaa1::DELAY,
        AA_OS2 | AA_OS4 => OS_LATENCY,
        _ => 0,
    }
}

// ---------------------------------------------------------------- the module

pub struct Harmonics {
    // targets, plain units
    family: usize,
    h_db: [f64; 4],
    drive_db: f64,
    bias_pct: f64,
    src_low: f64,
    src_high: f64,
    harm_low: f64,
    harm_high: f64,
    mix: f64,
    listen: bool,
    auto_gain: bool,
    trim_db: f64,
    channels: usize,
    aa: usize,

    // what the audio runs now (family and channels change through a fade)
    family_now: usize,
    channels_now: usize,
    fade: f64,
    fade_step: f64,

    // smoothed
    s_rho: [f64; 4],
    s_g: f64,
    s_b: f64,
    s_mix: f64,
    s_trim: f64,
    s_ag: f64,
    // first stages of the two-pole smoothers
    q_rho: [f64; 4],
    q_g: f64,
    q_b: f64,
    q_mix: f64,
    q_trim: f64,
    q_ag: f64,
    q_fw: [f64; 4],
    s_fw: [f64; 4],
    s_freq: [f64; 4], // ln Hz
    k_smooth: f64,
    k_freq: f64,
    k_ag_pow: f64,
    k_ag_gain: f64,
    filter_count: u32,

    sr: f64,
    curve: ShapeCurve,
    built_rho: [f64; 4],
    built_b: f64,
    built_family: usize,

    units: [Unit; 2],
    dry: [[f64; DRY_LEN]; 2],
    dry_pos: usize,
    os2: [f64; MAX_TAPS],
    n2: usize,
    os4: [f64; MAX_TAPS],
    n4: usize,

    kin: [KWeight; 2],
    kout: [KWeight; 2],
    p_in: f64,
    p_out: f64,
    ag_target: f64,

    meters: Meters,
    shown: [f32; view::LEN],
}

impl Default for Harmonics {
    fn default() -> Self {
        let mut os2 = [0.0; MAX_TAPS];
        let mut os4 = [0.0; MAX_TAPS];
        let n2 = os::design(2, &mut os2);
        let n4 = os::design(4, &mut os4);
        let mut m = Self {
            family: FAMILY_POLY,
            h_db: [-30.0, -40.0, H_OFF_DB as f64, H_OFF_DB as f64],
            drive_db: 0.0,
            bias_pct: 0.0,
            src_low: LOW_OFF_HZ as f64,
            src_high: HIGH_OFF_HZ as f64,
            harm_low: LOW_OFF_HZ as f64,
            harm_high: HIGH_OFF_HZ as f64,
            mix: 1.0,
            listen: false,
            auto_gain: true,
            trim_db: 0.0,
            channels: CH_STEREO,
            aa: AA_ADAA,
            family_now: FAMILY_POLY,
            channels_now: CH_STEREO,
            fade: 1.0,
            fade_step: 0.0,
            s_rho: [0.0; 4],
            s_g: 1.0,
            s_b: 0.0,
            s_mix: 1.0,
            s_trim: 1.0,
            s_ag: 1.0,
            q_rho: [0.0; 4],
            q_g: 1.0,
            q_b: 0.0,
            q_mix: 1.0,
            q_trim: 1.0,
            q_ag: 1.0,
            q_fw: [0.0; 4],
            s_fw: [0.0; 4],
            s_freq: [0.0; 4],
            k_smooth: 0.0,
            k_freq: 0.0,
            k_ag_pow: 0.0,
            k_ag_gain: 0.0,
            filter_count: 0,
            sr: 48000.0,
            curve: ShapeCurve::new(FAMILY_POLY, [0.0; 4], 0.0),
            built_rho: [0.0; 4],
            built_b: 0.0,
            built_family: FAMILY_POLY,
            units: [Unit::default(); 2],
            dry: [[0.0; DRY_LEN]; 2],
            dry_pos: 0,
            os2,
            n2,
            os4,
            n4,
            kin: [KWeight::default(); 2],
            kout: [KWeight::default(); 2],
            p_in: 0.0,
            p_out: 0.0,
            ag_target: 1.0,
            meters: Meters::default(),
            shown: [view::FLOOR_DB; view::LEN],
        };
        m.prepare(48000.0, 0);
        m
    }
}

impl Harmonics {
    /// The current target shaping (what the editor should draw).
    pub fn shape(&self) -> Shape {
        Shape { family: self.family, harmonics_db: self.h_db, drive_db: self.drive_db, bias_pct: self.bias_pct }
    }

    /// The current plain value of parameter `index` (default for an unknown index: NaN).
    pub fn param(&self, index: usize) -> f32 {
        let v = match index {
            P_FAMILY => self.family as f64,
            P_H2..=P_H5 => self.h_db[index - P_H2],
            P_DRIVE => self.drive_db,
            P_BIAS => self.bias_pct,
            P_SRC_LOW => self.src_low,
            P_SRC_HIGH => self.src_high,
            P_HARM_LOW => self.harm_low,
            P_HARM_HIGH => self.harm_high,
            P_MIX => self.mix * 100.0,
            P_LISTEN => self.listen as u8 as f64,
            P_AUTO_GAIN => self.auto_gain as u8 as f64,
            P_TRIM => self.trim_db,
            P_CHANNELS => self.channels as f64,
            P_ANTI_ALIAS => self.aa as f64,
            _ => f64::NAN,
        };
        v as f32
    }

    fn targets_rho(&self) -> [f64; 4] {
        [rho_of(self.h_db[0]), rho_of(self.h_db[1]), rho_of(self.h_db[2]), rho_of(self.h_db[3])]
    }

    fn filter_weights(&self) -> [f64; 4] {
        let lo = |hz: f64| if hz <= LOW_OFF_HZ as f64 + 1e-3 { 0.0 } else { 1.0 };
        let hi = |hz: f64| if hz >= HIGH_OFF_HZ as f64 - 1e-3 { 0.0 } else { 1.0 };
        [lo(self.src_low), hi(self.src_high), lo(self.harm_low), hi(self.harm_high)]
    }

    fn freq_targets(&self) -> [f64; 4] {
        [self.src_low.ln(), self.src_high.ln(), self.harm_low.ln(), self.harm_high.ln()]
    }

    fn snap(&mut self) {
        self.s_rho = self.targets_rho();
        self.s_g = db_to_lin(self.drive_db);
        self.s_b = BIAS_MAX * self.bias_pct / 100.0;
        self.s_mix = self.mix;
        self.s_trim = db_to_lin(self.trim_db);
        self.q_rho = self.s_rho;
        self.q_g = self.s_g;
        self.q_b = self.s_b;
        self.q_mix = self.s_mix;
        self.q_trim = self.s_trim;
        self.s_fw = self.filter_weights();
        self.q_fw = self.s_fw;
        self.s_freq = self.freq_targets();
        self.family_now = self.family;
        self.channels_now = self.channels;
        self.fade = 1.0;
        self.rebuild_curve(true);
        self.update_filters();
    }

    fn rebuild_curve(&mut self, force: bool) {
        if force || self.built_family != self.family_now {
            self.curve = ShapeCurve::new(self.family_now, self.s_rho, self.s_b);
            self.built_family = self.family_now;
            self.built_rho = self.s_rho;
            self.built_b = self.s_b;
            return;
        }
        if self.built_rho != self.s_rho {
            self.curve.set_generator(self.s_rho);
            self.built_rho = self.s_rho;
        }
        if self.built_b != self.s_b {
            self.curve.set_bias(self.s_b);
            self.built_b = self.s_b;
        }
    }

    fn update_filters(&mut self) {
        let sr = self.sr;
        let f = self.s_freq;
        for u in self.units.iter_mut() {
            u.src_hp.set_butter(true, f[0].exp(), sr);
            u.src_lp.set_butter(false, f[1].exp(), sr);
            u.h_hp.set_butter(true, f[2].exp(), sr);
            u.h_lp.set_butter(false, f[3].exp(), sr);
        }
    }

    #[inline]
    fn smooth(cur: &mut f64, target: f64, k: f64) -> bool {
        let d = target - *cur;
        if d == 0.0 {
            return false;
        }
        if d.abs() < 1e-9 * (1.0 + target.abs()) {
            *cur = target;
        } else {
            *cur = target - d * k;
        }
        true
    }

    /// Two one-pole stages in series (critically damped): a ramp starts without a slope step.
    #[inline]
    fn smooth2(first: &mut f64, out: &mut f64, target: f64, k: f64) -> bool {
        let a = Self::smooth(first, target, k);
        let b = Self::smooth(out, *first, k);
        a || b
    }
}

impl Module for Harmonics {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate > 1000.0 { sample_rate as f64 } else { 48000.0 };
        self.sr = sr;
        // two-pole smoothers: each stage at half the time
        self.k_smooth = (-2.0 / (SMOOTH_S * sr)).exp();
        self.k_freq = (-(FILTER_EVERY as f64) / (FREQ_SMOOTH_S * sr)).exp();
        self.k_ag_pow = (-1.0 / (AG_TAU_S * sr)).exp();
        self.k_ag_gain = (-2.0 / (AG_GAIN_S * sr)).exp();
        self.fade_step = 1.0 / (FADE_S * sr);
        for u in self.units.iter_mut() {
            u.dc.set(DC_HZ, sr);
        }
        for k in self.kin.iter_mut().chain(self.kout.iter_mut()) {
            k.set(sr);
        }
        self.reset();
    }

    fn reset(&mut self) {
        for u in self.units.iter_mut() {
            u.clear();
        }
        self.dry = [[0.0; DRY_LEN]; 2];
        self.dry_pos = 0;
        for k in self.kin.iter_mut().chain(self.kout.iter_mut()) {
            k.clear();
        }
        self.p_in = 0.0;
        self.p_out = 0.0;
        self.ag_target = 1.0;
        self.s_ag = 1.0;
        self.q_ag = 1.0;
        self.filter_count = 0;
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
            P_FAMILY => self.family = (v.round() as usize).min(FAMILY_LABELS.len() - 1),
            P_H2..=P_H5 => self.h_db[index - P_H2] = v,
            P_DRIVE => self.drive_db = v,
            P_BIAS => self.bias_pct = v,
            P_SRC_LOW => self.src_low = v,
            P_SRC_HIGH => self.src_high = v,
            P_HARM_LOW => self.harm_low = v,
            P_HARM_HIGH => self.harm_high = v,
            P_MIX => self.mix = v / 100.0,
            P_LISTEN => self.listen = v >= 0.5,
            P_AUTO_GAIN => self.auto_gain = v >= 0.5,
            P_TRIM => self.trim_db = v,
            P_CHANNELS => self.channels = (v.round() as usize).min(CHANNEL_LABELS.len() - 1),
            P_ANTI_ALIAS => {
                let a = (v.round() as usize).min(AA_LABELS.len() - 1);
                if a != self.aa {
                    // The latency changes now; the added signal restarts from silence.
                    self.aa = a;
                    for u in self.units.iter_mut() {
                        u.a1.reset();
                        u.a2.reset();
                        u.os.reset();
                        u.reset_even();
                    }
                    self.fade = 0.0;
                }
            }
            _ => {}
        }
    }

    fn latency(&self) -> usize {
        latency_of(self.aa)
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let rho_t = self.targets_rho();
        let g_t = db_to_lin(self.drive_db);
        let b_t = BIAS_MAX * self.bias_pct / 100.0;
        let mix_t = self.mix;
        let trim_t = db_to_lin(self.trim_db);
        let f_t = self.freq_targets();
        let k = self.k_smooth;
        let d = latency_of(self.aa);
        let fw_t = self.filter_weights();
        let ag_lo = db_to_lin(-AG_RANGE_DB);
        let ag_hi = db_to_lin(AG_RANGE_DB);

        let mut pk_in: f64 = 0.0;
        let mut pk_out: f64 = 0.0;
        let mut pk_u: f64 = 0.0;
        let mut e_add = 0.0;
        let mut e_dry = 0.0;

        for i in 0..n {
            // parameter smoothing and the curve of this sample
            let mut moved = false;
            for j in 0..4 {
                moved |= Self::smooth2(&mut self.q_rho[j], &mut self.s_rho[j], rho_t[j], k);
            }
            Self::smooth2(&mut self.q_g, &mut self.s_g, g_t, k);
            moved |= Self::smooth2(&mut self.q_b, &mut self.s_b, b_t, k);
            Self::smooth2(&mut self.q_mix, &mut self.s_mix, mix_t, k);
            Self::smooth2(&mut self.q_trim, &mut self.s_trim, trim_t, k);
            for j in 0..4 {
                Self::smooth2(&mut self.q_fw[j], &mut self.s_fw[j], fw_t[j], k);
            }

            // family / channel changes go through a short fade of the added signal
            let pending = self.family != self.family_now || self.channels != self.channels_now;
            if pending {
                self.fade -= self.fade_step;
                if self.fade <= 0.0 {
                    self.fade = 0.0;
                    if self.channels != self.channels_now {
                        for u in self.units.iter_mut() {
                            u.clear();
                        }
                    }
                    self.family_now = self.family;
                    self.channels_now = self.channels;
                }
            } else if self.fade < 1.0 {
                self.fade = (self.fade + self.fade_step).min(1.0);
            }
            if moved || self.built_family != self.family_now {
                self.rebuild_curve(false);
            }

            if self.filter_count == 0 {
                let mut changed = false;
                for j in 0..4 {
                    changed |= Self::smooth(&mut self.s_freq[j], f_t[j], self.k_freq);
                }
                if changed {
                    self.update_filters();
                }
            }
            self.filter_count = (self.filter_count + 1) % FILTER_EVERY;

            // a non-finite input would stay in every filter state: treat it as silence
            let l = if left[i].is_finite() { left[i] as f64 } else { 0.0 };
            let r = if right[i].is_finite() { right[i] as f64 } else { 0.0 };
            pk_in = pk_in.max(l.abs()).max(r.abs());

            // dry, delayed by the latency
            self.dry_pos = (self.dry_pos + 1) & (DRY_LEN - 1);
            self.dry[0][self.dry_pos] = l;
            self.dry[1][self.dry_pos] = r;
            let rp = self.dry_pos.wrapping_sub(d) & (DRY_LEN - 1);
            let dl = self.dry[0][rp];
            let dr = self.dry[1][rp];

            // the added signal
            let ctx = Ctx {
                curve: &self.curve,
                g: self.s_g,
                aa: self.aa,
                fw: self.s_fw,
                os2: &self.os2,
                n2: self.n2,
                os4: &self.os4,
                n4: self.n4,
            };
            let (hl, hr) = match self.channels_now {
                CH_MID_SIDE | CH_MID | CH_SIDE => {
                    let m = 0.5 * (l + r);
                    let s = 0.5 * (l - r);
                    let (hm, um) = if self.channels_now != CH_SIDE { self.units[0].run(&ctx, m) } else { (0.0, 0.0) };
                    let (hs, us) = if self.channels_now != CH_MID { self.units[1].run(&ctx, s) } else { (0.0, 0.0) };
                    pk_u = pk_u.max(um).max(us);
                    (hm + hs, hm - hs)
                }
                _ => {
                    let (a, ua) = self.units[0].run(&ctx, l);
                    let (b, ub) = self.units[1].run(&ctx, r);
                    pk_u = pk_u.max(ua).max(ub);
                    (a, b)
                }
            };
            // raised-cosine fade: no slope step at either end
            let w = self.s_mix * (0.5 - 0.5 * (std::f64::consts::PI * self.fade).cos());
            let al = w * hl;
            let ar = w * hr;
            let ol = dl + al;
            let or = dr + ar;
            e_add += al * al + ar * ar;
            e_dry += dl * dl + dr * dr;

            // auto gain: K-weighted delayed input against the output, before the gain
            let ki = (self.kin[0].run(dl), self.kin[1].run(dr));
            let ko = (self.kout[0].run(ol), self.kout[1].run(or));
            let kp = self.k_ag_pow;
            self.p_in = ki.0 * ki.0 + ki.1 * ki.1 + kp * (self.p_in - (ki.0 * ki.0 + ki.1 * ki.1));
            self.p_out = ko.0 * ko.0 + ko.1 * ko.1 + kp * (self.p_out - (ko.0 * ko.0 + ko.1 * ko.1));
            if self.p_in > AG_GATE_MS && self.p_out > 1e-30 {
                self.ag_target = (self.p_in / self.p_out).sqrt().clamp(ag_lo, ag_hi);
            }
            let ag_goal = if self.auto_gain { self.ag_target } else { 1.0 };
            Self::smooth2(&mut self.q_ag, &mut self.s_ag, ag_goal, self.k_ag_gain);
            let gain = self.s_ag * self.s_trim;

            let (yl, yr) = if self.listen { (al * gain, ar * gain) } else { (ol * gain, or * gain) };
            pk_out = pk_out.max(yl.abs()).max(yr.abs());
            left[i] = yl as f32;
            right[i] = yr as f32;
        }

        for u in self.units.iter_mut() {
            u.flush();
        }
        for kw in self.kin.iter_mut().chain(self.kout.iter_mut()) {
            kw.flush();
        }

        // read-outs
        let lv = |x: f64| if x > 1e-6 { ((20.0 * x.log10()) as f32).max(view::FLOOR_DB) } else { view::FLOOR_DB };
        let mut est = [view::FLOOR_DB as f64; 4];
        let mut thd = view::FLOOR_DB;
        if pk_u > 1e-6 {
            let h = curve_harmonics(&self.curve, pk_u);
            est = levels_from(&h);
            if h[1] > 1e-300 {
                let p: f64 = (2..6).map(|j| h[j] * h[j]).sum();
                thd = lv(p.sqrt() / h[1]);
            }
        }
        let added = if e_dry > 1e-24 && e_add > 0.0 { ((10.0 * (e_add / e_dry).log10()) as f32).max(view::FLOOR_DB) } else { view::FLOOR_DB };
        let ag_db = (20.0 * self.s_ag.log10()) as f32;
        self.shown = [est[0] as f32, est[1] as f32, est[2] as f32, est[3] as f32, thd, added, lv(pk_u), ag_db, lv(pk_in), lv(pk_out)];
        self.meters.reduction_db = (-ag_db).max(0.0);
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
