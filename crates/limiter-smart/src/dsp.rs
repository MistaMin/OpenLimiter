//! Small building blocks. Every buffer is allocated in `alloc`; `push`, `run` and `clear`
//! never allocate, lock or panic (rings are powers of two indexed through a mask, and every
//! method checks for an unallocated buffer first).

use std::f64::consts::{LN_10, PI};

pub const LN10_20: f64 = LN_10 / 20.0;
/// What `lin_to_db` returns for silence.
pub const DB_FLOOR: f64 = -600.0;

#[inline]
pub fn db_to_lin(db: f64) -> f64 {
    (db * LN10_20).exp()
}

#[inline]
pub fn lin_to_db(x: f64) -> f64 {
    if x > 1e-30 {
        20.0 * x.log10()
    } else {
        DB_FLOOR
    }
}

/// One-pole coefficient `exp(-1 / (t fs))` for a time constant in ms; 0 (instant) for 0 ms.
pub fn pole(ms: f64, fs: f64) -> f64 {
    let n = ms * 1e-3 * fs;
    if n <= 1e-9 || !n.is_finite() {
        0.0
    } else {
        (-1.0 / n).exp()
    }
}

/// Samples for a time in ms, rounded, at least `min`.
pub fn ms_to_samples(ms: f64, fs: f64, min: usize) -> usize {
    ((ms * 1e-3 * fs).round().max(0.0) as usize).max(min)
}

pub fn pow2_at_least(n: usize) -> usize {
    n.max(2).next_power_of_two()
}

pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    let mut k = 1.0;
    while k < 80.0 {
        term *= q / (k * k);
        sum += term;
        if term < 1e-18 * sum {
            break;
        }
        k += 1.0;
    }
    sum
}

#[inline]
pub fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// Kaiser window at `t` in [-1, 1].
#[inline]
pub fn kaiser(t: f64, beta: f64) -> f64 {
    let r = 1.0 - t * t;
    if r <= 0.0 {
        0.0
    } else {
        bessel_i0(beta * r.sqrt()) / bessel_i0(beta)
    }
}

// ---------------------------------------------------------------------------------------------

/// Delay line on a power-of-two ring.
#[derive(Default)]
pub struct Delay {
    buf: Vec<f64>,
    mask: usize,
    pos: usize,
}

impl Delay {
    pub fn alloc(&mut self, max_delay: usize) {
        let n = pow2_at_least(max_delay + 1);
        self.buf = vec![0.0; n];
        self.mask = n - 1;
        self.pos = 0;
    }
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }
    /// Sets every stored value to `v`.
    pub fn fill(&mut self, v: f64) {
        self.buf.iter_mut().for_each(|x| *x = v);
    }
    /// The value written `d` pushes ago (`d = 0`: the newest), without pushing.
    #[inline]
    pub fn tap(&self, d: usize) -> f64 {
        self.buf.get(self.pos.wrapping_sub(d.min(self.mask)) & self.mask).copied().unwrap_or(0.0)
    }
    /// Writes `x` and returns the value written `d` pushes ago (`d = 0` returns `x`).
    #[inline]
    pub fn push(&mut self, x: f64, d: usize) -> f64 {
        if self.buf.is_empty() {
            return x;
        }
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
        self.buf[self.pos.wrapping_sub(d.min(self.mask)) & self.mask]
    }
}

/// Minimum of the last `window` pushed values (monotonic queue). The window may change from
/// one push to the next; growing it by one sample per push is exact, shrinking always is.
#[derive(Default)]
pub struct SlidingMin {
    val: Vec<f64>,
    stamp: Vec<u64>,
    mask: usize,
    head: usize,
    len: usize,
    now: u64,
}

