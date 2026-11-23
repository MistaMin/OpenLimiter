//! Five-band compressor with an optional per-band peak limiter.
//!
//! Built from measured behaviour only: MASTER_BRIEF.md section 4.2 (source F) and the source F
//! table file in `spec/params/` (sections 3, 4 and 5). Section numbers in the comments below
//! refer to that table file unless they say "brief".
//!
//! Signal path per channel:
//!
//! ```text
//! in -> crossover (4 first-order splits, tree) -> 5 bands
//!          each band: detector (undelayed) -> gain computer -> hold -> slew -> smoother
//!                     audio delay (15 ms, fixed) -> band gain -> [band limiter, own addition]
//!       -> sum -> out
//! ```
//!
//! What the measurements pin down, and how it is built here:
//!
//! * Crossover (section 5): a tree of first-order low-pass / high-pass pairs made with the
//!   bilinear transform and frequency pre-warping. `Lo = LP1`, `LoMid = HP1*LP2`,
//!   `Mid = HP1*HP2*LP3`, `HiMid = HP1*HP2*HP3*LP4`, `Hi = HP1*HP2*HP3*HP4`. This reproduces the
//!   measured complex band responses to 1e-6 and sums to exactly one.
//! * Static curve (section 4.1): inside one band the knee is hard and the ratio is the nominal
//!   one. The "soft, wide knee" and the "infinite ratio only reaches 2.9:1" of the table are what
//!   the broad band overlap makes of it once the bands are summed (a 4 kHz tone sits in three
//!   bands). Nothing is tabulated; the table falls out of the structure.
//! * Detector (sections 4.1, 4.6): mean square of the band signal (two poles), calibrated to the
//!   sine peak, with a faster path 3 dB under it for sudden steps and, for the crest settings, a
//!   peak path under it by the crest value. The detector input carries a small weighting.
//! * Timing (section 4.3): the gain moves at a constant rate in dB, 10 dB per attack time and
//!   9.75 dB per release time, with a hold of 20.5 ms before the release starts.
//! * Look-ahead (sections 4.3, 4.6): the audio delay is fixed; the look-ahead setting moves the
//!   gain curve earlier against the audio (by at most 3.3 attack times).
//! * Modes (sections 4.4, 4.5): downward, parallel (`0.5 * (dry + wet * compressed)`), parallel
//!   with one detector per channel, and a low-level lift.
//!
//! The per-band limiter is this project's own addition and is not derived from any measurement.

use olm_api::{Kind, Meters, Module, ParamDesc};

pub const BANDS: usize = 5;

// ---------------------------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------------------------

pub const P_MODE: usize = 0;
pub const P_REFERENCE: usize = 1;
pub const P_LOOKAHEAD: usize = 2;
pub const P_DETECTOR: usize = 3;
pub const P_WET_GAIN: usize = 4;
pub const P_FLOOR_GUARD_THRESHOLD: usize = 5;
pub const P_FLOOR_GUARD_RATIO: usize = 6;
pub const P_SPLIT_1: usize = 7;
pub const P_SPLIT_2: usize = 8;
pub const P_SPLIT_3: usize = 9;
pub const P_SPLIT_4: usize = 10;
pub const P_BAND_LIMITER: usize = 11;
pub const P_BAND_LIMITER_RELEASE: usize = 12;
/// First per-band parameter. Band `b` (0 = lowest) uses `P_BAND_BASE + b * PER_BAND + offset`.
pub const P_BAND_BASE: usize = 13;
pub const PER_BAND: usize = 9;
pub const B_THRESHOLD: usize = 0;
pub const B_RATIO: usize = 1;
pub const B_ATTACK: usize = 2;
pub const B_RELEASE: usize = 3;
pub const B_MAKEUP: usize = 4;
pub const B_LIFT: usize = 5;
pub const B_SOLO: usize = 6;
pub const B_BYPASS: usize = 7;
pub const B_CEILING: usize = 8;
/// Where the detectors listen when the host feeds a key: 0 = the module's own input, 1 = the
/// key. Appended after the band parameters so no earlier index moves.
pub const P_KEY_SOURCE: usize = P_BAND_BASE + BANDS * PER_BAND;
pub const PARAM_COUNT: usize = P_KEY_SOURCE + 1;

/// Index of a per-band parameter.
pub const fn band_param(band: usize, offset: usize) -> usize {
    P_BAND_BASE + band * PER_BAND + offset
}

pub const MODE_DOWNWARD: f32 = 0.0;
pub const MODE_PARALLEL: f32 = 1.0;
pub const MODE_PARALLEL_UNLINKED: f32 = 2.0;
pub const MODE_LIFT: f32 = 3.0;

/// A ratio at or above this value is treated as infinite.
pub const RATIO_INFINITE: f32 = 100.0;
/// A wet gain at or below this value silences the compressed path (section 4.4, "Off").
pub const WET_OFF_DB: f32 = -40.0;
/// The two lower splits are switched off at or below this frequency: the band below the split
/// is then empty and its content passes to the band above (section 5).
pub const SPLIT_LOW_OFF_HZ: f32 = 20.0;
/// The two upper splits are switched off at or above this frequency: the band above the split
/// is then empty and its content stays in the band below (section 5).
pub const SPLIT_HIGH_OFF_HZ: f32 = 20000.0;
/// The audio delay, fixed, equal to the longest look-ahead.
pub const MAX_LOOKAHEAD_MS: f32 = 15.0;

static MODE_LABELS: [&str; 4] = ["Downward", "Parallel", "Parallel Unlinked", "Low-Level Lift"];
static KEY_SOURCE_LABELS: [&str; 2] = ["Internal", "External"];
static DETECTOR_LABELS: [&str; 9] =
    ["Peak", "Crest 6 dB", "Crest 10 dB", "Crest 12 dB", "Crest 14 dB", "Crest 16 dB", "Crest 20 dB", "Crest 24 dB", "RMS"];

