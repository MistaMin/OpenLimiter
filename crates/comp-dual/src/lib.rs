//! `comp-dual`: a dual-set wideband compressor / de-esser with a safety limiter.
//!
//! Clean-room implementation from measured behaviour only. The numbers behind every law are in
//! `spec/SPEC_LIMITERS.md` (sections 17, 20, 22, 24, 25, 28, 32, 34, 37, 39, 57, 58, 67, 69, 70)
//! and in the curve files `spec/curves/srcB__*.csv` and `spec/curves/cl_*.csv`. `REPORT.md` in
//! this crate lists what was matched, what was chosen freely and what was left out.
//!
//! Signal path (per sample):
//!
//! ```text
//! in -> input gain -> [mid/side] -> band split (linear-phase FIR, band + remainder)
//!    -> detector (true peak or RMS; own band or external key) -> look-ahead hold
//!    -> attack / staged release follower -> static curve -> gain on the band, make-up on the band
//!    -> band + remainder (or band alone = solo) -> parallel mix with the dry signal
//!    -> [left/right] -> limiter gain -> safety limiter -> output gain -> out
//! ```
#![forbid(unsafe_code)]

use olm_api::{Kind, Meters, Module, ParamDesc};

mod parts;
use parts::{Delay, Follower, Limiter, SlideMax, Splitter, Timing, TruePeak, OVERSAMPLE, TP_DELAY};

// ---------------------------------------------------------------------------------------------
// Constants of the model (how each was found is in REPORT.md, "Model").
// ---------------------------------------------------------------------------------------------

/// Attack and Average: time constant = half the displayed time. Fitted on the attack rows of
/// `measurements/srcB_features/srcB_timing_8s.json` and the look-ahead rows of `jobs3/all_jobs.json`.
const TIME_DIVISOR: f64 = 2.0;
/// A release stage drops the detector level by this many dB per displayed time (e^-2).
/// Read from `spec/curves/srcB__ratio_1000to1_knee0.csv` (173.6 dB/s at 100 ms).
const RELEASE_DB_PER_TIME: f64 = 17.372;
/// The fast stage runs while the detector level is above this factor times the averaged
/// rectified level, the slow stage otherwise. Fitted on `spec/curves/srcB__release_slow_2600ms.csv`
/// and the 90 % release times of `measurements/srcB_features/srcB_static_and_release_grid.json`.
const STAGE_SWITCH: f64 = 2.03;
/// The top of the full soft knee sits at this detector level (dBFS), whatever the threshold.
/// From the static rows of `measurements/srcB_features/srcB_static_and_release_grid.json`.
const KNEE_TOP_DB: f64 = 6.0;
/// The RMS detector reads this factor (+0.09 dB) above the true RMS: sine and square both settle
/// 0.08 to 0.10 dB lower than a true RMS gives in `measurements/float_check/srcB_rms_dither.json`.
const RMS_TRIM: f64 = 1.010_416;
/// Oversampled steps taken off the look-ahead hold. The minimum look-ahead (0.02 ms, one sample
/// at 48 kHz) then holds the detector input for half a sample; fitted on the 5 kHz rows of
/// `measurements/srcB_features/filter_audio_path.json` against the Release Slow rows of `jobs3`.
const HOLD_TRIM: usize = 2;
/// Band edges are at this factor times the displayed frequency (low band at "415 Hz": half
/// amplitude at 406 Hz in `measurements/srcB_features/filter_shape.json`).
const EDGE_SHIFT: f64 = 0.98;
/// Upward ratios are not allowed to add more than this (free choice, keeps numbers finite).
const MAX_UPWARD_DB: f64 = 60.0;
/// Narrowest band-edge transition in Hz; it fixes the length of the split filter.
const MIN_EDGE_WIDTH_HZ: f64 = 30.0;
/// The split kernel is cut at this many standard deviations of its Gaussian window.
const KERNEL_SIGMAS: f64 = 3.5;
/// Safety limiter look-ahead and release (free choices; the source's were not timed).
const LIMITER_LOOKAHEAD_MS: f64 = 1.5;
const LIMITER_RELEASE_MS: f64 = 80.0;
/// Ceilings of the three limiter types, dBFS / dBFS / dBTP (section 57.3).
const CEILINGS_DB: [f64; 3] = [-0.05, -0.01, -0.20];

// ---------------------------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------------------------

