//! The history's spectrum: the chain's output, one column per editor tick, drawn as a
//! scrolling spectrogram (2D) or as a waterfall of spectra receding in time (3D).

use std::collections::VecDeque;

use openplugin::dri::goodlookindri::color::Color as C;

use super::analyser::Spectrum;
use super::raster::{blend, rgb, Target, R};
use super::theme::Theme;
use crate::shared::Shared;

pub const BANDS: usize = 160;
/// Columns kept: 4 s at the editor's 30 ticks per second, the levels' window.
pub const COLS: usize = 120;
pub const LO: f32 = 20.0;
pub const HI: f32 = 20_000.0;
pub const TOP_DB: f32 = -6.0;
pub const SPAN_DB: f32 = 84.0;
/// Slices of the waterfall, each the average of `SLICE_S` seconds, and how much of the plot the
/// front slice takes. Averaged and smoothed, a slice reads as a shape instead of flicker.
const SLICES: usize = 30;
const SLICE_S: f32 = 0.2;
/// Bands each side averaged together: about a third of an octave (16 bands per octave).
const SMOOTH_BANDS: usize = 3;
const FRONT_W: f32 = 0.72;
const FRONT_H: f32 = 0.62;
pub const TICKS: [(f32, &str); 8] = [(50.0, "50"), (100.0, "100"), (200.0, "200"), (500.0, "500"), (1000.0, "1k"), (2000.0, "2k"), (5000.0, "5k"), (10_000.0, "10k")];

pub struct SpecHist {
    spec: Spectrum,
    buf: Vec<f32>,
    cols: VecDeque<Vec<f32>>,
    head: usize,
    /// The waterfall: finished slices (dB, newest last) and the power summed for the next one.
    slices: VecDeque<Vec<f32>>,
    acc: Vec<f32>,
    acc_n: u32,
    acc_t: f32,
}

impl Default for SpecHist {
    fn default() -> Self {
        let mut spec = Spectrum::new(4096, BANDS, LO, HI);
        spec.decay_db_s = 150.0;
        Self { spec, buf: vec![0.0; 4096], cols: VecDeque::with_capacity(COLS + 1), head: usize::MAX, slices: VecDeque::with_capacity(SLICES + 1), acc: vec![0.0; BANDS], acc_n: 0, acc_t: 0.0 }
    }
}

impl SpecHist {
    /// Adds a column when new audio arrived; true when there is something new to draw.
    pub fn tick(&mut self, shared: &Shared, dt: f32) -> bool {
        let head = shared.out_head();
        let rate = shared.timing().1;
        // A stopped transport freezes the picture instead of scrolling the last spectrum.
        if head == self.head || rate <= 0.0 {
            return false;
        }
        self.head = head;
        shared.out_latest(&mut self.buf);
        self.spec.update(&self.buf, None, rate, dt);
        let mut col = if self.cols.len() == COLS { self.cols.pop_front().unwrap_or_default() } else { Vec::new() };
        col.clear();
        col.extend_from_slice(&self.spec.db);
        self.cols.push_back(col);
        for (a, db) in self.acc.iter_mut().zip(&self.spec.db) {
            *a += 10f32.powf(db / 10.0);
        }
        self.acc_n += 1;
        self.acc_t += dt;
        if self.acc_t >= SLICE_S {
            let slice = self.averaged();
            if self.slices.len() == SLICES {
                self.slices.pop_front();
            }
            self.slices.push_back(slice);
            self.acc.fill(0.0);
            (self.acc_n, self.acc_t) = (0, 0.0);
        }
        true
    }

    /// The slice being summed, in dB, smoothed across neighbouring bands.
    fn averaged(&self) -> Vec<f32> {
        let n = self.acc_n.max(1) as f32;
        (0..BANDS)
            .map(|b| {
                let (lo, hi) = (b.saturating_sub(SMOOTH_BANDS), (b + SMOOTH_BANDS).min(BANDS - 1));
                let p = self.acc[lo..=hi].iter().sum::<f32>() / (n * (hi - lo + 1) as f32);
                10.0 * p.max(1e-14).log10()
            })
            .collect()
    }

    /// Level of `band` (fractional) in the column `age` ticks old, 0..1 of the scale.
    fn level(&self, age: usize, band: f32) -> Option<f32> {
        let col = self.cols.get(self.cols.len().checked_sub(age + 1)?)?;
        let i = (band.max(0.0) as usize).min(BANDS - 2);
        let m = (band - i as f32).clamp(0.0, 1.0);
        Some(norm(col[i] + (col[i + 1] - col[i]) * m))
    }
}

/// Where `hz` sits on the log axis, 0..1.
pub fn freq_frac(hz: f32) -> f32 {
    (hz / LO).ln() / (HI / LO).ln()
}

pub fn norm(db: f32) -> f32 {
    ((db - (TOP_DB - SPAN_DB)) / SPAN_DB).clamp(0.0, 1.0)
}

/// Background, then cool, warm and hot as the level rises.
fn ramp(t: &Theme) -> [u32; 65] {
    ramp_from([(0.0, t.plot_bg), (0.35, t.cool), (0.7, t.warm), (1.0, t.hot)])
}

/// The waterfall's scale: full colour from the bottom up, no fade into the background.
fn ramp_3d(t: &Theme) -> [u32; 65] {
    ramp_from([(0.0, t.cool), (0.3, t.cool), (0.65, t.warm), (1.0, t.hot)])
}

