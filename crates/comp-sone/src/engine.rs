//! Line-by-line port of DualSone's `Source/dsp/DualLoudnessCore.h` (v1.1.1), stereo only.
//!
//! The float/double split of the original is kept on purpose (every `float` there is `f32`
//! here, every `double` is `f64`), so that the output can be compared sample by sample with
//! the C++ build. See that header for the design notes; the comments here are short.
//!
//! Audio path:  out[n] = x[n - latency] * (mix * g[n] + (1 - mix)) * output * autogain
//! Delta:       out[n] = x[n - latency] * (1 - (mix * g[n] + (1 - mix))) * (autogain ? 1 / input : 1)

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db * 0.05)
}

/// BS.1770 loudness of a channel-summed mean square.
#[inline]
pub fn loudness_of(mean_square: f64) -> f32 {
    -0.691f32 + 10.0f32 * ((mean_square + 1.0e-20).log10() as f32)
}

/// Soft-knee downward gain computer in dB; returns gain <= 0.
#[inline]
pub fn gain_computer(level: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let over = level - threshold;
    let slope = 1.0f32 / ratio - 1.0;
    if 2.0 * over <= -knee {
        return 0.0;
    }
    if 2.0 * over.abs() < knee {
        return slope * (over + 0.5 * knee) * (over + 0.5 * knee) / (2.0 * knee);
    }
    slope * over
}

#[inline]
fn one_pole_coeff(seconds: f64, fs: f64) -> f64 {
    if seconds <= 0.0 {
        0.0
    } else {
        (-1.0 / (seconds * fs)).exp()
    }
}

// std::max / std::min semantics (first argument wins ties and NaN comparisons).
#[inline]
fn maxf(a: f32, b: f32) -> f32 {
    if a < b { b } else { a }
}
#[inline]
fn minf(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}
#[inline]
fn maxd(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}
#[inline]
fn mind(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}
#[inline]
fn lround(x: f64) -> i64 {
    x.round() as i64
}
#[inline]
fn odd_at_least_1(n: i32) -> i32 {
    (n | 1).max(1)
}

//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
    z1: f64,
    z2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }
}

impl Biquad {
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// BS.1770 K-weighting (pre-filter shelf + RLB high-pass), from the analog prototype.
#[derive(Clone, Copy, Debug, Default)]
pub struct KWeighting {
    pub shelf: Biquad,
    pub high_pass: Biquad,
}

impl KWeighting {
    pub fn prepare(&mut self, fs: f64) {
        let pi = 3.14159265358979323846f64;
        {
            let f0 = 1681.974450955533f64;
            let gain_db = 3.999843853973347f64;
            let q = 0.7071752369554196f64;
            let k = (pi * f0 / fs).tan();
            let vh = 10.0f64.powf(gain_db / 20.0);
            let vb = vh.powf(0.4996667741545416);
            let a0 = 1.0 + k / q + k * k;
            self.shelf.b0 = (vh + vb * k / q + k * k) / a0;
            self.shelf.b1 = 2.0 * (k * k - vh) / a0;
            self.shelf.b2 = (vh - vb * k / q + k * k) / a0;
            self.shelf.a1 = 2.0 * (k * k - 1.0) / a0;
            self.shelf.a2 = (1.0 - k / q + k * k) / a0;
        }
        {
            let f0 = 38.13547087602444f64;
            let q = 0.5003270373238773f64;
            let k = (pi * f0 / fs).tan();
            let a0 = 1.0 + k / q + k * k;
            self.high_pass.b0 = 1.0;
            self.high_pass.b1 = -2.0;
            self.high_pass.b2 = 1.0;
            self.high_pass.a1 = 2.0 * (k * k - 1.0) / a0;
            self.high_pass.a2 = (1.0 - k / q + k * k) / a0;
        }
        self.reset();
    }
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let s = self.shelf.process(x);
        self.high_pass.process(s)
    }
    pub fn reset(&mut self) {
        self.shelf.reset();
        self.high_pass.reset();
    }
}

//==============================================================================
/// Running mean over the last `len` samples, length glides one sample at a time.
#[derive(Default)]
struct MovingMean {
    buf: Vec<f64>,
    pos: i32,
    len: i32,
    fresh_count: i32,
    sum: f64,
    fresh: f64,
}

