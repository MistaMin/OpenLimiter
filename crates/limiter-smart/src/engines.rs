//! The six engines. Each takes one post-drive stereo sample and returns one stereo sample
//! (and the unity-gain reference for delta) after its own fixed latency; the module pads
//! every engine to the same latency.
//!
//! Gain channels: in Stereo the two gain channels are L and R, in Mid-Side they are M and S
//! (`M = (L+R)/2`, `S = (L-R)/2`), and every requirement is computed with the diamond bound
//! so that the decoded L/R stays within the ceiling (see `stages::requirement`).

use crate::dsp::{
    db_to_lin, lin_to_db, ms_to_samples, pole, tp_kernels, Delay, KWeighting, MovingMean, SlidingMax, TpHist, TP_HALF,
    TP_TAPS,
};
use crate::hull::Hull;
use crate::lane::Lane;
use crate::os::{passband_peak, OsPath, MAX_U, OS_DELAY};
use crate::stages::{
    band_allocation, clip_adaa1, clip_curve, crest_release_factor, diamond, knee_gain_db, requirement, Crest,
};

/// Engines aim this far under the ceiling (relative) so that in-band tones leave the shared
/// trim idle.
pub const MARGIN: f64 = 1.0 - 1e-5;
/// Largest lookahead, ms.
pub const LOOKAHEAD_MAX_MS: f64 = 15.0;

/// Settings for one block, in engine units.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub c_lin: f64,
    pub c_db: f64,
    pub knee: f64,
    pub release_ms: f64,
    /// Lookahead, samples.
    pub m: usize,
    pub lt: f64,
    pub lr: f64,
    pub tp: bool,
    pub refine: bool,
    pub auto: bool,
    pub body: f64,
    pub clip_hr_db: f64,
    pub clip_k: f64,
    pub focus: f64,
    pub crest_mode: bool,
    pub crest_hr: f64,
    pub slow_win_s: f64,
    pub bass_hold_ms: f64,
    pub pause_hold: bool,
    pub join_db: f64,
    pub kweight: bool,
    pub splits: [f64; 3],
    pub need_ref: bool,
}

impl Default for Ctx {
    fn default() -> Self {
        Self {
            c_lin: db_to_lin(-0.2),
            c_db: -0.2,
            knee: 0.0,
            release_ms: 100.0,
            m: 72,
            lt: 1.0,
            lr: 1.0,
            tp: true,
            refine: false,
            auto: true,
            body: 0.5,
            clip_hr_db: 1.0,
            clip_k: 0.4,
            focus: 1.0,
            crest_mode: false,
            crest_hr: 10.0,
            slow_win_s: 0.4,
            bass_hold_ms: 14.0,
            pause_hold: true,
            join_db: 6.0,
            kweight: true,
            splits: [120.0, 800.0, 4500.0],
            need_ref: false,
        }
    }
}

/// What an engine reports for meters, the deepest of the block (dB, <= 0) unless noted.
#[derive(Clone, Copy, Debug)]
pub struct Probe {
    pub slow: f64,
    pub fast: f64,
    pub clip: f64,
    pub band: [f64; 4],
    pub catcher: f64,
    pub ch: [f64; 2],
    pub total: f64,
    /// Last values (not minima).
    pub crest: f64,
    pub rel_ms: f64,
    pub loud: f64,
    pub hold: bool,
    pub post_ms: f64,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            slow: 0.0,
            fast: 0.0,
            clip: 0.0,
            band: [0.0; 4],
            catcher: 0.0,
            ch: [0.0; 2],
            total: 0.0,
            crest: 0.0,
            rel_ms: 0.0,
            loud: -100.0,
            hold: false,
            post_ms: 0.0,
        }
    }
}

impl Probe {
    #[inline]
    fn ch(&mut self, g: [f64; 2]) {
        self.ch[0] = self.ch[0].min(g[0]);
        self.ch[1] = self.ch[1].min(g[1]);
        self.total = self.total.min(g[0].min(g[1]));
    }
}

/// One output sample of an engine.
#[derive(Clone, Copy, Default, Debug)]
pub struct Frame {
    pub l: f64,
    pub r: f64,
    pub rl: f64,
    pub rr: f64,
    /// Engine gain (dB, the lower of the two channels), aligned with the output.
    pub g: f64,
}

#[inline]
fn encode(ms: bool, l: f64, r: f64) -> (f64, f64) {
    if ms {
        (0.5 * (l + r), 0.5 * (l - r))
    } else {
        (l, r)
    }
}

#[inline]
fn decode(ms: bool, a: f64, b: f64) -> (f64, f64) {
    if ms {
        (a + b, a - b)
    } else {
        (a, b)
    }
}

#[inline]
fn gain(db: f64) -> f64 {
    if db == 0.0 {
        1.0
    } else {
        db_to_lin(db)
    }
}

/// Flat top of the Smooth-type lanes: 8 samples at 48 kHz.
pub fn flat_top(fs: f64) -> usize {
    ((8.0 * fs / 48_000.0).round() as usize).max(1)
}

/// Lookahead in samples, 0.5..15 ms.
pub fn lookahead_samples(ms: f64, fs: f64) -> usize {
    ms_to_samples(ms.clamp(0.5, LOOKAHEAD_MAX_MS), fs, 1)
}

fn m_max(fs: f64) -> usize {
    lookahead_samples(LOOKAHEAD_MAX_MS, fs)
}

/// Crossover delay of the linear-phase crossover (half of the 256-tap kernel at 48 kHz).
pub fn crossover_delay(fs: f64) -> usize {
    ((128.0 * fs / 48_000.0).round() as usize).max(8)
}

fn catcher_lookahead(fs: f64) -> usize {
    ms_to_samples(0.5, fs, 2)
}

fn rounder_len(fs: f64) -> usize {
    ms_to_samples(0.5, fs, 2)
}

/// Latency of each engine (Pure, Smooth, Precise, Punch, Multiband, Glide) at `fs`.
pub fn engine_latencies(fs: f64) -> [usize; 6] {
    let m = m_max(fs);
    let f = flat_top(fs);
    let pure = OS_DELAY + m;
    let smooth = TP_HALF + (m - 1 + f);
    let precise = OS_DELAY + (m + f);
    let punch = OS_DELAY + m;
    let multi = crossover_delay(fs) + (m - 1 + f) + TP_HALF + (catcher_lookahead(fs) - 1 + f);
    let glide = TP_HALF + m + (rounder_len(fs) - 1);
    [pure, smooth, precise, punch, multi, glide]
}

// ---------------------------------------------------------------------------------------------

/// 1x detector: sample peak, or with `tp` the largest of the sample and the reconstructed
/// intervals on either side. Returns the value for the sample `TP_HALF` pushes ago.
pub struct Det1x {
    hist: [TpHist; 2],
    prev: [f64; 2],
    kern: crate::dsp::TpKernels,
    bound: f64,
    wmax: SlidingMax,
}

impl Default for Det1x {
    fn default() -> Self {
        let kern = tp_kernels();
        Self { hist: [TpHist::default(); 2], prev: [0.0; 2], bound: crate::dsp::tp_gain_bound(&kern), kern, wmax: SlidingMax::default() }
    }
}

impl Det1x {
    pub fn alloc(&mut self) {
        self.wmax.alloc(TP_TAPS);
    }
    pub fn clear(&mut self) {
        for h in self.hist.iter_mut() {
            h.clear();
        }
        self.prev = [0.0; 2];
        self.wmax.clear();
    }
    /// `quiet`: below this level no requirement is formed, so while every sample in the
    /// reader's window times its gain bound stays under it, the reader is skipped.
    #[inline]
    pub fn push(&mut self, a: f64, b: f64, tp: bool, refine: bool, quiet: f64) -> [f64; 2] {
        self.hist[0].push(a);
        self.hist[1].push(b);
        let wm = self.wmax.push(a.abs().max(b.abs()), TP_TAPS);
        let read = tp && wm * self.bound > quiet;
        let mut out = [0.0; 2];
        for c in 0..2 {
            let h = &self.hist[c];
            let (e0, e1) = h.ends();
            let s = e0.abs();
            if read {
                let now = crate::dsp::interval_peak(h.window(), (e0, e1), &self.kern, refine);
                out[c] = s.max(now).max(self.prev[c]);
                self.prev[c] = now;
            } else {
                out[c] = s;
                self.prev[c] = 0.0;
            }
            if !out[c].is_finite() {
                out[c] = 0.0;
            }
        }
        out
    }
}

