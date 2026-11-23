//! 1x / 2x / 4x / 8x oversampling with minimum-phase (polyphase IIR) or linear-phase (FIR)
//! half-band stages, and a matched dry path.
//!
//! Replaces `DSP/Resampler.h`, whose filters come from the original's framework. The structure
//! and specification are the same: cascaded 2x stages, stage n designed with transition width
//! 0.05 / 0.06 (up / down) for the first stage and 0.10 / 0.12 after it (normalised to the
//! stage's high rate), stop-band -90 / -75 dB for the first stage and 10 dB less per later
//! stage. The minimum-phase stages are the classic two-path all-pass half-band (elliptic
//! design, closed form). The linear-phase stages are Kaiser-windowed half-band FIRs (the
//! original uses an equiripple design), so their coefficients differ from the original's.
//!
//! The dry signals (Mix's dry and Delta's untouched input) each run through their own copy of the
//! same up/down chain with nothing in between, so they match the wet path in magnitude and phase,
//! not only in nominal delay.

use std::f64::consts::PI;

/// Oversampling factor choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsMode {
    X1 = 0,
    X2 = 1,
    X4 = 2,
    X8 = 3,
}

impl OsMode {
    pub fn from_index(i: i32) -> Self {
        match i.clamp(0, 3) {
            0 => OsMode::X1,
            1 => OsMode::X2,
            2 => OsMode::X4,
            _ => OsMode::X8,
        }
    }
    pub fn factor(self) -> usize {
        1 << (self as usize)
    }
    pub fn stages(self) -> usize {
        self as usize
    }
}

/// Resampling filter choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsFilter {
    MinPhase = 0,
    LinPhase = 1,
}

impl OsFilter {
    pub fn from_index(i: i32) -> Self {
        if i >= 1 {
            OsFilter::LinPhase
        } else {
            OsFilter::MinPhase
        }
    }
}

/// Stage `n` (0-based) design: (up transition, up stop-band dB, down transition, down stop-band dB).
pub fn stage_spec(n: usize) -> (f64, f64, f64, f64) {
    let first = if n == 0 { 0.5 } else { 1.0 };
    (0.10 * first, -90.0 + 10.0 * n as f64, 0.12 * first, -75.0 + 10.0 * n as f64)
}

// ---------------------------------------------------------------------------------------------
// Design
// ---------------------------------------------------------------------------------------------

