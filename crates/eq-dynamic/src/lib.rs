//! Dynamic EQ: OpenEQ 1.0.1's dynamic bands as their own module. 32 bands, each the full OpenEQ
//! band filter (type, frequency, gain, Q, slope, placement, pan, solo) plus every dynamic control:
//! threshold, signed range, attack, release, single action, knee, detector source, relative
//! blend, lookahead, transient weight, automatic timing/threshold, detector link / frequency / Q,
//! the Below and Above sections (range and ratio each) and the relative detector (free band,
//! frequency, Q, source).
//!
//! Everything runs on the zero-latency (minimum-phase) path, as in OpenEQ, where the kernel modes
//! leave dynamic bands out. Latency is 0, or a fixed 20 ms while the global lookahead is on or any
//! audible dynamic band asks for lookahead; a change fades out, switches, and fades back in.
//!
//! External detector sources read the key of `process_keyed`; with plain `process` they follow
//! the main input. The relative detector's External source reads the key without lookahead, as
//! in OpenEQ.

#![allow(clippy::excessive_precision)]

pub mod dynamics;
pub mod params;

use dynamics::{Dynamics, Mix, Settings, MAX_SLICE};
use eq_core::band::CHUNK;
use eq_core::engine::{any_solo, BandState, ZlEngine, BANDS};
use eq_core::fade::LatencyFade;
use eq_core::types::*;
use eq_core::{curve, zero_non_finite};
use olm_api::{Meters, Module, ParamDesc};
pub use params::*;
/// The filter type rules (which controls a type uses), for an editor.
pub use eq_core::types as filter_types;

pub static TYPES: &[&str] = &FILTER_TYPE_NAMES;
pub static PLACEMENTS: &[&str] = &PLACEMENT_NAMES;
pub static ACTIONS: &[&str] = &["Above", "Below"];
pub static SOURCES: &[&str] = &["Follow", "Left", "Right", "Mid", "Side", "External"];
pub static REL_SOURCES: &[&str] = &["Energy", "Left", "Right", "Mid", "Side", "External"];

/// Index of field `field` (`F_*`) of band `band` (0..32).
pub const fn band_param(band: usize, field: usize) -> usize {
    band * PER_BAND + field
}

const DELAY: usize = 65536;
const LOOKAHEAD_SECONDS: f64 = 0.020;

/// The `view()` read-out layout: 32 floats each.
pub mod view {
    /// Current dynamic gain per band, dB (0 for bands that are not dynamic).
    pub const GAIN_DB: usize = 0;
    /// Detector envelope per band, dB (-100 floor).
    pub const ENVELOPE_DB: usize = 32;
    /// Threshold the gain computer used, dB (Auto moves it).
    pub const THRESHOLD_DB: usize = 64;
    pub const LEN: usize = 96;
}

struct Delay {
    l: Vec<f32>,
    r: Vec<f32>,
    kl: Vec<f32>,
    kr: Vec<f32>,
    write: usize,
}

impl Delay {
    fn new() -> Self {
        Delay { l: vec![0.0; DELAY], r: vec![0.0; DELAY], kl: vec![0.0; DELAY], kr: vec![0.0; DELAY], write: 0 }
    }
    fn clear(&mut self) {
        for v in [&mut self.l, &mut self.r, &mut self.kl, &mut self.kr] {
            v.fill(0.0);
        }
        self.write = 0;
    }
    #[inline]
    fn push(&mut self, l: f32, r: f32, kl: f32, kr: f32) {
        self.l[self.write] = l;
        self.r[self.write] = r;
        self.kl[self.write] = kl;
        self.kr[self.write] = kr;
        self.write = (self.write + 1) & (DELAY - 1);
    }
    #[inline]
    fn idx(&self, age: usize) -> usize {
        (self.write + DELAY - 1 - age.min(DELAY - 1)) & (DELAY - 1)
    }
}

pub struct DynamicEq {
    values: Vec<f32>,
    sample_rate: f64,
    prepared: bool,
    engine: Box<ZlEngine>,
    dynamics: Box<[Dynamics; BANDS]>,
    delay: Option<Delay>,
    lookahead: usize,
    fade: LatencyFade,
    primed: bool,
    applied_gain: [f32; BANDS],
    applied_threshold: [f32; BANDS],
}

impl Default for DynamicEq {
    fn default() -> Self {
        DynamicEq {
            values: PARAMS.iter().map(|p| p.default).collect(),
            sample_rate: 48000.0,
            prepared: false,
            engine: Box::default(),
            dynamics: Box::new([Dynamics::default(); BANDS]),
            delay: None,
            lookahead: 0,
            fade: LatencyFade::default(),
            primed: false,
            applied_gain: [0.0; BANDS],
            applied_threshold: [-24.0; BANDS],
        }
    }
}

