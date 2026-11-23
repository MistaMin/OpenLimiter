//! Requantiser: dither, noise shaping and word-length reduction.
//!
//! Built from MASTER_BRIEF.md 4.6 and the tables it points to: SPEC_LIMITERS.md 56.3 and 56.4 (source A),
//! 70.2 (source B), 68.3 to 68.6 (source D) and the source-H table in `spec/params/` (its section 6).
//!
//! Signal path per channel (all arithmetic in f64):
//!
//! ```text
//! x -> [DC filter] -> v = x + sum c_k e[n-k] -> + TPDF dither -> round to the word length -> [peak limit] -> y
//!                                     e[n] = round(v + d) - v   (rounding error plus dither, fed back)
//! ```
//!
//! The error feedback gives the output noise the spectrum |NTF|^2 with NTF = 1 + sum c_k z^-k; the taps are
//! designed offline by `design/design_curves.py` to hit the measured band levels (see `curves.rs`).
//! Zero latency. With word length Off and the DC filter off the module does not touch the samples.

use olm_api::{Kind, Meters, Module, ParamDesc};

mod curves;

/// Parameter indices.
pub const P_WORD_LENGTH: usize = 0;
pub const P_DITHER_AMOUNT: usize = 1;
pub const P_SHAPING: usize = 2;
pub const P_SHAPING_DEPTH: usize = 3;
pub const P_AUTO_BLANK: usize = 4;
pub const P_PEAK_LIMIT: usize = 5;
pub const P_DC_FILTER: usize = 6;
pub const P_KEEP_EXACT: usize = 7;

static WORD_LABELS: [&str; 8] = ["Off", "24 bit", "22 bit", "20 bit", "18 bit", "16 bit", "12 bit", "8 bit"];
const WORD_BITS: [u32; 8] = [0, 24, 22, 20, 18, 16, 12, 8];

/// Shaping curves, own names. Index 0 is no shaping.
/// Tilt 1, Tilt 2, Top Octave, Tilt Soft: the four shaped types of SPEC_LIMITERS.md 56.4 (designed at depth 50 % and 100 %).
/// Rise 1 to Rise 6: the six shaping strengths of SPEC_LIMITERS.md 68.3 (lowest to strongest).
/// Valley 1 to Valley 3: the three curves of the source-H table, section 6 (mildest to strongest).
pub static SHAPING_LABELS: [&str; 14] = [
    "Off", "Tilt 1", "Tilt 2", "Top Octave", "Tilt Soft", "Rise 1", "Rise 2", "Rise 3", "Rise 4", "Rise 5", "Rise 6",
    "Valley 1", "Valley 2", "Valley 3",
];

static PARAMS: [ParamDesc; 8] = [
    ParamDesc::choice("wordLength", "Word Length", &WORD_LABELS, 1),
    ParamDesc::new("ditherAmount", "Dither Amount", 0.0, 125.0, 100.0, Kind::Percent),
    ParamDesc::choice("shaping", "Noise Shaping", &SHAPING_LABELS, 0),
    ParamDesc::new("shapingDepth", "Shaping Depth", 0.0, 100.0, 100.0, Kind::Percent),
    ParamDesc::toggle("autoBlank", "Auto-Blank", false),
    ParamDesc::new("peakLimit", "Noise Peak Limit (steps, 0 = off)", 0.0, 32.0, 0.0, Kind::Plain).steps(33),
    ParamDesc::toggle("dcFilter", "DC Filter", false),
    ParamDesc::toggle("keepExact", "Silence Stays Silent", false),
];

/// Largest filter length (Rise 6 uses all of it). Power of two for the ring buffer.
const MAX_TAPS: usize = 256;
/// Auto-blank switches the dither off after this much silence (SPEC_LIMITERS.md 68.5: 0.70 s).
const BLANK_SECONDS: f64 = 0.70;
/// An input sample counts as silent at or below this magnitude (about -180.6 dBFS). Free choice.
const SILENCE: f64 = 1.0 / 1_073_741_824.0;
/// DC filter time constant in seconds (fitted to the DC and settling figures of SPEC_LIMITERS.md 68.4; corner about 0.71 Hz).
const DC_TAU: f64 = 0.225;

#[derive(Clone)]
struct Channel {
    /// Error history, written twice (at `pos` and `pos + MAX_TAPS`) so the taps see one contiguous slice,
    /// newest first.
    hist: [f64; 2 * MAX_TAPS],
    pos: usize,
    rng: u64,
    dc_x1: f64,
    dc_y1: f64,
}

impl Channel {
    fn new(seed: u64) -> Self {
        Self { hist: [0.0; 2 * MAX_TAPS], pos: 0, rng: seed, dc_x1: 0.0, dc_y1: 0.0 }
    }

