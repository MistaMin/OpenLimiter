//! Shared stages: static curves, the input matrix with its diamond allocation, the
//! multiband allocation, the crest meter that drives automatic release, the Option B
//! residual true-peak trim and gain match.

use crate::dsp::{
    db_to_lin, interval_peak, lin_to_db, pole, tp_kernels, Delay, KWeighting, MovingMean, SlidingMax, SlidingMin,
    TpHist, TpKernels, TP_HALF, TP_TAPS,
};

// ---------------------------------------------------------------------------------------------
// Static curves (pure functions, also for an editor)

/// Gain in dB (<= 0) of an infinite-ratio limiter with a quadratic soft knee of total width
/// `knee` dB, for a level `over` dB above the ceiling. Never above the hard-knee gain
/// `-max(over, 0)`; equal to it outside `[-knee/2, knee/2]`.
#[inline]
pub fn knee_gain_db(over: f64, knee: f64) -> f64 {
    if knee <= 1e-9 {
        return -over.max(0.0);
    }
    let h = 0.5 * knee;
    if over <= -h {
        0.0
    } else if over >= h {
        -over
    } else {
        let t = over + h;
        -t * t / (2.0 * knee)
    }
}

/// Lowest level (dB over the ceiling) at which [`knee_gain_db`] is below 0.
#[inline]
pub fn knee_floor_db(knee: f64) -> f64 {
    -0.5 * knee.max(0.0)
}

/// Unlinked diamond (Lagrange) gains for `|M| = a`, `|S| = b` and the L/R ceiling `c`,
/// blended with the linked gain `c / (a + b)` by `link` in [0, 1]. Returns `(g_M, g_S)` with
/// `g_M a + g_S b = c` whenever `a + b > c`, else `(1, 1)`.
#[inline]
pub fn diamond(a: f64, b: f64, c: f64, link: f64) -> (f64, f64) {
    let (a, b) = (a.abs(), b.abs());
    let sum = a + b;
    if !(sum > c) || sum <= 1e-30 {
        return (1.0, 1.0);
    }
    let e = sum - c;
    let n2 = a * a + b * b;
    let mut gm = 1.0 - a * e / n2;
    let mut gs = 1.0 - b * e / n2;
    // one-step water-fill (at most one of the two can go negative)
    if gm < 0.0 {
        gm = 0.0;
        gs = if b > 0.0 { (c / b).min(1.0) } else { 1.0 };
    } else if gs < 0.0 {
        gs = 0.0;
        gm = if a > 0.0 { (c / a).min(1.0) } else { 1.0 };
    }
    let gl = c / sum;
    let l = link.clamp(0.0, 1.0);
    ((1.0 - l) * gm + l * gl, (1.0 - l) * gs + l * gl)
}

/// The fast requirement (dB, <= 0) of both gain channels for peaks `p` and slow gains `gs`
/// (dB) already applied, with the ceiling `c_db`/`c_lin`, knee and transient link.
///
/// * Stereo: per channel `knee(P + G_slow - C)`, blended (linear gain) with the linked total.
/// * Mid-Side: the knee acts on the diamond size `|M'| + |S'|`, then the diamond allocation.
#[inline]
pub fn requirement(ms: bool, p: [f64; 2], gs: [f64; 2], c_db: f64, c_lin: f64, knee: f64, link: f64) -> [f64; 2] {
    let floor = c_lin * db_to_lin(knee_floor_db(knee));
    if ms {
        let a = p[0] * db_to_lin(gs[0]);
        let b = p[1] * db_to_lin(gs[1]);
        let sum = a + b;
        if !(sum > floor) {
            return [0.0, 0.0];
        }
        let k = knee_gain_db(lin_to_db(sum) - c_db, knee);
        if k >= 0.0 {
            return [0.0, 0.0];
        }
        let (gm, gsd) = diamond(a, b, sum * db_to_lin(k), link);
        [lin_to_db(gm).min(0.0), lin_to_db(gsd).min(0.0)]
    } else {
        if !(p[0] > floor) && !(p[1] > floor) {
            return [0.0, 0.0];
        }
        let mut k = [0.0; 2];
        for c in 0..2 {
            if p[c] > floor {
                k[c] = knee_gain_db(lin_to_db(p[c]) + gs[c] - c_db, knee);
            }
        }
        if link <= 0.0 {
            return k;
        }
        let tl = (gs[0] + k[0]).min(gs[1] + k[1]);
        let mut out = [0.0; 2];
        for c in 0..2 {
            let lk = (tl - gs[c]).min(0.0);
            if link >= 1.0 {
                out[c] = lk.min(k[c]);
            } else {
                let g = (1.0 - link) * db_to_lin(k[c]) + link * db_to_lin(lk);
                out[c] = lin_to_db(g).min(k[c]);
            }
        }
        out
    }
}

