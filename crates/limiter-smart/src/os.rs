//! Decoupled two-rate oversampling (section 2 of the spec).
//!
//! A linear-phase cascade of Type-I halfband stages:
//!
//! | stage      | taps | side pairs | delay (base samples, up + down) |
//! |------------|------|------------|---------------------------------|
//! | 1x <-> 2x  | 79   | 20         | 39                              |
//! | 2x <-> 4x  | 15   | 4          | 3.5                             |
//! | 4x <-> 8x  | 7    | 2          | 0.75                            |
//! | 8x <-> 16x | 7    | 2          | 0.375                           |
//!
//! A pad at the high rate (10 samples at 2x, 6 at 4x, 8x and 16x, 44 at 1x) brings every factor
//! to exactly [`OS_DELAY`] base samples. The 79-tap stage (Kaiser beta 8) is flat past 21 kHz
//! at 48 kHz and stops the clipper's 5th harmonic of 5 kHz before it folds to 23 kHz; the
//! 15-tap stage is Kaiser beta 5.5; the 7-tap stages are the maximally flat halfband
//! `[-1, 0, 9, 16, 9, 0, -1] / 32`.
//!
//! The filters run in `f32` (kernels and histories); designs and responses stay `f64`. The
//! 79-tap stage keeps a mirrored ring and runs its side taps as 4-wide products with
//! independent 4-wide accumulators; the short stages run a whole group at a time from one
//! contiguous local buffer (history, then the new samples).
//!
//! [`OsPath`] holds both gain channels: it upsamples one base sample to a group of `U`
//! sub-samples, writes them to a ring, collapses the group to its peak for the 1x envelope,
//! and later reads the group back for gain, shaping and decimation.

use crate::dsp::{kaiser, pow2_at_least, sinc};

/// Base-rate delay of up + pad + down, every factor.
pub const OS_DELAY: usize = 44;
/// Largest factor.
pub const MAX_U: usize = 16;
/// Highest internal rate.
pub const MAX_RATE: f64 = 768_000.0;

/// Most side pairs of any stage.
const MAX_PAIRS: usize = 20;
/// Ring length of the filter histories (the window is at most `2 * MAX_PAIRS`).
const RING: usize = 64;

#[derive(Clone, Copy)]
pub struct Halfband {
    /// Side taps at offsets 1, 3, 5, ... from the centre (centre tap is 0.5).
    h: [f64; MAX_PAIRS],
    n: usize,
    /// The side taps as one symmetric `f32` kernel over a window of `2 n` samples (oldest
    /// first): `k[n + i] = k[n - 1 - i] = h[i]`.
    k: [f32; 2 * MAX_PAIRS],
}

impl Halfband {
    fn from_side(h: [f64; MAX_PAIRS], n: usize) -> Self {
        let mut k = [0.0f32; 2 * MAX_PAIRS];
        for i in 0..n {
            k[n + i] = h[i] as f32;
            k[n - 1 - i] = h[i] as f32;
        }
        Self { h, n, k }
    }
    fn kaiser(pairs: usize, beta: f64) -> Self {
        let mut h = [0.0; MAX_PAIRS];
        let half = (2 * pairs) as f64; // window reaches zero one tap past the end
        let mut sum = 0.0;
        for (i, v) in h.iter_mut().take(pairs).enumerate() {
            let k = (2 * i + 1) as f64;
            *v = 0.5 * sinc(k / 2.0) * kaiser(k / half, beta);
            sum += *v;
        }
        // DC gain 1: centre 0.5 plus both sides.
        for v in h.iter_mut().take(pairs) {
            *v *= 0.25 / sum;
        }
        Self::from_side(h, pairs)
    }
    fn maxflat7() -> Self {
        let mut h = [0.0; MAX_PAIRS];
        h[0] = 9.0 / 32.0;
        h[1] = -1.0 / 32.0;
        Self::from_side(h, 2)
    }
    /// Centre tap index (delay at the high rate).
    pub fn centre(&self) -> usize {
        2 * self.n - 1
    }
    /// Magnitude response at `f` cycles per high-rate sample.
    pub fn response(&self, f: f64) -> f64 {
        let mut a = 0.5;
        for i in 0..self.n {
            a += 2.0 * self.h[i] * (2.0 * std::f64::consts::PI * f * (2 * i + 1) as f64).cos();
        }
        a
    }
}

