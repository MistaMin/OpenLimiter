//! Building blocks: delay lines, the split-filter ring, the true-peak reader, the sliding
//! maximum, the level follower and the safety limiter. Everything here is allocated once
//! (`alloc`) and then runs without allocation; all ring sizes are powers of two and every index
//! is masked, so nothing can run off the end.

/// Taps per phase of the 4x reconstruction filter.
const TP_TAPS: usize = 16;
/// Delay in samples between a sample going in and its peak reading coming out.
pub const TP_DELAY: usize = 8;
/// The detector and the true-peak reader work at this multiple of the sample rate.
pub const OVERSAMPLE: usize = 4;

// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Delay {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Delay {
    pub fn alloc(&mut self, len_pow2: usize) {
        let len = len_pow2.max(2).next_power_of_two();
        self.buf = vec![0.0; len];
        self.mask = len - 1;
        self.pos = 0;
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }

    /// Stores `x` and returns the sample from `d` samples ago (`d = 0` returns `x`).
    #[inline]
    pub fn push(&mut self, x: f32, d: usize) -> f32 {
        if self.buf.is_empty() {
            return x;
        }
        let m = self.mask;
        self.buf[self.pos & m] = x;
        let r = self.buf[(self.pos + m + 1 - (d & m)) & m];
        self.pos = (self.pos + 1) & m;
        r
    }
}

// ---------------------------------------------------------------------------------------------

/// Ring written twice so the newest N samples are always one contiguous slice.
#[derive(Default)]
pub struct Splitter {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Splitter {
    pub fn alloc(&mut self, len_pow2: usize) {
        let len = len_pow2.max(2).next_power_of_two();
        self.buf = vec![0.0; 2 * len];
        self.mask = len - 1;
        self.pos = 0;
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        if self.buf.is_empty() {
            return;
        }
        let p = self.pos & self.mask;
        self.buf[p] = x;
        self.buf[p + self.mask + 1] = x;
        self.pos = (p + 1) & self.mask;
    }

    /// The sample pushed `d` pushes before the newest one.
    #[inline]
    pub fn delayed(&self, d: usize) -> f32 {
        if self.buf.is_empty() {
            return 0.0;
        }
        let m = self.mask;
        self.buf[(self.pos + 2 * (m + 1) - 1 - (d & m)) & m]
    }

    /// Symmetric FIR centred `d` samples back, using `taps[d - half ..= d + half]`.
    #[inline]
    pub fn fir(&self, taps: &[f32], d: usize, half: usize) -> f32 {
        let m = self.mask;
        let half = half.min(d);
        let len = 2 * half + 1;
        if self.buf.is_empty() || d + half > m {
            return 0.0;
        }
        let start = (self.pos + 2 * (m + 1) - 1 - d - half) & m;
        match (self.buf.get(start..start + len), taps.get(d - half..d - half + len)) {
            (Some(x), Some(t)) => dot(x, t),
            _ => 0.0,
        }
    }
}

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 8];
    let ca = a.chunks_exact(8);
    let cb = b.chunks_exact(8);
    let ra = ca.remainder();
    let rb = cb.remainder();
    for (x, y) in ca.zip(cb) {
        for i in 0..8 {
            acc[i] += x[i] * y[i];
        }
    }
    let mut s = (acc[0] + acc[4]) + (acc[1] + acc[5]) + (acc[2] + acc[6]) + (acc[3] + acc[7]);
    for (x, y) in ra.iter().zip(rb.iter()) {
        s += x * y;
    }
    s
}

// ---------------------------------------------------------------------------------------------

/// Reads the true peak of a signal: the largest magnitude among each sample and the three
/// values a 4x reconstruction puts between it and the next sample (section 70.3).
pub struct TruePeak {
    h: [[f32; TP_TAPS]; 3],
    buf: [f32; 2 * TP_TAPS],
    pos: usize,
}