/// Directional Lagrange allocation of one channel's excess over its band signals `x`
/// (`bandFocus = focus` in [0, 1], neutral band weights). Returns the band gains (<= 1).
/// `d_b = (1 - phi) E / |x_sum| + phi c_b E / sum c_k^2`, `c_b = max(sgn(x_sum) x_b, 0)`,
/// clamped to [0, 1] with one water-fill step and a uniform fallback.
pub fn band_allocation(x: [f64; 4], c_db: f64, c_lin: f64, knee: f64, focus: f64) -> [f64; 4] {
    let xs = x[0] + x[1] + x[2] + x[3];
    let a = xs.abs();
    if !(a > c_lin * db_to_lin(knee_floor_db(knee))) {
        return [1.0; 4];
    }
    let k = knee_gain_db(lin_to_db(a) - c_db, knee);
    if k >= 0.0 {
        return [1.0; 4];
    }
    let e = a - a * db_to_lin(k);
    let s = if xs >= 0.0 { 1.0 } else { -1.0 };
    let phi = focus.clamp(0.0, 1.0);
    let uni = e / a;
    let mut cp = [0.0; 4];
    let mut den = 0.0;
    for b in 0..4 {
        cp[b] = (s * x[b]).max(0.0);
        den += cp[b] * cp[b];
    }
    let mut d = [uni; 4];
    if den > 0.0 {
        for b in 0..4 {
            d[b] = (1.0 - phi) * uni + phi * cp[b] * e / den;
        }
    }
    let delivered = |d: &[f64; 4]| (0..4).map(|b| d[b] * s * x[b]).sum::<f64>();
    let mut clamped = false;
    for v in d.iter_mut() {
        if *v > 1.0 {
            *v = 1.0;
            clamped = true;
        }
    }
    if clamped {
        let deficit = e - delivered(&d);
        if deficit > 0.0 {
            let mut free = 0.0;
            for b in 0..4 {
                if d[b] < 1.0 {
                    free += cp[b] * cp[b];
                }
            }
            if free > 0.0 {
                for b in 0..4 {
                    if d[b] < 1.0 {
                        d[b] = (d[b] + cp[b] * deficit / free).min(1.0);
                    }
                }
            }
        }
        if e - delivered(&d) > 1e-12 * e.max(1e-12) {
            d = [uni; 4];
        }
    }
    [1.0 - d[0], 1.0 - d[1], 1.0 - d[2], 1.0 - d[3]]
}

/// Release-time factor of the crest-adaptive release: `4^((12 - crest) / 9)`, crest clamped
/// to 3..21 dB (x4 for dense material, x1/4 for very spiky material), log-linear in between.
#[inline]
pub fn crest_release_factor(crest_db: f64) -> f64 {
    let c = crest_db.clamp(3.0, 21.0);
    4f64.powf((12.0 - c) / 9.0)
}

/// The first, quadratic-knee C1 clipper curve (normalised to the ceiling): `u` for
/// `|u| <= 1-k`, `1 - (1+k-|u|)^2 / (4k)` up to `1+k`, then 1. Odd.
#[inline]
pub fn clip_curve(u: f64, k: f64) -> f64 {
    let a = u.abs();
    let y = if a <= 1.0 - k {
        a
    } else if a >= 1.0 + k {
        1.0
    } else {
        let t = 1.0 + k - a;
        1.0 - t * t / (4.0 * k)
    };
    y.copysign(u)
}

