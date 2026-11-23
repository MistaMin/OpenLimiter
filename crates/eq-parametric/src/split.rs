//! Transient/tonal separator (OpenEQ `SplitSeparator`): median-filter separation on a short-time
//! spectrum with a soft mask. A sustained partial holds its level from frame to frame, so a median
//! over time keeps it; a short event is wide in frequency, so a median over frequency keeps it.
//! Only the transient stream is resynthesised; the tonal stream is the delayed input minus it, so
//! the two always add back to the delayed input. Both channels share one mask. Hann windows, 4x
//! overlap, a frame of about 21 ms at any rate. `process` never allocates.

use eq_core::cx::C32;
use eq_core::fft::Fft;

pub const TIME_FRAMES: usize = 9;
pub const FUTURE_FRAMES: usize = 3;
pub const FREQUENCY_SPAN: usize = 13;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// 0..1; higher asks for a stronger case before a bin counts as transient.
    pub separation: f32,
    /// 0..1; how long a bin stays transient after its onset.
    pub decay: f32,
    /// 0..50 ms.
    pub smoothing_ms: f32,
    pub source_scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { separation: 0.5, decay: 0.5, smoothing_ms: 5.0, source_scale: 1.0 }
    }
}

/// Source presets scale the separation bias: Full Mix, Drums, Percussive, Instrument, Vocal.
pub const SOURCE_SCALE: [f32; 5] = [1.0, 0.8, 0.7, 1.1, 1.4];

pub struct SplitSeparator {
    sample_rate: f64,
    fft_size: usize,
    hop: usize,
    bins: usize,
    latency: usize,
    delay_size: usize,
    fft: Fft,
    window: Vec<f32>,
    hop_scale: Vec<f32>,
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
    delay: [Vec<f32>; 2],
    spectra: [Vec<Vec<C32>>; 2],
    magnitudes: Vec<Vec<f32>>,
    frame: Vec<C32>,
    raw_mask: Vec<f32>,
    mirrored: Vec<f32>,
    held: Vec<f32>,
    mask: Vec<f32>,
    newest: usize,
    seen: u64,
}