macro_rules! band_params {
    ($id:literal, $name:literal, $thr:expr, $att:expr, $rel:expr) => {
        [
            ParamDesc::new(concat!($id, "Threshold"), concat!($name, " Threshold"), -25.0, 20.0, $thr, Kind::Db),
            ParamDesc::new(concat!($id, "Ratio"), concat!($name, " Ratio"), 1.0, RATIO_INFINITE, 2.0, Kind::Ratio).log(),
            ParamDesc::new(concat!($id, "Attack"), concat!($name, " Attack"), 0.3, 350.0, $att, Kind::Ms).log(),
            ParamDesc::new(concat!($id, "Release"), concat!($name, " Release"), 20.0, 7000.0, $rel, Kind::Ms).log(),
            ParamDesc::new(concat!($id, "Makeup"), concat!($name, " Make-Up"), -24.0, 12.0, 0.0, Kind::Db),
            ParamDesc::new(concat!($id, "Lift"), concat!($name, " Lift Amount"), 0.0, 50.0, 35.0, Kind::Percent),
            ParamDesc::toggle(concat!($id, "Solo"), concat!($name, " Solo"), false),
            ParamDesc::toggle(concat!($id, "Bypass"), concat!($name, " Bypass"), false),
            ParamDesc::new(concat!($id, "Ceiling"), concat!($name, " Limiter Ceiling"), -24.0, 0.0, -3.0, Kind::Db),
        ]
    };
}

const GLOBAL_PARAMS: [ParamDesc; P_BAND_BASE] = [
    ParamDesc::choice("mode", "Mode", &MODE_LABELS, 0),
    ParamDesc::new("referenceLevel", "Reference Level", -24.0, 0.0, -8.0, Kind::Db),
    ParamDesc::new("lookahead", "Look-Ahead", 0.0, MAX_LOOKAHEAD_MS, 10.0, Kind::Ms),
    ParamDesc::choice("detector", "Detector", &DETECTOR_LABELS, 8),
    ParamDesc::new("wetGain", "Parallel Wet Gain", WET_OFF_DB, 6.0, 0.0, Kind::Db),
    ParamDesc::new("floorGuardThreshold", "Floor Guard Threshold", -50.0, -3.0, -30.0, Kind::Db),
    ParamDesc::new("floorGuardRatio", "Floor Guard Ratio", 1.0, RATIO_INFINITE, 1.0, Kind::Ratio).log(),
    ParamDesc::new("split1", "Split 1 (Low | Low-Mid)", SPLIT_LOW_OFF_HZ, 4000.0, 125.0, Kind::Hz).log(),
    ParamDesc::new("split2", "Split 2 (Low-Mid | Mid)", SPLIT_LOW_OFF_HZ, 4000.0, 500.0, Kind::Hz).log(),
    ParamDesc::new("split3", "Split 3 (Mid | High-Mid)", 100.0, SPLIT_HIGH_OFF_HZ, 2000.0, Kind::Hz).log(),
    ParamDesc::new("split4", "Split 4 (High-Mid | High)", 100.0, SPLIT_HIGH_OFF_HZ, 8000.0, Kind::Hz).log(),
    ParamDesc::toggle("bandLimiter", "Band Limiter", false),
    ParamDesc::new("bandLimiterRelease", "Band Limiter Release", 10.0, 1000.0, 100.0, Kind::Ms).log(),
];

// Defaults per band: section 2 (indices 82 to 131) for the four bands the original exposes, and
// section 3 (the values its editor held) for the middle band.
const BAND_PARAMS: [[ParamDesc; PER_BAND]; BANDS] = [
    band_params!("low", "Low", -7.0, 50.0, 300.0),
    band_params!("lowMid", "Low-Mid", -7.0, 40.0, 250.0),
    band_params!("mid", "Mid", -8.0, 35.0, 200.0),
    band_params!("highMid", "High-Mid", -9.0, 30.0, 175.0),
    band_params!("high", "High", -10.0, 25.0, 150.0),
];

const fn build_params() -> [ParamDesc; PARAM_COUNT] {
    let mut out = [GLOBAL_PARAMS[0]; PARAM_COUNT];
    let mut i = 0;
    while i < P_BAND_BASE {
        out[i] = GLOBAL_PARAMS[i];
        i += 1;
    }
    let mut b = 0;
    while b < BANDS {
        let mut k = 0;
        while k < PER_BAND {
            out[P_BAND_BASE + b * PER_BAND + k] = BAND_PARAMS[b][k];
            k += 1;
        }
        b += 1;
    }
    out[P_KEY_SOURCE] = ParamDesc::choice("keySource", "Key Source", &KEY_SOURCE_LABELS, 0);
    out
}

static PARAMS: [ParamDesc; PARAM_COUNT] = build_params();

// ---------------------------------------------------------------------------------------------
// Constants fitted to the measurements
// ---------------------------------------------------------------------------------------------

