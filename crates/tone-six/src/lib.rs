//! Six-band equaliser with a first-order low cut and a stereo / mid-side matrix.
//!
//! Behaviour source: source F of `spec/MASTER_BRIEF.md` (sections 2, 3 step 2 to 4, 4.3), its
//! parameter file in `spec/params/` sections 1, 6, 7 and 9. Signal order (F table section 1):
//! M/S encode -> low cut -> six bands -> M/S decode. Zero latency, every filter minimum phase.
//!
//! Click-free changes: frequency, gain, width and depth are smoothed and the coefficients are
//! recomputed every `CONTROL_BLOCK` samples; switching a band on or off or changing its type
//! fades that band against its own input (10 ms out, switch, 10 ms in); switching the matrix or
//! the whole EQ fades the module against its dry input. Nothing allocates after construction.

pub mod design;

use design::{Cascade, Shape, Sos, SHAPE_LABELS};
use olm_api::{Kind, Meters, Module, ParamDesc};

pub const BANDS: usize = 6;
/// Number of global parameters before the band parameters.
pub const GLOBALS: usize = 5;
/// Parameters per band per channel set.
pub const PER_BAND: usize = 6;
pub const NPARAMS: usize = GLOBALS + 2 * BANDS * PER_BAND;

pub const P_EQ_ON: usize = 0;
pub const P_MATRIX: usize = 1;
pub const P_LINK: usize = 2;
pub const P_LOW_CUT_ON: usize = 3;
pub const P_LOW_CUT_FREQ: usize = 4;
pub const F_ON: usize = 0;
pub const F_TYPE: usize = 1;
pub const F_FREQ: usize = 2;
pub const F_GAIN: usize = 3;
pub const F_WIDTH: usize = 4;
pub const F_DEPTH: usize = 5;

/// Index of field `field` of band `band` (0..6) in channel set `set` (0 = L or M, or both when
/// linked; 1 = R or S).
pub const fn band_param(set: usize, band: usize, field: usize) -> usize {
    GLOBALS + (set * BANDS + band) * PER_BAND + field
}

const MATRIX_LABELS: &[&str] = &["Left/Right", "Mid/Side"];

macro_rules! table {
    ($( ($n:literal, $sfx:literal, $lab:literal, $ty:expr, $fr:expr) ),* $(,)?) => {
        [
            ParamDesc::toggle("eqOn", "EQ On", true),
            ParamDesc::choice("matrix", "Channel Matrix", MATRIX_LABELS, 0),
            ParamDesc::toggle("linkSets", "Link Channel Sets", true),
            ParamDesc::toggle("lowCutOn", "Low Cut On", false),
            ParamDesc::new("lowCutFreq", "Low Cut Frequency", 2.0, 200.0, 20.0, Kind::Hz).log(),
            $(
                ParamDesc::toggle(concat!("b", $n, "On", $sfx), concat!("Band ", $n, " On", $lab), true),
                ParamDesc::choice(concat!("b", $n, "Type", $sfx), concat!("Band ", $n, " Type", $lab), SHAPE_LABELS, $ty),
                ParamDesc::new(concat!("b", $n, "Freq", $sfx), concat!("Band ", $n, " Frequency", $lab), 20.0, 20000.0, $fr, Kind::Hz).log(),
                ParamDesc::new(concat!("b", $n, "Gain", $sfx), concat!("Band ", $n, " Gain", $lab), -12.0, 12.0, 0.0, Kind::Db),
                ParamDesc::new(concat!("b", $n, "Width", $sfx), concat!("Band ", $n, " Width", $lab), 0.02, 4.0, 2.0, Kind::Plain).log(),
                ParamDesc::new(concat!("b", $n, "Depth", $sfx), concat!("Band ", $n, " Reject Depth", $lab), -100.0, 0.0, 0.0, Kind::Db),
            )*
        ]
    };
}

/// Defaults follow the source's state at insertion (F table section 2, indices 12 to 71):
/// band 1 low shelf 3 dB/oct at 50 Hz, bands 2 to 5 bells of 2 octaves at 150 Hz, 500 Hz,
/// 2 kHz and 6 kHz, band 6 high shelf 3 dB/oct at 14 kHz, every gain 0 dB (flat).
pub static PARAMS: [ParamDesc; NPARAMS] = table![
    ("1", "", "", 2, 50.0),
    ("2", "", "", 0, 150.0),
    ("3", "", "", 0, 500.0),
    ("4", "", "", 0, 2000.0),
    ("5", "", "", 0, 6000.0),
    ("6", "", "", 5, 14000.0),
    ("1", "Ch2", " (R/S)", 2, 50.0),
    ("2", "Ch2", " (R/S)", 0, 150.0),
    ("3", "Ch2", " (R/S)", 0, 500.0),
    ("4", "Ch2", " (R/S)", 0, 2000.0),
    ("5", "Ch2", " (R/S)", 0, 6000.0),
    ("6", "Ch2", " (R/S)", 5, 14000.0),
];

