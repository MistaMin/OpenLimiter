//! Resonance suppressor: a port of the OpenEQ 1.0.1 suppressor (the RESO field) as its own
//! module. It compares every frequency with its smoothed surroundings and removes only the
//! excess, so the result does not depend on the input level.
//!
//! OpenEQ's per-band "suppressor focus" used the EQ bands' frequency and Q. Here the same focus
//! shaping is four free focus regions (amount, frequency, Q) with OpenEQ's formula.
//!
//! The latency is the STFT length minus one (2047 samples up to 50 kHz, 4095 up to 100 kHz, 8191
//! above) and does not change with the settings. `process_keyed` with the Sidechain switch on
//! detects from the key; plain `process` (or the switch off) detects from the input.

#![allow(clippy::excessive_precision)]

pub mod engine;

use engine::{display_band_hz, Engine, Focus, Settings, DISPLAY_BANDS};
use olm_api::{Kind, Meters, Module, ParamDesc};

pub const FOCUS_REGIONS: usize = 4;

pub const P_DEPTH: usize = 0;
pub const P_SHARPNESS: usize = 1;
pub const P_SELECTIVITY: usize = 2;
pub const P_ATTACK: usize = 3;
pub const P_RELEASE: usize = 4;
pub const P_FLOOR: usize = 5;
pub const P_CENTRE: usize = 6;
pub const P_WIDTH: usize = 7;
pub const P_MIX: usize = 8;
pub const P_LINK: usize = 9;
pub const P_HARD: usize = 10;
pub const P_DELTA: usize = 11;
pub const P_MID_SIDE: usize = 12;
pub const P_SIDECHAIN: usize = 13;
pub const GLOBALS: usize = 14;
/// Focus region `r` (0..4): amount, frequency, Q.
pub const fn focus_param(r: usize, field: usize) -> usize {
    GLOBALS + r * 3 + field
}
pub const NPARAMS: usize = GLOBALS + FOCUS_REGIONS * 3;

macro_rules! focus {
    ($n:literal, $f:expr) => {
        [
            ParamDesc::new(concat!("focus", $n, "Amount"), concat!("Focus ", $n, " Amount"), -100.0, 100.0, 0.0, Kind::Percent),
            ParamDesc::new(concat!("focus", $n, "Freq"), concat!("Focus ", $n, " Frequency"), 20.0, 20000.0, $f, Kind::Hz).log(),
            ParamDesc::new(concat!("focus", $n, "Q"), concat!("Focus ", $n, " Q"), 0.025, 40.0, 1.0, Kind::Plain).log(),
        ]
    };
}