/// Parameter indices. The two sets follow the globals: `SET_BASE + set * SET_LEN + offset`.
pub mod index {
    pub const INPUT_GAIN: usize = 0;
    pub const OUTPUT_GAIN: usize = 1;
    pub const LIMITER_GAIN: usize = 2;
    pub const LIMITER_ON: usize = 3;
    pub const LIMITER_TYPE: usize = 4;
    pub const BAND_SOLO: usize = 5;
    pub const CHANNEL_MODE: usize = 6;
    pub const GANGED_SET: usize = 7;
    pub const AUTO_MAKEUP: usize = 8;
    pub const LOOKAHEAD: usize = 9;
    pub const DETECTOR: usize = 10;
    pub const RMS_TIME: usize = 11;
    pub const KEY_SOURCE: usize = 12;
    pub const KEY_LINK: usize = 13;
    pub const KEY_FILTER: usize = 14;
    pub const PARALLEL_MODE: usize = 15;
    pub const PARALLEL_MIX: usize = 16;
    pub const SET_BASE: usize = 17;
    pub const SET_LEN: usize = 14;
    // offsets inside a set
    pub const FILTER_TYPE: usize = 0;
    pub const BANDWIDTH: usize = 1;
    pub const CENTER_FREQ: usize = 2;
    pub const ATTACK: usize = 3;
    pub const RELEASE_DELAY: usize = 4;
    pub const RELEASE_FAST: usize = 5;
    pub const RELEASE_AVERAGE: usize = 6;
    pub const RELEASE_SLOW: usize = 7;
    pub const AUTO_RELEASE: usize = 8;
    pub const AUTO_RELEASE_TIME: usize = 9;
    pub const SOFT_KNEE: usize = 10;
    pub const THRESHOLD: usize = 11;
    pub const RATIO: usize = 12;
    pub const MAKEUP: usize = 13;
    pub const COUNT: usize = SET_BASE + 2 * SET_LEN;
}
use index as ix;

const LIMITER_TYPES: &[&str] = &["Sample -0.05 dB", "Sample -0.01 dB", "True peak"];
const CHANNEL_MODES: &[&str] = &["Ganged", "Per channel", "Mid / Side"];
const SETS: &[&str] = &["Set 1", "Set 2"];
const DETECTORS: &[&str] = &["Peak", "RMS"];
const KEY_SOURCES: &[&str] = &["Internal", "External"];
const PARALLEL_MODES: &[&str] = &["Off", "Sum", "Blend"];
const FILTER_TYPES: &[&str] = &["Low band", "Mid band", "High band", "Full band"];

const FT_LOW: u8 = 0;
const FT_MID: u8 = 1;
const FT_HIGH: u8 = 2;
const FT_FULL: u8 = 3;

macro_rules! set_params {
    ($n:literal) => {
        [
            ParamDesc::choice(concat!("filterType", $n), concat!("Band ", $n), FILTER_TYPES, 3),
            ParamDesc::new(concat!("bandwidth", $n), concat!("Band Width ", $n, " (oct)"), 1.0 / 6.0, 8.0, 2.0, Kind::Plain),
            ParamDesc::new(concat!("centerFreq", $n), concat!("Band Frequency ", $n), 41.0, 17740.0, 415.0, Kind::Hz).log(),
            ParamDesc::new(concat!("attack", $n), concat!("Attack ", $n), 0.02, 800.0, 31.5, Kind::Ms).log(),
            ParamDesc::new(concat!("releaseDelay", $n), concat!("Release Hold ", $n), 0.02, 8000.0, 0.02, Kind::Ms).log(),
            ParamDesc::new(concat!("releaseFast", $n), concat!("Release Fast ", $n), 0.02, 8000.0, 100.0, Kind::Ms).log(),
            ParamDesc::new(concat!("releaseAverage", $n), concat!("Release Average ", $n), 0.02, 8000.0, 50.0, Kind::Ms).log(),
            ParamDesc::new(concat!("releaseSlow", $n), concat!("Release Slow ", $n), 0.02, 8000.0, 200.0, Kind::Ms).log(),
            ParamDesc::toggle(concat!("autoRelease", $n), concat!("Auto Release ", $n), false),
            ParamDesc::new(concat!("autoReleaseTime", $n), concat!("Auto Release Time ", $n), 0.63, 8000.0, 100.0, Kind::Ms).log(),
            ParamDesc::new(concat!("softKnee", $n), concat!("Soft Knee ", $n), 0.0, 100.0, 100.0, Kind::Percent),
            ParamDesc::new(concat!("threshold", $n), concat!("Threshold ", $n), -60.0, 0.0, 0.0, Kind::Db),
            ParamDesc::new(concat!("ratio", $n), concat!("Ratio ", $n), 0.2, 1000.0, 2.0, Kind::Ratio).log(),
            ParamDesc::new(concat!("makeup", $n), concat!("Make-Up ", $n), -60.0, 60.0, 0.0, Kind::Db),
        ]
    };
}