impl MovingMean {
    fn new() -> Self {
        Self { len: 1, ..Default::default() }
    }
    fn prepare(&mut self, max_length: i32) {
        let max_length = max_length.max(1);
        self.buf = vec![0.0; max_length as usize + 1];
        self.len = self.len.clamp(1, max_length);
        self.reset();
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.sum = 0.0;
        self.fresh = 0.0;
        self.fresh_count = 0;
    }
    fn length(&self) -> i32 {
        self.len
    }
    fn max_length(&self) -> i32 {
        (self.buf.len() as i32 - 1).max(1)
    }
    fn set_length(&mut self, new_length: i32) {
        if self.buf.is_empty() {
            return;
        }
        let new_length = new_length.clamp(1, self.max_length());
        while self.len < new_length {
            self.len += 1;
            self.sum += self.at(self.len);
        }
        while self.len > new_length {
            self.sum -= self.at(self.len);
            self.len -= 1;
        }
        self.fresh = 0.0;
        self.fresh_count = 0;
    }
    #[inline]
    fn push(&mut self, x: f64) -> f64 {
        let size = self.buf.len() as i32;
        if size == 0 {
            return x;
        }
        let pos = self.pos as usize;
        if let Some(slot) = self.buf.get_mut(pos) {
            *slot = x;
        }
        let mut out = self.pos - self.len;
        if out < 0 {
            out += size;
        }
        self.sum += x - self.buf.get(out as usize).copied().unwrap_or(0.0);
        self.pos = if self.pos + 1 == size { 0 } else { self.pos + 1 };

        self.fresh += x;
        self.fresh_count += 1;
        if self.fresh_count == self.len {
            self.sum = self.fresh;
            self.fresh = 0.0;
            self.fresh_count = 0;
        }
        self.sum / self.len as f64
    }
    // The sample pushed k pushes ago (k = 1 is the newest).
    fn at(&self, k: i32) -> f64 {
        let mut i = self.pos - k;
        if i < 0 {
            i += self.buf.len() as i32;
        }
        self.buf.get(i as usize).copied().unwrap_or(0.0)
    }
}

/// Minimum over the last `len` samples (monotonic deque in a fixed ring).
#[derive(Default)]
struct SlidingMin {
    times: Vec<i64>,
    values: Vec<f32>,
    cap: i32,
    head: i32,
    count: i32,
    len: i32,
    now: i64,
}

impl SlidingMin {
    fn new() -> Self {
        Self { cap: 1, len: 1, ..Default::default() }
    }
    fn prepare(&mut self, max_length: i32) {
        let max_length = max_length.max(1);
        self.cap = max_length + 1;
        self.times = vec![0; self.cap as usize];
        self.values = vec![0.0; self.cap as usize];
        self.len = self.len.clamp(1, max_length);
        self.reset();
    }
    fn reset(&mut self) {
        self.head = 0;
        self.count = 0;
        self.now = 0;
    }
    fn set_length(&mut self, new_length: i32) {
        self.len = new_length.clamp(1, (self.cap - 1).max(1));
    }
    #[inline]
    fn push(&mut self, v: f32) -> f32 {
        let cap = self.cap;
        if self.values.len() != cap as usize || cap < 2 {
            return v;
        }
        self.now += 1;
        while self.count > 0 && self.values[self.back() as usize] >= v {
            self.count -= 1;
        }
        let slot = ((self.head + self.count) % cap) as usize;
        self.times[slot] = self.now;
        self.values[slot] = v;
        self.count += 1;
        while self.times[self.head as usize] <= self.now - self.len as i64 {
            self.head = if self.head + 1 == cap { 0 } else { self.head + 1 };
            self.count -= 1;
        }
        self.values[self.head as usize]
    }
    #[inline]
    fn back(&self) -> i32 {
        (self.head + self.count - 1) % self.cap
    }
}

#[derive(Default)]
struct DelayLine<T: Copy + Default> {
    buf: Vec<T>,
    pos: usize,
}

impl<T: Copy + Default> DelayLine<T> {
    fn prepare(&mut self, max_delay: i32) {
        self.buf = vec![T::default(); max_delay.max(0) as usize + 1];
        self.pos = 0;
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = T::default());
        self.pos = 0;
    }
    /// Writes x and returns the sample written `delay` pushes ago (delay 0 returns x).
    #[inline]
    fn push(&mut self, x: T, delay: i32) -> T {
        let size = self.buf.len();
        if size == 0 {
            return x;
        }
        let delay = (delay.max(0) as usize).min(size - 1);
        let pos = self.pos.min(size - 1);
        self.buf[pos] = x;
        let r = if pos >= delay { pos - delay } else { pos + size - delay };
        self.pos = if pos + 1 == size { 0 } else { pos + 1 };
        self.buf[r]
    }
}

//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Parallel,
    Series,
    FastLed,
    Mix,
}

impl Mode {
    pub fn from_index(i: i32) -> Mode {
        match i {
            1 => Mode::Series,
            2 => Mode::FastLed,
            3 => Mode::Mix,
            _ => Mode::Parallel,
        }
    }
}

#[derive(Clone, Copy)]
struct ModeWeights {
    parallel: f32,
    series: f32,
    fast_led: f32,
}

fn weights_for(m: Mode) -> ModeWeights {
    match m {
        Mode::Series => ModeWeights { parallel: 0.0, series: 1.0, fast_led: 0.0 },
        Mode::FastLed => ModeWeights { parallel: 0.0, series: 0.0, fast_led: 1.0 },
        Mode::Mix => ModeWeights { parallel: 0.5, series: 0.0, fast_led: 0.5 },
        Mode::Parallel => ModeWeights { parallel: 1.0, series: 0.0, fast_led: 0.0 },
    }
}

