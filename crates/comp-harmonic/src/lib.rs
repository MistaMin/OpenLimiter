//! Harmonic Compressor as an OpenLimiter module.
//!
//! A port of the owner's Harmonic Compressor 1.5.2 DSP (MIT, C++): seven compression modes
//! ([`modes`]), the ADAA harmonic stage ([`harmonics`]), 1x-8x oversampling with a matched dry
//! path ([`resampler`]), the transient designer and the Auto Gain loudness matcher
//! ([`dynamics`]). [`HarmonicCompressor`] wires them in the processor's order:
//!
//! 1. copy the untouched input (Auto Gain reference, Delta reference);
//! 2. Input gain (10 ms multiplicative ramp);
//! 3. dry copy, run through the matched resampler chain;
//! 4. upsample; per oversampled sample: sidechain (input or key, optional high-pass), stereo-linked
//!    detector (the louder channel), the mode's gain, makeup, harmonics (or Sidechain Listen);
//!    downsample;
//! 5. Mix with the aligned dry; 6. transient designer; 7. Auto Gain; 8. Delta; 9. Output gain.

pub mod dynamics;
pub mod harmonics;
pub mod modes;
pub mod resampler;
pub mod util;

use dynamics::{LoudnessMatcher, TransientDesigner};
use harmonics::Harmonics;
pub use modes::{curve, CompParams, CurveParams, Mode};
use modes::{Modes, SidechainHp};
use olm_api::{Kind, Meters, Module, ParamDesc};
use resampler::{Multirate, OsFilter, OsMode};
use util::*;

const OS_MODES: &[&str] = &["1x", "2x", "4x", "8x"];
const OS_FILTERS: &[&str] = &["Min Phase", "Lin Phase"];
const KEY_SOURCES: &[&str] = &["Internal", "External"];

/// Parameter indices: the original's 27 in its host order, then `keySource`.
pub mod idx {
    pub const MODE: usize = 0;
    pub const THRESHOLD: usize = 1;
    pub const RATIO: usize = 2;
    pub const ATTACK: usize = 3;
    pub const RELEASE: usize = 4;
    pub const KNEE: usize = 5;
    pub const MAKEUP: usize = 6;
    pub const MIX: usize = 7;
    pub const H2: usize = 8;
    pub const H3: usize = 9;
    pub const H4: usize = 10;
    pub const H5: usize = 11;
    pub const SC_ENABLED: usize = 12;
    pub const SC_LISTEN: usize = 13;
    pub const SC_HP_FREQ: usize = 14;
    pub const AUTO_RELEASE: usize = 15;
    pub const AUTO_ATTACK: usize = 16;
    pub const INPUT_GAIN: usize = 17;
    pub const OUTPUT_GAIN: usize = 18;
    pub const OS_MODE: usize = 19;
    pub const OS_FILTER: usize = 20;
    pub const TRANSIENT_ENABLED: usize = 21;
    pub const TRANSIENT_ATTACK: usize = 22;
    pub const TRANSIENT_SUSTAIN: usize = 23;
    pub const TRANSIENT_SPEED: usize = 24;
    pub const AUTO_GAIN: usize = 25;
    pub const DELTA: usize = 26;
    pub const KEY_SOURCE: usize = 27;
    pub const COUNT: usize = 28;
}