fn mix_of(i: u8) -> Mix {
    match i {
        1 => Mix::Left,
        2 => Mix::Right,
        3 => Mix::Mid,
        4 => Mix::Side,
        _ => Mix::Energy,
    }
}

impl DynamicEq {
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// Band `i` as the audio uses it.
    pub fn band_state(&self, i: usize) -> BandState {
        let v = |f: usize| self.values[band_param(i, f)];
        let ty = FilterType::from_index(v(F_TYPE) as usize);
        let global_ahead = self.values[P_LOOKAHEAD_ON] > 0.5;
        let mut b = BandState::new(i);
        b.active = v(F_ON) > 0.5;
        b.solo = v(F_SOLO) > 0.5;
        b.ty = ty;
        b.stereo = StereoMode::from_index(v(F_PLACEMENT) as usize);
        b.frequency = v(F_FREQ);
        b.gain_db = v(F_GAIN);
        b.q = v(F_Q);
        b.slope_db = BandState::slope_for(ty, v(F_SLOPE) / 100.0);
        b.pan = (v(F_PAN) / 100.0).clamp(-1.0, 1.0);
        b.dynamic = v(F_DYN) > 0.5;
        b.dyn_threshold_db = v(F_THRESH);
        b.dyn_range_db = v(F_RANGE);
        b.dyn_attack_ms = v(F_ATTACK);
        b.dyn_release_ms = v(F_RELEASE);
        b.dyn_action = v(F_ACTION) as u8;
        b.dyn_knee_db = v(F_KNEE);
        b.dyn_source = v(F_SOURCE) as u8;
        b.dyn_relative = v(F_RELATIVE);
        b.dyn_lookahead_ms = if global_ahead { self.values[P_LOOKAHEAD_MS] } else { v(F_LOOKAHEAD) };
        b.dyn_transient = v(F_TRANSIENT);
        b.dyn_auto = v(F_AUTO) > 0.5;
        b.dyn_det_link = v(F_DET_LINK) > 0.5;
        b.dyn_det_freq = v(F_DET_FREQ);
        b.dyn_det_q = v(F_DET_Q);
        b.dyn_below_on = v(F_BELOW_ON) > 0.5;
        b.dyn_below_range_db = v(F_BELOW_RANGE);
        b.dyn_below_ratio = v(F_BELOW_RATIO) / 100.0;
        b.dyn_above_on = v(F_ABOVE_ON) > 0.5;
        b.dyn_above_range_db = v(F_ABOVE_RANGE);
        b.dyn_above_ratio = v(F_ABOVE_RATIO) / 100.0;
        b.dyn_rel_free = v(F_REL_FREE) > 0.5;
        b.dyn_rel_freq = v(F_REL_FREQ);
        b.dyn_rel_q = v(F_REL_Q);
        b.dyn_rel_source = v(F_REL_SOURCE) as u8;
        b
    }

    fn states(&self) -> [BandState; BANDS] {
        std::array::from_fn(|i| self.band_state(i))
    }

    fn wanted_lookahead(&self, params: &[BandState; BANDS], solo: bool) -> usize {
        let n = (LOOKAHEAD_SECONDS * self.sample_rate).round() as usize;
        if self.values[P_LOOKAHEAD_ON] > 0.5 {
            return n;
        }
        if params.iter().any(|b| b.is_dynamic(solo) && b.dyn_lookahead_ms > 0.05) {
            return n;
        }
        0
    }

    /// Current dynamic gain of band `i` in dB (0 while the band is not dynamic).
    pub fn band_gain_db(&self, i: usize) -> f32 {
        self.applied_gain.get(i).copied().unwrap_or(0.0)
    }

    /// Detector envelope of band `i` in dB.
    pub fn envelope_db(&self, i: usize) -> f32 {
        self.dynamics.get(i).map_or(-100.0, |d| d.envelope_db())
    }

    /// Threshold the gain computer last used for band `i` (Auto moves it).
    pub fn threshold_db(&self, i: usize) -> f32 {
        self.applied_threshold.get(i).copied().unwrap_or(-24.0)
    }

    /// Combined magnitude of the audible bands in dB at `freqs`; `live` adds each band's
    /// current dynamic gain. Allocation-free.
    pub fn response_db(&self, freqs: &[f32], out: &mut [f32], live: bool) {
        let s = self.states();
        curve::total_db(&s, self.sample_rate, if live { Some(&self.applied_gain) } else { None }, freqs, out);
    }