/// The engine's settings, in the core's own units (like `dlc::Settings`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub k_weighted: bool,
    pub slow_on: bool,
    pub threshold_db: f32,
    pub slow_ratio: f32,
    pub slow_attack_ms: f32,
    pub slow_release_ms: f32,
    pub slow_window_seconds: f32,
    pub fast_on: bool,
    pub fast_crest_db: f32,
    pub fast_ratio: f32,
    pub fast_attack_ms: f32,
    pub fast_release_ms: f32,
    pub lookahead: f32,
    pub mode: Mode,
    pub mix: f32,
    pub output_db: f32,
    pub input_db: f32,
    pub auto_gain: bool,
    pub learn_auto_gain: bool,
    pub delta: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            k_weighted: true,
            slow_on: true,
            threshold_db: -18.0,
            slow_ratio: 2.0,
            slow_attack_ms: 800.0,
            slow_release_ms: 2500.0,
            slow_window_seconds: 0.4,
            fast_on: true,
            fast_crest_db: 6.0,
            fast_ratio: 3.0,
            fast_attack_ms: 10.0,
            fast_release_ms: 100.0,
            lookahead: 1.0,
            mode: Mode::Parallel,
            mix: 1.0,
            output_db: 0.0,
            input_db: 0.0,
            auto_gain: false,
            learn_auto_gain: true,
            delta: false,
        }
    }
}

impl Settings {
    // Exact comparison of the settings that shape the compression.
    fn same_compression(a: &Settings, b: &Settings) -> bool {
        let fa = [
            a.threshold_db, a.slow_ratio, a.slow_attack_ms, a.slow_release_ms, a.slow_window_seconds, a.fast_crest_db,
            a.fast_ratio, a.fast_attack_ms, a.fast_release_ms, a.lookahead, a.mix, a.input_db,
        ];
        let fb = [
            b.threshold_db, b.slow_ratio, b.slow_attack_ms, b.slow_release_ms, b.slow_window_seconds, b.fast_crest_db,
            b.fast_ratio, b.fast_attack_ms, b.fast_release_ms, b.lookahead, b.mix, b.input_db,
        ];
        a.k_weighted == b.k_weighted
            && a.slow_on == b.slow_on
            && a.fast_on == b.fast_on
            && a.mode == b.mode
            && fa.iter().zip(fb.iter()).all(|(x, y)| x == y)
    }
}

/// Optional per-sample traces (any may be `None`), like `dlc::Trace`.
#[derive(Default)]
pub struct Trace<'a> {
    pub slow_gain_db: Option<&'a mut [f32]>,
    pub fast_gain_db: Option<&'a mut [f32]>,
    pub slow_loudness: Option<&'a mut [f32]>,
    pub fast_loudness: Option<&'a mut [f32]>,
    pub reference: Option<&'a mut [f32]>,
    pub fast_target_db: Option<&'a mut [f32]>,
    pub slow_engine_loudness: Option<&'a mut [f32]>,
}

#[inline]
fn put(dst: &mut Option<&mut [f32]>, n: usize, v: f32) {
    if let Some(s) = dst.as_deref_mut() {
        if let Some(x) = s.get_mut(n) {
            *x = v;
        }
    }
}

// Fast gain path: min-hold, release, two boxes and the alignment delay.
struct FastLane {
    hold: SlidingMin,
    box1: MovingMean,
    box2: MovingMean,
    delay: DelayLine<f32>,
    held: f64,
    last_hold: f32,
}

impl FastLane {
    fn new() -> Self {
        Self {
            hold: SlidingMin::new(),
            box1: MovingMean::new(),
            box2: MovingMean::new(),
            delay: DelayLine::default(),
            held: 0.0,
            last_hold: 0.0,
        }
    }
    fn prepare(&mut self, max_hold: i32, max_box: i32, max_delay: i32) {
        self.hold.prepare(max_hold);
        self.box1.prepare(max_box);
        self.box2.prepare(max_box);
        self.delay.prepare(max_delay);
    }
    fn reset(&mut self) {
        self.hold.reset();
        self.box1.reset();
        self.box2.reset();
        self.delay.reset();
        self.held = 0.0;
        self.last_hold = 0.0;
    }
    fn set_lengths(&mut self, hold_length: i32, box_length: i32) {
        self.hold.set_length(hold_length);
        self.box1.set_length(box_length);
        self.box2.set_length(box_length);
    }
    #[inline]
    fn push(&mut self, target: f32, release_coeff: f64, delay_samples: i32) -> f32 {
        let m = self.hold.push(target);
        let falling = minf(0.0, m - self.last_hold);
        self.last_hold = m;
        if (m as f64) <= self.held {
            self.held = m as f64;
        } else {
            self.held = m as f64 + release_coeff * (self.held - m as f64);
            let closeness = maxd(0.0, 1.0 - (m as f64 - self.held) / Core::JOIN_WIDTH_DB);
            self.held = mind(m as f64, self.held + closeness * closeness * falling as f64);
        }
        let b1 = self.box1.push(self.held);
        let b2 = self.box2.push(b1);
        self.delay.push(b2 as f32, delay_samples)
    }
}

/// The DualSone engine (`dlc::DualLoudnessCore`), stereo.
pub struct Core {
    settings: Settings,
    fs: f64,
    prepared: bool,
    h_max: i32,
    fast_window: i32,
    ripple_hold: i32,
    latency: i32,
    h: i32,
    h_target: i32,
    gain_delay_samples: i32,
    glide_counter: u32,