    fn clear_errors(&mut self) {
        self.hist = [0.0; 2 * MAX_TAPS];
        self.pos = 0;
    }

    #[inline]
    fn push(&mut self, e: f64) {
        self.pos = (self.pos + MAX_TAPS - 1) & (MAX_TAPS - 1);
        self.hist[self.pos] = e;
        self.hist[self.pos + MAX_TAPS] = e;
    }

    /// xorshift64* step.
    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    #[inline]
    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    #[inline]
    fn dc(&mut self, x: f64, r: f64) -> f64 {
        let y = x - self.dc_x1 + r * self.dc_y1;
        self.dc_x1 = x;
        self.dc_y1 = if y.is_finite() { y } else { 0.0 };
        y
    }
}

const SEEDS: [u64; 2] = [0x9E37_79B9_7F4A_7C15, 0xD1B5_4A32_D192_ED03];

/// Dither and word-length reduction module.
pub struct Requantiser {
    sample_rate: f64,
    word: usize,
    amount: f64,
    shaping: usize,
    depth: f64,
    auto_blank: bool,
    peak_limit: f64,
    dc_on: bool,
    keep_exact: bool,

    q: f64,
    inv_q: f64,
    coef: [f64; MAX_TAPS],
    ntaps: usize,
    dc_r: f64,
    blank_after: u64,

    ch: [Channel; 2],
    silent_run: u64,
    blanked: bool,
    bits_or: u32,
    bits_and: u32,
}

impl Default for Requantiser {
    fn default() -> Self {
        let mut s = Self {
            sample_rate: 48_000.0,
            word: 1,
            amount: 1.0,
            shaping: 0,
            depth: 1.0,
            auto_blank: false,
            peak_limit: 0.0,
            dc_on: false,
            keep_exact: false,
            q: 0.0,
            inv_q: 0.0,
            coef: [0.0; MAX_TAPS],
            ntaps: 0,
            dc_r: 0.0,
            blank_after: 0,
            ch: [Channel::new(SEEDS[0]), Channel::new(SEEDS[1])],
            silent_run: 0,
            blanked: false,
            bits_or: 0,
            bits_and: 0x00FF_FFFF,
        };
        s.update_rate();
        s.update_word();
        s.update_curve();
        s
    }
}

/// (taps at depth 50 % if separately designed, taps at depth 100 %).
fn curve_taps(index: usize) -> (Option<&'static [f64]>, &'static [f64]) {
    use curves::*;
    let table: [(Option<&'static [f64]>, &'static [f64]); 13] = [
        (Some(&TILT1_50), &TILT1_100),
        (Some(&TILT2_50), &TILT2_100),
        (Some(&TOP_50), &TOP_100),
        (Some(&SOFT_50), &SOFT_100),
        (None, &RISE1),
        (None, &RISE2),
        (None, &RISE3),
        (None, &RISE4),
        (None, &RISE5),
        (None, &RISE6),
        (None, &VALLEY1),
        (None, &VALLEY2),
        (None, &VALLEY3),
    ];
    if index == 0 || index > table.len() {
        return (None, &[]);
    }
    table[index - 1]
}

/// The error-feedback taps for a shaping curve at a depth (0..1), written to `coef`; returns
/// how many are in use. Depth law (free choice): 0 % is flat. Curves designed at 50 % and
/// 100 % interpolate their taps piecewise linearly through 0, 50 and 100 %; the other curves
/// scale their taps with the depth.
fn shaping_taps(shaping: usize, depth: f64, coef: &mut [f64; MAX_TAPS]) -> usize {
    let (half, full) = curve_taps(shaping);
    let d = depth.clamp(0.0, 1.0);
    *coef = [0.0; MAX_TAPS];
    let n = full.len().min(MAX_TAPS);
    for k in 0..n {
        coef[k] = if let Some(h) = half {
            let h_k = h.get(k).copied().unwrap_or(0.0);
            if d <= 0.5 {
                h_k * (d / 0.5)
            } else {
                h_k + (full[k] - h_k) * ((d - 0.5) / 0.5)
            }
        } else {
            full[k] * d
        };
    }
    if d > 0.0 {
        n
    } else {
        0
    }
}

/// Editor read-outs. Pure functions of the parameters; the module does not use them.
pub mod view {
    use super::{shaping_taps, MAX_TAPS, WORD_BITS};

