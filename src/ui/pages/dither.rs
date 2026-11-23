//! The dither page (`design/shell.md` section 4): reduce the word length for the print and
//! choose where the noise goes. Core picture: the spectrum of the noise this setting adds,
//! next to a 24-bit ladder of the word that leaves the plugin.
//!
//! The plugin starts the word length at Off (its own default overrides the crate's 24 bit),
//! so Off is the page's starting state.

use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use requantiser::view::{shaping_power_gain, shaping_response_db, word_bits};
use requantiser::{P_AUTO_BLANK, P_DC_FILTER, P_DITHER_AMOUNT, P_KEEP_EXACT, P_PEAK_LIMIT, P_SHAPING, P_SHAPING_DEPTH, P_WORD_LENGTH};

use super::limiter::{self as frame};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::ui::raster::R;

/// Shaping families and the `shaping` values they hold.
const FAMILIES: [(&str, &[usize]); 4] = [("OFF", &[0]), ("TILT", &[1, 2, 3, 4]), ("RISE", &[5, 6, 7, 8, 9, 10]), ("VALLEY", &[11, 12, 13])];
const VARIANTS: [&[&str]; 4] = [&[], &["1", "2", "TOP", "SOFT"], &["1", "2", "3", "4", "5", "6"], &["1", "2", "3"]];
const FAMILY_ROW: R = (120.0, 724.0, 256.0, 28.0);
const VARIANT_ROW: R = (384.0, 724.0, 360.0, 28.0);
const LADDER: R = (784.0, 440.0, 288.0, 28.0);
const POINTS: usize = 160;

fn family_of(shaping: usize) -> usize {
    FAMILIES.iter().position(|(_, v)| v.contains(&shaping)).unwrap_or(0)
}

fn peak_text(v: f32) -> String {
    if v < 0.5 {
        "Off".into()
    } else {
        format!("±{v:.0} steps")
    }
}

struct Dither {
    /// The variant last used in each family.
    last: [usize; 4],
    /// The response curve, cached for (shaping, depth, sample rate).
    cache: std::sync::Mutex<Option<((usize, u32, u32), Vec<f32>)>>,
}

impl Dither {
    fn sr(cx: &Ctx) -> f32 {
        if cx.sample_rate > 1000.0 {
            cx.sample_rate
        } else {
            48_000.0
        }
    }

    fn graph(cx: &Ctx) -> (R, R) {
        let top = cx.zones.picture.1;
        let panel = (120.0, top, 640.0, 616.0 - top);
        (panel, (152.0, top + 8.0, 592.0, 616.0 - top - 32.0))
    }

    fn freqs(sr: f32) -> Vec<f32> {
        let hi = sr * 0.5;
        (0..POINTS).map(|k| kit::x_freq(k as f32, 0.0, (POINTS - 1) as f32, 20.0, hi)).collect()
    }

    fn response(&self, cx: &Ctx) -> Vec<f32> {
        let shaping = cx.choice(P_SHAPING);
        let depth = cx.v(P_SHAPING_DEPTH);
        let sr = Self::sr(cx);
        let key = (shaping, depth.to_bits(), sr.to_bits());
        let mut c = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if c.as_ref().is_none_or(|(k, _)| *k != key) {
            let f = Self::freqs(sr);
            let mut out = vec![0.0; f.len()];
            shaping_response_db(shaping, depth, sr, &f, &mut out);
            *c = Some((key, out));
        }
        c.as_ref().map(|(_, v)| v.clone()).unwrap_or_default()
    }

    /// Unweighted and A-weighted noise floor, dBFS: q^2 (1/12 + a^2/6) under the shaping.
    fn floor(cx: &Ctx, response: &[f32]) -> Option<(f32, f32)> {
        let bits = word_bits(cx.choice(P_WORD_LENGTH));
        if bits == 0 {
            return None;
        }
        let q = 2f64.powi(1 - bits as i32);
        let a = cx.v(P_DITHER_AMOUNT) as f64 / 100.0;
        let white = q * q * (1.0 / 12.0 + a * a / 6.0);
        let total = white * shaping_power_gain(cx.choice(P_SHAPING), cx.v(P_SHAPING_DEPTH));
        // A-weighting over a linear frequency grid up to Nyquist, through the response.
        let sr = Self::sr(cx) as f64;
        let n = 2048;
        let f = Self::freqs(sr as f32);
        let mut sum = 0.0;
        for k in 0..n {
            let hz = (k as f64 + 0.5) / n as f64 * sr * 0.5;
            let j = f.partition_point(|&x| (x as f64) < hz).min(response.len() - 1);
            let shaped = 10f64.powf(response[j] as f64 / 10.0);
            sum += shaped * a_weight(hz);
        }
        let weighted = white * sum / n as f64;
        Some(((10.0 * total.log10()) as f32, (10.0 * weighted.max(1e-30).log10()) as f32))
    }