/// Samples between control updates (smoothing and coefficient recomputation).
const CONTROL_BLOCK: usize = 16;
/// Time constant of each of the two smoothing stages for frequency, gain, width and depth,
/// seconds. Two stages start the movement with zero slope, so the first coefficient steps are
/// small.
const SMOOTH_TAU: f64 = 0.008;
/// Length of each fade (out or in), seconds.
const FADE_TIME: f64 = 0.010;
/// State magnitude below which an identity band is treated as idle.
const IDLE_STATE: f64 = 1e-15;

#[derive(Clone, Copy)]
struct Target {
    on: bool,
    shape: Shape,
    log_f: f64,
    gain: f64,
    log_w: f64,
    depth: f64,
}

/// Coefficients that glide linearly, sample by sample, to a new design over one control block,
/// so a smoothed parameter never steps the filter (no zipper noise). A straight line between
/// two stable second-order sections stays inside the stability triangle, so the glide is stable.
#[derive(Clone, Copy)]
struct Glide {
    cur: [Sos; 2],
    inc: [[f64; 5]; 2],
    left: usize,
}

impl Glide {
    const fn new() -> Self {
        Glide { cur: [design::IDENTITY, design::IDENTITY], inc: [[0.0; 5]; 2], left: 0 }
    }
    fn to(&mut self, t: &[Sos; 2], glide: bool) {
        if !glide {
            self.cur = *t;
            self.left = 0;
            return;
        }
        let n = CONTROL_BLOCK as f64;
        for k in 0..2 {
            let (c, d) = (&self.cur[k], &t[k]);
            self.inc[k] = [
                (d.b[0] - c.b[0]) / n,
                (d.b[1] - c.b[1]) / n,
                (d.b[2] - c.b[2]) / n,
                (d.a[1] - c.a[1]) / n,
                (d.a[2] - c.a[2]) / n,
            ];
        }
        self.left = CONTROL_BLOCK;
    }
    #[inline]
    fn step(&mut self, t: &[Sos; 2]) {
        if self.left == 0 {
            return;
        }
        self.left -= 1;
        if self.left == 0 {
            self.cur = *t;
            return;
        }
        for k in 0..2 {
            let (c, i) = (&mut self.cur[k], &self.inc[k]);
            c.b[0] += i[0];
            c.b[1] += i[1];
            c.b[2] += i[2];
            c.a[1] += i[3];
            c.a[2] += i[4];
        }
    }
}

/// Fade law: smoothstep of the linear position, so a fade starts and ends with zero slope.
#[inline]
fn fade_gain(w: f64) -> f64 {
    w * w * (3.0 - 2.0 * w)
}

#[inline]
fn ramp(wet: &mut f64, to: f64, step: f64) {
    if *wet != to {
        if *wet < to {
            *wet = (*wet + step).min(to);
        } else {
            *wet = (*wet - step).max(to);
        }
    }
}

#[derive(Clone, Copy)]
struct BandRt {
    shape: Shape,
    log_f: f64,
    gain: f64,
    log_w: f64,
    depth: f64,
    /// First smoothing stage for log_f, gain, log_w, depth (the second stage is the field itself).
    mid: [f64; 4],
    /// The design for the smoothed values.
    casc: Cascade,
    glide: Glide,
    /// Sections run: the design's count, or more while a dropped section still holds state.
    run_n: usize,
    z: [[f64; 2]; 2],
    wet: f64,
    wet_to: f64,
    dirty: bool,
}

impl BandRt {
    const fn new() -> Self {
        BandRt {
            shape: Shape::Bell,
            log_f: 10.0,
            gain: 0.0,
            log_w: 1.0,
            depth: 0.0,
            mid: [10.0, 0.0, 1.0, 0.0],
            casc: Cascade::IDENTITY,
            glide: Glide::new(),
            run_n: 0,
            z: [[0.0; 2]; 2],
            wet: 0.0,
            wet_to: 0.0,
            dirty: true,
        }
    }