    /// Channel-aware curves (left-to-left and right-to-right gains of the placement matrix).
    pub fn response_channels_db(&self, freqs: &[f32], left: &mut [f32], right: &mut [f32], live: bool) {
        let s = self.states();
        curve::channel_db(&s, self.sample_rate, if live { Some(&self.applied_gain) } else { None }, freqs, left, right);
    }

    /// Band `i` alone in dB. `gain_override_db` replaces the band gain (for example the static
    /// gain plus `band_gain_db(i)` for the live curve); `None` draws the static band.
    pub fn band_response_db(&self, i: usize, gain_override_db: Option<f32>, freqs: &[f32], out: &mut [f32]) {
        if i >= BANDS {
            return;
        }
        let mut b = self.band_state(i);
        if let Some(g) = gain_override_db {
            b.gain_db = g;
        }
        curve::band_db(&b, self.sample_rate, 0.0, freqs, out);
    }

    fn process_inner(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        if !self.prepared {
            return;
        }
        let n = left.len().min(right.len());
        let (left, right) = (&mut left[..n], &mut right[..n]);
        zero_non_finite(left);
        zero_non_finite(right);
        let key = key.filter(|(a, b)| a.len() >= n && b.len() >= n);

        let params = self.states();
        let solo = any_solo(&params);
        self.engine.set_smoother_targets(&params, solo);

        let wanted = self.wanted_lookahead(&params, solo);
        let mut switched = false;
        if !self.primed {
            self.primed = true;
            self.lookahead = wanted;
        } else if wanted != self.lookahead {
            if !self.fade.pending {
                self.fade.pending = true;
            } else if self.fade.ready_to_switch() {
                self.lookahead = wanted;
                switched = true;
            }
        } else {
            self.fade.pending = false;
        }
        if switched {
            self.fade.switched(self.lookahead);
            if let Some(d) = self.delay.as_mut() {
                d.clear();
            }
        }

        let mut off = 0;
        while off < n {
            let count = CHUNK.min(n - off);
            let k = key.map(|(a, b)| (&a[off..off + count], &b[off..off + count]));
            self.segment(&params, solo, &mut left[off..off + count], &mut right[off..off + count], k);
            off += count;
        }
        self.fade.apply(left, right);

        let bad = zero_non_finite(left) | zero_non_finite(right);
        if bad {
            self.engine.reset();
        }
    }

