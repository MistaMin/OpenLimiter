//! Parametric EQ: the static OpenEQ 1.0.1 equaliser. 32 bands with every OpenEQ filter type and
//! the continuous slope, stereo placement (Stereo, Mid, Side, Left, Right, Mid Side) with pan,
//! solo, per-band free phase for the kernel modes, and the transient/tonal split with per-band
//! transient and tail gains. Globals: processing mode (Zero Latency, Linear Phase, Analog, Mix,
//! Analog ZL) with FIR window, quality and mix crossover; Digital+ phase steps; 117-bit
//! (double-double) recursion; 2x oversampling; input trim; output gain; gain scale; width; side
//! level; balance; drive; polarity; auto gain; latency-compensated bypass; and the split globals.
//!
//! Latency (internal samples, see `latency()` for the host figure):
//! Zero Latency 0; + split separator while a band is split (1791 at 48 kHz); Linear Phase, Analog
//! and Mix half the kernel (1024 / 2048 / 4096 / 8192 by quality); Analog ZL 0; + the drive
//! oversampler while drive is above 0. With 2x oversampling the host sees
//! ceil(oversampler) + (internal + 1) / 2. A change fades out, switches, waits out the new delay
//! and fades in.

#![allow(clippy::excessive_precision)]

pub mod lp;
pub mod params;
pub mod split;

use eq_core::band::CHUNK;
use eq_core::engine::{any_solo, BandState, SplitIo, ZlEngine, BANDS};
use eq_core::fade::LatencyFade;
use eq_core::jfilt::db_to_gain;
use eq_core::oversample::{round_trip_latency, HalfBand};
use eq_core::response::design_band;
use eq_core::smooth::Smoothed;
use eq_core::types::*;
use eq_core::{curve, zero_non_finite};
use lp::{CurveBand, LpEngine, Request, Target};
use olm_api::{Meters, Module, ParamDesc};
pub use params::*;
/// The filter type rules (which controls a type uses), for an editor.
pub use eq_core::types as filter_types;
use split::SplitSeparator;

pub static TYPES: &[&str] = &FILTER_TYPE_NAMES;
pub static PLACEMENTS: &[&str] = &PLACEMENT_NAMES;
pub static MODES: &[&str] = &["Zero Latency", "Linear Phase", "Analog", "Mix", "Analog ZL"];
pub static QUALITIES: &[&str] = &["Low", "Medium", "High", "Maximum"];
pub static WINDOWS: &[&str] = &["Blackman-Harris", "Kaiser", "Hann", "Hamming", "Blackman", "Rectangular"];
pub static CROSSOVERS: &[&str] = &["500 Hz", "1 kHz", "2 kHz", "4 kHz", "8 kHz"];
pub static DIGITAL_PLUS: &[&str] = &["Off", "1", "2", "3", "4"];
pub static SPLIT_SOURCES: &[&str] = &["Full Mix", "Drums", "Percussive", "Instrument", "Vocal"];
pub static SPLIT_SOLO: &[&str] = &["Off", "Transient", "Tonal"];
const CROSSOVER_HZ: [f64; 5] = [500.0, 1000.0, 2000.0, 4000.0, 8000.0];

/// Index of field `field` (`F_*`) of band `band` (0..32).
pub const fn band_param(band: usize, field: usize) -> usize {
    band * PER_BAND + field
}

/// The `view()` read-out layout.
pub mod view {
    /// Peak of the transient stream (linear, held with a slow fall), 0 while no band is split.
    pub const SPLIT_TRANSIENT_PEAK: usize = 0;
    /// Peak of the tonal stream.
    pub const SPLIT_TONAL_PEAK: usize = 1;
    /// Share of the energy in the transient stream, 0..1 (0.5 while idle).
    pub const SPLIT_TRANSIENT_SHARE: usize = 2;
    /// 1 while the split path runs.
    pub const SPLIT_ACTIVE: usize = 3;
    /// Host latency in samples.
    pub const LATENCY: usize = 4;
    pub const LEN: usize = 5;
}

const DELAY: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
struct LatencyConfig {
    mode: usize,
    order: u32,
    drive: bool,
    split: bool,
}

impl LatencyConfig {
    fn same_latency(&self, o: &LatencyConfig) -> bool {
        (self.mode != 0) == (o.mode != 0) && (self.mode == 4) == (o.mode == 4) && (self.mode == 0 || self.order == o.order) && self.drive == o.drive && self.split == o.split
    }
}

