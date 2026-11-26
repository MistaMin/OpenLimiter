//! Kernel (FIR) modes (OpenEQ `LinearPhaseEngine`): Linear Phase, Analog, Mix and Analog ZL.
//!
//! A worker thread designs an immutable kernel from a copied request: each band's response on
//! the bins of a kernel-length FFT, combined through the 2x2 placement matrix (left, right, mid,
//! side, pan), turned into an impulse, centred (or left causal for Analog ZL), windowed with the
//! window normalised at its centre (a flat curve stays unity), and transformed for overlap-add
//! convolution. Analog designs every band at 16x the rate (no bilinear warp); Mix blends the
//! minimum-phase response below the crossover with the linear one above; per-band free phase
//! rotates a band between inverse (-1), linear (0) and minimum (+1) phase. All-pass bands and
//! split stream gains are left out, as in OpenEQ.
//!
//! Hand-off without locks: the audio thread tries to place a request in a single slot guarded by
//! an atomic flag (it never waits; when busy it retries on the next block). Finished kernels are
//! published by an atomic pointer swap; the audio thread takes ownership with another swap and
//! gives kernels it no longer needs back through atomic retire slots, which the worker frees. A
//! new kernel of the same length takes over after one kernel length of input, then the outputs
//! crossfade over 512 samples.

use eq_core::cx::{Cx, C32};
use eq_core::fft::Fft;
use eq_core::response::{band_matrix, design_band, multiply, normalize, stage_magnitude, stage_response, Matrix, Norm, MAX_STAGES};
use eq_core::types::*;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::thread::JoinHandle;

pub const MIN_ORDER: u32 = 11;
pub const MAX_ORDER: u32 = 14;
pub const MAX_KERNEL: usize = 1 << MAX_ORDER;
const CROSSFADE: usize = 512;
const RETIRE_SLOTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Linear,
    Analog,
    Mixed,
    AnalogZeroLatency,
}

#[derive(Clone, Copy, Debug)]
pub struct CurveBand {
    pub ty: FilterType,
    pub placement: StereoMode,
    pub pan: f32,
    pub frequency: f32,
    pub gain_db: f32,
    pub q: f32,
    pub slope_db: i32,
    pub phase: f32,
}