/// Antiderivative of [`clip_curve`] (even).
#[inline]
pub fn clip_antiderivative(u: f64, k: f64) -> f64 {
    let a = u.abs();
    let lo = 1.0 - k;
    if a <= lo {
        0.5 * a * a
    } else if a < 1.0 + k {
        let t = 1.0 + k - a;
        0.5 * lo * lo + (a - lo) + (t * t * t - 8.0 * k * k * k) / (12.0 * k)
    } else {
        0.5 * lo * lo + 2.0 * k - 2.0 * k * k / 3.0 + (a - (1.0 + k))
    }
}

/// First-order ADAA of the clipper: mean of the curve over the line from `u1` to `u0`, with
/// the midpoint (first-order Taylor) for `|du| < 1e-5`. Bounded by 1 (mean value theorem);
/// clamped against rounding.
#[inline]
pub fn clip_adaa1(u0: f64, u1: f64, k: f64) -> f64 {
    let d = u0 - u1;
    let y = if d.abs() < 1e-5 {
        clip_curve(0.5 * (u0 + u1), k)
    } else {
        (clip_antiderivative(u0, k) - clip_antiderivative(u1, k)) / d
    };
    if y.is_finite() {
        y.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------------------------
// Crest meter for automatic release

/// 30 ms RMS against 30 ms peak of the detector. Updated per sample; the factor is read every
/// 16 samples by the engines.
#[derive(Default)]
pub struct Crest {
    ms: MovingMean,
    pk: SlidingMax,
    win: usize,
    pub crest_db: f64,
}

impl Crest {
    pub fn alloc(&mut self, fs: f64) {
        self.win = crate::dsp::ms_to_samples(30.0, fs, 1);
        self.ms.alloc(self.win);
        self.ms.set_len(self.win);
        self.pk.alloc(self.win);
        self.clear();
    }
    pub fn clear(&mut self) {
        self.ms.clear();
        self.pk.clear();
        self.crest_db = 12.0;
    }
    /// `x2`: mean square of the channels; `p`: their peak.
    #[inline]
    pub fn push(&mut self, x2: f64, p: f64, update: bool) {
        let m = self.ms.push(x2);
        let pk = self.pk.push(p, self.win);
        if update {
            self.crest_db = if m > 1e-14 && pk > 1e-7 { lin_to_db(pk) - 10.0 * m.log10() } else { 12.0 };
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Option B residual true-peak micro-trim

/// Shared post-engine trim: idle (`g = 1`, bit transparent) while the reconstructed (or, in
/// sample-peak mode, the sample) peak stays within the ceiling; otherwise a smooth ramp
/// (two boxcars, 21 samples at 48 kHz) and a flat top, ahead of the over.
///
/// Delay `TP_HALF + 2 (bl - 1) + f_new` (32 samples at 48 kHz: 8 + 20 + 4).
pub struct Trim {
    kern: TpKernels,
    bound: f64,
    wmax: SlidingMax,
    hist: [TpHist; 2],
    audio: [Delay; 2],
    refs: [Delay; 2],
    trace: Delay,
    hold: SlidingMin,
    b1: ExactBox,
    b2: ExactBox,
    w: usize,
    pub delay: usize,
    pub last_gain: f64,
    pub block_min_gain: f64,
    pub block_peak: f64,
}

impl Default for Trim {
    fn default() -> Self {
        let kern = tp_kernels();
        Self {
            bound: crate::dsp::tp_gain_bound(&kern),
            kern,
            wmax: SlidingMax::default(),
            hist: [TpHist::default(); 2],
            audio: [Delay::default(), Delay::default()],
            refs: [Delay::default(), Delay::default()],
            trace: Delay::default(),
            hold: SlidingMin::default(),
            b1: ExactBox::default(),
            b2: ExactBox::default(),
            w: 28,
            delay: 32,
            last_gain: 1.0,
            block_min_gain: 1.0,
            block_peak: 0.0,
        }
    }
}

/// The trim's delay at `fs`.
pub fn trim_delay(fs: f64) -> usize {
    let (bl, f_new, _) = trim_geometry(fs);
    TP_HALF + 2 * (bl - 1) + f_new
}

fn trim_geometry(fs: f64) -> (usize, usize, usize) {
    let s = fs / 48_000.0;
    let bl = ((11.0 * s).round() as usize).max(2);
    let f_new = ((4.0 * s).round() as usize).max(1);
    let f_old = ((2.0 * s).round() as usize).max(1);
    (bl, f_new, f_old)
}

impl Trim {
    pub fn alloc(&mut self, fs: f64) {
        let (bl, f_new, f_old) = trim_geometry(fs);
        self.delay = TP_HALF + 2 * (bl - 1) + f_new;
        // intervals of output sample k are known at k + TP_HALF - 1 and k + TP_HALF
        self.w = self.delay - (TP_HALF - 1) + 1 + f_old;
        self.hold.alloc(self.w + 1);
        self.wmax.alloc(TP_TAPS);
        self.b1.alloc(bl);
        self.b2.alloc(bl);
        for d in self.audio.iter_mut().chain(self.refs.iter_mut()) {
            d.alloc(self.delay);
        }
        self.trace.alloc(self.delay);
        self.clear();
    }

    pub fn clear(&mut self) {
        for h in self.hist.iter_mut() {
            h.clear();
        }
        for d in self.audio.iter_mut().chain(self.refs.iter_mut()) {
            d.clear();
        }
        self.trace.clear();
        self.hold.clear();
        self.wmax.clear();
        self.b1.clear();
        self.b2.clear();
        self.last_gain = 1.0;
        self.block_min_gain = 1.0;
        self.block_peak = 0.0;
    }

    /// One stereo sample. `tp`: reconstructed peak (else sample peak); `refine`: parabolic
    /// refinement. Returns the delayed, trimmed output, the delayed reference and trace.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn tick(&mut self, l: f64, r: f64, rl: f64, rr: f64, tr: f64, c: f64, tp: bool, refine: bool) -> [f64; 6] {
        self.hist[0].push(l);
        self.hist[1].push(r);
        // the largest sample in the reader's window bounds every interpolated point; when even
        // that bound stays under the ceiling the reader is skipped (the result is the same)
        let wm = self.wmax.push(l.abs().max(r.abs()), TP_TAPS);
        let mut q = 0.0f64;
        let read = tp && wm * self.bound > c;
        for h in self.hist.iter() {
            let p = if read { interval_peak(h.window(), h.ends(), &self.kern, refine) } else { h.end_peak() };
            q = q.max(p);
        }
        if !q.is_finite() {
            q = 0.0;
        }
        self.block_peak = self.block_peak.max(q);
        let req = if q > c { c / q } else { 1.0 };
        let h = self.hold.push(req, self.w);
        let g = self.b2.push(self.b1.push(h));
        let d = self.delay;
        let yl = self.audio[0].push(l, d);
        let yr = self.audio[1].push(r, d);
        let zl = self.refs[0].push(rl, d);
        let zr = self.refs[1].push(rr, d);
        let t = self.trace.push(tr, d);
        self.last_gain = g;
        if g < self.block_min_gain {
            self.block_min_gain = g;
        }
        if g == 1.0 {
            [yl, yr, zl, zr, t, g]
        } else {
            [yl * g, yr * g, zl, zr, t, g]
        }
    }
}

/// Boxcar on gains <= 1 that returns exactly 1.0 while every value in it is 1.0.
#[derive(Default)]
struct ExactBox {
    buf: Vec<f64>,
    pos: usize,
    sum: f64,
    below: usize,
}

impl ExactBox {
    fn alloc(&mut self, n: usize) {
        self.buf = vec![1.0; n.max(1)];
        self.clear();
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 1.0);
        self.pos = 0;
        self.sum = self.buf.len() as f64;
        self.below = 0;
    }
    #[inline]
    fn push(&mut self, v: f64) -> f64 {
        let n = self.buf.len();
        if n == 0 {
            return v;
        }
        if self.pos >= n {
            self.pos = 0;
        }
        let old = self.buf[self.pos];
        self.buf[self.pos] = v;
        self.pos += 1;
        if old < 1.0 {
            self.below -= 1;
        }
        if v < 1.0 {
            self.below += 1;
        }
        if self.below == 0 {
            self.sum = n as f64;
            return 1.0;
        }
        self.sum += v - old;
        (self.sum / n as f64).min(1.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Gain match

/// K-weighted loudness of the module input (before drive) and of the output (after the
/// trim), integrated over about 3 s while the input is active (above -70 LUFS and within
/// 20 LU of its own long-term level). The applied gain is `min(L_in - L_out, 0)`, smoothed.
pub struct GainMatch {
    kin: [KWeighting; 2],
    kout: [KWeighting; 2],
    ms_in: f64,
    ms_out: f64,
    mom: f64,
    lt: f64,
    k_int: f64,
    k_mom: f64,
    k_lt: f64,
    k_gain: f64,
    k_fade: f64,
    measured_db: f64,
    smoothed_db: f64,
    on_mix: f64,
    count: u64,
    pub applied_db: f64,
}

impl Default for GainMatch {
    fn default() -> Self {
        Self {
            kin: [KWeighting::default(); 2],
            kout: [KWeighting::default(); 2],
            ms_in: 0.0,
            ms_out: 0.0,
            mom: 0.0,
            lt: -70.0,
            k_int: 0.0,
            k_mom: 0.0,
            k_lt: 0.0,
            k_gain: 0.0,
            k_fade: 0.0,
            measured_db: 0.0,
            smoothed_db: 0.0,
            on_mix: 0.0,
            count: 0,
            applied_db: 0.0,
        }
    }
}

impl GainMatch {
    pub fn prepare(&mut self, fs: f64) {
        for k in self.kin.iter_mut().chain(self.kout.iter_mut()) {
            k.prepare(fs);
        }
        self.k_int = pole(3000.0, fs);
        self.k_mom = pole(400.0, fs);
        self.k_lt = pole(10_000.0, fs);
        self.k_gain = pole(500.0, fs);
        self.k_fade = pole(20.0 / 3.0, fs);
        self.clear();
    }

    pub fn clear(&mut self) {
        for k in self.kin.iter_mut().chain(self.kout.iter_mut()) {
            k.clear();
        }
        self.ms_in = 0.0;
        self.ms_out = 0.0;
        self.mom = 0.0;
        self.lt = -70.0;
        self.measured_db = 0.0;
        self.smoothed_db = 0.0;
        self.count = 0;
        self.applied_db = 0.0;
    }

    pub fn loudness_in(&self) -> f64 {
        -0.691 + 10.0 * (self.ms_in + 1e-20).log10()
    }
    pub fn loudness_out(&self) -> f64 {
        -0.691 + 10.0 * (self.ms_out + 1e-20).log10()
    }

    /// Feeds one input sample (before drive) and one output sample; returns the gain.
    #[inline]
    pub fn tick(&mut self, il: f64, ir: f64, ol: f64, or: f64, on: bool) -> f64 {
        let a = self.kin[0].run(il);
        let b = self.kin[1].run(ir);
        let c = self.kout[0].run(ol);
        let d = self.kout[1].run(or);
        let xin = a * a + b * b;
        let xout = c * c + d * d;
        self.mom = xin + self.k_mom * (self.mom - xin);
        let lm = -0.691 + 10.0 * (self.mom + 1e-20).log10();
        if lm > -70.0 {
            self.lt = lm + self.k_lt * (self.lt - lm);
        }
        let active = lm > -70.0 && lm > self.lt - 20.0;
        if active {
            self.count += 1;
            self.ms_in = xin + self.k_int * (self.ms_in - xin);
            self.ms_out = xout + self.k_int * (self.ms_out - xout);
            if self.ms_out > 1e-20 && self.ms_in > 1e-20 {
                self.measured_db = (10.0 * (self.ms_in / self.ms_out).log10()).clamp(-24.0, 0.0);
            }
        }
        self.smoothed_db = self.measured_db + self.k_gain * (self.smoothed_db - self.measured_db);
        let t = if on { 1.0 } else { 0.0 };
        self.on_mix = t + self.k_fade * (self.on_mix - t);
        if (self.on_mix - t).abs() < 1e-9 {
            self.on_mix = t;
        }
        self.applied_db = self.on_mix * self.smoothed_db;
        if self.applied_db == 0.0 {
            1.0
        } else {
            db_to_lin(self.applied_db)
        }
    }
}