/// Two-path all-pass half-band design. Returns the coefficients of the first-order sections
/// (in z^-2 at the high rate), alternately for the direct and the delayed path.
pub fn design_iir_halfband(transition: f64, stop_db: f64) -> Vec<f64> {
    let wt = 2.0 * PI * transition;
    let ds = 10.0f64.powf(stop_db / 20.0);
    let k = ((PI - wt) / 4.0).tan().powi(2);
    let kp = (1.0 - k * k).sqrt();
    let e = (1.0 - kp.sqrt()) / (1.0 + kp.sqrt()) * 0.5;
    let q = e + 2.0 * e.powi(5) + 15.0 * e.powi(9) + 150.0 * e.powi(13);
    let k1 = ds * ds / (1.0 - ds * ds);
    let mut n = ((k1 * k1 / 16.0).ln() / q.ln()).ceil() as i32;
    if n % 2 == 0 {
        n += 1;
    }
    if n == 1 {
        n = 3;
    }
    let nn = (n - 1) / 2;
    let mut out = Vec::with_capacity(nn as usize);
    for i in 1..=nn {
        let mut num = 0.0;
        let mut m = 0i32;
        loop {
            let d = (-1.0f64).powi(m)
                * q.powi(m * (m + 1))
                * ((2 * m + 1) as f64 * PI * i as f64 / n as f64).sin();
            num += d;
            m += 1;
            if d.abs() <= 1e-100 || m > 200 {
                break;
            }
        }
        num *= 2.0 * q.powf(0.25);
        let mut den = 0.0;
        let mut m = 1i32;
        loop {
            let d = (-1.0f64).powi(m) * q.powi(m * m) * (m as f64 * 2.0 * PI * i as f64 / n as f64).cos();
            den += d;
            m += 1;
            if d.abs() <= 1e-100 || m > 200 {
                break;
            }
        }
        den = 1.0 + 2.0 * den;
        let wi = num / den;
        let api = ((1.0 - wi * wi * k) * (1.0 - wi * wi / k)).sqrt() / (1.0 + wi * wi);
        out.push((1.0 - api) / (1.0 + api));
    }
    out
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let h = x * 0.5;
    for k in 1..200 {
        term *= (h / k as f64) * (h / k as f64);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Kaiser-windowed half-band FIR (cut-off at a quarter of the high rate). The length is the
/// smallest 4k+3 (end taps non-zero, centre on an odd index) the Kaiser estimate allows; these are
/// the lengths that give the original's linear-phase latencies at 2x (49 samples) and close to
/// them at 4x and 8x. At this length the Kaiser design falls short of `stop_db` at the band edge
/// by up to about 2 dB (an equiripple design of the same length would not). Every second tap from
/// the centre is exactly zero.
pub fn design_fir_halfband(transition: f64, stop_db: f64) -> Vec<f64> {
    let a = -stop_db;
    let dw = 2.0 * PI * transition;
    let est = ((a - 7.95) / (2.285 * dw)).ceil() as usize + 1;
    let mut n = 3;
    while n < est {
        n += 4;
    }
    kaiser_halfband(n, a)
}

/// Designs of stage `n`, built once: (IIR up, IIR down, FIR up, FIR down).
fn stage_designs(n: usize) -> &'static StageDesign {
    static D: std::sync::OnceLock<Vec<StageDesign>> = std::sync::OnceLock::new();
    &D.get_or_init(|| {
        (0..3)
            .map(|n| {
                let (tu, au, td, ad) = stage_spec(n);
                StageDesign {
                    iir_up: design_iir_halfband(tu, au),
                    iir_down: design_iir_halfband(td, ad),
                    fir_up: design_fir_halfband(tu, au),
                    fir_down: design_fir_halfband(td, ad),
                }
            })
            .collect()
    })[n.min(2)]
}

struct StageDesign {
    iir_up: Vec<f64>,
    iir_down: Vec<f64>,
    fir_up: Vec<f64>,
    fir_down: Vec<f64>,
}

fn kaiser_halfband(n: usize, a: f64) -> Vec<f64> {
    let beta = if a > 50.0 {
        0.1102 * (a - 8.7)
    } else if a >= 21.0 {
        0.5842 * (a - 21.0).powf(0.4) + 0.07886 * (a - 21.0)
    } else {
        0.0
    };
    let m = (n - 1) / 2;
    let i0b = bessel_i0(beta);
    let mut h = vec![0.0f64; n];
    let mut odd_sum = 0.0;
    for (j, tap) in h.iter_mut().enumerate() {
        let off = j as i64 - m as i64;
        if off == 0 {
            *tap = 0.5;
        } else if off % 2 == 0 {
            *tap = 0.0;
        } else {
            let r = 2.0 * j as f64 / (n - 1) as f64 - 1.0;
            let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b;
            *tap = (PI * off as f64 / 2.0).sin() / (PI * off as f64) * w;
            odd_sum += *tap;
        }
    }
    // The two polyphase branches each sum to exactly 1/2, so DC passes with no image at the
    // high rate's Nyquist.
    for (j, tap) in h.iter_mut().enumerate() {
        if (j as i64 - m as i64) % 2 != 0 {
            *tap *= 0.5 / odd_sum;
        }
    }
    h
}

/// Phase delay (in high-rate samples) at a very low frequency of the two-path half-band.
fn iir_phase_delay(coeffs: &[f64]) -> f64 {
    let f = 0.0001;
    let w = 2.0 * PI * f;
    // H(z) = (A0(z^2) + z^-1 A1(z^2)) / 2.
    let allpass = |a: f64, w2: f64| -> (f64, f64) {
        // (a + z^-1) / (1 + a z^-1) at z = e^{j w2}
        let (c, s) = (w2.cos(), -w2.sin());
        let (nr, ni) = (a + c, s);
        let (dr, di) = (1.0 + a * c, a * s);
        let den = dr * dr + di * di;
        ((nr * dr + ni * di) / den, (ni * dr - nr * di) / den)
    };
    let mul = |x: (f64, f64), y: (f64, f64)| (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0);
    let mut p0 = (1.0, 0.0);
    let mut p1 = (w.cos(), -w.sin());
    for (i, &a) in coeffs.iter().enumerate() {
        if i % 2 == 0 {
            p0 = mul(p0, allpass(a, 2.0 * w));
        } else {
            p1 = mul(p1, allpass(a, 2.0 * w));
        }
    }
    let h = (0.5 * (p0.0 + p1.0), 0.5 * (p0.1 + p1.1));
    -h.1.atan2(h.0) / w
}

// ---------------------------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct IirPath {
    direct: Vec<f32>,
    delayed: Vec<f32>,
}