impl SlidingMin {
    pub fn alloc(&mut self, max_window: usize) {
        let n = pow2_at_least(max_window + 2);
        self.val = vec![0.0; n];
        self.stamp = vec![0; n];
        self.mask = n - 1;
        self.clear();
    }
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
        self.now = 0;
    }
    #[inline]
    pub fn push(&mut self, v: f64, window: usize) -> f64 {
        if self.val.is_empty() {
            return v;
        }
        let window = window.clamp(1, self.mask) as u64;
        self.now += 1;
        while self.len > 0 {
            let b = (self.head + self.len - 1) & self.mask;
            if self.val[b] >= v {
                self.len -= 1;
            } else {
                break;
            }
        }
        if self.len > self.mask {
            self.head = (self.head + 1) & self.mask;
            self.len -= 1;
        }
        let b = (self.head + self.len) & self.mask;
        self.val[b] = v;
        self.stamp[b] = self.now;
        self.len += 1;
        while self.len > 1 && self.stamp[self.head] + window <= self.now {
            self.head = (self.head + 1) & self.mask;
            self.len -= 1;
        }
        self.val[self.head]
    }
}

/// Maximum of the last `window` values.
#[derive(Default)]
pub struct SlidingMax(SlidingMin);

impl SlidingMax {
    pub fn alloc(&mut self, max_window: usize) {
        self.0.alloc(max_window);
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    #[inline]
    pub fn push(&mut self, v: f64, window: usize) -> f64 {
        -self.0.push(-v, window)
    }
}

/// Running mean over the last `len` values. The running sum is rebuilt from scratch every
/// `len` pushes, so it cannot drift, and a run of zeros gives exactly zero. The length may
/// change between pushes: growing takes older values from the history.
#[derive(Default)]
pub struct MovingMean {
    buf: Vec<f64>,
    mask: usize,
    pos: usize,
    len: usize,
    sum: f64,
    fresh: f64,
    count: usize,
}

impl MovingMean {
    pub fn alloc(&mut self, max_len: usize) {
        let n = pow2_at_least(max_len + 1);
        self.buf = vec![0.0; n];
        self.mask = n - 1;
        self.len = self.len.clamp(1, self.mask);
        self.clear();
    }
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
        self.sum = 0.0;
        self.fresh = 0.0;
        self.count = 0;
    }
    pub fn len(&self) -> usize {
        self.len
    }
    #[inline]
    fn at(&self, k: usize) -> f64 {
        self.buf.get(self.pos.wrapping_sub(k) & self.mask).copied().unwrap_or(0.0)
    }
    pub fn set_len(&mut self, n: usize) {
        if self.buf.is_empty() {
            return;
        }
        let n = n.clamp(1, self.mask);
        if n == self.len {
            return;
        }
        while self.len < n {
            self.len += 1;
            self.sum += self.at(self.len);
        }
        while self.len > n {
            self.sum -= self.at(self.len);
            self.len -= 1;
        }
        self.fresh = 0.0;
        self.count = 0;
    }
    #[inline]
    pub fn push(&mut self, x: f64) -> f64 {
        if self.buf.is_empty() {
            return x;
        }
        self.buf[self.pos] = x;
        let old = self.buf[self.pos.wrapping_sub(self.len) & self.mask];
        self.sum += x - old;
        self.pos = (self.pos + 1) & self.mask;
        self.fresh += x;
        self.count += 1;
        if self.count >= self.len {
            self.sum = self.fresh;
            self.fresh = 0.0;
            self.count = 0;
        }
        self.sum / self.len as f64
    }
}

// ---------------------------------------------------------------------------------------------

/// Transposed direct form II biquad.
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BqKind {
    Low,
    High,
    All,
}