/// Release timing from the crest meter, refreshed every 16 samples.
#[derive(Default)]
struct Timing {
    tau: f64,
    cnt: u32,
}

impl Timing {
    #[inline]
    fn due(&mut self) -> bool {
        self.cnt = self.cnt.wrapping_add(1);
        (self.cnt & 15) == 1
    }
    fn update(&mut self, ctx: &Ctx, crest: f64) -> f64 {
        let f = if ctx.auto { crest_release_factor(crest) } else { 1.0 };
        self.tau = (ctx.release_ms * f).clamp(1.0, 8000.0);
        self.tau
    }
}

// =============================================================================================
// Pure

/// Engine 0: DualSone-derived slow macro engine plus fast peak lane, oversampled.
pub struct Pure {
    fs: f64,
    d_fix: usize,
    ms: bool,
    osp: OsPath,
    os_peak: f64,
    lanes: [Lane; 2],
    gs_delay: [Delay; 2],
    slow: SlowMacro,
    crest: Crest,
    timing: Timing,
    c_rel: f64,
    m: usize,
    // bass-adaptive post-hold
    lp: [f64; 2],
    a_lp: f64,
    plo: SlidingMax,
    ptot: SlidingMax,
    w20: usize,
    rho: f64,
    k_rho: f64,
    h_post: usize,
    gprev: [f64; 2],
    trace: Delay,
}

impl Default for Pure {
    fn default() -> Self {
        Self {
            fs: 48_000.0,
            d_fix: 1,
            ms: false,
            osp: OsPath::default(),
            os_peak: 1.0,
            lanes: Default::default(),
            gs_delay: Default::default(),
            slow: SlowMacro::default(),
            crest: Crest::default(),
            timing: Timing::default(),
            c_rel: 0.0,
            m: 0,
            lp: [0.0; 2],
            a_lp: 0.0,
            plo: SlidingMax::default(),
            ptot: SlidingMax::default(),
            w20: 1,
            rho: 0.0,
            k_rho: 0.0,
            h_post: 0,
            gprev: [1.0; 2],
            trace: Delay::default(),
        }
    }
}

impl Pure {
    pub fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        self.d_fix = m_max(fs);
        self.osp.alloc(self.d_fix + 2);
        let h_max = ms_to_samples(20.5, fs, 1);
        for l in self.lanes.iter_mut() {
            l.alloc(self.d_fix, 0, 1, h_max);
            l.set_rate(fs);
        }
        for d in self.gs_delay.iter_mut() {
            d.alloc(self.d_fix + 1);
        }
        self.slow.alloc(fs);
        self.crest.alloc(fs);
        self.a_lp = 1.0 - (-2.0 * std::f64::consts::PI * 180.0 / fs).exp();
        self.w20 = ms_to_samples(20.0, fs, 1);
        self.plo.alloc(self.w20);
        self.ptot.alloc(self.w20);
        self.k_rho = pole(50.0, fs);
        self.trace.alloc(OS_DELAY);
        self.reset();
    }

    pub fn configure(&mut self, u: usize, ms: bool) {
        self.osp.set_factor(u);
        self.os_peak = passband_peak(self.osp.k, self.fs, 20_000f64.min(0.45 * self.fs));
        self.ms = ms;
    }

    pub fn latency(&self) -> usize {
        OS_DELAY + self.d_fix
    }

    pub fn reset(&mut self) {
        self.osp.clear();
        for l in self.lanes.iter_mut() {
            l.clear();
        }
        for d in self.gs_delay.iter_mut() {
            d.clear();
        }
        self.slow.clear();
        self.crest.clear();
        self.timing = Timing::default();
        self.lp = [0.0; 2];
        self.plo.clear();
        self.ptot.clear();
        self.rho = 0.0;
        self.m = 0;
        self.gprev = [1.0; 2];
        self.trace.clear();
    }

    pub fn tick(&mut self, ctx: &Ctx, l: f64, r: f64, pr: &mut Probe) -> Frame {
        let ms = self.ms;
        let (a0, a1) = encode(ms, l, r);
        let (p0, p1) = self.osp.push(a0, a1);
        let ce = ctx.c_lin * MARGIN / self.os_peak;
        let ce_db = lin_to_db(ce);
        let pk = if ms { p0 + p1 } else { p0.max(p1) };
        let due = self.timing.due();
        self.crest.push(0.5 * (l * l + r * r), pk, due);

        // bass-adaptive post-hold: 180 Hz one-pole share of the 20 ms peak
        for (c, x) in [a0, a1].into_iter().enumerate() {
            self.lp[c] += self.a_lp * (x - self.lp[c]);
        }
        let lo = if ms { self.lp[0].abs() + self.lp[1].abs() } else { self.lp[0].abs().max(self.lp[1].abs()) };
        let plo = self.plo.push(lo, self.w20);
        let ptot = self.ptot.push(pk, self.w20);
        let rho_t = if ptot > 1e-9 { (plo / ptot).clamp(0.0, 1.0) } else { 0.0 };
        self.rho = rho_t + self.k_rho * (self.rho - rho_t);

        if due || self.m != ctx.m {
            let tau = self.timing.update(ctx, self.crest.crest_db);
            self.c_rel = pole(tau, self.fs);
            let h_ms = 0.5 + (ctx.bass_hold_ms - 0.5) * self.rho;
            self.h_post = ms_to_samples(h_ms.max(0.0), self.fs, 0);
            let first = self.m == 0;
            self.m = ctx.m;
            for lane in self.lanes.iter_mut() {
                lane.set_target(ctx.m, self.h_post);
                if first {
                    lane.snap();
                }
            }
        }

        let gs = self.slow.tick(ctx, l, r, [p0, p1], ms, ce_db, ce, pr);
        let req = requirement(ms, [p0, p1], gs, ce_db, ce, ctx.knee, ctx.lt);
        let mut g = [0.0; 2];
        for c in 0..2 {
            let lane = &mut self.lanes[c];
            let h = lane.hold_release(req[c], self.c_rel, ctx.join_db);
            let s = lane.smooth(h);
            pr.fast = pr.fast.min(s);
            let d = lane.d();
            let gsd = self.gs_delay[c].push(gs[c], d);
            let mut t = lane.delay(s + gsd);
            if t.abs() < 1e-9 {
                t = 0.0;
            }
            g[c] = t.min(0.0);
        }
        pr.slow = pr.slow.min(gs[0].min(gs[1]));
        pr.ch(g);
        pr.crest = self.crest.crest_db;
        pr.rel_ms = self.timing.tau;
        pr.post_ms = self.h_post as f64 * 1000.0 / self.fs;

        let gl = [gain(g[0]), gain(g[1])];
        let mut b0 = [0.0; MAX_U];
        let mut b1 = [0.0; MAX_U];
        self.osp.fetch(self.d_fix, &mut b0, &mut b1);
        let (mut r0, mut r1) = (b0, b1);
        let u = self.osp.u;
        let inv = 1.0 / u as f64;
        for i in 0..u {
            let t = (i + 1) as f64 * inv;
            b0[i] *= self.gprev[0] + (gl[0] - self.gprev[0]) * t;
            b1[i] *= self.gprev[1] + (gl[1] - self.gprev[1]) * t;
        }
        self.gprev = gl;
        let (y0, y1) = self.osp.down(&mut b0, &mut b1);
        let (yl, yr) = decode(ms, y0, y1);
        let (rl, rr) = if ctx.need_ref {
            let (z0, z1) = self.osp.down_ref(&mut r0, &mut r1);
            decode(ms, z0, z1)
        } else {
            (0.0, 0.0)
        };
        let gt = self.trace.push(g[0].min(g[1]), OS_DELAY / 2);
        Frame { l: yl, r: yr, rl, rr, g: gt }
    }
}

/// The DualSone-style slow macro engine of Pure: a gated loudness tracker that either takes
/// `bodyShare` of the peak excess (Peak Body) or levels the loudness to `C - crestHeadroom`
/// with a 10 dB soft knee (Loudness Crest).
pub struct SlowMacro {
    fs: f64,
    kw: [KWeighting; 2],
    act: MovingMean,
    lt: f64,
    lt_known: bool,
    lt_cnt: usize,
    lt_need: usize,
    k_lt: f64,
    active: bool,
    penv: [SlidingMax; 2],
    w20: usize,
    blk: usize,
    blk_n: usize,
    acc_a: f64,
    acc_ms: f64,
    mean_e: [MovingMean; 2],
    mean_a: MovingMean,
    mean_ms: MovingMean,
    win_s: f64,
    target: [f64; 2],
    g: [f64; 2],
    ca: f64,
    cr: f64,
    pub loud: f64,
}