    fn variant_rect(family: usize, i: usize) -> R {
        let n = VARIANTS[family].len().max(1) as f32;
        let w = (VARIANT_ROW.2 / n).min(60.0);
        (VARIANT_ROW.0 + w * i as f32, VARIANT_ROW.1, w, VARIANT_ROW.3)
    }

    fn family_rect(i: usize) -> R {
        (FAMILY_ROW.0 + 64.0 * i as f32, FAMILY_ROW.1, 64.0, FAMILY_ROW.3)
    }
}

/// A-weighting, power, 0 dB at 1 kHz.
fn a_weight(f: f64) -> f64 {
    let f2 = f * f;
    let ra = (12194.0f64.powi(2) * f2 * f2) / ((f2 + 20.6f64.powi(2)) * ((f2 + 107.7f64.powi(2)) * (f2 + 737.9f64.powi(2))).sqrt() * (f2 + 12194.0f64.powi(2)));
    let ra1k = {
        let f2 = 1.0e6f64;
        (12194.0f64.powi(2) * f2 * f2) / ((f2 + 20.6f64.powi(2)) * ((f2 + 107.7f64.powi(2)) * (f2 + 737.9f64.powi(2))).sqrt() * (f2 + 12194.0f64.powi(2)))
    };
    (ra / ra1k).powi(2)
}

/// A segmented row the page draws: `labels`, the chosen key, dim.
fn segments(s: &mut Scene, t: &crate::ui::theme::Theme, rects: &[R], labels: &[&str], chosen: Option<usize>, dim: bool) {
    if rects.is_empty() {
        return;
    }
    let first = rects[0];
    let last = rects[rects.len() - 1];
    let all = (first.0, first.1, last.0 + last.2 - first.0, first.3);
    kit::inset(s, t, all, 6.0);
    for (i, (r, label)) in rects.iter().zip(labels).enumerate() {
        let key = Rect::new(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0);
        let on = Some(i) == chosen && !dim;
        if on {
            s.fill_round(key, 4.0, t.accent, t.accent, true);
        }
        let ink = if on { t.on_accent } else if dim { t.dim.with_alpha(0.5) } else { t.dim };
        s.text(key.center(), *label, ink, 9.0, true);
    }
}

impl Page for Dither {
    fn has_more(&self) -> bool {
        true
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let p = |i| cx.p(i);
        if cx.more {
            return vec![
                Cell::pill(p(P_AUTO_BLANK), "AUTO-BLANK", (120.0, 652.0, 120.0, 28.0)).tip("Mutes the dither after 0.7 s of silence"),
                Cell::pill(p(P_KEEP_EXACT), "SILENCE STAYS SILENT", (256.0, 652.0, 184.0, 28.0)).tip("Samples already exact pass untouched"),
                Cell::pill(p(P_DC_FILTER), "DC FILTER", (456.0, 652.0, 104.0, 28.0)).tip("0.7 Hz high-pass; also works with word length Off"),
                Cell::knob(Size::Big, p(P_PEAK_LIMIT), "NOISE PEAK LIMIT", 592.0, 636.0).format(peak_text).tip("Limits how far the noise may move a sample, in steps"),
            ];
        }
        let off = cx.choice(P_WORD_LENGTH) == 0;
        let flat = cx.choice(P_SHAPING) == 0;
        vec![
            Cell::segmented(p(P_WORD_LENGTH), "WORD LENGTH", (120.0, 644.0, 448.0, 32.0)).keys(&["OFF", "24", "22", "20", "18", "16", "12", "8"]).tip("The word length of the print"),
            Cell::dropdown(p(P_SHAPING), "CURVE", (604.0, 696.0, 140.0, 24.0)).tip("Every shaping curve in one list"),
            Cell::knob(Size::Big, p(P_DITHER_AMOUNT), "AMOUNT", 816.0, 636.0).dim(off).tip("Dither level; 100 % is standard triangular dither"),
            Cell::knob(Size::Big, p(P_SHAPING_DEPTH), "DEPTH", 952.0, 636.0).dim(off || flat).tip("How much of the shaping curve is used"),
        ]
    }

