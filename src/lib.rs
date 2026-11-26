//! OpenLimiter: a modular mastering chain on OpenPluginSDK, with a GoodLookin DRI editor.
//!
//! Signal path, meant to be the last plugin before the file is printed:
//!
//! input -> input gain -> [input tap: peaks, loudness] -> modules, in the user's order
//!       -> dither (always last) -> [output tap: peaks, loudness, true peak] -> output
//!
//! There is no output gain: nothing may sit between the last limiter and the dither.
//! After the output tap come two listening aids, Autogain and Delta (`monitor.rs`), off by default.
//! The modules all implement the contract in `crates/api`; the first one is the look-ahead
//! limiter in `limiter.rs`.

mod chain;
mod ui;
mod limiter;
mod loudness;
mod monitor;
mod shared;

use chain::Chain;
use limiter::{make_kernel, true_peak, Limiter, Settings, PHASES, TAPS};
use loudness::Loudness;
use shared::{Entry, Loud, HIST_RATE, LOUD_RATE};
use olm_api::{Kind, Meters, Module, ParamDesc};
use std::sync::{Arc, LazyLock};

pub use ui::make_editor;
use openplugin::core::EditorState;
use shared::Shared;
use openplugin::prelude::*;

/// The first limiter's parameters. The last one, No Latency, is not part of a factory preset.
pub static PARAMS: [ParamInfo; 20] = [
    ParamInfo::new("gainL", "Gain L", -24.0, 24.0, 0.0, Unit::Db).with_steps(481),
    ParamInfo::new("gainR", "Gain R", -24.0, 24.0, 0.0, Unit::Db).with_steps(481),
    ParamInfo::new("threshold", "Threshold", -25.0, 0.1, 0.0, Unit::Db).with_steps(252),
    ParamInfo::new("softClip", "Soft Clip", -12.0, 24.0, 0.0, Unit::Db).with_steps(37),
    ParamInfo::new("releaseA", "Release A", -3.0, 3.0, 0.0, Unit::None).with_steps(7),
    ParamInfo::new("releaseM", "Release M", 20.0, 7000.0, 300.0, Unit::Ms).with_taper(Taper::Log),
    ParamInfo::new("autoRelease", "Auto Release", 0.0, 1.0, 1.0, Unit::Toggle),
    ParamInfo::new("output", "Output", -120.0, 0.0, 0.0, Unit::Db).with_steps(721),
    ParamInfo::new("delayL", "Delay L", 0.0, 300.0, 0.0, Unit::Ms),
    ParamInfo::new("delayR", "Delay R", 0.0, 300.0, 0.0, Unit::Ms),
    ParamInfo::new("delayFineL", "Delay Fine L", 0.0, 100.0, 0.0, Unit::None).with_steps(101),
    ParamInfo::new("delayFineR", "Delay Fine R", 0.0, 100.0, 0.0, Unit::None).with_steps(101),
    ParamInfo::new("phaseL", "Phase Inv L", 0.0, 1.0, 0.0, Unit::Toggle),
    ParamInfo::new("phaseR", "Phase Inv R", 0.0, 1.0, 0.0, Unit::Toggle),
    ParamInfo::new("delayUnit", "Delay Unit", 0.0, 3.0, 0.0, Unit::Choice)
        .with_choices(&["MS", "24", "25", "30"])
        .with_steps(4),
    ParamInfo::new("upsample", "Upsample", 0.0, 1.0, 1.0, Unit::Toggle),
    ParamInfo::new("link", "Link", 0.0, 1.0, 1.0, Unit::Toggle),
    ParamInfo::new("headroom", "Headroom", 0.0, 24.0, 0.0, Unit::Db).with_steps(5),
    ParamInfo::new("dither", "Dither", 8.0, 26.0, 25.0, Unit::None).with_steps(19),
    ParamInfo::new("noLatency", "No Latency", 0.0, 1.0, 0.0, Unit::Toggle),
];

/// Where everything sits in the host parameter list.
pub struct Layout {
    pub slots: Vec<SlotInfo>,
    /// The chain's parameters are `0..chain_count`.
    pub chain_count: usize,
    pub dither_base: usize,
    pub dither_count: usize,
    pub input_gain: [usize; 2],
    pub input_link: usize,
    /// Which slots the rack shows, one bit per slot. Saved, hidden from automation.
    pub rack_shown: usize,
    /// Listening aids for the whole plugin.
    pub autogain: usize,
    pub delta: usize,
}

pub struct SlotInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub base: usize,
    pub count: usize,
    pub on: usize,
    pub position: usize,
}

const DITHER_KEY: &str = "dither";

static TABLE: LazyLock<(Vec<ParamInfo>, Layout)> = LazyLock::new(|| {
    let chain = Chain::new(modules(None));
    let mut params = chain.param_infos();
    let slots: Vec<SlotInfo> = chain
        .slots()
        .iter()
        .map(|s| SlotInfo { key: s.key, label: s.label, base: s.base, count: s.count, on: s.on_index(), position: s.position_index() })
        .collect();
    let chain_count = params.len();
    let dither = requantiser::Requantiser::default();
    for (i, d) in dither.params().iter().enumerate() {
        let mut info = chain::info_for(DITHER_KEY, "Requantise", d);
        // The word length starts at Off: the output is untouched until a word length is chosen.
        if i == 0 {
            info.default = 0.0;
        }
        params.push(info);
    }
    let dither_count = params.len() - chain_count;
    let input_gain = [params.len(), params.len() + 1];
    params.push(ParamInfo::new("inputGainL", "Input Gain L", -24.0, 24.0, 0.0, Unit::Db).with_steps(481).with_group("Input"));
    params.push(ParamInfo::new("inputGainR", "Input Gain R", -24.0, 24.0, 0.0, Unit::Db).with_steps(481).with_group("Input"));
    let input_link = params.len();
    params.push(ParamInfo::new("inputLink", "Input Link", 0.0, 1.0, 1.0, Unit::Toggle).with_group("Input"));
    let rack_shown = params.len();
    // Bits of shown slots: an f32 holds every integer up to 2^24 exactly, so 24 slots fit.
    params.push(
        ParamInfo::new("rackShown", "Rack Modules", 0.0, 16_777_215.0, 1.0, Unit::None)
            .with_steps(16_777_216)
            .with_flags(ParamFlags::HIDDEN.union(ParamFlags::PRESET_EXCLUDE))
            .with_group("Editor"),
    );
    let autogain = params.len();
    params.push(ParamInfo::new("autogain", "Autogain", 0.0, 1.0, 0.0, Unit::Toggle).with_flags(ParamFlags::PRESET_EXCLUDE).with_group("Monitor"));
    let delta = params.len();
    params.push(ParamInfo::new("delta", "Delta", 0.0, 1.0, 0.0, Unit::Toggle).with_flags(ParamFlags::PRESET_EXCLUDE).with_group("Monitor"));
    (params, Layout { slots, chain_count, dither_base: chain_count, dither_count, input_gain, input_link, rack_shown, autogain, delta })
});

pub static LAYOUT: LazyLock<&'static Layout> = LazyLock::new(|| &TABLE.1);

/// The plugin's description.
pub static INFO: LazyLock<PluginInfo> = LazyLock::new(|| PluginInfo::new("com.opengrid.openlimiter", "OpenLimiter", "OpenGrid", env!("CARGO_PKG_VERSION"), Category::Dynamics, &TABLE.0).with_editor(true));