struct Saturator {
    hb: [HalfBand; 2],
    latency: usize,
}

impl Saturator {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], drive: f32) {
        if drive <= 1.0e-4 {
            return;
        }
        let k = 1.0 + drive.clamp(0.0, 1.0) * 8.0;
        for (ch, x) in [l, r].into_iter().enumerate() {
            let hb = &mut self.hb[ch];
            for v in x.iter_mut() {
                let [a, b] = hb.up(*v);
                let s = [(k * a).tanh() / k, (k * b).tanh() / k];
                *v = hb.down(s);
            }
        }
    }
}

struct Globals {
    input: Smoothed,
    output: Smoothed,
    width: Smoothed,
    side: Smoothed,
    pan: Smoothed,
    bypass: Smoothed,
}

pub struct ParametricEq {
    values: Vec<f32>,
    host_rate: f64,
    rate: f64,
    prepared: bool,
    oversampling: bool,
    engine: Box<ZlEngine>,
    splitters: Vec<SplitSeparator>,
    split_index: usize,
    split_trans: [Vec<f32>; 2],
    split_tonal: [Vec<f32>; 2],
    lp: Option<Box<LpEngine>>,
    sat: Saturator,
    host_os: [HalfBand; 2],
    os_latency: f64,
    os_buf: [Vec<f32>; 2],
    g: Globals,
    dry_delay: [Vec<f32>; 2],
    delay_write: usize,
    latched: LatencyConfig,
    primed: bool,
    fade: LatencyFade,
    kernel_wait: i64,
    held_drive: f32,
    reported: usize,
    was_bypassed: bool,
    meters: [f32; 3],
    split_active: bool,
    os_pending: bool,
}

impl Default for ParametricEq {
    fn default() -> Self {
        ParametricEq {
            values: PARAMS.iter().map(|p| p.default).collect(),
            host_rate: 48000.0,
            rate: 48000.0,
            prepared: false,
            oversampling: false,
            engine: Box::default(),
            splitters: Vec::new(),
            split_index: 0,
            split_trans: [Vec::new(), Vec::new()],
            split_tonal: [Vec::new(), Vec::new()],
            lp: None,
            sat: Saturator { hb: [HalfBand::new(), HalfBand::new()], latency: 0 },
            host_os: [HalfBand::new(), HalfBand::new()],
            os_latency: 0.0,
            os_buf: [Vec::new(), Vec::new()],
            g: Globals {
                input: Smoothed::default(),
                output: Smoothed::default(),
                width: Smoothed::default(),
                side: Smoothed::default(),
                pan: Smoothed::default(),
                bypass: Smoothed::default(),
            },
            dry_delay: [Vec::new(), Vec::new()],
            delay_write: 0,
            latched: LatencyConfig::default(),
            primed: false,
            fade: LatencyFade::default(),
            kernel_wait: 0,
            held_drive: 0.0,
            reported: 0,
            was_bypassed: false,
            meters: [0.0, 0.0, 0.5],
            split_active: false,
            os_pending: false,
        }
    }
}

fn side_to_linear(db: f32) -> f32 {
    if db <= -99.9 {
        0.0
    } else {
        db_to_gain(db as f64) as f32
    }
}

fn lp_order(quality: usize) -> u32 {
    11 + quality.min(3) as u32
}

impl ParametricEq {
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    fn v(&self, i: usize) -> f32 {
        self.values[i]
    }

    /// Band `i` as the audio uses it (gain after the gain scale).
    pub fn band_state(&self, i: usize) -> BandState {
        let v = |f: usize| self.values[band_param(i, f)];
        let ty = FilterType::from_index(v(F_TYPE) as usize);
        let mut b = BandState::new(i);
        b.active = v(F_ON) > 0.5;
        b.solo = v(F_SOLO) > 0.5;
        b.ty = ty;
        b.stereo = StereoMode::from_index(v(F_PLACEMENT) as usize);
        b.frequency = v(F_FREQ);
        b.gain_db = v(F_GAIN) * self.v(P_GAIN_SCALE) / 100.0;
        b.q = v(F_Q);
        b.slope_db = BandState::slope_for(ty, v(F_SLOPE) / 100.0);
        b.pan = (v(F_PAN) / 100.0).clamp(-1.0, 1.0);
        b.phase = (v(F_PHASE) / 100.0).clamp(-1.0, 1.0);
        b.split_on = v(F_SPLIT) > 0.5;
        b.split_trans_db = v(F_SPLIT_TRANS);
        b.split_tail_db = v(F_SPLIT_TAIL);
        b
    }

