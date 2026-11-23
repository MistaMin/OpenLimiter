//! The seven compression modes (gain computers and their programme-dependent envelopes), the
//! sidechain high-pass, and a pure static transfer curve per mode for an editor.
//!
//! A port of `CompressorCore.h`. Each mode takes one stereo-linked detector sample (the louder
//! channel, rectified or not) and returns a linear gain. Float/double splits and evaluation order
//! follow the original so the output matches it bit for bit.

use crate::util::*;

/// Compression mode. Indices match the original's choice parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Feed-forward peak detector with a progressive soft knee and transient relief.
    ClassAPeak = 0,
    /// Feed-forward detector blended 75/25 with a feedback path.
    HybridFfFb = 1,
    /// Feedback, slow recovery memory.
    DiodeBridge = 2,
    /// Feedback with sub-unity sidechain tracking.
    BusVca = 3,
    /// Feedback JFET shunt with ratio-dependent detector gain.
    FastFet = 4,
    /// Optical cell with three-state memory.
    Optical = 5,
    /// Feed-forward mean-square (log-cap) detector.
    RmsVca = 6,
}

impl Mode {
    pub const COUNT: usize = 7;
    pub const ALL: [Mode; 7] =
        [Mode::ClassAPeak, Mode::HybridFfFb, Mode::DiodeBridge, Mode::BusVca, Mode::FastFet, Mode::Optical, Mode::RmsVca];
    pub const LABELS: &'static [&'static str] =
        &["Class-A Peak", "Hybrid FF/FB", "Diode Bridge", "Bus VCA", "Fast FET", "Optical", "RMS VCA"];

    pub fn from_index(i: i32) -> Mode {
        Mode::ALL[i.clamp(0, 6) as usize]
    }

    pub fn label(self) -> &'static str {
        Self::LABELS[self as usize]
    }

    /// The mode's characteristic attack ms, release ms and knee dB (the original seeds each
    /// mode's remembered settings with these; an editor may offer them on a mode change).
    pub fn defaults(self) -> (f32, f32, f32) {
        match self {
            Mode::ClassAPeak => (10.0, 300.0, 6.0),
            Mode::HybridFfFb => (5.0, 150.0, 6.0),
            Mode::DiodeBridge => (2.0, 400.0, 0.0),
            Mode::BusVca => (3.0, 100.0, 0.0),
            Mode::FastFet => (0.2, 200.0, 0.0),
            Mode::Optical => (116.54, 91.85, 0.0),
            Mode::RmsVca => (34.5, 34.5, 0.0),
        }
    }

    /// Whether the mode reads the Knee control.
    pub fn uses_knee(self) -> bool {
        self == Mode::ClassAPeak
    }

    /// Whether the mode reads the Attack and Release controls.
    pub fn uses_times(self) -> bool {
        !matches!(self, Mode::Optical | Mode::RmsVca)
    }
}

/// Per-block compressor settings, as the original's `CompressorParams`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompParams {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub knee_db: f32,
    pub makeup_db: f32,
    /// 0..1.
    pub mix: f32,
    pub auto_release: bool,
    pub auto_attack: bool,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 6.0,
            makeup_db: 0.0,
            mix: 1.0,
            auto_release: false,
            auto_attack: false,
        }
    }
}

/// `1 - 1/ratio`, 1 at "infinity" (>= 99.5) and near zero.
#[inline]
pub fn compressor_slope(ratio: f32) -> f32 {
    if ratio >= 99.5 {
        return 1.0;
    }
    if ratio.abs() < 0.01 {
        return 1.0;
    }
    1.0 - 1.0 / ratio
}

/// Soft-knee gain change in dB (0 or negative for ratios above 1).
#[inline]
pub fn soft_knee_gr(input_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let slope = compressor_slope(ratio);
    let knee = jmax(0.0, knee_db);
    let over = input_db - threshold_db;
    if knee <= 0.0001 {
        return if over > 0.0 { -over * slope } else { 0.0 };
    }
    let half = knee * 0.5;
    if over <= -half {
        return 0.0;
    }
    if over >= half {
        return -over * slope;
    }
    let x = over + half;
    -(slope * x * x) / (2.0 * knee)
}