/// The side-tap sum of the 20-pair stage over its window `w` of 40 samples (oldest first;
/// the newest was stored just now): the 36 oldest taps as 4-wide `f32` products with three
/// independent 4-wide accumulators, the 4 newest as scalars, so no vector load overlaps the
/// scalar store of the same push (that would stall store-to-load forwarding).
#[inline(always)]
fn side40(k: &[f32; 2 * MAX_PAIRS], w: &[f32]) -> f32 {
    let w = &w[..40];
    let mut acc = [[0.0f32; 4]; 3];
    for c in 0..3 {
        for q in 0..3 {
            let o = 4 * (3 * c + q);
            for i in 0..4 {
                acc[q][i] += k[o + i] * w[o + i];
            }
        }
    }
    let tail = (k[36] * w[36] + k[37] * w[37]) + (k[38] * w[38] + k[39] * w[39]);
    let mut v = [0.0f32; 4];
    for i in 0..4 {
        v[i] = acc[0][i] + acc[1][i] + acc[2][i];
    }
    ((v[0] + v[1]) + (v[2] + v[3])) + tail
}

/// The side-tap sum of a short stage over its window `w` of `L = 2 n` samples (oldest
/// first), as symmetric pairs: `sum_i h[i] (w[n + i] + w[n - 1 - i])`.
#[inline(always)]
fn side_short<const L: usize>(k: &[f32; 2 * MAX_PAIRS], w: &[f32; L]) -> f32 {
    let n = L / 2;
    let mut acc = [0.0f32; 4];
    for i in 0..n {
        acc[i & 3] += k[n + i] * (w[n + i] + w[n - 1 - i]);
    }
    (acc[0] + acc[1]) + (acc[2] + acc[3])
}

/// The four stages, in order from the base rate upward.
pub fn stages() -> [Halfband; 4] {
    [Halfband::kaiser(20, 8.0), Halfband::kaiser(4, 5.5), Halfband::maxflat7(), Halfband::maxflat7()]
}

/// Delay of the filters alone for `2^k`, in base samples.
pub fn filter_delay(k: usize) -> f64 {
    let s = stages();
    (0..k).map(|i| s[i].centre() as f64 / (1u32 << i) as f64).sum()
}

/// High-rate samples of pad that bring the factor `2^k` to [`OS_DELAY`].
pub fn pad(k: usize) -> usize {
    (((OS_DELAY as f64 - filter_delay(k)) * (1u32 << k) as f64).round().max(0.0)) as usize
}

/// Largest factor not above `requested` with `factor * fs <= 768 kHz` (at least 1).
pub fn effective_factor(requested: usize, fs: f64) -> usize {
    let mut u = requested.clamp(1, MAX_U).next_power_of_two().min(MAX_U);
    while u > 1 && u as f64 * fs > MAX_RATE + 1.0 {
        u /= 2;
    }
    u
}

/// Largest magnitude of one pass (up or down) of the cascade for `2^k` from DC to `f_hi` Hz.
pub fn passband_peak(k: usize, fs: f64, f_hi: f64) -> f64 {
    let s = stages();
    let mut peak: f64 = 1.0;
    let steps = 2000;
    for i in 0..=steps {
        let f = f_hi * i as f64 / steps as f64;
        let mut g = 1.0;
        for (j, st) in s.iter().enumerate().take(k) {
            g *= st.response(f / (fs * (2u32 << j) as f64)).abs();
        }
        peak = peak.max(g);
    }
    peak
}

/// A history of `RING` samples, mirrored so that the newest 40 are contiguous (stage 1).
#[derive(Clone, Copy)]
struct Hist {
    x: [f32; 2 * RING],
    pos: usize,
}

impl Default for Hist {
    fn default() -> Self {
        Self { x: [0.0; 2 * RING], pos: 0 }
    }
}

impl Hist {
    #[inline(always)]
    fn push(&mut self, v: f32) {
        self.pos = (self.pos + 1) & (RING - 1);
        self.x[self.pos] = v;
        self.x[self.pos + RING] = v;
    }
    /// The newest `len` samples, oldest first.
    #[inline(always)]
    fn window(&self, len: usize) -> &[f32] {
        let e = self.pos + RING + 1;
        &self.x[e - len..e]
    }
}

/// The upsampler: stage 1 (20 pairs) on a mirrored ring, stages 2 to 4 (4, 2 and 2 pairs) on
/// shift registers that stay in locals while a group runs.
#[derive(Clone, Copy)]
pub struct Up {
    s1: Hist,
    s2: [f32; 8],
    s3: [f32; 4],
    s4: [f32; 4],
}

impl Default for Up {
    fn default() -> Self {
        Self { s1: Hist::default(), s2: [0.0; 8], s3: [0.0; 4], s4: [0.0; 4] }
    }
}

/// Scratch for one short stage: its history (`L` samples) followed by up to 8 new ones.
const SCRATCH: usize = 16;