    kw: [KWeighting; 2],
    audio_delay: [DelayLine<f32>; 2],
    fast_mean: MovingMean,
    slow_mean: MovingMean,
    slow_active: MovingMean,
    series_mean: MovingMean,
    led_mean: MovingMean,
    ref_hold: SlidingMin,
    crest_lane: FastLane,
    level_lane: FastLane,
    ref_delay: DelayLine<f64>,
    active_delay: DelayLine<f64>,
    post_delay: DelayLine<f64>,
    post_k_delay: DelayLine<f64>,
    post_active: DelayLine<f64>,

    slow_attack_coeff: f64,
    slow_release_coeff: f64,
    fast_release_coeff: f64,
    long_term_coeff: f64,
    smooth_coeff: f64,
    mode_coeff: f64,
    w_par: f64,
    w_ser: f64,
    w_led: f64,
    long_term: f64,
    long_term_known: bool,
    ref_held: f32,
    entry_count: i32,
    g_s: f64,
    g_series: f64,
    g_led: f64,
    mix_smoothed: f64,
    out_smoothed: f64,
    in_smoothed: f64,

    ag_in: f64,
    ag_gain_db: f64,
    ag_smoothed: f64,
    delta_smoothed: f64,
    delta_trim_smoothed: f64,
    ag_count: i32,
    auto_gain_window: i32,
    auto_gain_relearn: i32,
    auto_gain_min: i32,

    /// Values of the last processed block.
    pub meter_slow_gr: f32,
    pub meter_fast_gr: f32,
    pub meter_loudness: f32,
    pub meter_crest: f32,
    pub meter_auto_gain_db: f32,
    /// Not in the original: the deepest slow + fast gain of the last block (dB, <= 0).
    pub meter_total_gr: f32,
    /// Not in the original: the fast engine's reference loudness (what the crest is measured
    /// from) at the end of the last block.
    pub meter_reference: f32,
    /// Not in the original: the highest 5 ms loudness of the last block.
    pub meter_fast_level: f32,
    /// Not in the original: the slow engine was holding its gain (programme paused) at the
    /// end of the last block.
    pub meter_hold: bool,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    pub const MAX_ATTACK_SECONDS: f64 = 0.030;
    pub const FAST_WINDOW_SECONDS: f64 = 0.005;
    pub const RIPPLE_HOLD_SECONDS: f64 = 0.020;
    pub const JOIN_WIDTH_DB: f64 = 6.0;
    pub const SLOW_KNEE_DB: f32 = 10.0;
    pub const FAST_KNEE_DB: f32 = 6.0;
    pub const ABSOLUTE_GATE: f32 = -70.0;
    pub const RELATIVE_GATE: f32 = 10.0;
    pub const ACTIVITY_GATE: f32 = 20.0;
    pub const LONG_TERM_SECONDS: f64 = 4.0;
    pub const REF_HOLD_SECONDS: f64 = 2.0;
    pub const AUTO_GAIN_SECONDS: f64 = 10.0;
    pub const AUTO_GAIN_RELEARN_SECONDS: f64 = 0.5;
    pub const AUTO_GAIN_MIN_SECONDS: f64 = 0.4;
    pub const AUTO_GAIN_RAMP: i32 = 4;
    pub const AUTO_GAIN_MAX_DB: f32 = 24.0;

    pub fn new() -> Self {
        Self {
            settings: Settings::default(),
            fs: 48000.0,
            prepared: false,
            h_max: 1,
            fast_window: 1,
            ripple_hold: 0,
            latency: 0,
            h: 1,
            h_target: 1,
            gain_delay_samples: 0,
            glide_counter: 0,
            kw: [KWeighting::default(); 2],
            audio_delay: [DelayLine::default(), DelayLine::default()],
            fast_mean: MovingMean::new(),
            slow_mean: MovingMean::new(),
            slow_active: MovingMean::new(),
            series_mean: MovingMean::new(),
            led_mean: MovingMean::new(),
            ref_hold: SlidingMin::new(),
            crest_lane: FastLane::new(),
            level_lane: FastLane::new(),
            ref_delay: DelayLine::default(),
            active_delay: DelayLine::default(),
            post_delay: DelayLine::default(),
            post_k_delay: DelayLine::default(),
            post_active: DelayLine::default(),
            slow_attack_coeff: 0.0,
            slow_release_coeff: 0.0,
            fast_release_coeff: 0.0,
            long_term_coeff: 0.0,
            smooth_coeff: 0.0,
            mode_coeff: 0.0,
            w_par: 1.0,
            w_ser: 0.0,
            w_led: 0.0,
            long_term: -200.0,
            long_term_known: false,
            ref_held: -200.0,
            entry_count: 0,
            g_s: 0.0,
            g_series: 0.0,
            g_led: 0.0,
            mix_smoothed: 1.0,
            out_smoothed: 1.0,
            in_smoothed: 1.0,
            ag_in: 0.0,
            ag_gain_db: 0.0,
            ag_smoothed: 1.0,
            delta_smoothed: 0.0,
            delta_trim_smoothed: 1.0,
            ag_count: 0,
            auto_gain_window: 1,
            auto_gain_relearn: 1,
            auto_gain_min: 1,
            meter_slow_gr: 0.0,
            meter_fast_gr: 0.0,
            meter_loudness: -100.0,
            meter_crest: -100.0,
            meter_auto_gain_db: 0.0,
            meter_total_gr: 0.0,
            meter_reference: -100.0,
            meter_fast_level: -100.0,
            meter_hold: false,
        }
    }