    /// The noise-shaping filter's magnitude |1 + sum c_k z^-k| in dB relative to flat (unshaped)
    /// dither, at each frequency of `freqs` (Hz), for `shaping` (the parameter's index) at
    /// `depth_percent`. Writes `out[i]` for `freqs[i]`.
    pub fn shaping_response_db(shaping: usize, depth_percent: f32, sample_rate: f32, freqs: &[f32], out: &mut [f32]) {
        let mut coef = [0.0; MAX_TAPS];
        let n = shaping_taps(shaping, depth_percent as f64 / 100.0, &mut coef);
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 { sample_rate as f64 } else { 48_000.0 };
        for (o, &f) in out.iter_mut().zip(freqs) {
            let w = std::f64::consts::TAU * f as f64 / sr;
            let (mut re, mut im) = (1.0, 0.0);
            for (k, c) in coef[..n].iter().enumerate() {
                let a = w * (k + 1) as f64;
                re += c * a.cos();
                im -= c * a.sin();
            }
            *o = (10.0 * (re * re + im * im).max(1e-30).log10()) as f32;
        }
    }

    /// The noise power the shaping adds over flat dither, as a factor: 1 + sum c_k^2 (the mean
    /// of |NTF|^2 over 0..Nyquist).
    pub fn shaping_power_gain(shaping: usize, depth_percent: f32) -> f64 {
        let mut coef = [0.0; MAX_TAPS];
        let n = shaping_taps(shaping, depth_percent as f64 / 100.0, &mut coef);
        1.0 + coef[..n].iter().map(|c| c * c).sum::<f64>()
    }

    /// Bits of the output word for the `wordLength` index (0 = Off).
    pub fn word_bits(word_length: usize) -> u32 {
        WORD_BITS[word_length.min(WORD_BITS.len() - 1)]
    }
}

impl Requantiser {
    fn update_rate(&mut self) {
        let sr = if self.sample_rate.is_finite() && self.sample_rate > 0.0 { self.sample_rate } else { 48_000.0 };
        self.dc_r = (-1.0 / (DC_TAU * sr)).exp();
        self.blank_after = (BLANK_SECONDS * sr).round() as u64;
    }

    fn update_word(&mut self) {
        let bits = WORD_BITS[self.word.min(WORD_BITS.len() - 1)];
        if bits == 0 {
            self.q = 0.0;
            self.inv_q = 0.0;
        } else {
            self.q = (2.0f64).powi(-(bits as i32 - 1));
            self.inv_q = (2.0f64).powi(bits as i32 - 1);
        }
    }

    /// Depth law (free choice): 0 % is flat. Curves designed at 50 % and 100 % interpolate their taps piecewise
    /// linearly through 0, 50 and 100 %; the other curves scale their taps with the depth.
    fn update_curve(&mut self) {
        self.ntaps = shaping_taps(self.shaping, self.depth, &mut self.coef);
    }

    /// Bits of the 24-bit output word (bit 23 = most significant) that have been both 0 and 1 since the last
    /// clear. With word length Off nothing is recorded.
    pub fn toggled_bits(&self) -> u32 {
        self.bits_or & !self.bits_and & 0x00FF_FFFF
    }

    /// How many bits of a 24-bit word are in use, counted from the top down to the lowest bit that toggled.
    pub fn bits_in_use(&self) -> u32 {
        let t = self.toggled_bits();
        if t == 0 { 0 } else { 24 - t.trailing_zeros() }
    }

    pub fn clear_bit_activity(&mut self) {
        self.bits_or = 0;
        self.bits_and = 0x00FF_FFFF;
    }

    /// True while auto-blank holds the output at digital silence.
    pub fn is_blanked(&self) -> bool {
        self.blanked
    }

    #[inline]
    fn quantise(&mut self, c: usize, x: f64) -> f64 {
        let q = self.q;
        let inv_q = self.inv_q;
        let nt = self.ntaps;
        let amount = self.amount;
        let peak = self.peak_limit;
        let keep = self.keep_exact;
        let coef = &self.coef;
        let chn = &mut self.ch[c];

        let mut fb = 0.0;
        let h = &chn.hist[chn.pos..chn.pos + nt];
        for (a, b) in coef[..nt].iter().zip(h) {
            fb += a * b;
        }
        let v = x + fb;
        if !v.is_finite() {
            chn.push(0.0);
            return x;
        }
        if keep && (x * inv_q).fract() == 0.0 {
            // the sample is already exact at this word length: pass it untouched and feed back no error
            chn.push(0.0);
            return x;
        }
        let d = if amount > 0.0 { amount * q * (chn.uniform() - chn.uniform()) } else { 0.0 };
        let k = ((v + d) * inv_q).round();
        let e = k * q - v;
        chn.push(if e.is_finite() { e } else { 0.0 });
        let mut k_out = k;
        if peak > 0.0 {
            let centre = (x * inv_q).round();
            k_out = k_out.max(centre - peak).min(centre + peak);
        }
        k_out * q
    }

