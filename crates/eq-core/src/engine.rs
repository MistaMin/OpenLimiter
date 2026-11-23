//! The zero-latency band engine shared by both EQ modules (OpenEQ `processSegment`,
//! `runBandChain`, `applyBandTargets` for the minimum-phase path): 32 bands in series, each with
//! a stereo placement and pan, click-free smoothing of frequency, gain, Q, on/off and the split
//! stream gains, and an optional second band bank for the tonal stream of the transient/tonal split.

use crate::band::{EqBand, CHUNK};
use crate::response::pan_weights;
use crate::smooth::Smoothed;
use crate::types::*;

pub const BANDS: usize = 32;
pub const CONTROL_INTERVAL: usize = 64;

const FREQUENCY_RAMP: f64 = 0.025;
const GAIN_RAMP: f64 = 0.025;
const Q_RAMP: f64 = 0.025;
const BAND_BYPASS_RAMP: f64 = 0.015;

/// One band's settings as the audio uses them (OpenEQ `BandParameterState`).
#[derive(Clone, Copy, Debug)]
pub struct BandState {
    pub active: bool,
    pub solo: bool,
    pub ty: FilterType,
    pub stereo: StereoMode,
    pub frequency: f32,
    /// After the global gain scale.
    pub gain_db: f32,
    pub q: f32,
    /// Encoded slope (see `types::encode_slope_norm`) or the fixed steep slope.
    pub slope_db: i32,
    /// -1..1.
    pub pan: f32,
    pub split_on: bool,
    pub split_trans_db: f32,
    pub split_tail_db: f32,
    /// Free phase for the kernel modes: -1 inverse, 0 linear, +1 minimum.
    pub phase: f32,
    pub dynamic: bool,
    pub dyn_threshold_db: f32,
    pub dyn_range_db: f32,
    pub dyn_attack_ms: f32,
    pub dyn_release_ms: f32,
    pub dyn_action: u8,
    pub dyn_knee_db: f32,
    pub dyn_source: u8,
    pub dyn_relative: f32,
    pub dyn_lookahead_ms: f32,
    pub dyn_transient: f32,
    pub dyn_auto: bool,
    pub dyn_det_link: bool,
    pub dyn_det_freq: f32,
    pub dyn_det_q: f32,
    pub dyn_below_on: bool,
    pub dyn_below_range_db: f32,
    /// 1.0 is 100 %.
    pub dyn_below_ratio: f32,
    pub dyn_above_on: bool,
    pub dyn_above_range_db: f32,
    pub dyn_above_ratio: f32,
    pub dyn_rel_free: bool,
    pub dyn_rel_freq: f32,
    pub dyn_rel_q: f32,
    pub dyn_rel_source: u8,
}

/// OpenEQ's default band frequency: 60 Hz x 1.75^i, up to 20 kHz.
pub fn default_frequency(i: usize) -> f32 {
    (60.0f32 * 1.75f32.powi(i as i32)).clamp(20.0, 20000.0)
}

impl BandState {
    pub fn new(index: usize) -> Self {
        let f = default_frequency(index);
        BandState {
            active: index < 3,
            solo: false,
            ty: FilterType::Bell,
            stereo: StereoMode::Stereo,
            frequency: f,
            gain_db: 0.0,
            q: 1.0,
            slope_db: encode_slope_norm(0.0),
            pan: 0.0,
            split_on: false,
            split_trans_db: 0.0,
            split_tail_db: 0.0,
            phase: 0.0,
            dynamic: false,
            dyn_threshold_db: -24.0,
            dyn_range_db: 6.0,
            dyn_attack_ms: 15.0,
            dyn_release_ms: 150.0,
            dyn_action: 0,
            dyn_knee_db: 0.0,
            dyn_source: 0,
            dyn_relative: 0.0,
            dyn_lookahead_ms: 0.0,
            dyn_transient: 0.0,
            dyn_auto: false,
            dyn_det_link: true,
            dyn_det_freq: f,
            dyn_det_q: 1.0,
            dyn_below_on: false,
            dyn_below_range_db: 6.0,
            dyn_below_ratio: 1.0,
            dyn_above_on: false,
            dyn_above_range_db: -6.0,
            dyn_above_ratio: 1.0,
            dyn_rel_free: false,
            dyn_rel_freq: f,
            dyn_rel_q: 1.0,
            dyn_rel_source: 0,
        }
    }

