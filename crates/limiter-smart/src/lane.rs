//! The envelope lane shared by the hold-and-smooth engines, in dB (gains <= 0).
//!
//! ```text
//! requirement r[n] -> sliding min over W -> release (one pole, optional Join Bend)
//!                  -> [engine: second stage, links] -> 4 boxcars of length b (cubic B-spline,
//!                     support S = 4(b-1)+1) -> gain delay (d_fix - D)
//! ```
//!
//! Index algebra. The output `s[n]` is applied to the audio that was at the detector at
//! `n - D`. With a flat top of `F` samples each side, every boxcar input `h[n-j]`
//! (`j < S`) must be a minimum over a window that covers `[n-D-F, n-D+F]`, which holds when
//! `S - 1 + F <= D` and `W >= D + F + 1`. The oversampled engines interpolate between
//! `s[n-1]` and `s[n]` across the sub-samples of one base sample, so both must satisfy the
//! bound: one more sample, `S + F <= D` (`os = 1`). The lane keeps one extra sample of hold
//! (`W = D + F + 2 + H_post`) so that the one-sample steps of a lookahead glide stay covered.
//!
//! Lookahead glide (DualSone style): `W` follows at one sample per push, `D` moves one sample
//! every 8 pushes and the boxcar length one sample every 8 pushes (in between), always in the
//! order that keeps the two conditions true. The gain delay `d_fix - D` takes up the change,
//! so the audio delay (and `latency()`) never moves.

use crate::dsp::{pole, Delay, MovingMean, SlidingMin};

/// Decay of the Join Bend offset, ms.
pub const JOIN_DECAY_MS: f64 = 8.0;
/// Upward relaxation of the Join Bend low-water mark, ms.
pub const JOIN_LOW_WATER_MS: f64 = 20.0;

#[derive(Default)]
pub struct Lane {
    hold: SlidingMin,
    boxes: [MovingMean; 4],
    gdelay: Delay,
    /// The released minimum (dB): `min(h_rel + j, m)`.
    pub held: f64,
    /// The last sliding minimum (dB).
    pub m: f64,
    /// The pure (un-bent) one-pole release state (dB).
    h_rel: f64,
    /// The decaying Join Bend offset (dB, <= 0).
    j: f64,
    /// Low-water mark of the hold: the bend only acts when the hold goes below it.
    m_low: f64,
    c_join: f64,
    c_up: f64,
    d_fix: usize,
    f: usize,
    os: usize,
    d: usize,
    d_t: usize,
    b: usize,
    b_t: usize,
    w: usize,
    h_post: usize,
    tick: u32,
}

/// The delay `D` and boxcar length `b` for a lookahead of `m` samples.
pub fn geometry(m: usize, f: usize, os: usize) -> (usize, usize) {
    let m = m.max(1);
    (m - 1 + f + os, (m - 1) / 4 + 1)
}

impl Lane {
    /// `d_fix`: the largest `D` (the lane's fixed delay); `h_max`: the largest post-hold.
    pub fn alloc(&mut self, d_fix: usize, f: usize, os: usize, h_max: usize) {
        self.d_fix = d_fix;
        self.f = f;
        self.os = os;
        self.hold.alloc(d_fix + f + 3 + h_max);
        for b in self.boxes.iter_mut() {
            b.alloc(d_fix / 4 + 2);
        }
        self.gdelay.alloc(d_fix + 1);
        self.d = d_fix;
        self.d_t = d_fix;
        self.b = 1;
        self.b_t = 1;
        self.w = d_fix + f + 2;
        if self.c_join == 0.0 {
            self.set_rate(48_000.0);
        }
        self.clear();
    }

    /// The rate the lane runs at (for the Join Bend poles); 48 kHz unless set.
    pub fn set_rate(&mut self, fs: f64) {
        self.c_join = pole(JOIN_DECAY_MS, fs);
        self.c_up = pole(JOIN_LOW_WATER_MS, fs);
    }

    pub fn d_fix(&self) -> usize {
        self.d_fix
    }

    /// Current `D`.
    pub fn d(&self) -> usize {
        self.d
    }

    pub fn boxlen(&self) -> usize {
        self.b
    }

    /// Sets the lookahead target (samples) and the post-hold (samples after the peak).
    pub fn set_target(&mut self, m: usize, h_post: usize) {
        let (d, b) = geometry(m, self.f, self.os);
        self.d_t = d.min(self.d_fix);
        // keep S - 1 + F + os <= D at the clamped D
        let bmax = (self.d_t.saturating_sub(self.f + self.os)) / 4 + 1;
        self.b_t = b.min(bmax).max(1);
        self.h_post = h_post;
    }

