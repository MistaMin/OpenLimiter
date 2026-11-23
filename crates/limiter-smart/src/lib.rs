//! `limiter-smart`: the Smart Limiter module of OpenLimiter. Six switchable look-ahead
//! limiter engines behind one shared input matrix, one shared residual true-peak trim and
//! one gain match / delta stage, at one constant latency.
//!
//! ```text
//! in -> smoothed drive (5 ms); the ceiling glides with the same 5 ms pole
//!    -> input matrix (Stereo L/R or Mid/Side, inside each engine's detector and gain)
//!    -> active engine (1 of 6), padded to the common engine latency
//!       [20 ms sample-aligned crossfade when the engine, oversampling, crossover or
//!        channel mode changes; the new engine first runs one latency to fill its delays]
//!    -> Option B residual trim (32 samples at 48 kHz; bit-transparent while within C)
//!    -> clamp to the gliding ceiling (a safety net). The gliding ceiling travels with the
//!       audio: the engines read it as the sample enters, the trim and the clamp read the
//!       value that entered with the sample they hold, so a change never meets audio that
//!       was limited for the old value.
//!    -> gain match (K-weighted, activity-gated, <= 0 dB) / delta
//!    -> out
//! ```
//!
//! Engines (see [`engines`]): Pure (default; slow macro engine + fast peak lane, oversampled),
//! Smooth (1x B-spline, flat top, crest-adaptive dual-stage release), Precise (Smooth inside
//! the oversampler), Punch (RMS leveller, limiter, ADAA clipper), Multiband (4 bands,
//! directional Lagrange allocation, safety catcher), Glide (sliding lower convex hull).
//!
//! `prepare` allocates everything for both transition slots and all six engines; `process`,
//! `set_param`, `set_offline` and `reset` never allocate, lock or panic.

// Index loops mirror the formulas; `!(a > b)` is used on purpose so that NaN takes the safe
// branch.
#![allow(clippy::needless_range_loop, clippy::neg_cmp_op_on_partial_ord)]

pub mod dsp;
pub mod engines;
pub mod hull;
pub mod lane;
pub mod os;
pub mod stages;

use dsp::{db_to_lin, interval_peak, lin_to_db, tp_gain_bound, tp_kernels, Delay, SlidingMax, TpHist, TpKernels, TP_HALF, TP_TAPS};
use engines::{engine_latencies, lookahead_samples, Ctx, Env, Frame, Glide, Multi, Probe, Punch, Pure};
use olm_api::{Kind, Meters, Module, ParamDesc};
use stages::{GainMatch, Trim};

pub use engines::ordered_splits;
pub use lane::join_bend;
pub use stages::{
    band_allocation, clip_adaa1, clip_antiderivative, clip_curve, crest_release_factor, diamond, knee_gain_db,
};

pub const ENGINES: &[&str] = &["Pure", "Smooth", "Precise", "Punch", "Multiband", "Glide"];
const CHANNEL_MODES: &[&str] = &["Stereo", "Mid-Side"];
const CROSSOVERS: &[&str] = &["LR4", "Linear Phase"];
const SLOW_MODES: &[&str] = &["Peak Body", "Loudness Crest"];
const SLOW_WINDOWS: &[&str] = &["200 ms", "400 ms", "3 s"];
const OVERSAMPLING: &[&str] = &["4x", "8x", "16x"];
const OVERSAMPLING_OFFLINE: &[&str] = &["Same", "4x", "8x", "16x"];

/// Parameter indices, in the order of section 6 of the spec.
pub mod idx {
    pub const ENGINE: usize = 0;
    pub const DRIVE: usize = 1;
    pub const CEILING: usize = 2;
    pub const RELEASE: usize = 3;
    pub const LOOKAHEAD: usize = 4;
    pub const KNEE: usize = 5;
    pub const CHANNEL_MODE: usize = 6;
    pub const TRANSIENT_LINK: usize = 7;
    pub const RELEASE_LINK: usize = 8;
    pub const TRUE_PEAK: usize = 9;
    pub const AUTO_RELEASE: usize = 10;
    pub const GAIN_MATCH: usize = 11;
    pub const DELTA: usize = 12;
    pub const BODY_SHARE: usize = 13;
    pub const CLIP_HEADROOM: usize = 14;
    pub const CLIP_KNEE: usize = 15;
    pub const BAND_FOCUS: usize = 16;
    pub const CROSSOVER_MODE: usize = 17;
    pub const SLOW_MODE: usize = 18;
    pub const CREST_HEADROOM: usize = 19;
    pub const SLOW_WINDOW: usize = 20;
    pub const BASS_HOLD_MAX: usize = 21;
    pub const PAUSE_HOLD: usize = 22;
    pub const JOIN_WIDTH: usize = 23;
    pub const OVERSAMPLING: usize = 24;
    pub const OVERSAMPLING_OFFLINE: usize = 25;
    pub const K_WEIGHTING: usize = 26;
    pub const SPLIT1: usize = 27;
    pub const SPLIT2: usize = 28;
    pub const SPLIT3: usize = 29;
    pub const COUNT: usize = 30;
}