impl SplitSeparator {
    pub fn new(sample_rate: f64) -> Self {
        let sr = if sample_rate > 0.0 { sample_rate } else { 44100.0 };
        let order = (10 + (sr / 48000.0).log2().round() as i32).clamp(10, 13) as u32;
        let n = 1usize << order;
        let hop = n / 4;
        let bins = n / 2 + 1;
        let latency = n - 1 + FUTURE_FRAMES * hop;
        let window: Vec<f32> = (0..n).map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())).collect();
        let mut hop_scale = vec![1.0f32; hop];
        for (r, hs) in hop_scale.iter_mut().enumerate() {
            let s: f32 = (0..n / hop).map(|k| window[r + k * hop] * window[r + k * hop]).sum();
            *hs = if s > 1.0e-8 { 1.0 / s } else { 0.0 };
        }
        let delay_size = (latency + 2).next_power_of_two();
        SplitSeparator {
            sample_rate: sr,
            fft_size: n,
            hop,
            bins,
            latency,
            delay_size,
            fft: Fft::new(order),
            window,
            hop_scale,
            input: std::array::from_fn(|_| vec![0.0; n]),
            output: std::array::from_fn(|_| vec![0.0; 2 * n]),
            delay: std::array::from_fn(|_| vec![0.0; delay_size]),
            spectra: std::array::from_fn(|_| (0..TIME_FRAMES).map(|_| vec![C32::ZERO; bins]).collect()),
            magnitudes: (0..TIME_FRAMES).map(|_| vec![0.0; bins]).collect(),
            frame: vec![C32::ZERO; n],
            raw_mask: vec![0.0; bins],
            mirrored: vec![0.0; bins + 2 * (FREQUENCY_SPAN / 2)],
            held: vec![0.0; bins],
            mask: vec![0.0; bins],
            newest: 0,
            seen: 0,
        }
    }

    pub fn reset(&mut self) {
        for v in self.input.iter_mut().chain(self.output.iter_mut()).chain(self.delay.iter_mut()) {
            v.fill(0.0);
        }
        for ch in self.spectra.iter_mut() {
            for s in ch.iter_mut() {
                s.fill(C32::ZERO);
            }
        }
        for m in self.magnitudes.iter_mut() {
            m.fill(0.0);
        }
        self.held.fill(0.0);
        self.mask.fill(0.0);
        self.newest = 0;
        self.seen = 0;
    }

    /// Samples between an input sample and the same sample in the streams.
    pub fn latency(&self) -> usize {
        self.latency
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Stereo: writes the transient and tonal streams, both delayed by `latency()`.
    #[allow(clippy::too_many_arguments)]
    pub fn process(&mut self, in_l: &[f32], in_r: &[f32], tl: &mut [f32], tr: &mut [f32], sl: &mut [f32], sr: &mut [f32], s: &Settings) {
        let n = in_l.len().min(in_r.len()).min(tl.len()).min(tr.len()).min(sl.len()).min(sr.len());
        let (size, dmask) = (self.fft_size as u64, (self.delay_size - 1) as u64);
        for i in 0..n {
            let ii = (self.seen % size) as usize;
            let di = (self.seen & dmask) as usize;
            self.input[0][ii] = in_l[i];
            self.input[1][ii] = in_r[i];
            self.delay[0][di] = in_l[i];
            self.delay[1][di] = in_r[i];
            self.seen += 1;
            if self.seen % self.hop as u64 == 0 {
                self.synthesise(s);
            }
            let emit = self.seen as i64 - 1 - self.latency as i64;
            for ch in 0..2 {
                let t = self.take(ch, emit);
                let dry = if emit >= 0 { self.delay[ch][(emit as u64 & dmask) as usize] } else { 0.0 };
                if ch == 0 {
                    tl[i] = t;
                    sl[i] = dry - t;
                } else {
                    tr[i] = t;
                    sr[i] = dry - t;
                }
            }
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

    fn analyse(&mut self, ch: usize, slot: usize) {
        let n = self.fft_size;
        let start = self.seen as i64 - n as i64;
        for i in 0..n {
            let t = start + i as i64;
            let x = if t >= 0 { self.input[ch][(t as u64 % n as u64) as usize] } else { 0.0 };
            self.frame[i] = C32::new(x * self.window[i], 0.0);
        }
        self.fft.forward(&mut self.frame);
        let bins = self.bins;
        self.spectra[ch][slot][..bins].copy_from_slice(&self.frame[..bins]);
    }

    fn add_frame(&mut self, ch: usize, slot: usize, frame_start: i64) {
        let n = self.fft_size;
        for b in 0..self.bins {
            let v = self.spectra[ch][slot][b].scale(self.mask[b]);
            self.frame[b] = v;
            if b > 0 && b < n / 2 {
                self.frame[n - b] = v.conj();
            }
        }
        self.fft.inverse(&mut self.frame);
        let ring = &mut self.output[ch];
        let rs = ring.len() as u64;
        for i in 0..n {
            let t = frame_start + i as i64;
            if t < 0 {
                continue;
            }
            ring[(t as u64 % rs) as usize] += self.frame[i].re * self.window[i] * self.hop_scale[(t as u64 % self.hop as u64) as usize];
        }
    }

    fn synthesise(&mut self, s: &Settings) {
        self.newest = (self.newest + 1) % TIME_FRAMES;
        let newest = self.newest;
        self.analyse(0, newest);
        self.analyse(1, newest);
        for b in 0..self.bins {
            let p = self.spectra[0][newest][b].norm() + self.spectra[1][newest][b].norm();
            self.magnitudes[newest][b] = p.sqrt();
        }
        let centre = (newest + TIME_FRAMES - FUTURE_FRAMES) % TIME_FRAMES;
        let bias = 2f32.powf((s.separation.clamp(0.0, 1.0) - 0.5) * 6.0) * s.source_scale.max(0.1);
        let bias2 = bias * bias;
        let half = (FREQUENCY_SPAN / 2) as i64;
        let bins = self.bins as i64;
        for i in -half..bins + half {
            let mut idx = i;
            if idx < 0 {
                idx = -idx;
            }
            if idx >= bins {
                idx = 2 * (bins - 1) - idx;
            }
            self.mirrored[(i + half) as usize] = self.magnitudes[centre][idx as usize];
        }
        for b in 0..self.bins {
            let mut over_time = [0.0f32; TIME_FRAMES];
            for (f, o) in over_time.iter_mut().enumerate() {
                *o = self.magnitudes[f][b];
            }
            let mut over_freq = [0.0f32; FREQUENCY_SPAN];
            over_freq.copy_from_slice(&self.mirrored[b..b + FREQUENCY_SPAN]);
            over_time.sort_unstable_by(|a, c| a.partial_cmp(c).unwrap_or(std::cmp::Ordering::Equal));
            over_freq.sort_unstable_by(|a, c| a.partial_cmp(c).unwrap_or(std::cmp::Ordering::Equal));
            let tonal = over_time[TIME_FRAMES / 2];
            let transient = over_freq[FREQUENCY_SPAN / 2];
            let tp = transient * transient;
            let sp = tonal * tonal * bias2;
            self.raw_mask[b] = tp / (tp + sp + 1.0e-20);
        }
        let hop_seconds = self.hop as f64 / self.sample_rate;
        let decay_seconds = 0.150 * s.decay as f64 * s.decay as f64;
        let hold = if decay_seconds > 1.0e-4 { (-hop_seconds / decay_seconds).exp() as f32 } else { 0.0 };
        let smooth = s.smoothing_ms as f64 * 0.001;
        let fall = if smooth > 1.0e-4 { (-hop_seconds / smooth).exp() as f32 } else { 0.0 };
        let rise = if smooth > 1.0e-4 { (-4.0 * hop_seconds / smooth).exp() as f32 } else { 0.0 };
        let last = self.bins - 1;
        for b in 0..self.bins {
            let below = self.raw_mask[b.saturating_sub(1)];
            let above = self.raw_mask[(b + 1).min(last)];
            let blurred = 0.25 * below + 0.5 * self.raw_mask[b] + 0.25 * above;
            self.held[b] = blurred.max(self.held[b] * hold);
            let c = if self.held[b] > self.mask[b] { rise } else { fall };
            self.mask[b] = self.held[b] + c * (self.mask[b] - self.held[b]);
        }
        let frame_start = self.seen as i64 - self.fft_size as i64 - (FUTURE_FRAMES * self.hop) as i64;
        self.add_frame(0, centre, frame_start);
        self.add_frame(1, centre, frame_start);
    }
}