/// The movable modules, one slot each. Append only: the order fixes the host parameter indices.
fn modules(shared: Option<Arc<Shared>>) -> Vec<(&'static str, &'static str, Box<dyn Module>)> {
    vec![
        ("", "Classic Limiter", Box::new(Classic::new(shared))),
        ("shaper", "Punch & Energy", Box::new(shaper::Shaper::default())),
        ("modes", "Loudness MAX", Box::new(limiter_modes::LimiterModes::default())),
        ("eq", "SimplEQ", Box::new(tone_six::ToneSix::default())),
        ("sone", "DualSone", Box::new(comp_sone::DualSone::default())),
        ("clear", "Clean Peak", Box::new(limiter_clear::ClearLimiter::default())),
        ("dual", "Compressor / De-esser", Box::new(comp_dual::CompDual::default())),
        ("wall", "Final Wall", Box::new(limiter_wall::WallLimiter::default())),
        ("bands", "Multiband", Box::new(comp_bands::BandCompressor::default())),
        ("peq", "PrecisionEQ", Box::new(eq_parametric::ParametricEq::default())),
        ("deq", "DynEQ", Box::new(eq_dynamic::DynamicEq::default())),
        ("sup", "Suppressor", Box::new(reso_suppress::ResonanceSuppressor::default())),
        ("hcomp", "Harmonic Compressor", Box::new(comp_harmonic::HarmonicCompressor::default())),
        ("decomp", "Dynamics Restore", Box::new(dyn_decompress::Decompressor::default())),
        ("deexp", "Floor Lifter", Box::new(dyn_deexpand::DeExpander::default())),
        ("harm", "Overtone Generator", Box::new(harmonics::Harmonics::default())),
        ("smart", "Smart Limiter", Box::new(limiter_smart::SmartLimiter::default())),
    ]
}

/// Factory presets as full parameter lists: the chain's defaults with the limiter's values on top.
static PRESET_VALUES: LazyLock<Vec<Vec<f32>>> = LazyLock::new(|| {
    PRESETS
        .iter()
        .map(|(_, values)| INFO.params.iter().enumerate().map(|(i, p)| values.get(i).map_or(p.default, |v| p.clamp(*v))).collect())
        .collect()
});

/// Factory presets: plain values in `PARAMS` order.
pub(crate) static PRESETS: [(&str, [f32; 20]); 6] = [
    ("Default", [0.0, 0.0, 0.0, 0.0, 0.0, 300.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 25.0, 0.0]),
    ("Ceiling 0 dBFS", [0.0, 0.0, 0.0, 0.0, 0.0, 20.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 25.0, 0.0]),
    ("Ceiling -6 dBFS", [0.0, 0.0, -6.0, 0.0, 0.0, 20.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 25.0, 0.0]),
    ("Broadcast EU", [0.0, 0.0, -9.0, 24.0, 0.0, 20.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 25.0, 0.0]),
    ("Broadcast US", [0.0, 0.0, -6.0, 24.0, 0.0, 20.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 25.0, 0.0]),
    ("Soft Clip -12 dBFS", [0.0, 0.0, 0.0, -12.0, 0.0, 100.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 25.0, 0.0]),
];

/// Builds engine settings from the plain parameter values.
fn settings_from(v: &[f32; 20]) -> Settings {
    Settings {
        gain_l_db: v[0],
        gain_r_db: v[1],
        threshold_db: v[2],
        soft_clip_db: v[3],
        release_step: v[4].round() as i32,
        release_ms: v[5],
        auto_release: v[6] >= 0.5,
        output_db: v[7],
        delay_l_ms: v[8],
        delay_r_ms: v[9],
        fine_l: v[10].round() as u32,
        fine_r: v[11].round() as u32,
        phase_l: v[12] >= 0.5,
        phase_r: v[13] >= 0.5,
        upsample: v[15] >= 0.5,
        link: v[16] >= 0.5,
        dither_bits: v[18].round() as i32,
        no_latency: v[19] >= 0.5,
    }
}

/// The first limiter as a chain module: the look-ahead engine in `limiter.rs`.
struct Classic {
    values: [f32; 20],
    limiter: Option<Limiter>,
    shared: Option<Arc<Shared>>,
}

impl Classic {
    fn new(shared: Option<Arc<Shared>>) -> Self {
        // Start from the Default preset, which is what a user sees on load.
        let mut values = [0.0; 20];
        for (i, v) in PRESETS[0].1.iter().enumerate() {
            values[i] = PARAMS[i].clamp(*v);
        }
        Self { values, limiter: None, shared }
    }
}

/// `PARAMS` in the module contract's terms.
static CLASSIC_PARAMS: LazyLock<Vec<ParamDesc>> = LazyLock::new(|| {
    PARAMS
        .iter()
        .map(|p| {
            let kind = match p.unit {
                Unit::Db => Kind::Db,
                Unit::Ms => Kind::Ms,
                Unit::Toggle => Kind::Toggle,
                Unit::Choice => Kind::Choice(p.choices),
                _ => Kind::Plain,
            };
            ParamDesc { id: p.id, name: p.name, min: p.min, max: p.max, default: p.default, kind, log: p.taper == Taper::Log, steps: p.steps }
        })
        .collect()
});

impl Module for Classic {
    fn params(&self) -> &'static [ParamDesc] {
        &CLASSIC_PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.limiter = Some(Limiter::new(sample_rate, settings_from(&self.values), self.shared.clone()));
    }

    fn reset(&mut self) {
        if let Some(l) = self.limiter.as_mut() {
            l.reset();
        }
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(slot) = self.values.get_mut(index) else { return };
        *slot = PARAMS[index].clamp(value);
        if let Some(l) = self.limiter.as_mut() {
            l.set_settings(settings_from(&self.values));
        }
    }

    fn latency(&self) -> usize {
        self.limiter.as_ref().map_or(0, Limiter::latency)
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if let Some(l) = self.limiter.as_mut() {
            l.process(left, Some(right));
        }
    }

    fn meters(&self) -> Meters {
        Meters { reduction_db: self.limiter.as_ref().map_or(0.0, Limiter::reduction_db), ..Meters::default() }
    }
}

/// The two measuring points: after the input gain, and after the dither.
struct Taps {
    kernel: [[f32; TAPS]; PHASES],
    /// The last `TAPS` samples per channel, for inter-sample peaks: input, then output.
    window: [[[f32; TAPS]; 2]; 2],
    loud_in: Loudness,
    loud_out: Loudness,
    loud_len: usize,
    loud_count: usize,
    hop_len: usize,
    hop_count: usize,
    /// The hop being gathered: input peak, output peak, deepest reduction in dB.
    hop: (f32, f32, f32),
    /// Input peaks per hop, so the history can show the input the output came from.
    ring: Vec<f32>,
    ring_pos: usize,
    peaks: ([f32; 2], [f32; 2]),
    /// Sum of squares of the output per channel over the hop being gathered.
    sq: [f64; 2],
}

const RING_HOPS: usize = 1024;

impl Taps {
    fn new(sample_rate: f32) -> Self {
        Self {
            kernel: make_kernel(),
            window: [[[0.0; TAPS]; 2]; 2],
            loud_in: Loudness::new(sample_rate as f64),
            loud_out: Loudness::new(sample_rate as f64),
            loud_len: (sample_rate / LOUD_RATE).round().max(1.0) as usize,
            loud_count: 0,
            hop_len: (sample_rate / HIST_RATE).round().max(1.0) as usize,
            hop_count: 0,
            hop: (0.0, 0.0, 0.0),
            ring: vec![0.0; RING_HOPS],
            ring_pos: 0,
            peaks: ([0.0; 2], [0.0; 2]),
            sq: [0.0; 2],
        }
    }

    /// Back to the state `new` gives, without allocating (reset may run on the audio thread).
    fn reset(&mut self) {
        self.window = [[[0.0; TAPS]; 2]; 2];
        self.loud_in.reset();
        self.loud_out.reset();
        self.loud_count = 0;
        self.hop_count = 0;
        self.hop = (0.0, 0.0, 0.0);
        self.ring.fill(0.0);
        self.ring_pos = 0;
        self.peaks = ([0.0; 2], [0.0; 2]);
        self.sq = [0.0; 2];
    }