const GLOBALS: [ParamDesc; ix::SET_BASE] = [
    ParamDesc::new("inputGain", "Input Gain", -60.0, 12.0, 0.0, Kind::Db),
    ParamDesc::new("outputGain", "Output Gain", -60.0, 12.0, 0.0, Kind::Db),
    ParamDesc::new("limiterGain", "Limiter Drive", 0.0, 30.0, 0.0, Kind::Db),
    ParamDesc::toggle("limiterOn", "Safety Limiter", true),
    ParamDesc::choice("limiterType", "Ceiling Type", LIMITER_TYPES, 1),
    ParamDesc::toggle("bandSolo", "Band Solo", false),
    ParamDesc::choice("channelMode", "Channel Mode", CHANNEL_MODES, 0),
    ParamDesc::choice("gangedSet", "Ganged Set", SETS, 0),
    ParamDesc::toggle("autoMakeup", "Auto Make-Up", false),
    ParamDesc::new("lookahead", "Look-Ahead", 0.02, 63.0, 0.02, Kind::Ms).log(),
    ParamDesc::choice("detector", "Detector", DETECTORS, 0),
    ParamDesc::new("rmsTime", "RMS Time", 0.12, 8000.0, 20.0, Kind::Ms).log(),
    ParamDesc::choice("keySource", "Key Source", KEY_SOURCES, 0),
    ParamDesc::toggle("keyLink", "Key Link", false),
    ParamDesc::toggle("keyFilter", "Key Filter", false),
    ParamDesc::choice("parallelMode", "Parallel", PARALLEL_MODES, 0),
    ParamDesc::new("parallelMix", "Parallel Mix", 0.0, 100.0, 100.0, Kind::Percent),
];

const fn all_params() -> [ParamDesc; ix::COUNT] {
    let a = set_params!("1");
    let b = set_params!("2");
    let mut out = [GLOBALS[0]; ix::COUNT];
    let mut i = 0;
    while i < ix::SET_BASE {
        out[i] = GLOBALS[i];
        i += 1;
    }
    let mut j = 0;
    while j < ix::SET_LEN {
        out[ix::SET_BASE + j] = a[j];
        out[ix::SET_BASE + ix::SET_LEN + j] = b[j];
        j += 1;
    }
    out
}

static PARAMS: [ParamDesc; ix::COUNT] = all_params();

// ---------------------------------------------------------------------------------------------
// Static curve (sections 24 and 37)
// ---------------------------------------------------------------------------------------------

/// Gain in dB for a detector level in dBFS.
///
/// * `ratio >= 1`: compression. Hard knee: `-(1 - 1/ratio) * (level - threshold)` above the
///   threshold. Soft knee: a quadratic of half-width `h` around the threshold, taken on the
///   input level.
/// * `ratio < 1` (written 1:x, x = 1/ratio): upward. The same knee, but taken on the level
///   after the gain, which is what the measured 1:2.40 row shows. Solved in closed form.
pub fn static_gain_db(level: f64, threshold: f64, h: f64, ratio: f64) -> f64 {
    let over = level - threshold;
    if ratio >= 1.0 {
        let s = 1.0 - 1.0 / ratio;
        if h <= 0.0 {
            return -s * over.max(0.0);
        }
        let u = over + h;
        if u <= 0.0 {
            0.0
        } else if u < 2.0 * h {
            -s * u * u / (4.0 * h)
        } else {
            -s * over
        }
    } else {
        let c = 1.0 - ratio.max(1.0e-3);
        let linear = c * over / (1.0 - c);
        let g = if h <= 0.0 {
            linear.max(0.0)
        } else {
            let u = over + h;
            if u <= 0.0 {
                0.0
            } else {
                let disc = h * (h - c * u);
                if disc >= 0.0 {
                    let g = (2.0 * h - c * u - 2.0 * disc.sqrt()) / c;
                    if u + g < 2.0 * h {
                        g
                    } else {
                        linear
                    }
                } else {
                    linear
                }
            }
        };
        g.min(MAX_UPWARD_DB)
    }
}

/// Half-width of the soft knee in dB. Measured at 100 only (36 dB at -30, 54 dB at -48);
/// scaling it with the percentage is a free choice.
pub fn knee_half_width_db(threshold: f64, knee_percent: f64) -> f64 {
    (knee_percent / 100.0).clamp(0.0, 1.0) * (KNEE_TOP_DB - threshold)
}

/// Auto make-up in dB: the gain that brings a full-scale input back to full scale (section 37).
pub fn auto_makeup_db(threshold: f64, ratio: f64) -> f64 {
    -threshold * (1.0 - 1.0 / ratio.max(1.0e-3))
}