    /// Slope channel for a type from the normalised slope (0..1), as OpenEQ reads it.
    pub fn slope_for(ty: FilterType, slope_norm: f32) -> i32 {
        if is_steep(ty) {
            STEEP_SLOPE_DB
        } else if uses_continuous_slope(ty) {
            encode_slope_norm(slope_norm)
        } else {
            12
        }
    }

    pub fn audible(&self, any_solo: bool) -> bool {
        self.active && (!any_solo || self.solo)
    }

    pub fn effective_split_db(&self, stream_db: f32) -> f32 {
        if self.split_on && self.active {
            stream_db
        } else {
            0.0
        }
    }

    pub fn is_dynamic(&self, any_solo: bool) -> bool {
        self.dynamic && supports_dynamics(self.ty) && self.audible(any_solo)
    }
}

pub fn any_solo(bands: &[BandState; BANDS]) -> bool {
    bands.iter().any(|b| b.active && b.solo)
}

#[derive(Clone, Copy, Default)]
pub struct BandSmoother {
    pub log_frequency: Smoothed,
    pub gain_db: Smoothed,
    pub log_q: Smoothed,
    pub wet: Smoothed,
    pub split_trans_db: Smoothed,
    pub split_tail_db: Smoothed,
}

impl BandSmoother {
    fn smoothing(&self) -> bool {
        self.log_frequency.is_smoothing()
            || self.gain_db.is_smoothing()
            || self.log_q.is_smoothing()
            || self.wet.is_smoothing()
            || self.split_trans_db.is_smoothing()
            || self.split_tail_db.is_smoothing()
    }
    fn skip(&mut self, n: usize) {
        self.log_frequency.skip(n);
        self.gain_db.skip(n);
        self.log_q.skip(n);
        self.wet.skip(n);
        self.split_trans_db.skip(n);
        self.split_tail_db.skip(n);
    }
}

#[derive(Clone, Copy, Default)]
pub struct SliceMix {
    pub start: f32,
    pub end: f32,
}

/// The split streams of one segment: the transient and tonal streams (both stereo) that the two
/// band banks process, and the solo choice (0 off, 1 transient, 2 tonal).
pub struct SplitIo<'a> {
    pub trans_l: &'a mut [f32],
    pub trans_r: &'a mut [f32],
    pub tonal_l: &'a mut [f32],
    pub tonal_r: &'a mut [f32],
    pub solo: u8,
}

pub struct ZlEngine {
    pub bands: [EqBand; BANDS],
    pub tail: [EqBand; BANDS],
    pub smoothers: [BandSmoother; BANDS],
    was_dry: [bool; BANDS],
    tail_was_dry: [bool; BANDS],
    split_flags: [bool; BANDS],
    pub any_split_bands: bool,
    /// True while the audio runs as two streams.
    pub split_engaged: bool,
    sample_rate: f64,
    /// Coefficient designs installed (tests check that a settled path stops redesigning).
    pub design_updates: u64,
}

impl Default for ZlEngine {
    fn default() -> Self {
        ZlEngine {
            bands: std::array::from_fn(|_| EqBand::default()),
            tail: std::array::from_fn(|_| EqBand::default()),
            smoothers: [BandSmoother::default(); BANDS],
            was_dry: [true; BANDS],
            tail_was_dry: [true; BANDS],
            split_flags: [false; BANDS],
            any_split_bands: false,
            split_engaged: false,
            sample_rate: 44100.0,
            design_updates: 0,
        }
    }
}

/// Runs `data` through `band` on `lane`, blending wet against dry with a mix ramp.
fn process_lane(band: &mut EqBand, data: &mut [f32], lane: usize, start: f32, end: f32) {
    let unity = start >= 1.0 - 1.0e-6 && end >= 1.0 - 1.0e-6;
    if unity {
        band.process(data, lane);
        return;
    }
    let n = data.len();
    let mut dry = [0.0f32; CHUNK];
    dry[..n].copy_from_slice(data);
    band.process(data, lane);
    blend(data, &dry[..n], start, end);
}

fn blend(data: &mut [f32], dry: &[f32], start: f32, end: f32) {
    let n = data.len() as f32;
    for (i, (d, &x)) in data.iter_mut().zip(dry.iter()).enumerate() {
        let t = (i + 1) as f32 / n;
        let mix = start + (end - start) * t;
        *d = x + mix * (*d - x);
    }
}