// ---------------------------------------------------------------------------------------------
// Class-A Peak
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ClassAPeak {
    sr: f64,
    p: CompParams,
    env_db: f32,
    tracker: f32,
    auto_slow: f32,
    att: f32,
    rel: f32,
    tr: f32,
    pub last_gr_db: f32,
}

impl ClassAPeak {
    fn coeff(&self, ms: f32) -> f32 {
        (-1.0f32 / (0.001f32 * jmax(0.1, ms) * self.sr as f32)).exp()
    }
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        // As the original: the slow auto-timing envelope is not cleared here.
        self.env_db = 0.0;
        self.tracker = 0.0;
        self.last_gr_db = 0.0;
    }
    pub fn update(&mut self, p: &CompParams) {
        self.p = *p;
        self.att = self.coeff(p.attack_ms);
        self.rel = self.coeff(p.release_ms);
        self.tr = self.coeff(1.5);
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let rect = x.abs();
        self.tracker = self.tr * self.tracker + (1.0 - self.tr) * rect;
        let relief = jlimit(0.0, 3.0, (rect - self.tracker) * 6.0);
        let input_db = gain_to_db(rect + 1.0e-12);
        let target = soft_knee_gr(input_db, self.p.threshold_db, self.p.ratio, self.p.knee_db) + relief;
        let rising = target < self.env_db;
        let mut rel = self.rel;
        let mut att = self.att;
        if self.p.auto_release || self.p.auto_attack {
            self.auto_slow = 0.9997 * self.auto_slow + 0.0003 * self.env_db.abs();
            let a = jlimit(0.0, 1.0, self.auto_slow / 6.0);
            if self.p.auto_release {
                rel = self.coeff(jmap(a, self.p.release_ms * 0.4, self.p.release_ms * 2.0));
            }
            if self.p.auto_attack {
                att = self.coeff(jmap(a, self.p.attack_ms * 0.3, self.p.attack_ms * 1.2));
            }
        }
        let c = if rising { att } else { rel };
        self.env_db = c * self.env_db + (1.0 - c) * jmin(0.0, target);
        self.last_gr_db = self.env_db;
        db_to_gain(self.env_db)
    }
}

// ---------------------------------------------------------------------------------------------
// Hybrid FF/FB
// ---------------------------------------------------------------------------------------------

const HYBRID_FEEDBACK: f32 = 0.25;

#[derive(Clone, Debug, Default)]
pub struct HybridFfFb {
    sr: f64,
    p: CompParams,
    ff_db: f32,
    fb_db: f32,
    auto_slow: f32,
    att: f32,
    rel: f32,
    pub last_gr_db: f32,
}

/// The hybrid mode's reduction-only hard-knee gain computer.
#[inline]
fn hybrid_gr(level: f32, threshold_db: f32, ratio: f32) -> f32 {
    let over = gain_to_db(level + 1.0e-12) - threshold_db;
    if over > 0.0 {
        jmin(0.0, -over * compressor_slope(ratio))
    } else {
        0.0
    }
}