/// One short upsampling stage over `n_in` inputs, from one contiguous local buffer (history
/// then the new samples): `out[2 i]` is the FIR phase, `out[2 i + 1]` the delayed copy.
#[inline(always)]
fn up_short<const L: usize>(st: &mut [f32; L], k: &[f32; 2 * MAX_PAIRS], a: &[f32; MAX_U], n_in: usize, b: &mut [f32; MAX_U]) {
    let n_in = n_in.min(SCRATCH - L).min(MAX_U / 2);
    let mut buf = [0.0f32; SCRATCH];
    buf[..L].copy_from_slice(st);
    buf[L..L + n_in].copy_from_slice(&a[..n_in]);
    for i in 0..n_in {
        if let Ok(w) = <&[f32; L]>::try_from(&buf[i + 1..i + 1 + L]) {
            b[2 * i] = 2.0 * side_short(k, w);
            b[2 * i + 1] = w[L / 2];
        }
    }
    st.copy_from_slice(&buf[n_in..n_in + L]);
}

impl Up {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// One base sample -> `2^k` samples in `out`, oldest first.
    #[inline]
    pub fn run(&mut self, x: f64, k: usize, hb: &[Halfband; 4], out: &mut [f64; MAX_U]) {
        let mut a = [0.0f32; MAX_U];
        a[0] = x as f32;
        if k >= 1 {
            self.s1.push(a[0]);
            let w = self.s1.window(40);
            a[0] = 2.0 * side40(&hb[0].k, w);
            a[1] = w[20];
        }
        let mut b = [0.0f32; MAX_U];
        if k >= 2 {
            up_short(&mut self.s2, &hb[1].k, &a, 2, &mut b);
            a = b;
        }
        if k >= 3 {
            up_short(&mut self.s3, &hb[2].k, &a, 4, &mut b);
            a = b;
        }
        if k >= 4 {
            up_short(&mut self.s4, &hb[3].k, &a, 8, &mut b);
            a = b;
        }
        for (o, v) in out.iter_mut().zip(a.iter()) {
            *o = *v as f64;
        }
    }
}

/// The decimator, laid out like [`Up`]; `od` keeps the odd (centre-tap) samples.
#[derive(Clone, Copy)]
pub struct Down {
    e1: Hist,
    o1: [f32; RING],
    e2: [f32; 8],
    o2: [f32; 5],
    e3: [f32; 4],
    o3: [f32; 3],
    e4: [f32; 4],
    o4: [f32; 3],
}

impl Default for Down {
    fn default() -> Self {
        Self { e1: Hist::default(), o1: [0.0; RING], e2: [0.0; 8], o2: [0.0; 5], e3: [0.0; 4], o3: [0.0; 3], e4: [0.0; 4], o4: [0.0; 3] }
    }
}

/// One short decimating stage: `n_out` outputs from `2 n_out` inputs, in place, from local
/// contiguous buffers (history then the new even / odd samples).
/// `z[m] = 0.5 od[m - n] + sum_i h[i] (ev[m - n + 1 + i] + ev[m - n - i])`; `O = n + 1` odd
/// samples of history.
#[inline(always)]
fn down_short<const L: usize, const O: usize>(ev: &mut [f32; L], od: &mut [f32; O], k: &[f32; 2 * MAX_PAIRS], a: &mut [f32; MAX_U], n_out: usize) {
    let n_out = n_out.min(SCRATCH - L).min(MAX_U / 2);
    let mut be = [0.0f32; SCRATCH];
    let mut bo = [0.0f32; SCRATCH];
    be[..L].copy_from_slice(ev);
    bo[..O].copy_from_slice(od);
    for i in 0..n_out {
        be[L + i] = a[2 * i];
        bo[O + i] = a[2 * i + 1];
    }
    for i in 0..n_out {
        if let Ok(w) = <&[f32; L]>::try_from(&be[i + 1..i + 1 + L]) {
            // newest odd sample at O + i; the centre tap is n = L / 2 pushes older
            a[i] = 0.5 * bo[O + i - L / 2] + side_short(k, w);
        }
    }
    ev.copy_from_slice(&be[n_out..n_out + L]);
    od.copy_from_slice(&bo[n_out..n_out + O]);
}

impl Down {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    /// `2^k` samples -> one base sample. `buf` is the input (left unchanged).
    #[inline]
    pub fn run(&mut self, buf: &mut [f64; MAX_U], k: usize, hb: &[Halfband; 4]) -> f64 {
        let mut a = [0.0f32; MAX_U];
        let n = 1usize << k.min(4);
        for (d, s) in a.iter_mut().zip(buf.iter()).take(n) {
            *d = *s as f32;
        }
        if k >= 4 {
            down_short(&mut self.e4, &mut self.o4, &hb[3].k, &mut a, 8);
        }
        if k >= 3 {
            down_short(&mut self.e3, &mut self.o3, &hb[2].k, &mut a, 4);
        }
        if k >= 2 {
            down_short(&mut self.e2, &mut self.o2, &hb[1].k, &mut a, 2);
        }
        if k >= 1 {
            let (y0, y1) = (a[0], a[1]);
            self.e1.push(y0);
            self.o1[self.e1.pos] = y1;
            let w = self.e1.window(40);
            a[0] = 0.5 * self.o1[self.e1.pos.wrapping_sub(20) & (RING - 1)] + side40(&hb[0].k, w);
        }
        a[0] as f64
    }
}