    fn rebuild_on(&self, local_index: usize, _cx: &Ctx) -> bool {
        local_index == P_WORD_LENGTH || local_index == P_SHAPING
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let (_, g) = Self::graph(cx);
        kit::plot(s, t, g);
        if cx.more {
            s.text(Point::new(120.0, 630.0), "MORE", t.dim, 9.0, false);
            s.text(Point::new(120.0, 692.0), "mutes the dither after 0.7 s of silence", t.dim, 8.0, false);
            s.text(Point::new(256.0, 692.0), "samples already exact pass untouched", t.dim, 8.0, false);
            s.text(Point::new(456.0, 692.0), "0.7 Hz high-pass,", t.dim, 8.0, false);
            s.text(Point::new(456.0, 704.0), "also with word length Off", t.dim, 8.0, false);
        } else {
            s.text(Point::new(120.0, 632.0), "WORD LENGTH", t.dim, 9.0, false);
            s.text(Point::new(120.0, 710.0), "NOISE SHAPING", t.dim, 9.0, false);
            kit::text_right(s, 598.0, 708.0, "CURVE", t.dim, 9.0);
        }
        s.text(Point::new(LADDER.0, cx.zones.picture.1 + 10.0), "BIT ACTIVITY", t.dim, 9.0, false);
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, _live: &Live) {
        let t = cx.t;
        let (panel, g) = Self::graph(cx);
        let sr = Self::sr(cx);
        let response = self.response(cx);
        let peak = response.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let range = if peak <= 22.0 { 24.0 } else if peak <= 46.0 { 48.0 } else { 72.0 };
        let off = cx.choice(P_WORD_LENGTH) == 0;
        kit::freq_grid(s, t, g, 20.0, sr * 0.5, range, 2.0 * range, range / 2.0);
        s.text(Point::new(panel.0 + 40.0, g.1 + 10.0), "NOISE vs FLAT (dB)", t.dim, 9.0, false);
        let zero = g.1 + g.3 * 0.5;
        kit::dashed_h(s, g.0, g.0 + g.2, zero, 4.0, 3.0, t.grid_strong, 1.0);
        let pts: Vec<Point> = response
            .iter()
            .enumerate()
            .map(|(k, &d)| Point::new(g.0 + g.2 * k as f32 / (POINTS - 1) as f32, (zero - d / range * g.3 * 0.5).clamp(g.1, g.1 + g.3)))
            .collect();
        let fade = if off { 0.35 } else { 1.0 };
        s.clip_round(kit::rect(g), 4.0);
        kit::area(s, &pts, zero, t.accent.with_alpha(0.15 * fade));
        kit::curve(s, pts, t.accent.with_alpha(fade), 2.0);
        s.pop_clip();
        let amount = cx.v(P_DITHER_AMOUNT);
        let note = if off {
            Some(("Output untouched", t.dim))
        } else if amount <= 0.0 {
            Some(("No dither: truncation distortion", t.warm))
        } else if amount > 100.0 {
            Some(("Above standard level", t.dim))
        } else {
            None
        };
        if let Some((n, c)) = note {
            kit::text_right(s, g.0 + g.2 - 8.0, g.1 + 12.0, n, c, 11.0);
        }
        // The ladder: the bits the word keeps, the bracket at the word length. Measured bit
        // activity is not published by the plugin for the dither, so the cells show the
        // setting, not proof.
        let bits = word_bits(cx.choice(P_WORD_LENGTH)) as usize;
        let ly = cx.zones.picture.1 + 24.0;
        for i in 0..24 {
            let r = Rect::new(LADDER.0 + 12.0 * i as f32, ly, 10.0, 28.0);
            let used = bits == 0 || i < bits;
            let c = if used { t.seg_on.with_alpha(if bits == 0 { 0.35 } else { 0.85 }) } else { t.seg_off };
            s.fill_round(r, 2.0, c, c, true);
        }
        if bits > 0 && bits < 24 {
            let x = LADDER.0 + 12.0 * bits as f32 - 1.0;
            kit::dashed(s, &[Point::new(x, ly - 4.0), Point::new(x, ly + 32.0)], 3.0, 2.0, t.accent, 1.5);
        }
        s.text(Point::new(LADDER.0, ly + 40.0), "24", t.dim, 8.5, false);
        kit::text_right(s, LADDER.0 + LADDER.2, ly + 40.0, "1", t.dim, 8.5);
        let line = if bits == 0 { "Off: the full word passes".to_string() } else { format!("{bits} of 24 bits in use (set)") };
        s.text(Point::new(LADDER.0, ly + 58.0), line, t.text, 12.0, false);
        if let Some((flat, a)) = Self::floor(cx, &response) {
            s.text(Point::new(LADDER.0, ly + 84.0), "NOISE FLOOR", t.dim, 9.0, false);
            s.text(Point::new(LADDER.0, ly + 106.0), format!("{flat:.1} dBFS"), t.text, 22.0, false);
            s.text(Point::new(LADDER.0, ly + 132.0), format!("A-weighted  {a:.0} dBFS"), t.dim, 12.0, false);
        }
        if cx.on(P_AUTO_BLANK) {
            let r = Rect::new(LADDER.0 + 200.0, ly + 128.0, 88.0, 22.0);
            s.stroke_round(r, 11.0, t.warm, 1.0);
            s.text(r.center(), "AUTO-BLANK", t.warm, 8.5, true);
        }
        if cx.more {
            return;
        }
        // Shaping: family, then the variant in it.
        let shaping = cx.choice(P_SHAPING);
        let fam = family_of(shaping);
        let frects: Vec<R> = (0..4).map(Self::family_rect).collect();
        segments(s, t, &frects, &FAMILIES.map(|f| f.0), Some(fam), off);
        let n = VARIANTS[fam].len();
        if n > 0 {
            let vrects: Vec<R> = (0..n).map(|i| Self::variant_rect(fam, i)).collect();
            let chosen = FAMILIES[fam].1.iter().position(|&v| v == shaping);
            segments(s, t, &vrects, VARIANTS[fam], chosen, off);
        } else {
            let r = (VARIANT_ROW.0, VARIANT_ROW.1, 240.0, VARIANT_ROW.3);
            segments(s, t, &[r], &["flat dither, no shaping"], None, true);
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let (panel, _) = Self::graph(cx);
        let top = cx.zones.picture.1;
        vec![panel, (776.0, top, 312.0, 616.0 - top), (116.0, 720.0, 632.0, 36.0)]
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let Pointer::Down { x, y, .. } = ev else { return Response::NONE };
        if cx.more || cx.choice(P_WORD_LENGTH) == 0 {
            return Response::NONE;
        }
        let shaping = cx.choice(P_SHAPING);
        let fam = family_of(shaping);
        if let Some(f) = (0..4).find(|&i| kit::inside(Self::family_rect(i), x, y)) {
            self.last[fam] = shaping;
            let v = if f == fam { shaping } else { self.last[f] };
            io.set(cx.p(P_SHAPING), v as f32);
            return Response::REBUILD;
        }
        if let Some(i) = (0..VARIANTS[fam].len()).find(|&i| kit::inside(Self::variant_rect(fam, i), x, y)) {
            let v = FAMILIES[fam].1[i];
            self.last[fam] = v;
            io.set(cx.p(P_SHAPING), v as f32);
            return Response::TAKEN;
        }
        Response::NONE
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let top = cx.zones.picture.1;
        vec![
            frame::tip(Self::graph(cx).0, "The spectrum of the noise this setting adds, against flat dither"),
            frame::tip((LADDER.0, top + 20.0, LADDER.2, 60.0), "The word that leaves the plugin; the bracket is the word length. Measured bit activity is not published yet"),
        ]
    }
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    Box::new(Dither { last: [0, 1, 5, 11], cache: std::sync::Mutex::new(None) })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
#[allow(dead_code)] // the dither has no rack chip of its own
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let _ = values;
    (false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weighting_is_zero_at_1k_and_low_at_50_hz() {
        assert!((a_weight(1000.0) - 1.0).abs() < 1e-9);
        assert!(10.0 * a_weight(50.0).log10() < -25.0);
    }
}