    /// Measures one point (0 input, 1 output) over a chunk; returns its inter-sample peak.
    fn measure(&mut self, point: usize, left: &[f32], right: &[f32], stereo: bool) -> f32 {
        let mut top = 0.0f32;
        for (&l, &r) in left.iter().zip(right) {
            for (c, v) in [l, r].into_iter().enumerate().take(if stereo { 2 } else { 1 }) {
                let w = &mut self.window[point][c];
                w.rotate_left(1);
                w[TAPS - 1] = v;
                let p = true_peak(&self.kernel, w);
                top = top.max(p);
                let slot = if point == 0 { &mut self.peaks.0[c] } else { &mut self.peaks.1[c] };
                *slot = slot.max(p);
            }
            if point == 1 {
                self.sq[0] += f64::from(l) * f64::from(l);
                let r = if stereo { r } else { l };
                self.sq[1] += f64::from(r) * f64::from(r);
            }
            let meter = if point == 0 { &mut self.loud_in } else { &mut self.loud_out };
            meter.process(l, if stereo { r } else { 0.0 }, stereo);
        }
        top
    }
}

/// Stereo in and out, with or without a sidechain key (stereo or mono).
static LAYOUTS: &[IoConfig] = &[
    IoConfig::STEREO,
    IoConfig { input: SpeakerLayout::Stereo, output: SpeakerLayout::Stereo, sidechain: Some(SpeakerLayout::Stereo) },
    IoConfig { input: SpeakerLayout::Stereo, output: SpeakerLayout::Stereo, sidechain: Some(SpeakerLayout::Mono) },
];

/// Pop-out windows of an instance read its shared state: they close before the instance goes.
impl Drop for OpenLimiter {
    fn drop(&mut self) {
        ui::pops::close_all(&self.shared);
    }
}

pub struct OpenLimiter {
    /// Plain values of every host parameter.
    values: Vec<f32>,
    chain: Chain,
    /// The word length stage. Always last, never moved.
    dither: requantiser::Requantiser,
    /// Input gain now and where it is heading, linear.
    gain: [f32; 2],
    gain_target: [f32; 2],
    gain_step: f32,
    taps: Option<Taps>,
    /// Autogain and Delta, after the output tap.
    monitor: Option<monitor::Monitor>,
    /// The right channel of a mono stream.
    scratch: Vec<f32>,
    max_block: usize,
    sample_rate: f32,
    /// Meters, history and loudness for the editor.
    shared: Arc<Shared>,
}

impl Default for OpenLimiter {
    fn default() -> Self {
        let shared = Arc::new(Shared::default());
        let mut plugin = Self {
            values: PRESET_VALUES[0].clone(),
            chain: Chain::new(modules(Some(shared.clone()))),
            dither: requantiser::Requantiser::default(),
            gain: [1.0; 2],
            gain_target: [1.0; 2],
            gain_step: 0.0,
            taps: None,
            monitor: None,
            scratch: Vec::new(),
            max_block: 0,
            sample_rate: 48_000.0,
            shared,
        };
        // The dither's own default word length differs from the plugin's.
        for i in 0..LAYOUT.dither_count {
            let index = LAYOUT.dither_base + i;
            plugin.dither.set_param(i, plugin.values[index]);
        }
        for (i, v) in plugin.values.iter().enumerate() {
            plugin.shared.set_value(i, *v);
        }
        plugin
    }
}

impl OpenLimiter {
    fn run(&mut self, io: &mut [&mut [f32]], sidechain: Option<&[&[f32]]>, frames: usize) {
        let Some(mut taps) = self.taps.take() else { return };
        let Some(mut monitor) = self.monitor.take() else {
            self.taps = Some(taps);
            return;
        };
        let Some((first, rest)) = io.split_first_mut() else {
            self.taps = Some(taps);
            self.monitor = Some(monitor);
            return;
        };
        let latency = self.latency();
        let stereo = !rest.is_empty();
        // The sidechain key, as two channels (a mono key feeds both). Too short: ignored.
        let key: Option<[&[f32]; 2]> = sidechain.and_then(|side| match side {
            [kl, kr, ..] if kl.len() >= frames && kr.len() >= frames => Some([&kl[..frames], &kr[..frames]]),
            [k] if k.len() >= frames => Some([&k[..frames], &k[..frames]]),
            _ => None,
        });
        self.shared.set_key_present(key.is_some());
        let left = &mut first[..frames];
        // NaN, infinity and denormal input become 0 (ST-2, ST-3).
        zero_abnormal(left, rest, frames);
        if self.shared.take_loud_reset() {
            taps.loud_in.reset();
            taps.loud_out.reset();
        }
        // How many history entries the chain delays the output by.
        let lag = ((latency + taps.hop_len / 2) / taps.hop_len).min(RING_HOPS - 1);
        let mut start = 0;
        while start < frames {
            let n = (frames - start).min(taps.hop_len - taps.hop_count).min(self.max_block);
            let l = &mut left[start..start + n];
            let r: &mut [f32] = match rest.first_mut() {
                Some(r) => &mut r[start..start + n],
                None => {
                    let r = &mut self.scratch[..n];
                    r.copy_from_slice(l);
                    r
                }
            };
            monitor.feed_raw(l, r);
            // Input gain, gliding to a new value and exact once there.
            for (c, buf) in [&mut *l, &mut *r].into_iter().enumerate() {
                let target = self.gain_target[c];
                for s in buf.iter_mut() {
                    let g = &mut self.gain[c];
                    if *g != target {
                        // In f32 the glide stalls short of the target, so it lands on it from
                        // within 0.001 dB (a step nobody hears).
                        *g += (target - *g) * self.gain_step;
                        if (*g - target).abs() <= 1e-4 * target.max(1e-3) {
                            *g = target;
                        }
                    }
                    if *g != 1.0 {
                        *s *= *g;
                    }
                }
            }
            let top_in = taps.measure(0, l, r, stereo);
            monitor.feed_input(l, r);
            let key = key.map(|[kl, kr]| [&kl[start..start + n], &kr[start..start + n]]);
            if let Some([kl, kr]) = key {
                let p = kl.iter().chain(kr.iter()).fold(0.0f32, |m, x| m.max(x.abs()));
                self.shared.raise_key(p);
            }
            self.chain.set_watch(self.shared.watched());
            self.chain.process_keyed(l, Some(&mut *r), key);
            for i in 0..self.chain.slots().len() {
                if let Some((m, latency, on)) = self.chain.slot_meters(i) {
                    self.shared.raise_slot_gr(i, if on { m.reduction_db.max(0.0) } else { 0.0 });
                    self.shared.set_slot_latency(i, latency);
                    self.shared.raise_slot_lift(i, self.chain.slot_lift(i));
                }
            }
            if let Some(w) = self.shared.watched() {
                let [pl, pr, ql, qr] = self.chain.taps();
                if !pl.is_empty() {
                    self.shared.push_taps([pl, pr], [ql, qr], key);
                }
                self.shared.set_view(w, self.chain.view());
            }
            let reduction = self.chain.reduction_db();
            self.dither.process(l, r);
            let top_out = taps.measure(1, l, r, stereo);
            self.shared.push_out(l, if stereo { &*r } else { &*l });
            monitor.process(l, r, latency);

            taps.hop = (taps.hop.0.max(top_in), taps.hop.1.max(top_out), taps.hop.2.max(reduction));
            taps.hop_count += n;
            if taps.hop_count >= taps.hop_len {
                let pos = taps.ring_pos;
                taps.ring[pos] = taps.hop.0;
                let shown = taps.ring[(pos + RING_HOPS - lag) % RING_HOPS];
                taps.ring_pos = (pos + 1) % RING_HOPS;
                let hop = taps.hop_count.max(1) as f64;
                let ms = [(taps.sq[0] / hop) as f32, (taps.sq[1] / hop) as f32];
                taps.sq = [0.0; 2];
                self.shared.push(Entry { input: shown, output: taps.hop.1, gain: 10f32.powf(-taps.hop.2 / 20.0), ms });
                taps.hop = (0.0, 0.0, 0.0);
                taps.hop_count = 0;
            }
            taps.loud_count += n;
            if taps.loud_count >= taps.loud_len {
                taps.loud_count = 0;
                let (a, b) = (taps.loud_in.readings(), taps.loud_out.readings());
                self.shared.push_loud(Loud {
                    momentary_in: a.momentary,
                    momentary_out: b.momentary,
                    short_in: a.short_term,
                    short_out: b.short_term,
                    integrated: b.integrated,
                    integrated_in: a.integrated,
                    range: b.range,
                    true_peak: b.true_peak,
                });
            }
            start += n;
        }
        // The kit and some hosts run without flush-to-zero: an f64 result too small for f32 lands
        // as a denormal (Shaper after the multiband compressor, soak seed 8).
        zero_abnormal(left, rest, frames);
        let (peak_in, peak_out) = std::mem::take(&mut taps.peaks);
        for c in 0..2 {
            let c_src = if stereo { c } else { 0 };
            self.shared.raise_peaks(c, peak_in[c_src], peak_out[c_src]);
        }
        self.shared.set_autogain_db(monitor.gain_db());
        self.taps = Some(taps);
        self.monitor = Some(monitor);
    }