/// Width `w` in Hz of a band edge at `f` Hz: the band's share of a tone across the edge is
/// `0.5 * erfc((f - edge) / w)` (10 to 90 % takes about 1.8 `w`). Log-log interpolation of the
/// widths read from `measurements/srcB_features/filter_shape.json` (derivation in REPORT.md).
pub fn edge_width_hz(f: f64) -> f64 {
    const T: [(f64, f64); 8] = [
        (41.0, 32.0),
        (95.0, 30.0),
        (207.0, 45.0),
        (415.0, 60.0),
        (800.0, 154.0),
        (1760.0, 480.0),
        (6600.0, 1650.0),
        (17740.0, 2800.0),
    ];
    if f <= T[0].0 {
        return T[0].1;
    }
    let mut i = 0;
    while i + 1 < T.len() {
        let (f0, w0) = T[i];
        let (f1, w1) = T[i + 1];
        if f <= f1 {
            let t = (f / f0).ln() / (f1 / f0).ln();
            return (w0.ln() + t * (w1.ln() - w0.ln())).exp().max(MIN_EDGE_WIDTH_HZ);
        }
        i += 1;
    }
    T[T.len() - 1].1 * f / T[T.len() - 1].0
}

// ---------------------------------------------------------------------------------------------
// Derived per-set values
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct SetCoef {
    ftype: u8,
    threshold: f64,
    knee_h: f64,
    ratio: f64,
    makeup: f32,
    /// The make-up in dB, auto make-up included (for the read-out).
    makeup_db: f32,
    timing: Timing,
}

impl SetCoef {
    #[inline]
    fn gain_db(&self, level_lin: f64) -> f64 {
        let l = 20.0 * level_lin.max(1.0e-12).log10();
        static_gain_db(l, self.threshold, self.knee_h, self.ratio)
    }
}

#[inline]
fn db_to_lin(db: f64) -> f64 {
    (db * (std::f64::consts::LN_10 / 20.0)).exp()
}