const fn table() -> [ParamDesc; NPARAMS] {
    let globals = [
        ParamDesc::new("depth", "Depth", 0.0, 100.0, 0.0, Kind::Percent),
        ParamDesc::new("sharpness", "Sharpness", 0.0, 100.0, 50.0, Kind::Percent),
        ParamDesc::new("selectivity", "Selectivity", 0.0, 100.0, 35.0, Kind::Percent),
        // OpenEQ: 1 ms x 500^a and 20 ms x 100^r for controls a, r in 0..1; defaults 0.2 and 0.3.
        ParamDesc::new("attack", "Attack", 1.0, 500.0, 3.4657, Kind::Ms).log(),
        ParamDesc::new("release", "Release", 20.0, 2000.0, 79.621, Kind::Ms).log(),
        ParamDesc::new("floor", "Floor", -100.0, 0.0, -80.0, Kind::Db),
        ParamDesc::new("centre", "Region Centre", 20.0, 20000.0, 3000.0, Kind::Hz).log(),
        ParamDesc::new("width", "Region Width (octaves)", 0.1, 10.0, 10.0, Kind::Plain),
        ParamDesc::new("mix", "Mix", 0.0, 100.0, 100.0, Kind::Percent),
        ParamDesc::new("link", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
        ParamDesc::toggle("hard", "Hard Mode", false),
        ParamDesc::toggle("delta", "Delta Listen", false),
        ParamDesc::toggle("midSide", "Mid/Side", false),
        ParamDesc::toggle("sidechain", "Sidechain", false),
    ];
    let f = [focus!("1", 250.0), focus!("2", 1000.0), focus!("3", 4000.0), focus!("4", 10000.0)];
    let mut out = [globals[0]; NPARAMS];
    let mut i = 0;
    while i < GLOBALS {
        out[i] = globals[i];
        i += 1;
    }
    let mut r = 0;
    while r < FOCUS_REGIONS {
        let mut k = 0;
        while k < 3 {
            out[GLOBALS + r * 3 + k] = f[r][k];
            k += 1;
        }
        r += 1;
    }
    out
}

pub static PARAMS: [ParamDesc; NPARAMS] = table();

/// The `view()` read-out layout.
pub mod view {
    use super::engine::DISPLAY_BANDS;
    /// Reduction in dB (positive) per readout band, 120 bands log-spaced 20 Hz..20 kHz
    /// (`engine::display_band_hz`), first channel: left, or mid in Mid/Side mode.
    pub const SET_A: usize = 0;
    /// Second channel: right, or side.
    pub const SET_B: usize = DISPLAY_BANDS;
    /// Deepest reduction over both sets, dB.
    pub const DEEPEST_DB: usize = 2 * DISPLAY_BANDS;
    /// Frequency of the deepest reduction, Hz.
    pub const DEEPEST_HZ: usize = 2 * DISPLAY_BANDS + 1;
    pub const LEN: usize = 2 * DISPLAY_BANDS + 2;
}

pub struct ResonanceSuppressor {
    values: [f32; NPARAMS],
    engine: Option<Engine>,
    sample_rate: f32,
}

impl Default for ResonanceSuppressor {
    fn default() -> Self {
        let mut values = [0.0; NPARAMS];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        ResonanceSuppressor { values, engine: None, sample_rate: 48000.0 }
    }
}

impl ResonanceSuppressor {
    pub fn param_index(id: &str) -> Option<usize> {
        PARAMS.iter().position(|p| p.id == id)
    }

    fn settings(&self) -> Settings {
        let v = &self.values;
        let mut s = Settings {
            depth: v[P_DEPTH] / 100.0,
            sharpness: v[P_SHARPNESS] / 100.0,
            selectivity: v[P_SELECTIVITY] / 100.0,
            attack_s: v[P_ATTACK] / 1000.0,
            release_s: v[P_RELEASE] / 1000.0,
            floor_db: v[P_FLOOR],
            centre_hz: v[P_CENTRE],
            width_octaves: v[P_WIDTH],
            mix: v[P_MIX] / 100.0,
            link: v[P_LINK] / 100.0,
            hard: v[P_HARD] > 0.5,
            delta: v[P_DELTA] > 0.5,
            mid_side: v[P_MID_SIDE] > 0.5,
            ..Settings::default()
        };
        for r in 0..FOCUS_REGIONS {
            let amount = v[focus_param(r, 0)] / 100.0;
            if amount.abs() < 0.005 {
                continue;
            }
            s.focus[s.focus_count] = Focus { hz: v[focus_param(r, 1)], amount, q: v[focus_param(r, 2)] };
            s.focus_count += 1;
        }
        s
    }

    /// Current reduction in readout band `band` (0..120) for channel set 0 or 1, dB.
    pub fn reduction_db(&self, set: usize, band: usize) -> f32 {
        match &self.engine {
            Some(e) if band < DISPLAY_BANDS => e.display[set.min(1)][band],
            _ => 0.0,
        }
    }

    /// The deepest current reduction (dB) and its frequency (Hz).
    pub fn deepest(&self) -> (f32, f32) {
        let mut best = (0.0f32, 0.0f32);
        if let Some(e) = &self.engine {
            for set in &e.display {
                for (b, &r) in set.iter().enumerate() {
                    if r > best.0 {
                        best = (r, display_band_hz(b));
                    }
                }
            }
        }
        best
    }

    fn run(&mut self, left: &mut [f32], right: &mut [f32], key: Option<(&[f32], &[f32])>) {
        let s = self.settings();
        let keyed = if self.values[P_SIDECHAIN] > 0.5 { key } else { None };
        if let Some(e) = self.engine.as_mut() {
            let n = left.len().min(right.len());
            for v in left[..n].iter_mut().chain(right[..n].iter_mut()) {
                if !v.is_finite() {
                    *v = 0.0;
                }
            }
            e.process(&mut left[..n], &mut right[..n], &s, keyed);
        }
    }
}

impl Module for ResonanceSuppressor {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.engine = Some(Engine::new(sample_rate as f64));
    }

    fn reset(&mut self) {
        if let Some(e) = self.engine.as_mut() {
            e.reset();
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

    fn latency(&self) -> usize {
        match &self.engine {
            Some(e) => e.latency(),
            None => Engine::latency_for(self.sample_rate as f64),
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.run(left, right, None);
    }

    fn process_keyed(&mut self, left: &mut [f32], right: &mut [f32], key_left: &[f32], key_right: &[f32]) {
        self.run(left, right, Some((key_left, key_right)));
    }

    fn meters(&self) -> Meters {
        let mut m = Meters::default();
        if let Some(e) = &self.engine {
            for set in &e.display {
                for (b, &r) in set.iter().enumerate() {
                    m.reduction_db = m.reduction_db.max(r);
                    let region = b * 8 / DISPLAY_BANDS;
                    m.band_reduction_db[region] = m.band_reduction_db[region].max(r);
                }
            }
        }
        m
    }

    fn view(&self, out: &mut [f32]) -> usize {
        let n = out.len().min(view::LEN);
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = if i < 2 * DISPLAY_BANDS {
                self.reduction_db(i / DISPLAY_BANDS, i % DISPLAY_BANDS)
            } else if i == view::DEEPEST_DB {
                self.deepest().0
            } else {
                self.deepest().1
            };
        }
        n
    }
}