impl IirPath {
    fn new(c: &[f64]) -> Self {
        Self {
            direct: c.iter().step_by(2).map(|&a| a as f32).collect(),
            delayed: c.iter().skip(1).step_by(2).map(|&a| a as f32).collect(),
        }
    }
}

#[inline]
fn allpass_chain(coefs: &[f32], state: &mut [f32], mut x: f32) -> f32 {
    for (a, s) in coefs.iter().zip(state.iter_mut()) {
        let y = a * x + *s;
        *s = x - a * y;
        x = y;
    }
    x
}

#[derive(Clone, Debug)]
struct IirStage {
    up: IirPath,
    down: IirPath,
    up_direct: [Vec<f32>; 2],
    up_delayed: [Vec<f32>; 2],
    dn_direct: [Vec<f32>; 2],
    dn_delayed: [Vec<f32>; 2],
    delay_down: [f32; 2],
    latency: f64,
}

impl IirStage {
    fn new(n: usize) -> Self {
        let d = stage_designs(n);
        let (cu, cd) = (&d.iir_up, &d.iir_down);
        let latency = iir_phase_delay(cu) + iir_phase_delay(cd);
        let up = IirPath::new(cu);
        let down = IirPath::new(cd);
        let z = |len: usize| [vec![0.0f32; len], vec![0.0f32; len]];
        Self {
            up_direct: z(up.direct.len()),
            up_delayed: z(up.delayed.len()),
            dn_direct: z(down.direct.len()),
            dn_delayed: z(down.delayed.len()),
            up,
            down,
            delay_down: [0.0; 2],
            latency,
        }
    }

    fn reset(&mut self) {
        for v in self.up_direct.iter_mut().chain(self.up_delayed.iter_mut()) {
            v.fill(0.0);
        }
        for v in self.dn_direct.iter_mut().chain(self.dn_delayed.iter_mut()) {
            v.fill(0.0);
        }
        self.delay_down = [0.0; 2];
    }

    fn up(&mut self, ch: usize, input: &[f32], out: &mut [f32]) {
        for (i, &x) in input.iter().enumerate() {
            out[2 * i] = allpass_chain(&self.up.direct, &mut self.up_direct[ch], x);
            out[2 * i + 1] = allpass_chain(&self.up.delayed, &mut self.up_delayed[ch], x);
        }
    }