    fn states(&self) -> [BandState; BANDS] {
        std::array::from_fn(|i| self.band_state(i))
    }

    fn mode(&self) -> usize {
        self.v(P_PROCESSING_MODE) as usize
    }

    fn wanted_config(&self, params: &[BandState; BANDS]) -> LatencyConfig {
        let mode = self.mode();
        LatencyConfig {
            mode,
            order: lp_order(self.v(P_LINEAR_PHASE_QUALITY) as usize),
            drive: self.v(P_DRIVE) > 0.01,
            split: mode == 0 && params.iter().any(|b| b.split_on && b.active),
        }
    }

    fn split_latency(&self, rate: f64) -> usize {
        let order = (10 + (rate / 48000.0).log2().round() as i32).clamp(10, 13);
        let n = 1usize << order;
        n - 1 + split::FUTURE_FRAMES * n / 4
    }

    fn internal_latency(&self, c: &LatencyConfig, rate: f64) -> usize {
        let mut l = 0;
        if c.split {
            l += self.split_latency(rate);
        }
        if c.mode != 0 && c.mode != 4 {
            l += (1usize << c.order) / 2;
        }
        if c.drive {
            l += self.sat.latency;
        }
        l
    }

    fn host_latency(&self, internal: usize, oversampling: bool) -> usize {
        if oversampling {
            self.os_latency.ceil() as usize + (internal + 1) / 2
        } else {
            internal
        }
    }

    /// True once the kernel modes have a kernel for the current settings (tests and offline
    /// renders can wait on this). Always true in Zero Latency.
    pub fn kernel_ready(&self) -> bool {
        if self.mode() == 0 {
            return true;
        }
        match &self.lp {
            Some(lp) => lp.has_kernel(self.latched.order, self.latched.mode == 4) && lp.current_hash() == lp.last_submitted_hash(),
            None => false,
        }
    }

    /// Kernel designs the worker has finished.
    pub fn kernel_designs(&self) -> u64 {
        self.lp.as_ref().map_or(0, |l| l.completed())
    }

    /// Coefficient designs installed by the band engine (a settled path stops redesigning).
    pub fn design_updates(&self) -> u64 {
        self.engine.design_updates
    }

    /// Combined static magnitude in dB at `freqs` (product of the audible bands).
    pub fn response_db(&self, freqs: &[f32], out: &mut [f32]) {
        let s = self.states();
        curve::total_db(&s, self.rate, None, freqs, out);
    }

    /// Channel-aware curves: left-to-left and right-to-right gains of the placement matrix.
    pub fn response_channels_db(&self, freqs: &[f32], left: &mut [f32], right: &mut [f32]) {
        let s = self.states();
        curve::channel_db(&s, self.rate, None, freqs, left, right);
    }

    /// Band `i` alone, in dB; `gain_override_db` replaces its gain.
    pub fn band_response_db(&self, i: usize, gain_override_db: Option<f32>, freqs: &[f32], out: &mut [f32]) {
        if i >= BANDS {
            return;
        }
        let mut b = self.band_state(i);
        if let Some(g) = gain_override_db {
            b.gain_db = g;
        }
        curve::band_db(&b, self.rate, 0.0, freqs, out);
    }

    /// Split stream meters: transient peak, tonal peak, transient energy share.
    pub fn split_meters(&self) -> [f32; 3] {
        self.meters
    }