impl Default for SlowMacro {
    fn default() -> Self {
        Self {
            fs: 48_000.0,
            kw: [KWeighting::default(); 2],
            act: MovingMean::default(),
            lt: -70.0,
            lt_known: false,
            lt_cnt: 0,
            lt_need: 1,
            k_lt: 0.0,
            active: false,
            penv: Default::default(),
            w20: 1,
            blk: 48,
            blk_n: 0,
            acc_a: 0.0,
            acc_ms: 0.0,
            mean_e: Default::default(),
            mean_a: MovingMean::default(),
            mean_ms: MovingMean::default(),
            win_s: 0.0,
            target: [0.0; 2],
            g: [0.0; 2],
            ca: 0.0,
            cr: 0.0,
            loud: -100.0,
        }
    }
}

impl SlowMacro {
    /// Activity window for the pause gate, ms.
    const ACT_MS: f64 = 50.0;
    const LONG_TERM_MS: f64 = 4000.0;
    const RELATIVE_GATE: f64 = 10.0;
    const ABSOLUTE_GATE: f64 = -70.0;

    fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        for k in self.kw.iter_mut() {
            k.prepare(fs);
        }
        let a = ms_to_samples(Self::ACT_MS, fs, 1);
        self.act.alloc(a);
        self.act.set_len(a);
        self.k_lt = pole(Self::LONG_TERM_MS, fs);
        self.lt_need = ms_to_samples(400.0, fs, 1);
        self.w20 = ms_to_samples(20.0, fs, 1);
        for p in self.penv.iter_mut() {
            p.alloc(self.w20);
        }
        self.blk = ms_to_samples(1.0, fs, 1);
        for m in self.mean_e.iter_mut() {
            m.alloc(3000);
        }
        self.mean_a.alloc(3000);
        self.mean_ms.alloc(3000);
        self.clear();
    }

    fn clear(&mut self) {
        for k in self.kw.iter_mut() {
            k.clear();
        }
        self.act.clear();
        self.lt = Self::ABSOLUTE_GATE;
        self.lt_known = false;
        self.lt_cnt = 0;
        self.active = false;
        for p in self.penv.iter_mut() {
            p.clear();
        }
        self.blk_n = 0;
        self.acc_a = 0.0;
        self.acc_ms = 0.0;
        for m in self.mean_e.iter_mut() {
            m.clear();
        }
        self.mean_a.clear();
        self.mean_ms.clear();
        self.win_s = 0.0;
        self.target = [0.0; 2];
        self.g = [0.0; 2];
        self.loud = -100.0;
    }

    #[allow(clippy::too_many_arguments)]
    fn tick(&mut self, ctx: &Ctx, l: f64, r: f64, p: [f64; 2], ms: bool, c_db: f64, c_lin: f64, pr: &mut Probe) -> [f64; 2] {
        if self.win_s != ctx.slow_win_s {
            self.win_s = ctx.slow_win_s;
            let n = ((self.win_s * 1000.0).round() as usize).clamp(1, 3000);
            for m in self.mean_e.iter_mut() {
                m.set_len(n);
            }
            self.mean_a.set_len(n);
            self.mean_ms.set_len(n);
            self.ca = pole(500.0 * self.win_s, self.fs);
            self.cr = pole(3000.0 * self.win_s, self.fs);
        }
        let kl = self.kw[0].run(l);
        let kr = self.kw[1].run(r);
        let (wl, wr) = if ctx.kweight { (kl, kr) } else { (l, r) };
        let x2 = wl * wl + wr * wr;
        let m50 = self.act.push(x2).max(0.0);
        let l50 = -0.691 + 10.0 * (m50 + 1e-20).log10();
        if l50 > Self::ABSOLUTE_GATE {
            if self.lt_known {
                self.lt = l50 + self.k_lt * (self.lt - l50);
            } else {
                self.lt = l50;
                self.lt_cnt += 1;
                self.lt_known = self.lt_cnt >= self.lt_need;
            }
        }
        self.active = l50 > Self::ABSOLUTE_GATE && (!self.lt_known || l50 > self.lt - Self::RELATIVE_GATE);
        let pe = [self.penv[0].push(p[0], self.w20), self.penv[1].push(p[1], self.w20)];
        if self.active {
            self.acc_a += 1.0;
            self.acc_ms += x2;
        }
        self.blk_n += 1;
        if self.blk_n >= self.blk {
            let inv = 1.0 / self.blk as f64;
            let fa = self.acc_a * inv;
            let e = if ms {
                let (gm, gs) = diamond(pe[0], pe[1], c_lin, 0.0);
                [-lin_to_db(gm).min(0.0), -lin_to_db(gs).min(0.0)]
            } else {
                [(lin_to_db(pe[0]) - c_db).max(0.0), (lin_to_db(pe[1]) - c_db).max(0.0)]
            };
            let ma = self.mean_a.push(fa);
            let me = [self.mean_e[0].push(e[0] * fa), self.mean_e[1].push(e[1] * fa)];
            let mms = self.mean_ms.push(self.acc_ms * inv);
            if ctx.crest_mode {
                let lg = if ma > 1e-9 { -0.691 + 10.0 * (mms.max(0.0) / ma + 1e-20).log10() } else { -200.0 };
                self.loud = lg;
                let t = knee_gain_db(lg - (c_db - ctx.crest_hr), 10.0);
                self.target = [t, t];
            } else {
                for c in 0..2 {
                    self.target[c] = if ma > 1e-9 { -ctx.body * (me[c] / ma).max(0.0) } else { 0.0 };
                }
                self.loud = if ma > 1e-9 { -0.691 + 10.0 * (mms.max(0.0) / ma + 1e-20).log10() } else { -200.0 };
            }
            self.blk_n = 0;
            self.acc_a = 0.0;
            self.acc_ms = 0.0;
        }
        let frozen = ctx.pause_hold && !self.active;
        if !frozen {
            for c in 0..2 {
                let t = self.target[c];
                let k = if t < self.g[c] { self.ca } else { self.cr };
                self.g[c] = t + k * (self.g[c] - t);
                if self.g[c].abs() < 1e-10 {
                    self.g[c] = 0.0;
                }
            }
        }
        pr.hold = frozen;
        pr.loud = self.loud;
        let mn = self.g[0].min(self.g[1]);
        let lr = ctx.lr;
        [(1.0 - lr) * self.g[0] + lr * mn, (1.0 - lr) * self.g[1] + lr * mn]
    }

    /// The slow gains before linking (dB).
    pub fn gains(&self) -> [f64; 2] {
        self.g
    }
}

// =============================================================================================
// Smooth (1x) and Precise (oversampled)

/// Engines 1 and 2: cubic B-spline with flat top, crest-adaptive dual-stage release and the
/// split link. `os = false` is Smooth (1x with the 4x reader as detector), `os = true`
/// Precise (inside the halfband oversampler).
pub struct Env {
    os: bool,
    fs: f64,
    d_fix: usize,
    f: usize,
    ms: bool,
    det: Det1x,
    audio: [Delay; 2],
    osp: OsPath,
    os_peak: f64,
    lanes: [Lane; 2],
    slow: [f64; 2],
    crest: Crest,
    timing: Timing,
    c_fast: f64,
    c_slow: f64,
    c_charge: f64,
    m: usize,
    gprev: [f64; 2],
    trace: Delay,
}

impl Env {
    pub fn new(os: bool) -> Self {
        Self {
            os,
            fs: 48_000.0,
            d_fix: 1,
            f: 1,
            ms: false,
            det: Det1x::default(),
            audio: Default::default(),
            osp: OsPath::default(),
            os_peak: 1.0,
            lanes: Default::default(),
            slow: [0.0; 2],
            crest: Crest::default(),
            timing: Timing::default(),
            c_fast: 0.0,
            c_slow: 0.0,
            c_charge: 0.0,
            m: 0,
            gprev: [1.0; 2],
            trace: Delay::default(),
        }
    }

    /// Slow-stage charge time, ms.
    const CHARGE_MS: f64 = 60.0;
    /// Fast stage release relative to the effective release.
    const FAST_SHARE: f64 = 0.3;