fn process_lane_pair(band: &mut EqBand, a: &mut [f32], b: &mut [f32], sa: f32, ea: f32, sb: f32, eb: f32) {
    let ua = sa >= 1.0 - 1.0e-6 && ea >= 1.0 - 1.0e-6;
    let ub = sb >= 1.0 - 1.0e-6 && eb >= 1.0 - 1.0e-6;
    let n = a.len();
    let mut da = [0.0f32; CHUNK];
    let mut db = [0.0f32; CHUNK];
    if !ua {
        da[..n].copy_from_slice(a);
    }
    if !ub {
        db[..n].copy_from_slice(b);
    }
    band.process_pair(a, b);
    if !ua {
        blend(a, &da[..n], sa, ea);
    }
    if !ub {
        blend(b, &db[..n], sb, eb);
    }
}

/// One band bank over a block of at most `CHUNK` samples, in band order.
fn run_chain(chain: &mut [EqBand; BANDS], was_dry: &mut [bool; BANDS], left: &mut [f32], right: &mut [f32], mixes: &[SliceMix; BANDS]) {
    let n = left.len().min(right.len()).min(CHUNK);
    let (left, right) = (&mut left[..n], &mut right[..n]);
    let mut mid = [0.0f32; CHUNK];
    let mut side = [0.0f32; CHUNK];
    for i in 0..BANDS {
        let (ms, me) = (mixes[i].start, mixes[i].end);
        if ms <= 1.0e-6 && me <= 1.0e-6 {
            was_dry[i] = true;
            continue;
        }
        let band = &mut chain[i];
        if was_dry[i] {
            band.reset();
        }
        was_dry[i] = false;
        match band.stereo_mode() {
            StereoMode::Stereo => {
                let (wl, wr) = pan_weights(band.pan());
                process_lane_pair(band, left, right, ms * wl as f32, me * wl as f32, ms * wr as f32, me * wr as f32);
            }
            StereoMode::Left => process_lane(band, left, 0, ms, me),
            StereoMode::Right => process_lane(band, right, 1, ms, me),
            mode => {
                for k in 0..n {
                    mid[k] = 0.5 * (left[k] + right[k]);
                    side[k] = 0.5 * (left[k] - right[k]);
                }
                match mode {
                    StereoMode::MidSide => {
                        let (wm, ws) = pan_weights(band.pan());
                        process_lane_pair(band, &mut mid[..n], &mut side[..n], ms * wm as f32, me * wm as f32, ms * ws as f32, me * ws as f32);
                    }
                    StereoMode::Mid => process_lane(band, &mut mid[..n], 0, ms, me),
                    _ => process_lane(band, &mut side[..n], 1, ms, me),
                }
                for k in 0..n {
                    left[k] = mid[k] + side[k];
                    right[k] = mid[k] - side[k];
                }
            }
        }
    }
}

impl ZlEngine {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        for b in self.bands.iter_mut().chain(self.tail.iter_mut()) {
            b.prepare(sample_rate);
        }
        self.was_dry = [true; BANDS];
        self.tail_was_dry = [true; BANDS];
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn reset(&mut self) {
        for b in self.bands.iter_mut().chain(self.tail.iter_mut()) {
            b.reset();
        }
    }

    pub fn reset_tail(&mut self) {
        for b in self.tail.iter_mut() {
            b.reset();
        }
        self.tail_was_dry = [true; BANDS];
    }

    pub fn set_ultra(&mut self, on: bool) {
        for b in self.bands.iter_mut().chain(self.tail.iter_mut()) {
            b.set_ultra(on);
        }
    }

    pub fn set_digital_plus(&mut self, steps: usize) {
        for b in self.bands.iter_mut().chain(self.tail.iter_mut()) {
            b.set_digital_plus(steps);
        }
    }

    pub fn update_split_flags(&mut self, params: &[BandState; BANDS]) {
        let mut any = false;
        for i in 0..BANDS {
            self.split_flags[i] = params[i].split_on && params[i].active;
            any |= self.split_flags[i];
        }
        self.any_split_bands = any;
    }