    fn set_rate(&mut self, oversampling: bool) {
        self.oversampling = oversampling;
        self.rate = self.host_rate * if oversampling { 2.0 } else { 1.0 };
        self.split_index = usize::from(oversampling);
        self.engine.prepare(self.rate);
        for s in self.splitters.iter_mut() {
            s.reset();
        }
        for h in self.host_os.iter_mut().chain(self.sat.hb.iter_mut()) {
            h.reset();
        }
        if let Some(lp) = self.lp.as_mut() {
            lp.reset();
            lp.force_resubmit();
        }
        for d in self.dry_delay.iter_mut() {
            d.fill(0.0);
        }
        self.fade.prepare(self.rate);
        let r = self.rate;
        self.g.input.reset(self.host_rate, 0.02);
        for (s, t) in [(&mut self.g.output, 0.020), (&mut self.g.width, 0.020), (&mut self.g.side, 0.020), (&mut self.g.pan, 0.020), (&mut self.g.bypass, 0.015)] {
            s.reset(r, t);
        }
        let params = self.states();
        let solo = any_solo(&params);
        self.engine.snap_smoothers(&params, solo);
        self.engine.apply_band_targets(&params, &[0.0; BANDS], true);
        self.g.input.set_current_and_target(self.v(P_INPUT_GAIN));
        self.g.output.set_current_and_target(self.v(P_OUTPUT_GAIN));
        self.g.width.set_current_and_target(self.v(P_STEREO_WIDTH));
        self.g.side.set_current_and_target(side_to_linear(self.v(P_SIDE_GAIN)));
        self.g.pan.set_current_and_target((self.v(P_OUTPUT_PAN) / 100.0).clamp(-1.0, 1.0));
        let bypassed = self.v(P_BYPASS) > 0.5;
        self.g.bypass.set_current_and_target(if bypassed { 0.0 } else { 1.0 });
        self.was_bypassed = bypassed;
        self.primed = false;
        self.kernel_wait = 0;
        self.engine.split_engaged = false;
        self.meters = [0.0, 0.0, 0.5];
    }

    fn request(&self, params: &[BandState; BANDS], solo: bool) -> Request {
        let mode = self.latched.mode;
        let mut r = Request {
            sample_rate: self.rate,
            order: self.latched.order,
            target: match mode {
                2 => Target::Analog,
                3 => Target::Mixed,
                4 => Target::AnalogZeroLatency,
                _ => Target::Linear,
            },
            crossover_hz: CROSSOVER_HZ[(self.v(P_MIX_CROSSOVER) as usize).min(4)],
            window_shape: self.v(P_WINDOW_SHAPE) as u8,
            window_parameter: self.v(P_WINDOW_PARAMETER) / 100.0,
            ..Request::default()
        };
        for b in params.iter() {
            if !b.audible(solo) || !supported_in_linear_phase(b.ty) {
                continue;
            }
            r.bands[r.count] = CurveBand {
                ty: b.ty,
                placement: b.stereo,
                pan: b.pan,
                frequency: clamp_frequency(b.frequency, self.rate),
                gain_db: b.gain_db,
                q: clamp_q(b.q),
                slope_db: b.slope_db,
                phase: b.phase,
            };
            r.count += 1;
        }
        r
    }

    fn auto_gain_compensation(&self, params: &[BandState; BANDS], solo: bool) -> f32 {
        const PROBES: [f64; 7] = [80.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0];
        let mut mags = [1.0f64; 7];
        for b in params.iter().filter(|b| b.audible(solo)) {
            let d = design_band(b.ty, b.frequency, b.gain_db, b.q, b.slope_db, self.rate);
            for (m, &f) in mags.iter_mut().zip(PROBES.iter()) {
                *m *= d.magnitude(f, self.rate);
            }
        }
        let sum: f64 = mags.iter().sum();
        (7.0 / sum.max(1.0e-6)).clamp(0.25, 4.0) as f32
    }