fn ramp_from(stops: [(f32, C); 4]) -> [u32; 65] {
    let mut out = [0u32; 65];
    for (i, v) in out.iter_mut().enumerate() {
        let u = i as f32 / 64.0;
        let s = stops.windows(2).find(|w| u <= w[1].0).unwrap_or(&stops[2..4]);
        *v = blend(rgb(s[0].1), s[1].1, (u - s[0].0) / (s[1].0 - s[0].0));
    }
    out
}

fn px(plot: R, k: f32) -> (usize, usize, usize, usize) {
    ((plot.0 * k).round() as usize, (plot.1 * k).round() as usize, (plot.2 * k).round().max(1.0) as usize, (plot.3 * k).round().max(1.0) as usize)
}

/// Time across, frequency up, level as colour.
pub fn paint_2d(sh: &SpecHist, f: &mut Target, t: &Theme, k: f32, plot: R) {
    let (x0, y0, w, h) = px(plot, k);
    if x0 + w > f.w || y0 + h > f.h {
        return;
    }
    let lut = ramp(t);
    let bands: Vec<f32> = (0..h).map(|y| (1.0 - (y as f32 + 0.5) / h as f32) * (BANDS - 1) as f32).collect();
    let mut grid = vec![false; h];
    for (hz, _) in TICKS {
        let y = ((1.0 - freq_frac(hz)) * h as f32) as usize;
        if y < h {
            grid[y] = true;
        }
    }
    for x in 0..w {
        let age = ((w - 1 - x) as f32 * COLS as f32 / w as f32) as usize;
        for (y, &band) in bands.iter().enumerate() {
            let mut c = sh.level(age, band).map_or(lut[0], |v| lut[(v * 64.0) as usize]);
            if grid[y] {
                c = blend(c, t.grid_strong, 0.5);
            }
            f.px[(y0 + y) * f.w + x0 + x] = c;
        }
    }
}

/// Frequency across, level up, older slices further back and up to the right. The slices slide
/// back smoothly as the next one fills, instead of jumping a slice at a time.
pub fn paint_3d(sh: &SpecHist, f: &mut Target, t: &Theme, k: f32, plot: R) {
    let (x0, y0, w, h) = px(plot, k);
    if x0 + w > f.w || y0 + h > f.h {
        return;
    }
    f.fill(x0, y0, x0 + w, y0 + h, rgb(t.plot_bg));
    let lut = ramp_3d(t);
    let (fw, fh) = ((w as f32 * FRONT_W) as usize, h as f32 * FRONT_H);
    let (dx, dy) = ((w - fw) as f32, h as f32 - fh);
    let line = (0.8 * k).max(0.7);
    let phase = (sh.acc_t / SLICE_S).clamp(0.0, 1.0);
    // The slice being filled at the front, then the finished ones behind it.
    let front = (sh.acc_n > 0).then(|| sh.averaged());
    let n = sh.slices.len();
    let mut rows: Vec<(f32, &[f32])> = sh.slices.iter().enumerate().map(|(i, v)| (((n - 1 - i) as f32 + phase) / SLICES as f32, v.as_slice())).filter(|(d, _)| *d <= 1.0).collect();
    if let Some(v) = &front {
        rows.push((0.0, v.as_slice()));
    }
    for (depth, col) in rows {
        let (ox, base) = (x0 as f32 + depth * dx, y0 as f32 + h as f32 - depth * dy);
        let fade = 1.0 - 0.35 * depth;
        let mut prev: Option<f32> = None;
        for x in 0..fw {
            let band = x as f32 / (fw - 1).max(1) as f32 * (BANDS - 1) as f32;
            let i = (band as usize).min(BANDS - 2);
            let m = band - i as f32;
            let v = norm(col[i] + (col[i + 1] - col[i]) * m);
            let top = base - v * fh;
            let c = C(0xff00_0000 | lut[(v * 64.0) as usize]);
            let column = ox.round() as usize + x;
            // Opaque under the line, so a newer slice hides the older ones behind it.
            f.vspan(column, top, base, y0, y0 + h, C(0xff00_0000 | blend(rgb(t.plot_bg), c, 0.8 * fade)), 1.0);
            let p = prev.unwrap_or(top);
            f.vspan(column, p.min(top) - line, p.max(top) + line, y0, y0 + h, c, fade);
            prev = Some(top);
        }
    }
}

/// Axis labels for a spectrum view of `plot`: `(x, y, text, right_aligned)` in points.
pub fn labels(mode: u8, plot: R) -> Vec<(f32, f32, String, bool)> {
    let mut out = Vec::new();
    if mode == 1 {
        for (hz, s) in TICKS {
            out.push((plot.0 - 6.0, plot.1 + (1.0 - freq_frac(hz)) * plot.3, s.to_string(), true));
        }
    } else {
        let fw = plot.2 * FRONT_W;
        for (hz, s) in TICKS {
            out.push((plot.0 + freq_frac(hz) * fw, plot.1 + plot.3 + 13.0, s.to_string(), false));
        }
        let mut db = TOP_DB - 6.0;
        while db >= TOP_DB - SPAN_DB {
            out.push((plot.0 - 6.0, plot.1 + plot.3 - norm(db) * plot.3 * FRONT_H, format!("{db:.0}"), true));
            db -= 24.0;
        }
    }
    out
}