    fn snap(&mut self, t: &Target) {
        self.shape = t.shape;
        self.log_f = t.log_f;
        self.gain = t.gain;
        self.log_w = t.log_w;
        self.depth = t.depth;
        self.mid = [t.log_f, t.gain, t.log_w, t.depth];
        self.dirty = true;
    }

    /// Designs for the smoothed values; `glide` moves the running coefficients there over one
    /// control block, otherwise they jump (only when the band is silent or being reset).
    fn design(&mut self, fs: f64, glide: bool) {
        self.casc = design::band(self.shape, fs, self.log_f.exp2(), self.gain, self.log_w.exp2(), self.depth);
        self.glide.to(&self.casc.s, glide);
        self.run_n = if glide { self.run_n.max(self.casc.n) } else { self.casc.n };
        self.dirty = false;
    }

    fn idle(&self) -> bool {
        self.wet == 0.0 && self.wet_to == 0.0
    }

    fn passes_through(&self) -> bool {
        self.casc.n == 0 && self.run_n == 0 && self.glide.left == 0
    }

    #[inline]
    fn tick(&mut self, x: f64, step: f64) -> f64 {
        ramp(&mut self.wet, self.wet_to, step);
        if self.wet == 0.0 {
            return x;
        }
        self.glide.step(&self.casc.s);
        let mut y = x;
        for k in 0..self.run_n.min(2) {
            y = df2t(&self.glide.cur[k], &mut self.z[k], y);
        }
        if self.run_n > self.casc.n && self.glide.left == 0 {
            // a section the design no longer uses runs as identity until its state is flushed
            let k = self.run_n - 1;
            if self.z[k][0].abs() < IDLE_STATE && self.z[k][1].abs() < IDLE_STATE {
                self.z[k] = [0.0; 2];
                self.run_n -= 1;
            }
        }
        if self.wet >= 1.0 {
            y
        } else {
            x + fade_gain(self.wet) * (y - x)
        }
    }

    fn clear(&mut self) {
        self.z = [[0.0; 2]; 2];
    }
}

#[inline]
fn df2t(s: &Sos, z: &mut [f64; 2], x: f64) -> f64 {
    let y = s.b[0] * x + z[0];
    z[0] = s.b[1] * x - s.a[1] * y + z[1];
    z[1] = s.b[2] * x - s.a[2] * y;
    y
}

#[derive(Clone, Copy)]
struct LowCutRt {
    log_f: f64,
    mid: f64,
    sos: [Sos; 2],
    glide: Glide,
    z: [f64; 2],
    wet: f64,
    wet_to: f64,
    dirty: bool,
}

impl LowCutRt {
    const fn new() -> Self {
        LowCutRt {
            log_f: 4.32,
            mid: 4.32,
            sos: [design::IDENTITY, design::IDENTITY],
            glide: Glide::new(),
            z: [0.0; 2],
            wet: 0.0,
            wet_to: 0.0,
            dirty: true,
        }
    }
    fn design(&mut self, fs: f64, glide: bool) {
        self.sos[0] = design::low_cut(fs, self.log_f.exp2());
        self.glide.to(&self.sos, glide);
        self.dirty = false;
    }
    #[inline]
    fn tick(&mut self, x: f64, step: f64) -> f64 {
        ramp(&mut self.wet, self.wet_to, step);
        if self.wet == 0.0 {
            return x;
        }
        self.glide.step(&self.sos);
        let y = df2t(&self.glide.cur[0], &mut self.z, x);
        if self.wet >= 1.0 {
            y
        } else {
            x + fade_gain(self.wet) * (y - x)
        }
    }
}

/// The six-band equaliser module.
pub struct ToneSix {
    values: [f32; NPARAMS],
    fs: f64,
    smooth: f64,
    fade_step: f64,
    bands: [[BandRt; BANDS]; 2],
    low_cut: [LowCutRt; 2],
    mid_side: bool,
    global_wet: f64,
    global_to: f64,
}

impl Default for ToneSix {
    fn default() -> Self {
        let mut values = [0.0f32; NPARAMS];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        let mut m = ToneSix {
            values,
            fs: 48000.0,
            smooth: 0.0,
            fade_step: 0.0,
            bands: [[BandRt::new(); BANDS]; 2],
            low_cut: [LowCutRt::new(); 2],
            mid_side: false,
            global_wet: 1.0,
            global_to: 1.0,
        };
        m.set_rate(48000.0);
        m.reset();
        m
    }
}