/// The gain moves 10 dB in one attack time and 9.75 dB in one release time (section 4.3: release
/// 20 ms / 175 ms / 374 ms / 1.62 s give 63 % after 28 / 141 / 285 / 1180 ms from about 11 dB of
/// band reduction; attack 350 ms reaches about 5.8 dB of band reduction in 210 ms).
const ATTACK_DB_PER_TIME: f32 = 10.0;
const RELEASE_DB_PER_TIME: f32 = 9.75;
/// The reduction is held this long before the release starts (section 4.3 table "Lookahead and
/// attack overshoot" with look-ahead 0: release 63 % after 151 ms at release 175 ms, of which
/// the ramp itself takes about 126 ms; release 20 ms: 63 % after 28 ms with 10 ms look-ahead).
const HOLD_MS: f32 = 20.5;
/// Look-ahead (sections 4.3, 4.6): the whole gain curve, attack and release, is moved earlier by
/// the look-ahead time, but by no more than this many attack times. Round 5 data: with attack
/// 30 ms the release moves 10 ms and 15 ms earlier for 10 and 15 ms of look-ahead; with attack
/// 0.3 ms it moves 1.0 ms earlier for every look-ahead from 1 to 15 ms.
const ADVANCE_ATTACK_TIMES: f32 = 10.0 / 3.0;
/// Time constant of each of the two cascaded one-poles that average the squared signal. A
/// compromise between the measured burst reaction, a reading without ripple on low tones and
/// the pink-noise result (REPORT.md, open questions).
const RMS_TIME_MS: f32 = 2.0;
/// `RMS_TIME_MS` holds for a band whose lower edge is at this frequency; other bands scale it
/// with the edge ratio to the power `RMS_EDGE_POWER` (lower bands average longer). The edge of
/// the lowest band is taken as a quarter of the first split. Round 5 "band timing":
/// with a 0.3 ms attack the low, low-mid, high-mid and high bands reach 63 % after 3.7, 2.6,
/// 0.47 and 0.02 ms.
const RMS_REFERENCE_HZ: f32 = 2000.0;
const RMS_EDGE_POWER: f32 = 0.4;
/// A second, fast average of the squared signal (one pole, `FAST_TIME_RATIO` of the slow time)
/// sits `FAST_PATH_MARGIN_DB` under the slow one. It catches a sudden level step within a
/// fraction of a millisecond; the slow average then takes over. At 3 dB it can never exceed the
/// slow average on a steady sine of any frequency, so the static curve is not touched. (Attack
/// 0.3 ms curve: fast first part, then a tail; overshoot table of section 4.3.)
const FAST_TIME_RATIO: f32 = 0.125;
const FAST_PATH_MARGIN_DB: f32 = 3.0;
/// After the constant-rate stage the gain passes a one-pole smoother whose time constant is
/// `SOFT_K * sqrt(attack in ms)` ms. Empirical: the measured ramps start and end softly and lag
/// the ideal ramp by a time that grows roughly with the square root of the attack time.
const SOFT_K: f32 = 0.77;
/// Detector weighting (inferred from section 4.1, see REPORT.md): the level every band detector
/// reads on a sine differs from the band level by an amount that depends on the tone frequency
/// and not on the band: about -0.3 dB at 200 Hz, -0.2 dB at 1 kHz, +0.7 dB at 4 kHz (the same in
/// three bands) and +0.25 dB at 12 kHz. Modelled as a trim and a bell in front of the detector
/// crossover. (The 40 Hz tables ask for a much lower reading still; a detector high-pass that
/// gives it makes the pink-noise result of section 4.4 wrong by 1.4 dB, so it is left out.)
const DETECTOR_TRIM_DB: f32 = -0.3;
const WEIGHT_BELL_HZ: f64 = 6000.0;
const WEIGHT_BELL_Q: f64 = 0.6;
const WEIGHT_BELL_DB: f64 = 1.28;
/// Low-level lift (sections 4.4, 4.5): band gain = (1 - w) + w * (level / reference)^-p above
/// the threshold, constant below it: a dry path plus a compressor of ratio 1 / (1 - p) whose
/// output is brought back to the reference level. `w` is the lift amount. The six measured
/// lift curves are each fitted to 0.01 dB by one (w, p) pair (the default one, which has three
/// curves, to 0.14 dB); the largest is w = 0.5, p = 1
/// (half dry, half an infinite-ratio compressor). The table gives p for a given w.
const LIFT_W: [f32; 6] = [0.0, 0.03, 0.19, 0.35, 0.44, 0.5];
const LIFT_P: [f32; 6] = [0.3, 0.4, 0.53, 0.725, 0.88, 1.0];
/// The floor guard takes the lift back completely this many dB under its threshold when its
/// ratio is infinite (section 4.5, direction only).
const GUARD_SPAN_DB: f32 = 10.0;
/// Look-ahead of the added band limiter.
const LIMITER_WINDOW_MS: f32 = 1.5;

const LN10_OVER_20: f32 = 0.115_129_25;
const DB_PER_LN_POWER: f32 = 4.342_944_8; // 10 / ln(10)

#[inline]
fn db_to_lin(db: f32) -> f32 {
    (db * LN10_OVER_20).exp()
}

// ---------------------------------------------------------------------------------------------
// Sliding maximum (monotonic wedge), no allocation after construction
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Wedge {
    val: Vec<f32>,
    idx: Vec<u32>,
    head: usize,
    len: usize,
    mask: usize,
}

impl Wedge {
    fn with_window(max_window: usize) -> Self {
        let cap = (max_window + 2).next_power_of_two();
        Self { val: vec![0.0; cap], idx: vec![0; cap], head: 0, len: 0, mask: cap - 1 }
    }

    fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Adds `v` at sample `i` and returns the largest value of the last `window + 1` samples.
    #[inline]
    fn push(&mut self, v: f32, i: u32, window: u32) -> f32 {
        if self.val.is_empty() {
            return v;
        }
        while self.len > 0 && self.val[(self.head + self.len - 1) & self.mask] <= v {
            self.len -= 1;
        }
        while self.len > 0 && i.wrapping_sub(self.idx[self.head]) > window {
            self.head = (self.head + 1) & self.mask;
            self.len -= 1;
        }
        if self.len > self.mask {
            self.head = (self.head + 1) & self.mask;
            self.len -= 1;
        }
        let t = (self.head + self.len) & self.mask;
        self.val[t] = v;
        self.idx[t] = i;
        self.len += 1;
        self.val[self.head]
    }
}