    /// Jumps to the targets (after `reset`).
    pub fn snap(&mut self) {
        self.d = self.d_t;
        self.b = self.b_t;
        for bx in self.boxes.iter_mut() {
            bx.set_len(self.b);
        }
        self.w = self.d + self.f + 2 + self.h_post;
    }

    pub fn clear(&mut self) {
        self.hold.clear();
        for b in self.boxes.iter_mut() {
            b.clear();
        }
        self.gdelay.clear();
        self.held = 0.0;
        self.m = 0.0;
        self.h_rel = 0.0;
        self.j = 0.0;
        self.m_low = 0.0;
        self.tick = 0;
    }

    #[inline]
    fn glide(&mut self) {
        let w_t = self.d.max(self.d_t) + self.f + 2 + self.h_post;
        if self.w < w_t {
            self.w += 1;
        } else {
            self.w = w_t;
        }
        self.tick = self.tick.wrapping_add(1);
        let phase = self.tick & 7;
        if phase == 0 {
            if self.d < self.d_t && self.w > self.d + 1 + self.f + 2 {
                self.d += 1;
            } else if self.d > self.d_t && 4 * (self.b - 1) + self.f + self.os < self.d {
                self.d -= 1;
            }
        } else if phase == 4 {
            if self.b < self.b_t && 4 * self.b + self.f + self.os <= self.d {
                self.b += 1;
                for bx in self.boxes.iter_mut() {
                    bx.set_len(self.b);
                }
            } else if self.b > self.b_t {
                self.b -= 1;
                for bx in self.boxes.iter_mut() {
                    bx.set_len(self.b);
                }
            }
        }
    }

    /// Hold and one-pole release of a requirement (dB, <= 0). `c_rel` is the release pole;
    /// `join` the Join Bend width in dB (0 = off). Returns the released value (<= the hold).
    ///
    /// The release `h_rel` is never bent itself. The bend lives in a separate offset
    /// `j <= 0` that decays (8 ms), and it is only integrated while the hold goes below its
    /// low-water mark `m_low` (which snaps down and relaxes up over 20 ms). Downward chatter
    /// of the hold on dense material (a short hold window on noise) therefore cannot ratchet
    /// the released value below the peaks, while a separate second burst still gets the
    /// exact closed-form join. The output `min(h_rel + j, m)` is continuous across hold
    /// crossings and never above the hold.
    #[inline]
    pub fn hold_release(&mut self, req: f64, c_rel: f64, join: f64) -> f64 {
        self.glide();
        let m = self.hold.push(req, self.w);
        if m <= self.h_rel {
            self.j = (self.h_rel + self.j - m).min(0.0);
            self.h_rel = m;
            self.m_low = m;
            self.j *= self.c_join;
        } else {
            self.h_rel = m + c_rel * (self.h_rel - m);
            if m - self.h_rel < 1e-10 {
                self.h_rel = m;
            }
            self.j *= self.c_join;
            if m < self.m_low {
                if join > 1e-9 {
                    self.j += join_bend(self.h_rel + self.j, self.m_low, m, join);
                }
                self.m_low = m;
            } else {
                self.m_low = m + self.c_up * (self.m_low - m);
            }
        }
        if self.j > -1e-12 {
            self.j = 0.0;
        }
        self.m = m;
        self.held = (self.h_rel + self.j).min(m);
        self.held
    }

    /// The four boxcars (cubic B-spline).
    #[inline]
    pub fn smooth(&mut self, v: f64) -> f64 {
        let mut y = v;
        for b in self.boxes.iter_mut() {
            y = b.push(y);
        }
        if y.abs() < 1e-9 {
            0.0
        } else {
            y
        }
    }

    /// The gain delay `d_fix - D`.
    #[inline]
    pub fn delay(&mut self, v: f64) -> f64 {
        self.gdelay.push(v, self.d_fix - self.d.min(self.d_fix))
    }
}

/// The exact, step-size invariant Join Bend: the change of the released value `h` when the
/// hold steps down from `m0` to `m1` (`m1 < m0`), the integral of `closeness^2` over the
/// step with `closeness = clamp(1 - (m - h) / W, 0, 1)`:
/// `dh = -(W / 3) (u1^3 - u0^3) <= 0`.
#[inline]
pub fn join_bend(h: f64, m0: f64, m1: f64, w: f64) -> f64 {
    if w <= 1e-9 || m1 >= m0 {
        return 0.0;
    }
    let u0 = (1.0 - (m0 - h) / w).clamp(0.0, 1.0);
    let u1 = (1.0 - (m1 - h) / w).clamp(0.0, 1.0);
    -(w / 3.0) * (u1 * u1 * u1 - u0 * u0 * u0)
}