    pub fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        self.f = flat_top(fs);
        let osb = usize::from(self.os);
        self.d_fix = m_max(fs) - 1 + self.f + osb;
        for l in self.lanes.iter_mut() {
            l.alloc(self.d_fix, self.f, osb, 0);
        }
        if self.os {
            self.osp.alloc(self.d_fix + 2);
            self.trace.alloc(OS_DELAY);
        } else {
            for a in self.audio.iter_mut() {
                a.alloc(TP_HALF + self.d_fix + 1);
            }
        }
        self.crest.alloc(fs);
        self.det.alloc();
        self.c_charge = pole(Self::CHARGE_MS, fs);
        self.reset();
    }

    pub fn configure(&mut self, u: usize, ms: bool) {
        if self.os {
            self.osp.set_factor(u);
            self.os_peak = passband_peak(self.osp.k, self.fs, 20_000f64.min(0.45 * self.fs));
        }
        self.ms = ms;
    }

    pub fn latency(&self) -> usize {
        if self.os {
            OS_DELAY + self.d_fix
        } else {
            TP_HALF + self.d_fix
        }
    }

    pub fn reset(&mut self) {
        self.det.clear();
        for a in self.audio.iter_mut() {
            a.clear();
        }
        self.osp.clear();
        for l in self.lanes.iter_mut() {
            l.clear();
        }
        self.slow = [0.0; 2];
        self.crest.clear();
        self.timing = Timing::default();
        self.m = 0;
        self.gprev = [1.0; 2];
        self.trace.clear();
    }

    pub fn tick(&mut self, ctx: &Ctx, l: f64, r: f64, pr: &mut Probe) -> Frame {
        let ms = self.ms;
        let (a0, a1) = encode(ms, l, r);
        let p = if self.os {
            let (p0, p1) = self.osp.push(a0, a1);
            [p0, p1]
        } else {
            let quiet = ctx.c_lin * MARGIN * db_to_lin(crate::stages::knee_floor_db(ctx.knee));
            self.det.push(a0, a1, ctx.tp, ctx.refine, if ms { 0.5 * quiet } else { quiet })
        };
        let ce = if self.os { ctx.c_lin * MARGIN / self.os_peak } else { ctx.c_lin * MARGIN };
        let ce_db = lin_to_db(ce);
        let pk = if ms { p[0] + p[1] } else { p[0].max(p[1]) };
        let due = self.timing.due();
        self.crest.push(0.5 * (l * l + r * r), pk, due);
        if due || self.m != ctx.m {
            let tau = self.timing.update(ctx, self.crest.crest_db);
            self.c_fast = pole(Self::FAST_SHARE * tau, self.fs);
            self.c_slow = pole(tau, self.fs);
            let first = self.m == 0;
            self.m = ctx.m;
            for lane in self.lanes.iter_mut() {
                lane.set_target(ctx.m, 0);
                if first {
                    lane.snap();
                }
            }
        }
        let req = requirement(ms, p, [0.0; 2], ce_db, ce, ctx.knee, ctx.lt);
        let mut e = [0.0; 2];
        for c in 0..2 {
            let lane = &mut self.lanes[c];
            e[c] = lane.hold_release(req[c], self.c_fast, 0.0);
            let m = lane.m;
            let s = &mut self.slow[c];
            if m < *s {
                *s = m + self.c_charge * (*s - m);
            } else {
                *s = m + self.c_slow * (*s - m);
                if *s - m > -1e-10 {
                    *s = m;
                }
            }
        }
        let mn = self.slow[0].min(self.slow[1]);
        let lr = ctx.lr;
        let mut g = [0.0; 2];
        for c in 0..2 {
            let sl = (1.0 - lr) * self.slow[c] + lr * mn;
            let v = e[c].min(sl);
            pr.fast = pr.fast.min(e[c]);
            pr.slow = pr.slow.min(sl);
            let lane = &mut self.lanes[c];
            let s = lane.smooth(v);
            g[c] = lane.delay(s).min(0.0);
        }
        pr.ch(g);
        pr.crest = self.crest.crest_db;
        pr.rel_ms = self.timing.tau;
        let gl = [gain(g[0]), gain(g[1])];

        if self.os {
            let mut b0 = [0.0; MAX_U];
            let mut b1 = [0.0; MAX_U];
            self.osp.fetch(self.d_fix, &mut b0, &mut b1);
            let (mut r0, mut r1) = (b0, b1);
            let u = self.osp.u;
            let inv = 1.0 / u as f64;
            for i in 0..u {
                let t = (i + 1) as f64 * inv;
                b0[i] *= self.gprev[0] + (gl[0] - self.gprev[0]) * t;
                b1[i] *= self.gprev[1] + (gl[1] - self.gprev[1]) * t;
            }
            self.gprev = gl;
            let (y0, y1) = self.osp.down(&mut b0, &mut b1);
            let (yl, yr) = decode(ms, y0, y1);
            let (rl, rr) = if ctx.need_ref {
                let (z0, z1) = self.osp.down_ref(&mut r0, &mut r1);
                decode(ms, z0, z1)
            } else {
                (0.0, 0.0)
            };
            let gt = self.trace.push(g[0].min(g[1]), OS_DELAY / 2);
            Frame { l: yl, r: yr, rl, rr, g: gt }
        } else {
            let d = TP_HALF + self.d_fix;
            let x0 = self.audio[0].push(a0, d);
            let x1 = self.audio[1].push(a1, d);
            let (yl, yr) = decode(ms, x0 * gl[0], x1 * gl[1]);
            let (rl, rr) = decode(ms, x0, x1);
            Frame { l: yl, r: yr, rl, rr, g: g[0].min(g[1]) }
        }
    }
}

// =============================================================================================
// Punch

/// Engine 3: slow RMS leveller -> B-spline look-ahead limiter to `C + clipHeadroom` ->
/// (decode M/S) -> oversampled C1 quadratic-knee clipper with first-order ADAA.
pub struct Punch {
    fs: f64,
    d_fix: usize,
    ms: bool,
    osp: OsPath,
    os_peak: f64,
    lanes: [Lane; 2],
    g1_delay: [Delay; 2],
    rms: [MovingMean; 2],
    g1: [f64; 2],
    ka: f64,
    kr: f64,
    crest: Crest,
    timing: Timing,
    c_rel: f64,
    m: usize,
    gprev: [f64; 2],
    uprev: [f64; 2],
    /// The previous decoded sub-sample before the clipper (output units).
    vprev: [f64; 2],
    /// The previous decoded reference sub-sample.
    rprev: [f64; 2],
    /// The ceiling carried along the lookahead delay: the clipper shapes each sub-sample
    /// against the ceiling that came in with it.
    c_line: Delay,
    c_primed: bool,
    trace: Delay,
}

impl Default for Punch {
    fn default() -> Self {
        Self {
            fs: 48_000.0,
            d_fix: 1,
            ms: false,
            osp: OsPath::default(),
            os_peak: 1.0,
            lanes: Default::default(),
            g1_delay: Default::default(),
            rms: Default::default(),
            g1: [0.0; 2],
            ka: 0.0,
            kr: 0.0,
            crest: Crest::default(),
            timing: Timing::default(),
            c_rel: 0.0,
            m: 0,
            gprev: [1.0; 2],
            uprev: [0.0; 2],
            vprev: [0.0; 2],
            rprev: [0.0; 2],
            c_line: Delay::default(),
            c_primed: false,
            trace: Delay::default(),
        }
    }
}

impl Punch {
    const RMS_MS: f64 = 80.0;
    const ATTACK_MS: f64 = 40.0;
    const RELEASE_MS: f64 = 240.0;