impl Default for CurveBand {
    fn default() -> Self {
        CurveBand { ty: FilterType::Bell, placement: StereoMode::Stereo, pan: 0.0, frequency: 1000.0, gain_db: 0.0, q: 1.0, slope_db: 12, phase: 0.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub sample_rate: f64,
    pub order: u32,
    pub target: Target,
    pub crossover_hz: f64,
    pub window_shape: u8,
    pub window_parameter: f32,
    pub count: usize,
    pub bands: [CurveBand; 32],
}

impl Default for Request {
    fn default() -> Self {
        Request {
            sample_rate: 48000.0,
            order: 12,
            target: Target::Linear,
            crossover_hz: 1000.0,
            window_shape: 0,
            window_parameter: 0.0,
            count: 0,
            bands: [CurveBand::default(); 32],
        }
    }
}

impl Request {
    pub fn causal(&self) -> bool {
        self.target == Target::AnalogZeroLatency
    }

    /// FNV-1a over everything the kernel depends on.
    pub fn hash(&self) -> u64 {
        let mut h = 1469598103934665603u64;
        let mut mix = |v: u32| {
            h ^= v as u64;
            h = h.wrapping_mul(1099511628211);
        };
        mix(self.order);
        mix(self.target as u32);
        mix((self.crossover_hz as f32).to_bits());
        mix((self.sample_rate as f32).to_bits());
        mix(self.window_shape as u32);
        mix(self.window_parameter.to_bits());
        mix(self.count as u32);
        for b in &self.bands[..self.count] {
            mix(b.ty as u32);
            mix(b.placement as u32);
            mix(b.pan.to_bits());
            mix(b.slope_db as u32);
            mix(b.frequency.to_bits());
            mix(b.gain_db.to_bits());
            mix(b.q.to_bits());
            mix(b.phase.to_bits());
        }
        h
    }
}

pub struct Kernel {
    pub order: u32,
    pub len: usize,
    pub causal: bool,
    pub request_hash: u64,
    /// Four paths (L->L, R->L, L->R, R->R), each 2 * len bins.
    spectra: Vec<C32>,
}

struct Shared {
    busy: AtomicBool,
    ready: AtomicBool,
    request: UnsafeCell<Request>,
    incoming: AtomicPtr<Kernel>,
    retired: [AtomicPtr<Kernel>; RETIRE_SLOTS],
    quit: AtomicBool,
    completed: AtomicU64,
}

// The request cell is only touched by the side that won the `busy` flag.
unsafe impl Sync for Shared {}
unsafe impl Send for Shared {}

fn free(p: *mut Kernel) {
    if !p.is_null() {
        // SAFETY: every non-null pointer in the shared slots came from Box::into_raw and is owned
        // by exactly one slot at a time.
        unsafe { drop(Box::from_raw(p)) };
    }
}

impl Shared {
    fn drain_retired(&self) {
        for slot in &self.retired {
            free(slot.swap(std::ptr::null_mut(), Ordering::AcqRel));
        }
    }
}

/// Owns the worker thread. Dropping it stops the thread and frees every kernel.
pub struct Worker {
    shared: Box<Shared>,
    thread: Option<JoinHandle<()>>,
}

/// The worker's view of the shared state. The owner joins the thread before the box is freed.
struct SharedPtr(*const Shared);
unsafe impl Send for SharedPtr {}

impl Worker {
    pub fn start() -> Self {
        let shared = Box::new(Shared {
            busy: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            request: UnsafeCell::new(Request::default()),
            incoming: AtomicPtr::new(std::ptr::null_mut()),
            retired: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            quit: AtomicBool::new(false),
            completed: AtomicU64::new(0),
        });
        let ptr = SharedPtr(&*shared as *const Shared);
        let thread = std::thread::Builder::new()
            .name("eq kernel design".into())
            .spawn(move || {
                let p = ptr;
                // SAFETY: `Worker::drop` joins this thread before the shared box is freed.
                worker_loop(unsafe { &*p.0 })
            })
            .ok();
        Worker { shared, thread }
    }
}

impl Worker {
    /// Wakes the worker for a new request. Any thread, the audio thread too: no lock, no
    /// allocation, no wait.
    fn wake(&self) {
        if let Some(t) = &self.thread {
            t.thread().unpark();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        self.wake();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        free(self.shared.incoming.swap(std::ptr::null_mut(), Ordering::AcqRel));
        self.shared.drain_retired();
    }
}

fn worker_loop(s: &Shared) {
    let mut designer = Designer::default();
    while !s.quit.load(Ordering::Acquire) {
        s.drain_retired();
        if !s.ready.load(Ordering::Acquire) {
            // Woken by `Worker::wake` when a request comes; the timeout only frees retired kernels.
            std::thread::park_timeout(std::time::Duration::from_millis(50));
            continue;
        }
        if s.busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            std::thread::yield_now();
            continue;
        }
        // SAFETY: we hold the busy flag.
        let req = unsafe { *s.request.get() };
        s.ready.store(false, Ordering::Relaxed);
        s.busy.store(false, Ordering::Release);
        let kernel = Box::new(designer.design(&req));
        free(s.incoming.swap(Box::into_raw(kernel), Ordering::AcqRel));
        s.completed.fetch_add(1, Ordering::AcqRel);
    }
}

/// Worker-side scratch: FFTs and windows per kernel order.
#[derive(Default)]
struct Designer {
    ffts: Vec<(u32, Fft, Fft)>,
}

fn window(shape: u8, parameter: f32, len: usize) -> Vec<f32> {
    let p = parameter.clamp(0.0, 1.0) as f64;
    if shape == 1 {
        let beta = 1.0 + 13.0 * p;
        let i0 = |x: f64| {
            let (mut sum, mut term) = (1.0f64, 1.0f64);
            for k in 1..30 {
                term *= x * x / (4.0 * k as f64 * k as f64);
                sum += term;
                if term < 1.0e-15 * sum {
                    break;
                }
            }
            sum
        };
        let den = i0(beta);
        let half = len as f64 / 2.0;
        return (0..len)
            .map(|i| {
                let x = 2.0 * (i as f64 - half) / len as f64;
                (i0(beta * (1.0 - x * x).max(0.0).sqrt()) / den) as f32
            })
            .collect();
    }
    let m = (len - 1) as f64;
    let exponent = 1.0 + 3.0 * p;
    (0..len)
        .map(|i| {
            let c = |k: f64| (k * 2.0 * std::f64::consts::PI * i as f64 / m).cos();
            let w = match shape {
                2 => 0.5 - 0.5 * c(1.0),
                3 => 0.54 - 0.46 * c(1.0),
                4 => 0.42 - 0.5 * c(1.0) + 0.08 * c(2.0),
                5 => 1.0,
                _ => 0.35875 - 0.48829 * c(1.0) + 0.14128 * c(2.0) - 0.01168 * c(3.0),
            };
            (w as f32).max(1.0e-12).powf(exponent as f32)
        })
        .collect()
}

fn mixed_weight(f: f64, crossover: f64) -> f64 {
    let oct = (f.max(1.0) / crossover.max(1.0)).log2();
    let t = oct.clamp(-0.5, 0.5);
    0.5 * (1.0 + (std::f64::consts::PI * (t + 0.5)).cos())
}

impl Designer {
    fn ffts(&mut self, order: u32) -> usize {
        if let Some(i) = self.ffts.iter().position(|(o, _, _)| *o == order) {
            return i;
        }
        self.ffts.push((order, Fft::new(order), Fft::new(order + 1)));
        self.ffts.len() - 1
    }

    fn design(&mut self, r: &Request) -> Kernel {
        let order = r.order.clamp(MIN_ORDER, MAX_ORDER);
        let n = 1usize << order;
        let bins = n / 2 + 1;
        let causal = r.causal();
        let design_rate = if r.target == Target::Analog || causal { r.sample_rate * 16.0 } else { r.sample_rate };
        let mut kernel = Kernel { order, len: n, causal, request_hash: r.hash(), spectra: vec![C32::ZERO; 4 * 2 * n] };

        struct Prepared {
            placement: StereoMode,
            pan: f32,
            phase: f32,
            count: usize,
            stages: [Norm; MAX_STAGES],
        }
        let mut prepared: Vec<Prepared> = Vec::with_capacity(r.count);
        for b in &r.bands[..r.count.min(32)] {
            if !supported_in_linear_phase(b.ty) {
                continue;
            }
            let d = design_band(b.ty, b.frequency, b.gain_db, b.q, b.slope_db, design_rate);
            let mut stages = [[0.0; 5]; MAX_STAGES];
            for (s, raw) in stages.iter_mut().zip(d.raw[..d.stage_count].iter()) {
                *s = normalize(raw);
            }
            prepared.push(Prepared { placement: b.placement, pan: b.pan, phase: if causal { 1.0 } else { b.phase }, count: d.stage_count, stages });
        }
        let any_free_phase = causal || prepared.iter().any(|p| p.phase.abs() > 1.0e-6);
        let complex = any_free_phase || r.target == Target::Mixed;

        let mut real = vec![Matrix::default(); bins];
        let mut cplx = vec![[Cx::ONE, Cx::ZERO, Cx::ZERO, Cx::ONE]; bins];
        for bin in 0..bins {
            let f = (bin as f64 * r.sample_rate / n as f64).max(1.0);
            let mut m = Matrix::default();
            for p in &prepared {
                let mut g = 1.0;
                for s in &p.stages[..p.count] {
                    g *= stage_magnitude(s, f, design_rate);
                }
                m = multiply(band_matrix(g, p.placement, p.pan), m);
            }
            real[bin] = m;
            if complex {
                let (mut ll, mut lr, mut rl, mut rr) = (Cx::ONE, Cx::ZERO, Cx::ZERO, Cx::ONE);
                for p in &prepared {
                    let mut h = Cx::ONE;
                    for s in &p.stages[..p.count] {
                        h *= stage_response(s, f, design_rate);
                    }
                    if p.phase.abs() > 1.0e-6 {
                        let mag = h.abs();
                        let rot = (p.phase as f64).clamp(-1.0, 1.0);
                        h = h.powf(rot) * mag.max(1.0e-12).powf(1.0 - rot);
                    }
                    let route = band_matrix(1.0, p.placement, p.pan);
                    let (bll, blr, brl, brr) = (h * route.ll, h * route.lr, h * route.rl, h * route.rr);
                    let nll = bll * ll + blr * rl;
                    let nlr = bll * lr + blr * rr;
                    let nrl = brl * ll + brr * rl;
                    let nrr = brl * lr + brr * rr;
                    ll = nll;
                    lr = nlr;
                    rl = nrl;
                    rr = nrr;
                }
                let w = if r.target == Target::Mixed { mixed_weight(f, r.crossover_hz) } else { 1.0 };
                cplx[bin] = [
                    ll * w + (1.0 - w) * m.ll,
                    lr * w + (1.0 - w) * m.lr,
                    rl * w + (1.0 - w) * m.rl,
                    rr * w + (1.0 - w) * m.rr,
                ];
            }
        }

        let fi = self.ffts(order);
        let win = window(r.window_shape, r.window_parameter, n);
        let centre = win[n / 2];
        let scale = if centre.abs() > 1.0e-8 { 1.0 / centre } else { 1.0 };
        let mut time = vec![C32::ZERO; 2 * n];
        for path in 0..4 {
            time.iter_mut().for_each(|v| *v = C32::ZERO);
            for bin in 0..bins {
                let v = if complex {
                    cplx[bin][path]
                } else {
                    let m = real[bin];
                    Cx::real([m.ll, m.lr, m.rl, m.rr][path])
                };
                let re = if v.re.is_finite() { v.re as f32 } else { 0.0 };
                let im = if v.im.is_finite() { v.im as f32 } else { 0.0 };
                time[bin] = C32::new(re, im);
                if bin > 0 && bin < n / 2 {
                    time[n - bin] = C32::new(re, -im);
                }
            }
            self.ffts[fi].1.inverse(&mut time[..n]);
            let mut taps = vec![0.0f32; n];
            for (i, t) in taps.iter_mut().enumerate() {
                let source = if causal { i } else { (i + n / 2) % n };
                let pre_ring = causal && i >= n / 2;
                let wi = if causal { (n / 2 + i).min(n - 1) } else { i };
                *t = if pre_ring { 0.0 } else { time[source].re * win[wi] * scale };
            }
            for (i, v) in time.iter_mut().enumerate() {
                *v = if i < n { C32::new(taps[i], 0.0) } else { C32::ZERO };
            }
            self.ffts[fi].2.forward(&mut time);
            kernel.spectra[path * 2 * n..(path + 1) * 2 * n].copy_from_slice(&time);
        }
        kernel
    }
}

/// Audio side of the kernel modes.
pub struct LpEngine {
    worker: Worker,
    ffts: Vec<Fft>,
    current: *mut Kernel,
    fade: *mut Kernel,
    overlap: [Vec<f32>; 2],
    overlap_retire: [Vec<f32>; 2],
    time: Vec<C32>,
    freq: Vec<C32>,
    acc: [[f32; 512]; 2],
    acc_fade: [[f32; 512]; 2],
    engage: usize,
    fade_left: usize,
    hard_next: bool,
    /// The plain delay is being heard (the kernel came late): the next kernel blends in from it.
    from_dry: bool,
    /// Samples the delay is still heard while a late kernel fills, then the crossfade from it.
    dry_engage: usize,
    dry_fade: usize,
    delay: [Vec<f32>; 2],
    delay_write: usize,
    last_hash: u64,
    submit_pending: bool,
}

// The raw kernel pointers are owned by this engine alone (see the module notes).
unsafe impl Send for LpEngine {}

impl Default for LpEngine {
    fn default() -> Self {
        LpEngine::new()
    }
}

impl LpEngine {
    pub fn new() -> Self {
        let max_fft = 2 * MAX_KERNEL;
        LpEngine {
            worker: Worker::start(),
            ffts: (MIN_ORDER..=MAX_ORDER).map(|o| Fft::new(o + 1)).collect(),
            current: std::ptr::null_mut(),
            fade: std::ptr::null_mut(),
            overlap: [vec![0.0; MAX_KERNEL], vec![0.0; MAX_KERNEL]],
            overlap_retire: [vec![0.0; MAX_KERNEL], vec![0.0; MAX_KERNEL]],
            time: vec![C32::ZERO; max_fft],
            freq: vec![C32::ZERO; max_fft],
            acc: [[0.0; 512]; 2],
            acc_fade: [[0.0; 512]; 2],
            engage: 0,
            fade_left: 0,
            hard_next: true,
            from_dry: false,
            dry_engage: 0,
            dry_fade: 0,
            delay: [vec![0.0; 2 * MAX_KERNEL], vec![0.0; 2 * MAX_KERNEL]],
            delay_write: 0,
            last_hash: 0,
            submit_pending: true,
        }
    }

    /// Designs finished by the worker since it started.
    pub fn completed(&self) -> u64 {
        self.worker.shared.completed.load(Ordering::Acquire)
    }

    /// Hash of the request the current kernel was designed from (0 when none).
    pub fn current_hash(&self) -> u64 {
        if self.current.is_null() {
            0
        } else {
            // SAFETY: owned by this engine.
            unsafe { (*self.current).request_hash }
        }
    }

    pub fn last_submitted_hash(&self) -> u64 {
        self.last_hash
    }

    fn retire(&self, p: *mut Kernel) {
        if p.is_null() {
            return;
        }
        for slot in &self.worker.shared.retired {
            if slot.compare_exchange(std::ptr::null_mut(), p, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                return;
            }
        }
        // Every slot full (the worker frees them every millisecond): leaking is the only choice
        // that keeps the audio thread free of deallocation.
    }

    /// Never waits. Returns false when the worker holds the slot; the caller retries later.
    pub fn submit(&mut self, r: &Request) -> bool {
        let h = r.hash();
        if h == self.last_hash && !self.submit_pending {
            return true;
        }
        let s = &self.worker.shared;
        if s.busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            self.submit_pending = true;
            return false;
        }
        // SAFETY: we hold the busy flag.
        unsafe { *s.request.get() = *r };
        s.ready.store(true, Ordering::Release);
        s.busy.store(false, Ordering::Release);
        self.worker.wake();
        self.last_hash = h;
        self.submit_pending = false;
        true
    }

    /// The wait for a kernel ran out and the plain delay is now heard: blend the next kernel in
    /// from it instead of switching at once.
    pub fn blend_from_dry(&mut self) {
        self.from_dry = true;
    }

    pub fn force_resubmit(&mut self) {
        self.submit_pending = true;
    }

    /// Clears the convolution state; the next kernel is taken at once, without a hand-off.
    pub fn reset(&mut self) {
        for o in self.overlap.iter_mut().chain(self.overlap_retire.iter_mut()) {
            o.fill(0.0);
        }
        let f = std::mem::replace(&mut self.fade, std::ptr::null_mut());
        self.retire(f);
        self.engage = 0;
        self.fade_left = 0;
        self.hard_next = true;
        self.from_dry = false;
        self.dry_engage = 0;
        self.dry_fade = 0;
        for d in self.delay.iter_mut() {
            d.fill(0.0);
        }
    }

    /// True when the installed kernel has this length and kind.
    pub fn has_kernel(&self, order: u32, causal: bool) -> bool {
        // SAFETY: owned by this engine.
        !self.current.is_null() && unsafe { (*self.current).order == order && (*self.current).causal == causal }
    }

    /// Takes a finished kernel from the worker, if there is one.
    pub fn poll(&mut self, order: u32, causal: bool) {
        let p = self.worker.shared.incoming.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if p.is_null() {
            return;
        }
        // SAFETY: we own `p` now.
        let (po, pc, plen) = unsafe { ((*p).order, (*p).causal, (*p).len) };
        if po != order || pc != causal {
            self.retire(p);
            return;
        }
        let same = !self.hard_next && self.has_kernel(order, causal);
        if same {
            if self.engage == 0 && self.fade_left == 0 {
                let old_fade = std::mem::replace(&mut self.fade, self.current);
                self.retire(old_fade);
                for ch in 0..2 {
                    let (a, b) = (&self.overlap[ch], &mut self.overlap_retire[ch]);
                    b.copy_from_slice(a);
                }
            } else {
                self.retire(self.current);
            }
            self.engage = plen;
            self.fade_left = 0;
        } else {
            self.retire(self.current);
            let f = std::mem::replace(&mut self.fade, std::ptr::null_mut());
            self.retire(f);
            self.engage = 0;
            self.fade_left = 0;
        }
        for o in self.overlap.iter_mut() {
            o.fill(0.0);
        }
        if !same && self.from_dry {
            self.from_dry = false;
            self.dry_engage = plen;
            self.dry_fade = CROSSFADE;
        }
        self.current = p;
        self.hard_next = false;
    }

    fn convolve(ffts: &[Fft], k: &Kernel, time: &mut [C32], freq: &mut [C32], l: &[f32], r: &[f32], overlap: &mut [Vec<f32>; 2], out: &mut [[f32; 512]; 2]) {
        let n = k.len;
        let m = 2 * n;
        let count = l.len();
        let fft = &ffts[(k.order - MIN_ORDER) as usize];
        for i in 0..m {
            time[i] = if i < count { C32::new(l[i], r[i]) } else { C32::ZERO };
        }
        fft.forward(&mut time[..m]);
        let h = &k.spectra;
        for kk in 0..m {
            let km = (m - kk) % m;
            let z = time[kk];
            let zc = time[km].conj();
            let xl = z.add(zc).scale(0.5);
            let d = z.sub(zc);
            let xr = C32::new(d.im * 0.5, -d.re * 0.5);
            let yl = h[kk].mul(xl).add(h[m + kk].mul(xr));
            let yr = h[2 * m + kk].mul(xl).add(h[3 * m + kk].mul(xr));
            freq[kk] = C32::new(yl.re - yr.im, yl.im + yr.re);
        }
        fft.inverse(&mut freq[..m]);
        let tail = n - 1;
        for ch in 0..2 {
            let ov = &mut overlap[ch];
            for i in 0..count {
                let y = if ch == 0 { freq[i].re } else { freq[i].im };
                out[ch][i] = y + ov[i];
            }
            for i in 0..tail {
                let older = if i + count < tail { ov[i + count] } else { 0.0 };
                let newer = if i + count < m { if ch == 0 { freq[i + count].re } else { freq[i + count].im } } else { 0.0 };
                ov[i] = older + newer;
            }
        }
    }

    /// Processes up to 512 samples in place with the installed kernel; without a kernel of the
    /// expected kind, a plain delay of the same latency.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], order: u32, causal: bool) {
        let count = l.len().min(r.len()).min(512);
        let latency = if causal { 0 } else { (1usize << order) / 2 };
        // The identity path's delay line always runs, so falling back is seamless in time.
        let dmask = self.delay[0].len() - 1;
        let mut dry = [[0.0f32; 512]; 2];
        for i in 0..count {
            self.delay[0][self.delay_write] = l[i];
            self.delay[1][self.delay_write] = r[i];
            let j = (self.delay_write + self.delay[0].len() - latency) & dmask;
            dry[0][i] = self.delay[0][j];
            dry[1][i] = self.delay[1][j];
            self.delay_write = (self.delay_write + 1) & dmask;
        }
        if !self.has_kernel(order, causal) {
            l[..count].copy_from_slice(&dry[0][..count]);
            r[..count].copy_from_slice(&dry[1][..count]);
            return;
        }
        // SAFETY: owned by this engine; not freed while installed.
        let cur = unsafe { &*self.current };
        Self::convolve(&self.ffts, cur, &mut self.time, &mut self.freq, &l[..count], &r[..count], &mut self.overlap, &mut self.acc);
        let handoff = !self.fade.is_null() && (self.engage > 0 || self.fade_left > 0);
        if handoff {
            // SAFETY: as above.
            let old = unsafe { &*self.fade };
            Self::convolve(&self.ffts, old, &mut self.time, &mut self.freq, &l[..count], &r[..count], &mut self.overlap_retire, &mut self.acc_fade);
        }
        for i in 0..count {
            let (mut newer, mut older) = (1.0f32, 0.0f32);
            if handoff && self.engage > 0 {
                newer = 0.0;
                older = 1.0;
                self.engage -= 1;
                if self.engage == 0 {
                    self.fade_left = CROSSFADE;
                }
            } else if handoff && self.fade_left > 0 {
                newer = 1.0 - self.fade_left as f32 / CROSSFADE as f32;
                older = 1.0 - newer;
                self.fade_left -= 1;
            }
            l[i] = newer * self.acc[0][i] + older * self.acc_fade[0][i];
            r[i] = newer * self.acc[1][i] + older * self.acc_fade[1][i];
        }
        if self.dry_engage > 0 || self.dry_fade > 0 {
            for i in 0..count {
                let wet = if self.dry_engage > 0 {
                    self.dry_engage -= 1;
                    0.0
                } else if self.dry_fade > 0 {
                    self.dry_fade -= 1;
                    1.0 - self.dry_fade as f32 / CROSSFADE as f32
                } else {
                    1.0
                };
                l[i] = wet * l[i] + (1.0 - wet) * dry[0][i];
                r[i] = wet * r[i] + (1.0 - wet) * dry[1][i];
            }
        }
        if handoff && self.fade_left == 0 && self.engage == 0 {
            let f = std::mem::replace(&mut self.fade, std::ptr::null_mut());
            self.retire(f);
        }
    }
}

impl Drop for LpEngine {
    fn drop(&mut self) {
        free(std::mem::replace(&mut self.current, std::ptr::null_mut()));
        free(std::mem::replace(&mut self.fade, std::ptr::null_mut()));
    }
}