impl HybridFfFb {
    fn coeff(&self, ms: f32) -> f32 {
        (-1.0f32 / (0.001f32 * jmax(0.1, ms) * self.sr as f32)).exp()
    }
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.ff_db = 0.0;
        self.fb_db = 0.0;
        self.auto_slow = 0.0;
        self.last_gr_db = 0.0;
    }
    pub fn update(&mut self, p: &CompParams) {
        self.p = *p;
        self.att = self.coeff(p.attack_ms);
        self.rel = self.coeff(p.release_ms);
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let target = hybrid_gr(x.abs(), self.p.threshold_db, self.p.ratio);
        let mut rel = self.rel;
        let mut att = self.att;
        if self.p.auto_release || self.p.auto_attack {
            self.auto_slow = 0.9997 * self.auto_slow + 0.0003 * self.ff_db.abs();
            let a = jlimit(0.0, 1.0, self.auto_slow / 6.0);
            if self.p.auto_release {
                rel = self.coeff(jmap(a, self.p.release_ms * 0.3, self.p.release_ms * 2.5));
            }
            if self.p.auto_attack {
                att = self.coeff(jmap(a, self.p.attack_ms * 0.3, self.p.attack_ms * 1.2));
            }
        }
        let rising = target < self.ff_db;
        let c = if rising { att } else { rel };
        self.ff_db = c * self.ff_db + (1.0 - c) * target;
        let blended = self.ff_db * (1.0 - HYBRID_FEEDBACK) + self.fb_db * HYBRID_FEEDBACK;
        let gain = db_to_gain(blended);
        // Feedback path: the level after this gain, on the release time constant; it reaches the
        // gain from the next sample on.
        self.fb_db = self.rel * self.fb_db
            + (1.0 - self.rel) * hybrid_gr(x.abs() * gain, self.p.threshold_db, self.p.ratio);
        self.last_gr_db = blended;
        gain
    }
}

// ---------------------------------------------------------------------------------------------
// Optical
// ---------------------------------------------------------------------------------------------

const OPT_WEIGHTS: [f32; 3] = [0.65467, 0.2131, 0.13223];
const OPT_R6: f32 = 68000.0;
const OPT_DARK: f32 = 2.0e-7;

#[derive(Clone, Debug, Default)]
pub struct Optical {
    sr: f64,
    cell: [f32; 3],
    attack: [f32; 3],
    release: [f32; 3],
    light: f32,
    gain: f32,
    drive: f32,
    light_coeff: f32,
    limit: bool,
    pub last_gr_db: f32,
}

impl Optical {
    fn coefficient(&self, seconds: f64) -> f32 {
        (-1.0 / (self.sr * seconds)).exp() as f32
    }
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.light_coeff = self.coefficient(0.001);
        self.attack = [self.coefficient(0.11654), self.coefficient(0.11654), self.coefficient(1.0)];
        self.release = [self.coefficient(0.01333), self.coefficient(0.09185), self.coefficient(0.69596)];
        if self.drive == 0.0 {
            self.drive = 1.0;
        }
        self.reset();
    }
    pub fn reset(&mut self) {
        self.cell = [0.0; 3];
        self.light = 0.0;
        self.gain = 1.0;
        self.last_gr_db = 0.0;
    }
    pub fn update(&mut self, p: &CompParams) {
        self.drive = db_to_gain(-p.threshold_db);
        // Ratio below 10 selects the compress configuration, 10 and above the limit one.
        self.limit = p.ratio >= 10.0;
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let r7 = if self.limit { 2700.0f32 } else { 0.0 };
        let junction = self.gain + (1.0 - self.gain) * r7 / (OPT_R6 + r7);
        let raw = jlimit(0.0, 10.0, x.abs() * self.drive * junction).powf(5.1);
        self.light = self.light_coeff * self.light + (1.0 - self.light_coeff) * raw;
        for i in 0..3 {
            let c = if self.light > self.cell[i] { self.attack[i] } else { self.release[i] };
            self.cell[i] = c * self.cell[i] + (1.0 - c) * self.light;
        }
        let state = OPT_WEIGHTS[0] * self.cell[0] + OPT_WEIGHTS[1] * self.cell[1] + OPT_WEIGHTS[2] * self.cell[2];
        let conductance = OPT_DARK + 0.001 * jmax(0.0, state).powf(0.9);
        self.gain = (1.0 + (OPT_R6 + r7) * OPT_DARK) / (1.0 + (OPT_R6 + r7) * conductance);
        self.last_gr_db = gain_to_db_floor(self.gain, -120.0);
        self.gain
    }
}