    /// A fresh monitor stage with the current Autogain and Delta switches.
    fn new_monitor(&self) -> monitor::Monitor {
        monitor::Monitor::new(self.sample_rate, self.values[LAYOUT.autogain] >= 0.5, self.values[LAYOUT.delta] >= 0.5)
    }

    fn publish_timing(&self) {
        self.shared.set_timing(self.latency(), self.sample_rate);
    }
}

impl Plugin for OpenLimiter {
    fn state_is_parameters_only(&self) -> bool {
        true
    }

    fn info(&self) -> &'static PluginInfo {
        &INFO
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, _channels: usize) {
        self.sample_rate = sample_rate;
        self.max_block = max_block.max(1);
        self.chain.prepare(sample_rate, self.max_block);
        self.dither.prepare(sample_rate, self.max_block);
        self.taps = Some(Taps::new(sample_rate));
        self.monitor = Some(self.new_monitor());
        self.scratch = vec![0.0; self.max_block];
        // About 5 ms to reach a new input gain.
        self.gain_step = 1.0 - (-1.0 / (0.005 * sample_rate)).exp();
        self.gain = self.gain_target;
        self.publish_timing();
    }

    fn reset(&mut self) {
        self.chain.reset();
        self.dither.reset();
        if let Some(taps) = &mut self.taps {
            taps.reset();
        }
        if let Some(monitor) = &mut self.monitor {
            monitor.reset(self.values[LAYOUT.autogain] >= 0.5, self.values[LAYOUT.delta] >= 0.5);
        }
        self.gain = self.gain_target;
    }

    fn set_param(&mut self, id: &str, value: f32) -> bool {
        match INFO.params.iter().position(|p| p.id == id) {
            Some(i) => self.set_param_index(i, value),
            None => false,
        }
    }

    fn set_param_index(&mut self, index: usize, value: f32) -> bool {
        let Some(info) = INFO.params.get(index) else { return false };
        let value = info.clamp(value);
        self.values[index] = value;
        self.shared.set_value(index, value);
        let l = &*LAYOUT;
        let known = if index < l.chain_count {
            self.chain.set(index, value)
        } else if index < l.dither_base + l.dither_count {
            self.dither.set_param(index - l.dither_base, value);
            true
        } else if let Some(c) = l.input_gain.iter().position(|&g| g == index) {
            self.gain_target[c] = 10f32.powf(value / 20.0);
            true
        } else if index == l.autogain || index == l.delta {
            if let Some(m) = self.monitor.as_mut() {
                m.autogain = self.values[l.autogain] >= 0.5;
                m.delta = self.values[l.delta] >= 0.5;
            }
            true
        } else {
            index == l.input_link || index == l.rack_shown
        };
        self.publish_timing();
        known
    }

    fn param(&self, id: &str) -> Option<f32> {
        INFO.params.iter().position(|p| p.id == id).map(|i| self.values[i])
    }

    fn set_offline(&mut self, offline: bool) {
        self.chain.set_offline(offline);
    }

    fn supported_layouts(&self) -> &'static [IoConfig] {
        LAYOUTS
    }

    fn process(&mut self, io: &mut [&mut [f32]], frames: usize) {
        self.run(io, None, frames);
    }

    fn process_buses(&mut self, io: &mut [&mut [f32]], sidechain: Option<&[&[f32]]>, frames: usize) {
        self.run(io, sidechain, frames);
    }

    fn latency(&self) -> usize {
        self.chain.latency() + self.dither.latency()
    }

    fn editor_state(&self) -> Option<EditorState> {
        Some(self.shared.clone())
    }

    fn note_ports(&self) -> NotePorts {
        NotePorts { input: false, output: false }
    }

    fn preset_count(&self) -> usize {
        PRESETS.len()
    }

    fn preset_name(&self, index: usize) -> &'static str {
        PRESETS.get(index).map_or("", |p| p.0)
    }

    fn preset_values(&self, index: usize) -> Option<&'static [f32]> {
        PRESET_VALUES.get(index).map(|v| &v[..])
    }

    /// The LV2 URI, fixed from the first release on: LV2 sessions find the plugin by it.
    fn lv2_uri(&self) -> Option<&'static str> {
        Some("https://opengrid.studio/plugins/openlimiter")
    }

    /// AAX identity, fixed from the first release on: Pro Tools sessions find the plugin by it.
    fn aax_ids(&self) -> Option<openplugin::core::AaxIds> {
        Some(openplugin::core::AaxIds { manufacturer: *b"OpGd", product: *b"OLim", mono: *b"OLmM", stereo: *b"OLmS" })
    }
}

openplugin::targets::export_clap!(OpenLimiter, editor = make_editor);
openplugin::targets::export_lv2!(OpenLimiter, editor = make_editor);
openplugin::targets::export_shell!(OpenLimiter, editor = make_editor);

