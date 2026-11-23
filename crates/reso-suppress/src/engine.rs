//! Resonance suppressor engine (OpenEQ `ResonanceEngine`).
//!
//! Works on a short-time spectrum (Hann analysis and synthesis, 4x overlap). For every bin it asks
//! how far the level stands above its own surroundings (a 1.2-octave average, taken twice with
//! peaks held down in the second pass) and turns down only that excess, so a tilted or broadly
//! shaped spectrum passes untouched and the result does not depend on the input level. With no
//! reduction the input is reconstructed after `latency()` samples. `process` never allocates.

use eq_core::cx::C32;
use eq_core::fft::Fft;

pub const DISPLAY_BANDS: usize = 120;
pub const MAX_FOCUS: usize = 24;

pub fn display_band_hz(band: usize) -> f32 {
    20.0 * 1000f32.powf(band as f32 / (DISPLAY_BANDS - 1) as f32)
}

#[derive(Clone, Copy, Debug)]
pub struct Focus {
    pub hz: f32,
    /// -1..1: +1 quadruples the cut at the centre, -1 quarters it.
    pub amount: f32,
    pub q: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub depth: f32,
    pub sharpness: f32,
    pub selectivity: f32,
    pub attack_s: f32,
    pub release_s: f32,
    pub floor_db: f32,
    pub centre_hz: f32,
    pub width_octaves: f32,
    pub mix: f32,
    pub link: f32,
    pub hard: bool,
    pub delta: bool,
    pub mid_side: bool,
    pub focus: [Focus; MAX_FOCUS],
    pub focus_count: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            depth: 0.0,
            sharpness: 0.5,
            selectivity: 0.35,
            attack_s: 0.001 * 500f32.powf(0.2),
            release_s: 0.02 * 100f32.powf(0.3),
            floor_db: -80.0,
            centre_hz: 3000.0,
            width_octaves: 10.0,
            mix: 1.0,
            link: 1.0,
            hard: false,
            delta: false,
            mid_side: false,
            focus: [Focus { hz: 1000.0, amount: 0.0, q: 1.0 }; MAX_FOCUS],
            focus_count: 0,
        }
    }
}

pub struct Engine {
    sample_rate: f64,
    fft_size: usize,
    hop: usize,
    bins: usize,
    fft: Fft,
    window: Vec<f32>,
    hop_scale: Vec<f32>,
    magnitude_scale: f32,
    hop_seconds: f32,
    detector_coef: f32,
    input: [Vec<f32>; 4],
    output: [Vec<f32>; 2],
    frame: Vec<C32>,
    spectrum: [Vec<C32>; 4],
    focus_weight: Vec<f32>,
    focus_cached: [Focus; MAX_FOCUS],
    focus_cached_count: isize,
    detector: [Vec<f32>; 2],
    level_db: [Vec<f32>; 2],
    wanted: [Vec<f32>; 2],
    reduction: [Vec<f32>; 2],
    prefix: Vec<f64>,
    scratch: Vec<f32>,
    env_low: Vec<usize>,
    env_high: Vec<usize>,
    bin_band: Vec<i32>,
    seen: u64,
    primed: bool,
    /// Reduction per readout band per channel (L/R, or M/S in mid/side mode), dB, positive.
    pub display: [[f32; DISPLAY_BANDS]; 2],
}