    pub fn is_prepared(&self) -> bool {
        self.prepared
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        let fs = sample_rate;
        self.fs = fs;
        self.h_max = odd_at_least_1(lround(Self::MAX_ATTACK_SECONDS * fs) as i32);
        self.fast_window = odd_at_least_1(lround(Self::FAST_WINDOW_SECONDS * fs) as i32);
        self.ripple_hold = 2 * lround(Self::RIPPLE_HOLD_SECONDS * fs / 2.0) as i32;
        self.latency = self.h_max + self.fast_window - 2 + self.ripple_hold / 2;
        let (h_max, w, r, lat) = (self.h_max, self.fast_window, self.ripple_hold, self.latency);

        for k in self.kw.iter_mut() {
            k.prepare(fs);
        }
        for d in self.audio_delay.iter_mut() {
            d.prepare(lat);
        }
        self.fast_mean.prepare(w);
        self.fast_mean.set_length(w);
        let rh = 1.max(lround(Self::REF_HOLD_SECONDS * fs / 32.0) as i32);
        self.ref_hold.prepare(rh);
        self.ref_hold.set_length(rh);
        self.ref_delay.prepare(2 * w);
        self.active_delay.prepare(2 * w);
        let three = lround(3.0 * fs) as i32;
        self.slow_mean.prepare(three);
        self.slow_active.prepare(three);
        self.series_mean.prepare(three);
        self.led_mean.prepare(three);
        self.post_delay.prepare(lat);
        self.post_k_delay.prepare(lat);
        self.post_active.prepare(lat);
        self.auto_gain_window = 1.max(lround(Self::AUTO_GAIN_SECONDS * fs) as i32);
        self.auto_gain_relearn = 1.max(lround(Self::AUTO_GAIN_RAMP as f64 * Self::AUTO_GAIN_RELEARN_SECONDS * fs) as i32);
        self.auto_gain_min = 1.max(lround(Self::AUTO_GAIN_MIN_SECONDS * fs) as i32);
        self.crest_lane.prepare(h_max + w - 1 + r, (h_max + 1) / 2, lat);
        self.level_lane.prepare(h_max + w - 1 + r, (h_max + 1) / 2, lat);

        self.smooth_coeff = one_pole_coeff(0.02, fs);
        self.mode_coeff = one_pole_coeff(0.05, fs);
        self.prepared = true;
        let s = self.settings;
        self.set_settings(&s);
        self.reset();
    }

