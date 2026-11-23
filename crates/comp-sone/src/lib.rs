//! DualSone, the dual-engine loudness compressor, as an OpenLimiter module.
//!
//! A port of the DualSone 1.1.1 engine (`Source/dsp/DualLoudnessCore.h`) and of how its
//! processor drives it (`Source/PluginProcessor.cpp`, `Source/Params.h`). The engine lives in
//! [`engine`]; [`DualSone`] is the module: the same parameters, ranges and defaults as the
//! plugin, plain values, and the same latency (45 ms: 2160 samples at 48 kHz).

pub mod engine;

use engine::{Core, Mode, Settings, Trace};
use olm_api::{Kind, Meters, Module, ParamDesc};

const DETECTOR: &[&str] = &["LUFS (K)", "RMS (flat)"];
const MODES: &[&str] = &["Parallel", "Series", "Fast-led", "Mix"];
const WINDOWS: &[&str] = &["200 ms", "400 ms", "3 s"];

/// Parameter indices, in the order of `Params.h` (and of `DualSone::params`).
pub mod idx {
    pub const DETECTOR: usize = 0;
    pub const MODE: usize = 1;
    pub const THRESHOLD: usize = 2;
    pub const MIX: usize = 3;
    pub const OUTPUT: usize = 4;
    pub const SLOW_ON: usize = 5;
    pub const SLOW_RATIO: usize = 6;
    pub const SLOW_ATTACK: usize = 7;
    pub const SLOW_RELEASE: usize = 8;
    pub const SLOW_WINDOW: usize = 9;
    pub const FAST_ON: usize = 10;
    pub const FAST_CREST: usize = 11;
    pub const FAST_RATIO: usize = 12;
    pub const FAST_ATTACK: usize = 13;
    pub const FAST_RELEASE: usize = 14;
    pub const FAST_LOOKAHEAD: usize = 15;
    pub const INPUT: usize = 16;
    pub const AUTO_GAIN: usize = 17;
    pub const DELTA: usize = 18;
    pub const COUNT: usize = 19;
}