// Ids, names, ranges and defaults as the original's layout. Its 0.1 / 1 / 0.01 intervals become
// `steps` (values are snapped in `set_param`); its skewed ranges are marked `log` where the range
// is positive (the API has no skew centre). Mix is 0..100 % here (the original stores 0..1).
static PARAMS: [ParamDesc; idx::COUNT] = [
    ParamDesc::choice("algorithmMode", "Mode", Mode::LABELS, 0),
    ParamDesc::new("threshold", "Threshold", -60.0, 0.0, -18.0, Kind::Db).steps(601),
    ParamDesc::new("ratio", "Ratio", -2.0, 100.0, 4.0, Kind::Ratio).steps(1021),
    ParamDesc::new("attack", "Attack", 0.1, 300.0, 10.0, Kind::Ms).log().steps(3000),
    ParamDesc::new("release", "Release", 5.0, 3000.0, 100.0, Kind::Ms).log().steps(2996),
    ParamDesc::new("knee", "Knee", 0.0, 24.0, 6.0, Kind::Db).steps(241),
    ParamDesc::new("makeupGain", "Makeup Gain", -12.0, 24.0, 0.0, Kind::Db).steps(361),
    ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent).steps(101),
    ParamDesc::new("harmonic2", "2nd Harmonic", 0.0, 100.0, 0.0, Kind::Percent).steps(1001),
    ParamDesc::new("harmonic3", "3rd Harmonic", 0.0, 100.0, 0.0, Kind::Percent).steps(1001),
    ParamDesc::new("harmonic4", "4th Harmonic", 0.0, 100.0, 0.0, Kind::Percent).steps(1001),
    ParamDesc::new("harmonic5", "5th Harmonic", 0.0, 100.0, 0.0, Kind::Percent).steps(1001),
    ParamDesc::toggle("scEnabled", "Sidechain Enable", false),
    ParamDesc::toggle("scListen", "Sidechain Listen", false),
    ParamDesc::new("scHPFreq", "Sidechain HP Freq", 20.0, 500.0, 150.0, Kind::Hz).log().steps(481),
    ParamDesc::toggle("autoRelease", "Auto Release", false),
    ParamDesc::toggle("autoAttack", "Auto Attack", false),
    ParamDesc::new("inputGain", "Input Gain", -24.0, 24.0, 0.0, Kind::Db).steps(481),
    ParamDesc::new("outputGain", "Output Gain", -24.0, 24.0, 0.0, Kind::Db).steps(481),
    ParamDesc::choice("oversampleMode", "Oversample", OS_MODES, 1),
    ParamDesc::choice("oversampleFilter", "Oversample Filter", OS_FILTERS, 0),
    ParamDesc::toggle("transientEnabled", "Transient Enable", false),
    ParamDesc::new("transientAttack", "Transient Attack", -100.0, 100.0, 0.0, Kind::Percent).steps(2001),
    ParamDesc::new("transientSustain", "Transient Sustain", -100.0, 100.0, 0.0, Kind::Percent).steps(2001),
    ParamDesc::new("transientSpeed", "Transient Speed", 0.5, 50.0, 10.0, Kind::Ms).log().steps(496),
    ParamDesc::toggle("autoGain", "Auto Gain", false),
    ParamDesc::toggle("delta", "Delta Monitor", false),
    ParamDesc::choice("keySource", "Key Source", KEY_SOURCES, 0),
];

/// Slot layout of [`Module::view`], for the block just processed. dB values are dBFS or dB of
/// gain as named; gains are signed (negative is reduction) unless said otherwise.
pub mod view {
    /// The mode's gain at the end of the block, dB, signed (the original's GR meter).
    pub const GAIN_REDUCTION_DB: usize = 0;
    /// The deepest reduction in the block, dB, positive (as `Meters::reduction_db`).
    pub const DEEPEST_REDUCTION_DB: usize = 1;
    /// The highest detector level in the block (after key selection and sidechain high-pass), dBFS, floor -120.
    pub const DETECTOR_DB: usize = 2;
    /// Auto Gain applied at the end of the block, dB.
    pub const AUTO_GAIN_DB: usize = 3;
    /// The gain Auto Gain is heading for (measured even while it is off), dB.
    pub const AUTO_GAIN_TARGET_DB: usize = 4;
    /// Transient designer gain at the end of the block, dB.
    pub const TRANSIENT_GAIN_DB: usize = 5;
    /// The transient designer gain of largest magnitude in the block, dB.
    pub const TRANSIENT_EXTREME_DB: usize = 6;
    /// Input peak after Input gain, dBFS, floor -72 (the original's curve-dot input).
    pub const INPUT_PEAK_DB: usize = 7;
    /// Output peak, dBFS, floor -72.
    pub const OUTPUT_PEAK_DB: usize = 8;
    /// Static-curve output (with makeup) at `DETECTOR_DB`: where the live dot sits on the curve.
    pub const CURVE_OUTPUT_DB: usize = 9;
    /// The settings the transfer-curve display draws from: mode index, threshold dB, ratio,
    /// knee dB as the mode uses it (0 for modes without a knee), makeup dB.
    pub const MODE: usize = 10;
    pub const THRESHOLD_DB: usize = 11;
    pub const RATIO: usize = 12;
    pub const KNEE_DB: usize = 13;
    pub const MAKEUP_DB: usize = 14;
    /// 1 while the external key drives the detector, else 0.
    pub const KEY_ACTIVE: usize = 15;
    /// Oversampling factor in use (1, 2, 4, 8).
    pub const OS_FACTOR: usize = 16;
    /// Latency of the current oversampling setting, fractional samples (`latency()` rounds it).
    pub const LATENCY_SAMPLES: usize = 17;
    pub const LEN: usize = 18;
}