impl Biquad {
    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    pub fn clear(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
    /// Sets the coefficients (bilinear, prewarped at `f`), keeping the state.
    pub fn design(&mut self, kind: BqKind, f: f64, q: f64, fs: f64) {
        let f = f.clamp(1.0, 0.49 * fs);
        let w = 2.0 * PI * f / fs;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match kind {
            BqKind::Low => ((1.0 - cs) / 2.0, 1.0 - cs, (1.0 - cs) / 2.0),
            BqKind::High => ((1.0 + cs) / 2.0, -(1.0 + cs), (1.0 + cs) / 2.0),
            BqKind::All => (1.0 - alpha, -2.0 * cs, 1.0 + alpha),
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
    pub fn flush(&mut self) {
        if self.z1.abs() < 1e-30 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-30 {
            self.z2 = 0.0;
        }
    }
}

/// BS.1770 K-weighting (head shelf + RLB high-pass) from the analogue prototype, exact
/// BS.1770 Table 1 and 2 values at 48 kHz.
#[derive(Clone, Copy, Default)]
pub struct KWeighting {
    shelf: Biquad,
    hp: Biquad,
}

impl KWeighting {
    pub fn prepare(&mut self, fs: f64) {
        let (f0, g, q) = (1681.974450955533_f64, 3.999843853973347_f64, 0.7071752369554196_f64);
        let k = (PI * f0 / fs).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        self.shelf.b0 = (vh + vb * k / q + k * k) / a0;
        self.shelf.b1 = 2.0 * (k * k - vh) / a0;
        self.shelf.b2 = (vh - vb * k / q + k * k) / a0;
        self.shelf.a1 = 2.0 * (k * k - 1.0) / a0;
        self.shelf.a2 = (1.0 - k / q + k * k) / a0;
        let (f0, q) = (38.13547087602444_f64, 0.5003270373238773_f64);
        let k = (PI * f0 / fs).tan();
        let a0 = 1.0 + k / q + k * k;
        self.hp.b0 = 1.0;
        self.hp.b1 = -2.0;
        self.hp.b2 = 1.0;
        self.hp.a1 = 2.0 * (k * k - 1.0) / a0;
        self.hp.a2 = (1.0 - k / q + k * k) / a0;
        self.clear();
    }
    pub fn clear(&mut self) {
        self.shelf.clear();
        self.hp.clear();
    }
    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.hp.run(self.shelf.run(x));
        if y.is_finite() {
            y
        } else {
            self.clear();
            0.0
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// Taps of the true-peak reader. The spec asks for 16 taps (Kaiser beta 8); that kernel
/// reads 1.2 dB low at 20 kHz and 7.6 dB low at 22.5 kHz (48 kHz), so the reader here uses
/// 48 taps, Kaiser beta 7: within 0.01 dB to 21 kHz, -0.55 dB at 22.5 kHz.
pub const TP_TAPS: usize = 48;
/// Kaiser beta of the reader.
pub const TP_BETA: f64 = 7.0;
/// How far the reader runs behind its input: the interval it reads lies between the samples
/// `TP_HALF` and `TP_HALF - 1` pushes ago.
pub const TP_HALF: usize = TP_TAPS / 2;
const TP_RING: usize = 64;

/// The reader's kernels in `f32`, the form [`interval_peak`] runs with.
pub type TpKernels = [[f32; TP_TAPS]; 3];

/// The largest absolute tap sum of the three interpolators: no interpolated point can exceed
/// this times the largest sample magnitude in the window (`f32` rounding of the taps and of
/// the sums is covered by a relative 1e-6).
pub fn tp_gain_bound(k: &TpKernels) -> f64 {
    k.iter().map(|row| row.iter().map(|h| (*h as f64).abs()).sum::<f64>()).fold(1.0, f64::max) * (1.0 + 1e-6)
}

/// The three quarter-sample interpolators (Kaiser-windowed sinc, unit DC gain), designed in
/// `f64`. Row `r` gives the point `(r + 1) / 4` of the way from tap `TP_HALF - 1` to tap
/// `TP_HALF`.
pub fn tp_kernels_f64() -> [[f64; TP_TAPS]; 3] {
    let mut out = [[0.0; TP_TAPS]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        let frac = (r + 1) as f64 / 4.0;
        let mut sum = 0.0;
        for (j, h) in row.iter_mut().enumerate() {
            let t = j as f64 - (TP_HALF as f64 - 1.0) - frac;
            *h = sinc(t) * kaiser(t / TP_HALF as f64, TP_BETA);
            sum += *h;
        }
        for h in row.iter_mut() {
            *h /= sum;
        }
    }
    out
}

/// [`tp_kernels_f64`] rounded to `f32`.
pub fn tp_kernels() -> TpKernels {
    let k = tp_kernels_f64();
    let mut out = [[0.0f32; TP_TAPS]; 3];
    for (o, r) in out.iter_mut().zip(k.iter()) {
        for (a, b) in o.iter_mut().zip(r.iter()) {
            *a = *b as f32;
        }
    }
    out
}

/// History for the true-peak reader: the last `TP_TAPS` samples of one channel, contiguous,
/// in `f32` for the filter, plus the two interval end points in `f64` (exact sample peaks).
#[derive(Clone, Copy)]
pub struct TpHist {
    buf: [f32; 2 * TP_RING],
    pos: usize,
    /// The samples `TP_HALF` and `TP_HALF - 1` pushes ago, exact.
    ends: [f64; TP_HALF + 1],
    epos: usize,
}

impl Default for TpHist {
    fn default() -> Self {
        Self { buf: [0.0; 2 * TP_RING], pos: 0, ends: [0.0; TP_HALF + 1], epos: 0 }
    }
}

impl TpHist {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    #[inline]
    pub fn push(&mut self, x: f64) {
        self.pos = (self.pos + 1) & (TP_RING - 1);
        let v = x as f32;
        self.buf[self.pos] = v;
        self.buf[self.pos + TP_RING] = v;
        self.epos = if self.epos >= TP_HALF { 0 } else { self.epos + 1 };
        self.ends[self.epos] = x;
    }
    /// The `TP_TAPS` newest samples, oldest first (`f32`).
    #[inline]
    pub fn window(&self) -> &[f32] {
        let e = self.pos + TP_RING + 1;
        &self.buf[e - TP_TAPS..e]
    }
    /// The interval end points, exact: the samples `TP_HALF` and `TP_HALF - 1` pushes ago
    /// (`window()[TP_HALF - 1]` and `window()[TP_HALF]` in full precision).
    #[inline]
    pub fn ends(&self) -> (f64, f64) {
        // ends holds the last TP_HALF + 1 samples; the oldest of them is TP_HALF pushes ago
        let old = if self.epos >= TP_HALF { 0 } else { self.epos + 1 };
        let next = if old >= TP_HALF { 0 } else { old + 1 };
        (self.ends[old], self.ends[next])
    }
    /// The larger magnitude of the two interval end points (the sample peak).
    #[inline]
    pub fn end_peak(&self) -> f64 {
        let (a, b) = self.ends();
        a.abs().max(b.abs())
    }
}

/// Largest magnitude on the interval between taps `TP_HALF - 1` and `TP_HALF` of `w` (the two
/// samples `ends` and three interpolated points). With `refine`, a three-point parabola
/// through the largest of the five points and its neighbours refines an interior maximum
/// (bounded to +0.5 dB). The filter runs in `f32` with four independent accumulators per
/// phase; the parabola in `f64`.
#[inline]
pub fn interval_peak(w: &[f32], ends: (f64, f64), k: &TpKernels, refine: bool) -> f64 {
    if w.len() < TP_TAPS {
        return 0.0;
    }
    let mut v = [ends.0, 0.0, 0.0, 0.0, ends.1];
    // three phases in one pass, a 4-wide accumulator each (vectorises), over the older
    // TP_TAPS - 4 taps; the 4 newest (one of them stored just now) as scalars, so no vector
    // load overlaps the scalar store of this push (store-to-load forwarding)
    const V: usize = TP_TAPS - 4;
    let w = &w[..TP_TAPS];
    let mut acc = [[0.0f32; 4]; 3];
    for c in 0..V / 4 {
        for i in 0..4 {
            let x = w[4 * c + i];
            acc[0][i] += k[0][4 * c + i] * x;
            acc[1][i] += k[1][4 * c + i] * x;
            acc[2][i] += k[2][4 * c + i] * x;
        }
    }
    for r in 0..3 {
        let kr = &k[r];
        let tail = (kr[V] * w[V] + kr[V + 1] * w[V + 1]) + (kr[V + 2] * w[V + 2] + kr[V + 3] * w[V + 3]);
        v[r + 1] = (((acc[r][0] + acc[r][1]) + (acc[r][2] + acc[r][3])) + tail) as f64;
    }
    let mut best = 0;
    let mut peak = 0.0f64;
    for (i, x) in v.iter().enumerate() {
        if x.abs() > peak {
            peak = x.abs();
            best = i;
        }
    }
    if refine && (1..=3).contains(&best) {
        let (ym, y0, yp) = (v[best - 1], v[best], v[best + 1]);
        let den = yp - 2.0 * y0 + ym;
        // A maximum of |y|: the curvature points back towards zero.
        if den * y0 < 0.0 {
            let e = y0 - (yp - ym) * (yp - ym) / (8.0 * den);
            let a = e.abs();
            if a.is_finite() && a > peak {
                peak = a.min(peak * 1.059_253_7);
            }
        }
    }
    peak
}