    pub fn reset(&mut self) {
        for k in self.kw.iter_mut() {
            k.reset();
        }
        for d in self.audio_delay.iter_mut() {
            d.reset();
        }
        self.fast_mean.reset();
        self.ref_delay.reset();
        self.active_delay.reset();
        self.ref_hold.reset();
        self.ref_held = -200.0;
        self.series_mean.reset();
        self.led_mean.reset();
        self.post_delay.reset();
        self.post_k_delay.reset();
        self.post_active.reset();
        self.ag_gain_db = 0.0;
        self.ag_in = 0.0;
        self.ag_count = 0;
        let wt = weights_for(self.settings.mode);
        self.w_par = wt.parallel as f64;
        self.w_ser = wt.series as f64;
        self.w_led = wt.fast_led as f64;
        self.slow_mean.reset();
        self.slow_active.reset();
        self.crest_lane.reset();
        self.level_lane.reset();

        self.h = self.h_target;
        self.apply_attack_length();
        self.gain_delay_samples = self.target_gain_delay();
        self.g_s = 0.0;
        self.g_series = 0.0;
        self.g_led = 0.0;
        self.long_term = -200.0;
        self.long_term_known = false;
        self.entry_count = 0;
        self.mix_smoothed = self.settings.mix as f64;
        self.out_smoothed = db_to_gain(self.settings.output_db) as f64;
        self.in_smoothed = db_to_gain(self.settings.input_db) as f64;
        self.ag_smoothed = self.auto_gain_target();
        self.delta_smoothed = if self.settings.delta { 1.0 } else { 0.0 };
        self.delta_trim_smoothed = self.delta_trim_target();
        self.meter_auto_gain_db = (20.0 * self.ag_smoothed.log10()) as f32;
        self.meter_slow_gr = 0.0;
        self.meter_fast_gr = 0.0;
        self.meter_total_gr = 0.0;
        self.meter_loudness = -100.0;
        self.meter_crest = -100.0;
        self.meter_reference = -100.0;
        self.meter_fast_level = -100.0;
        self.meter_hold = false;
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn set_settings(&mut self, s: &Settings) {
        if self.settings.learn_auto_gain && s.learn_auto_gain && !Settings::same_compression(&self.settings, s) {
            self.ag_count = self.ag_count.min(self.auto_gain_relearn);
        }
        self.settings = *s;
        if !self.prepared {
            return;
        }
        let fs = self.fs;
        let len = lround((s.slow_window_seconds as f64).clamp(0.05, 3.0) * fs) as i32;
        self.slow_mean.set_length(len);
        let l = self.slow_mean.length();
        self.slow_active.set_length(l);
        self.series_mean.set_length(l);
        self.led_mean.set_length(l);
        self.slow_attack_coeff = one_pole_coeff(s.slow_attack_ms as f64 * 0.001, fs);
        self.slow_release_coeff = one_pole_coeff(s.slow_release_ms as f64 * 0.001, fs);
        self.fast_release_coeff = one_pole_coeff(s.fast_release_ms as f64 * 0.001, fs);
        self.h_target = odd_at_least_1(lround(s.fast_attack_ms as f64 * 0.001 * fs) as i32).clamp(1, self.h_max.max(1));
        self.long_term_coeff = one_pole_coeff(Self::LONG_TERM_SECONDS, fs);
    }

    pub fn latency_samples(&self) -> i32 {
        self.latency
    }
    pub fn max_attack_samples(&self) -> i32 {
        self.h_max
    }
    pub fn fast_window_samples(&self) -> i32 {
        self.fast_window
    }
    pub fn current_attack_samples(&self) -> i32 {
        self.h
    }

    /// Input gain, in place, gliding over ~20 ms. Call before `process`.
    pub fn apply_input_gain(&mut self, left: &mut [f32], right: &mut [f32]) {
        let target = db_to_gain(self.settings.input_db) as f64;
        if (target - 1.0).abs() < 1.0e-12 && (self.in_smoothed - 1.0).abs() < 1.0e-12 {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            self.in_smoothed = target + self.smooth_coeff * (self.in_smoothed - target);
            if (self.in_smoothed - target).abs() < 1.0e-9 {
                self.in_smoothed = target;
            }
            left[i] = (left[i] as f64 * self.in_smoothed) as f32;
            right[i] = (right[i] as f64 * self.in_smoothed) as f32;
        }
    }

    /// In place, stereo-linked.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], mut trace: Option<&mut Trace>) {
        if !self.prepared {
            return;
        }
        let s = self.settings;
        let out_target = db_to_gain(s.output_db);
        let mut block_slow_gr = 0.0f32;
        let mut block_fast_gr = 0.0f32;
        let mut block_total_gr = 0.0f32;
        let mut block_crest = -100.0f32;
        let mut last_loudness = self.meter_loudness;
        let mut last_reference = self.meter_reference;
        let mut block_fast_level = -200.0f32;
        let mut hold = self.meter_hold;
        let latency = self.latency;
        let fast_window = self.fast_window;
        let num = left.len().min(right.len());

        for n in 0..num {
            self.glide_counter = self.glide_counter.wrapping_add(1);
            if (self.glide_counter & 7) == 0 {
                if (self.glide_counter & 15) == 0 && self.h != self.h_target {
                    self.h += if self.h < self.h_target { 2 } else { -2 };
                    self.apply_attack_length();
                }
                self.gain_delay_samples += (self.target_gain_delay() - self.gain_delay_samples).clamp(-1, 1);
            }

            let wt = weights_for(s.mode);
            self.w_par = wt.parallel as f64 + self.mode_coeff * (self.w_par - wt.parallel as f64);
            self.w_ser = wt.series as f64 + self.mode_coeff * (self.w_ser - wt.series as f64);
            self.w_led = wt.fast_led as f64 + self.mode_coeff * (self.w_led - wt.fast_led as f64);

            // ---- detector ----
            let mut ms = 0.0f64;
            let mut ms_k = 0.0f64;
            for c in 0..2 {
                let xf = if c == 0 { left[n] } else { right[n] };
                let x = xf as f64;
                let mut wk = self.kw[c].process(x);
                if !wk.is_finite() {
                    self.kw[c].reset();
                    wk = 0.0;
                }
                let w = if s.k_weighted { wk } else if x.is_finite() { x } else { 0.0 };
                ms += w * w;
                ms_k += wk * wk;
            }

            let fast_ms = maxd(0.0, self.fast_mean.push(ms));
            let l_fast = loudness_of(fast_ms);

            let active = l_fast
                > if self.long_term_known {
                    maxf(Self::ABSOLUTE_GATE, self.long_term as f32 - Self::ACTIVITY_GATE)
                } else {
                    Self::ABSOLUTE_GATE
                };

            let past_ms = self.ref_delay.push(ms, 2 * fast_window);
            let past_active = self.active_delay.push(if active { 1.0 } else { 0.0 }, 2 * fast_window);
            let slow_ms = maxd(0.0, self.slow_mean.push(past_ms));
            let any_active = self.slow_active.push(past_active) > 0.0;

            self.entry_count = if any_active { (self.entry_count + 1).min(self.slow_mean.length()) } else { 0 };
            let l_slow = if any_active {
                loudness_of(slow_ms * self.slow_mean.length() as f64 / self.entry_count as f64)
            } else {
                -200.0
            };
            let have_context = self.entry_count >= fast_window;
            let l_plain = loudness_of(slow_ms);

            if !self.long_term_known {
                self.long_term = if any_active { l_slow as f64 } else { -200.0 };
                self.long_term_known = self.entry_count == self.slow_mean.length();
            } else if l_plain > Self::ABSOLUTE_GATE {
                self.long_term = l_plain as f64 + self.long_term_coeff * (self.long_term - l_plain as f64);
            }

            // ---- fast engine ----
            if (self.glide_counter & 31) == 0 {
                self.ref_held = if have_context { -self.ref_hold.push(-l_slow) } else { -200.0 };
            }
            if !have_context {
                self.ref_hold.reset();
            }
            let mut reference = if have_context {
                maxf(self.ref_held, l_slow)
            } else if self.long_term_known {
                -200.0
            } else {
                l_fast
            };
            if self.long_term_known {
                reference = maxf(reference, self.long_term as f32);
            }
            let t_crest = if s.fast_on {
                gain_computer(l_fast - maxf(reference + s.fast_crest_db, s.threshold_db), 0.0, s.fast_ratio, Self::FAST_KNEE_DB)
            } else {
                0.0
            };
            let t_level = if s.fast_on {
                gain_computer(l_fast - (s.threshold_db + s.fast_crest_db), 0.0, s.fast_ratio, Self::FAST_KNEE_DB)
            } else {
                0.0
            };
            let g_crest = self.crest_lane.push(t_crest, self.fast_release_coeff, self.gain_delay_samples);
            let g_level = self.level_lane.push(t_level, self.fast_release_coeff, self.gain_delay_samples);

            // ---- slow engine ----
            let now_ms = self.post_delay.push(ms, latency);
            let l_series = Self::after_fast(&mut self.series_mean, self.entry_count, now_ms * 10.0f64.powf(0.1 * g_crest as f64), any_active);
            let l_led = Self::after_fast(&mut self.led_mean, self.entry_count, now_ms * 10.0f64.powf(0.1 * g_level as f64), any_active);
            let lv = LevelCtx {
                have_context,
                long_term_known: self.long_term_known,
                long_term: self.long_term,
                active,
                slow_on: s.slow_on,
                threshold: s.threshold_db,
                ratio: s.slow_ratio,
                attack: self.slow_attack_coeff,
                release: self.slow_release_coeff,
                smooth: self.smooth_coeff,
            };
            lv.level(&mut self.g_s, l_slow);
            lv.level(&mut self.g_series, l_series);
            lv.level(&mut self.g_led, l_led);

            let (w_par, w_ser, w_led) = (self.w_par, self.w_ser, self.w_led);
            let slow_gr = (w_par * self.g_s + w_ser * self.g_series + w_led * self.g_led) as f32;
            let g_f = ((w_par + w_ser) * g_crest as f64 + w_led * g_level as f64) as f32;
            let t_f = ((w_par + w_ser) * t_crest as f64 + w_led * t_level as f64) as f32;
            let l_engine = (w_par * l_slow as f64 + w_ser * l_series as f64 + w_led * l_led as f64) as f32;

            // ---- audio path: delay x gain ----
            self.mix_smoothed = s.mix as f64 + self.smooth_coeff * (self.mix_smoothed - s.mix as f64);
            self.out_smoothed = out_target as f64 + self.smooth_coeff * (self.out_smoothed - out_target as f64);
            let g = db_to_gain(slow_gr + g_f);
            let kc = self.mix_smoothed * g as f64 + (1.0 - self.mix_smoothed);

            // ---- autogain ----
            let now_k = self.post_k_delay.push(ms_k, latency);
            let now_active = self.post_active.push(if active { 1.0 } else { 0.0 }, latency) > 0.5;
            if s.learn_auto_gain && now_active {
                self.ag_count = (self.ag_count + 1).min(Self::AUTO_GAIN_RAMP * self.auto_gain_window);
                let a = mind(1.0, Self::AUTO_GAIN_RAMP as f64 / self.ag_count as f64);
                self.ag_in += a * (now_k - self.ag_in);
                let v = if kc == 1.0 { 0.0 } else { now_k * 20.0 * maxd(kc, 1.0e-6).log10() };
                self.ag_gain_db += a * (v - self.ag_gain_db);
            }
            let ag_target = self.auto_gain_target();
            self.ag_smoothed = ag_target + self.smooth_coeff * (self.ag_smoothed - ag_target);
            if (self.ag_smoothed - ag_target).abs() < 1.0e-9 {
                self.ag_smoothed = ag_target;
            }

            // ---- delta ----
            let d_target = if s.delta { 1.0 } else { 0.0 };
            self.delta_smoothed = d_target + self.smooth_coeff * (self.delta_smoothed - d_target);
            if (self.delta_smoothed - d_target).abs() < 1.0e-9 {
                self.delta_smoothed = d_target;
            }
            let trim_target = self.delta_trim_target();
            self.delta_trim_smoothed = trim_target + self.smooth_coeff * (self.delta_trim_smoothed - trim_target);
            if (self.delta_trim_smoothed - trim_target).abs() < 1.0e-9 {
                self.delta_trim_smoothed = trim_target;
            }
            let mut k_out = kc * self.out_smoothed * self.ag_smoothed;
            if self.delta_smoothed > 0.0 {
                k_out += self.delta_smoothed * ((1.0 - kc) * self.delta_trim_smoothed - k_out);
            }
            let k = k_out as f32;

            left[n] = k * self.audio_delay[0].push(left[n], latency);
            right[n] = k * self.audio_delay[1].push(right[n], latency);

            block_slow_gr = minf(block_slow_gr, slow_gr);
            block_fast_gr = minf(block_fast_gr, g_f);
            block_total_gr = minf(block_total_gr, slow_gr + g_f);
            block_crest = maxf(block_crest, l_fast - l_slow);
            last_loudness = l_slow;
            last_reference = reference;
            block_fast_level = maxf(block_fast_level, l_fast);
            hold = !active;

            if let Some(t) = trace.as_deref_mut() {
                put(&mut t.slow_gain_db, n, slow_gr);
                put(&mut t.fast_gain_db, n, g_f);
                put(&mut t.slow_loudness, n, l_slow);
                put(&mut t.fast_loudness, n, l_fast);
                put(&mut t.reference, n, reference);
                put(&mut t.fast_target_db, n, t_f);
                put(&mut t.slow_engine_loudness, n, l_engine);
            }
        }

        self.meter_slow_gr = block_slow_gr;
        self.meter_fast_gr = block_fast_gr;
        self.meter_total_gr = block_total_gr;
        self.meter_loudness = last_loudness;
        self.meter_crest = block_crest;
        self.meter_reference = last_reference;
        if num > 0 {
            self.meter_fast_level = block_fast_level;
        }
        self.meter_hold = hold;
        self.meter_auto_gain_db = (20.0 * self.ag_smoothed.log10()) as f32;
    }