/// The curve settings of a parameter set (for [`curve`]).
pub fn curve_params(values: &[f32; idx::COUNT]) -> (Mode, CurveParams) {
    let mode = Mode::from_index(values[idx::MODE].round() as i32);
    (
        mode,
        CurveParams {
            threshold_db: values[idx::THRESHOLD],
            ratio: values[idx::RATIO],
            knee_db: if mode.uses_knee() { values[idx::KNEE] } else { 0.0 },
            makeup_db: values[idx::MAKEUP],
        },
    )
}

/// The Harmonic Compressor module.
pub struct HarmonicCompressor {
    values: [f32; idx::COUNT],
    host_sr: f64,
    engine_sr: f64,
    max_block: usize,

    modes: Modes,
    harmonics: Harmonics,
    sidechain: SidechainHp,
    multirate: Multirate,
    transient: TransientDesigner,
    loudness: LoudnessMatcher,

    auto_gain: Smoothed,
    delta: Smoothed,
    delta_ref: Smoothed,
    in_gain: Smoothed,
    out_gain: Smoothed,

    dry: [Vec<f32>; 2],
    reference: [Vec<f32>; 2],
    loud_in: [Vec<f32>; 2],

    key_seen: bool,
    reference_running: bool,
    /// The dry chain runs only while the dry signal is heard (mix below 100 %, Delta).
    dry_running: bool,

    // Read-outs of the last block.
    block_reduction_db: f32,
    detector_peak_db: f32,
    auto_gain_db: f32,
    transient_extreme_db: f32,
    input_peak_db: f32,
    output_peak_db: f32,
    key_active: bool,
}

impl Default for HarmonicCompressor {
    fn default() -> Self {
        let mut values = [0.0; idx::COUNT];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        let mut m = Self {
            values,
            host_sr: 48000.0,
            engine_sr: 0.0,
            max_block: 0,
            modes: Modes::default(),
            harmonics: Harmonics::default(),
            sidechain: SidechainHp::default(),
            multirate: Multirate::new(1),
            transient: TransientDesigner::default(),
            loudness: LoudnessMatcher::default(),
            auto_gain: Smoothed::multiplicative(1.0),
            delta: Smoothed::linear(0.0),
            delta_ref: Smoothed::linear(0.0),
            in_gain: Smoothed::multiplicative(1.0),
            out_gain: Smoothed::multiplicative(1.0),
            dry: [Vec::new(), Vec::new()],
            reference: [Vec::new(), Vec::new()],
            loud_in: [Vec::new(), Vec::new()],
            key_seen: false,
            reference_running: false,
            dry_running: false,
            block_reduction_db: 0.0,
            detector_peak_db: -120.0,
            auto_gain_db: 0.0,
            transient_extreme_db: 0.0,
            input_peak_db: -72.0,
            output_peak_db: -72.0,
            key_active: false,
        };
        m.prepare(48000.0, 512);
        m
    }
}