impl ToneSix {
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks a parameter up by id.
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// The current coefficients of a band, for displays and tests. `channel` 0 is L or M.
    pub fn band_cascade(&self, channel: usize, band: usize) -> Cascade {
        self.bands[channel & 1][band % BANDS].casc
    }

    fn set_rate(&mut self, fs: f32) {
        let fs = if fs.is_finite() && fs >= 8000.0 { fs as f64 } else { 48000.0 };
        self.fs = fs;
        self.smooth = 1.0 - (-(CONTROL_BLOCK as f64) / (SMOOTH_TAU * fs)).exp();
        self.fade_step = 1.0 / (FADE_TIME * fs).max(1.0);
    }

    fn v(&self, i: usize) -> f32 {
        self.values[i]
    }

    fn target(&self, channel: usize, band: usize) -> Target {
        let set = if self.v(P_LINK) >= 0.5 { 0 } else { channel };
        let p = |f| self.v(band_param(set, band, f)) as f64;
        Target {
            on: p(F_ON) >= 0.5,
            shape: Shape::from_index(p(F_TYPE) as usize),
            log_f: p(F_FREQ).log2(),
            gain: p(F_GAIN),
            log_w: p(F_WIDTH).log2(),
            depth: p(F_DEPTH),
        }
    }

    fn matrix_target(&self) -> bool {
        self.v(P_MATRIX) >= 0.5
    }

    /// Two-stage smoothing of `cur` towards `to` through `mid`; snaps when close. Returns true
    /// when `cur` moved.
    #[inline]
    fn approach(cur: &mut f64, mid: &mut f64, to: f64, a: f64, eps: f64) -> bool {
        if *mid != to {
            let d = to - *mid;
            if d.abs() <= eps {
                *mid = to;
            } else {
                *mid += a * d;
            }
        }
        if *cur == *mid {
            return false;
        }
        let d = *mid - *cur;
        if d.abs() <= eps && *mid == to {
            *cur = to;
        } else {
            *cur += a * d;
        }
        true
    }

    /// Control update, once per `CONTROL_BLOCK`.
    fn control(&mut self) {
        // global fade: whole EQ on/off and matrix changes
        let eq_on = self.v(P_EQ_ON) >= 0.5;
        let want_ms = self.matrix_target();
        if self.global_wet == 0.0 && want_ms != self.mid_side {
            self.mid_side = want_ms;
            for ch in 0..2 {
                for b in 0..BANDS {
                    self.bands[ch][b].clear();
                }
                self.low_cut[ch].z = [0.0; 2];
            }
        }
        self.global_to = if eq_on && want_ms == self.mid_side { 1.0 } else { 0.0 };
        if self.global_wet == 0.0 && self.global_to == 0.0 {
            // the EQ is silent: drop stale histories so it fades back in from rest
            for ch in 0..2 {
                for b in 0..BANDS {
                    self.bands[ch][b].clear();
                }
                self.low_cut[ch].z = [0.0; 2];
            }
        }

        let a = self.smooth;
        let fs = self.fs;
        for ch in 0..2 {
            for b in 0..BANDS {
                let t = self.target(ch, b);
                let rt = &mut self.bands[ch][b];
                // coefficients may jump only while the band is silent
                let mut jump = rt.wet == 0.0;
                if rt.idle() {
                    // silent band: take the new settings at once, then fade in if wanted
                    if rt.shape != t.shape || rt.log_f != t.log_f || rt.gain != t.gain || rt.log_w != t.log_w || rt.depth != t.depth {
                        rt.snap(&t);
                    }
                    rt.clear();
                }
                if rt.shape != t.shape {
                    rt.wet_to = 0.0;
                    if rt.wet == 0.0 {
                        rt.snap(&t);
                        rt.clear();
                        jump = true;
                    }
                } else {
                    rt.wet_to = if t.on { 1.0 } else { 0.0 };
                }
                if rt.shape == t.shape {
                    let mut ch_ = false;
                    ch_ |= Self::approach(&mut rt.log_f, &mut rt.mid[0], t.log_f, a, 1e-6);
                    ch_ |= Self::approach(&mut rt.gain, &mut rt.mid[1], t.gain, a, 1e-5);
                    ch_ |= Self::approach(&mut rt.log_w, &mut rt.mid[2], t.log_w, a, 1e-6);
                    ch_ |= Self::approach(&mut rt.depth, &mut rt.mid[3], t.depth, a, 1e-4);
                    if ch_ {
                        rt.dirty = true;
                    }
                }
                if rt.dirty {
                    rt.design(fs, !jump);
                }
            }
            let lc = &mut self.low_cut[ch];
            let lc_on = self.values[P_LOW_CUT_ON] >= 0.5;
            let lc_f = (self.values[P_LOW_CUT_FREQ] as f64).log2();
            if lc.wet == 0.0 && lc.wet_to == 0.0 {
                if lc.log_f != lc_f {
                    lc.log_f = lc_f;
                    lc.mid = lc_f;
                    lc.dirty = true;
                }
                lc.z = [0.0; 2];
            }
            lc.wet_to = if lc_on { 1.0 } else { 0.0 };
            if Self::approach(&mut lc.log_f, &mut lc.mid, lc_f, a, 1e-6) {
                lc.dirty = true;
            }
            if lc.dirty {
                let glide = lc.wet != 0.0;
                lc.design(fs, glide);
            }
        }
    }