    fn core(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let params = self.states();
        let solo = any_solo(&params);
        self.engine.set_smoother_targets(&params, solo);
        self.g.output.set_target(self.v(P_OUTPUT_GAIN));
        self.g.width.set_target(self.v(P_STEREO_WIDTH));
        self.g.side.set_target(side_to_linear(self.v(P_SIDE_GAIN)));
        self.g.pan.set_target((self.v(P_OUTPUT_PAN) / 100.0).clamp(-1.0, 1.0));
        self.g.bypass.set_target(if self.v(P_BYPASS) > 0.5 { 0.0 } else { 1.0 });
        self.engine.set_ultra(self.v(P_ULTRA_PRECISION) > 0.5);
        self.engine.update_split_flags(&params);

        let wanted = self.wanted_config(&params);
        let mut latched_now = false;
        let was_primed = self.primed;
        if !self.primed {
            self.primed = true;
            self.latch(wanted);
        } else if !wanted.same_latency(&self.latched) {
            if !self.fade.pending {
                self.fade.pending = true;
            } else if self.fade.ready_to_switch() {
                self.latch(wanted);
                self.fade.pending = false;
                latched_now = true;
            }
        } else {
            if !self.os_pending {
                self.fade.pending = false;
            }
            self.latched = wanted;
        }
        let mode = self.latched.mode;
        let linear = mode != 0;
        let causal = mode == 4;
        let order = self.latched.order;

        let mut hold_silent = false;
        if latched_now && linear {
            if let Some(lp) = self.lp.as_mut() {
                lp.reset();
            }
            self.kernel_wait = (0.5 * self.rate) as i64;
        }
        if linear {
            let req = self.request(&params, solo);
            if let Some(lp) = self.lp.as_mut() {
                lp.submit(&req);
                lp.poll(order, causal);
            }
        }
        if self.kernel_wait > 0 {
            self.kernel_wait -= n as i64;
            let ready = self.lp.as_ref().is_some_and(|lp| lp.has_kernel(order, causal));
            if !linear || self.kernel_wait <= 0 {
                self.kernel_wait = 0;
                // Faster than real time (an offline render) the kernel can come later than the
                // wait: the delay is heard meanwhile, and the kernel blends in from it.
                if linear && !ready {
                    if let Some(lp) = self.lp.as_mut() {
                        lp.blend_from_dry();
                    }
                }
            } else if ready {
                self.kernel_wait = 0;
                latched_now = true;
            } else {
                hold_silent = true;
            }
        }
        self.engine.set_digital_plus(self.v(P_DIGITAL_PLUS) as usize);

        let latency = self.internal_latency(&self.latched, self.rate);
        if hold_silent {
            self.fade.gain = 0.0;
            self.fade.mute = self.fade.mute.max(n + 1);
        } else if latched_now || (was_primed && latency != self.reported) {
            self.fade.switched(latency);
        }
        self.reported = latency;

        let comp = if self.v(P_AUTO_GAIN) > 0.5 { Some(self.auto_gain_compensation(&params, solo)) } else { None };
        let mut off = 0;
        while off < n {
            let count = CHUNK.min(n - off);
            self.segment(&params, solo, linear, order, causal, comp, &mut l[off..off + count], &mut r[off..off + count]);
            off += count;
        }
        self.fade.apply(l, r);
    }

    fn latch(&mut self, c: LatencyConfig) {
        if c.split != self.engine.split_engaged {
            if let Some(s) = self.splitters.get_mut(self.split_index) {
                s.reset();
            }
            self.engine.reset_tail();
            self.engine.split_engaged = c.split;
            self.meters = [0.0, 0.0, 0.5];
        }
        self.latched = c;
    }