    pub fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        self.d_fix = m_max(fs);
        self.osp.alloc(self.d_fix + 2);
        for l in self.lanes.iter_mut() {
            l.alloc(self.d_fix, 0, 1, 0);
        }
        for d in self.g1_delay.iter_mut() {
            d.alloc(self.d_fix + 1);
        }
        let n = ms_to_samples(Self::RMS_MS, fs, 1);
        for m in self.rms.iter_mut() {
            m.alloc(n);
            m.set_len(n);
        }
        self.ka = pole(Self::ATTACK_MS, fs);
        self.kr = pole(Self::RELEASE_MS, fs);
        self.crest.alloc(fs);
        self.c_line.alloc(self.d_fix + 1);
        self.trace.alloc(OS_DELAY);
        self.reset();
    }

    pub fn configure(&mut self, u: usize, ms: bool) {
        self.osp.set_factor(u);
        self.os_peak = passband_peak(self.osp.k, self.fs, 20_000f64.min(0.45 * self.fs));
        self.ms = ms;
    }

    pub fn latency(&self) -> usize {
        OS_DELAY + self.d_fix
    }

    pub fn reset(&mut self) {
        self.osp.clear();
        self.c_primed = false;
        for l in self.lanes.iter_mut() {
            l.clear();
        }
        for d in self.g1_delay.iter_mut() {
            d.clear();
        }
        for m in self.rms.iter_mut() {
            m.clear();
        }
        self.g1 = [0.0; 2];
        self.crest.clear();
        self.timing = Timing::default();
        self.m = 0;
        self.gprev = [1.0; 2];
        self.uprev = [0.0; 2];
        self.vprev = [0.0; 2];
        self.rprev = [0.0; 2];
        self.trace.clear();
    }

    pub fn tick(&mut self, ctx: &Ctx, l: f64, r: f64, pr: &mut Probe) -> Frame {
        let ms = self.ms;
        let (a0, a1) = encode(ms, l, r);
        let (p0, p1) = self.osp.push(a0, a1);
        if !self.c_primed {
            self.c_line.fill(ctx.c_lin);
            self.c_primed = true;
        }
        let c_late = self.c_line.push(ctx.c_lin, self.d_fix);
        let c_lim = ctx.c_lin * MARGIN * db_to_lin(ctx.clip_hr_db) / self.os_peak;
        let c_lim_db = lin_to_db(c_lim);
        let pk = if ms { p0 + p1 } else { p0.max(p1) };
        let due = self.timing.due();
        self.crest.push(0.5 * (l * l + r * r), pk, due);
        if due || self.m != ctx.m {
            let tau = self.timing.update(ctx, self.crest.crest_db);
            self.c_rel = pole(tau, self.fs);
            let first = self.m == 0;
            self.m = ctx.m;
            for lane in self.lanes.iter_mut() {
                lane.set_target(ctx.m, 0);
                if first {
                    lane.snap();
                }
            }
        }
        // Stage 1: RMS leveller on the sine-equivalent peak sqrt(2) * RMS.
        let pe = [
            (2.0 * self.rms[0].push(a0 * a0).max(0.0)).sqrt(),
            (2.0 * self.rms[1].push(a1 * a1).max(0.0)).sqrt(),
        ];
        let e = if ms {
            let (gm, gs) = diamond(pe[0], pe[1], c_lim, 0.0);
            [-lin_to_db(gm).min(0.0), -lin_to_db(gs).min(0.0)]
        } else {
            [(lin_to_db(pe[0]) - c_lim_db).max(0.0), (lin_to_db(pe[1]) - c_lim_db).max(0.0)]
        };
        for c in 0..2 {
            let t = -ctx.body * e[c];
            let k = if t < self.g1[c] { self.ka } else { self.kr };
            self.g1[c] = t + k * (self.g1[c] - t);
            if self.g1[c].abs() < 1e-10 {
                self.g1[c] = 0.0;
            }
        }
        let mn = self.g1[0].min(self.g1[1]);
        let lr = ctx.lr;
        let gs = [(1.0 - lr) * self.g1[0] + lr * mn, (1.0 - lr) * self.g1[1] + lr * mn];
        pr.slow = pr.slow.min(gs[0].min(gs[1]));

        // Stage 2: look-ahead limiter to C_lim.
        let req = requirement(ms, [p0, p1], gs, c_lim_db, c_lim, ctx.knee, ctx.lt);
        let mut g = [0.0; 2];
        for c in 0..2 {
            let lane = &mut self.lanes[c];
            let h = lane.hold_release(req[c], self.c_rel, 0.0);
            let s = lane.smooth(h);
            pr.fast = pr.fast.min(s);
            let d = lane.d();
            let gd = self.g1_delay[c].push(gs[c], d);
            let mut t = lane.delay(s + gd);
            if t.abs() < 1e-9 {
                t = 0.0;
            }
            g[c] = t.min(0.0);
        }
        pr.ch(g);
        pr.crest = self.crest.crest_db;
        pr.rel_ms = self.timing.tau;

        // Stage 3 at Ux: gain, decode, ADAA1 clipper. Where the clipper is linear (both ends
        // of the step within 1 - k) its ADAA1 is exactly the two-point average, computed as
        // such; the dry reference goes through the same average (the same half-sample delay
        // and droop), so the delta nulls exactly below threshold.
        let gl = [gain(g[0]), gain(g[1])];
        let mut b0 = [0.0; MAX_U];
        let mut b1 = [0.0; MAX_U];
        self.osp.fetch(self.d_fix, &mut b0, &mut b1);
        let (mut r0, mut r1) = (b0, b1);
        let u = self.osp.u;
        let inv = 1.0 / u as f64;
        let k = ctx.clip_k.clamp(0.1, 1.0);
        let lin_top = 1.0 - k;
        let c_clip = c_late * MARGIN;
        let inv_c = 1.0 / c_clip;
        let mut deepest = 0.0f64;
        for i in 0..u {
            let t = (i + 1) as f64 * inv;
            let v0 = b0[i] * (self.gprev[0] + (gl[0] - self.gprev[0]) * t);
            let v1 = b1[i] * (self.gprev[1] + (gl[1] - self.gprev[1]) * t);
            let (vl, vr) = decode(ms, v0, v1);
            let ul = vl * inv_c;
            let ur = vr * inv_c;
            b0[i] = if ul.abs() <= lin_top && self.uprev[0].abs() <= lin_top {
                0.5 * (vl + self.vprev[0])
            } else {
                c_clip * clip_adaa1(ul, self.uprev[0], k)
            };
            b1[i] = if ur.abs() <= lin_top && self.uprev[1].abs() <= lin_top {
                0.5 * (vr + self.vprev[1])
            } else {
                c_clip * clip_adaa1(ur, self.uprev[1], k)
            };
            self.uprev = [ul, ur];
            self.vprev = [vl, vr];
            for uu in [ul, ur] {
                let a = uu.abs();
                if a > lin_top {
                    deepest = deepest.min(lin_to_db(clip_curve(a, k) / a));
                }
            }
            if ctx.need_ref {
                let (zl, zr) = decode(ms, r0[i], r1[i]);
                r0[i] = 0.5 * (zl + self.rprev[0]);
                r1[i] = 0.5 * (zr + self.rprev[1]);
                self.rprev = [zl, zr];
            }
        }
        if !(self.uprev[0].is_finite() && self.uprev[1].is_finite() && self.vprev[0].is_finite() && self.vprev[1].is_finite()) {
            self.uprev = [0.0; 2];
            self.vprev = [0.0; 2];
        }
        if !(self.rprev[0].is_finite() && self.rprev[1].is_finite()) {
            self.rprev = [0.0; 2];
        }
        pr.clip = pr.clip.min(deepest);
        self.gprev = gl;
        let (yl, yr) = self.osp.down(&mut b0, &mut b1);
        let (rl, rr) = if ctx.need_ref { self.osp.down_ref(&mut r0, &mut r1) } else { (0.0, 0.0) };
        let gt = self.trace.push(g[0].min(g[1]) + deepest, OS_DELAY / 2);
        Frame { l: yl, r: yr, rl, rr, g: gt }
    }
}

// =============================================================================================
// Multiband

#[derive(Default, Clone, Copy)]
struct Lr4 {
    lp1: [crate::dsp::Biquad; 2],
    hp1: [crate::dsp::Biquad; 2],
    lp2: [crate::dsp::Biquad; 2],
    hp2: [crate::dsp::Biquad; 2],
    lp3: [crate::dsp::Biquad; 2],
    hp3: [crate::dsp::Biquad; 2],
    ap2: crate::dsp::Biquad,
    ap3a: crate::dsp::Biquad,
    ap3b: crate::dsp::Biquad,
}

#[inline]
fn two(b: &mut [crate::dsp::Biquad; 2], x: f64) -> f64 {
    let y = b[0].run(x);
    b[1].run(y)
}