    fn chain_idle(&self) -> bool {
        !self.mid_side
            && self.low_cut.iter().all(|l| l.wet == 0.0 && l.wet_to == 0.0)
            && self.bands.iter().all(|c| c.iter().all(|b| b.idle() || (b.wet == b.wet_to && b.passes_through())))
    }
}

impl Module for ToneSix {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.set_rate(sample_rate);
        self.reset();
    }

    /// Clears every history and settles every smoother and fade on its target.
    fn reset(&mut self) {
        self.mid_side = self.matrix_target();
        self.global_to = if self.v(P_EQ_ON) >= 0.5 { 1.0 } else { 0.0 };
        self.global_wet = self.global_to;
        let fs = self.fs;
        for ch in 0..2 {
            for b in 0..BANDS {
                let t = self.target(ch, b);
                let rt = &mut self.bands[ch][b];
                rt.snap(&t);
                rt.clear();
                rt.wet_to = if t.on { 1.0 } else { 0.0 };
                rt.wet = rt.wet_to;
                rt.design(fs, false);
            }
            let lc = &mut self.low_cut[ch];
            lc.log_f = (self.values[P_LOW_CUT_FREQ] as f64).log2();
            lc.mid = lc.log_f;
            lc.design(fs, false);
            lc.z = [0.0; 2];
            lc.wet_to = if self.values[P_LOW_CUT_ON] >= 0.5 { 1.0 } else { 0.0 };
            lc.wet = lc.wet_to;
        }
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        if !value.is_finite() {
            return;
        }
        let mut v = value.max(p.min).min(p.max);
        if p.steps >= 2 {
            v = v.round();
        }
        self.values[index] = v;
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let mut i = 0;
        while i < n {
            self.control();
            let end = (i + CONTROL_BLOCK).min(n);
            if self.global_wet == 0.0 && self.global_to == 0.0 {
                i = end; // whole EQ off: input passes untouched
                continue;
            }
            if self.global_wet == 1.0 && self.global_to == 1.0 && self.chain_idle() {
                i = end; // everything flat: bit-exact pass-through
                continue;
            }
            let step = self.fade_step;
            for k in i..end {
                let (dl, dr) = (left[k] as f64, right[k] as f64);
                let (mut a, mut b) = if self.mid_side { (0.5 * (dl + dr), 0.5 * (dl - dr)) } else { (dl, dr) };
                a = self.low_cut[0].tick(a, step);
                b = self.low_cut[1].tick(b, step);
                for band in 0..BANDS {
                    a = self.bands[0][band].tick(a, step);
                    b = self.bands[1][band].tick(b, step);
                }
                let (wl, wr) = if self.mid_side { (a + b, a - b) } else { (a, b) };
                if self.global_wet != self.global_to {
                    if self.global_wet < self.global_to {
                        self.global_wet = (self.global_wet + step).min(self.global_to);
                    } else {
                        self.global_wet = (self.global_wet - step).max(self.global_to);
                    }
                }
                let g = fade_gain(self.global_wet);
                let (ol, or) = if g >= 1.0 { (wl, wr) } else { (dl + g * (wl - dl), dr + g * (wr - dr)) };
                left[k] = ol as f32;
                right[k] = or as f32;
            }
            i = end;
        }
    }

    fn meters(&self) -> Meters {
        Meters::default()
    }
}