impl Engine {
    pub fn new(sample_rate: f64) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 44100.0 };
        let order = if sr <= 50000.0 { 11 } else if sr <= 100000.0 { 12 } else { 13 };
        let n = 1usize << order;
        let hop = n / 4;
        let bins = n / 2 + 1;
        let window: Vec<f32> = (0..n).map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())).collect();
        let wsum: f64 = window.iter().map(|&w| w as f64).sum();
        let mut hop_scale = vec![1.0f32; hop];
        for (r, hs) in hop_scale.iter_mut().enumerate() {
            let s: f32 = (0..n / hop).map(|k| window[r + k * hop] * window[r + k * hop]).sum();
            *hs = if s > 1.0e-8 { 1.0 / s } else { 0.0 };
        }
        let down = 2f64.powf(-0.6);
        let up = 2f64.powf(0.6);
        let mut env_low = vec![0usize; bins];
        let mut env_high = vec![0usize; bins];
        for k in 0..bins {
            let (mut lo, mut hi) = ((k as f64 * down).floor() as i64, (k as f64 * up).ceil() as i64);
            if hi - lo < 8 {
                lo = k as i64 - 4;
                hi = k as i64 + 4;
            }
            env_low[k] = lo.clamp(1, bins as i64 - 1) as usize;
            env_high[k] = hi.clamp(1, bins as i64 - 1) as usize;
        }
        let bin_band = (0..bins)
            .map(|k| {
                let hz = (k as f64 * sr / n as f64).max(1.0);
                let pos = (hz / 20.0).ln() / 1000f64.ln() * (DISPLAY_BANDS - 1) as f64;
                if !(20.0..=20000.0).contains(&hz) {
                    -1
                } else {
                    (pos.round() as i32).clamp(0, DISPLAY_BANDS as i32 - 1)
                }
            })
            .collect();
        let hop_seconds = (hop as f64 / sr) as f32;
        let mut e = Engine {
            sample_rate: sr,
            fft_size: n,
            hop,
            bins,
            fft: Fft::new(order),
            window,
            hop_scale,
            magnitude_scale: (2.0 / wsum) as f32,
            hop_seconds,
            detector_coef: (-hop_seconds / 0.02).exp(),
            input: std::array::from_fn(|_| vec![0.0; n]),
            output: std::array::from_fn(|_| vec![0.0; 2 * n]),
            frame: vec![C32::ZERO; n],
            spectrum: std::array::from_fn(|_| vec![C32::ZERO; n]),
            focus_weight: vec![1.0; bins],
            focus_cached: [Focus { hz: 0.0, amount: 0.0, q: 0.0 }; MAX_FOCUS],
            focus_cached_count: -1,
            detector: std::array::from_fn(|_| vec![0.0; bins]),
            level_db: std::array::from_fn(|_| vec![-200.0; bins]),
            wanted: std::array::from_fn(|_| vec![0.0; bins]),
            reduction: std::array::from_fn(|_| vec![0.0; bins]),
            prefix: vec![0.0; bins + 1],
            scratch: vec![0.0; bins],
            env_low,
            env_high,
            bin_band,
            seen: 0,
            primed: false,
            display: [[0.0; DISPLAY_BANDS]; 2],
        };
        e.reset();
        e
    }

    pub fn reset(&mut self) {
        for r in self.input.iter_mut().chain(self.output.iter_mut()) {
            r.fill(0.0);
        }
        for v in self.detector.iter_mut().chain(self.reduction.iter_mut()) {
            v.fill(0.0);
        }
        self.display = [[0.0; DISPLAY_BANDS]; 2];
        self.seen = 0;
        self.primed = false;
    }

    pub fn latency(&self) -> usize {
        self.fft_size - 1
    }

    pub fn latency_for(sample_rate: f64) -> usize {
        let order = if sample_rate <= 50000.0 { 11 } else if sample_rate <= 100000.0 { 12 } else { 13 };
        (1usize << order) - 1
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Stereo in place. `key` (left, right) replaces the detection input when given.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], s: &Settings, key: Option<(&[f32], &[f32])>) {
        let n = left.len().min(right.len());
        let keyed = key.is_some();
        let size = self.fft_size as u64;
        for i in 0..n {
            let slot = (self.seen % size) as usize;
            self.input[0][slot] = left[i];
            self.input[1][slot] = right[i];
            let (kl, kr) = match key {
                Some((a, b)) => (a.get(i).copied().unwrap_or(0.0), b.get(i).copied().unwrap_or(0.0)),
                None => (0.0, 0.0),
            };
            self.input[2][slot] = kl;
            self.input[3][slot] = kr;
            self.seen += 1;
            if self.seen % self.hop as u64 == 0 {
                self.process_frame(keyed, s);
            }
            let emit = self.seen as i64 - 1 - self.latency() as i64;
            left[i] = self.take(0, emit);
            right[i] = self.take(1, emit);
        }
    }

    fn take(&mut self, ch: usize, t: i64) -> f32 {
        if t < 0 {
            return 0.0;
        }
        let ring = &mut self.output[ch];
        let idx = (t as u64 % ring.len() as u64) as usize;
        let v = ring[idx];
        ring[idx] = 0.0;
        v
    }

    fn analyse(&mut self, ch: usize) {
        let n = self.fft_size;
        let start = self.seen as i64 - n as i64;
        for i in 0..n {
            let t = start + i as i64;
            let x = if t >= 0 { self.input[ch][(t as u64 % n as u64) as usize] } else { 0.0 };
            self.spectrum[ch][i] = C32::new(x * self.window[i], 0.0);
        }
        self.fft.forward(&mut self.spectrum[ch]);
    }

    fn add_frame(&mut self, ch: usize) {
        let n = self.fft_size;
        self.frame.copy_from_slice(&self.spectrum[ch]);
        self.fft.inverse(&mut self.frame);
        let start = self.seen as i64 - n as i64;
        let ring = &mut self.output[ch];
        let rs = ring.len() as u64;
        for i in 0..n {
            let t = start + i as i64;
            if t < 0 {
                continue;
            }
            ring[(t as u64 % rs) as usize] += self.frame[i].re * self.window[i] * self.hop_scale[(t as u64 % self.hop as u64) as usize];
        }
    }

    fn to_mid_side(&mut self, a: usize, b: usize) {
        for k in 0..self.bins {
            let (l, r) = (self.spectrum[a][k], self.spectrum[b][k]);
            self.spectrum[a][k] = l.add(r).scale(0.5);
            self.spectrum[b][k] = l.sub(r).scale(0.5);
        }
    }

    fn to_left_right(&mut self) {
        let n = self.fft_size;
        for k in 0..self.bins {
            let (m, s) = (self.spectrum[0][k], self.spectrum[1][k]);
            self.spectrum[0][k] = m.add(s);
            self.spectrum[1][k] = m.sub(s);
            if k > 0 && k < n / 2 {
                self.spectrum[0][n - k] = self.spectrum[0][k].conj();
                self.spectrum[1][n - k] = self.spectrum[1][k].conj();
            }
        }
    }

    fn update_focus(&mut self, s: &Settings) {
        let count = s.focus_count.min(MAX_FOCUS);
        let mut same = count as isize == self.focus_cached_count;
        for i in 0..count {
            if !same {
                break;
            }
            let (a, b) = (s.focus[i], self.focus_cached[i]);
            same = a.hz == b.hz && a.amount == b.amount && a.q == b.q;
        }
        if same {
            return;
        }
        self.focus_cached_count = count as isize;
        self.focus_cached = s.focus;
        for k in 0..self.bins {
            let hz = ((k as f64 * self.sample_rate / self.fft_size as f64) as f32).max(1.0);
            let mut octaves = 0.0f32;
            for band in &s.focus[..count] {
                let sigma = (0.7 / band.q.max(0.05)).clamp(0.06, 2.5);
                let d = (hz / band.hz.clamp(20.0, 20000.0)).log2() / sigma;
                octaves += 2.0 * band.amount.clamp(-1.0, 1.0) * (-0.5 * d * d).exp();
            }
            self.focus_weight[k] = 2f32.powf(octaves.clamp(-4.0, 4.0));
        }
    }

    fn process_frame(&mut self, keyed: bool, s: &Settings) {
        let n = self.fft_size;
        let bins = self.bins;
        for ch in 0..2 {
            self.analyse(ch);
        }
        if keyed {
            self.analyse(2);
            self.analyse(3);
        }
        if s.mid_side {
            self.to_mid_side(0, 1);
            if keyed {
                self.to_mid_side(2, 3);
            }
        }
        self.update_focus(s);

        let depth = s.depth.clamp(0.0, 1.0);
        let sharpness = s.sharpness.clamp(0.0, 1.0);
        let knee = 1.0 + 8.0 * s.selectivity.clamp(0.0, 1.0);
        let ceiling = 4.0 + 14.0 * depth;
        let ratio = (if s.hard { 3.0 } else { 1.5 }) * depth;
        let half_width = s.width_octaves.max(0.05) * 0.5;
        let centre = s.centre_hz.clamp(20.0, 20000.0);
        let spread = 2f64.powf(0.5 * (0.4 + (0.03 - 0.4) * sharpness as f64)) - 1.0;

        for ch in 0..2 {
            let src = if keyed { 2 + ch } else { ch };
            self.prefix[0] = 0.0;
            for k in 0..bins {
                let m = self.spectrum[src][k].abs() * self.magnitude_scale;
                let inst = m * m;
                let p = &mut self.detector[ch][k];
                *p = if self.primed { inst + (*p - inst) * self.detector_coef } else { inst };
                self.level_db[ch][k] = 10.0 * (*p + 1.0e-20).log10();
                self.prefix[k + 1] = self.prefix[k] + self.level_db[ch][k] as f64;
            }
            for k in 1..bins {
                let (lo, hi) = (self.env_low[k], self.env_high[k]);
                self.scratch[k] = ((self.prefix[hi + 1] - self.prefix[lo]) / (hi - lo + 1) as f64) as f32;
            }
            self.scratch[0] = self.scratch[1];
            self.prefix[0] = 0.0;
            for k in 0..bins {
                self.prefix[k + 1] = self.prefix[k] + self.level_db[ch][k].min(self.scratch[k] + 2.0) as f64;
            }
            self.wanted[ch][0] = 0.0;
            for k in 1..bins {
                let (lo, hi) = (self.env_low[k], self.env_high[k]);
                let surroundings = ((self.prefix[hi + 1] - self.prefix[lo]) / (hi - lo + 1) as f64) as f32;
                let level = self.level_db[ch][k];
                let excess = level - surroundings - knee;
                let mut cut = 0.0f32;
                if excess > 0.0 && depth > 0.0 {
                    cut = if s.hard { (ratio * excess).min(40.0) } else { ceiling * (ratio * excess / ceiling).tanh() };
                    let hz = (k as f64 * self.sample_rate / n as f64) as f32;
                    let distance = (hz.max(1.0) / centre).log2().abs();
                    let region = if distance <= half_width { 1.0 } else { (1.0 - (distance - half_width) * 2.0).max(0.0) };
                    let audible = ((level - s.floor_db) * 0.1).clamp(0.0, 1.0);
                    cut = (cut * region * audible * self.focus_weight[k]).min(40.0);
                }
                self.wanted[ch][k] = cut;
            }
            self.prefix[0] = 0.0;
            for k in 0..bins {
                self.prefix[k + 1] = self.prefix[k] + self.wanted[ch][k] as f64;
            }
            for k in 1..bins {
                let reach = ((k as f64 * spread).round() as usize).max(1);
                let lo = k.saturating_sub(reach).max(1);
                let hi = (k + reach).min(bins - 1);
                self.scratch[k] = ((self.prefix[hi + 1] - self.prefix[lo]) / (hi - lo + 1) as f64) as f32;
            }
            let (w, sc) = (&mut self.wanted[ch], &self.scratch);
            w[1..].copy_from_slice(&sc[1..]);
        }
        self.primed = true;

        if s.link > 0.0 {
            let link = s.link.min(1.0);
            for k in 1..bins {
                let deeper = self.wanted[0][k].max(self.wanted[1][k]);
                self.wanted[0][k] += (deeper - self.wanted[0][k]) * link;
                self.wanted[1][k] += (deeper - self.wanted[1][k]) * link;
            }
        }

        let attack_coef = (-self.hop_seconds / s.attack_s.clamp(0.001, 0.5)).exp();
        let release_coef = (-self.hop_seconds / s.release_s.clamp(0.02, 2.0)).exp();
        let mix = s.mix.clamp(0.0, 1.0);
        let mut shown = [[0.0f32; DISPLAY_BANDS]; 2];
        for ch in 0..2 {
            for k in 0..bins {
                let target = self.wanted[ch][k];
                let cur = &mut self.reduction[ch][k];
                let coef = if target > *cur { attack_coef } else { release_coef };
                *cur = target + (*cur - target) * coef;
                let cut_gain = (-0.115129255 * *cur).exp();
                let gain = if s.delta { 1.0 - cut_gain } else { 1.0 + (cut_gain - 1.0) * mix };
                self.spectrum[ch][k] = self.spectrum[ch][k].scale(gain);
                if k > 0 && k < n / 2 {
                    self.spectrum[ch][n - k] = self.spectrum[ch][k].conj();
                }
                let band = self.bin_band[k];
                if band >= 0 {
                    let b = band as usize;
                    shown[ch][b] = shown[ch][b].max(*cur);
                }
            }
        }
        if s.mid_side {
            self.to_left_right();
        }
        for ch in 0..2 {
            self.add_frame(ch);
        }
        for (ch, sh) in shown.iter_mut().enumerate() {
            let mut last = 0.0f32;
            let mut last_band: i32 = -1;
            for k in 1..bins {
                let band = self.bin_band[k];
                if band < 0 {
                    continue;
                }
                for fill in (last_band + 1)..band {
                    sh[fill as usize] = last.max(sh[band as usize]);
                }
                last = sh[band as usize];
                last_band = band;
            }
            self.display[ch] = *sh;
        }
    }
}