    /// What autogain would apply now, in dB (also while it is off).
    pub fn auto_gain_measured_db(&self) -> f32 {
        (20.0 * maxd(1.0e-12, self.measured_auto_gain()).log10()) as f32
    }

    fn measured_auto_gain(&self) -> f64 {
        let mut compression_db = 0.0;
        if self.ag_count >= self.auto_gain_min && self.ag_in > 1.0e-20 {
            compression_db = self.ag_gain_db / self.ag_in;
        }
        let mut gain_db = -compression_db - self.settings.input_db as f64;
        let lim = Self::AUTO_GAIN_MAX_DB as f64;
        gain_db = gain_db.clamp(-lim, lim);
        if gain_db == 0.0 {
            1.0
        } else {
            10.0f64.powf(gain_db / 20.0)
        }
    }

    fn auto_gain_target(&self) -> f64 {
        if self.settings.auto_gain { self.measured_auto_gain() } else { 1.0 }
    }

    fn delta_trim_target(&self) -> f64 {
        if self.settings.auto_gain { 1.0 / db_to_gain(self.settings.input_db) as f64 } else { 1.0 }
    }

    fn after_fast(mean: &mut MovingMean, entry_count: i32, ms: f64, any_active: bool) -> f32 {
        let m = maxd(0.0, mean.push(ms));
        if any_active && entry_count > 0 {
            loudness_of(m * mean.length() as f64 / entry_count as f64)
        } else {
            -200.0
        }
    }