    /// Jumps every smoother to its target (prepare, settled bypass).
    pub fn snap_smoothers(&mut self, params: &[BandState; BANDS], any_solo: bool) {
        let sr = self.sample_rate;
        for i in 0..BANDS {
            let b = &params[i];
            let s = &mut self.smoothers[i];
            s.log_frequency.reset(sr, FREQUENCY_RAMP);
            s.gain_db.reset(sr, GAIN_RAMP);
            s.log_q.reset(sr, Q_RAMP);
            s.wet.reset(sr, BAND_BYPASS_RAMP);
            s.split_trans_db.reset(sr, GAIN_RAMP);
            s.split_tail_db.reset(sr, GAIN_RAMP);
            s.log_frequency.set_current_and_target(clamp_frequency(b.frequency, sr).ln());
            s.gain_db.set_current_and_target(b.gain_db);
            s.log_q.set_current_and_target(clamp_q(b.q).ln());
            s.wet.set_current_and_target(if b.audible(any_solo) { 1.0 } else { 0.0 });
            s.split_trans_db.set_current_and_target(b.effective_split_db(b.split_trans_db));
            s.split_tail_db.set_current_and_target(b.effective_split_db(b.split_tail_db));
            self.was_dry[i] = s.wet.current() <= 1.0e-6;
        }
    }

    pub fn set_smoother_targets(&mut self, params: &[BandState; BANDS], any_solo: bool) {
        let sr = self.sample_rate;
        for i in 0..BANDS {
            let b = &params[i];
            let s = &mut self.smoothers[i];
            s.log_frequency.set_target(clamp_frequency(b.frequency, sr).ln());
            s.gain_db.set_target(b.gain_db);
            s.log_q.set_target(clamp_q(b.q).ln());
            s.wet.set_target(if b.audible(any_solo) { 1.0 } else { 0.0 });
            s.split_trans_db.set_target(b.effective_split_db(b.split_trans_db));
            s.split_tail_db.set_target(b.effective_split_db(b.split_tail_db));
        }
    }

    /// The frequency and Q a band is designed at right now (smoothed while ramping).
    pub fn current_frequency_q(&self, i: usize, b: &BandState) -> (f32, f32) {
        let s = &self.smoothers[i];
        let f = if s.log_frequency.is_smoothing() { s.log_frequency.current().exp() } else { clamp_frequency(b.frequency, self.sample_rate) };
        let q = if s.log_q.is_smoothing() { s.log_q.current().exp() } else { clamp_q(b.q) };
        (f, q)
    }

    fn design_pair(&mut self, i: usize, b: &BandState, f: f32, gain: f32, q: f32, trans: f32, tail: f32) {
        let changed = self.bands[i].apply_if_changed(b.ty, f, gain + trans, q, b.slope_db);
        if self.any_split_bands || self.split_engaged {
            let t = &mut self.tail[i];
            t.set_pan(b.pan);
            if t.stereo_mode() != b.stereo {
                t.set_stereo_mode(b.stereo);
                t.reset();
            }
            t.apply_if_changed(b.ty, f, gain + tail, q, b.slope_db);
        }
        if changed {
            self.design_updates += 1;
        }
    }

    /// Installs every band's design, at the targets (`use_targets`) or at the smoothed values.
    pub fn apply_band_targets(&mut self, params: &[BandState; BANDS], offsets: &[f32; BANDS], use_targets: bool) {
        self.update_split_flags(params);
        let sr = self.sample_rate;
        for i in 0..BANDS {
            let b = params[i];
            let s = self.smoothers[i];
            let f = if use_targets || !s.log_frequency.is_smoothing() { clamp_frequency(b.frequency, sr) } else { s.log_frequency.current().exp() };
            let q = if use_targets || !s.log_q.is_smoothing() { clamp_q(b.q) } else { s.log_q.current().exp() };
            let gain = (if use_targets { b.gain_db } else { s.gain_db.current() }) + offsets[i];
            let trans = if use_targets { b.effective_split_db(b.split_trans_db) } else { s.split_trans_db.current() };
            let tail = if use_targets { b.effective_split_db(b.split_tail_db) } else { s.split_tail_db.current() };
            self.bands[i].set_pan(b.pan);
            if self.bands[i].stereo_mode() != b.stereo {
                self.bands[i].set_stereo_mode(b.stereo);
                self.bands[i].reset();
            }
            self.design_pair(i, &b, f, gain, q, trans, tail);
        }
    }

    /// Settled bypass: jump to targets and install them, without processing.
    pub fn settle(&mut self, params: &[BandState; BANDS], any_solo: bool) {
        for i in 0..BANDS {
            let b = &params[i];
            let s = &mut self.smoothers[i];
            s.log_frequency.set_current_and_target(clamp_frequency(b.frequency, self.sample_rate).ln());
            s.gain_db.set_current_and_target(b.gain_db);
            s.log_q.set_current_and_target(clamp_q(b.q).ln());
            s.wet.set_current_and_target(if b.audible(any_solo) { 1.0 } else { 0.0 });
            self.was_dry[i] = s.wet.current() <= 1.0e-6;
        }
        self.apply_band_targets(params, &[0.0; BANDS], true);
    }