impl TruePeak {
    pub fn new() -> Self {
        let mut h = [[0.0f32; TP_TAPS]; 3];
        for (p, row) in h.iter_mut().enumerate() {
            let frac = (p + 1) as f64 / 4.0;
            let mut sum = 0.0;
            let mut tmp = [0.0f64; TP_TAPS];
            for (j, t) in tmp.iter_mut().enumerate() {
                // window slot j holds x[n - 15 + j]; the wanted instant is n - 8 + frac
                let a = 7.0 - j as f64 + frac;
                let s = if a.abs() < 1.0e-12 { 1.0 } else { (std::f64::consts::PI * a).sin() / (std::f64::consts::PI * a) };
                let u = std::f64::consts::PI * a / 8.0;
                // Blackman-Harris style 3-term window over +-8 samples
                let w = 0.42 + 0.5 * u.cos() + 0.08 * (2.0 * u).cos();
                *t = s * w.max(0.0);
                sum += *t;
            }
            for (o, t) in row.iter_mut().zip(tmp.iter()) {
                *o = (*t / sum) as f32;
            }
        }
        Self { h, buf: [0.0; 2 * TP_TAPS], pos: 0 }
    }

    pub fn clear(&mut self) {
        self.buf = [0.0; 2 * TP_TAPS];
        self.pos = 0;
    }

    /// Stores `x`; returns the sample from `TP_DELAY` samples ago.
    #[inline]
    pub fn push_plain(&mut self, x: f32) -> f32 {
        let p = self.pos & (TP_TAPS - 1);
        self.buf[p] = x;
        self.buf[p + TP_TAPS] = x;
        self.pos = (p + 1) & (TP_TAPS - 1);
        self.buf[self.pos + TP_TAPS - 1 - TP_DELAY]
    }

    /// Stores `x`; returns the sample from `TP_DELAY` samples ago and the magnitudes of the 4x
    /// reconstruction from that sample up to (not including) the next one, in time order.
    #[inline]
    pub fn push(&mut self, x: f32) -> (f32, [f32; OVERSAMPLE]) {
        let d = self.push_plain(x);
        let w = &self.buf[self.pos..self.pos + TP_TAPS];
        let mut out = [d.abs(); OVERSAMPLE];
        for (o, row) in out[1..].iter_mut().zip(self.h.iter()) {
            let mut v = 0.0f32;
            for j in 0..TP_TAPS {
                v += w[j] * row[j];
            }
            *o = v.abs();
        }
        (d, out)
    }
}

// ---------------------------------------------------------------------------------------------

/// Maximum over the last `window` values (monotonic queue in a ring).
#[derive(Default)]
pub struct SlideMax {
    val: Vec<f32>,
    idx: Vec<u64>,
    mask: usize,
    head: usize,
    tail: usize,
    n: u64,
}

impl SlideMax {
    pub fn alloc(&mut self, len_pow2: usize) {
        let len = len_pow2.max(4).next_power_of_two();
        self.val = vec![0.0; len];
        self.idx = vec![0; len];
        self.mask = len - 1;
        self.clear();
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.n = 0;
    }

    #[inline]
    pub fn push(&mut self, v: f32, window: usize) -> f32 {
        if self.val.is_empty() {
            return v;
        }
        let m = self.mask;
        let window = window.clamp(1, m.saturating_sub(1).max(1)) as u64;
        while self.tail != self.head {
            let b = (self.tail + m) & m;
            if self.val[b] <= v {
                self.tail = b;
            } else {
                break;
            }
        }
        self.val[self.tail] = v;
        self.idx[self.tail] = self.n;
        self.tail = (self.tail + 1) & m;
        while self.head != self.tail && self.idx[self.head] + window <= self.n {
            self.head = (self.head + 1) & m;
        }
        self.n += 1;
        if self.head == self.tail {
            // cannot happen (the newest entry never expires); keep the queue sane anyway
            self.head = (self.tail + m) & m;
        }
        self.val[self.head]
    }
}

// ---------------------------------------------------------------------------------------------

/// Per-sample coefficients of the level follower.
#[derive(Clone, Copy, Default)]
pub struct Timing {
    /// One-pole step towards a louder input.
    pub attack: f64,
    /// One-pole step of the rectified-level average that picks the release stage.
    pub average: f64,
    /// Per-sample multipliers of the fast and slow release stages.
    pub fast: f64,
    pub slow: f64,
    /// Samples the level is held after the last attack step.
    pub hold: u32,
    pub switch: f64,
}