    fn down(&mut self, ch: usize, input: &[f32], out: &mut [f32]) {
        for (i, y) in out.iter_mut().enumerate() {
            let direct = allpass_chain(&self.down.direct, &mut self.dn_direct[ch], input[2 * i]);
            let delayed = allpass_chain(&self.down.delayed, &mut self.dn_delayed[ch], input[2 * i + 1]);
            *y = (self.delay_down[ch] + direct) * 0.5;
            self.delay_down[ch] = delayed;
        }
    }
}

/// A delay line read backwards from the newest sample, power-of-two sized.
#[derive(Clone, Debug)]
struct Ring {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Ring {
    fn new(len: usize) -> Self {
        let size = len.next_power_of_two().max(2);
        Self { buf: vec![0.0; size], mask: size - 1, pos: 0 }
    }
    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }
    /// The sample `k` steps before the newest.
    #[inline]
    fn back(&self, k: usize) -> f32 {
        self.buf[(self.pos.wrapping_sub(k)) & self.mask]
    }
    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

#[derive(Clone, Debug)]
struct FirStage {
    /// Up: the non-zero even-index taps h[2k] times 2 (the zero-stuffing gain).
    up_even: Vec<f32>,
    /// Up: the centre tap's delay in low-rate samples.
    up_centre: usize,
    /// Down: (offset back from the newest high-rate sample, tap) for every non-zero tap.
    down_taps: Vec<(usize, f32)>,
    up_hist: [Ring; 2],
    dn_hist: [Ring; 2],
    latency: f64,
}

impl FirStage {
    fn new(n: usize) -> Self {
        let d = stage_designs(n);
        let (hu, hd) = (&d.fir_up, &d.fir_down);
        let mu = (hu.len() - 1) / 2;
        let md = (hd.len() - 1) / 2;
        let up_even: Vec<f32> = hu.iter().step_by(2).map(|&h| (2.0 * h) as f32).collect();
        let down_taps: Vec<(usize, f32)> =
            hd.iter().enumerate().filter(|(_, &h)| h != 0.0).map(|(j, &h)| (j, h as f32)).collect();
        Self {
            up_centre: (mu - 1) / 2,
            up_hist: [Ring::new(up_even.len()), Ring::new(up_even.len())],
            dn_hist: [Ring::new(hd.len()), Ring::new(hd.len())],
            up_even,
            down_taps,
            latency: (mu + md) as f64,
        }
    }

    fn reset(&mut self) {
        for r in self.up_hist.iter_mut().chain(self.dn_hist.iter_mut()) {
            r.clear();
        }
    }

    fn up(&mut self, ch: usize, input: &[f32], out: &mut [f32]) {
        let hist = &mut self.up_hist[ch];
        for (i, &x) in input.iter().enumerate() {
            hist.push(x);
            let mut acc = 0.0f32;
            for (k, &h) in self.up_even.iter().enumerate() {
                acc += h * hist.back(k);
            }
            out[2 * i] = acc;
            out[2 * i + 1] = hist.back(self.up_centre);
        }
    }