/// Every sample that is not a normal float (NaN, infinity, denormal, zero) becomes +0; normal
/// samples pass bit for bit, so the sound does not change.
fn zero_abnormal(left: &mut [f32], rest: &mut [&mut [f32]], frames: usize) {
    for s in left.iter_mut().chain(rest.iter_mut().flat_map(|c| c.iter_mut().take(frames))) {
        if !s.is_normal() {
            *s = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Plugin::process` on owned buffers: the SDK takes the channels as slices.
    fn process(p: &mut OpenLimiter, io: &mut [Vec<f32>], frames: usize) {
        let mut chans: Vec<&mut [f32]> = io.iter_mut().map(|c| c.as_mut_slice()).collect();
        Plugin::process(p, &mut chans, frames);
    }

    fn process_buses(p: &mut OpenLimiter, io: &mut [Vec<f32>], key: &[Vec<f32>], frames: usize) {
        let mut chans: Vec<&mut [f32]> = io.iter_mut().map(|c| c.as_mut_slice()).collect();
        let key: Vec<&[f32]> = key.iter().map(|c| c.as_slice()).collect();
        Plugin::process_buses(p, &mut chans, Some(&key), frames);
    }
    use crate::limiter::{db_to_lin, lookahead_samples};

    const SR: f32 = 48_000.0;
    const PI: f32 = std::f32::consts::PI;

    /// The Default preset with everything that colours the sound switched off.
    fn neutral() -> [f32; 20] {
        let mut v = PRESETS[0].1;
        v[3] = 24.0; // soft clip off
        v[6] = 0.0; // auto release off
        v[15] = 0.0; // upsample off
        v[18] = 25.0; // dither off
        v
    }

    fn make_limiter(v: &[f32; 20]) -> Limiter {
        Limiter::new(SR, settings_from(v), None)
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    /// The peak of the waveform a converter would rebuild from `buf`: 16 points per sample
    /// from a long windowed sinc. Independent of the limiter's own interpolator.
    fn reconstructed_peak(buf: &[f32]) -> f32 {
        let half = 32isize;
        let mut pk = 0.0f32;
        for n in half as usize..buf.len() - half as usize {
            for step in 0..16 {
                let frac = step as f32 / 16.0;
                let mut v = 0.0f32;
                for j in -half + 1..=half {
                    let t = j as f32 - frac;
                    let sinc = if t.abs() < 1e-6 { 1.0 } else { (PI * t).sin() / (PI * t) };
                    let window = 0.5 + 0.5 * (PI * t / half as f32).cos();
                    v += buf[(n as isize + j) as usize] * sinc * window;
                }
                pk = pk.max(v.abs());
            }
        }
        pk
    }

    #[test]
    fn ceiling_holds_with_and_without_upsample() {
        for up in [0.0, 1.0] {
            for auto in [0.0, 1.0] {
                let mut v = neutral();
                v[2] = -1.0;
                v[6] = auto;
                v[15] = up;
                let mut l = make_limiter(&v);
                let mut buf: Vec<f32> = (0..48_000).map(|i| 2.0 * (2.0 * PI * 997.0 * i as f32 / SR).sin()).collect();
                l.process(&mut buf, None);
                let ceil = 10f32.powf(-1.0 / 20.0);
                assert!(peak(&buf) <= ceil + 1e-5, "upsample {up}, auto {auto}: peak {} above ceiling {ceil}", peak(&buf));
            }
        }
    }

    #[test]
    fn upsample_removes_inter_sample_peaks() {
        // A sine at a quarter of the sample rate, sampled 45 degrees off its crests: every
        // sample is 0.99, and the waveform between them reaches 1.4 (+2.9 dBFS).
        let tone: Vec<f32> = (0..24_000).map(|i| 1.4 * (2.0 * PI * 0.25 * i as f32 + PI / 4.0).sin()).collect();
        assert!(peak(&tone) < 1.0);
        let run = |up: f32| {
            let mut v = neutral();
            v[15] = up;
            let mut l = make_limiter(&v);
            let mut buf = tone.clone();
            l.process(&mut buf, None);
            reconstructed_peak(&buf[4_000..])
        };
        let (off, on) = (run(0.0), run(1.0));
        assert!(off > 1.3, "sample limiting leaves the inter-sample peak: {off}");
        assert!(on < 1.02, "upsample holds the rebuilt waveform at the ceiling: {on}");
    }

    #[test]
    fn latency_is_the_manuals_look_ahead_at_every_rate() {
        for (rate, samples) in [(44_100.0, 37), (48_000.0, 39), (88_200.0, 64), (96_000.0, 69)] {
            assert_eq!(lookahead_samples(rate), samples);
            for up in [0.0, 1.0] {
                let mut v = neutral();
                v[15] = up;
                assert_eq!(Limiter::new(rate, settings_from(&v), None).latency(), samples, "{rate} Hz, upsample {up}");
            }
        }
    }

    #[test]
    fn impulse_appears_at_reported_latency() {
        for up in [0.0, 1.0] {
            let mut v = neutral();
            v[15] = up;
            let mut l = make_limiter(&v);
            let lat = l.latency();
            let mut buf = vec![0.0f32; 1024];
            buf[0] = 0.5;
            l.process(&mut buf, None);
            assert!((buf[lat] - 0.5).abs() < 1e-6, "upsample {up}");
            assert!(buf[..lat].iter().all(|s| s.abs() < 1e-9));
        }
    }

    #[test]
    fn no_latency_clamps_at_once() {
        let mut v = neutral();
        v[19] = 1.0;
        v[2] = -6.0;
        let mut l = make_limiter(&v);
        assert_eq!(l.latency(), 0);
        let mut buf = vec![0.0f32; 16];
        buf[0] = 1.0;
        l.process(&mut buf, None);
        assert!((buf[0] - 10f32.powf(-6.0 / 20.0)).abs() < 1e-4);
    }

    #[test]
    fn passthrough_is_bit_exact_below_threshold() {
        // Also with bit-shift gains that sum to 0 dB, and with auto release and upsample on.
        for (gain, output, auto, up) in [(0.0, 0.0, 0.0, 0.0), (0.0, 0.0, 1.0, 1.0), (-6.0, 0.0, 1.0, 1.0), (-12.0, 0.0, 0.0, 1.0)] {
            let mut v = neutral();
            (v[0], v[7], v[6], v[15]) = (gain, output, auto, up);
            let mut l = make_limiter(&v);
            let lat = l.latency();
            let input: Vec<f32> = (0..8192).map(|i| ((i * 7919 % 2001) as f32 / 2000.0 - 0.5) * 0.2).collect();
            let mut out = input.clone();
            l.process(&mut out, None);
            let shift = db_to_lin(gain);
            for i in lat..input.len() {
                assert_eq!(out[i], input[i - lat] * shift, "gain {gain}, sample {i}");
            }
        }
        assert_eq!(db_to_lin(6.0), 2.0);
        assert_eq!(db_to_lin(-18.0), 0.125);
    }

    #[test]
    fn threshold_off_does_not_limit() {
        let mut v = neutral();
        v[2] = PARAMS[2].max;
        let mut l = make_limiter(&v);
        let mut buf: Vec<f32> = (0..4800).map(|i| 3.0 * (2.0 * PI * 440.0 * i as f32 / SR).sin()).collect();
        l.process(&mut buf, None);
        assert!(peak(&buf) > 2.9);
    }

    #[test]
    fn soft_clip_works_before_the_limiter_and_the_ceiling_still_holds() {
        // Soft clip alone (limiter off): bends the peaks, never past 6 dB over its level.
        let mut v = neutral();
        v[2] = PARAMS[2].max;
        v[3] = -6.0;
        let mut l = make_limiter(&v);
        let tone: Vec<f32> = (0..4800).map(|i| 2.0 * (2.0 * PI * 440.0 * i as f32 / SR).sin()).collect();
        let mut buf = tone.clone();
        l.process(&mut buf, None);
        let knee = 10f32.powf(-6.0 / 20.0);
        assert!(peak(&buf) > knee && peak(&buf) <= 2.0 * knee + 1e-4, "{}", peak(&buf));
        // With the limiter on, the output still stops at the threshold.
        v[2] = -3.0;
        let mut l = make_limiter(&v);
        let mut buf = tone;
        l.process(&mut buf, None);
        assert!(peak(&buf) <= 10f32.powf(-3.0 / 20.0) + 1e-5);
    }

    #[test]
    fn auto_release_forgets_one_peak_faster_than_sustained_limiting() {
        // Seconds until the gain is back within 0.5 dB, after `loud` seconds of 12 dB over.
        let recovery = |loud: f32| {
            let mut v = neutral();
            v[6] = 1.0;
            let mut l = make_limiter(&v);
            let n_loud = (loud * SR) as usize;
            let total = n_loud + 2 * SR as usize;
            let mut buf: Vec<f32> = (0..total).map(|i| if i < n_loud { 4.0 } else { 0.5 }).collect();
            l.process(&mut buf, None);
            let lat = l.latency();
            let back = (n_loud + lat..total).find(|&i| buf[i] > 0.5 * 10f32.powf(-0.5 / 20.0)).expect("recovers");
            (back - n_loud - lat) as f32 / SR
        };
        let (brief, sustained) = (recovery(0.002), recovery(1.0));
        assert!(brief < 0.15, "one peak: {brief} s");
        assert!(sustained > 3.0 * brief, "sustained {sustained} s, one peak {brief} s");
    }

    #[test]
    fn channel_delay_and_reported_latency() {
        let mut v = neutral();
        v[8] = 1.0; // 1 ms left = 48 samples
        let mut l = make_limiter(&v);
        let base = lookahead_samples(SR);
        assert_eq!(l.latency(), base + 48);
        let mut left = vec![0.0f32; 1024];
        let mut right = vec![0.0f32; 1024];
        left[0] = 0.25;
        right[0] = 0.25;
        l.process(&mut left, Some(&mut right));
        assert!((left[base + 48] - 0.25).abs() < 1e-6);
        assert!((right[base] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn every_module_runs_in_one_chain() {
        let ids: std::collections::HashSet<&str> = INFO.params.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), INFO.params.len(), "parameter ids are unique");
        println!("{} host parameters", INFO.params.len());

        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 256, 2);
        let slots: Vec<(usize, &str)> = plugin.chain.slots().iter().map(|s| (s.on_index(), s.label)).collect();
        assert_eq!(slots.len(), 17);
        let mut total = 0;
        for (on, label) in &slots {
            assert!(plugin.set_param_index(*on, 1.0));
            let now = plugin.latency();
            println!("{label}: {} samples", now - total);
            total = now;
        }
        // Programme-like noise with a level step, through everything at once.
        let mut seed = 0x2545_f491u32;
        let mut noise = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32 * 2.0 - 1.0
        };
        let mut peak = 0.0f32;
        for block in 0..400 {
            let level = if block % 100 < 50 { 0.05 } else { 0.9 };
            let mut io = vec![(0..256).map(|_| noise() * level).collect::<Vec<f32>>(), (0..256).map(|_| noise() * level).collect()];
            process(&mut plugin, &mut io, 256);
            assert!(io.iter().flatten().all(|s| s.is_finite()), "block {block}");
            peak = peak.max(io.iter().flatten().fold(0.0f32, |m, s| m.max(s.abs())));
        }
        assert!(peak > 0.01 && peak < 4.0, "peak {peak}");
        // Moving a module changes nothing about what exists: same latency.
        let position = plugin.chain.slots()[8].position_index();
        assert!(plugin.set_param_index(position, 1.0));
        assert_eq!(plugin.latency(), total);
        // Both measuring points reported.
        assert!(plugin.shared.head() > 0, "history");
        assert!(plugin.shared.loud_head() > 0, "loudness");
        assert_eq!(plugin.shared.timing().0 as usize, total);
    }

    /// Input gain then dither, nothing else: what the two taps see and what comes out.
    #[test]
    fn input_gain_and_dither_frame_the_chain() {
        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 512, 2);
        plugin.set_param_index(LAYOUT.slots[0].on, 0.0);
        assert_eq!(plugin.latency(), 0);
        // Nothing on, word length off: bit for bit.
        let tone: Vec<f32> = (0..4800).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
        let mut io = vec![tone.clone(), tone.clone()];
        process(&mut plugin, &mut io, 4800);
        assert_eq!(io[0], tone);
        // +6 dB of input gain, once it has glided there.
        plugin.set_param("inputGainL", 6.0);
        plugin.set_param("inputGainR", 6.0);
        let g = 10f32.powf(6.0 / 20.0);
        for _ in 0..4 {
            let mut io = vec![tone.clone(), tone.clone()];
            process(&mut plugin, &mut io, 4800);
        }
        let mut io = vec![tone.clone(), tone.clone()];
        process(&mut plugin, &mut io, 4800);
        let worst = io[0].iter().zip(&tone).map(|(o, i)| (o - g * i).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-6, "worst {worst}, first {:?} vs {:?}", &io[0][..3], &tone[..3]);
        // 16-bit word length: every sample lands on the 16-bit grid.
        plugin.set_param_index(LAYOUT.dither_base, 5.0);
        let mut io = vec![tone.clone(), tone.clone()];
        process(&mut plugin, &mut io, 4800);
        assert!(io[0].iter().all(|s| (s * 32768.0 - (s * 32768.0).round()).abs() < 1e-3));
        assert!(plugin.shared.take_peaks().1[0] > 0.5, "output meter after the dither");
    }

    #[test]
    fn the_watched_slot_publishes_its_audio_key_and_meters() {
        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 512, 2);
        plugin.set_param("inputGainL", 12.0);
        plugin.set_param("inputGainR", 12.0);
        plugin.shared.watch(Some(0));
        let tone: Vec<f32> = (0..4800).map(|i| 0.9 * (i as f32 * 0.05).sin()).collect();
        let key: Vec<f32> = (0..4800).map(|i| 0.25 * (i as f32 * 0.011).cos()).collect();
        for _ in 0..3 {
            let mut io = vec![tone.clone(), tone.clone()];
            process_buses(&mut plugin, &mut io, std::slice::from_ref(&key), 4800);
        }
        assert!(plugin.shared.take_key().0, "key bus seen");
        let mut pre = vec![0.0; 1024];
        plugin.shared.read_tap(0, &mut pre);
        assert!(pre.iter().any(|&x| x.abs() > 1.0), "pre tap carries the +12 dB input");
        let mut k = vec![0.0; 1024];
        plugin.shared.read_tap(5, &mut k);
        assert_eq!(&k[..], &key[4800 - 1024..], "mono key fed to both key channels, aligned at slot 0");
        let (gr, latency) = plugin.shared.take_slot(0);
        assert!(gr > 3.0, "limiter reduces: {gr}");
        assert_eq!(latency as usize, plugin.chain.slot_meters(0).unwrap().1);
        // Unwatched: no more taps.
        plugin.shared.watch(None);
        let mut io = vec![vec![0.0; 4800], vec![0.0; 4800]];
        process(&mut plugin, &mut io, 4800);
        plugin.shared.read_tap(0, &mut pre);
        assert!(pre.iter().any(|&x| x.abs() > 1.0), "old taps kept, nothing new written");
        assert!(!plugin.shared.take_key().0);
    }

/// Audio-thread cost of each module alone in the whole plug-in (taps, loudness, requantiser
    /// included), and of the shell with every module off. 48 kHz stereo, 512-sample blocks, 20 s
    /// of programme-like audio after a 1 s warm-up. `cargo test --release --lib dsp_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dsp_cost() {
        let (sr, block) = (48_000.0f32, 512usize);
        let secs: usize = std::env::var("SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
        let mut seed = 0x2545_f491u32;
        let n = sr as usize * (secs + 1);
        let sig: Vec<f32> = (0..n)
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let t = i as f32 / sr;
                let env = if (t * 2.0).fract() < 0.1 { 1.0 } else { 0.35 };
                env * (0.5 * (t * 55.0 * std::f32::consts::TAU).sin() + 0.25 * (t * 1234.0 * std::f32::consts::TAU).sin() + 0.3 * (seed as f32 / u32::MAX as f32 - 0.5))
            })
            .collect();
        let slots: Vec<(usize, &str)> = OpenLimiter::default().chain.slots().iter().map(|s| (s.on_index(), s.label)).collect();
        let mut runs: Vec<(String, Option<usize>)> = vec![("shell only".into(), None)];
        runs.extend(slots.iter().map(|(on, label)| (label.to_string(), Some(*on))));
        let only = std::env::var("MODULE").ok();
        for (label, on) in runs {
            if only.as_deref().is_some_and(|m| !label.to_lowercase().contains(&m.to_lowercase())) {
                continue;
            }
            let mut plugin = OpenLimiter::default();
            plugin.prepare(sr, block, 2);
            for (o, _) in &slots {
                plugin.set_param_index(*o, 0.0);
            }
            if let Some(on) = on {
                plugin.set_param_index(on, 1.0);
            }
            // HEAVY=1: every oversampling, upsampling and true-peak switch at its maximum.
            let mut heavy = Vec::new();
            if std::env::var("HEAVY").is_ok() {
                for (i, p) in INFO.params.iter().enumerate() {
                    let tail = p.id.rsplit_once('.').map_or(p.id, |(_, t)| t).to_ascii_lowercase();
                    if ["oversampl", "upsampl", "truepeak", "oversample"].iter().any(|k| tail.starts_with(k) || tail.contains(k)) && !tail.contains("offline") {
                        plugin.set_param_index(i, p.max);
                        heavy.push(format!("{}={}", p.id, p.max));
                    }
                }
            }
            if !heavy.is_empty() && on.is_none() {
                println!("heavy: {}", heavy.join(", "));
            }
            let mut io = vec![vec![0.0f32; block], vec![0.0f32; block]];
            let mut time = std::time::Duration::ZERO;
            for (k, start) in (0..n - block).step_by(block).enumerate() {
                io[0].copy_from_slice(&sig[start..start + block]);
                io[1].copy_from_slice(&sig[start..start + block]);
                let t0 = std::time::Instant::now();
                process(&mut plugin, &mut io, block);
                if k as f32 * block as f32 >= sr {
                    time += t0.elapsed();
                }
            }
            let rt = 100.0 * time.as_secs_f64() / secs as f64;
            println!("{label:14} {:7.2} % of one core in realtime ({:.3} ms per 512-sample block)", rt, rt / 100.0 * 1000.0 * block as f64 / sr as f64);
        }
    }

    fn render(plugin: &mut OpenLimiter, tone: &[f32], blocks: usize) -> Vec<f32> {
        let mut last = Vec::new();
        for _ in 0..blocks {
            let mut io = vec![tone.to_vec(), tone.to_vec()];
            process(plugin, &mut io, tone.len());
            last = io.swap_remove(0);
        }
        last
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn autogain_and_delta_are_off_and_leave_the_audio_alone() {
        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 512, 2);
        assert_eq!(INFO.params[LAYOUT.autogain].default, 0.0);
        assert_eq!(INFO.params[LAYOUT.delta].default, 0.0);
        assert!(INFO.params[LAYOUT.autogain].flags.contains(ParamFlags::PRESET_EXCLUDE));
        plugin.set_param_index(LAYOUT.slots[0].on, 0.0);
        let tone: Vec<f32> = (0..4800).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
        assert_eq!(render(&mut plugin, &tone, 3), tone, "bit for bit with both off");
        // Switched on and off again: back to bit for bit once the glide is over.
        plugin.set_param("autogain", 1.0);
        plugin.set_param("delta", 1.0);
        render(&mut plugin, &tone, 10);
        plugin.set_param("autogain", 0.0);
        plugin.set_param("delta", 0.0);
        render(&mut plugin, &tone, 10);
        assert_eq!(render(&mut plugin, &tone, 1), tone);
    }

    #[test]
    fn delta_is_the_output_minus_the_aligned_input() {
        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 512, 2);
        let tone: Vec<f32> = (0..4800).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
        // The first limiter below its threshold is a pure delay: nothing changes, delta is silent.
        assert!(plugin.latency() > 0);
        plugin.set_param("delta", 1.0);
        let d = render(&mut plugin, &tone, 10);
        assert!(d.iter().all(|x| x.abs() < 1e-6), "delta of an untouched signal: {}", rms(&d));
        // +12 dB into the limiter: the delta is what it took away. A twin without Delta, fed
        // the same audio, gives the output to compare with; the input is known.
        let mut twin = OpenLimiter::default();
        twin.prepare(SR, 512, 2);
        render(&mut twin, &tone, 10);
        for p in [&mut plugin, &mut twin] {
            p.set_param("inputGainL", 12.0);
            p.set_param("inputGainR", 12.0);
        }
        // A sweep, so no block repeats the one before.
        let sweep: Vec<f32> = (0..48_000).map(|i| 0.3 * (i as f32 * (0.01 + i as f32 * 2e-6)).sin()).collect();
        let d = render(&mut plugin, &sweep, 1);
        let out = render(&mut twin, &sweep, 1);
        let g = 10f32.powf(12.0 / 20.0);
        let lat = plugin.latency();
        // After the input gain has glided in (5 ms), delta = output - gain x input, delayed.
        for j in 2400.max(lat)..sweep.len() {
            let expect = out[j] - g * sweep[j - lat];
            assert!((d[j] - expect).abs() < 1e-5, "sample {j}: {} vs {expect}", d[j]);
        }
        assert!(rms(&d) > 0.1, "the limiter took something away");
    }

    #[test]
    fn autogain_matches_the_heard_loudness_to_the_input() {
        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 512, 2);
        // A loud master: +12 dB into the limiter at a -6 dB ceiling.
        plugin.set_param("inputGainL", 12.0);
        plugin.set_param("inputGainR", 12.0);
        plugin.set_param("threshold", -6.0);
        let tone: Vec<f32> = (0..4800).map(|i| 0.1 * (i as f32 * 0.05).sin()).collect();
        let out = render(&mut plugin, &tone, 20);
        let louder = 20.0 * (rms(&out) / rms(&tone)).log10();
        assert!(louder > 5.0, "the output is louder: {louder}");
        plugin.set_param("autogain", 1.0);
        let heard = render(&mut plugin, &tone, 200);
        let off = 20.0 * (rms(&heard) / rms(&tone)).log10();
        assert!(off.abs() < 0.3, "heard within 0.3 dB of the input: {off}");
        let shown = plugin.shared.autogain_db();
        assert!((shown + louder).abs() < 0.3, "shown {shown} vs {louder}");
        // The meters still read the real output.
        let (_, out_peaks) = plugin.shared.take_peaks();
        assert!(out_peaks[0] > 0.4, "output meter before Autogain: {}", out_peaks[0]);
    }

    #[test]
    fn autogain_holds_through_silence() {
        let mut plugin = OpenLimiter::default();
        plugin.prepare(SR, 512, 2);
        plugin.set_param("inputGainL", 12.0);
        plugin.set_param("inputGainR", 12.0);
        plugin.set_param("autogain", 1.0);
        let tone: Vec<f32> = (0..4800).map(|i| 0.1 * (i as f32 * 0.05).sin()).collect();
        render(&mut plugin, &tone, 100);
        let before = plugin.shared.autogain_db();
        render(&mut plugin, &vec![0.0; 4800], 200);
        let after = plugin.shared.autogain_db();
        assert!(before < -6.0 && (after - before).abs() < 0.5, "{before} then {after}");
    }

    /// Second route for the host sweep's DEAD parameters (`build/dead.txt`, one index per line):
    /// each module on, all of its parameters random in up to 40 states, each suspect at its ends.
    /// A fresh instance per render, so nothing carries over. Writes `build/dead-check.tsv`.
    /// Run with `--ignored`.
    #[test]
    #[ignore]
    fn recheck_dead_parameters() {
        let dir = env!("CARGO_MANIFEST_DIR");
        let Ok(text) = std::fs::read_to_string(format!("{dir}/build/dead.txt")) else { return };
        let dead: Vec<usize> = text.lines().filter_map(|l| l.trim().parse().ok()).collect();
        let n = (1.5 * SR) as usize;
        let signal: Vec<[f32; 2]> = {
            let mut seed = 1u32;
            (0..n)
                .map(|i| {
                    let t = i as f32 / SR;
                    let k = 1.5 / (1000f32).ln();
                    let sweep = (2.0 * PI * 20.0 * k * ((t / 1.5 * (1000f32).ln()).exp() - 1.0)).sin();
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let noise = ((seed >> 8) as f32 / 8_388_608.0 - 1.0) * 0.5 * if t % 0.75 < 0.2 { 1.0 } else { 0.0 };
                    let pulse = if t % 0.5 < 0.002 { 1.0 } else { 0.0 };
                    [0.25 * (sweep + noise + pulse), 0.25 * (sweep * 0.7 + noise * 0.5 + pulse)]
                })
                .collect()
        };
        // (latency, left then right). Compared after aligning the latencies, as a host does.
        let render = |values: &[f32]| -> (usize, Vec<f32>) {
            let mut p = OpenLimiter::default();
            for (i, v) in values.iter().enumerate() {
                p.set_param_index(i, *v);
            }
            p.prepare(SR, 512, 2);
            let mut l: Vec<f32> = signal.iter().map(|s| s[0]).collect();
            let mut r: Vec<f32> = signal.iter().map(|s| s[1]).collect();
            for k in (0..n).step_by(512) {
                let e = (k + 512).min(n);
                let mut io = vec![l[k..e].to_vec(), r[k..e].to_vec()];
                process(&mut p, &mut io, e - k);
                l[k..e].copy_from_slice(&io[0]);
                r[k..e].copy_from_slice(&io[1]);
            }
            l.extend(r);
            (p.latency(), l)
        };
        let differs = |a: &(usize, Vec<f32>), b: &(usize, Vec<f32>)| {
            let len = n - a.0.max(b.0);
            (0..2).any(|c| (0..len).any(|i| (a.1[c * n + i + a.0] - b.1[c * n + i + b.0]).abs() > 1e-5))
        };
        let defaults: Vec<f32> = INFO.params.iter().map(|p| p.default).collect();
        let mut out = String::from("index\tid\tresult\tstate\n");
        for (si, slot) in LAYOUT.slots.iter().enumerate() {
            let mut left: Vec<usize> = dead.iter().copied().filter(|&d| d >= slot.base && d < slot.base + slot.count).collect();
            if left.is_empty() {
                continue;
            }
            let mut found: Vec<(usize, usize)> = Vec::new();
            let mut rng = 0x9e37_79b9u32 ^ si as u32;
            for state in 0..40 {
                if left.is_empty() {
                    break;
                }
                let mut values = defaults.clone();
                values[slot.on] = 1.0;
                if si != 0 {
                    values[LAYOUT.slots[0].on] = 0.0;
                }
                let span = slot.base..slot.base + slot.count;
                for (p, v) in INFO.params[span.clone()].iter().zip(&mut values[span]) {
                    rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    // A soloed band hides every other band: solo switches stay at their defaults.
                    if !p.id.to_ascii_lowercase().contains("solo") {
                        *v = p.from_normalized((rng >> 8) as f32 / 16_777_216.0);
                    }
                }
                let reference = render(&values);
                if reference.1.iter().all(|x| x.abs() < 1.6e-5) || !reference.1.iter().all(|x| x.is_finite()) {
                    continue;
                }
                let acted: Vec<usize> = std::thread::scope(|sc| {
                    let chunks: Vec<Vec<usize>> = left.chunks(left.len().div_ceil(8)).map(|c| c.to_vec()).collect();
                    let handles: Vec<_> = chunks
                        .into_iter()
                        .map(|chunk| {
                            let (values, reference, render) = (&values, &reference, &render);
                            sc.spawn(move || {
                                chunk
                                    .into_iter()
                                    .filter(|&d| {
                                        [0.0f32, 1.0].iter().any(|&t| {
                                            let mut v = values.clone();
                                            v[d] = INFO.params[d].from_normalized(t);
                                            differs(&render(&v), reference)
                                        })
                                    })
                                    .collect::<Vec<usize>>()
                            })
                        })
                        .collect();
                    handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
                });
                for d in &acted {
                    found.push((*d, state));
                }
                left.retain(|d| !acted.contains(d));
                eprintln!("{}: state {state}, {} still dead", slot.label, left.len());
            }
            for (d, st) in found {
                out += &format!("{d}\t{}\tCONDITIONAL\t{st}\n", INFO.params[d].id);
            }
            for d in left {
                out += &format!("{d}\t{}\tDEAD\t-\n", INFO.params[d].id);
            }
        }
        std::fs::write(format!("{dir}/build/dead-check.tsv"), out).expect("write");
    }

    /// The host sweep's last suspects, each in the setting it needs (second route, rule 7):
    /// SimplEQ width and depth on a band reject, on both channel sets; Loudness MAX's learn pair
    /// with a target above the measured loudness.
    #[test]
    fn conditional_parameters_act_once_their_condition_holds() {
        let id = |s: &str| INFO.params.iter().position(|p| p.id == s).unwrap_or_else(|| panic!("{s}"));
        let tone: Vec<f32> = (0..9600).map(|i| 0.2 * (i as f32 * 0.13).sin() + 0.1 * (i as f32 * 0.021).sin()).collect();
        let render = |set: &[(&str, f32)]| -> Vec<f32> {
            let mut p = OpenLimiter::default();
            for (k, v) in set {
                p.set_param(k, *v);
            }
            p.prepare(SR, 512, 2);
            let mut io = vec![tone.clone(), tone.clone()];
            process(&mut p, &mut io, tone.len());
            let lat = p.latency();
            io.iter().flat_map(|c| c[lat.min(4800)..lat.min(4800) + 4800].to_vec()).collect()
        };
        let acts = |base: &[(&str, f32)], key: &str| {
            let info = &INFO.params[id(key)];
            let mut lo = base.to_vec();
            lo.push((key, info.min));
            let mut hi = base.to_vec();
            hi.push((key, info.max));
            let (a, b) = (render(&lo), render(&hi));
            a.iter().zip(&b).any(|(x, y)| (x - y).abs() > 1e-5)
        };
        let off = ("chain0.on", 0.0);
        for (band, sfx, extra) in [("2", "", None), ("4", "", None), ("6", "Ch2", Some(("eq.linkSets", 0.0)))] {
            let mut base = vec![off, ("eq.on", 1.0), (Box::leak(format!("eq.b{band}Type{sfx}").into_boxed_str()) as &str, 1.0)];
            base.extend(extra);
            base.push((Box::leak(format!("eq.b{band}Freq{sfx}").into_boxed_str()), 3000.0));
            for f in ["Width", "Depth"] {
                let key: &str = Box::leak(format!("eq.b{band}{f}{sfx}").into_boxed_str());
                let mut b = base.clone();
                if f == "Width" {
                    b.push((Box::leak(format!("eq.b{band}Depth{sfx}").into_boxed_str()), -40.0));
                }
                assert!(acts(&b, key), "{key} on a band reject");
            }
        }
        let modes = [off, ("modes.on", 1.0), ("modes.drive", 0.0)];
        // At its ends learnLoudness sets nothing: idle, or louder than any target. In between it
        // sets the drive to target minus loudness.
        let learn = |loudness: f32| {
            let mut b = modes.to_vec();
            b.extend([("modes.learnTarget", -6.0), ("modes.learnLoudness", loudness)]);
            render(&b)
        };
        let (idle, learned) = (learn(-70.0), learn(-30.0));
        assert!(idle.iter().zip(&learned).any(|(x, y)| (x - y).abs() > 1e-5), "learning a quiet input raises the drive");
        // learnLoudness is applied when it is set, so the target has to come first.
        let render_learn = |target: f32| render(&[off, ("modes.on", 1.0), ("modes.learnTarget", target), ("modes.learnLoudness", -40.0)]);
        let (a, c) = (render_learn(-32.0), render_learn(-1.0));
        assert!(a.iter().zip(&c).any(|(x, y)| (x - y).abs() > 1e-5), "the learn target sets how far the drive goes");
    }
}