// ---------------------------------------------------------------------------------------------
// RMS VCA
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct RmsVca {
    p: CompParams,
    power: f32,
    coeff: f32,
    pub last_gr_db: f32,
}

impl RmsVca {
    pub fn prepare(&mut self, sr: f64) {
        // Constant-current log-cap release, about 126 dB/s, as one mean-square integrator.
        let tau = 10.0 / 10.0f64.ln() / 126.0;
        self.coeff = (-1.0 / (sr * tau)).exp() as f32;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.power = 1e-12;
        self.last_gr_db = 0.0;
    }
    pub fn update(&mut self, p: &CompParams) {
        self.p = *p;
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.power = self.coeff * self.power + (1.0 - self.coeff) * x * x;
        self.power = jmax(1e-12, self.power);
        let level = 10.0 * self.power.log10();
        let gain_db = rms_vca_gain_db(level, self.p.threshold_db, self.p.ratio);
        self.last_gr_db = gain_db;
        db_to_gain(gain_db)
    }
}

#[inline]
fn rms_vca_gain_db(level_db: f32, threshold_db: f32, ratio: f32) -> f32 {
    let over = level_db - threshold_db;
    let slope = 1.0 - 1.0 / jlimit(1.0, 100.0, ratio);
    // Paired -6 mV/dB detector, 1/2 divider, +2.98 mV/dB VCA; 1 % leakage below threshold.
    let control = if over >= 0.0 { over } else { 0.01 * over };
    jlimit(-100.0, 6.0, -slope * (3.0f32 / 2.98f32) * control)
}

// ---------------------------------------------------------------------------------------------
// Feedback circuit modes: Diode Bridge, Bus VCA, Fast FET
// ---------------------------------------------------------------------------------------------

const BUS_TRACKING: f32 = 8.89 / 9.53;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitKind {
    DiodeBridge,
    BusVca,
    FastFet,
}

#[derive(Clone, Debug)]
pub struct Circuit {
    kind: CircuitKind,
    sr: f64,
    p: CompParams,
    env: f32,
    memory: f32,
    gain: f32,
    drive: f32,
    attack: f32,
    release: f32,
    memory_coeff: f32,
    pub last_gr_db: f32,
}

#[inline]
fn fet_detector_gain(ratio: f32) -> f32 {
    if ratio >= 99.0 {
        10.15
    } else if ratio >= 20.0 {
        9.08
    } else if ratio >= 12.0 {
        5.23
    } else if ratio >= 8.0 {
        3.23
    } else if ratio >= 4.0 {
        2.0
    } else {
        1.0
    }
}

/// JFET shunt gain for an envelope value (small-signal channel conductance, 27k series, 180R).
#[inline]
fn fet_gain(env: f32) -> f32 {
    let gate = jlimit(-3.5, 0.0, -3.5 + 1.6 * env);
    let conductance = 2.0f32 * 0.0012 * jmax(0.0, gate + 2.5);
    1.0 / (1.0 + 27000.0 * conductance / (1.0 + 180.0 * conductance))
}

#[inline]
fn circuit_feedback_gain(env: f32, ratio: f32, tracking: f32) -> f32 {
    let over = jmax(0.0, gain_to_db_floor(env, -120.0));
    let slope = (ratio - 1.0) / tracking;
    db_to_gain(-jmin(80.0, over * slope))
}