    #[allow(clippy::too_many_arguments)]
    fn segment(&mut self, params: &[BandState; BANDS], solo: bool, linear: bool, order: u32, causal: bool, comp: Option<f32>, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let settled_bypass = !self.g.bypass.is_smoothing() && self.g.bypass.current() <= 1.0e-6;
        if settled_bypass && self.reported == 0 {
            self.engine.settle(params, solo);
            self.g.output.set_current_and_target(self.v(P_OUTPUT_GAIN));
            self.g.width.set_current_and_target(self.v(P_STEREO_WIDTH));
            self.g.side.set_current_and_target(side_to_linear(self.v(P_SIDE_GAIN)));
            self.g.pan.set_current_and_target((self.v(P_OUTPUT_PAN) / 100.0).clamp(-1.0, 1.0));
            self.g.bypass.set_current_and_target(0.0);
            self.was_bypassed = true;
            return;
        }
        if self.was_bypassed {
            self.engine.reset();
            self.was_bypassed = false;
        }
        let delayed_dry = self.reported > 0;
        let fade_bypass = self.g.bypass.is_smoothing() || (delayed_dry && self.g.bypass.current() < 1.0 - 1.0e-6);
        let mut dry = [[0.0f32; CHUNK]; 2];
        if fade_bypass && !delayed_dry {
            dry[0][..n].copy_from_slice(l);
            dry[1][..n].copy_from_slice(r);
        }
        if delayed_dry {
            for i in 0..n {
                self.dry_delay[0][self.delay_write] = l[i];
                self.dry_delay[1][self.delay_write] = r[i];
                self.delay_write = (self.delay_write + 1) & (DELAY - 1);
            }
        }

        if linear {
            self.engine.skip_smoothers(n);
            if let Some(lp) = self.lp.as_mut() {
                lp.process(l, r, order, causal);
            }
        } else if self.engine.split_engaged {
            let src = self.v(P_SPLIT_SOURCE) as usize;
            let st = split::Settings {
                separation: self.values[P_SPLIT_SEPARATION] / 100.0,
                decay: self.values[P_SPLIT_DECAY] / 100.0,
                smoothing_ms: self.values[P_SPLIT_SMOOTHING],
                source_scale: split::SOURCE_SCALE[src.min(4)],
            };
            let s = &mut self.splitters[self.split_index];
            let [tl, tr] = &mut self.split_trans;
            let [sl, sr] = &mut self.split_tonal;
            s.process(l, r, &mut tl[..n], &mut tr[..n], &mut sl[..n], &mut sr[..n], &st);
            let (mut tp, mut sp, mut te, mut se) = (0.0f32, 0.0f32, 0.0f64, 0.0f64);
            for ch in [&tl[..n], &tr[..n]] {
                for &v in ch {
                    tp = tp.max(v.abs());
                    te += v as f64 * v as f64;
                }
            }
            for ch in [&sl[..n], &sr[..n]] {
                for &v in ch {
                    sp = sp.max(v.abs());
                    se += v as f64 * v as f64;
                }
            }
            self.meters[0] = if tp > self.meters[0] { tp } else { self.meters[0] * 0.92 };
            self.meters[1] = if sp > self.meters[1] { sp } else { self.meters[1] * 0.92 };
            if te + se > 1.0e-12 {
                self.meters[2] = (te / (te + se)) as f32;
            }
            let io = SplitIo { trans_l: &mut tl[..n], trans_r: &mut tr[..n], tonal_l: &mut sl[..n], tonal_r: &mut sr[..n], solo: self.values[P_SPLIT_SOLO] as u8 };
            self.engine.process_segment(params, l, r, Some(io), false, &mut |_, _, _, _, _| {});
        } else {
            self.engine.process_segment(params, l, r, None, false, &mut |_, _, _, _, _| {});
        }
        self.split_active = self.engine.split_engaged && self.engine.any_split_bands && !linear;

        self.width_and_gain(l, r);

        if let Some(c) = comp {
            for v in l.iter_mut().chain(r.iter_mut()) {
                *v *= c;
            }
        }
        if self.latched.drive {
            if self.v(P_DRIVE) > 0.01 {
                self.held_drive = self.v(P_DRIVE) / 100.0;
            }
            self.sat.process(l, r, self.held_drive);
        }
        if self.v(P_POLARITY) > 0.5 {
            for v in l.iter_mut().chain(r.iter_mut()) {
                *v = -*v;
            }
        }

        if fade_bypass {
            for i in 0..n {
                let wet = self.g.bypass.next();
                for (ch, x) in [&mut *l, &mut *r].into_iter().enumerate() {
                    let d = if delayed_dry {
                        let age = (n - 1 - i) + self.reported;
                        self.dry_delay[ch][(self.delay_write + DELAY - 1 - age.min(DELAY - 1)) & (DELAY - 1)]
                    } else {
                        dry[ch][i]
                    };
                    x[i] = d + wet * (x[i] - d);
                }
            }
        }
    }

    fn width_and_gain(&mut self, l: &mut [f32], r: &mut [f32]) {
        let g = &mut self.g;
        let moves = g.width.is_smoothing() || g.side.is_smoothing() || (g.width.current() - 100.0).abs() > 0.01 || (g.side.current() - 1.0).abs() > 1.0e-6;
        if moves {
            for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                let width = (if g.width.is_smoothing() { g.width.next() } else { g.width.current() }) / 100.0;
                let side_scale = if g.side.is_smoothing() { g.side.next() } else { g.side.current() };
                let mid = 0.5 * (*a + *b);
                let side = 0.5 * (*a - *b) * width * side_scale;
                *a = mid + side;
                *b = mid - side;
            }
        }
        if g.pan.is_smoothing() || g.pan.current().abs() > 1.0e-6 {
            for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                let p = if g.pan.is_smoothing() { g.pan.next() } else { g.pan.current() };
                *a *= (1.0 - p).min(1.0);
                *b *= (1.0 + p).min(1.0);
            }
        }
        if g.output.is_smoothing() {
            for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                let k = db_to_gain(g.output.next() as f64) as f32;
                *a *= k;
                *b *= k;
            }
        } else {
            let k = db_to_gain(g.output.current() as f64) as f32;
            if (k - 1.0).abs() > 1.0e-6 {
                for v in l.iter_mut().chain(r.iter_mut()) {
                    *v *= k;
                }
            }
        }
    }
}