impl Lr4 {
    fn design(&mut self, s: [f64; 3], fs: f64) {
        use crate::dsp::BqKind::{All, High, Low};
        let q = std::f64::consts::FRAC_1_SQRT_2;
        for b in self.lp1.iter_mut() {
            b.design(Low, s[0], q, fs);
        }
        for b in self.hp1.iter_mut() {
            b.design(High, s[0], q, fs);
        }
        for b in self.lp2.iter_mut() {
            b.design(Low, s[1], q, fs);
        }
        for b in self.hp2.iter_mut() {
            b.design(High, s[1], q, fs);
        }
        for b in self.lp3.iter_mut() {
            b.design(Low, s[2], q, fs);
        }
        for b in self.hp3.iter_mut() {
            b.design(High, s[2], q, fs);
        }
        self.ap2.design(All, s[1], q, fs);
        self.ap3a.design(All, s[2], q, fs);
        self.ap3b.design(All, s[2], q, fs);
    }
    fn clear(&mut self) {
        for b in self
            .lp1
            .iter_mut()
            .chain(self.hp1.iter_mut())
            .chain(self.lp2.iter_mut())
            .chain(self.hp2.iter_mut())
            .chain(self.lp3.iter_mut())
            .chain(self.hp3.iter_mut())
        {
            b.clear();
        }
        self.ap2.clear();
        self.ap3a.clear();
        self.ap3b.clear();
    }
    #[inline]
    fn run(&mut self, x: f64) -> [f64; 4] {
        let l1 = two(&mut self.lp1, x);
        let h1 = two(&mut self.hp1, x);
        let b0 = self.ap3a.run(self.ap2.run(l1));
        let lo2 = two(&mut self.lp2, h1);
        let h2 = two(&mut self.hp2, h1);
        let b1 = self.ap3b.run(lo2);
        let b2 = two(&mut self.lp3, h2);
        let b3 = two(&mut self.hp3, h2);
        [b0, b1, b2, b3]
    }
}

/// The 4-band crossover: LR4 (all-pass aligned, `|sum| = 1`) or linear phase
/// (`sum = z^-dx` exactly). The LR4 path is delayed by `dx` so both modes have the same delay.
///
/// The linear-phase lowpasses run in `f32`: full symmetric kernels (zero-padded at the old
/// end to a multiple of 4) over a contiguous mirrored history, three kernels in one pass with
/// four independent accumulators each. The bands are differences of the lowpasses and the
/// top band is the exact `f64` centre sample minus the last lowpass, so the bands still sum
/// to the delayed input exactly.
pub struct Xover {
    fs: f64,
    dx: usize,
    lin: bool,
    splits: [f64; 3],
    lr: [Lr4; 2],
    pre: [Delay; 2],
    kern: [Vec<f32>; 3],
    /// Kernel length: `2 dx + 1` rounded up to a multiple of 4.
    klen: usize,
    hist: [Vec<f32>; 2],
    hmask: usize,
    hlen: usize,
    pos: usize,
}

impl Default for Xover {
    fn default() -> Self {
        Self {
            fs: 48_000.0,
            dx: 128,
            lin: false,
            splits: [0.0; 3],
            lr: [Lr4::default(); 2],
            pre: Default::default(),
            kern: [Vec::new(), Vec::new(), Vec::new()],
            klen: 0,
            hist: [Vec::new(), Vec::new()],
            hmask: 0,
            hlen: 0,
            pos: 0,
        }
    }
}

/// Splits kept in order and below 0.45 fs: `s2 >= 1.25 s1`, `s3 >= 1.25 s2`.
pub fn ordered_splits(s: [f64; 3], fs: f64) -> [f64; 3] {
    let top = 0.45 * fs;
    let s1 = s[0].clamp(20.0, top / 1.6);
    let s2 = s[1].max(1.25 * s1).min(top / 1.25);
    let s3 = s[2].max(1.25 * s2).min(top);
    [s1, s2, s3]
}

impl Xover {
    fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        self.dx = crossover_delay(fs);
        self.klen = (2 * self.dx + 1).div_ceil(4) * 4;
        for k in self.kern.iter_mut() {
            *k = vec![0.0; self.klen];
        }
        let n = (2 * self.dx + 1).next_power_of_two().max(self.klen);
        self.hlen = n;
        self.hmask = n - 1;
        self.hist = [vec![0.0; 2 * n], vec![0.0; 2 * n]];
        for p in self.pre.iter_mut() {
            p.alloc(self.dx);
        }
        self.splits = [0.0; 3];
        self.clear();
    }

    fn clear(&mut self) {
        for l in self.lr.iter_mut() {
            l.clear();
        }
        for p in self.pre.iter_mut() {
            p.clear();
        }
        for h in self.hist.iter_mut() {
            h.iter_mut().for_each(|v| *v = 0.0);
        }
        self.pos = 0;
    }

    fn set_splits(&mut self, s: [f64; 3]) {
        let s = ordered_splits(s, self.fs);
        if s == self.splits {
            return;
        }
        self.splits = s;
        for l in self.lr.iter_mut() {
            l.design(s, self.fs);
        }
        let dx = self.dx;
        let pad = self.klen.saturating_sub(2 * dx + 1);
        for (k, kern) in self.kern.iter_mut().enumerate() {
            // half kernel h[j], j = 0..=dx, offset t = j - dx (h[dx] is the centre), in f64
            let fc = s[k] / self.fs;
            let mut half = [0.0f64; 2049];
            let mut sum = 0.0;
            for j in 0..=dx.min(2048) {
                let t = j as f64 - dx as f64;
                let v = 2.0 * fc * crate::dsp::sinc(2.0 * fc * t) * crate::dsp::kaiser(t / (dx as f64 + 1.0), 6.0);
                half[j] = v;
                sum += if j == dx { v } else { 2.0 * v };
            }
            // full symmetric kernel over the window, oldest first, after `pad` zeros
            for (i, slot) in kern.iter_mut().enumerate() {
                *slot = if i < pad {
                    0.0
                } else {
                    let t = (i - pad) as i64 - dx as i64;
                    let j = dx - (t.unsigned_abs() as usize).min(dx);
                    (half[j] / sum) as f32
                };
            }
        }
    }

    #[inline]
    fn run(&mut self, x: [f64; 2]) -> [[f64; 4]; 2] {
        let mut out = [[0.0; 4]; 2];
        if self.lin {
            if self.hist[0].is_empty() || self.kern[2].len() != self.klen {
                return out;
            }
            self.pos = (self.pos + 1) & self.hmask;
            let n = self.hlen;
            let kl = self.klen;
            for c in 0..2 {
                let h = &mut self.hist[c];
                let v = x[c] as f32;
                h[self.pos] = v;
                h[self.pos + n] = v;
                // the newest `kl` samples, oldest first (newest at h[pos + n])
                let e = self.pos + n + 1;
                let w = &h[e - kl..e];
                let centre = self.pre[c].push(x[c], self.dx);
                let mut acc = [[0.0f32; 4]; 3];
                for (wc, ((k0, k1), k2)) in
                    w.chunks_exact(4).zip(self.kern[0].chunks_exact(4).zip(self.kern[1].chunks_exact(4)).zip(self.kern[2].chunks_exact(4)))
                {
                    for i in 0..4 {
                        acc[0][i] += k0[i] * wc[i];
                        acc[1][i] += k1[i] * wc[i];
                        acc[2][i] += k2[i] * wc[i];
                    }
                }
                let y = acc.map(|a| ((a[0] + a[1]) + (a[2] + a[3])) as f64);
                out[c] = [y[0], y[1] - y[0], y[2] - y[1], centre - y[2]];
            }
        } else {
            for c in 0..2 {
                let xd = self.pre[c].push(x[c], self.dx);
                out[c] = self.lr[c].run(xd);
            }
        }
        out
    }
}

/// Engine 4: 4-band directional Lagrange de-pumping with a broadband safety catcher.
pub struct Multi {
    fs: f64,
    f: usize,
    d_fix_b: usize,
    d_fix_c: usize,
    mc: usize,
    ms: bool,
    xo: Xover,
    lanes: [[Lane; 2]; 4],
    slow: [[f64; 2]; 4],
    bands: [[Delay; 2]; 4],
    cat_det: Det1x,
    cat_lane: [Lane; 2],
    cat_audio: [Delay; 2],
    cat_ref: [Delay; 2],
    crest: Crest,
    timing: Timing,
    c_fast: [f64; 4],
    c_slow: [f64; 4],
    c_charge: f64,
    c_cat: f64,
    m: usize,
    h_post: [usize; 4],
    /// The ceiling carried along the band delay: the catcher checks the band sum against the
    /// ceiling that came in with it (a ceiling glide must not meet audio already limited).
    c_line: Delay,
    c_primed: bool,
}

impl Default for Multi {
    fn default() -> Self {
        Self {
            fs: 48_000.0,
            f: 1,
            d_fix_b: 1,
            d_fix_c: 1,
            mc: 2,
            ms: false,
            xo: Xover::default(),
            lanes: Default::default(),
            slow: [[0.0; 2]; 4],
            bands: Default::default(),
            cat_det: Det1x::default(),
            cat_lane: Default::default(),
            cat_audio: Default::default(),
            cat_ref: Default::default(),
            crest: Crest::default(),
            timing: Timing::default(),
            c_fast: [0.0; 4],
            c_slow: [0.0; 4],
            c_charge: 0.0,
            c_cat: 0.0,
            m: 0,
            h_post: [0; 4],
            c_line: Delay::default(),
            c_primed: false,
        }
    }
}