    fn segment(&mut self, params: &[BandState; BANDS], solo: bool, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let n = left.len();
        let mut det_l = [0.0f32; CHUNK];
        let mut det_r = [0.0f32; CHUNK];
        det_l[..n].copy_from_slice(left);
        det_r[..n].copy_from_slice(right);
        let lookahead = self.lookahead;
        if lookahead > 0 {
            if let Some(d) = self.delay.as_mut() {
                for i in 0..n {
                    let (kl, kr) = key.map_or((0.0, 0.0), |(a, b)| (a[i], b[i]));
                    d.push(det_l[i], det_r[i], kl, kr);
                    let j = d.idx(lookahead);
                    left[i] = d.l[j];
                    right[i] = d.r[j];
                }
            }
        }
        let any_dynamic = params.iter().any(|b| b.is_dynamic(solo));
        let sr = self.sample_rate;
        let dynamics = &mut self.dynamics;
        let applied_gain = &mut self.applied_gain;
        let applied_threshold = &mut self.applied_threshold;
        let delay = self.delay.as_ref();
        let mut advance = |pos: usize, count: usize, freq: &[f32; BANDS], q: &[f32; BANDS], out: &mut [f32; BANDS]| {
            for i in 0..BANDS {
                out[i] = 0.0;
                let b = &params[i];
                if !b.is_dynamic(solo) {
                    applied_gain[i] = 0.0;
                    continue;
                }
                let dfreq = if b.dyn_det_link { freq[i] } else { clamp_frequency(b.dyn_det_freq, sr) };
                let dq = if b.dyn_det_link { q[i] } else { clamp_q(b.dyn_det_q) };
                let dy = &mut dynamics[i];
                dy.set_trigger_filter(dfreq, dq);
                dy.set_relative_filter(clamp_frequency(b.dyn_rel_freq, sr), clamp_q(b.dyn_rel_q));
                let external = b.dyn_source == 5 && key.is_some();
                let mut hl = [0.0f32; MAX_SLICE];
                let mut hr = [0.0f32; MAX_SLICE];
                let (dl, dr): (&[f32], &[f32]) = match (lookahead > 0, delay) {
                    (true, Some(d)) => {
                        let lead = ((b.dyn_lookahead_ms as f64 * 0.001 * sr).round() as usize).min(lookahead);
                        let extra = lookahead - lead;
                        for k in 0..count {
                            let age = (n - 1 - (pos + k)) + extra;
                            let j = d.idx(age);
                            if external {
                                hl[k] = d.kl[j];
                                hr[k] = d.kr[j];
                            } else {
                                hl[k] = d.l[j];
                                hr[k] = d.r[j];
                            }
                        }
                        (&hl[..count], &hr[..count])
                    }
                    _ => match key {
                        Some((a, bk)) if external => (&a[pos..pos + count], &bk[pos..pos + count]),
                        _ => (&det_l[pos..pos + count], &det_r[pos..pos + count]),
                    },
                };
                let mix = if external {
                    Mix::Energy
                } else {
                    match b.dyn_source {
                        1..=4 => mix_of(b.dyn_source),
                        _ => match b.stereo {
                            StereoMode::Left => Mix::Left,
                            StereoMode::Right => Mix::Right,
                            StereoMode::Mid => Mix::Mid,
                            StereoMode::Side => Mix::Side,
                            _ => Mix::Energy,
                        },
                    }
                };
                let rel_mix = if (1..=4).contains(&b.dyn_rel_source) { mix_of(b.dyn_rel_source) } else { Mix::Energy };
                let rel = match key {
                    Some((a, bk)) if b.dyn_rel_source == 5 => Some((&a[pos..pos + count], &bk[pos..pos + count])),
                    _ => None,
                };
                let s = Settings {
                    threshold_db: b.dyn_threshold_db,
                    knee_db: b.dyn_knee_db,
                    attack_ms: b.dyn_attack_ms,
                    release_ms: b.dyn_release_ms,
                    transient_percent: b.dyn_transient,
                    automatic: b.dyn_auto,
                    relative_percent: b.dyn_relative,
                    relative_free: b.dyn_rel_free,
                    modern: b.dyn_below_on || b.dyn_above_on,
                    below_on: b.dyn_below_on,
                    below_range_db: b.dyn_below_range_db,
                    below_ratio: b.dyn_below_ratio,
                    above_on: b.dyn_above_on,
                    above_range_db: b.dyn_above_range_db,
                    above_ratio: b.dyn_above_ratio,
                    action: b.dyn_action,
                    range_db: b.dyn_range_db,
                };
                let g = dy.advance(dl, dr, mix, rel, rel_mix, &s);
                applied_threshold[i] = dy.last_threshold_db();
                out[i] = g;
                applied_gain[i] = g;
            }
        };
        self.engine.process_segment(params, left, right, None, any_dynamic, &mut advance);
        if !any_dynamic {
            self.applied_gain = [0.0; BANDS];
        }
    }
}

impl Module for DynamicEq {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate as f64;
        self.engine.prepare(self.sample_rate);
        for d in self.dynamics.iter_mut() {
            d.prepare(self.sample_rate);
        }
        self.delay = Some(Delay::new());
        self.fade.prepare(self.sample_rate);
        let params = self.states();
        let solo = any_solo(&params);
        self.engine.snap_smoothers(&params, solo);
        self.engine.apply_band_targets(&params, &[0.0; BANDS], true);
        self.lookahead = self.wanted_lookahead(&params, solo);
        self.primed = true;
        self.applied_gain = [0.0; BANDS];
        self.prepared = true;
    }

    fn reset(&mut self) {
        self.engine.reset();
        for d in self.dynamics.iter_mut() {
            d.reset();
        }
        if let Some(d) = self.delay.as_mut() {
            d.clear();
        }
        self.applied_gain = [0.0; BANDS];
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index >= NPARAMS || value.is_nan() {
            return;
        }
        let p = PARAMS[index];
        let mut v = value.clamp(p.min, p.max);
        if p.steps >= 2 {
            v = v.round();
        }
        self.values[index] = v;
    }

    /// The latency for the current settings (20 ms of lookahead or none). After a change the
    /// audio follows once its 5 ms fade-out is done.
    fn latency(&self) -> usize {
        let params = self.states();
        self.wanted_lookahead(&params, any_solo(&params))
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_inner(left, right, None);
    }

    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        self.process_inner(left, right, Some((key_left, key_right)));
    }

    fn meters(&self) -> Meters {
        let mut m = Meters::default();
        for (i, &g) in self.applied_gain.iter().enumerate() {
            let cut = (-g).max(0.0);
            m.reduction_db = m.reduction_db.max(cut);
            if i < 8 {
                m.band_reduction_db[i] = cut;
            }
        }
        m
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        for (k, o) in out[..n].iter_mut().enumerate() {
            let i = k % BANDS;
            *o = match k / BANDS {
                0 => self.band_gain_db(i),
                1 => self.envelope_db(i),
                _ => self.threshold_db(i),
            };
        }
        n
    }
}