// ---------------------------------------------------------------------------------------------
// Crossover: four first-order splits in a tree (section 5)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Split {
    Active,
    /// The band below the split is empty.
    LowerEmpty,
    /// The band above the split (and everything above it) is empty.
    UpperEmpty,
}

#[derive(Clone, Copy)]
struct Crossover {
    /// g / (1 + g) with g = tan(pi * f / sample rate): the trapezoidal one-pole low-pass, which
    /// is the bilinear low-pass with pre-warping. High-pass = input - low-pass, exactly.
    coef: [f64; 4],
    kind: [Split; 4],
    state: [[f64; 4]; 2],
}

impl Crossover {
    fn new() -> Self {
        Self { coef: [0.0; 4], kind: [Split::Active; 4], state: [[0.0; 4]; 2] }
    }

    fn reset(&mut self) {
        self.state = [[0.0; 4]; 2];
    }

    #[inline]
    fn split(&mut self, ch: usize, x: f32) -> [f32; BANDS] {
        let mut out = [0.0f32; BANDS];
        let mut rest = x as f64;
        let st = &mut self.state[ch & 1];
        for k in 0..4 {
            let low = match self.kind[k] {
                Split::Active => {
                    let v = (rest - st[k]) * self.coef[k];
                    let y = v + st[k];
                    st[k] = y + v + 1.0e-30;
                    y
                }
                Split::LowerEmpty => 0.0,
                Split::UpperEmpty => rest,
            };
            out[k] = low as f32;
            rest -= low;
        }
        out[4] = rest as f32;
        out
    }
}

/// Detector weighting: a bell, per channel.
#[derive(Clone, Copy)]
struct Weighting {
    b: [f64; 3],
    a: [f64; 2],
    state: [[f64; 2]; 2],
}

impl Weighting {
    fn new() -> Self {
        Self { b: [1.0, 0.0, 0.0], a: [0.0; 2], state: [[0.0; 2]; 2] }
    }

    fn set(&mut self, sr: f64) {
        let amp = 10f64.powf(WEIGHT_BELL_DB / 40.0);
        let w = 2.0 * core::f64::consts::PI * WEIGHT_BELL_HZ.min(0.45 * sr) / sr;
        let alpha = w.sin() / (2.0 * WEIGHT_BELL_Q);
        let a0 = 1.0 + alpha / amp;
        self.b = [(1.0 + alpha * amp) / a0, -2.0 * w.cos() / a0, (1.0 - alpha * amp) / a0];
        self.a = [-2.0 * w.cos() / a0, (1.0 - alpha / amp) / a0];
    }

    #[inline]
    fn run(&mut self, ch: usize, x: f32) -> f32 {
        let st = &mut self.state[ch & 1];
        let h = x as f64;
        let y = self.b[0] * h + st[0];
        st[0] = self.b[1] * h - self.a[0] * y + st[1] + 1.0e-30;
        st[1] = self.b[2] * h - self.a[1] * y;
        y as f32
    }
}

// ---------------------------------------------------------------------------------------------
// One gain chain: detector state, look-ahead ramp, hold, slew, and the gain history
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Chain {
    fast_square: f32,
    mean_square_1: f32,
    mean_square: f32,
    hold: Wedge,
    depth: f32,
    smooth: f32,
    history: Vec<f32>,
}