impl Multi {
    /// Post-hold per band, ms (low bands hold longer).
    /// Band 0 holds 12 ms: a full half-cycle of 50 Hz.
    const POST_MS: [f64; 4] = [12.0, 2.0, 0.5, 0.0];
    /// Release scale per band.
    const REL_SCALE: [f64; 4] = [2.0, 1.0, 0.7, 0.5];
    const CATCH_RELEASE_MS: f64 = 20.0;

    pub fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        self.f = flat_top(fs);
        self.d_fix_b = m_max(fs) - 1 + self.f;
        self.mc = catcher_lookahead(fs);
        self.d_fix_c = self.mc - 1 + self.f;
        self.xo.alloc(fs);
        for b in 0..4 {
            self.h_post[b] = ms_to_samples(Self::POST_MS[b], fs, 0);
            for c in 0..2 {
                self.lanes[b][c].alloc(self.d_fix_b, self.f, 0, self.h_post[b]);
                self.bands[b][c].alloc(self.d_fix_b + 1);
            }
        }
        for c in 0..2 {
            self.cat_lane[c].alloc(self.d_fix_c, self.f, 0, 0);
            self.cat_audio[c].alloc(TP_HALF + self.d_fix_c + 1);
            self.cat_ref[c].alloc(TP_HALF + self.d_fix_c + 1);
        }
        self.crest.alloc(fs);
        self.cat_det.alloc();
        self.c_line.alloc(self.xo.dx + self.d_fix_b + 1);
        self.c_charge = pole(Env::CHARGE_MS, fs);
        self.c_cat = pole(Self::CATCH_RELEASE_MS, fs);
        self.reset();
    }

    pub fn configure(&mut self, lin: bool, ms: bool) {
        self.xo.lin = lin;
        self.ms = ms;
    }

    pub fn latency(&self) -> usize {
        self.xo.dx + self.d_fix_b + TP_HALF + self.d_fix_c
    }

    pub fn reset(&mut self) {
        self.xo.clear();
        for b in 0..4 {
            for c in 0..2 {
                self.lanes[b][c].clear();
                self.bands[b][c].clear();
            }
        }
        self.slow = [[0.0; 2]; 4];
        self.cat_det.clear();
        for c in 0..2 {
            self.cat_lane[c].clear();
            self.cat_lane[c].set_target(self.mc, 0);
            self.cat_lane[c].snap();
            self.cat_audio[c].clear();
            self.cat_ref[c].clear();
        }
        self.crest.clear();
        self.timing = Timing::default();
        self.m = 0;
        self.c_primed = false;
    }

    pub fn tick(&mut self, ctx: &Ctx, l: f64, r: f64, pr: &mut Probe) -> Frame {
        let ms = self.ms;
        if !self.c_primed {
            self.c_line.fill(ctx.c_lin);
            self.c_primed = true;
        }
        let c_band = self.c_line.push(ctx.c_lin, self.xo.dx + self.d_fix_b);
        self.xo.set_splits(ctx.splits);
        let (a0, a1) = encode(ms, l, r);
        let x = self.xo.run([a0, a1]);
        let ce = ctx.c_lin * MARGIN;
        let ce_db = lin_to_db(ce);
        let due = self.timing.due();
        let pk = (l.abs()).max(r.abs());
        self.crest.push(0.5 * (l * l + r * r), pk, due);
        if due || self.m != ctx.m {
            let tau = self.timing.update(ctx, self.crest.crest_db);
            for b in 0..4 {
                let t = tau * Self::REL_SCALE[b];
                self.c_fast[b] = pole(Env::FAST_SHARE * t, self.fs);
                self.c_slow[b] = pole(t, self.fs);
            }
            let first = self.m == 0;
            self.m = ctx.m;
            for b in 0..4 {
                for lane in self.lanes[b].iter_mut() {
                    lane.set_target(ctx.m, self.h_post[b]);
                    if first {
                        lane.snap();
                    }
                }
            }
        }
        // Allocation in the L/R domain.
        let mut lb = [0.0; 4];
        let mut rb = [0.0; 4];
        for b in 0..4 {
            let (u, v) = decode(ms, x[0][b], x[1][b]);
            lb[b] = u;
            rb[b] = v;
        }
        let gl = band_allocation(lb, ce_db, ce, ctx.knee, ctx.focus);
        let gr = band_allocation(rb, ce_db, ce, ctx.knee, ctx.focus);
        let mut g = [[1.0f64; 2]; 4];
        let lt = ctx.lt;
        for b in 0..4 {
            if ms {
                let gb = gl[b].min(gr[b]);
                if b == 0 || gb >= 1.0 {
                    g[b] = [gb, gb];
                } else {
                    let (am, as_) = (x[0][b].abs(), x[1][b].abs());
                    let (gm, gs) = diamond(am, as_, gb * (am + as_), lt);
                    g[b] = [gm, gs];
                }
            } else {
                let lk = gl[b].min(gr[b]);
                g[b] = [(1.0 - lt) * gl[b] + lt * lk, (1.0 - lt) * gr[b] + lt * lk];
            }
        }
        let mut gd = [[0.0f64; 2]; 4];
        for b in 0..4 {
            let mut e = [0.0; 2];
            for c in 0..2 {
                let req = if g[b][c] >= 1.0 { 0.0 } else { lin_to_db(g[b][c]).min(0.0) };
                let lane = &mut self.lanes[b][c];
                e[c] = lane.hold_release(req, self.c_fast[b], 0.0);
                let m = lane.m;
                let s = &mut self.slow[b][c];
                if m < *s {
                    *s = m + self.c_charge * (*s - m);
                } else {
                    *s = m + self.c_slow[b] * (*s - m);
                    if *s - m > -1e-10 {
                        *s = m;
                    }
                }
            }
            let mn = self.slow[b][0].min(self.slow[b][1]);
            for c in 0..2 {
                let sl = (1.0 - ctx.lr) * self.slow[b][c] + ctx.lr * mn;
                let lane = &mut self.lanes[b][c];
                let s = lane.smooth(e[c].min(sl));
                gd[b][c] = lane.delay(s).min(0.0);
            }
            pr.band[b] = pr.band[b].min(gd[b][0].min(gd[b][1]));
        }
        let mut y = [0.0; 2];
        let mut z = [0.0; 2];
        for c in 0..2 {
            for b in 0..4 {
                let v = self.bands[b][c].push(x[c][b], self.d_fix_b);
                z[c] += v;
                y[c] += if gd[b][c] == 0.0 { v } else { v * db_to_lin(gd[b][c]) };
            }
        }
        let (yl, yr) = decode(ms, y[0], y[1]);
        let (zl, zr) = decode(ms, z[0], z[1]);
        let chg = [gd.iter().map(|v| v[0]).fold(0.0, f64::min), gd.iter().map(|v| v[1]).fold(0.0, f64::min)];
        // Broadband safety catcher, against the ceiling that came in with this audio.
        let (cc, cc_db) = if c_band == ctx.c_lin { (ce, ce_db) } else { (c_band * MARGIN, lin_to_db(c_band * MARGIN)) };
        let pc = self.cat_det.push(yl, yr, ctx.tp, ctx.refine, cc);
        let reqc = requirement(false, pc, [0.0; 2], cc_db, cc, 0.0, lt);
        let mut gc = [0.0; 2];
        for c in 0..2 {
            let lane = &mut self.cat_lane[c];
            let h = lane.hold_release(reqc[c], self.c_cat, 0.0);
            let s = lane.smooth(h);
            gc[c] = lane.delay(s).min(0.0);
        }
        pr.catcher = pr.catcher.min(gc[0].min(gc[1]));
        let d = TP_HALF + self.d_fix_c;
        let ol = self.cat_audio[0].push(yl, d);
        let or = self.cat_audio[1].push(yr, d);
        let rl = self.cat_ref[0].push(zl, d);
        let rr = self.cat_ref[1].push(zr, d);
        pr.ch([chg[0] + gc[0], chg[1] + gc[1]]);
        pr.crest = self.crest.crest_db;
        pr.rel_ms = self.timing.tau;
        Frame { l: ol * gain(gc[0]), r: or * gain(gc[1]), rl, rr, g: chg[0].min(chg[1]) + gc[0].min(gc[1]) }
    }
}