/// Attack / hold / staged-release level follower (sections 22, 24, 39.4; `REPORT.md`, "Model").
///
/// * Input above the level: move towards it with the attack step.
/// * Otherwise wait out the hold, then decay exponentially (a straight line in dB): at the
///   fast rate while the level is above `switch` times the averaged input, at the slow rate
///   otherwise. A steady tone therefore sits in the slow stage, and its level settles below
///   the true peak by an amount set by attack and slow release.
#[derive(Clone, Copy, Default)]
pub struct Follower {
    p: f64,
    avg: f64,
    hold: u32,
}

impl Follower {
    #[inline]
    pub fn step(&mut self, v: f64, t: &Timing) -> f64 {
        self.avg += (v - self.avg) * t.average;
        if v > self.p {
            self.p += (v - self.p) * t.attack;
            self.hold = t.hold;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.p *= if self.p > t.switch * self.avg { t.fast } else { t.slow };
            if self.p < 1.0e-30 {
                self.p = 0.0;
            }
        }
        if self.avg < 1.0e-30 {
            self.avg = 0.0;
        }
        self.p
    }
}

// ---------------------------------------------------------------------------------------------

/// Stereo-linked look-ahead peak limiter with a hard ceiling.
///
/// Wanted gain per sample `t = min(1, ceiling / peak)`; minimum of `t` over `w + 1` samples;
/// a fixed exponential release that never rises above that minimum; a `w`-sample moving
/// average; applied to the signal delayed by `w - 1`. Every averaged term is at most the gain
/// the delayed sample needs, so no sample exceeds the ceiling.
#[derive(Default)]
pub struct Limiter {
    tp: Option<[TruePeak; 2]>,
    dl: [Delay; 2],
    smin: SlideMax,
    ring: Vec<f64>,
    mask: usize,
    pos: usize,
    sum: f64,
    rel: f64,
    w: usize,
    krel: f64,
}

impl Limiter {
    pub fn alloc(&mut self, w: usize, krel: f64) {
        let w = w.max(2);
        self.w = w;
        self.krel = krel;
        self.tp = Some([TruePeak::new(), TruePeak::new()]);
        let len = (w + 2).next_power_of_two();
        self.dl[0].alloc(len);
        self.dl[1].alloc(len);
        self.smin.alloc((w + 4).next_power_of_two());
        self.ring = vec![1.0; len];
        self.mask = len - 1;
        self.clear();
    }

    pub fn clear(&mut self) {
        if let Some(tp) = self.tp.as_mut() {
            tp[0].clear();
            tp[1].clear();
        }
        self.dl[0].clear();
        self.dl[1].clear();
        self.smin.clear();
        self.ring.iter_mut().for_each(|v| *v = 1.0);
        self.pos = 0;
        self.sum = self.w as f64;
        self.rel = 1.0;
    }

    /// Samples of delay through the limiter.
    pub fn delay(&self) -> usize {
        TP_DELAY + self.w.max(1) - 1
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, ceiling: f32, true_peak: bool) -> (f32, f32, f32) {
        let Some(tp) = self.tp.as_mut() else { return (l, r, 1.0) };
        if self.ring.is_empty() {
            return (l, r, 1.0);
        }
        let (dl, dr, pk) = if true_peak {
            let (a, pa) = tp[0].push(l);
            let (b, pb) = tp[1].push(r);
            let pk = pa.iter().chain(pb.iter()).fold(0.0f32, |m, v| m.max(*v));
            (a, b, pk)
        } else {
            let a = tp[0].push_plain(l);
            let b = tp[1].push_plain(r);
            (a, b, a.abs().max(b.abs()))
        };
        let t = if pk > ceiling { ceiling / pk } else { 1.0 };
        let m = -self.smin.push(-t, self.w + 1) as f64;
        self.rel = m.min(self.rel + (1.0 - self.rel) * self.krel);
        let old = self.ring[(self.pos + self.mask + 1 - self.w) & self.mask];
        self.ring[self.pos & self.mask] = self.rel;
        self.pos = (self.pos + 1) & self.mask;
        self.sum += self.rel - old;
        let g = ((self.sum / self.w as f64) as f32).clamp(0.0, 1.0);
        let mut yl = self.dl[0].push(dl, self.w - 1) * g;
        let mut yr = self.dl[1].push(dr, self.w - 1) * g;
        if !true_peak {
            // rounding guard only: by construction the samples are already within the ceiling
            yl = yl.clamp(-ceiling, ceiling);
            yr = yr.clamp(-ceiling, ceiling);
        }
        (yl, yr, g)
    }
}