#[inline]
fn clean(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

#[inline]
fn one_pole(time_s: f64, sr: f64) -> f64 {
    let n = time_s * sr;
    if n <= 1.0e-3 {
        1.0
    } else {
        1.0 - (-1.0 / n).exp()
    }
}

// ---------------------------------------------------------------------------------------------
// The module
// ---------------------------------------------------------------------------------------------

/// The dual-set compressor / de-esser. See the crate documentation and `REPORT.md`.
pub struct CompDual {
    v: [f32; ix::COUNT],
    sr: f64,
    ready: bool,

    set: [SetCoef; 2],
    gin_t: f32,
    gout_t: f32,
    glim_t: f32,
    mix_t: f32,
    gin: f32,
    gout: f32,
    glim: f32,
    mix: f32,
    mk: [f32; 2],
    ksm: f32,
    krms: f64,
    split_on: bool,
    d_split: usize,
    la: usize,
    hold4: usize,
    la_max: usize,
    ceiling: f32,

    taps: [Vec<f32>; 2],
    half: [usize; 2],
    taps_dirty: bool,
    split: [Splitter; 2],
    key_split: [Splitter; 2],
    tp: [TruePeak; 2],
    pw: [f64; 2],
    smax: [SlideMax; 2],
    fol: [Follower; 2],
    dl_band: [Delay; 2],
    dl_dry: [Delay; 2],
    lim: Limiter,
    meters: Meters,
    /// The editor read-out of the last block (see [`view`]).
    view: [f32; view::LEN],
}

impl Default for CompDual {
    fn default() -> Self {
        let mut v = [0.0f32; ix::COUNT];
        for (slot, p) in v.iter_mut().zip(PARAMS.iter()) {
            *slot = p.default;
        }
        let mut s = Self {
            v,
            sr: 48_000.0,
            ready: false,
            set: [SetCoef::default(); 2],
            gin_t: 1.0,
            gout_t: 1.0,
            glim_t: 1.0,
            mix_t: 1.0,
            gin: 1.0,
            gout: 1.0,
            glim: 1.0,
            mix: 1.0,
            mk: [1.0; 2],
            ksm: 1.0,
            krms: 1.0,
            split_on: false,
            d_split: 0,
            la: 0,
            hold4: 1,
            la_max: 1,
            ceiling: 1.0,
            taps: [Vec::new(), Vec::new()],
            half: [0; 2],
            taps_dirty: true,
            split: [Splitter::default(), Splitter::default()],
            key_split: [Splitter::default(), Splitter::default()],
            tp: [TruePeak::new(), TruePeak::new()],
            pw: [0.0; 2],
            smax: [SlideMax::default(), SlideMax::default()],
            fol: [Follower::default(); 2],
            dl_band: [Delay::default(), Delay::default()],
            dl_dry: [Delay::default(), Delay::default()],
            lim: Limiter::default(),
            meters: Meters::default(),
            view: [0.0; view::LEN],
        };
        s.update();
        s.snap();
        s
    }
}

impl CompDual {
    /// Index of the parameter with this id, if there is one.
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// Current value of a parameter (after clamping).
    pub fn param(&self, index: usize) -> Option<f32> {
        self.v.get(index).copied()
    }

    #[inline]
    fn sv(&self, set: usize, off: usize) -> f64 {
        self.v[ix::SET_BASE + set * ix::SET_LEN + off] as f64
    }

    #[inline]
    fn choice(&self, i: usize) -> usize {
        self.v[i].round().max(0.0) as usize
    }

    #[inline]
    fn on(&self, i: usize) -> bool {
        self.v[i] >= 0.5
    }

    /// Recomputes everything derived from the parameters. Never allocates.
    fn update(&mut self) {
        let sr = self.sr;
        let trim = |db: f32, min: f32| if db <= min + 0.005 { 0.0 } else { db_to_lin(db as f64) as f32 };
        // Input and output gain: plain dB; the bottom of the range is silence, as the source's
        // taper ends in "-inf" (section 58).
        self.gin_t = trim(self.v[ix::INPUT_GAIN], PARAMS[ix::INPUT_GAIN].min);
        self.gout_t = trim(self.v[ix::OUTPUT_GAIN], PARAMS[ix::OUTPUT_GAIN].min);
        self.glim_t = db_to_lin(self.v[ix::LIMITER_GAIN] as f64) as f32;
        self.mix_t = (self.v[ix::PARALLEL_MIX] / 100.0).clamp(0.0, 1.0);
        self.ksm = one_pole(0.005, sr) as f32;
        // The detector runs at 4x. Look-ahead: the detector input is held (sliding maximum) over
        // the look-ahead time and the audio is delayed by it; the minimum setting means none.
        let dsr = sr * OVERSAMPLE as f64;
        // RMS: one pole on the squared signal, time constant = half the displayed time (70.1).
        self.krms = one_pole(self.v[ix::RMS_TIME] as f64 * 0.5e-3, dsr);
        let n4 = ((self.v[ix::LOOKAHEAD] as f64 * 1.0e-3 * dsr).round() as usize).saturating_sub(HOLD_TRIM);
        self.hold4 = n4.clamp(1, (self.la_max * OVERSAMPLE).max(1));
        self.la = ((self.hold4 - 1) as f64 / OVERSAMPLE as f64 - 0.75).floor().max(0.0) as usize;
        self.ceiling = db_to_lin(CEILINGS_DB[self.choice(ix::LIMITER_TYPE).min(2)]) as f32;

        let auto_mk = self.on(ix::AUTO_MAKEUP);
        for s in 0..2 {
            let thr = self.sv(s, ix::THRESHOLD);
            let ratio = self.sv(s, ix::RATIO);
            let fast = self.sv(s, ix::RELEASE_FAST);
            // Auto release (measured on the sibling unit, section 37): the slow stage becomes
            // auto + 2 x fast and the averaging time half of that; Average and Slow are ignored.
            let (avg, slow) = if self.sv(s, ix::AUTO_RELEASE) >= 0.5 {
                let t = self.sv(s, ix::AUTO_RELEASE_TIME) + 2.0 * fast;
                (0.5 * t, t)
            } else {
                (self.sv(s, ix::RELEASE_AVERAGE), self.sv(s, ix::RELEASE_SLOW))
            };
            let per_sample = |ms: f64| db_to_lin(-RELEASE_DB_PER_TIME / (ms * 1.0e-3 * dsr).max(1.0e-3));
            let mut mk_db = self.sv(s, ix::MAKEUP);
            if auto_mk {
                mk_db += auto_makeup_db(thr, ratio);
            }
            self.set[s] = SetCoef {
                ftype: (self.sv(s, ix::FILTER_TYPE).round() as i64).clamp(0, 3) as u8,
                threshold: thr,
                knee_h: knee_half_width_db(thr, self.sv(s, ix::SOFT_KNEE)),
                ratio,
                makeup: db_to_lin(mk_db.clamp(-120.0, 120.0)) as f32,
                makeup_db: mk_db.clamp(-120.0, 120.0) as f32,
                timing: Timing {
                    attack: one_pole(self.sv(s, ix::ATTACK) * 1.0e-3 / TIME_DIVISOR, dsr),
                    average: one_pole(avg * 1.0e-3 / TIME_DIVISOR, dsr),
                    fast: per_sample(fast),
                    slow: per_sample(slow),
                    hold: (self.sv(s, ix::RELEASE_DELAY) * 1.0e-3 * dsr).round() as u32,
                    switch: STAGE_SWITCH,
                },
            };
        }
        let mode = self.choice(ix::CHANNEL_MODE);
        let g = self.choice(ix::GANGED_SET).min(1);
        self.split_on = if mode == 0 {
            self.set[g].ftype != FT_FULL
        } else {
            self.set[0].ftype != FT_FULL || self.set[1].ftype != FT_FULL
        };
        if self.taps_dirty && self.ready {
            for s in 0..2 {
                let (ftype, fc, bw, d) = (self.set[s].ftype, self.sv(s, ix::CENTER_FREQ), self.sv(s, ix::BANDWIDTH), self.d_split);
                self.half[s] = make_taps(&mut self.taps[s], d, sr, ftype, fc, bw);
            }
            self.taps_dirty = false;
        }
    }

    fn snap(&mut self) {
        self.gin = self.gin_t;
        self.gout = self.gout_t;
        self.glim = self.glim_t;
        self.mix = self.mix_t;
        self.mk = [self.set[0].makeup, self.set[1].makeup];
    }

    fn limiter_delay(&self) -> usize {
        if self.on(ix::LIMITER_ON) {
            self.lim.delay()
        } else {
            0
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        if !self.ready {
            return;
        }
        let n = left.len().min(right.len());
        let mode = self.choice(ix::CHANNEL_MODE);
        let ganged = mode == 0;
        let ms = mode == 2;
        let gset = self.choice(ix::GANGED_SET).min(1);
        let sidx = if ganged { [gset, gset] } else { [0, 1] };
        let ext = self.on(ix::KEY_SOURCE);
        let link = self.on(ix::KEY_LINK);
        let key_filter = self.on(ix::KEY_FILTER);
        let solo = self.on(ix::BAND_SOLO);
        let par = self.choice(ix::PARALLEL_MODE);
        let peak_mode = self.choice(ix::DETECTOR) == 0;
        let lim_on = self.on(ix::LIMITER_ON);
        let true_peak = self.choice(ix::LIMITER_TYPE) >= 2;
        let d = self.d_split;
        let hold4 = self.hold4;
        let adelay = TP_DELAY + self.la;
        let ksm = self.ksm;
        let mut deepest = [0.0f64; 2];
        let mut lim_min = 1.0f32;
        let mut det_max = [0.0f64; 2];
        let mut gain_min = [f64::INFINITY; 2];

        for i in 0..n {
            self.gin += (self.gin_t - self.gin) * ksm;
            self.gout += (self.gout_t - self.gout) * ksm;
            self.glim += (self.glim_t - self.glim) * ksm;
            self.mix += (self.mix_t - self.mix) * ksm;
            self.mk[0] += (self.set[0].makeup - self.mk[0]) * ksm;
            self.mk[1] += (self.set[1].makeup - self.mk[1]) * ksm;

            let mut x = [clean(left[i]) * self.gin, clean(right[i]) * self.gin];
            let mut k = match key {
                Some((kl, kr)) if ext => [
                    clean(kl.get(i).copied().unwrap_or(0.0)),
                    clean(kr.get(i).copied().unwrap_or(0.0)),
                ],
                _ => [0.0, 0.0],
            };
            if ms {
                x = [0.5 * (x[0] + x[1]), 0.5 * (x[0] - x[1])];
                k = [0.5 * (k[0] + k[1]), 0.5 * (k[0] - k[1])];
            }

            let mut band = [0.0f32; 2];
            let mut dry = [0.0f32; 2];
            let mut level = [[0.0f32; OVERSAMPLE]; 2];
            for c in 0..2 {
                let s = sidx[c];
                let filtered = self.set[s].ftype != FT_FULL;
                let det;
                if self.split_on {
                    self.split[c].push(x[c]);
                    dry[c] = self.split[c].delayed(d);
                    band[c] = if filtered { self.split[c].fir(&self.taps[s], d, self.half[s]) } else { dry[c] };
                    if ext {
                        self.key_split[c].push(k[c]);
                        det = if filtered && key_filter {
                            self.key_split[c].fir(&self.taps[s], d, self.half[s])
                        } else {
                            self.key_split[c].delayed(d)
                        };
                    } else {
                        det = band[c];
                    }
                } else {
                    dry[c] = x[c];
                    band[c] = x[c];
                    det = if ext { k[c] } else { x[c] };
                }
                let (_, mut mags) = self.tp[c].push(det);
                if !peak_mode {
                    // True RMS of the reconstructed signal: one pole on the square (70.1).
                    let p = &mut self.pw[c];
                    for m in mags.iter_mut() {
                        *p += ((*m as f64) * (*m as f64) - *p) * self.krms;
                        *m = (p.sqrt() * RMS_TRIM) as f32;
                    }
                    if *p < 1.0e-30 {
                        *p = 0.0;
                    }
                }
                level[c] = mags;
            }

            let mut g_db = [0.0f64; 2];
            if ganged {
                let mut p = 0.0;
                for j in 0..OVERSAMPLE {
                    let held = self.smax[0].push(level[0][j].max(level[1][j]), hold4);
                    p = self.fol[0].step(held as f64, &self.set[gset].timing);
                }
                let g = self.set[gset].gain_db(p);
                g_db = [g, g];
                det_max = [det_max[0].max(p), det_max[1].max(p)];
            } else {
                for c in 0..2 {
                    let mut p = 0.0;
                    for j in 0..OVERSAMPLE {
                        let held = self.smax[c].push(level[c][j], hold4);
                        p = self.fol[c].step(held as f64, &self.set[c].timing);
                    }
                    g_db[c] = self.set[c].gain_db(p);
                    det_max[c] = det_max[c].max(p);
                }
                if link {
                    let m = g_db[0].min(g_db[1]);
                    g_db = [m, m];
                }
            }

            let mut y = [0.0f32; 2];
            for c in 0..2 {
                if -g_db[c] > deepest[c] {
                    deepest[c] = -g_db[c];
                }
                gain_min[c] = gain_min[c].min(g_db[c]);
                let g = db_to_lin(g_db[c]) as f32 * self.mk[sidx[c]];
                let b = self.dl_band[c].push(band[c], adelay);
                let dr = self.dl_dry[c].push(dry[c], adelay);
                y[c] = if solo {
                    // Band solo: the processed band alone, after make-up (sections 67, 67.1).
                    b * g
                } else {
                    let wet = b * g + (dr - b);
                    match par {
                        // Sum: dry at unity plus the compressed signal (section 69).
                        1 => dr + wet,
                        // Blend: 0 = dry, 100 = compressed (the sibling unit's mix control).
                        2 => dr + (wet - dr) * self.mix,
                        _ => wet,
                    }
                };
            }
            if ms {
                y = [y[0] + y[1], y[0] - y[1]];
            }
            y[0] *= self.glim;
            y[1] *= self.glim;
            if lim_on {
                let (l, r, g) = self.lim.process(y[0], y[1], self.ceiling, true_peak);
                y = [l, r];
                if g < lim_min {
                    lim_min = g;
                }
            }
            // Output gain comes after the limiter and is never limited (section 57).
            left[i] = y[0] * self.gout;
            right[i] = y[1] * self.gout;
        }

        let mut m = Meters::default();
        m.band_reduction_db[0] = deepest[0] as f32;
        m.band_reduction_db[1] = deepest[1] as f32;
        m.band_reduction_db[2] = (-20.0 * (lim_min.max(1.0e-9) as f64).log10()).max(0.0) as f32;
        m.reduction_db = m.band_reduction_db[0].max(m.band_reduction_db[1]);
        self.meters = m;
        if n > 0 {
            for c in 0..2 {
                self.view[view::DETECTOR + c] = (20.0 * det_max[c].max(1.0e-6).log10()) as f32;
                self.view[view::GAIN + c] = gain_min[c] as f32;
            }
            self.view[view::LIMIT] = m.band_reduction_db[2];
            self.view[view::MAKEUP] = self.set[0].makeup_db;
            self.view[view::MAKEUP + 1] = self.set[1].makeup_db;
            self.view[view::KEYED] = if ext && key.is_some() { 1.0 } else { 0.0 };
        }
    }
}

/// Writes the linear-phase band kernel for one set and returns its half-length in samples.
///
/// Each band edge is an ideal low-pass at the edge frequency times a Gaussian window, which
/// gives an error-function shaped transition of the width `edge_width_hz` asks for. Low band =
/// that low-pass, high band = its complement, mid band = the difference of two.
fn make_taps(taps: &mut [f32], d: usize, sr: f64, ftype: u8, fc: f64, bw_oct: f64) -> usize {
    for t in taps.iter_mut() {
        *t = 0.0;
    }
    if taps.len() < 2 * d + 1 {
        return 0;
    }
    let top = 0.45 * sr;
    // The half-amplitude point of every edge sits 2 % under the displayed frequency.
    let fc = fc * EDGE_SHIFT;
    match ftype {
        FT_LOW => add_low_pass(taps, d, sr, fc.min(top), 1.0),
        FT_HIGH => {
            let h = add_low_pass(taps, d, sr, fc.min(top), -1.0);
            taps[d] += 1.0;
            h
        }
        FT_MID => {
            // The lower edge does not go below the bottom of the frequency range (the 8-octave
            // row of `filter_shape.json` has its lower edge near 41 Hz, not at 26 Hz).
            let lo = (fc * 2f64.powf(-0.5 * bw_oct)).max(41.0 * EDGE_SHIFT);
            let hi = (fc * 2f64.powf(0.5 * bw_oct)).min(top);
            let a = add_low_pass(taps, d, sr, hi, 1.0);
            let b = add_low_pass(taps, d, sr, lo.min(hi), -1.0);
            a.max(b)
        }
        _ => 0,
    }
}

fn add_low_pass(taps: &mut [f32], d: usize, sr: f64, f0: f64, sign: f64) -> usize {
    // 0.5 * erfc((f - f0) / w): the Gaussian window has sigma_t = sqrt(2) / (2 pi w) seconds.
    let sigma = std::f64::consts::SQRT_2 / (2.0 * std::f64::consts::PI * edge_width_hz(f0)) * sr;
    let half = ((KERNEL_SIGMAS * sigma).ceil() as usize).min(d);
    let a = 2.0 * f0 / sr;
    for k in 0..=half {
        let t = k as f64;
        let s = if k == 0 { 1.0 } else { (std::f64::consts::PI * a * t).sin() / (std::f64::consts::PI * a * t) };
        let h = (sign * a * s * (-0.5 * (t / sigma) * (t / sigma)).exp()) as f32;
        taps[d + k] += h;
        if k > 0 {
            taps[d - k] += h;
        }
    }
    half
}

impl Module for CompDual {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 8_000.0 { sample_rate as f64 } else { 48_000.0 };
        self.sr = sr;
        // Split filter: long enough for the narrowest edge (30 Hz wide).
        let sigma = std::f64::consts::SQRT_2 / (2.0 * std::f64::consts::PI * MIN_EDGE_WIDTH_HZ) * sr;
        self.d_split = (KERNEL_SIGMAS * sigma).ceil() as usize;
        let ring = (2 * self.d_split + 2).next_power_of_two();
        for c in 0..2 {
            self.taps[c] = vec![0.0; 2 * self.d_split + 1];
            self.split[c].alloc(ring);
            self.key_split[c].alloc(ring);
        }
        self.la_max = (PARAMS[ix::LOOKAHEAD].max as f64 * 1.0e-3 * sr).ceil() as usize + 1;
        let line = (TP_DELAY + self.la_max + 2).next_power_of_two();
        for c in 0..2 {
            self.smax[c].alloc((self.la_max * OVERSAMPLE + 4).next_power_of_two());
            self.dl_band[c].alloc(line);
            self.dl_dry[c].alloc(line);
        }
        let w = ((LIMITER_LOOKAHEAD_MS * 1.0e-3 * sr).round() as usize).max(2);
        self.lim.alloc(w, one_pole(LIMITER_RELEASE_MS * 1.0e-3, sr));
        self.ready = true;
        self.taps_dirty = true;
        self.update();
        self.reset();
    }

    fn reset(&mut self) {
        for c in 0..2 {
            self.split[c].clear();
            self.key_split[c].clear();
            self.tp[c].clear();
            self.smax[c].clear();
            self.fol[c] = Follower::default();
            self.dl_band[c].clear();
            self.dl_dry[c].clear();
            self.pw[c] = 0.0;
        }
        self.lim.clear();
        self.meters = Meters::default();
        self.view = [0.0; view::LEN];
        self.view[view::DETECTOR] = VIEW_FLOOR_DB;
        self.view[view::DETECTOR + 1] = VIEW_FLOOR_DB;
        self.snap();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        if value.is_nan() {
            return;
        }
        let mut v = value.clamp(p.min, p.max);
        if p.steps > 0 {
            v = v.round();
        }
        if self.v[index] == v {
            return;
        }
        self.v[index] = v;
        if index >= ix::SET_BASE {
            let off = (index - ix::SET_BASE) % ix::SET_LEN;
            if off <= ix::CENTER_FREQ {
                self.taps_dirty = true;
            }
        }
        self.update();
    }

    /// Band split (0, or 26.3 ms when any active set has a band selected) + 8 samples of
    /// true-peak reconstruction + the look-ahead + the limiter (8 + 1.5 ms - 1 sample, when on).
    /// 87 samples at 48 kHz with the defaults. The source's fixed 8097 samples are not copied.
    fn latency(&self) -> usize {
        let split = if self.split_on { self.d_split } else { 0 };
        split + TP_DELAY + self.la + self.limiter_delay()
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.run(left, right, None);
    }

    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        self.run(left, right, Some((key_left, key_right)));
    }

    fn meters(&self) -> Meters {
        self.meters
    }

    /// The read-out of the last block, laid out as [`view`] says.
    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&self.view[..n]);
        n
    }
}

/// The lowest detector level the read-out reports, dBFS.
pub const VIEW_FLOOR_DB: f32 = -120.0;

/// Slot layout of [`Module::view`], of the block just processed. Channel 1 and 2 are left and
/// right, or mid and side; in Ganged both show the same.
pub mod view {
    /// Detector level per channel after the follower, dBFS, the highest in the block.
    pub const DETECTOR: usize = 0;
    /// Signed compressor gain per channel without make-up, dB, the lowest in the block
    /// (negative reduces, positive is upward).
    pub const GAIN: usize = 2;
    /// Safety-limiter reduction, dB, positive.
    pub const LIMIT: usize = 4;
    /// Make-up per set (1, 2), dB, auto make-up included.
    pub const MAKEUP: usize = 5;
    /// 1 when the detector ran from the key in this block, else 0.
    pub const KEYED: usize = 7;
    pub const LEN: usize = 8;
}