    fn down(&mut self, ch: usize, input: &[f32], out: &mut [f32]) {
        let hist = &mut self.dn_hist[ch];
        for (i, y) in out.iter_mut().enumerate() {
            hist.push(input[2 * i]);
            let mut acc = 0.0f32;
            for &(j, h) in &self.down_taps {
                acc += h * hist.back(j);
            }
            *y = acc;
            hist.push(input[2 * i + 1]);
        }
    }
}

#[derive(Clone, Debug)]
enum Stage {
    Iir(IirStage),
    Fir(FirStage),
}

impl Stage {
    fn reset(&mut self) {
        match self {
            Stage::Iir(s) => s.reset(),
            Stage::Fir(s) => s.reset(),
        }
    }
    fn up(&mut self, ch: usize, input: &[f32], out: &mut [f32]) {
        match self {
            Stage::Iir(s) => s.up(ch, input, out),
            Stage::Fir(s) => s.up(ch, input, out),
        }
    }
    fn down(&mut self, ch: usize, input: &[f32], out: &mut [f32]) {
        match self {
            Stage::Iir(s) => s.down(ch, input, out),
            Stage::Fir(s) => s.down(ch, input, out),
        }
    }
    /// Up plus down latency, in samples of this stage's high rate.
    fn latency(&self) -> f64 {
        match self {
            Stage::Iir(s) => s.latency,
            Stage::Fir(s) => s.latency,
        }
    }
}

/// One oversampler per factor and filter: `[factor 2/4/8][min, lin]`, each with its own stages.
#[derive(Clone, Debug)]
struct Chain {
    sets: Vec<Vec<Stage>>,
}

impl Chain {
    fn new() -> Self {
        let mut sets = Vec::with_capacity(6);
        for stages in 1..=3 {
            for f in 0..2 {
                let set: Vec<Stage> = (0..stages)
                    .map(|n| if f == 0 { Stage::Iir(IirStage::new(n)) } else { Stage::Fir(FirStage::new(n)) })
                    .collect();
                sets.push(set);
            }
        }
        Self { sets }
    }
    fn get(&mut self, mode: OsMode, filter: OsFilter) -> Option<&mut Vec<Stage>> {
        if mode == OsMode::X1 {
            return None;
        }
        self.sets.get_mut((mode.stages() - 1) * 2 + filter as usize)
    }
    fn reset(&mut self) {
        for set in &mut self.sets {
            for s in set {
                s.reset();
            }
        }
    }
}

/// Fractional latency in base-rate samples of a configuration.
pub fn latency_samples(mode: OsMode, filter: OsFilter) -> f64 {
    let mut total = 0.0;
    let mut order = 1.0;
    for n in 0..mode.stages() {
        order *= 2.0;
        let stage = match filter {
            OsFilter::MinPhase => Stage::Iir(IirStage::new(n)),
            OsFilter::LinPhase => Stage::Fir(FirStage::new(n)),
        };
        total += stage.latency() / order;
    }
    total
}

/// Ping-pong work buffers, stereo, sized for the highest factor.
struct Work {
    a: [Vec<f32>; 2],
    b: [Vec<f32>; 2],
}

impl Work {
    fn new(len: usize) -> Self {
        Self { a: [vec![0.0; len], vec![0.0; len]], b: [vec![0.0; len], vec![0.0; len]] }
    }
}

/// Wet, dry and reference chains with their work buffers.
pub struct Multirate {
    wet: Chain,
    dry: Chain,
    reference: Chain,
    mode: OsMode,
    filter: OsFilter,
    latency: [[f64; 2]; 4],
    max_block: usize,
    work: Work,
    aux: Work,
    /// Length of the oversampled block held in the wet work buffer.
    os_len: usize,
}

impl Multirate {
    pub fn new(max_block: usize) -> Self {
        let max_block = max_block.max(1);
        let mut latency = [[0.0; 2]; 4];
        for (m, row) in latency.iter_mut().enumerate() {
            for (f, l) in row.iter_mut().enumerate() {
                *l = latency_samples(OsMode::from_index(m as i32), OsFilter::from_index(f as i32));
            }
        }
        Self {
            wet: Chain::new(),
            dry: Chain::new(),
            reference: Chain::new(),
            mode: OsMode::X2,
            filter: OsFilter::MinPhase,
            latency,
            max_block,
            work: Work::new(max_block * 8),
            aux: Work::new(max_block * 8),
            os_len: 0,
        }
    }

    pub fn max_block(&self) -> usize {
        self.max_block
    }

    pub fn reset(&mut self) {
        self.wet.reset();
        self.dry.reset();
        self.reference.reset();
    }

    pub fn reset_reference(&mut self) {
        self.reference.reset();
    }

    /// Returns true (and clears every chain) when the configuration changed.
    pub fn set_config(&mut self, mode: OsMode, filter: OsFilter) -> bool {
        if mode == self.mode && filter == self.filter {
            return false;
        }
        self.mode = mode;
        self.filter = filter;
        self.reset();
        true
    }