    pub fn modulating(&self) -> bool {
        self.smoothers.iter().any(|s| s.smoothing())
    }

    pub fn skip_smoothers(&mut self, n: usize) {
        for s in self.smoothers.iter_mut() {
            s.skip(n);
        }
    }

    fn process_bands(&mut self, left: &mut [f32], right: &mut [f32], mixes: &[SliceMix; BANDS], split: &mut Option<SplitIo<'_>>, pos: usize) {
        let n = left.len();
        match split {
            None => run_chain(&mut self.bands, &mut self.was_dry, left, right, mixes),
            Some(io) => {
                let (tl, tr) = (&mut io.trans_l[pos..pos + n], &mut io.trans_r[pos..pos + n]);
                run_chain(&mut self.bands, &mut self.was_dry, tl, tr, mixes);
                let (sl, sr) = (&mut io.tonal_l[pos..pos + n], &mut io.tonal_r[pos..pos + n]);
                run_chain(&mut self.tail, &mut self.tail_was_dry, sl, sr, mixes);
                let tg = if io.solo == 2 { 0.0 } else { 1.0 };
                let sg = if io.solo == 1 { 0.0 } else { 1.0 };
                for k in 0..n {
                    left[k] = tg * io.trans_l[pos + k] + sg * io.tonal_l[pos + k];
                    right[k] = tg * io.trans_r[pos + k] + sg * io.tonal_r[pos + k];
                }
            }
        }
    }

    /// Processes one segment of at most `CHUNK` samples through the band banks.
    ///
    /// `dynamics(position, count, frequency, q, offsets)` is called once per control slice when
    /// `any_dynamic` is set and fills the per-band gain offsets in dB; `frequency` and `q` are the
    /// values each band is designed at for that slice.
    pub fn process_segment(
        &mut self,
        params: &[BandState; BANDS],
        left: &mut [f32],
        right: &mut [f32],
        mut split: Option<SplitIo<'_>>,
        any_dynamic: bool,
        dynamics: &mut dyn FnMut(usize, usize, &[f32; BANDS], &[f32; BANDS], &mut [f32; BANDS]),
    ) {
        let n = left.len().min(right.len()).min(CHUNK);
        for i in 0..BANDS {
            self.bands[i].set_pan(params[i].pan);
            if self.bands[i].stereo_mode() != params[i].stereo {
                self.bands[i].set_stereo_mode(params[i].stereo);
                self.bands[i].reset();
            }
        }
        if !self.modulating() && !any_dynamic {
            self.skip_smoothers(n);
            self.apply_band_targets(params, &[0.0; BANDS], true);
            let mut mixes = [SliceMix::default(); BANDS];
            for i in 0..BANDS {
                let w = self.smoothers[i].wet.current();
                mixes[i] = SliceMix { start: w, end: w };
            }
            self.process_bands(&mut left[..n], &mut right[..n], &mixes, &mut split, 0);
            return;
        }
        let mut pos = 0;
        while pos < n {
            let count = CONTROL_INTERVAL.min(n - pos);
            let mut mixes = [SliceMix::default(); BANDS];
            for i in 0..BANDS {
                let s = &mut self.smoothers[i];
                mixes[i].start = s.wet.current();
                s.skip(count);
                mixes[i].end = s.wet.current();
            }
            let mut freq = [0.0f32; BANDS];
            let mut qs = [0.0f32; BANDS];
            for i in 0..BANDS {
                let (f, q) = self.current_frequency_q(i, &params[i]);
                freq[i] = f;
                qs[i] = q;
            }
            let mut offsets = [0.0f32; BANDS];
            if any_dynamic {
                dynamics(pos, count, &freq, &qs, &mut offsets);
            }
            for i in 0..BANDS {
                let b = params[i];
                let s = self.smoothers[i];
                let gain = (if s.gain_db.is_smoothing() { s.gain_db.current() } else { b.gain_db }) + offsets[i];
                let (trans, tail) = (s.split_trans_db.current(), s.split_tail_db.current());
                self.design_pair(i, &b, freq[i], gain, qs[i], trans, tail);
            }
            let (l, r) = (&mut left[pos..pos + count], &mut right[pos..pos + count]);
            self.process_bands(l, r, &mixes, &mut split, pos);
            pos += count;
        }
    }
}
