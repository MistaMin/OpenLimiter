//! Table-driven maximizer modes and character limiter types (sources D and E of
//! `spec/MASTER_BRIEF.md`, build-order step 7).
//!
//! One look-ahead limiter with thirteen styles: ten modes whose latency, attack and
//! release follow SPEC_LIMITERS.md sections 21, 30 and 39, and three character types from
//! the params table of source E. The spec gives tables, not algorithms; the internal
//! method below is this crate's own design:
//!
//! 1. The input is raised by the channel gain, the drive and the style's gain offset.
//! 2. A detector turns each sample (or, with the true-peak switch, the 4x interpolated
//!    signal around it) into the reduction in dB needed to stay under the limit.
//! 3. A sliding maximum holds that reduction over the look-ahead window plus a short hold.
//! 4. Two followers track the held reduction: a fast one and a slow one (exponential in
//!    dB, optionally with a hold). The reduction is a fixed mix of the two; the mix and the
//!    time constants come from the style and the character value (`tables::timing`).
//! 5. Two moving averages in series (a triangular window as long as the look-ahead) turn
//!    the step into a ramp that ends when the peak arrives.
//! 6. The delayed audio is multiplied by the gain, then clamped at the limit, so the
//!    sample-peak ceiling holds in every state, including while settings change.
//!
//! Nothing in `process`, `set_param` or `reset` allocates, locks or panics.

pub mod tables;

use olm_api::{Kind, Meters, Module, ParamDesc};
use tables::{timing, STYLES, STYLE_COUNT, STYLE_LABELS};

pub const P_STYLE: usize = 0;
pub const P_CHARACTER: usize = 1;
pub const P_LEARN_TARGET: usize = 2;
pub const P_LEARN_LOUDNESS: usize = 3;
pub const P_DRIVE: usize = 4;
pub const P_CEILING: usize = 5;
pub const P_TRUE_PEAK: usize = 6;
pub const P_STEREO_LINK: usize = 7;
pub const P_INPUT_LINKED: usize = 8;
pub const P_INPUT_GAIN: usize = 9;
pub const P_INPUT_GAIN_LEFT: usize = 10;
pub const P_INPUT_GAIN_RIGHT: usize = 11;
pub const P_OUTPUT_LINKED: usize = 12;
pub const P_OUTPUT_GAIN: usize = 13;
pub const P_OUTPUT_GAIN_LEFT: usize = 14;
pub const P_OUTPUT_GAIN_RIGHT: usize = 15;
pub const P_UNITY_GAIN: usize = 16;
const PARAM_COUNT: usize = 17;

/// The "nothing learned yet" value of the learn-loudness parameter.
pub const LEARN_IDLE: f32 = -70.0;

