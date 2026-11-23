#![allow(dead_code)]

use comp_harmonic::{idx, HarmonicCompressor};
use olm_api::Module;
use std::f64::consts::PI;

pub const SR: f32 = 48000.0;

pub struct Rng(pub u32);
impl Rng {
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

/// A module at `sr` with the given (index, value) settings, prepared.
pub fn module(sr: f32, block: usize, settings: &[(usize, f32)]) -> HarmonicCompressor {
    let mut m = HarmonicCompressor::default();
    for &(i, v) in settings {
        m.set_param(i, v);
    }
    m.prepare(sr, block);
    m
}

/// Neutral: 1x, nothing compresses (ratio 1), no harmonics.
pub fn neutral() -> Vec<(usize, f32)> {
    vec![(idx::OS_MODE, 0.0), (idx::RATIO, 1.0), (idx::THRESHOLD, 0.0)]
}

pub fn with(base: &[(usize, f32)], more: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let mut v = base.to_vec();
    v.extend_from_slice(more);
    v
}

/// Processes stereo signals in blocks of `block`; returns the outputs.
pub fn run(m: &mut HarmonicCompressor, l: &[f32], r: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    let mut s = 0;
    while s < ol.len() {
        let e = (s + block).min(ol.len());
        m.process(&mut ol[s..e], &mut or[s..e]);
        s = e;
    }
    (ol, or)
}

pub fn run_keyed(m: &mut HarmonicCompressor, l: &[f32], r: &[f32], kl: &[f32], kr: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    let mut s = 0;
    while s < ol.len() {
        let e = (s + block).min(ol.len());
        m.process_keyed(&mut ol[s..e], &mut or[s..e], &kl[s..e], &kr[s..e]);
        s = e;
    }
    (ol, or)
}

pub fn sine(freq: f64, amp: f64, sr: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * freq * i as f64 / sr as f64).sin()) as f32).collect()
}

pub fn noise(seed: u32, amp: f32, n: usize) -> Vec<f32> {
    let mut r = Rng(seed);
    (0..n).map(|_| r.next() * amp).collect()
}

/// Amplitude of the component at `freq` over `x` (a whole number of cycles assumed), Hann-free DFT.
pub fn amplitude(x: &[f32], freq: f64, sr: f32) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 2.0 * PI * freq * i as f64 / sr as f64;
        re += v as f64 * w.cos();
        im += v as f64 * w.sin();
    }
    2.0 * (re * re + im * im).sqrt() / x.len() as f64
}

/// Power spectrum by bin (DFT over the whole slice; bins of sr/len Hz). O(n * bins).
pub fn bin_power(x: &[f32], bin: usize) -> f64 {
    let n = x.len();
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 2.0 * PI * (bin * i) as f64 / n as f64;
        re += v as f64 * w.cos();
        im += v as f64 * w.sin();
    }
    (re * re + im * im) / (n as f64 * n as f64)
}

pub fn db(x: f64) -> f64 {
    20.0 * x.max(1e-30).log10()
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / x.len() as f64).sqrt()
}

pub fn all_finite(x: &[f32]) -> bool {
    x.iter().all(|v| v.is_finite())
}