static PARAMS: [ParamDesc; idx::COUNT] = [
    // universal
    ParamDesc::choice("engine", "Engine", ENGINES, 0),
    ParamDesc::new("drive", "Drive", 0.0, 24.0, 0.0, Kind::Db),
    ParamDesc::new("ceiling", "Ceiling", -20.0, 0.0, -0.2, Kind::Db),
    ParamDesc::new("release", "Release", 5.0, 2000.0, 100.0, Kind::Ms).log(),
    ParamDesc::new("lookahead", "Lookahead", 0.5, 15.0, 1.5, Kind::Ms).log(),
    ParamDesc::new("knee", "Knee", 0.0, 10.0, 0.0, Kind::Db),
    ParamDesc::choice("channelMode", "Channel Mode", CHANNEL_MODES, 0),
    ParamDesc::new("transientLink", "Transient Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::new("releaseLink", "Release Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("truePeak", "True Peak", true),
    ParamDesc::toggle("autoRelease", "Auto Release", true),
    ParamDesc::toggle("gainMatch", "Gain Match", false),
    ParamDesc::toggle("delta", "Delta", false),
    // engine specific
    ParamDesc::new("bodyShare", "Body Share", 0.0, 100.0, 50.0, Kind::Percent),
    ParamDesc::new("clipHeadroom", "Clip Headroom", 0.0, 3.0, 1.0, Kind::Db),
    ParamDesc::new("clipKnee", "Clip Knee", 0.1, 1.0, 0.4, Kind::Plain),
    ParamDesc::new("bandFocus", "Band Focus", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::choice("crossoverMode", "Crossover Mode", CROSSOVERS, 0),
    ParamDesc::choice("slowMode", "Slow Mode", SLOW_MODES, 0),
    ParamDesc::new("crestHeadroom", "Crest Headroom", 4.0, 18.0, 10.0, Kind::Db),
    ParamDesc::choice("slowWindow", "Slow Window", SLOW_WINDOWS, 1),
    ParamDesc::new("bassHoldMax", "Bass Hold Max", 0.0, 20.0, 14.0, Kind::Ms),
    ParamDesc::toggle("pauseHold", "Pause Hold", true),
    ParamDesc::new("joinWidth", "Join Width", 0.0, 12.0, 6.0, Kind::Db),
    // menu / more
    ParamDesc::choice("oversampling", "Oversampling", OVERSAMPLING, 0),
    ParamDesc::choice("oversamplingOffline", "Oversampling Offline", OVERSAMPLING_OFFLINE, 3),
    ParamDesc::toggle("kWeighting", "K-Weighting", true),
    ParamDesc::new("split1", "Split 1", 40.0, 400.0, 120.0, Kind::Hz).log(),
    ParamDesc::new("split2", "Split 2", 250.0, 2000.0, 800.0, Kind::Hz).log(),
    ParamDesc::new("split3", "Split 3", 1500.0, 10000.0, 4500.0, Kind::Hz).log(),
];

/// Slot layout of [`Module::view`], for the block just processed. Gains are dB, signed
/// (0 or negative), the deepest of the block; levels are dBFS / dBTP / LUFS.
pub mod view {
    /// Total gain (engine + trim).
    pub const TOTAL_GAIN: usize = 0;
    /// Slow / body stage: Pure's slow macro gain, Smooth/Precise/Multiband slow release
    /// stage, Punch stage 1.
    pub const SLOW_GAIN: usize = 1;
    /// Fast / peak stage (Pure fast lane, Punch stage 2, Glide hull gain, fast release stage).
    pub const FAST_GAIN: usize = 2;
    /// Punch stage 3 clipper (static reduction of the deepest sub-sample).
    pub const CLIP_GAIN: usize = 3;
    /// Shared true-peak trim.
    pub const TRIM_GAIN: usize = 4;
    /// Multiband band gains 0..3 (deepest over the two gain channels).
    pub const BAND_GAIN_0: usize = 5;
    pub const BAND_GAIN_1: usize = 6;
    pub const BAND_GAIN_2: usize = 7;
    pub const BAND_GAIN_3: usize = 8;
    /// Multiband broadband safety catcher.
    pub const CATCHER_GAIN: usize = 9;
    /// Crest factor of the detector (30 ms peak over 30 ms RMS), dB.
    pub const CREST: usize = 10;
    /// Effective release time, ms (after the crest-adaptive mapping).
    pub const RELEASE_MS: usize = 11;
    /// Gain of gain channel 0 / 1: L and R in Stereo, M and S in Mid-Side.
    pub const CH0_GAIN: usize = 12;
    pub const CH1_GAIN: usize = 13;
    /// Input peaks (before drive).
    pub const IN_PEAK_L: usize = 14;
    pub const IN_PEAK_R: usize = 15;
    /// Output sample peaks.
    pub const OUT_PEAK_L: usize = 16;
    pub const OUT_PEAK_R: usize = 17;
    /// Reconstructed peak of the engine output as the trim reads it (before the trim), dBTP.
    pub const TRUE_PEAK: usize = 18;
    /// Gain-match loudness of the input and of the output, LUFS, and the applied gain.
    pub const LOUDNESS_IN: usize = 19;
    pub const LOUDNESS_OUT: usize = 20;
    pub const MATCH_GAIN: usize = 21;
    /// Active engine index (the incoming one once a crossfade is half way).
    pub const ENGINE: usize = 22;
    /// Effective oversampling factor of the active slot (1, 2, 4, 8 or 16).
    pub const OS_FACTOR: usize = 23;
    /// Pure: loudness of the slow tracker (gated, LUFS or dB RMS).
    pub const SLOW_LOUDNESS: usize = 24;
    /// Pure: 1 while Pause Hold freezes the slow gain.
    pub const PAUSE_HOLD: usize = 25;
    /// Pure: bass-adaptive post-hold, ms.
    pub const POST_HOLD_MS: usize = 26;
    /// Reconstructed (true) peak of the module output, after the trim and gain match, dBTP.
    /// Exact wherever it can reach the ceiling; while the output is provably below the
    /// ceiling (its sample bound times the reader's gain bound) it is the sample peak.
    pub const OUT_TRUE_PEAK: usize = 27;
    pub const LEN: usize = 28;
}

// ---------------------------------------------------------------------------------------------
// Pure functions for an editor

/// Output peak (dB) of the static curve for a steady input peak `input_db`: drive, then the
/// ceiling with the quadratic knee.
pub fn static_curve_db(input_db: f64, drive_db: f64, ceiling_db: f64, knee_db: f64) -> f64 {
    let x = input_db + drive_db;
    x + knee_gain_db(x - ceiling_db, knee_db)
}

/// Punch's static curve: limiter to `C + clipHeadroom`, then the quadratic-knee clipper.
pub fn punch_static_db(input_db: f64, drive_db: f64, ceiling_db: f64, knee_db: f64, clip_headroom_db: f64, clip_knee: f64) -> f64 {
    let x = input_db + drive_db;
    let lim_db = ceiling_db + clip_headroom_db;
    let y = x + knee_gain_db(x - lim_db, knee_db);
    let u = db_to_lin(y - ceiling_db);
    ceiling_db + lin_to_db(clip_curve(u, clip_knee.clamp(0.1, 1.0)))
}

/// Pure / Peak Body on a steady tone `over_db` above the ceiling: `(slow, fast)` gains (dB).
pub fn body_split_db(over_db: f64, body_share: f64) -> (f64, f64) {
    let e = over_db.max(0.0);
    let b = body_share.clamp(0.0, 1.0);
    (-b * e, -(1.0 - b) * e)
}

/// Pure / Loudness Crest: slow gain (dB) for a loudness, ceiling and crest headroom.
pub fn loudness_crest_gain_db(loudness: f64, ceiling_db: f64, crest_headroom_db: f64) -> f64 {
    knee_gain_db(loudness - (ceiling_db - crest_headroom_db), 10.0)
}

/// Effective oversampling factor for a requested factor at `fs` (768 kHz cap).
pub fn effective_oversampling(requested: usize, fs: f64) -> usize {
    os::effective_factor(requested, fs)
}

/// The module latency at `fs`, samples (the same for every setting).
pub fn module_latency(fs: f32) -> usize {
    let fs = sane_rate(fs);
    common_latency(fs) + stages::trim_delay(fs)
}

fn common_latency(fs: f64) -> usize {
    engine_latencies(fs).into_iter().max().unwrap_or(0)
}

fn sane_rate(fs: f32) -> f64 {
    if fs.is_finite() && fs >= 8000.0 {
        (fs as f64).min(768_000.0)
    } else {
        48_000.0
    }
}

// ---------------------------------------------------------------------------------------------

/// The structural setting of one slot: what a crossfade switches between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cfg {
    pub engine: usize,
    pub u: usize,
    pub lin: bool,
    pub ms: bool,
}

struct Engines {
    pure: Pure,
    smooth: Env,
    precise: Env,
    punch: Punch,
    multi: Multi,
    glide: Glide,
}

impl Engines {
    fn new() -> Self {
        Self {
            pure: Pure::default(),
            smooth: Env::new(false),
            precise: Env::new(true),
            punch: Punch::default(),
            multi: Multi::default(),
            glide: Glide::default(),
        }
    }
    fn alloc(&mut self, fs: f64) {
        self.pure.alloc(fs);
        self.smooth.alloc(fs);
        self.precise.alloc(fs);
        self.punch.alloc(fs);
        self.multi.alloc(fs);
        self.glide.alloc(fs);
    }
    fn latency(&self, e: usize) -> usize {
        match e {
            1 => self.smooth.latency(),
            2 => self.precise.latency(),
            3 => self.punch.latency(),
            4 => self.multi.latency(),
            5 => self.glide.latency(),
            _ => self.pure.latency(),
        }
    }
    fn start(&mut self, c: &Cfg) {
        match c.engine {
            1 => {
                self.smooth.configure(c.u, c.ms);
                self.smooth.reset();
            }
            2 => {
                self.precise.configure(c.u, c.ms);
                self.precise.reset();
            }
            3 => {
                self.punch.configure(c.u, c.ms);
                self.punch.reset();
            }
            4 => {
                self.multi.configure(c.lin, c.ms);
                self.multi.reset();
            }
            5 => {
                self.glide.configure(c.ms);
                self.glide.reset();
            }
            _ => {
                self.pure.configure(c.u, c.ms);
                self.pure.reset();
            }
        }
    }
    #[inline]
    fn tick(&mut self, e: usize, ctx: &Ctx, l: f64, r: f64, pr: &mut Probe) -> Frame {
        match e {
            1 => self.smooth.tick(ctx, l, r, pr),
            2 => self.precise.tick(ctx, l, r, pr),
            3 => self.punch.tick(ctx, l, r, pr),
            4 => self.multi.tick(ctx, l, r, pr),
            5 => self.glide.tick(ctx, l, r, pr),
            _ => self.pure.tick(ctx, l, r, pr),
        }
    }
}

struct Slot {
    cfg: Cfg,
    eng: Engines,
    pad: [Delay; 5],
    pad_len: usize,
    probe: Probe,
}

impl Slot {
    fn new() -> Self {
        Self {
            cfg: Cfg { engine: 0, u: 4, lin: false, ms: false },
            eng: Engines::new(),
            pad: Default::default(),
            pad_len: 0,
            probe: Probe::default(),
        }
    }
    fn alloc(&mut self, fs: f64, l_common: usize) {
        self.eng.alloc(fs);
        for p in self.pad.iter_mut() {
            p.alloc(l_common + 1);
        }
    }
    fn start(&mut self, cfg: Cfg, l_common: usize) {
        self.cfg = cfg;
        self.eng.start(&cfg);
        self.pad_len = l_common.saturating_sub(self.eng.latency(cfg.engine));
        for p in self.pad.iter_mut() {
            p.clear();
        }
        self.probe = Probe::default();
    }
    #[inline]
    fn tick(&mut self, ctx: &Ctx, l: f64, r: f64) -> Frame {
        let f = self.eng.tick(self.cfg.engine, ctx, l, r, &mut self.probe);
        let d = self.pad_len;
        Frame {
            l: self.pad[0].push(f.l, d),
            r: self.pad[1].push(f.r, d),
            rl: self.pad[2].push(f.rl, d),
            rr: self.pad[3].push(f.rr, d),
            g: self.pad[4].push(f.g, d),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    /// The incoming slot runs this many more samples before the crossfade starts.
    Warm(usize),
    /// Crossfade position.
    Fade(usize),
}

/// The Smart Limiter module.
pub struct SmartLimiter {
    values: [f32; idx::COUNT],
    fs: f64,
    prepared: bool,
    offline: bool,
    slots: [Slot; 2],
    act: usize,
    phase: Phase,
    fade_len: usize,
    l_common: usize,
    latency: usize,
    drive: f64,
    k_drive: f64,
    /// Gliding ceiling (linear) at the input, same pole as the drive.
    ceil: f64,
    /// The gliding ceiling delayed with the audio (for the trim and the output clamp).
    ceil_line: Delay,
    trim: Trim,
    gm: GainMatch,
    delta: f64,
    delta_warm: usize,
    delta_ready: bool,
    k_delta: f64,
    meter_red: f32,
    view_vals: [f32; view::LEN],
    refine_gated: bool,
    /// Read-out only: a true-peak reader on the final output (never feeds the audio).
    out_tp: OutPeak,
}

/// The output true-peak read-out (`view::OUT_TRUE_PEAK`).
struct OutPeak {
    hist: [TpHist; 2],
    kern: TpKernels,
    bound: f64,
    wmax: SlidingMax,
}

impl OutPeak {
    fn new() -> Self {
        let kern = tp_kernels();
        Self { hist: [TpHist::default(); 2], bound: tp_gain_bound(&kern), kern, wmax: SlidingMax::default() }
    }

    fn clear(&mut self) {
        for h in self.hist.iter_mut() {
            h.clear();
        }
        self.wmax.clear();
    }

    /// One output sample; returns the peak of the interval it closes.
    #[inline]
    fn push(&mut self, l: f64, r: f64, c: f64) -> f64 {
        self.hist[0].push(l);
        self.hist[1].push(r);
        let wm = self.wmax.push(l.abs().max(r.abs()), TP_TAPS);
        let read = wm * self.bound > c;
        let mut q = 0.0f64;
        for h in self.hist.iter() {
            let p = if read { interval_peak(h.window(), h.ends(), &self.kern, true) } else { h.end_peak() };
            q = q.max(p);
        }
        if q.is_finite() {
            q
        } else {
            0.0
        }
    }
}

impl Default for SmartLimiter {
    fn default() -> Self {
        let mut values = [0.0f32; idx::COUNT];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        Self {
            values,
            fs: 48_000.0,
            prepared: false,
            offline: false,
            slots: [Slot::new(), Slot::new()],
            act: 0,
            phase: Phase::Idle,
            fade_len: 960,
            l_common: 0,
            latency: module_latency(48_000.0),
            drive: 1.0,
            k_drive: 0.0,
            ceil: db_to_lin(-0.2),
            ceil_line: Delay::default(),
            trim: Trim::default(),
            gm: GainMatch::default(),
            delta: 0.0,
            delta_warm: 0,
            delta_ready: false,
            k_delta: 0.0,
            meter_red: 0.0,
            view_vals: [0.0; view::LEN],
            refine_gated: false,
            out_tp: OutPeak::new(),
        }
    }
}

impl SmartLimiter {
    /// The current plain value of parameter `index` (0 for an unknown index).
    pub fn param(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    /// Index of the parameter with this id.
    pub fn index_of(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// The structural setting the parameters ask for now.
    pub fn desired_cfg(&self) -> Cfg {
        let v = &self.values;
        let engine = (v[idx::ENGINE].round() as usize).min(5);
        let os_rt = [4usize, 8, 16][(v[idx::OVERSAMPLING].round() as usize).min(2)];
        let os_off = (v[idx::OVERSAMPLING_OFFLINE].round() as usize).min(3);
        let req = if self.offline && os_off > 0 { [4usize, 8, 16][os_off - 1] } else { os_rt };
        Cfg {
            engine,
            u: os::effective_factor(req, self.fs),
            lin: v[idx::CROSSOVER_MODE] >= 0.5,
            ms: v[idx::CHANNEL_MODE] >= 0.5,
        }
    }

    /// The structural setting of the slot that is (or is becoming) active.
    pub fn active_cfg(&self) -> Cfg {
        match self.phase {
            Phase::Idle => self.slots[self.act].cfg,
            _ => self.slots[1 - self.act].cfg,
        }
    }

    /// Measurement switch: `true` restricts the parabolic true-peak refinement to an
    /// effective oversampling of 8x and up, as the spec words it. The default (`false`)
    /// refines at every factor (see the report: without it a 4x read is up to 0.47 dB low
    /// at 20 kHz).
    #[doc(hidden)]
    pub fn set_refine_gated(&mut self, gated: bool) {
        self.refine_gated = gated;
    }

    /// True while a crossfade (or its warm-up) is running.
    pub fn in_transition(&self) -> bool {
        self.phase != Phase::Idle
    }

    fn ctx(&self) -> Ctx {
        let v = &self.values;
        let c_db = v[idx::CEILING] as f64;
        let refine = !self.refine_gated || self.active_cfg().u >= 8;
        Ctx {
            c_lin: db_to_lin(c_db),
            c_db,
            knee: v[idx::KNEE] as f64,
            release_ms: v[idx::RELEASE] as f64,
            m: lookahead_samples(v[idx::LOOKAHEAD] as f64, self.fs),
            lt: v[idx::TRANSIENT_LINK] as f64 * 0.01,
            lr: v[idx::RELEASE_LINK] as f64 * 0.01,
            tp: v[idx::TRUE_PEAK] >= 0.5,
            refine,
            auto: v[idx::AUTO_RELEASE] >= 0.5,
            body: v[idx::BODY_SHARE] as f64 * 0.01,
            clip_hr_db: v[idx::CLIP_HEADROOM] as f64,
            clip_k: v[idx::CLIP_KNEE] as f64,
            focus: v[idx::BAND_FOCUS] as f64 * 0.01,
            crest_mode: v[idx::SLOW_MODE] >= 0.5,
            crest_hr: v[idx::CREST_HEADROOM] as f64,
            slow_win_s: [0.2, 0.4, 3.0][(v[idx::SLOW_WINDOW].round() as usize).min(2)],
            bass_hold_ms: v[idx::BASS_HOLD_MAX] as f64,
            pause_hold: v[idx::PAUSE_HOLD] >= 0.5,
            join_db: v[idx::JOIN_WIDTH] as f64,
            kweight: v[idx::K_WEIGHTING] >= 0.5,
            splits: [v[idx::SPLIT1] as f64, v[idx::SPLIT2] as f64, v[idx::SPLIT3] as f64],
            need_ref: v[idx::DELTA] >= 0.5 || self.delta > 0.0,
        }
    }

    /// Starts (or retargets) a transition towards `want` if it differs.
    fn steer(&mut self, want: Cfg) {
        match self.phase {
            Phase::Idle => {
                if want != self.slots[self.act].cfg {
                    let o = 1 - self.act;
                    self.slots[o].start(want, self.l_common);
                    self.phase = Phase::Warm(self.l_common + 1);
                }
            }
            Phase::Warm(_) => {
                let o = 1 - self.act;
                if want == self.slots[self.act].cfg {
                    self.phase = Phase::Idle;
                } else if want != self.slots[o].cfg {
                    self.slots[o].start(want, self.l_common);
                    self.phase = Phase::Warm(self.l_common + 1);
                }
            }
            Phase::Fade(_) => {}
        }
    }

    /// Like `process`, also writing the engine + trim gain (dB) of each output sample into
    /// `gain` (as many as fit).
    pub fn process_traced(&mut self, left: &mut [f32], right: &mut [f32], mut gain: Option<&mut [f32]>) {
        let n = left.len().min(right.len());
        if !self.prepared || n == 0 {
            return;
        }
        let mut ctx = self.ctx();
        let want = self.desired_cfg();
        self.steer(want);
        let c_t = ctx.c_lin;
        let c_db_t = ctx.c_db;
        let tp = ctx.tp;
        let refine = ctx.refine;
        let drive_t = db_to_lin(self.values[idx::DRIVE] as f64);
        let gm_on = self.values[idx::GAIN_MATCH] >= 0.5;
        let delta_on = self.values[idx::DELTA] >= 0.5;
        self.slots[0].probe = Probe::default();
        self.slots[1].probe = Probe::default();
        self.trim.block_min_gain = 1.0;
        self.trim.block_peak = 0.0;
        let mut in_pk = [0.0f64; 2];
        let mut out_pk = [0.0f64; 2];
        let mut out_tp = 0.0f64;
        let mut deepest = 0.0f64;

        if !delta_on {
            self.delta_ready = false;
            self.delta_warm = 0;
        } else if !self.delta_ready && self.delta_warm == 0 && self.delta == 0.0 {
            self.delta_warm = self.latency + 1;
        }

        for i in 0..n {
            let mut il = left[i] as f64;
            let mut ir = right[i] as f64;
            // non-finite input is silence; finite input is held within +120 dBFS so that no
            // running sum can overflow
            il = if il.is_finite() { il.clamp(-1e6, 1e6) } else { 0.0 };
            ir = if ir.is_finite() { ir.clamp(-1e6, 1e6) } else { 0.0 };
            in_pk[0] = in_pk[0].max(il.abs());
            in_pk[1] = in_pk[1].max(ir.abs());
            self.drive = drive_t + self.k_drive * (self.drive - drive_t);
            if (self.drive - drive_t).abs() < 1e-12 {
                self.drive = drive_t;
            }
            let (xl, xr) = (il * self.drive, ir * self.drive);
            // Ceiling glide (5 ms), then carried along with the audio.
            if self.ceil != c_t {
                self.ceil = c_t + self.k_drive * (self.ceil - c_t);
                if (self.ceil - c_t).abs() < 1e-12 * c_t {
                    self.ceil = c_t;
                }
                ctx.c_lin = self.ceil;
                ctx.c_db = lin_to_db(self.ceil);
            } else if ctx.c_lin != c_t {
                ctx.c_lin = c_t;
                ctx.c_db = c_db_t;
            }
            self.ceil_line.push(self.ceil, 0);
            // the trim reads the interval between the samples TP_HALF and TP_HALF - 1 back from
            // its input; the output leaves the module one latency after it entered
            let lt = self.l_common + TP_HALF;
            let c_trim = self.ceil_line.tap(lt).min(self.ceil_line.tap(lt - 1));
            let c = self.ceil_line.tap(self.latency);

            let a = self.act;
            let fa = self.slots[a].tick(&ctx, xl, xr);
            let f = match self.phase {
                Phase::Idle => fa,
                Phase::Warm(k) => {
                    let _ = self.slots[1 - a].tick(&ctx, xl, xr);
                    self.phase = if k <= 1 { Phase::Fade(0) } else { Phase::Warm(k - 1) };
                    fa
                }
                Phase::Fade(j) => {
                    let fb = self.slots[1 - a].tick(&ctx, xl, xr);
                    let w = 0.5 - 0.5 * (std::f64::consts::PI * (j + 1) as f64 / self.fade_len as f64).cos();
                    let mix = |x: f64, y: f64| x + w * (y - x);
                    let out = Frame {
                        l: mix(fa.l, fb.l),
                        r: mix(fa.r, fb.r),
                        rl: mix(fa.rl, fb.rl),
                        rr: mix(fa.rr, fb.rr),
                        g: mix(fa.g, fb.g),
                    };
                    if j + 1 >= self.fade_len {
                        self.act = 1 - a;
                        self.phase = Phase::Idle;
                        self.steer(want);
                    } else {
                        self.phase = Phase::Fade(j + 1);
                    }
                    out
                }
            };

            let t = self.trim.tick(f.l, f.r, f.rl, f.rr, f.g, c_trim, tp, refine);
            let (zl, zr) = (t[2], t[3]);
            // never past the gliding ceiling (bit transparent whenever the trim kept it)
            let (yl, yr) = (t[0].clamp(-c, c), t[1].clamp(-c, c));
            let total = t[4] + if t[5] < 1.0 { lin_to_db(t[5]) } else { 0.0 };
            deepest = deepest.min(total);
            if let Some(g) = gain.as_deref_mut() {
                if let Some(s) = g.get_mut(i) {
                    *s = total as f32;
                }
            }
            let gmg = self.gm.tick(il, ir, yl, yr, gm_on);

            // delta: what the engine and trim removed, against the engine's own reference
            if self.delta_warm > 0 {
                self.delta_warm -= 1;
                if self.delta_warm == 0 {
                    self.delta_ready = true;
                }
            }
            let dt = if delta_on && self.delta_ready { 1.0 } else { 0.0 };
            self.delta = dt + self.k_delta * (self.delta - dt);
            if (self.delta - dt).abs() < 1e-9 {
                self.delta = dt;
            }
            let (ol, or) = if self.delta == 0.0 {
                if gmg == 1.0 {
                    (yl, yr)
                } else {
                    (yl * gmg, yr * gmg)
                }
            } else {
                let d = self.delta;
                ((1.0 - d) * yl * gmg + d * (zl - yl), (1.0 - d) * yr * gmg + d * (zr - yr))
            };
            out_pk[0] = out_pk[0].max(ol.abs());
            out_pk[1] = out_pk[1].max(or.abs());
            out_tp = out_tp.max(self.out_tp.push(ol, or, c));
            left[i] = ol as f32;
            right[i] = or as f32;
        }

        // meters and view
        let shown = match self.phase {
            Phase::Fade(j) if 2 * j >= self.fade_len => 1 - self.act,
            _ => self.act,
        };
        let p = self.slots[shown].probe;
        let cfg = self.slots[shown].cfg;
        self.meter_red = (-deepest).max(0.0) as f32;
        let trim_db = lin_to_db(self.trim.block_min_gain).min(0.0);
        let v = &mut self.view_vals;
        let db = |x: f64| lin_to_db(x).max(-200.0) as f32;
        v[view::TOTAL_GAIN] = deepest as f32;
        v[view::SLOW_GAIN] = p.slow as f32;
        v[view::FAST_GAIN] = p.fast as f32;
        v[view::CLIP_GAIN] = p.clip as f32;
        v[view::TRIM_GAIN] = trim_db as f32;
        for b in 0..4 {
            v[view::BAND_GAIN_0 + b] = p.band[b] as f32;
        }
        v[view::CATCHER_GAIN] = p.catcher as f32;
        v[view::CREST] = p.crest as f32;
        v[view::RELEASE_MS] = p.rel_ms as f32;
        v[view::CH0_GAIN] = p.ch[0] as f32;
        v[view::CH1_GAIN] = p.ch[1] as f32;
        v[view::IN_PEAK_L] = db(in_pk[0]);
        v[view::IN_PEAK_R] = db(in_pk[1]);
        v[view::OUT_PEAK_L] = db(out_pk[0]);
        v[view::OUT_PEAK_R] = db(out_pk[1]);
        v[view::TRUE_PEAK] = db(self.trim.block_peak);
        v[view::LOUDNESS_IN] = self.gm.loudness_in().max(-200.0) as f32;
        v[view::LOUDNESS_OUT] = self.gm.loudness_out().max(-200.0) as f32;
        v[view::MATCH_GAIN] = self.gm.applied_db as f32;
        v[view::ENGINE] = cfg.engine as f32;
        v[view::OS_FACTOR] = if matches!(cfg.engine, 0 | 2 | 3) { cfg.u as f32 } else { 1.0 };
        v[view::SLOW_LOUDNESS] = p.loud.max(-200.0) as f32;
        v[view::PAUSE_HOLD] = if p.hold { 1.0 } else { 0.0 };
        v[view::POST_HOLD_MS] = p.post_ms as f32;
        v[view::OUT_TRUE_PEAK] = db(out_tp);
    }
}

impl Module for SmartLimiter {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        // Everything is per sample, so the block size does not size any buffer.
        let fs = sane_rate(sample_rate);
        self.fs = fs;
        self.l_common = common_latency(fs);
        for s in self.slots.iter_mut() {
            s.alloc(fs, self.l_common);
        }
        self.trim.alloc(fs);
        self.out_tp.wmax.alloc(TP_TAPS);
        self.gm.prepare(fs);
        self.latency = self.l_common + self.trim.delay;
        self.fade_len = dsp::ms_to_samples(20.0, fs, 1);
        self.k_drive = dsp::pole(5.0, fs);
        self.ceil_line.alloc(self.latency + 1);
        self.k_delta = dsp::pole(20.0 / 3.0, fs);
        self.prepared = true;
        self.reset();
    }

    fn reset(&mut self) {
        if !self.prepared {
            return;
        }
        let want = self.desired_cfg();
        self.act = 0;
        self.slots[0].start(want, self.l_common);
        self.slots[1].start(want, self.l_common);
        self.phase = Phase::Idle;
        self.trim.clear();
        self.out_tp.clear();
        self.gm.clear();
        self.drive = db_to_lin(self.values[idx::DRIVE] as f64);
        self.ceil = db_to_lin(self.values[idx::CEILING] as f64);
        self.ceil_line.fill(self.ceil);
        self.delta = if self.values[idx::DELTA] >= 0.5 { 1.0 } else { 0.0 };
        self.delta_ready = self.delta > 0.0;
        self.delta_warm = 0;
        self.meter_red = 0.0;
        self.view_vals = [0.0; view::LEN];
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let (Some(p), true) = (PARAMS.get(index), value.is_finite()) else {
            return;
        };
        let mut v = value.clamp(p.min, p.max);
        if p.steps > 1 {
            let interval = (p.max - p.min) / (p.steps - 1) as f32;
            v = (p.min + ((v - p.min) / interval).round() * interval).clamp(p.min, p.max);
        }
        self.values[index] = v;
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_traced(left, right, None);
    }

    fn meters(&self) -> Meters {
        let mut m = Meters { reduction_db: self.meter_red, ..Meters::default() };
        let v = &self.view_vals;
        let e = v[view::ENGINE].round() as usize;
        if e == 4 {
            for b in 0..4 {
                m.band_reduction_db[b] = (-v[view::BAND_GAIN_0 + b]).max(0.0);
            }
            m.band_reduction_db[4] = (-v[view::CATCHER_GAIN]).max(0.0);
            m.band_reduction_db[5] = (-v[view::TRIM_GAIN]).max(0.0);
        } else {
            m.band_reduction_db[0] = (-v[view::SLOW_GAIN]).max(0.0);
            m.band_reduction_db[1] = (-v[view::FAST_GAIN]).max(0.0);
            m.band_reduction_db[2] = (-v[view::CLIP_GAIN]).max(0.0);
            m.band_reduction_db[3] = (-v[view::TRIM_GAIN]).max(0.0);
        }
        m
    }

    fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&self.view_vals[..n]);
        n
    }
}