impl Circuit {
    pub fn new(kind: CircuitKind) -> Self {
        Self {
            kind,
            sr: 44100.0,
            p: CompParams::default(),
            env: 0.0,
            memory: 0.0,
            gain: 1.0,
            drive: 1.0,
            attack: 0.0,
            release: 0.0,
            memory_coeff: 0.0,
            last_gr_db: 0.0,
        }
    }
    fn coefficient(&self, ms: f32) -> f32 {
        (-1.0f32 / (self.sr as f32 * 0.001f32 * jmax(0.02, ms))).exp()
    }
    fn default_attack(&self) -> f32 {
        match self.kind {
            CircuitKind::FastFet => 0.2,
            CircuitKind::DiodeBridge => 2.0,
            CircuitKind::BusVca => 3.0,
        }
    }
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr;
        self.reset();
    }
    pub fn reset(&mut self) {
        self.env = 0.0;
        self.memory = 0.0;
        self.gain = 1.0;
        self.last_gr_db = 0.0;
    }
    pub fn update(&mut self, p: &CompParams) {
        self.p = *p;
        self.attack = self.coefficient(if p.auto_attack { self.default_attack() } else { p.attack_ms });
        self.release = self.coefficient(p.release_ms);
        // Diode bridge recovery memory: 47 uF x 120 kOhm.
        self.memory_coeff = self.coefficient(if self.kind == CircuitKind::DiodeBridge { 5640.0 } else { 1200.0 });
        self.drive = db_to_gain(-p.threshold_db);
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let ratio = jlimit(1.0, 100.0, self.p.ratio);
        let mut detected = x.abs() * self.drive;
        match self.kind {
            CircuitKind::FastFet => detected *= self.gain * fet_detector_gain(ratio),
            CircuitKind::BusVca => detected *= self.gain.powf(BUS_TRACKING),
            CircuitKind::DiodeBridge => detected *= self.gain,
        }
        self.memory = self.memory_coeff * self.memory + (1.0 - self.memory_coeff) * self.env;
        let mut target = detected;
        if self.p.auto_release && target < self.env {
            target = jmax(target, self.memory);
        }
        let c = if target > self.env { self.attack } else { self.release };
        self.env = c * self.env + (1.0 - c) * target;
        self.gain = match self.kind {
            CircuitKind::FastFet => fet_gain(self.env),
            CircuitKind::BusVca => circuit_feedback_gain(self.env, ratio, BUS_TRACKING),
            CircuitKind::DiodeBridge => circuit_feedback_gain(self.env, ratio, 1.0),
        };
        self.last_gr_db = gain_to_db_floor(self.gain, -120.0);
        self.gain
    }
}

// ---------------------------------------------------------------------------------------------
// The pool: every mode is kept prepared; only the active one is updated and run per block.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Modes {
    pub class_a: ClassAPeak,
    pub hybrid: HybridFfFb,
    pub bridge: Circuit,
    pub bus: Circuit,
    pub fet: Circuit,
    pub optical: Optical,
    pub rms: RmsVca,
}

impl Default for Modes {
    fn default() -> Self {
        Self {
            class_a: ClassAPeak::default(),
            hybrid: HybridFfFb::default(),
            bridge: Circuit::new(CircuitKind::DiodeBridge),
            bus: Circuit::new(CircuitKind::BusVca),
            fet: Circuit::new(CircuitKind::FastFet),
            optical: Optical::default(),
            rms: RmsVca::default(),
        }
    }
}