    #[inline]
    fn note_bits(&mut self, y: f64) {
        let w = (y * 8_388_608.0).round().max(-8_388_608.0).min(8_388_607.0) as i32;
        let w = (w as u32) & 0x00FF_FFFF;
        self.bits_or |= w;
        self.bits_and &= w;
    }
}

impl Module for Requantiser {
    fn params(&self) -> &'static [ParamDesc] {
        &PARAMS
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate as f64;
        self.update_rate();
        self.reset();
    }

    fn reset(&mut self) {
        self.ch = [Channel::new(SEEDS[0]), Channel::new(SEEDS[1])];
        self.silent_run = 0;
        self.blanked = false;
        self.clear_bit_activity();
    }

    fn set_param(&mut self, index: usize, value: f32) {
        let Some(desc) = PARAMS.get(index) else { return };
        let v = if value.is_finite() { value.clamp(desc.min, desc.max) } else { desc.default } as f64;
        if index == P_WORD_LENGTH {
            self.word = v.round() as usize;
            self.update_word();
        } else if index == P_DITHER_AMOUNT {
            self.amount = v / 100.0;
        } else if index == P_SHAPING {
            self.shaping = v.round() as usize;
            self.update_curve();
        } else if index == P_SHAPING_DEPTH {
            self.depth = v / 100.0;
            self.update_curve();
        } else if index == P_AUTO_BLANK {
            self.auto_blank = v >= 0.5;
            if !self.auto_blank {
                self.blanked = false;
            }
        } else if index == P_PEAK_LIMIT {
            self.peak_limit = v.round();
        } else if index == P_DC_FILTER {
            self.dc_on = v >= 0.5;
        } else if index == P_KEEP_EXACT {
            self.keep_exact = v >= 0.5;
        }
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        let r = self.dc_r;
        if self.q == 0.0 {
            // word length Off: no requantisation and no dither
            if self.dc_on {
                for i in 0..n {
                    left[i] = self.ch[0].dc(left[i] as f64, r) as f32;
                    right[i] = self.ch[1].dc(right[i] as f64, r) as f32;
                }
            }
            return;
        }
        for i in 0..n {
            let mut xl = left[i] as f64;
            let mut xr = right[i] as f64;
            if self.auto_blank {
                if xl.abs() <= SILENCE && xr.abs() <= SILENCE {
                    self.silent_run = self.silent_run.saturating_add(1);
                } else {
                    self.silent_run = 0;
                }
                let blank = self.silent_run > self.blank_after;
                if blank && !self.blanked {
                    self.ch[0].clear_errors();
                    self.ch[1].clear_errors();
                }
                self.blanked = blank;
            }
            if self.dc_on {
                xl = self.ch[0].dc(xl, r);
                xr = self.ch[1].dc(xr, r);
            }
            let (yl, yr) = if self.blanked { (0.0, 0.0) } else { (self.quantise(0, xl), self.quantise(1, xr)) };
            self.note_bits(yl);
            self.note_bits(yr);
            left[i] = yl as f32;
            right[i] = yr as f32;
        }
    }

    fn meters(&self) -> Meters {
        Meters::default()
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;

    #[test]
    fn the_response_read_out_is_the_filter_the_module_runs() {
        for shaping in 0..SHAPING_LABELS.len() {
            for depth in [0.0f32, 30.0, 50.0, 80.0, 100.0] {
                let mut q = Requantiser::default();
                q.set_param(P_SHAPING, shaping as f32);
                q.set_param(P_SHAPING_DEPTH, depth);
                let freqs = [50.0f32, 1000.0, 6000.0, 15000.0, 21000.0];
                let mut got = [0.0f32; 5];
                view::shaping_response_db(shaping, depth, 48_000.0, &freqs, &mut got);
                for (f, g) in freqs.iter().zip(got) {
                    let w = std::f64::consts::TAU * *f as f64 / 48_000.0;
                    let (mut re, mut im) = (1.0f64, 0.0f64);
                    for k in 0..q.ntaps {
                        re += q.coef[k] * (w * (k + 1) as f64).cos();
                        im -= q.coef[k] * (w * (k + 1) as f64).sin();
                    }
                    let want = 10.0 * (re * re + im * im).log10();
                    assert!((g as f64 - want).abs() < 1e-3, "shaping {shaping} depth {depth} at {f} Hz: {g} vs {want}");
                }
                if shaping == 0 || depth == 0.0 {
                    assert!(got.iter().all(|g| g.abs() < 1e-6));
                    assert!((view::shaping_power_gain(shaping, depth) - 1.0).abs() < 1e-12);
                }
            }
        }
        assert_eq!(view::word_bits(5), 16);
    }
}