/// Both gain channels' oversampled path: upsampler, group ring, decimators (output and
/// reference).
pub struct OsPath {
    pub k: usize,
    pub u: usize,
    pad: usize,
    hb: [Halfband; 4],
    up: [Up; 2],
    down: [Down; 2],
    down_ref: [Down; 2],
    ring: [Vec<f64>; 2],
    mask: usize,
    /// Index of the newest group.
    group: usize,
}

impl Default for OsPath {
    fn default() -> Self {
        Self {
            k: 0,
            u: 1,
            pad: pad(0),
            hb: stages(),
            up: [Up::default(); 2],
            down: [Down::default(); 2],
            down_ref: [Down::default(); 2],
            ring: [Vec::new(), Vec::new()],
            mask: 0,
            group: 0,
        }
    }
}

impl OsPath {
    /// Room for groups up to `max_delay` groups old at the largest factor.
    pub fn alloc(&mut self, max_delay: usize) {
        let n = pow2_at_least((max_delay + 3) * MAX_U + OS_DELAY * MAX_U);
        self.ring = [vec![0.0; n], vec![0.0; n]];
        self.mask = n - 1;
        self.clear();
    }

    pub fn set_factor(&mut self, u: usize) {
        let u = u.clamp(1, MAX_U).next_power_of_two().min(MAX_U);
        self.u = u;
        self.k = u.trailing_zeros() as usize;
        self.pad = pad(self.k);
    }

    pub fn clear(&mut self) {
        for r in self.ring.iter_mut() {
            r.iter_mut().for_each(|v| *v = 0.0);
        }
        for c in 0..2 {
            self.up[c].clear();
            self.down[c].clear();
            self.down_ref[c].clear();
        }
        self.group = 0;
    }

    /// Upsamples one base sample per channel, stores the group and returns each channel's
    /// group peak `max_r |u[U n + r]|` (of the padded stream).
    #[inline]
    pub fn push(&mut self, x0: f64, x1: f64) -> (f64, f64) {
        if self.ring[0].is_empty() || self.ring[1].is_empty() {
            return (x0.abs(), x1.abs());
        }
        self.group = self.group.wrapping_add(1);
        let u = self.u;
        let start = self.group.wrapping_mul(u);
        let mut tmp = [0.0; MAX_U];
        let mut peaks = [0.0f64; 2];
        for (c, x) in [x0, x1].into_iter().enumerate() {
            self.up[c].run(x, self.k, &self.hb, &mut tmp);
            let ring = &mut self.ring[c];
            for (r, v) in tmp.iter().take(u).enumerate() {
                ring[start.wrapping_add(r) & self.mask] = *v;
            }
            let g0 = start.wrapping_sub(self.pad);
            let mut p = 0.0f64;
            for r in 0..u {
                p = p.max(ring[g0.wrapping_add(r) & self.mask].abs());
            }
            peaks[c] = p;
        }
        (peaks[0], peaks[1])
    }

    /// The group `delay` groups older than the newest, both channels.
    #[inline]
    pub fn fetch(&self, delay: usize, o0: &mut [f64; MAX_U], o1: &mut [f64; MAX_U]) {
        if self.ring[0].is_empty() || self.ring[1].is_empty() {
            return;
        }
        let g0 = self.group.wrapping_sub(delay).wrapping_mul(self.u).wrapping_sub(self.pad);
        for r in 0..self.u {
            let i = g0.wrapping_add(r) & self.mask;
            o0[r] = self.ring[0][i];
            o1[r] = self.ring[1][i];
        }
    }

    /// Decimates one group per channel to the base rate. `b0`, `b1` are used as scratch.
    #[inline]
    pub fn down(&mut self, b0: &mut [f64; MAX_U], b1: &mut [f64; MAX_U]) -> (f64, f64) {
        let a = self.down[0].run(b0, self.k, &self.hb);
        let b = self.down[1].run(b1, self.k, &self.hb);
        (a, b)
    }

    /// As [`down`](Self::down), through the reference decimators.
    #[inline]
    pub fn down_ref(&mut self, b0: &mut [f64; MAX_U], b1: &mut [f64; MAX_U]) -> (f64, f64) {
        let a = self.down_ref[0].run(b0, self.k, &self.hb);
        let b = self.down_ref[1].run(b1, self.k, &self.hb);
        (a, b)
    }
}