impl Modes {
    pub fn prepare(&mut self, sr: f64) {
        self.class_a.prepare(sr);
        self.hybrid.prepare(sr);
        self.bridge.prepare(sr);
        self.bus.prepare(sr);
        self.fet.prepare(sr);
        self.optical.prepare(sr);
        self.rms.prepare(sr);
    }
    pub fn reset(&mut self) {
        self.class_a.reset();
        self.hybrid.reset();
        self.bridge.reset();
        self.bus.reset();
        self.fet.reset();
        self.optical.reset();
        self.rms.reset();
    }
    pub fn update(&mut self, mode: Mode, p: &CompParams) {
        match mode {
            Mode::ClassAPeak => self.class_a.update(p),
            Mode::HybridFfFb => self.hybrid.update(p),
            Mode::DiodeBridge => self.bridge.update(p),
            Mode::BusVca => self.bus.update(p),
            Mode::FastFet => self.fet.update(p),
            Mode::Optical => self.optical.update(p),
            Mode::RmsVca => self.rms.update(p),
        }
    }
    #[inline]
    pub fn process(&mut self, mode: Mode, x: f32) -> f32 {
        match mode {
            Mode::ClassAPeak => self.class_a.process(x),
            Mode::HybridFfFb => self.hybrid.process(x),
            Mode::DiodeBridge => self.bridge.process(x),
            Mode::BusVca => self.bus.process(x),
            Mode::FastFet => self.fet.process(x),
            Mode::Optical => self.optical.process(x),
            Mode::RmsVca => self.rms.process(x),
        }
    }
    pub fn last_gr_db(&self, mode: Mode) -> f32 {
        match mode {
            Mode::ClassAPeak => self.class_a.last_gr_db,
            Mode::HybridFfFb => self.hybrid.last_gr_db,
            Mode::DiodeBridge => self.bridge.last_gr_db,
            Mode::BusVca => self.bus.last_gr_db,
            Mode::FastFet => self.fet.last_gr_db,
            Mode::Optical => self.optical.last_gr_db,
            Mode::RmsVca => self.rms.last_gr_db,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Sidechain high-pass: 2nd-order Butterworth, 20..500 Hz.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SidechainHp {
    sr: f64,
    last_sr: f64,
    target_hz: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Default for SidechainHp {
    fn default() -> Self {
        Self { sr: 48000.0, last_sr: -1.0, target_hz: 150.0, b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: [0.0; 2], z2: [0.0; 2] }
    }
}

impl SidechainHp {
    pub fn prepare(&mut self, sr: f64) {
        self.sr = sr.max(8000.0);
        self.last_sr = -1.0;
        self.reset();
        self.set_frequency(self.target_hz);
    }
    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
    pub fn set_sample_rate(&mut self, sr: f64) {
        let c = sr.max(8000.0);
        if (c - self.sr).abs() > 0.5 {
            self.sr = c;
            self.update_coeffs();
        }
    }
    pub fn set_frequency(&mut self, hz: f32) {
        let c = jlimit(20.0, 500.0, hz);
        if (c - self.target_hz).abs() > 0.01 || (self.sr - self.last_sr).abs() > 0.5 {
            self.target_hz = c;
            self.update_coeffs();
        }
    }
    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) -> f32 {
        let ch = ch & 1;
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }
    fn update_coeffs(&mut self) {
        self.last_sr = self.sr;
        let w0 = 2.0 * std::f64::consts::PI * self.target_hz as f64 / self.sr;
        let cosw = w0.cos();
        let alpha = w0.sin() / 2.0f64.sqrt();
        let a0 = 1.0 + alpha;
        self.b0 = (((1.0 + cosw) * 0.5) / a0) as f32;
        self.b1 = (-(1.0 + cosw) / a0) as f32;
        self.b2 = self.b0;
        self.a1 = ((-2.0 * cosw) / a0) as f32;
        self.a2 = ((1.0 - alpha) / a0) as f32;
    }
}

// ---------------------------------------------------------------------------------------------
// Static curves
// ---------------------------------------------------------------------------------------------

/// Settings a static curve depends on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveParams {
    pub threshold_db: f32,
    pub ratio: f32,
    /// Read by Class-A Peak only.
    pub knee_db: f32,
    pub makeup_db: f32,
}

/// Pure steady-state transfer curves: the gain each mode settles to for a constant detector
/// level (a DC level of that many dBFS at the detector; for RMS VCA this is the RMS level), and
/// the resulting output level including makeup. Envelopes and memories have settled, so attack,
/// release and auto timing do not appear. Feedback modes are solved for their fixed point.
pub mod curve {
    use super::*;

    fn level(input_db: f32) -> f32 {
        10.0f32.powf(input_db * 0.05)
    }

    /// Bisection for the root of an increasing function on [lo, hi].
    fn solve(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64) -> f64 {
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        0.5 * (lo + hi)
    }

    pub fn class_a_peak_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        let db = gain_to_db(level(input_db) + 1.0e-12);
        jmin(0.0, soft_knee_gr(db, p.threshold_db, p.ratio, p.knee_db))
    }

    pub fn hybrid_ff_fb_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        let l = level(input_db);
        let ff = hybrid_gr(l, p.threshold_db, p.ratio) as f64;
        let b = |fb: f64| (1.0 - HYBRID_FEEDBACK as f64) * ff + HYBRID_FEEDBACK as f64 * fb;
        let fb = solve(ff - 1.0, 1.0, |fb| {
            fb - hybrid_gr(l * 10.0f64.powf(b(fb) / 20.0) as f32, p.threshold_db, p.ratio) as f64
        });
        b(fb) as f32
    }