impl HarmonicCompressor {
    /// The current plain value of parameter `index` (0 for an unknown index).
    pub fn param(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    /// Index of the parameter with this id.
    pub fn index_of(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    /// All current plain values.
    pub fn values(&self) -> &[f32; idx::COUNT] {
        &self.values
    }

    fn os_config(&self) -> (OsMode, OsFilter) {
        (
            OsMode::from_index(self.values[idx::OS_MODE].round() as i32),
            OsFilter::from_index(self.values[idx::OS_FILTER].round() as i32),
        )
    }

    /// Fractional latency of the current oversampling setting, in samples.
    pub fn latency_fractional(&self) -> f64 {
        let (m, f) = self.os_config();
        self.multirate.latency_for(m, f)
    }

    fn comp_params(&self) -> CompParams {
        let v = &self.values;
        CompParams {
            threshold_db: v[idx::THRESHOLD],
            ratio: v[idx::RATIO],
            attack_ms: v[idx::ATTACK],
            release_ms: v[idx::RELEASE],
            knee_db: v[idx::KNEE],
            makeup_db: v[idx::MAKEUP],
            mix: v[idx::MIX] / 100.0,
            auto_release: v[idx::AUTO_RELEASE] > 0.5,
            auto_attack: v[idx::AUTO_ATTACK] > 0.5,
        }
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let n = left.len().min(right.len());
        let step = self.max_block.max(1);
        let mut start = 0;
        while start < n {
            let end = (start + step).min(n);
            let k = key.and_then(|(kl, kr)| {
                if kl.len() >= end && kr.len() >= end {
                    Some((&kl[start..end], &kr[start..end]))
                } else {
                    None
                }
            });
            self.block(&mut left[start..end], &mut right[start..end], k);
            start = end;
        }
    }

    fn block(&mut self, l: &mut [f32], r: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let n = l.len().min(r.len());
        if n == 0 {
            return;
        }
        let (l, r) = (&mut l[..n], &mut r[..n]);
        let v = self.values;
        let p = self.comp_params();
        let mode = Mode::from_index(v[idx::MODE].round() as i32);
        let sc_hp = v[idx::SC_HP_FREQ];
        let sc_enabled = v[idx::SC_ENABLED] > 0.5;
        let listen = v[idx::SC_LISTEN] > 0.5;
        let in_db = v[idx::INPUT_GAIN];
        let out_db = v[idx::OUTPUT_GAIN];
        let (os_mode, os_filter) = self.os_config();
        let tr_on = v[idx::TRANSIENT_ENABLED] > 0.5;
        let auto_on = v[idx::AUTO_GAIN] > 0.5;
        let delta_on = v[idx::DELTA] > 0.5;
        let external = v[idx::KEY_SOURCE] > 0.5;
        let h2 = v[idx::H2] / 100.0;
        let h3 = v[idx::H3] / 100.0;
        let h4 = v[idx::H4] / 100.0;
        let h5 = v[idx::H5] / 100.0;

        // Untouched input: Auto Gain's reference and Delta's reference with Auto Gain on.
        for ch in 0..2 {
            let src: &[f32] = if ch == 0 { l } else { r };
            self.reference[ch][..n].copy_from_slice(src);
            self.loud_in[ch][..n].copy_from_slice(src);
        }

        self.in_gain.set_target(db_to_gain(in_db));
        if self.in_gain.is_smoothing() || in_db.abs() > 1.0e-4 {
            for s in 0..n {
                let g = self.in_gain.next();
                l[s] *= g;
                r[s] *= g;
            }
        }

        self.multirate.set_config(os_mode, os_filter);
        let factor = self.multirate.factor();
        let eff = self.host_sr * factor as f64;
        if (eff - self.engine_sr).abs() > 0.5 {
            self.engine_sr = eff;
            self.modes.prepare(eff);
            self.harmonics.prepare(eff);
            self.sidechain.set_sample_rate(eff);
        }
        self.modes.update(mode, &p);
        self.sidechain.set_frequency(sc_hp);

        // External key: the input is detected until the key first carries signal; from then on a
        // silent key means no compression.
        let usable = external && key.is_some();
        if let (true, false, Some((kl, kr))) = (usable, self.key_seen, key) {
            let mag = kl.iter().chain(kr.iter()).fold(0.0f32, |m, x| jmax(m, x.abs()));
            if mag > 1.0e-7 {
                self.key_seen = true;
            }
        }
        let has_key = usable && self.key_seen;
        self.key_active = has_key;

        let makeup = db_to_gain(p.makeup_db);
        let mix = p.mix;

        let ref_needed = ((delta_on && !listen) || self.delta.current() > 0.0)
            && (auto_on || self.delta_ref.current() > 0.0);
        if ref_needed && !self.reference_running {
            self.multirate.reset_reference();
        }
        self.reference_running = ref_needed;
        // Owner, 2026-11-24: at 100 % mix with Delta off nobody hears the dry signal, so its
        // oversampling chain is skipped; it restarts from silence when it is needed again.
        let dry_needed = (!listen && mix < 1.0) || (delta_on && !listen) || self.delta.current() > 0.0;
        if dry_needed && !self.dry_running {
            self.multirate.reset_dry();
        }
        self.dry_running = dry_needed;

        self.dry[0][..n].copy_from_slice(l);
        self.dry[1][..n].copy_from_slice(r);
        {
            let (d0, d1) = self.dry.split_at_mut(1);
            let (r0, r1) = self.reference.split_at_mut(1);
            self.multirate.align_dry((&mut d0[0][..n], &mut d1[0][..n]), (&mut r0[0][..n], &mut r1[0][..n]), dry_needed, ref_needed);
        }

        let mut in_peak = 0.0f32;
        for s in 0..n {
            in_peak = jmax(in_peak, l[s].abs());
            in_peak = jmax(in_peak, r[s].abs());
        }

        let os_len = self.multirate.upsample(l, r);
        let mut min_gain = 1.0f32;
        let mut det_peak = 0.0f32;
        {
            let (ol, or) = self.multirate.os_block();
            let modes = &mut self.modes;
            let harm = &mut self.harmonics;
            let sc = &mut self.sidechain;
            let nf = n as i32;
            let read_key = |k: &[f32], s: usize| -> f32 {
                let base = s as f32 / factor as f32;
                let i0 = (base as i32).clamp(0, nf - 1);
                let i1 = (i0 + 1).min(nf - 1);
                let frac = base - i0 as f32;
                let (a, b) = (k[i0 as usize], k[i1 as usize]);
                a + frac * (b - a)
            };
            for s in 0..os_len {
                let (raw0, raw1) = match (has_key, key) {
                    (true, Some((kl, kr))) => (read_key(kl, s), read_key(kr, s)),
                    _ => (ol[s], or[s]),
                };
                let sc0 = if sc_enabled { sc.process(0, raw0) } else { raw0 };
                let sc1 = if sc_enabled { sc.process(1, raw1) } else { raw1 };
                let det = jmax(jmax(0.0, sc0.abs()), sc1.abs());
                det_peak = jmax(det_peak, det);
                let gr = modes.process(mode, det);
                min_gain = jmin(min_gain, gr);
                if listen {
                    ol[s] = sc0;
                    or[s] = sc1;
                } else {
                    ol[s] = harm.process(ol[s] * gr * makeup, h2, h3, h4, h5, 0);
                    or[s] = harm.process(or[s] * gr * makeup, h2, h3, h4, h5, 1);
                }
            }
        }
        self.block_reduction_db = -gain_to_db_floor(min_gain, -120.0);
        self.detector_peak_db = gain_to_db_floor(det_peak, -120.0);
        self.multirate.downsample(l, r);

        if !listen && mix < 1.0 {
            for s in 0..n {
                l[s] = mix * l[s] + (1.0 - mix) * self.dry[0][s];
                r[s] = mix * r[s] + (1.0 - mix) * self.dry[1][s];
            }
        }

        self.transient_extreme_db = 0.0;
        if !listen {
            self.transient_extreme_db = self.transient.process(
                l,
                r,
                tr_on,
                v[idx::TRANSIENT_ATTACK],
                v[idx::TRANSIENT_SUSTAIN],
                v[idx::TRANSIENT_SPEED],
            );
            self.loudness.process([&self.loud_in[0][..n], &self.loud_in[1][..n]], [&*l, &*r], n);
        }

        let target = if auto_on && !listen { self.loudness.target_gain_db() } else { 0.0 };
        self.auto_gain.set_target(db_to_gain(target));
        if self.auto_gain.is_smoothing() || (self.auto_gain.current() - 1.0).abs() > 1.0e-6 {
            for s in 0..n {
                let g = self.auto_gain.next();
                l[s] *= g;
                r[s] *= g;
            }
        }
        self.auto_gain_db = gain_to_db_floor(self.auto_gain.current(), -100.0);

        self.delta.set_target(if delta_on && !listen { 1.0 } else { 0.0 });
        self.delta_ref.set_target(if auto_on { 1.0 } else { 0.0 });
        if self.delta.is_smoothing() || self.delta.current() > 0.0 {
            for s in 0..n {
                let amount = self.delta.next();
                let use_in = self.delta_ref.next();
                let d0 = self.dry[0][s];
                let d1 = self.dry[1][s];
                let r0 = d0 + use_in * (self.reference[0][s] - d0);
                let r1 = d1 + use_in * (self.reference[1][s] - d1);
                l[s] -= amount * r0;
                r[s] -= amount * r1;
            }
        } else {
            self.delta_ref.skip(n as i32);
        }

        self.out_gain.set_target(db_to_gain(out_db));
        if self.out_gain.is_smoothing() || out_db.abs() > 1.0e-4 {
            for s in 0..n {
                let g = self.out_gain.next();
                l[s] *= g;
                r[s] *= g;
            }
        }

        let mut out_peak = 0.0f32;
        for s in 0..n {
            out_peak = jmax(out_peak, l[s].abs());
            out_peak = jmax(out_peak, r[s].abs());
        }
        self.input_peak_db = jmax(-72.0, gain_to_db_floor(in_peak, -72.0));
        self.output_peak_db = jmax(-72.0, gain_to_db_floor(out_peak, -72.0));
    }
}

impl Module for HarmonicCompressor {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize) {
        let sr = if sample_rate.is_finite() && sample_rate >= 1000.0 { sample_rate as f64 } else { 48000.0 };
        let max_block = max_block.max(1);
        self.host_sr = sr;
        self.max_block = max_block;
        self.key_seen = false;
        self.reference_running = false;
        self.dry_running = false;

        self.multirate = Multirate::new(max_block);
        let (m, f) = self.os_config();
        self.multirate.set_config(m, f);

        self.transient.prepare(sr);
        self.loudness.prepare(sr);
        self.auto_gain.reset(sr, 0.100);
        self.auto_gain.set_current_and_target(1.0);
        self.auto_gain_db = 0.0;
        self.delta.reset(sr, 0.020);
        self.delta.set_current_and_target(if self.values[idx::DELTA] > 0.5 { 1.0 } else { 0.0 });
        self.delta_ref.reset(sr, 0.050);
        self.delta_ref.set_current_and_target(if self.values[idx::AUTO_GAIN] > 0.5 { 1.0 } else { 0.0 });
        self.in_gain.reset(sr, 0.010);
        self.in_gain.set_current_and_target(db_to_gain(self.values[idx::INPUT_GAIN]));
        self.out_gain.reset(sr, 0.010);
        self.out_gain.set_current_and_target(db_to_gain(self.values[idx::OUTPUT_GAIN]));

        let eff = sr * self.multirate.factor() as f64;
        self.engine_sr = eff;
        self.modes.prepare(eff);
        self.harmonics.prepare(eff);
        self.sidechain.prepare(eff);
        self.modes.reset();
        self.harmonics.reset();
        self.sidechain.reset();

        for b in self.dry.iter_mut().chain(self.reference.iter_mut()).chain(self.loud_in.iter_mut()) {
            *b = vec![0.0; max_block];
        }
    }

    fn reset(&mut self) {
        self.multirate.reset();
        self.reference_running = false;
        self.dry_running = false;
        self.transient.reset();
        self.loudness.reset();
        self.auto_gain.set_current_and_target(1.0);
        self.auto_gain_db = 0.0;
        self.delta.set_current_and_target(if self.values[idx::DELTA] > 0.5 { 1.0 } else { 0.0 });
        self.delta_ref.set_current_and_target(if self.values[idx::AUTO_GAIN] > 0.5 { 1.0 } else { 0.0 });
        self.modes.reset();
        self.harmonics.reset();
        self.sidechain.reset();
        self.in_gain.set_current_and_target(db_to_gain(self.values[idx::INPUT_GAIN]));
        self.out_gain.set_current_and_target(db_to_gain(self.values[idx::OUTPUT_GAIN]));
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let (Some(p), true) = (PARAMS.get(index), value.is_finite()) else {
            return;
        };
        let mut v = value.clamp(p.min, p.max);
        if p.steps > 1 {
            let span = p.max - p.min;
            let k = ((v - p.min) / span * (p.steps - 1) as f32).round();
            v = (p.min + k * span / (p.steps - 1) as f32).clamp(p.min, p.max);
        }
        // Taken up at the start of the next block, as the original reads its parameters.
        self.values[index] = v;
    }

    fn latency(&self) -> usize {
        resampler::round_latency(self.latency_fractional())
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.run(left, right, None);
    }

    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        self.run(left, right, Some((key_left, key_right)));
    }

    fn meters(&self) -> Meters {
        Meters { reduction_db: self.block_reduction_db.max(0.0), ..Meters::default() }
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let (mode, cp) = curve_params(&self.values);
        let values: [f32; view::LEN] = [
            self.modes.last_gr_db(mode),
            self.block_reduction_db.max(0.0),
            self.detector_peak_db,
            self.auto_gain_db,
            self.loudness.target_gain_db(),
            self.transient.gain_db,
            self.transient_extreme_db,
            self.input_peak_db,
            self.output_peak_db,
            curve::output_db(mode, &cp, self.detector_peak_db),
            mode as usize as f32,
            cp.threshold_db,
            cp.ratio,
            cp.knee_db,
            cp.makeup_db,
            if self.key_active { 1.0 } else { 0.0 },
            self.multirate.factor() as f32,
            self.latency_fractional() as f32,
        ];
        let n = out.len().min(view::LEN);
        out[..n].copy_from_slice(&values[..n]);
        n
    }
}