    fn apply_attack_length(&mut self) {
        let hold = self.h + self.fast_window - 1 + self.ripple_hold;
        let bx = (self.h + 1) / 2;
        self.crest_lane.set_lengths(hold, bx);
        self.level_lane.set_lengths(hold, bx);
    }

    fn target_gain_delay(&self) -> i32 {
        let late = 1.0f32 - self.settings.lookahead.clamp(0.0, 1.0);
        let base = self.latency - self.h - self.fast_window + 2 - self.ripple_hold / 2;
        let span = (self.h + self.fast_window - 2 + self.ripple_hold / 2) as f32;
        base + (late * span).round() as i32
    }
}

// The leveller step shared by the three slow lanes (the original's `level` lambda).
struct LevelCtx {
    have_context: bool,
    long_term_known: bool,
    long_term: f64,
    active: bool,
    slow_on: bool,
    threshold: f32,
    ratio: f32,
    attack: f64,
    release: f64,
    smooth: f64,
}

impl LevelCtx {
    #[inline]
    fn level(&self, gain: &mut f64, loudness: f32) {
        let gated = !self.have_context
            || (self.long_term_known && (loudness as f64) < self.long_term - Core::RELATIVE_GATE as f64);
        if !gated {
            let target = if self.slow_on {
                gain_computer(loudness, self.threshold, self.ratio, Core::SLOW_KNEE_DB) as f64
            } else {
                0.0
            };
            if target < *gain || self.active {
                let a = if target < *gain { self.attack } else { self.release };
                *gain = target + a * (*gain - target);
            }
        }
        if !self.slow_on {
            *gain *= self.smooth;
        }
    }
}