    fn feedback_gain_db(p: &CurveParams, input_db: f32, tracking: f32) -> f32 {
        // Steady envelope E (dB) = input - threshold + tracking * G(E).
        let ratio = jlimit(1.0, 100.0, p.ratio) as f64;
        let t = tracking as f64;
        let slope = (ratio - 1.0) / t;
        let g = |e: f64| -(e.max(0.0) * slope).min(80.0);
        let lo_db = (input_db - p.threshold_db) as f64;
        let e = solve(lo_db - 81.0 * t - 1.0, lo_db + 1.0, |e| e - lo_db - t * g(e));
        g(e) as f32
    }

    pub fn diode_bridge_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        feedback_gain_db(p, input_db, 1.0)
    }

    pub fn bus_vca_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        feedback_gain_db(p, input_db, BUS_TRACKING)
    }

    pub fn fast_fet_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        let ratio = jlimit(1.0, 100.0, p.ratio);
        let k = (level(input_db) * db_to_gain(-p.threshold_db) * fet_detector_gain(ratio)) as f64;
        let env = solve(0.0, k.max(0.0) + 1.0, |e| e - k * fet_gain(e as f32) as f64);
        gain_to_db_floor(fet_gain(env as f32), -120.0)
    }

    pub fn optical_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        let r7 = if p.ratio >= 10.0 { 2700.0f64 } else { 0.0 };
        let x = (level(input_db) * db_to_gain(-p.threshold_db)) as f64;
        let r = OPT_R6 as f64 + r7;
        let dark = OPT_DARK as f64;
        let gain_for = |g: f64| {
            let junction = g + (1.0 - g) * r7 / r;
            let raw = (x * junction).clamp(0.0, 10.0).powf(5.1);
            let conductance = dark + 0.001 * raw.powf(0.9);
            (1.0 + r * dark) / (1.0 + r * conductance)
        };
        let g = solve(0.0, 1.5, |g| g - gain_for(g));
        gain_to_db_floor(g as f32, -120.0)
    }

    pub fn rms_vca_gain_db(p: &CurveParams, input_db: f32) -> f32 {
        let power = jmax(1e-12, level(input_db) * level(input_db));
        super::rms_vca_gain_db(10.0 * power.log10(), p.threshold_db, p.ratio)
    }

    /// Steady-state gain change in dB of `mode` (makeup not included).
    pub fn gain_db(mode: Mode, p: &CurveParams, input_db: f32) -> f32 {
        match mode {
            Mode::ClassAPeak => class_a_peak_gain_db(p, input_db),
            Mode::HybridFfFb => hybrid_ff_fb_gain_db(p, input_db),
            Mode::DiodeBridge => diode_bridge_gain_db(p, input_db),
            Mode::BusVca => bus_vca_gain_db(p, input_db),
            Mode::FastFet => fast_fet_gain_db(p, input_db),
            Mode::Optical => optical_gain_db(p, input_db),
            Mode::RmsVca => rms_vca_gain_db(p, input_db),
        }
    }

    /// Output level in dB for a steady input level in dB: input + gain + makeup.
    pub fn output_db(mode: Mode, p: &CurveParams, input_db: f32) -> f32 {
        input_db + gain_db(mode, p, input_db) + p.makeup_db
    }
}