// Same order, ids, names, ranges and defaults as Params.h. The JUCE skewed ranges are marked `log`
// (the API has no skew centre); the units the original snaps to whole steps (dB, LUFS, :1) get
// `steps` and are rounded in `set_param`, as JUCE's interval-1 range does.
static PARAMS: [ParamDesc; idx::COUNT] = [
    ParamDesc::choice("detector", "Detector", DETECTOR, 0),
    ParamDesc::choice("mode", "Mode", MODES, 0),
    ParamDesc::new("threshold", "Threshold", -40.0, 0.0, -18.0, Kind::Db).steps(41),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::new("output", "Output", -24.0, 24.0, 0.0, Kind::Db).steps(49),
    ParamDesc::toggle("slowOn", "Slow On", true),
    ParamDesc::new("slowRatio", "Slow Ratio", 1.0, 10.0, 2.0, Kind::Ratio).log().steps(10),
    ParamDesc::new("slowAttack", "Slow Attack", 50.0, 5000.0, 800.0, Kind::Ms).log(),
    ParamDesc::new("slowRelease", "Slow Release", 100.0, 10000.0, 2500.0, Kind::Ms).log(),
    ParamDesc::choice("slowWindow", "Slow Window", WINDOWS, 1),
    ParamDesc::toggle("fastOn", "Fast On", true),
    ParamDesc::new("fastCrest", "Fast Crest", 0.0, 24.0, 6.0, Kind::Db).steps(25),
    ParamDesc::new("fastRatio", "Fast Ratio", 1.0, 20.0, 3.0, Kind::Ratio).log().steps(20),
    ParamDesc::new("fastAttack", "Fast Attack", 0.5, 30.0, 10.0, Kind::Ms).log(),
    ParamDesc::new("fastRelease", "Fast Release", 5.0, 1000.0, 100.0, Kind::Ms).log(),
    ParamDesc::new("fastLookahead", "Fast Lookahead", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::new("input", "Input", -24.0, 24.0, 0.0, Kind::Db).steps(49),
    ParamDesc::toggle("autoGain", "Autogain", false),
    ParamDesc::toggle("delta", "Delta", false),
];

/// Slot layout of [`Module::view`], of the block just processed. Loudness is LUFS with the
/// K-weighted detector and dB RMS with the flat one.
pub mod view {
    /// The loudness the slow engine measures, at the end of the block.
    pub const LOUDNESS: usize = 0;
    /// Crest (5 ms loudness over the slow loudness), dB, the highest in the block.
    pub const CREST: usize = 1;
    /// Autogain applied, dB.
    pub const AUTO_GAIN: usize = 2;
    /// The fast engine's reference loudness (what the crest is measured from).
    pub const REFERENCE: usize = 3;
    /// Slow gain, dB, signed (0 or negative), the deepest in the block.
    pub const SLOW_GAIN: usize = 4;
    /// Fast gain, dB, signed, the deepest in the block.
    pub const FAST_GAIN: usize = 5;
    /// Slow + fast gain, dB, signed, the deepest in the block.
    pub const TOTAL_GAIN: usize = 6;
    /// 1 while the slow engine holds its gain through a pause, else 0.
    pub const HOLD: usize = 7;
    /// The highest 5 ms loudness in the block.
    pub const FAST_LEVEL: usize = 8;
    pub const LEN: usize = 9;
}

/// Maps plain parameter values to the engine's settings (`params::toSettings`).
pub fn to_settings(v: &[f32; idx::COUNT]) -> Settings {
    let window = (v[idx::SLOW_WINDOW].round() as i32).clamp(0, 2);
    Settings {
        k_weighted: v[idx::DETECTOR] < 0.5,
        mode: Mode::from_index((v[idx::MODE].round() as i32).clamp(0, 3)),
        mix: v[idx::MIX] * 0.01,
        output_db: v[idx::OUTPUT],
        input_db: v[idx::INPUT],
        slow_on: v[idx::SLOW_ON] > 0.5,
        threshold_db: v[idx::THRESHOLD],
        slow_ratio: v[idx::SLOW_RATIO],
        slow_attack_ms: v[idx::SLOW_ATTACK],
        slow_release_ms: v[idx::SLOW_RELEASE],
        slow_window_seconds: if window == 0 { 0.2 } else if window == 2 { 3.0 } else { 0.4 },
        fast_on: v[idx::FAST_ON] > 0.5,
        fast_crest_db: v[idx::FAST_CREST],
        fast_ratio: v[idx::FAST_RATIO],
        fast_attack_ms: v[idx::FAST_ATTACK],
        fast_release_ms: v[idx::FAST_RELEASE],
        lookahead: v[idx::FAST_LOOKAHEAD] * 0.01,
        auto_gain: v[idx::AUTO_GAIN] > 0.5,
        learn_auto_gain: true,
        delta: v[idx::DELTA] > 0.5,
    }
}

/// The DualSone compressor module.
pub struct DualSone {
    core: Core,
    values: [f32; idx::COUNT],
}

impl Default for DualSone {
    fn default() -> Self {
        let mut values = [0.0; idx::COUNT];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        let mut core = Core::new();
        core.set_settings(&to_settings(&values));
        Self { core, values }
    }
}

impl DualSone {
    /// The current plain value of parameter `index` (0 for an unknown index).
    pub fn param(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    /// Index of the parameter with this id.
    pub fn index_of(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// The engine, for tests and measurements.
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Like `process`, also writing the engine's per-sample traces.
    pub fn process_traced(&mut self, left: &mut [f32], right: &mut [f32], trace: Option<&mut Trace>) {
        let n = left.len().min(right.len());
        let (l, r) = (&mut left[..n], &mut right[..n]);
        // As PluginProcessor::run: the settings are handed to the engine at the start of every block.
        // (This also restarts the means' drift-correction accumulators each block, as in the plugin.)
        self.core.set_settings(&to_settings(&self.values));
        self.core.apply_input_gain(l, r);
        self.core.process(l, r, trace);
    }
}

impl Module for DualSone {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        // The engine is per sample with no scratch buffers, so the block size does not matter.
        let fs = if sample_rate.is_finite() && sample_rate >= 1000.0 { sample_rate as f64 } else { 48000.0 };
        self.core.set_settings(&to_settings(&self.values));
        self.core.prepare(fs);
    }

    fn reset(&mut self) {
        self.core.reset();
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
        // Taken up by the engine at the start of the next block, as the plugin reads its parameters.
        self.values[index] = v;
    }

    fn latency(&self) -> usize {
        self.core.latency_samples().max(0) as usize
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_traced(left, right, None);
    }

    /// The read-out of the last block, laid out as [`view`] says.
    fn view(&self, out: &mut [f32]) -> usize {
        let c = &self.core;
        let values = [
            c.meter_loudness,
            c.meter_crest,
            c.meter_auto_gain_db,
            c.meter_reference,
            c.meter_slow_gr,
            c.meter_fast_gr,
            c.meter_total_gr,
            if c.meter_hold { 1.0 } else { 0.0 },
            c.meter_fast_level,
        ];
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&values[..n]);
        n
    }

    fn meters(&self) -> Meters {
        let mut m = Meters { reduction_db: (-self.core.meter_total_gr).max(0.0), ..Meters::default() };
        m.band_reduction_db[0] = (-self.core.meter_slow_gr).max(0.0);
        m.band_reduction_db[1] = (-self.core.meter_fast_gr).max(0.0);
        m
    }
}