    pub fn mode(&self) -> OsMode {
        self.mode
    }

    pub fn factor(&self) -> usize {
        self.mode.factor()
    }

    pub fn latency_for(&self, mode: OsMode, filter: OsFilter) -> f64 {
        self.latency[mode as usize][filter as usize]
    }

    fn run_up(stages: &mut [Stage], work: &mut Work, n: usize) -> usize {
        let mut len = n;
        for stage in stages.iter_mut() {
            for ch in 0..2 {
                stage.up(ch, &work.a[ch][..len], &mut work.b[ch][..2 * len]);
            }
            std::mem::swap(&mut work.a, &mut work.b);
            len *= 2;
        }
        len
    }

    fn run_down(stages: &mut [Stage], work: &mut Work, mut len: usize) {
        for stage in stages.iter_mut().rev() {
            for ch in 0..2 {
                stage.down(ch, &work.a[ch][..len], &mut work.b[ch][..len / 2]);
            }
            std::mem::swap(&mut work.a, &mut work.b);
            len /= 2;
        }
    }

    /// Upsamples a block into the wet work buffer; returns the oversampled length.
    pub fn upsample(&mut self, l: &[f32], r: &[f32]) -> usize {
        let n = l.len().min(r.len()).min(self.max_block);
        self.work.a[0][..n].copy_from_slice(&l[..n]);
        self.work.a[1][..n].copy_from_slice(&r[..n]);
        let (mode, filter) = (self.mode, self.filter);
        self.os_len = match self.wet.get(mode, filter) {
            Some(stages) => Self::run_up(stages, &mut self.work, n),
            None => n,
        };
        self.os_len
    }

    /// The oversampled wet block, stereo.
    pub fn os_block(&mut self) -> (&mut [f32], &mut [f32]) {
        let len = self.os_len;
        let (a, b) = self.work.a.split_at_mut(1);
        (&mut a[0][..len], &mut b[0][..len])
    }

    /// Downsamples the wet work buffer into `l`/`r`.
    pub fn downsample(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (mode, filter) = (self.mode, self.filter);
        let len = self.os_len;
        if let Some(stages) = self.wet.get(mode, filter) {
            Self::run_down(stages, &mut self.work, len);
        }
        let n = len / mode.factor();
        let n = n.min(l.len()).min(r.len());
        l[..n].copy_from_slice(&self.work.a[0][..n]);
        r[..n].copy_from_slice(&self.work.a[1][..n]);
    }

    fn align(chain: &mut Chain, aux: &mut Work, mode: OsMode, filter: OsFilter, l: &mut [f32], r: &mut [f32]) {
        let Some(stages) = chain.get(mode, filter) else {
            return;
        };
        let n = l.len().min(r.len());
        if n == 0 {
            return;
        }
        aux.a[0][..n].copy_from_slice(&l[..n]);
        aux.a[1][..n].copy_from_slice(&r[..n]);
        let len = Self::run_up(stages, aux, n);
        Self::run_down(stages, aux, len);
        l[..n].copy_from_slice(&aux.a[0][..n]);
        r[..n].copy_from_slice(&aux.a[1][..n]);
    }

    /// Runs the dry block, and the reference block when asked, through their matched chains.
    pub fn align_dry(&mut self, dry: (&mut [f32], &mut [f32]), reference: (&mut [f32], &mut [f32]), with_reference: bool) {
        let (mode, filter) = (self.mode, self.filter);
        Self::align(&mut self.dry, &mut self.aux, mode, filter, dry.0, dry.1);
        if with_reference {
            Self::align(&mut self.reference, &mut self.aux, mode, filter, reference.0, reference.1);
        }
    }
}

/// Round half to even, as the original's latency report does.
pub fn round_latency(l: f64) -> usize {
    (l as f32).round_ties_even().max(0.0) as usize
}