impl Chain {
    fn clear(&mut self) {
        self.fast_square = 0.0;
        self.mean_square_1 = 0.0;
        self.mean_square = 0.0;
        self.hold.clear();
        self.depth = 0.0;
        self.smooth = 0.0;
        self.history.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// The added band limiter: linked over both channels of one band.
#[derive(Clone, Default)]
struct BandLimiter {
    env: f32,
    min: Wedge,
    box_ring: Vec<f32>,
    box_sum: f64,
    delay: [Vec<f32>; 2],
    pos: usize,
    lowest: f32,
}

impl BandLimiter {
    fn clear(&mut self) {
        self.env = 1.0;
        self.min.clear();
        self.box_ring.iter_mut().for_each(|v| *v = 1.0);
        self.box_sum = self.box_ring.len() as f64;
        self.delay[0].iter_mut().for_each(|v| *v = 0.0);
        self.delay[1].iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.lowest = 1.0;
    }
}

#[derive(Clone, Default)]
struct Band {
    chains: [Chain; 2],
    delay: [Vec<f32>; 2],
    limiter: BandLimiter,
    // Derived from parameters.
    threshold_abs: f32,
    slope: f32,
    up: f32,
    down: f32,
    advance: u32,
    soft: f32,
    rms_coef: f32,
    fast_coef: f32,
    makeup_db: f32,
    makeup_lin: f32,
    lift_w: f32,
    lift_p: f32,
    lift_rest_db: f32,
    lift_threshold_rel: f32,
    solo: bool,
    bypass: bool,
    ceiling_lin: f32,
    // Meter.
    deepest: f32,
    // Read-out for the editor (`view`), of the last block.
    view_detector: f32,
    view_output: f32,
    view_gain: f32,
    view_channel: [f32; 2],
}

/// The five-band compressor. See the crate documentation.
pub struct BandCompressor {
    values: [f32; PARAM_COUNT],
    sample_rate: f32,
    bands: [Band; BANDS],
    xover: Crossover,
    key_xover: Crossover,
    weighting: Weighting,
    // Derived, global.
    mode: u8,
    reference: f32,
    look: u32,
    max_delay: u32,
    hold: u32,
    ring_mask: u32,
    counter: u32,
    peak_only: bool,
    crest_k: f32,
    fast_k: f32,
    wet_lin: f32,
    guard_abs: f32,
    guard_slope: f32,
    any_solo: bool,
    limiter_on: bool,
    limiter_window: u32,
    limiter_release: f32,
    meters: Meters,
    /// The editor read-out of the last block (see [`view`]).
    view: [f32; view::LEN],
}

impl Default for BandCompressor {
    fn default() -> Self {
        let mut values = [0.0; PARAM_COUNT];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        let mut s = Self {
            values,
            sample_rate: 48000.0,
            bands: Default::default(),
            xover: Crossover::new(),
            key_xover: Crossover::new(),
            weighting: Weighting::new(),
            mode: 0,
            reference: -8.0,
            look: 0,
            max_delay: 0,
            hold: 0,
            ring_mask: 0,
            counter: 0,
            peak_only: false,
            crest_k: 0.0,
            fast_k: 0.5,
            wet_lin: 1.0,
            guard_abs: -38.0,
            guard_slope: 0.0,
            any_solo: false,
            limiter_on: false,
            limiter_window: 2,
            limiter_release: 0.0,
            meters: Meters::default(),
            view: [0.0; view::LEN],
        };
        s.prepare(48000.0, 512);
        s
    }
}

impl BandCompressor {
    pub fn new() -> Self {
        Self::default()
    }

    /// The value a parameter currently has (after clamping).
    pub fn param(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    fn update(&mut self) {
        let sr = self.sample_rate;
        let v = &self.values;
        self.mode = v[P_MODE].round().clamp(0.0, 3.0) as u8;
        self.reference = v[P_REFERENCE];
        self.look = ((v[P_LOOKAHEAD] * 0.001 * sr).round() as u32).min(self.max_delay);

        // Detector (section 4.6). Index 0 is peak, 8 is RMS, between them a crest limit.
        let det = v[P_DETECTOR].round().clamp(0.0, 8.0) as usize;
        const CREST_DB: [f32; 9] = [0.0, 6.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0, f32::INFINITY];
        self.peak_only = det == 0;
        // A crest limit: the peak path sits (crest - 3 dB) under the level a sine would read, so
        // it only wins on signals whose crest factor exceeds the setting. RMS: no peak path.
        self.crest_k = if det == 8 { 0.0 } else { db_to_lin(-2.0 * (CREST_DB[det] - 3.0103)) };
        self.fast_k = db_to_lin(-2.0 * FAST_PATH_MARGIN_DB);

        self.wet_lin = if v[P_WET_GAIN] <= WET_OFF_DB { 0.0 } else { db_to_lin(v[P_WET_GAIN]) };
        self.guard_abs = self.reference + v[P_FLOOR_GUARD_THRESHOLD];
        let gr = v[P_FLOOR_GUARD_RATIO];
        self.guard_slope = if gr >= RATIO_INFINITE { 1.0 } else { 1.0 - 1.0 / gr.max(1.0) };

        // Crossover.
        let freqs = [v[P_SPLIT_1], v[P_SPLIT_2], v[P_SPLIT_3], v[P_SPLIT_4]];
        for k in 0..4 {
            let kind = if k < 2 && freqs[k] <= SPLIT_LOW_OFF_HZ {
                Split::LowerEmpty
            } else if k >= 2 && freqs[k] >= SPLIT_HIGH_OFF_HZ {
                Split::UpperEmpty
            } else {
                Split::Active
            };
            let f = (freqs[k] as f64).min(0.49 * sr as f64);
            let g = (core::f64::consts::PI * f / sr as f64).tan();
            let coef = g / (1.0 + g);
            for x in [&mut self.xover, &mut self.key_xover] {
                if x.kind[k] != kind && kind != Split::Active {
                    x.state[0][k] = 0.0;
                    x.state[1][k] = 0.0;
                }
                x.kind[k] = kind;
                x.coef[k] = coef;
            }
        }

        let limiter_on = v[P_BAND_LIMITER] >= 0.5;
        let limiter_changed = limiter_on != self.limiter_on;
        self.limiter_on = limiter_on;
        self.limiter_release = 1.0 - (-1.0 / (v[P_BAND_LIMITER_RELEASE] * 0.001 * sr)).exp();

        self.any_solo = false;
        let lift = self.mode == 3;
        for b in 0..BANDS {
            let p = |o: usize| v[band_param(b, o)];
            let band = &mut self.bands[b];
            band.threshold_abs = self.reference + p(B_THRESHOLD);
            let ratio = p(B_RATIO);
            band.slope = if ratio >= RATIO_INFINITE { 1.0 } else { 1.0 - 1.0 / ratio.max(1.0) };
            let attack_ms = p(B_ATTACK).max(0.01);
            band.up = ATTACK_DB_PER_TIME / (attack_ms * 0.001 * sr);
            band.down = RELEASE_DB_PER_TIME / (p(B_RELEASE).max(0.01) * 0.001 * sr);
            band.advance = self.look.min((ADVANCE_ATTACK_TIMES * attack_ms * 0.001 * sr).round() as u32);
            // Detector times follow the lower edge of the band (see RMS_TIME_MS).
            let edge = match b {
                0 => freqs[0] * 0.25,
                k => freqs[k - 1],
            }
            .clamp(20.0, 8000.0);
            let rms_ms = RMS_TIME_MS * (RMS_REFERENCE_HZ / edge).powf(RMS_EDGE_POWER);
            band.rms_coef = 1.0 - (-1.0 / (rms_ms * 0.001 * sr)).exp();
            band.fast_coef = 1.0 - (-1.0 / (rms_ms * FAST_TIME_RATIO * 0.001 * sr)).exp();
            band.soft = 1.0 - (-1.0 / (SOFT_K * attack_ms.sqrt() * 0.001 * sr)).exp();
            band.makeup_db = p(B_MAKEUP);
            band.makeup_lin = db_to_lin(band.makeup_db);
            band.lift_w = (p(B_LIFT) * 0.01).clamp(0.0, 0.5);
            band.lift_p = lift_power(band.lift_w);
            band.lift_threshold_rel = p(B_THRESHOLD);
            band.lift_rest_db = if lift { lift_gain_db(band.lift_w, band.lift_p, band.lift_threshold_rel) } else { 0.0 };
            band.solo = p(B_SOLO) >= 0.5;
            band.bypass = p(B_BYPASS) >= 0.5;
            band.ceiling_lin = db_to_lin(p(B_CEILING));
            self.any_solo |= band.solo;
            if limiter_changed {
                band.limiter.clear();
            }
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let n = left.len().min(right.len());
        let n = match key {
            Some((kl, kr)) => n.min(kl.len()).min(kr.len()),
            None => n,
        };
        if self.ring_mask == 0 {
            return;
        }
        let mode = self.mode;
        let unlinked = mode == 2;
        let parallel = mode == 1 || mode == 2;
        let lift = mode == 3;
        let mask = self.ring_mask;
        let mut worst_limiter = [1.0f32; BANDS];
        for band in self.bands.iter_mut() {
            band.deepest = 0.0;
            band.limiter.lowest = 1.0;
            band.view_detector = VIEW_FLOOR_DB;
            band.view_output = 0.0;
            band.view_channel = [0.0; 2];
        }

        for i in 0..n {
            let xl = left[i];
            let xr = right[i];
            let al = self.xover.split(0, xl);
            let ar = self.xover.split(1, xr);
            // The detectors listen to the weighted input (or key), split by a second crossover.
            let (kl, kr) = match key {
                Some((kl, kr)) => (kl[i], kr[i]),
                None => (xl, xr),
            };
            let dl = self.key_xover.split(0, self.weighting.run(0, kl));
            let dr = self.key_xover.split(1, self.weighting.run(1, kr));
            let c = self.counter;
            let w = (c & mask) as usize;
            let r_audio = (c.wrapping_sub(self.max_delay) & mask) as usize;
            let mut out_l = 0.0f32;
            let mut out_r = 0.0f32;

            for b in 0..BANDS {
                let band = &mut self.bands[b];

                // Audio delay, fixed at the longest look-ahead.
                band.delay[0][w] = al[b];
                band.delay[1][w] = ar[b];
                let yl = band.delay[0][r_audio];
                let yr = band.delay[1][r_audio];
                // The gain that belongs to this audio sample was computed `advance` samples
                // after the sample passed the detector.
                let r_gain = (c.wrapping_sub(self.max_delay - band.advance) & mask) as usize;

                // Detector on the undelayed band signal (sections 4.1, 4.6).
                let mut level2 = [0.0f32; 2];
                for (ch, x) in [dl[b], dr[b]].into_iter().enumerate() {
                    let chain = &mut band.chains[ch];
                    let sq = x * x;
                    chain.mean_square_1 += band.rms_coef * (sq - chain.mean_square_1) + 1.0e-30;
                    chain.mean_square += band.rms_coef * (chain.mean_square_1 - chain.mean_square);
                    chain.fast_square += band.fast_coef * (sq - chain.fast_square) + 1.0e-30;
                    // 2 * mean square is the squared peak of a sine. The fast path is the same
                    // quantity over a short time, taken FAST_PATH_MARGIN_DB lower.
                    level2[ch] = if self.peak_only {
                        sq
                    } else {
                        (2.0 * chain.mean_square).max(2.0 * self.fast_k * chain.fast_square).max(sq * self.crest_k)
                    };
                }
                if !unlinked {
                    let m = level2[0].max(level2[1]);
                    level2 = [m, m];
                }

                let chains = if unlinked { 2 } else { 1 };
                let mut gain = [1.0f32; 2];
                for ch in 0..chains {
                    let level_db = DB_PER_LN_POWER * (level2[ch] + 1.0e-30).ln() + DETECTOR_TRIM_DB;
                    if level_db > band.view_detector {
                        band.view_detector = level_db;
                    }
                    // Gain computer: how far under its resting gain the band should be, in dB.
                    let target = if band.bypass {
                        0.0
                    } else if lift {
                        let u = (level_db - self.reference).max(band.lift_threshold_rel);
                        let mut depth = band.lift_rest_db - lift_gain_db(band.lift_w, band.lift_p, u);
                        if self.guard_slope > 0.0 && band.lift_rest_db > 0.0 && level_db < self.guard_abs {
                            let f = (self.guard_slope * (self.guard_abs - level_db) / GUARD_SPAN_DB).min(1.0);
                            depth += band.lift_rest_db * f;
                        }
                        depth
                    } else {
                        (level_db - band.threshold_abs).max(0.0) * band.slope
                    };

                    let chain = &mut band.chains[ch];
                    // Hold, then constant-rate attack and release in dB.
                    let held = chain.hold.push(target, c, self.hold);
                    if held > chain.depth {
                        chain.depth = (chain.depth + band.up).min(held);
                    } else {
                        chain.depth = (chain.depth - band.down).max(held);
                    }
                    chain.smooth += band.soft * (chain.depth - chain.smooth);
                    chain.history[w] = chain.smooth;
                    let depth = chain.history[r_gain];
                    if depth > band.deepest {
                        band.deepest = depth;
                    }
                    if depth > band.view_channel[ch] {
                        band.view_channel[ch] = depth;
                    }
                    band.view_gain = band.lift_rest_db - depth;
                    gain[ch] = if band.bypass {
                        1.0
                    } else if parallel {
                        // Section 4.4: out = 0.5 * (dry + wet * compressed).
                        0.5 * (1.0 + self.wet_lin * band.makeup_lin * db_to_lin(-depth))
                    } else {
                        db_to_lin(band.lift_rest_db - depth + band.makeup_db)
                    };
                }
                if !unlinked {
                    gain[1] = gain[0];
                }
                if self.any_solo && !band.solo {
                    gain = [0.0, 0.0];
                }

                let mut bl = yl * gain[0];
                let mut br = yr * gain[1];

                if self.limiter_on {
                    // Own addition: peak limiter on the band, linked over both channels.
                    let lim = &mut band.limiter;
                    let window = self.limiter_window;
                    let peak = bl.abs().max(br.abs());
                    let need = if peak > band.ceiling_lin { band.ceiling_lin / peak } else { 1.0 };
                    lim.env = need.min(lim.env + (1.0 - lim.env) * self.limiter_release);
                    let neg = lim.min.push(-lim.env, c, window - 1);
                    let floor = -neg;
                    let len = lim.box_ring.len();
                    if len > 0 {
                        let p = lim.pos % len;
                        lim.box_sum += floor as f64 - lim.box_ring[p] as f64;
                        lim.box_ring[p] = floor;
                        let g = ((lim.box_sum / len as f64) as f32).min(1.0);
                        let dlen = lim.delay[0].len();
                        if dlen > 0 {
                            let q = lim.pos % dlen;
                            let ol = lim.delay[0][q];
                            let or = lim.delay[1][q];
                            lim.delay[0][q] = bl;
                            lim.delay[1][q] = br;
                            bl = (ol * g).clamp(-band.ceiling_lin, band.ceiling_lin);
                            br = (or * g).clamp(-band.ceiling_lin, band.ceiling_lin);
                        } else {
                            bl *= g;
                            br *= g;
                        }
                        lim.pos = (lim.pos + 1) % (len * dlen.max(1));
                        if g < worst_limiter[b] {
                            worst_limiter[b] = g;
                        }
                    }
                }

                let peak = bl.abs().max(br.abs());
                if peak > band.view_output {
                    band.view_output = peak;
                }
                out_l += bl;
                out_r += br;
            }

            left[i] = out_l;
            right[i] = out_r;
            self.counter = c.wrapping_add(1);
        }

        let mut meters = Meters::default();
        for b in 0..BANDS {
            let mut r = self.bands[b].deepest;
            if self.limiter_on && worst_limiter[b] < 1.0 {
                r += -20.0 * worst_limiter[b].max(1.0e-6).log10();
            }
            meters.band_reduction_db[b] = r;
            if r > meters.reduction_db {
                meters.reduction_db = r;
            }
            let band = &self.bands[b];
            let limit = if self.limiter_on && worst_limiter[b] < 1.0 { -20.0 * worst_limiter[b].max(1.0e-6).log10() } else { 0.0 };
            let channel = if unlinked { band.view_channel } else { [band.view_channel[0]; 2] };
            self.view[view::GR + b] = band.deepest;
            self.view[view::LIMIT + b] = limit;
            self.view[view::DETECTOR + b] = band.view_detector.max(VIEW_FLOOR_DB);
            self.view[view::OUTPUT + b] = (20.0 * band.view_output.max(1.0e-6).log10()).max(VIEW_FLOOR_DB);
            self.view[view::GAIN + b] = if band.bypass { 0.0 } else { band.view_gain };
            self.view[view::GR_CHANNEL + 2 * b] = channel[0];
            self.view[view::GR_CHANNEL + 2 * b + 1] = channel[1];
        }
        self.view[view::KEYED] = if key.is_some() { 1.0 } else { 0.0 };
        self.meters = meters;
    }
}

/// Gain of the low-level lift in dB at `u` dB relative to the reference level (section 4.4).
#[inline]
fn lift_gain_db(w: f32, p: f32, u: f32) -> f32 {
    let lin = (1.0 - w) + w * db_to_lin(-p * u);
    20.0 * lin.max(1.0e-6).log10()
}

impl Module for BandCompressor {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 8000.0 { sample_rate } else { 48000.0 };
        self.sample_rate = sr;
        self.max_delay = (MAX_LOOKAHEAD_MS * 0.001 * sr).round() as u32;
        self.hold = (HOLD_MS * 0.001 * sr).round() as u32;
        let ring = (self.max_delay as usize + 2).next_power_of_two();
        self.ring_mask = ring as u32 - 1;
        self.limiter_window = ((LIMITER_WINDOW_MS * 0.001 * sr).round() as u32).max(2);
        let window = self.limiter_window as usize;
        for band in self.bands.iter_mut() {
            for ch in 0..2 {
                band.delay[ch] = vec![0.0; ring];
                band.chains[ch].history = vec![0.0; ring];
                band.chains[ch].hold = Wedge::with_window(self.hold as usize);
                band.limiter.delay[ch] = vec![0.0; window - 1];
            }
            band.limiter.min = Wedge::with_window(window);
            band.limiter.box_ring = vec![1.0; window];
        }
        self.weighting.set(sr as f64);
        self.update();
        self.reset();
    }

    fn reset(&mut self) {
        self.xover.reset();
        self.key_xover.reset();
        self.weighting.state = [[0.0; 2]; 2];
        self.counter = 0;
        for band in self.bands.iter_mut() {
            for ch in 0..2 {
                band.delay[ch].iter_mut().for_each(|v| *v = 0.0);
                band.chains[ch].clear();
            }
            band.limiter.clear();
            band.deepest = 0.0;
        }
        self.meters = Meters::default();
        self.view = [0.0; view::LEN];
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index >= PARAM_COUNT || !value.is_finite() {
            return;
        }
        let p = &PARAMS[index];
        let mut v = value.clamp(p.min, p.max);
        if matches!(p.kind, Kind::Toggle | Kind::Choice(_)) {
            v = v.round();
        }
        if v != self.values[index] {
            self.values[index] = v;
            self.update();
        }
    }

    fn latency(&self) -> usize {
        let limiter = if self.limiter_on { self.limiter_window as usize - 1 } else { 0 };
        self.max_delay as usize + limiter
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.run(left, right, None);
    }

    /// The key is used only with Key Source set to External; otherwise this is `process`.
    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        if self.values[P_KEY_SOURCE] >= 0.5 {
            self.run(left, right, Some((key_left, key_right)));
        } else {
            self.run(left, right, None);
        }
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

/// Slot layout of [`Module::view`]: all values dB, of the block just processed.
pub mod view {
    use super::BANDS;
    /// Compressor reduction per band, positive, the deepest in the block.
    pub const GR: usize = 0;
    /// Band-limiter reduction per band, positive, the deepest in the block.
    pub const LIMIT: usize = GR + BANDS;
    /// Detector level per band, dBFS: the highest value the gain computer saw.
    pub const DETECTOR: usize = LIMIT + BANDS;
    /// Band output peak, dBFS, after its gain and limiter.
    pub const OUTPUT: usize = DETECTOR + BANDS;
    /// Signed band gain at the end of the block, without make-up (negative reduces, the lift
    /// is positive).
    pub const GAIN: usize = OUTPUT + BANDS;
    /// Reduction per band and channel, L then R; equal unless the mode is Parallel Unlinked.
    pub const GR_CHANNEL: usize = GAIN + BANDS;
    /// 1 when the block was detected from the key, else 0.
    pub const KEYED: usize = GR_CHANNEL + 2 * BANDS;
    pub const LEN: usize = KEYED + 1;
}

/// The lowest level the read-out reports, dBFS.
pub const VIEW_FLOOR_DB: f32 = -120.0;

/// Steady-state band gain in dB, without make-up and without the parallel mix, for a detector
/// level `level_rel` dB relative to the reference level. `mode` is the Mode index (0 downward,
/// 1 parallel, 2 parallel unlinked, 3 low-level lift); `threshold` is relative to the
/// reference, as the parameter is; `lift` is the Lift Amount in percent; the guard threshold
/// is relative to the reference too. The same law as the audio path, without its timing.
pub fn static_gain_db(mode: usize, level_rel: f32, threshold: f32, ratio: f32, lift: f32, guard_threshold: f32, guard_ratio: f32) -> f32 {
    if mode == 3 {
        let w = (lift * 0.01).clamp(0.0, 0.5);
        let p = lift_power(w);
        let rest = lift_gain_db(w, p, threshold);
        let u = level_rel.max(threshold);
        let mut depth = rest - lift_gain_db(w, p, u);
        let guard_slope = if guard_ratio >= RATIO_INFINITE { 1.0 } else { 1.0 - 1.0 / guard_ratio.max(1.0) };
        if guard_slope > 0.0 && rest > 0.0 && level_rel < guard_threshold {
            depth += rest * (guard_slope * (guard_threshold - level_rel) / GUARD_SPAN_DB).min(1.0);
        }
        rest - depth
    } else {
        let slope = if ratio >= RATIO_INFINITE { 1.0 } else { 1.0 - 1.0 / ratio.max(1.0) };
        -(level_rel - threshold).max(0.0) * slope
    }
}

/// Magnitude in dB of band `band` (0 = lowest) of the crossover tree at `f` Hz, for the four
/// split frequencies and the sample rate, with the same off rules as the audio path.
pub fn band_response_db(splits: [f32; 4], band: usize, f: f32, sample_rate: f32) -> f32 {
    let (re, im) = band_response(splits, band, f, sample_rate);
    10.0 * (re * re + im * im).max(1.0e-24).log10() as f32
}

/// The complex response of one band of the crossover tree (see [`band_response_db`]).
pub fn band_response(splits: [f32; 4], band: usize, f: f32, sample_rate: f32) -> (f64, f64) {
    let sr = sample_rate.max(8000.0) as f64;
    let w = (core::f64::consts::PI * (f as f64).clamp(0.0, 0.499 * sr) / sr).tan();
    let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let mut rest = (1.0, 0.0);
    for (k, &fc) in splits.iter().enumerate() {
        // The trapezoidal one-pole is the bilinear one: LP = g / (g + jw), HP = jw / (g + jw).
        let (low, high) = if k < 2 && fc <= SPLIT_LOW_OFF_HZ {
            ((0.0, 0.0), (1.0, 0.0))
        } else if k >= 2 && fc >= SPLIT_HIGH_OFF_HZ {
            ((1.0, 0.0), (0.0, 0.0))
        } else {
            let g = (core::f64::consts::PI * (fc as f64).min(0.49 * sr) / sr).tan();
            let d = g * g + w * w;
            ((g * g / d, -g * w / d), (w * w / d, g * w / d))
        };
        if band == k {
            return mul(rest, low);
        }
        rest = mul(rest, high);
    }
    if band >= 4 {
        rest
    } else {
        (0.0, 0.0)
    }
}

/// The compressor exponent that belongs to a lift amount (see `LIFT_W`).
fn lift_power(w: f32) -> f32 {
    for k in 1..LIFT_W.len() {
        if w <= LIFT_W[k] {
            let t = (w - LIFT_W[k - 1]) / (LIFT_W[k] - LIFT_W[k - 1]);
            return LIFT_P[k - 1] + t * (LIFT_P[k] - LIFT_P[k - 1]);
        }
    }
    LIFT_P[LIFT_P.len() - 1]
}