impl Module for ParametricEq {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.host_rate = sample_rate as f64;
        if self.lp.is_none() {
            self.lp = Some(Box::new(LpEngine::new()));
        }
        self.splitters = vec![SplitSeparator::new(self.host_rate), SplitSeparator::new(2.0 * self.host_rate)];
        self.split_trans = [vec![0.0; CHUNK], vec![0.0; CHUNK]];
        self.split_tonal = [vec![0.0; CHUNK], vec![0.0; CHUNK]];
        self.dry_delay = [vec![0.0; DELAY], vec![0.0; DELAY]];
        self.os_buf = [vec![0.0; 2 * CHUNK], vec![0.0; 2 * CHUNK]];
        self.os_latency = round_trip_latency();
        self.sat.latency = round_trip_latency().floor() as usize;
        self.set_rate(self.v(P_OVERSAMPLE2X) > 0.5);
        self.prepared = true;
    }

    fn reset(&mut self) {
        let os = self.oversampling;
        if self.prepared {
            self.set_rate(os);
        }
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

    /// Host samples for the current settings. The audio follows after its fade.
    fn latency(&self) -> usize {
        let params = self.states();
        let c = self.wanted_config(&params);
        let os = self.v(P_OVERSAMPLE2X) > 0.5;
        let rate = self.host_rate * if os { 2.0 } else { 1.0 };
        self.host_latency(self.internal_latency(&c, rate), os)
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.prepared {
            return;
        }
        let n = left.len().min(right.len());
        let (left, right) = (&mut left[..n], &mut right[..n]);
        zero_non_finite(left);
        zero_non_finite(right);
        // Switching 2x re-prepares the path at the other rate: fade out first, switch in silence,
        // wait out the new delay, fade in.
        let os = self.v(P_OVERSAMPLE2X) > 0.5;
        if os != self.oversampling {
            if self.os_pending && self.fade.gain <= 0.0 {
                self.set_rate(os);
                self.os_pending = false;
                let params = self.states();
                let c = self.wanted_config(&params);
                let lat = self.internal_latency(&c, self.rate);
                self.fade.switched(lat);
            } else {
                self.os_pending = true;
                self.fade.pending = true;
            }
        } else if self.os_pending {
            self.os_pending = false;
        }

        self.g.input.set_target(self.v(P_INPUT_GAIN));
        if self.g.input.is_smoothing() {
            for (a, b) in left.iter_mut().zip(right.iter_mut()) {
                let k = db_to_gain(self.g.input.next() as f64) as f32;
                *a *= k;
                *b *= k;
            }
        } else {
            let k = db_to_gain(self.g.input.current() as f64) as f32;
            if (k - 1.0).abs() > 1.0e-6 {
                for v in left.iter_mut().chain(right.iter_mut()) {
                    *v *= k;
                }
            }
        }

        if !self.oversampling {
            self.core(left, right);
        } else {
            let mut off = 0;
            while off < n {
                let count = CHUNK.min(n - off);
                let [ul, ur] = &mut self.os_buf;
                for i in 0..count {
                    let a = self.host_os[0].up(left[off + i]);
                    let b = self.host_os[1].up(right[off + i]);
                    ul[2 * i] = a[0];
                    ul[2 * i + 1] = a[1];
                    ur[2 * i] = b[0];
                    ur[2 * i + 1] = b[1];
                }
                let mut bl = std::mem::take(&mut self.os_buf[0]);
                let mut br = std::mem::take(&mut self.os_buf[1]);
                self.core(&mut bl[..2 * count], &mut br[..2 * count]);
                for i in 0..count {
                    left[off + i] = self.host_os[0].down([bl[2 * i], bl[2 * i + 1]]);
                    right[off + i] = self.host_os[1].down([br[2 * i], br[2 * i + 1]]);
                }
                self.os_buf = [bl, br];
                off += count;
            }
        }

        // A host must never receive a non-finite sample (32 stacked +30 dB bells can overflow).
        if zero_non_finite(left) | zero_non_finite(right) {
            self.engine.reset();
        }
    }

    fn meters(&self) -> Meters {
        Meters::default()
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let vals = [
            if self.split_active { self.meters[0] } else { 0.0 },
            if self.split_active { self.meters[1] } else { 0.0 },
            if self.split_active { self.meters[2] } else { 0.5 },
            if self.split_active { 1.0 } else { 0.0 },
            self.latency() as f32,
        ];
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&vals[..n]);
        n
    }
}