static PARAMS: [ParamDesc; PARAM_COUNT] = [
    ParamDesc::choice("style", "Style", &STYLE_LABELS, 0),
    ParamDesc::new("character", "Character", 0.0, 10.0, 2.0, Kind::Plain),
    ParamDesc::new("learnTarget", "Learn Target (LUFS)", -32.0, -1.0, -11.0, Kind::Plain),
    ParamDesc::new("learnLoudness", "Learn: Measured Loudness (LUFS)", LEARN_IDLE, 0.0, LEARN_IDLE, Kind::Plain),
    ParamDesc::new("drive", "Drive", 0.0, 20.0, 0.0, Kind::Db),
    ParamDesc::new("ceiling", "Ceiling", -20.0, 0.0, 0.0, Kind::Db),
    ParamDesc::toggle("truePeak", "True Peak Ceiling", false),
    ParamDesc::new("stereoLink", "Stereo Link", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("inputLinked", "Input Gains Linked", true),
    ParamDesc::new("inputGain", "Input Gain", -144.0, 10.0, 0.0, Kind::Db),
    ParamDesc::new("inputGainLeft", "Input Gain Left", -144.0, 10.0, 0.0, Kind::Db),
    ParamDesc::new("inputGainRight", "Input Gain Right", -144.0, 10.0, 0.0, Kind::Db),
    ParamDesc::toggle("outputLinked", "Output Gains Linked", true),
    ParamDesc::new("outputGain", "Output Gain", -144.0, 10.0, 0.0, Kind::Db),
    ParamDesc::new("outputGainLeft", "Output Gain Left", -144.0, 10.0, 0.0, Kind::Db),
    ParamDesc::new("outputGainRight", "Output Gain Right", -144.0, 10.0, 0.0, Kind::Db),
    ParamDesc::toggle("unityGain", "Unity Gain", false),
];

/// Drive that "learn" sets from a measured integrated loudness of the input.
///
/// SPEC_LIMITERS.md sections 55 to 55.5 (measurements/learn_gain/learn2.json, learn3.json,
/// learn4_repeat.json): drive = target minus loudness, limited to 0..+20 dB; a needed gain
/// of 0.38 dB gave 0.00 and 0.53 dB was applied in full, so the switch point here is 0.45.
pub fn learned_drive(target_lufs: f32, loudness_lufs: f32) -> f32 {
    let need = target_lufs - loudness_lufs;
    // `!(need >= ..)` also sends a NaN to 0.
    if !(need >= 0.45) {
        0.0
    } else {
        need.min(20.0)
    }
}

const TP_TAPS: usize = 12;
const TP_HALF: usize = TP_TAPS / 2;
const DB_TO_NEPER: f32 = core::f32::consts::LN_10 / 20.0;

#[inline]
fn db_to_lin(db: f32) -> f32 {
    (db * DB_TO_NEPER).exp()
}

fn pow2_at_least(n: usize) -> usize {
    n.max(2).next_power_of_two()
}

/// Moving average over the last `len` values, kept exact by a periodic recount.
struct Average {
    buf: Vec<f64>,
    mask: usize,
    pos: usize,
    len: usize,
    sum: f64,
}

impl Average {
    fn empty() -> Self {
        Self { buf: Vec::new(), mask: 0, pos: 0, len: 1, sum: 0.0 }
    }

    fn allocate(&mut self, max_len: usize) {
        let cap = pow2_at_least(max_len + 1);
        self.buf = vec![0.0; cap];
        self.mask = cap - 1;
        self.fill(0.0, 1);
    }

    fn fill(&mut self, value: f64, len: usize) {
        for b in self.buf.iter_mut() {
            *b = value;
        }
        self.len = len.clamp(1, self.mask.max(1));
        self.sum = value * self.len as f64;
        self.pos = 0;
    }

    #[inline]
    fn push(&mut self, value: f64) -> f64 {
        let old = self.buf[self.pos.wrapping_sub(self.len) & self.mask];
        self.buf[self.pos & self.mask] = value;
        self.pos = self.pos.wrapping_add(1);
        self.sum += value - old;
        if self.pos & self.mask == 0 {
            let mut s = 0.0;
            for k in 1..=self.len {
                s += self.buf[self.pos.wrapping_sub(k) & self.mask];
            }
            self.sum = s;
        }
        self.sum / self.len as f64
    }
}

/// Settings shared by both channels, in samples and per-sample coefficients.
#[derive(Clone, Copy)]
struct Shape {
    window: u64,
    slow_share: f32,
    slow_coef: f32,
    slow_hold: u32,
    fast_coef: f32,
    sharp_release: bool,
}

/// One channel: delay line, sliding maximum, followers and smoother.
struct Channel {
    audio: Vec<f32>,
    held_value: Vec<f32>,
    held_index: Vec<u64>,
    held_mask: usize,
    head: usize,
    tail: usize,
    fast: f32,
    slow: f32,
    hold_left: u32,
    first: Average,
    second: Average,
    last_reduction: f32,
    previous_peak: f32,
}

impl Channel {
    fn empty() -> Self {
        Self {
            audio: Vec::new(),
            held_value: Vec::new(),
            held_index: Vec::new(),
            held_mask: 0,
            head: 0,
            tail: 0,
            fast: 0.0,
            slow: 0.0,
            hold_left: 0,
            first: Average::empty(),
            second: Average::empty(),
            last_reduction: 0.0,
            previous_peak: 0.0,
        }
    }

    fn allocate(&mut self, audio_len: usize, max_window: usize, max_look: usize) {
        self.audio = vec![0.0; audio_len];
        let cap = pow2_at_least(max_window + 2);
        self.held_value = vec![0.0; cap];
        self.held_index = vec![0; cap];
        self.held_mask = cap - 1;
        self.first.allocate(max_look);
        self.second.allocate(max_look);
    }

    fn clear(&mut self, len_first: usize, len_second: usize) {
        for a in self.audio.iter_mut() {
            *a = 0.0;
        }
        self.head = 0;
        self.tail = 0;
        self.fast = 0.0;
        self.slow = 0.0;
        self.hold_left = 0;
        self.first.fill(0.0, len_first);
        self.second.fill(0.0, len_second);
        self.last_reduction = 0.0;
        self.previous_peak = 0.0;
    }

    /// New smoother lengths without a jump in the gain: both averages restart from the
    /// reduction in force. The final clamp covers the samples still in the delay line.
    fn resize(&mut self, len_first: usize, len_second: usize) {
        let now = self.last_reduction as f64;
        self.first.fill(now, len_first);
        self.second.fill(now, len_second);
        self.head = 0;
        self.tail = 0;
        self.fast = self.fast.max(self.last_reduction);
        self.slow = self.slow.max(self.last_reduction);
    }

    /// Takes the reduction needed at the detector tap, returns the reduction to apply at
    /// the output tap. Both in dB, positive.
    #[inline]
    fn step(&mut self, need: f32, now: u64, s: &Shape) -> f32 {
        // Sliding maximum over `window` samples (monotonic queue).
        let m = self.held_mask;
        while self.tail != self.head && self.held_value[self.tail.wrapping_sub(1) & m] <= need {
            self.tail = self.tail.wrapping_sub(1);
        }
        self.held_value[self.tail & m] = need;
        self.held_index[self.tail & m] = now;
        self.tail = self.tail.wrapping_add(1);
        while self.tail.wrapping_sub(self.head) > 1 && self.held_index[self.head & m].wrapping_add(s.window) <= now {
            self.head = self.head.wrapping_add(1);
        }
        let held = self.held_value[self.head & m];

        // Followers: attack at once, recover exponentially in dB towards the held value.
        self.fast = if s.fast_coef > 0.0 { held.max(self.fast * s.fast_coef) } else { held };
        if self.fast < 1.0e-6 {
            self.fast = held;
        }
        if held >= self.slow {
            self.slow = held;
            self.hold_left = s.slow_hold;
        } else if self.hold_left > 0 {
            self.hold_left -= 1;
        } else {
            self.slow = held.max(self.slow * s.slow_coef);
            if self.slow < 1.0e-6 {
                self.slow = held;
            }
        }
        let mixed = self.fast + s.slow_share * (self.slow - self.fast);

        // Triangular smoothing: the ramp ends when the delayed peak reaches the output.
        let a = self.first.push(mixed as f64);
        let b = self.second.push(a) as f32;
        let out = if s.sharp_release { b.min(mixed) } else { b };
        self.last_reduction = out.max(0.0);
        self.last_reduction
    }
}

/// The module: thirteen limiter styles behind one `Module`.
pub struct LimiterModes {
    values: [f32; PARAM_COUNT],
    sample_rate: f32,
    prepared: bool,
    left: Channel,
    right: Channel,
    audio_mask: usize,
    write: usize,
    now: u64,
    max_look: usize,
    max_window: usize,
    // Derived from the parameters.
    latency: usize,
    detector_delay: usize,
    len_first: usize,
    len_second: usize,
    shape: Shape,
    true_peak: bool,
    link: f32,
    limit: f32,
    target: f32,
    in_target: [f32; 2],
    out_target: [f32; 2],
    in_gain: [f32; 2],
    out_gain: [f32; 2],
    smooth: f32,
    unity: bool,
    power_coef: f32,
    dry_power: f32,
    wet_power: f32,
    interp: [[f32; TP_TAPS]; 3],
    reduction_db: f32,
}

impl Default for LimiterModes {
    fn default() -> Self {
        let mut values = [0.0; PARAM_COUNT];
        for (v, p) in values.iter_mut().zip(PARAMS.iter()) {
            *v = p.default;
        }
        // 4x interpolator: 12-tap Hann-windowed sinc per phase, unity gain at DC.
        let mut interp = [[0.0f32; TP_TAPS]; 3];
        for (phase, taps) in interp.iter_mut().enumerate() {
            let frac = (phase + 1) as f64 * 0.25;
            let mut sum = 0.0;
            let mut raw = [0.0f64; TP_TAPS];
            for (k, r) in raw.iter_mut().enumerate() {
                // Tap k weights the sample at offset (k - 5) from the centre sample.
                let u = frac - (k as f64 - 5.0);
                let x = core::f64::consts::PI * u;
                let sinc = if x.abs() < 1.0e-12 { 1.0 } else { x.sin() / x };
                *r = sinc * (0.5 + 0.5 * (x / TP_HALF as f64).cos());
                sum += *r;
            }
            for (t, r) in taps.iter_mut().zip(raw.iter()) {
                *t = (*r / sum) as f32;
            }
        }
        let mut s = Self {
            values,
            sample_rate: 48_000.0,
            prepared: false,
            left: Channel::empty(),
            right: Channel::empty(),
            audio_mask: 0,
            write: 0,
            now: 0,
            max_look: 1,
            max_window: 1,
            latency: 0,
            detector_delay: 0,
            len_first: 1,
            len_second: 1,
            shape: Shape { window: 1, slow_share: 0.0, slow_coef: 0.0, slow_hold: 0, fast_coef: 0.0, sharp_release: false },
            true_peak: false,
            link: 1.0,
            limit: 1.0,
            target: 1.0,
            in_target: [1.0; 2],
            out_target: [1.0; 2],
            in_gain: [1.0; 2],
            out_gain: [1.0; 2],
            smooth: 1.0,
            unity: false,
            power_coef: 0.0,
            dry_power: 0.0,
            wet_power: 0.0,
            interp,
            reduction_db: 0.0,
        };
        s.configure();
        s
    }
}

impl LimiterModes {
    pub fn new() -> Self {
        Self::default()
    }

    /// "Learn drive": hands in the integrated loudness of the input (LUFS, measured by the
    /// caller, for example with a BS.1770 meter) and sets the drive by `learned_drive`.
    /// Returns the new drive in dB. Same as setting the `learnLoudness` parameter.
    pub fn learn_drive(&mut self, integrated_lufs: f32) -> f32 {
        let drive = learned_drive(self.values[P_LEARN_TARGET], integrated_lufs);
        self.values[P_DRIVE] = drive;
        self.configure();
        drive
    }

    /// The value a parameter holds now (after clamping).
    pub fn param(&self, index: usize) -> f32 {
        if index < PARAM_COUNT {
            self.values[index]
        } else {
            0.0
        }
    }

    fn style(&self) -> usize {
        (self.values[P_STYLE].round() as usize).min(STYLE_COUNT - 1)
    }

    fn ms_to_samples(&self, ms: f32) -> usize {
        (ms * 0.001 * self.sample_rate).round().max(0.0) as usize
    }

    fn coef(&self, tau_ms: f32) -> f32 {
        let n = tau_ms * 0.001 * self.sample_rate;
        if n < 1.0 {
            0.0
        } else {
            (-1.0 / n).exp()
        }
    }

    /// Turns the parameter values into everything `process` reads. No allocation.
    fn configure(&mut self) {
        let v = self.values;
        let style = self.style();
        let spec = &STYLES[style];
        let t = timing(style, v[P_CHARACTER]);
        self.true_peak = v[P_TRUE_PEAK] >= 0.5;

        // Latency: table values are for 48 kHz and are kept constant in time at other rates.
        let scale = self.sample_rate / 48_000.0;
        let mut latency = (spec.latency_48k as f32 * scale).round() as usize;
        if self.true_peak {
            latency += ((spec.true_peak_extra_48k as f32 * scale).round() as usize).max(TP_HALF + 1);
        }
        let spare = if self.true_peak { TP_HALF + 1 } else { 0 };
        let early = self.ms_to_samples(t.early_ms).min(latency.saturating_sub(spare + 1));
        let look = self
            .ms_to_samples(t.look_ms)
            .clamp(1, self.max_look.max(1))
            .min(latency.saturating_sub(spare + early).max(1));
        let hold = self.ms_to_samples(t.hold_ms);
        // The window must reach from the output sample to the end of the ramp.
        let window = look + early + hold;
        let len_first = (look + 1) / 2;
        let len_second = look + 1 - len_first;
        self.latency = latency;
        self.detector_delay = latency.saturating_sub(look - 1 + early);
        self.shape = Shape {
            window: window as u64,
            slow_share: t.slow_share.clamp(0.0, 1.0),
            slow_coef: self.coef(t.slow_tau_ms),
            slow_hold: self.ms_to_samples(t.slow_hold_ms) as u32,
            fast_coef: self.coef(t.fast_tau_ms),
            sharp_release: t.sharp_release,
        };
        if len_first != self.len_first || len_second != self.len_second {
            self.len_first = len_first;
            self.len_second = len_second;
            if self.prepared {
                self.left.resize(len_first, len_second);
                self.right.resize(len_first, len_second);
            }
        }

        self.link = (v[P_STEREO_LINK] * 0.01).clamp(0.0, 1.0);
        self.limit = db_to_lin(spec.limit_db);
        // With the true-peak ceiling on, no style lets a tone over the limit (own choice;
        // the third type was not measured with the switch on).
        // The 0.05 dB margin covers the ripple of the 12-tap interpolator.
        self.target = db_to_lin(spec.limit_db + if self.true_peak { -0.05 } else { t.over_db });
        self.unity = v[P_UNITY_GAIN] >= 0.5;

        // Channel gains: linked uses the one gain on both channels, unlinked the two
        // per-channel gains; the other set keeps its value (SPEC_LIMITERS.md 68.1).
        let pre = v[P_DRIVE] + spec.offset_db;
        let (il, ir) = if v[P_INPUT_LINKED] >= 0.5 {
            (v[P_INPUT_GAIN], v[P_INPUT_GAIN])
        } else {
            (v[P_INPUT_GAIN_LEFT], v[P_INPUT_GAIN_RIGHT])
        };
        let (ol, or) = if v[P_OUTPUT_LINKED] >= 0.5 {
            (v[P_OUTPUT_GAIN], v[P_OUTPUT_GAIN])
        } else {
            (v[P_OUTPUT_GAIN_LEFT], v[P_OUTPUT_GAIN_RIGHT])
        };
        self.in_target = [db_to_lin(il + pre), db_to_lin(ir + pre)];
        self.out_target = [db_to_lin(ol + v[P_CEILING]), db_to_lin(or + v[P_CEILING])];
        if !self.prepared {
            self.in_gain = self.in_target;
            self.out_gain = self.out_target;
        }
    }

    /// Peak of the signal around the detector tap `centre` (a ring position).
    #[inline]
    fn detect(&self, ch: &Channel, centre: usize) -> (f32, f32) {
        let mask = self.audio_mask;
        let here = ch.audio[centre & mask].abs();
        if !self.true_peak {
            return (here, here);
        }
        // Interpolated values between the centre sample and the next one.
        let mut peak = here;
        for taps in self.interp.iter() {
            let mut acc = 0.0f32;
            for (k, t) in taps.iter().enumerate() {
                acc += t * ch.audio[centre.wrapping_add(k).wrapping_sub(5) & mask];
            }
            peak = peak.max(acc.abs());
        }
        // An inter-sample peak belongs to both neighbours: cover the previous gap too.
        (peak.max(ch.previous_peak), peak)
    }

    #[inline]
    fn need(&self, peak: f32) -> f32 {
        if peak > self.target {
            (peak / self.target).ln() / DB_TO_NEPER
        } else {
            0.0
        }
    }
}

impl Module for LimiterModes {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = if sample_rate.is_finite() && sample_rate >= 1_000.0 { sample_rate } else { 48_000.0 };
        let scale = self.sample_rate / 48_000.0;
        let mut longest = 0usize;
        for s in STYLES.iter() {
            let n = ((s.latency_48k + s.true_peak_extra_48k) as f32 * scale).round() as usize;
            longest = longest.max(n);
        }
        let audio_len = pow2_at_least(longest + 2 * TP_TAPS + 8);
        self.max_look = self.ms_to_samples(tables::MAX_LOOK_MS).max(1);
        self.max_window = self.max_look + self.ms_to_samples(tables::MAX_HOLD_MS + tables::MAX_EARLY_MS) + 2;
        self.left.allocate(audio_len, self.max_window, self.max_look);
        self.right.allocate(audio_len, self.max_window, self.max_look);
        self.audio_mask = audio_len - 1;
        // Gain steps are spread over about 5 ms; the detector sees the smoothed input gain.
        self.smooth = 1.0 - (-1.0 / (0.005 * self.sample_rate)).exp();
        self.power_coef = 1.0 - (-1.0 / (0.3 * self.sample_rate)).exp();
        self.prepared = true;
        self.configure();
        self.reset();
    }

    fn reset(&mut self) {
        self.left.clear(self.len_first, self.len_second);
        self.right.clear(self.len_first, self.len_second);
        self.write = 0;
        self.now = 0;
        self.in_gain = self.in_target;
        self.out_gain = self.out_target;
        self.dry_power = 0.0;
        self.wet_power = 0.0;
        self.reduction_db = 0.0;
    }

    fn set_param(&mut self, index: usize, value: f32) {
        if index >= PARAM_COUNT || value.is_nan() {
            return;
        }
        let p = &PARAMS[index];
        let mut v = value.clamp(p.min, p.max);
        if p.steps > 0 {
            v = v.round();
        }
        self.values[index] = v;
        // A loudness handed in above the idle value sets the drive (learn). The parameter
        // sits before the drive, so a host restoring all values in order ends on the
        // stored drive.
        if index == P_LEARN_LOUDNESS && v > LEARN_IDLE {
            self.values[P_DRIVE] = learned_drive(self.values[P_LEARN_TARGET], v);
        }
        self.configure();
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        if !self.prepared || self.left.audio.is_empty() || self.right.audio.is_empty() {
            return;
        }
        let mask = self.audio_mask;
        let shape = self.shape;
        let mut deepest = 0.0f32;
        for i in 0..n {
            // Input gain, drive and style offset, then into the delay lines.
            self.in_gain[0] += self.smooth * (self.in_target[0] - self.in_gain[0]);
            self.in_gain[1] += self.smooth * (self.in_target[1] - self.in_gain[1]);
            self.out_gain[0] += self.smooth * (self.out_target[0] - self.out_gain[0]);
            self.out_gain[1] += self.smooth * (self.out_target[1] - self.out_gain[1]);
            let w = self.write;
            let (dry_l, dry_r) = (left[i], right[i]);
            let xl = if dry_l.is_finite() { dry_l * self.in_gain[0] } else { 0.0 };
            let xr = if dry_r.is_finite() { dry_r * self.in_gain[1] } else { 0.0 };
            self.left.audio[w & mask] = xl;
            self.right.audio[w & mask] = xr;

            // Detector at its own tap, ahead of the output tap by the look-ahead.
            let centre = w.wrapping_sub(self.detector_delay);
            let (pl, raw_l) = self.detect(&self.left, centre);
            let (pr, raw_r) = self.detect(&self.right, centre);
            self.left.previous_peak = raw_l;
            self.right.previous_peak = raw_r;
            let (nl, nr) = (self.need(pl), self.need(pr));
            let both = nl.max(nr);
            let rl = self.left.step(nl + self.link * (both - nl), self.now, &shape);
            let rr = self.right.step(nr + self.link * (both - nr), self.now, &shape);
            deepest = deepest.max(rl).max(rr);

            // Output tap: gain, then the clamp that makes the ceiling unconditional.
            let out = w.wrapping_sub(self.latency);
            let lim = self.limit;
            let dl = self.left.audio[out & mask];
            let dr = self.right.audio[out & mask];
            // The second clamp keeps the ceiling while the output gain glides to a lower value.
            let (top_l, top_r) = (lim * self.out_target[0], lim * self.out_target[1]);
            let mut yl = ((dl * db_to_lin(-rl)).clamp(-lim, lim) * self.out_gain[0]).clamp(-top_l, top_l);
            let mut yr = ((dr * db_to_lin(-rr)).clamp(-lim, lim) * self.out_gain[1]).clamp(-top_r, top_r);

            // Level tracking for unity gain: the delayed input before any gain against the
            // output, both channels together (SPEC_LIMITERS.md 68.2).
            let il = if self.in_gain[0] > 1.0e-6 { dl / self.in_gain[0] } else { 0.0 };
            let ir = if self.in_gain[1] > 1.0e-6 { dr / self.in_gain[1] } else { 0.0 };
            self.dry_power += self.power_coef * (il * il + ir * ir - self.dry_power);
            self.wet_power += self.power_coef * (yl * yl + yr * yr - self.wet_power);
            if self.unity && self.wet_power > 1.0e-20 {
                let g = (self.dry_power / self.wet_power).sqrt().min(1.0);
                yl *= g;
                yr *= g;
            }
            left[i] = yl;
            right[i] = yr;
            self.write = w.wrapping_add(1);
            self.now = self.now.wrapping_add(1);
        }
        if self.dry_power < 1.0e-30 {
            self.dry_power = 0.0;
        }
        if self.wet_power < 1.0e-30 {
            self.wet_power = 0.0;
        }
        self.reduction_db = deepest;
    }

    fn meters(&self) -> Meters {
        Meters { reduction_db: self.reduction_db, band_reduction_db: [0.0; 8] }
    }
}