// =============================================================================================
// Glide

/// Engine 5: streaming lower-convex-hull valley bridging.
pub struct Glide {
    fs: f64,
    m_max: usize,
    dr: usize,
    mr: usize,
    att_max: usize,
    ms: bool,
    det: Det1x,
    audio: [Delay; 2],
    req: [Vec<f64>; 2],
    rmask: usize,
    n: usize,
    full: [Hull; 2],
    att: [Hull; 2],
    next_full: usize,
    next_att: usize,
    neg_full: [usize; 2],
    neg_att: [usize; 2],
    g: [f64; 2],
    rounder: [Lane; 2],
    l_cur: usize,
    crest: Crest,
    timing: Timing,
    c_rel: f64,
    started: bool,
}

impl Default for Glide {
    fn default() -> Self {
        Self {
            fs: 48_000.0,
            m_max: 1,
            dr: 1,
            mr: 2,
            att_max: 1,
            ms: false,
            det: Det1x::default(),
            audio: Default::default(),
            req: [Vec::new(), Vec::new()],
            rmask: 0,
            n: 0,
            full: Default::default(),
            att: Default::default(),
            next_full: 0,
            next_att: 0,
            neg_full: [0; 2],
            neg_att: [0; 2],
            g: [0.0; 2],
            rounder: Default::default(),
            l_cur: 1,
            crest: Crest::default(),
            timing: Timing::default(),
            c_rel: 0.0,
            started: false,
        }
    }
}

impl Glide {
    /// Longest attack horizon, ms (including the vertex rounder's lead).
    const ATTACK_MS: f64 = 2.5;

    pub fn alloc(&mut self, fs: f64) {
        self.fs = fs;
        self.m_max = m_max(fs);
        self.mr = rounder_len(fs);
        self.dr = self.mr - 1;
        self.att_max = ms_to_samples(Self::ATTACK_MS, fs, 1).saturating_sub(self.dr).max(1);
        for a in self.audio.iter_mut() {
            a.alloc(TP_HALF + self.m_max + self.dr + 1);
        }
        let n = (self.m_max + 4).next_power_of_two();
        self.req = [vec![0.0; n], vec![0.0; n]];
        self.rmask = n - 1;
        for h in self.full.iter_mut().chain(self.att.iter_mut()) {
            h.alloc(self.m_max + 2);
        }
        for r in self.rounder.iter_mut() {
            r.alloc(self.dr, 0, 0, 0);
        }
        self.det.alloc();
        self.crest.alloc(fs);
        self.reset();
    }

    pub fn configure(&mut self, ms: bool) {
        self.ms = ms;
    }

    pub fn latency(&self) -> usize {
        TP_HALF + self.m_max + self.dr
    }

    pub fn reset(&mut self) {
        self.det.clear();
        for a in self.audio.iter_mut() {
            a.clear();
        }
        for q in self.req.iter_mut() {
            q.iter_mut().for_each(|v| *v = 0.0);
        }
        self.n = 0;
        for h in self.full.iter_mut().chain(self.att.iter_mut()) {
            h.clear();
        }
        self.next_full = 0;
        self.next_att = 0;
        self.neg_full = [0; 2];
        self.neg_att = [0; 2];
        self.g = [0.0; 2];
        for r in self.rounder.iter_mut() {
            r.clear();
            r.set_target(self.mr, 0);
            r.snap();
        }
        self.crest.clear();
        self.timing = Timing::default();
        self.started = false;
    }

    /// Keeps `hull` holding exactly the points `t ..= hi` that exist; `neg` counts the points
    /// below 0 dB in each.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn window(hull: &mut [Hull; 2], neg: &mut [usize; 2], next: &mut usize, req: &[Vec<f64>; 2], mask: usize, t: usize, hi: usize, newest: usize) {
        for c in 0..2 {
            let h = &mut hull[c];
            while let Some(x) = h.oldest() {
                let xi = x as usize;
                if xi < t {
                    if req[c][xi & mask] < 0.0 {
                        neg[c] = neg[c].saturating_sub(1);
                    }
                    h.pop();
                } else {
                    break;
                }
            }
        }
        if *next < t {
            *next = t;
        }
        while *next <= hi && *next <= newest {
            let i = *next & mask;
            for c in 0..2 {
                let v = req[c][i];
                if v < 0.0 {
                    neg[c] += 1;
                }
                hull[c].push(*next as f64, v);
            }
            *next += 1;
        }
    }

    pub fn tick(&mut self, ctx: &Ctx, l: f64, r: f64, pr: &mut Probe) -> Frame {
        let ms = self.ms;
        let (a0, a1) = encode(ms, l, r);
        let ce = ctx.c_lin * MARGIN;
        let quiet = ce * db_to_lin(crate::stages::knee_floor_db(ctx.knee));
        let p = self.det.push(a0, a1, ctx.tp, ctx.refine, if ms { 0.5 * quiet } else { quiet });
        let ce_db = lin_to_db(ce);
        let pk = if ms { p[0] + p[1] } else { p[0].max(p[1]) };
        let due = self.timing.due();
        self.crest.push(0.5 * (l * l + r * r), pk, due);
        let l_t = ctx.m.clamp(1, self.m_max);
        if !self.started {
            self.l_cur = l_t;
            self.started = true;
        }
        if due {
            let tau = self.timing.update(ctx, self.crest.crest_db);
            self.c_rel = pole(tau, self.fs);
        }
        if (self.n & 7) == 0 {
            if self.l_cur < l_t {
                self.l_cur += 1;
            } else if self.l_cur > l_t {
                self.l_cur -= 1;
            }
        }
        let la = self.l_cur.min(self.att_max);
        let req = requirement(ms, p, [0.0; 2], ce_db, ce, ctx.knee, ctx.lt);
        let i = self.n & self.rmask;
        self.req[0][i] = req[0];
        self.req[1][i] = req[1];
        let newest = self.n;
        self.n += 1;
        if newest >= self.m_max {
            let t = newest - self.m_max;
            Self::window(&mut self.full, &mut self.neg_full, &mut self.next_full, &self.req, self.rmask, t, t + self.l_cur, newest);
            Self::window(&mut self.att, &mut self.neg_att, &mut self.next_att, &self.req, self.rmask, t, t + la, newest);
            let px = t as f64 - 1.0;
            let mut cand = [0.0; 2];
            for c in 0..2 {
                let g = self.g[c];
                if g == 0.0 && self.neg_full[c] == 0 {
                    continue; // nothing ahead needs reduction: the gain stays at 0 dB
                }
                // with no point below 0 dB in a window its tangent cannot be negative
                let s_att = if self.neg_att[c] == 0 { f64::INFINITY } else { self.att[c].min_slope(px, g) };
                let s_full = if self.neg_full[c] == 0 { f64::INFINITY } else { self.full[c].min_slope(px, g) };
                let rel = -g * (1.0 - self.c_rel);
                let slope = if s_att < 0.0 { s_att } else { s_full.clamp(0.0, rel.max(0.0)) };
                let mut v = (g + slope).min(0.0);
                if !v.is_finite() {
                    v = 0.0;
                }
                if v.abs() < 1e-10 {
                    v = 0.0;
                }
                cand[c] = v;
            }
            // release link: a rising channel rises no faster than the link allows
            let lr = ctx.lr;
            let mut out = cand;
            for c in 0..2 {
                if cand[c] > self.g[c] {
                    let o = cand[1 - c];
                    let lim = (1.0 - lr) * cand[c] + lr * o.max(self.g[c]);
                    out[c] = cand[c].min(lim.max(self.g[c]));
                }
            }
            self.g = out;
            pr.fast = pr.fast.min(out[0].min(out[1]));
        }
        let mut gr = [0.0; 2];
        for c in 0..2 {
            let lane = &mut self.rounder[c];
            let h = lane.hold_release(self.g[c], 0.0, 0.0);
            let s = lane.smooth(h);
            gr[c] = lane.delay(s).min(0.0);
        }
        pr.ch(gr);
        pr.crest = self.crest.crest_db;
        pr.rel_ms = self.timing.tau;
        let d = TP_HALF + self.m_max + self.dr;
        let x0 = self.audio[0].push(a0, d);
        let x1 = self.audio[1].push(a1, d);
        let (yl, yr) = decode(ms, x0 * gain(gr[0]), x1 * gain(gr[1]));
        let (rl, rr) = decode(ms, x0, x1);
        Frame { l: yl, r: yr, rl, rr, g: gr[0].min(gr[1]) }
    }
}
