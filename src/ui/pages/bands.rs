//! Band Compressor (`crates/comp-bands`): five bands on one frequency graph, the selected
//! band's knobs and transfer curve. Graph-first (TALL). See `design/compressors.md` section 1.

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use comp_bands::{self as cb, band_param, view as vw, BANDS};

use self::common::*;
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::INFO;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != cb::PARAM_COUNT {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(Bands::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (values.get(cb::P_KEY_SOURCE).is_some_and(|v| *v >= 0.5), false)
}

const NAMES: [&str; BANDS] = ["LOW", "LOW-MID", "MID", "HIGH-MID", "HIGH"];
const SHORT: [&str; BANDS] = ["L", "LM", "M", "HM", "H"];
const MODE_KEYS: &[&str] = &["DOWN", "PARALLEL", "PAR L/R", "LIFT"];
/// The editor keeps neighbouring splits a third of an octave apart.
const SPLIT_GAP: f32 = 1.259_921;

/// Where things are, for STANDARD or TALL.
#[derive(Clone, Copy)]
struct Geo {
    tall: bool,
    graph: R,
    gr_top: f32,
    gr_bot: f32,
    gr0: f32,
    gr_ppd: f32,
    lv_top: f32,
    lv_ppd: f32,
    lv_min: f32,
    curve: R,
}

const X0: f32 = 152.0;
const X1: f32 = 840.0;
const LV_BOT: f32 = 564.0;
const CHIP_Y: f32 = 568.0;

impl Geo {
    fn new(tall: bool) -> Self {
        if tall {
            Self { tall, graph: (120.0, 244.0, 752.0, 372.0), gr_top: 252.0, gr_bot: 324.0, gr0: 254.0, gr_ppd: 3.5, lv_top: 328.0, lv_ppd: 4.0, lv_min: -59.0, curve: (880.0, 252.0, 200.0, 200.0) }
        } else {
            Self { tall, graph: (120.0, 416.0, 752.0, 200.0), gr_top: 424.0, gr_bot: 460.0, gr0: 426.0, gr_ppd: 1.7, lv_top: 464.0, lv_ppd: 2.0, lv_min: -50.0, curve: (890.0, 426.0, 180.0, 180.0) }
        }
    }

    fn ly(&self, level: f32) -> f32 {
        (self.lv_top - self.lv_ppd * level).clamp(self.lv_top, LV_BOT)
    }

    fn max_box(&self) -> R {
        (880.0, 560.0, 44.0, 18.0)
    }
}

fn fx(f: f32) -> f32 {
    X0 + 229.33 * (f.max(1.0) / 20.0).log10()
}

fn xf(x: f32) -> f32 {
    20.0 * 10f32.powf((x - X0) / 229.33)
}

#[derive(Clone, Debug)]
enum Drag {
    Split { k: usize },
    /// Incremental drags: `last` is the previous pointer coordinate, the values are where
    /// the parameters are now (Shift moves ten times finer).
    Thresh { params: Vec<(usize, f32)>, last: f32, ppd: f32 },
    CurveThr { param: usize, last: f32, v: f32, ppd: f32 },
    CurveRatio { param: usize, last: f32, n: f32 },
    Gang(GangDrag),
}

pub struct Bands {
    sel: usize,
    all: bool,
    drag: Option<Drag>,
    hold: [f32; BANDS],
    age: [f32; BANDS],
    max: f32,
    /// The last frequency of each split before it was switched off.
    remembered: [f32; 4],
}

impl Default for Bands {
    fn default() -> Self {
        Self { sel: 2, all: false, drag: None, hold: [0.0; BANDS], age: [0.0; BANDS], max: 0.0, remembered: [125.0, 500.0, 2000.0, 8000.0] }
    }
}

/// Main face, editor-only rects.
const PREV: R = (124.0, 640.0, 22.0, 22.0);
const NEXT: R = (206.0, 640.0, 22.0, 22.0);
const ALL: R = (124.0, 716.0, 104.0, 22.0);
const NAV: R = (120.0, 632.0, 116.0, 112.0);
const BIG_X: [f32; 5] = [240.0, 360.0, 480.0, 600.0, 720.0];
const BIG_OFF: [usize; 5] = [cb::B_THRESHOLD, cb::B_RATIO, cb::B_ATTACK, cb::B_RELEASE, cb::B_MAKEUP];
const BIG_LABEL: [&str; 5] = ["THRESHOLD", "RATIO", "ATTACK", "RELEASE", "MAKE-UP"];
const LIFT_R: R = (848.0, 636.0, 56.0, 60.0);
const CEIL_R: R = (904.0, 636.0, 56.0, 60.0);
const WET_R: R = (960.0, 636.0, 56.0, 60.0);
/// MORE face.
const SPLIT_X: [f32; 4] = [136.0, 304.0, 472.0, 640.0];
const SPLIT_LABEL: [&str; 4] = ["SPLIT 1  LOW | LOW-MID", "SPLIT 2  LOW-MID | MID", "SPLIT 3  MID | HIGH-MID", "SPLIT 4  HIGH-MID | HIGH"];
const GUARD_T: R = (808.0, 652.0, 80.0, 84.0);
const GUARD_R: R = (888.0, 652.0, 80.0, 84.0);
const LIM_REL: R = (968.0, 652.0, 80.0, 84.0);
const COPY_Y: f32 = 716.0;
const RESET_ALL: R = (304.0, 717.0, 148.0, 22.0);

fn off_pill(k: usize) -> R {
    (SPLIT_X[k] + 92.0, 664.0, 56.0, 22.0)
}

fn copy_buttons(sel: usize) -> Vec<(usize, R)> {
    (0..BANDS).filter(|&b| b != sel).enumerate().map(|(i, b)| (b, (136.0 + i as f32 * 40.0, COPY_Y, 38.0, 24.0))).collect()
}

fn ratio_text(v: f32) -> String {
    if v >= cb::RATIO_INFINITE - 0.01 {
        "∞:1".into()
    } else {
        format!("{v:.1}:1")
    }
}

fn wet_text(v: f32) -> String {
    if v <= cb::WET_OFF_DB + 0.01 {
        "OFF".into()
    } else {
        format!("{v:+.1} dB")
    }
}

fn guard_ratio_text(v: f32) -> String {
    if v <= 1.001 {
        "OFF".into()
    } else if v >= cb::RATIO_INFINITE - 0.01 {
        "∞".into()
    } else {
        format!("{v:.1}:1")
    }
}

fn split_text(v: f32) -> String {
    if v <= cb::SPLIT_LOW_OFF_HZ + 0.01 || v >= cb::SPLIT_HIGH_OFF_HZ - 0.5 {
        "OFF".into()
    } else {
        hz_text(v)
    }
}

impl Bands {
    fn mode(cx: &Ctx) -> usize {
        cx.choice(cb::P_MODE)
    }

    fn splits(cx: &Ctx) -> [f32; 4] {
        [cx.v(cb::P_SPLIT_1), cx.v(cb::P_SPLIT_2), cx.v(cb::P_SPLIT_3), cx.v(cb::P_SPLIT_4)]
    }

    fn split_off(k: usize, f: f32) -> bool {
        if k < 2 {
            f <= cb::SPLIT_LOW_OFF_HZ + 0.01
        } else {
            f >= cb::SPLIT_HIGH_OFF_HZ - 0.5
        }
    }

    /// The x of each band edge: 152, the four splits, 840.
    fn edges(cx: &Ctx) -> [f32; 6] {
        let s = Self::splits(cx);
        let mut e = [X0, 0.0, 0.0, 0.0, 0.0, X1];
        for k in 0..4 {
            e[k + 1] = fx(s[k]).clamp(X0, X1);
        }
        e
    }

    fn band_span(e: &[f32; 6], b: usize) -> (f32, f32) {
        (e[b].min(e[b + 1]), e[b].max(e[b + 1]))
    }

    fn band_at(cx: &Ctx, x: f32) -> usize {
        let e = Self::edges(cx);
        (0..BANDS).rev().find(|&b| {
            let (a, z) = Self::band_span(&e, b);
            x >= a && x <= z && z - a > 0.5
        })
        .unwrap_or_else(|| if x < e[1] { 0 } else { 4 })
    }

    fn p(cx: &Ctx, b: usize, off: usize) -> usize {
        cx.p(band_param(b, off))
    }

    fn bv(cx: &Ctx, b: usize, off: usize) -> f32 {
        cx.v(band_param(b, off))
    }

    fn thr_abs(cx: &Ctx, b: usize) -> f32 {
        cx.v(cb::P_REFERENCE) + Self::bv(cx, b, cb::B_THRESHOLD)
    }

    fn view(live: &Live, i: usize) -> Option<f32> {
        (live.view.len() >= vw::LEN).then(|| live.view[i])
    }

    /// The selected band's steady-state output level for an input level, dBFS.
    fn curve_out(cx: &Ctx, b: usize, level: f32) -> f32 {
        let mode = Self::mode(cx);
        let rel = level - cx.v(cb::P_REFERENCE);
        let g = cb::static_gain_db(mode, rel, Self::bv(cx, b, cb::B_THRESHOLD), Self::bv(cx, b, cb::B_RATIO), Self::bv(cx, b, cb::B_LIFT), cx.v(cb::P_FLOOR_GUARD_THRESHOLD), cx.v(cb::P_FLOOR_GUARD_RATIO));
        let out = level + Self::applied(cx, b, g);
        if cx.on(cb::P_BAND_LIMITER) {
            out.min(Self::bv(cx, b, cb::B_CEILING))
        } else {
            out
        }
    }

    /// The gain the band applies for a compressor gain `g` dB (make-up, parallel mix).
    fn applied(cx: &Ctx, b: usize, g: f32) -> f32 {
        let mk = Self::bv(cx, b, cb::B_MAKEUP);
        match Self::mode(cx) {
            1 | 2 => {
                let wet = cx.v(cb::P_WET_GAIN);
                let wet = if wet <= cb::WET_OFF_DB { 0.0 } else { db_lin(wet) };
                lin_db(0.5 * (1.0 + wet * db_lin(mk) * db_lin(g)))
            }
            _ => g + mk,
        }
    }

    fn big_cells(&self, cx: &Ctx) -> Vec<Cell> {
        let b = self.sel;
        let mut v = Vec::new();
        for k in 0..5 {
            let mut c = Cell::knob(Size::Big, Self::p(cx, b, BIG_OFF[k]), if k == 4 { self.makeup_label(cx) } else { BIG_LABEL[k] }, BIG_X[k], 636.0);
            c = match k {
                0 => c.tip("Threshold, relative to the reference level"),
                1 => c.format(ratio_text),
                4 => c.tip("Make-up; on the compressed path only in the parallel modes"),
                _ => c,
            };
            v.push(c);
        }
        v.push(Cell::knob(Size::Mini, Self::p(cx, b, cb::B_LIFT), "LIFT", LIFT_R.0, LIFT_R.1).tip("Lift amount, used in the LIFT mode").dim(Self::mode(cx) != 3));
        v.push(Cell::knob(Size::Mini, Self::p(cx, b, cb::B_CEILING), "CEILING", CEIL_R.0, CEIL_R.1).tip("Band limiter ceiling, used with BAND LIMIT").dim(!cx.on(cb::P_BAND_LIMITER)));
        v
    }

    /// The selected band's knobs as editor-driven gangs while ALL is on.
    fn gangs(&self, cx: &Ctx) -> Vec<Gang> {
        if !self.all || cx.more {
            return Vec::new();
        }
        let mut g = Vec::new();
        let order = |off: usize| -> Vec<usize> {
            let mut v = vec![Self::p(cx, self.sel, off)];
            v.extend((0..BANDS).filter(|&b| b != self.sel).map(|b| Self::p(cx, b, off)));
            v
        };
        for k in 0..5 {
            let label = if k == 4 { self.makeup_label(cx) } else { BIG_LABEL[k] };
            g.push(Gang::knob(Size::Big, (BIG_X[k], 636.0, 120.0, 124.0), order(BIG_OFF[k]), label, true).format(if k == 1 { Some(ratio_text) } else { None }));
        }
        g.push(Gang::knob(Size::Mini, LIFT_R, order(cb::B_LIFT), "LIFT", true).faded(Self::mode(cx) != 3));
        g.push(Gang::knob(Size::Mini, CEIL_R, order(cb::B_CEILING), "CEILING", true).faded(!cx.on(cb::P_BAND_LIMITER)));
        g
    }

    fn makeup_label(&self, cx: &Ctx) -> &'static str {
        if matches!(Self::mode(cx), 1 | 2) {
            "WET MAKE-UP"
        } else {
            "MAKE-UP"
        }
    }

    fn reset_holds(&mut self) {
        self.hold = [0.0; BANDS];
        self.age = [0.0; BANDS];
        self.max = 0.0;
    }

    fn copy_band(&self, cx: &Ctx, io: &mut Io, to: &[usize]) {
        for off in 0..cb::PER_BAND {
            let v = Self::bv(cx, self.sel, off);
            for &b in to {
                io.set(Self::p(cx, b, off), v);
            }
        }
    }

    fn reset_bands(cx: &Ctx, io: &mut Io) {
        for b in 0..BANDS {
            for off in 0..cb::PER_BAND {
                let i = Self::p(cx, b, off);
                io.set(i, INFO.params[i].default);
            }
        }
    }

    // ---- painting ----

    fn paint_graph(&self, s: &mut Scene, cx: &Ctx, live: &Live, g: &Geo) {
        let t = cx.t;
        let e = Self::edges(cx);
        let mode = Self::mode(cx);
        let any_solo = (0..BANDS).any(|b| cx.on(band_param(b, cb::B_SOLO)));
        let keyed = Self::view(live, vw::KEYED).is_some_and(|v| v >= 0.5);
        let splits = Self::splits(cx);
        let plot_bottom = 590.0;
        // Band tints and hatching.
        for b in 0..BANDS {
            let (a, z) = Self::band_span(&e, b);
            if z - a < 0.5 {
                continue;
            }
            let mut alpha = if b == self.sel { 0.28 } else { 0.18 };
            if any_solo && !cx.on(band_param(b, cb::B_SOLO)) {
                alpha *= 0.4;
            }
            let c = t.bands[b];
            let r = (a, g.lv_top, z - a, plot_bottom - g.lv_top);
            s.fill_rect(kit::rect(r), c.with_alpha(alpha), c.with_alpha(alpha), true);
            if cx.on(band_param(b, cb::B_BYPASS)) {
                hatch(s, r, t.dim.with_alpha(0.5), 7.0);
            }
        }
        // Band shapes of the crossover tree, 0 dB at the top of the level area.
        for b in 0..BANDS {
            let mut pts = Vec::with_capacity(180);
            let mut x = X0;
            while x <= X1 + 0.1 {
                let d = cb::band_response_db(splits, b, xf(x), cx.sample_rate.max(8000.0));
                pts.push(Point::new(x, g.ly(d.max(g.lv_min))));
                x += 4.0;
            }
            kit::curve(s, pts, t.bands[b].with_alpha(0.75), 1.0);
        }
        // Detector columns, output lines, threshold lines, ceilings.
        for b in 0..BANDS {
            let (a, z) = Self::band_span(&e, b);
            if z - a < 10.0 {
                continue;
            }
            let thr = Self::thr_abs(cx, b);
            let ty = g.ly(thr);
            let (ca, cz) = (a + 4.0, z - 4.0);
            if let Some(det) = Self::view(live, vw::DETECTOR + b) {
                let dy = g.ly(det);
                let fill = if keyed { t.warm } else { t.in_fill };
                if dy < LV_BOT {
                    s.fill_rect(Rect::new(ca, dy, cz - ca, LV_BOT - dy), fill.with_alpha(t.in_fill_alpha * 0.8), fill.with_alpha(t.in_fill_alpha * 0.8), true);
                    if dy < ty {
                        let solid = if keyed { t.warm } else { t.in_line };
                        s.fill_rect(Rect::new(ca, dy, cz - ca, ty - dy), solid, solid, true);
                    }
                }
                if let Some(out) = Self::view(live, vw::OUTPUT + b) {
                    let oy = g.ly(out);
                    if oy < LV_BOT {
                        s.line(Point::new(ca, oy), Point::new(cz, oy), t.out_line, 1.5);
                    }
                }
            }
            if cx.on(cb::P_BAND_LIMITER) {
                kit::dashed_h(s, a + 2.0, z - 2.0, g.ly(Self::bv(cx, b, cb::B_CEILING)), 4.0, 3.0, t.over.with_alpha(0.8), 1.0);
            }
            let colour = if b == self.sel { t.accent } else { t.text.with_alpha(0.7) };
            s.line(Point::new(a + 1.0, ty), Point::new(z - 1.0, ty), colour, if b == self.sel { 1.6 } else { 1.0 });
            let mid = (a + z) * 0.5;
            let grip = Rect::new(mid - 14.0, ty - 5.0, 28.0, 10.0);
            s.fill_round(grip, 4.0, colour, colour, true);
            if thr > 0.0 {
                s.text(Point::new(mid, ty + 13.0), format!("▲ +{thr:.1}"), colour, 8.5, true);
            }
        }
        // Split lines with their grips.
        for k in 0..4 {
            let x = e[k + 1];
            let off = Self::split_off(k, splits[k]);
            s.line(Point::new(x, g.gr_top), Point::new(x, plot_bottom), t.text.with_alpha(if off { 0.25 } else { 0.55 }), 1.0);
            let grip = Rect::new(x - 5.0, g.lv_top, 10.0, 14.0);
            s.fill_round(grip, 3.0, t.panel, t.panel, true);
            s.stroke_round(grip, 3.0, t.text.with_alpha(0.7), 1.0);
            if off {
                let lx = if k < 2 { x + 18.0 } else { x - 18.0 };
                s.text(Point::new(lx, g.lv_top + 7.0), "OFF", t.dim, 8.0, true);
            }
        }
        // Gain reduction lane.
        let lift = mode == 3;
        let mid_lane = (g.gr_top + g.gr_bot) * 0.5;
        let bip_ppd = (g.gr_bot - g.gr_top) * 0.5 / 10.0;
        if lift {
            s.line(Point::new(X0, mid_lane), Point::new(X1, mid_lane), t.grid_strong, 1.0);
        }
        for b in 0..BANDS {
            let (a, z) = Self::band_span(&e, b);
            if z - a < 14.0 {
                continue;
            }
            let (ba, bz) = (a + 6.0, z - 6.0);
            if lift {
                let gain = Self::view(live, vw::GAIN + b).unwrap_or(0.0);
                let h = (gain.abs() * bip_ppd).min((g.gr_bot - g.gr_top) * 0.5);
                if h > 0.3 {
                    let y = if gain > 0.0 { mid_lane - h } else { mid_lane };
                    s.fill_rect(Rect::new(ba, y, bz - ba, h), t.accent, t.accent, true);
                }
                if gain.abs() > 0.5 {
                    s.text(Point::new((a + z) * 0.5, if gain > 0.0 { mid_lane + 7.0 } else { mid_lane - 7.0 }), format!("{gain:+.1}"), t.text, 8.0, true);
                }
                continue;
            }
            let gr = Self::view(live, vw::GR + b).unwrap_or(0.0);
            let lim = Self::view(live, vw::LIMIT + b).unwrap_or(0.0);
            let room = g.gr_bot - g.gr0;
            let depth = (gr * g.gr_ppd).min(room);
            if mode == 2 {
                let half = (bz - ba) * 0.5;
                for c in 0..2 {
                    let d = (Self::view(live, vw::GR_CHANNEL + 2 * b + c).unwrap_or(0.0) * g.gr_ppd).min(room);
                    if d > 0.3 {
                        s.fill_rect(Rect::new(ba + half * c as f32 + 0.5, g.gr0, half - 1.0, d), t.accent, t.accent, true);
                    }
                }
            } else if depth > 0.3 {
                s.fill_rect(Rect::new(ba, g.gr0, bz - ba, depth), t.accent, t.accent, true);
            }
            let ld = (lim * g.gr_ppd).min(room - depth);
            if ld > 0.3 {
                let r = (ba, g.gr0 + depth, bz - ba, ld);
                s.stroke_round(kit::rect(r), 0.0, t.accent, 1.0);
                hatch(s, r, t.accent, 4.0);
            }
            let hold = (self.hold[b] * g.gr_ppd).min(room);
            if hold > 1.0 {
                s.line(Point::new(ba, g.gr0 + hold), Point::new(bz, g.gr0 + hold), t.text, 1.5);
            }
            if gr + lim > 0.5 {
                let ny = (g.gr0 + depth + ld + 7.0).min(g.gr_bot - 4.0);
                s.text(Point::new((a + z) * 0.5, ny), format!("{:.0}", (gr + lim).round() + 0.0), t.text, 8.0, true);
            }
        }
        // Band chips.
        for b in 0..BANDS {
            let (a, z) = Self::band_span(&e, b);
            let w = (z - a - 8.0).min(120.0);
            if w < 24.0 {
                continue;
            }
            let r = ((a + z) * 0.5 - w * 0.5, CHIP_Y, w, 22.0);
            s.fill_round(kit::rect(r), 5.0, t.panel, t.panel, true);
            s.stroke_round(kit::rect(r), 5.0, if b == self.sel { t.accent } else { t.border }, 1.0);
            s.fill_rect(Rect::new(r.0 + 3.0, r.1 + 4.0, 3.0, 14.0), t.bands[b], t.bands[b], true);
            let full = w >= 76.0;
            let name_w = if full { w - 58.0 } else { w - 12.0 };
            let name = text_fit(NAMES[b], 8.5, name_w);
            s.text(Point::new(r.0 + 10.0, r.1 + 11.0), name, t.text, 8.5, false);
            if full {
                for (i, (off, lab)) in [(cb::B_SOLO, "S"), (cb::B_BYPASS, "B")].into_iter().enumerate() {
                    let c = chip_toggle(r, i);
                    let on = cx.on(band_param(b, off));
                    round_toggle(s, t, c, lab, on, if off == cb::B_SOLO { t.accent } else { t.text });
                }
            }
        }
    }

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx, live: &Live, g: &Geo) {
        let t = cx.t;
        let m = Map::square(g.curve);
        let b = self.sel;
        let mut pts = Vec::with_capacity(64);
        let mut l = -60.0;
        while l <= 0.01 {
            pts.push(Point::new(m.x(l), m.y(Self::curve_out(cx, b, l))));
            l += 1.0;
        }
        // The reduction between unity and the curve.
        s.clip_round(kit::rect(g.curve), 4.0);
        kit::between(s, &pts, &[Point::new(m.x(-60.0), m.y(-60.0)), Point::new(m.x(0.0), m.y(0.0))], t.accent.with_alpha(0.1));
        if cx.on(cb::P_BAND_LIMITER) {
            kit::dashed_h(s, g.curve.0, g.curve.0 + g.curve.2, m.y(Self::bv(cx, b, cb::B_CEILING)), 4.0, 3.0, t.over.with_alpha(0.7), 1.0);
        }
        kit::curve(s, pts, t.accent, 2.0);
        // The knee handle.
        let thr = Self::thr_abs(cx, b);
        let kx = m.x(thr.clamp(-60.0, 0.0));
        let ky = m.y(Self::curve_out(cx, b, thr.clamp(-60.0, 0.0)));
        s.fill_ellipse_solid(Scene::ellipse_at(Point::new(kx, ky), 4.5), t.panel);
        s.stroke_ellipse(Scene::ellipse_at(Point::new(kx, ky), 4.5), t.accent, 1.5);
        // The operating dot.
        if let (Some(det), Some(gain)) = (Self::view(live, vw::DETECTOR + b), Self::view(live, vw::GAIN + b)) {
            if det > -60.0 {
                let out = det + Self::applied(cx, b, gain);
                let (dx, dy) = (m.x(det.min(0.0)), m.y(out.clamp(-60.0, 0.0)));
                let uy = m.y(det.min(0.0));
                if (uy - dy).abs() > 1.0 {
                    s.line(Point::new(dx, uy), Point::new(dx, dy), t.accent.with_alpha(0.35), 3.0);
                }
                let colour = if Self::view(live, vw::KEYED).is_some_and(|v| v >= 0.5) { t.warm } else { t.accent };
                s.fill_ellipse_solid(Scene::ellipse_at(Point::new(dx, dy), 4.0), colour);
            }
        }
        s.pop_clip();
        kit::text_right(s, g.curve.0 + g.curve.2 - 8.0, g.curve.1 + g.curve.3 - 12.0, NAMES[b], t.bands[b], 9.0);
        if !g.tall {
            return;
        }
        // Read-outs of the selected band.
        let line = |s: &mut Scene, y: f32, label: &str, value: String| {
            s.text(Point::new(880.0, y), label, t.dim, 9.0, false);
            s.text(Point::new(912.0, y), value, t.text, 10.5, false);
        };
        // Live values in whole dB: tenths changed on every frame and could not be read.
        let db_or = |v: Option<f32>| v.filter(|v| *v > -119.0).map_or("-".to_string(), |v| format!("{:.0} dB", v.round() + 0.0));
        line(s, 469.0, "IN", db_or(Self::view(live, vw::DETECTOR + b)));
        let gr = Self::view(live, vw::GR + b).unwrap_or(0.0);
        let lim = Self::view(live, vw::LIMIT + b).unwrap_or(0.0);
        let whole = |v: f32| v.round() + 0.0;
        let gtext = if cx.on(cb::P_BAND_LIMITER) { format!("{:.0} dB (lim {:.0})", whole(-gr), whole(lim)) } else { format!("{:.0} dB", whole(-gr)) };
        line(s, 487.0, "GR", gtext);
        line(s, 505.0, "OUT", db_or(Self::view(live, vw::OUTPUT + b)));
        let mb = g.max_box();
        max_box(s, t, mb, self.max);
        s.text(Point::new(mb.0 + 52.0, mb.1 + 9.0), "MAX, click to reset", t.dim, 8.5, false);
    }

    fn paint_controls(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let mode = Self::mode(cx);
        if cx.more {
            for k in 0..4 {
                let off = Self::split_off(k, cx.v(cb::P_SPLIT_1 + k));
                pill(s, t, off_pill(k), "OFF", off);
            }
            s.text(Point::new(136.0, 706.0), format!("COPY {} TO", NAMES[self.sel]), t.dim, 9.0, false);
            for (b, r) in copy_buttons(self.sel) {
                kit::button(s, t, r, SHORT[b], false);
            }
            s.text(Point::new(136.0 + 4.0 * 40.0 + 4.0, COPY_Y + 12.0), "", t.dim, 9.0, false);
            kit::button(s, t, RESET_ALL, "RESET ALL BANDS", false);
            return;
        }
        // Band navigation.
        for (r, dir) in [(PREV, -1.0f32), (NEXT, 1.0)] {
            kit::button(s, t, r, "", false);
            arrow(s, t, r, dir);
        }
        let b = self.sel;
        s.text(Point::new(176.0, 651.0), NAMES[b], t.bands[b], 13.0, true);
        let s4 = Self::splits(cx);
        let lo = if b == 0 { 20.0 } else { s4[b - 1] };
        let hi = if b == 4 { 20000.0 } else { s4[b] };
        let empty = (b == 0 && Self::split_off(0, s4[0])) || (b == 1 && Self::split_off(1, s4[1])) || (b == 3 && Self::split_off(2, s4[2])) || (b == 4 && Self::split_off(3, s4[3]));
        let range = if empty { "empty (split off)".to_string() } else { format!("{} – {}", hz_text(lo), hz_text(hi)) };
        s.text(Point::new(176.0, 672.0), text_fit(&range, 9.0, 112.0), t.dim, 9.0, true);
        pill(s, t, ALL, "ALL", self.all);
        let _ = mode;
        for g in self.gangs(cx) {
            g.paint(s, t, cx);
        }
    }

    fn control_rects(&self, cx: &Ctx) -> Vec<R> {
        if cx.more {
            let mut v: Vec<R> = (0..4).map(off_pill).collect();
            v.push((136.0, 698.0, 316.0, 46.0));
            v
        } else if self.all {
            vec![NAV, (240.0, 636.0, 600.0, 124.0), LIFT_R, CEIL_R]
        } else {
            vec![NAV]
        }
    }
}

/// Shift makes page drags ten times finer.
fn fine(io: &Io) -> f32 {
    if io.modifiers().shift {
        0.1
    } else {
        1.0
    }
}

fn chip_toggle(r: R, i: usize) -> R {
    let x = r.0 + r.2 - 46.0 + i as f32 * 22.0;
    (x, r.1 + 2.0, 18.0, 18.0)
}

impl Page for Bands {
    fn tall(&self) -> bool {
        true
    }

    fn has_more(&self) -> bool {
        true
    }

    fn key_param(&self, cx: &Ctx) -> Option<usize> {
        cx.find("keySource")
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let z = cx.zones;
        let hy = z.header.1;
        let mut v = vec![
            Cell::segmented(cx.p(cb::P_MODE), "MODE", (420.0, hy + 8.0, 272.0, 24.0)).keys(MODE_KEYS).tip("Downward, parallel, parallel with a detector per channel, low-level lift"),
            Cell::pill(cx.p(cb::P_BAND_LIMITER), "BAND LIMIT", (700.0, hy + 9.0, 88.0, 22.0)).tip("A peak limiter on every band"),
        ];
        if cx.more {
            for k in 0..4 {
                v.push(Cell::value(cx.p(cb::P_SPLIT_1 + k), format!("SPLIT {}", k + 1), (SPLIT_X[k], 664.0, 84.0, 22.0)).format(split_text));
            }
            let lift = Self::mode(cx) != 3;
            v.push(Cell::knob(Size::Small, cx.p(cb::P_FLOOR_GUARD_THRESHOLD), "GUARD THRESH", GUARD_T.0, GUARD_T.1).tip("Floor guard threshold, relative to the reference (LIFT)").dim(lift));
            v.push(Cell::knob(Size::Small, cx.p(cb::P_FLOOR_GUARD_RATIO), "GUARD RATIO", GUARD_R.0, GUARD_R.1).format(guard_ratio_text).tip("How much of the lift the floor guard takes back (LIFT)").dim(lift));
            v.push(Cell::knob(Size::Small, cx.p(cb::P_BAND_LIMITER_RELEASE), "LIMIT RELEASE", LIM_REL.0, LIM_REL.1).tip("Band limiter release (BAND LIMIT)").dim(!cx.on(cb::P_BAND_LIMITER)));
            return v;
        }
        let b = self.sel;
        if !self.all {
            v.extend(self.big_cells(cx));
        }
        v.push(Cell::pill(Self::p(cx, b, cb::B_SOLO), "SOLO", (124.0, 686.0, 50.0, 22.0)));
        v.push(Cell::pill(Self::p(cx, b, cb::B_BYPASS), "BYP", (178.0, 686.0, 50.0, 22.0)));
        v.push(Cell::knob(Size::Mini, cx.p(cb::P_WET_GAIN), "WET", WET_R.0, WET_R.1).format(wet_text).tip("Wet gain of the parallel modes").dim(!matches!(Self::mode(cx), 1 | 2)));
        v.push(Cell::knob(Size::Mini, cx.p(cb::P_REFERENCE), "REF", 1016.0, 636.0).tip("Reference level: every threshold moves with it"));
        v.push(Cell::knob(Size::Mini, cx.p(cb::P_LOOKAHEAD), "L-AHEAD", 848.0, 700.0));
        v.push(Cell::dropdown(cx.p(cb::P_DETECTOR), "DETECTOR", (912.0, 718.0, 168.0, 24.0)));
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let g = Geo::new(cx.zones.tall);
        // Graph.
        kit::plot(s, t, g.graph);
        let mut f = 10f32;
        while f <= 20000.0 {
            for m in 1..10 {
                let ff = f * m as f32;
                if !(20.0..=20000.0).contains(&ff) {
                    continue;
                }
                let x = fx(ff);
                s.line(Point::new(x, g.gr_top), Point::new(x, 592.0), if m == 1 { t.grid_strong } else { t.grid }, 1.0);
                if matches!(m, 1 | 2 | 5) {
                    s.text(Point::new(x, 604.0), hz_short(ff), t.dim, 8.5, true);
                }
            }
            f *= 10.0;
        }
        let mut l = 0.0;
        while l >= g.lv_min - 0.1 {
            let y = g.ly(l);
            s.line(Point::new(X0, y), Point::new(X1, y), if l == 0.0 { t.grid_strong } else { t.grid }, 1.0);
            kit::text_right(s, X0 - 4.0, y, &format!("{l:.0}"), t.dim, 8.0);
            l -= 12.0;
        }
        s.line(Point::new(X0, g.gr_bot + 2.0), Point::new(X1, g.gr_bot + 2.0), t.grid_strong, 1.0);
        kit::text_right(s, X0 - 4.0, g.gr0, "GR", t.dim, 8.0);
        if g.tall {
            for d in [6.0, 12.0] {
                let y = g.gr0 + d * g.gr_ppd;
                s.line(Point::new(X0, y), Point::new(X1, y), t.grid, 1.0);
                kit::text_right(s, X0 - 4.0, y, &format!("{d:.0}"), t.dim, 8.0);
            }
        }
        // Transfer curve frame.
        curve_frame(s, t, g.curve);
        if g.tall {
            s.text(Point::new(880.0, 532.0), "One band alone. Overlapping bands", t.dim, 9.0, false);
            s.text(Point::new(880.0, 545.0), "make the summed curve softer.", t.dim, 9.0, false);
        }
        // Control zone.
        if cx.more {
            for k in 0..4 {
                s.text(Point::new(SPLIT_X[k], 648.0), SPLIT_LABEL[k], t.dim, 9.0, false);
            }
        } else {
            s.line(Point::new(844.0, 636.0), Point::new(844.0, 760.0), t.border, 1.0);
            s.text(Point::new(912.0, 706.0), "DETECTOR", t.dim, 9.0, false);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let g = Geo::new(cx.zones.tall);
        self.paint_graph(s, cx, live, &g);
        self.paint_curve(s, cx, live, &g);
        self.paint_controls(s, cx);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let g = Geo::new(cx.zones.tall);
        let side = if g.tall { (876.0, 248.0, 208.0, 336.0) } else { (884.0, 420.0, 192.0, 192.0) };
        let mut v = vec![g.graph, side];
        v.extend(self.control_rects(cx));
        v
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        matches!(local, cb::P_MODE | cb::P_BAND_LIMITER)
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max};
        &[
            Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Last, Last, Last, Last, Last, Max, Max, Max, Max, Max, Max, Max, Max, Max, Max, Last,
        ]
    }

    fn tick(&mut self, _cx: &Ctx, live: &Live) -> bool {
        if live.view.len() >= vw::LEN {
            for b in 0..BANDS {
                let r = live.view[vw::GR + b] + live.view[vw::LIMIT + b];
                if r >= self.hold[b] {
                    self.hold[b] = r;
                    self.age[b] = 0.0;
                } else {
                    self.age[b] += live.dt;
                    if self.age[b] > 1.5 {
                        self.hold[b] = r;
                    }
                }
                self.max = self.max.max(r);
            }
        }
        true
    }

    fn states(&self) -> usize {
        BANDS
    }

    fn set_state(&mut self, i: usize) {
        self.sel = i % BANDS;
    }

    fn menu(&self, _cx: &Ctx) -> Vec<String> {
        vec![format!("Copy {} to every band", NAMES[self.sel]), "Reset all bands".into(), "Clear the peak holds".into()]
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        match item {
            0 => {
                let to: Vec<usize> = (0..BANDS).filter(|&b| b != self.sel).collect();
                self.copy_band(cx, io, &to);
            }
            1 => Self::reset_bands(cx, io),
            _ => self.reset_holds(),
        }
        Response::TAKEN
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let g = Geo::new(cx.zones.tall);
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let mut v = vec![
            tip((X0, g.lv_top, X1 - X0, LV_BOT - g.lv_top), "Click a band to select it. Drag a split line to move it (to the edge switches it off); drag a threshold grip up or down; double-click resets"),
            tip((X0, g.gr_top, X1 - X0, g.gr_bot - g.gr_top), "Reduction per band; hatched: band limiter"),
            tip(g.curve, "Selected band alone: drag the knee sideways for the threshold, above it up or down for the ratio"),
        ];
        if !cx.more {
            v.push(tip(ALL, "While on, a knob edit moves all five bands by the same amount"));
        }
        v
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let g = Geo::new(cx.zones.tall);
        // Drags in progress.
        if let Some(drag) = self.drag.clone() {
            let (x, y, up) = match ev {
                Pointer::Move { x, y } => (x, y, false),
                Pointer::Up { x, y } => (x, y, true),
                _ => return Response::TAKEN,
            };
            match drag {
                Drag::Gang(mut d) => {
                    let gangs = self.gangs(cx);
                    gang_move(&gangs, &mut d, y, io, up);
                    self.drag = if up { None } else { Some(Drag::Gang(d)) };
                    return Response::TAKEN;
                }
                Drag::Split { k } => {
                    let p = cx.p(cb::P_SPLIT_1 + k);
                    io.perform(p, self.split_target(cx, k, x));
                    if up {
                        io.end(p);
                    }
                }
                Drag::Thresh { mut params, last, ppd } => {
                    let d = (last - y) / ppd * fine(io);
                    for (p, v) in params.iter_mut() {
                        *v += d;
                        io.perform(*p, *v);
                        if up {
                            io.end(*p);
                        }
                    }
                    self.drag = Some(Drag::Thresh { params, last: y, ppd });
                }
                Drag::CurveThr { param, last, mut v, ppd } => {
                    v += (x - last) / ppd * fine(io);
                    io.perform(param, v);
                    if up {
                        io.end(param);
                    }
                    self.drag = Some(Drag::CurveThr { param, last: x, v, ppd });
                }
                Drag::CurveRatio { param, last, mut n } => {
                    // Up flattens the curve (a lower ratio).
                    n = (n - (last - y) / 200.0 * fine(io)).clamp(0.0, 1.0);
                    io.perform(param, INFO.params[param].from_normalized(n));
                    if up {
                        io.end(param);
                    }
                    self.drag = Some(Drag::CurveRatio { param, last: y, n });
                }
            }
            if up {
                self.drag = None;
            }
            return Response::TAKEN;
        }
        match ev {
            Pointer::Down { x, y, clicks } => self.down(x, y, clicks, cx, io, &g),
            Pointer::Wheel { x, y, dy } => {
                let gangs = self.gangs(cx);
                if gang_wheel(&gangs, x, y, dy, io) {
                    return Response::TAKEN;
                }
                Response::NONE
            }
            _ => Response::NONE,
        }
    }
}

impl Bands {
    /// Where a split dragged to `x` goes: a third of an octave from its neighbours, off at
    /// the outer edge for the two lower and the two upper splits.
    fn split_target(&mut self, cx: &Ctx, k: usize, x: f32) -> f32 {
        let info = &INFO.params[cx.p(cb::P_SPLIT_1 + k)];
        let s = Self::splits(cx);
        if k < 2 && x <= X0 + 3.0 {
            return cb::SPLIT_LOW_OFF_HZ;
        }
        if k >= 2 && x >= X1 - 3.0 {
            return cb::SPLIT_HIGH_OFF_HZ;
        }
        let mut f = xf(x);
        let below = (0..k).rev().find(|&j| !Self::split_off(j, s[j])).map(|j| s[j]);
        let above = (k + 1..4).find(|&j| !Self::split_off(j, s[j])).map(|j| s[j]);
        if let Some(lo) = below {
            f = f.max(lo * SPLIT_GAP);
        }
        if let Some(hi) = above {
            f = f.min(hi / SPLIT_GAP);
        }
        let f = f.clamp(info.min.max(if k < 2 { 21.0 } else { info.min }), info.max.min(if k >= 2 { 19_900.0 } else { info.max }));
        self.remembered[k] = f;
        f
    }

    fn down(&mut self, x: f32, y: f32, clicks: u32, cx: &Ctx, io: &mut Io, g: &Geo) -> Response {
        let gangs = self.gangs(cx);
        if let Some(d) = gang_down(&gangs, x, y, clicks, io) {
            if let Some(d) = d {
                self.drag = Some(Drag::Gang(d));
            }
            return Response::TAKEN;
        }
        if cx.more {
            for k in 0..4 {
                if kit::inside(off_pill(k), x, y) {
                    let p = cx.p(cb::P_SPLIT_1 + k);
                    let v = cx.v(cb::P_SPLIT_1 + k);
                    if Self::split_off(k, v) {
                        io.set(p, self.remembered[k]);
                    } else {
                        self.remembered[k] = v;
                        io.set(p, if k < 2 { cb::SPLIT_LOW_OFF_HZ } else { cb::SPLIT_HIGH_OFF_HZ });
                    }
                    return Response::TAKEN;
                }
            }
            for (b, r) in copy_buttons(self.sel) {
                if kit::inside(r, x, y) {
                    self.copy_band(cx, io, &[b]);
                    return Response::TAKEN;
                }
            }
            if kit::inside(RESET_ALL, x, y) {
                Self::reset_bands(cx, io);
                return Response::TAKEN;
            }
        } else {
            if kit::inside(PREV, x, y) || kit::inside(NEXT, x, y) {
                let step = if kit::inside(PREV, x, y) { BANDS - 1 } else { 1 };
                self.sel = (self.sel + step) % BANDS;
                return Response::REBUILD;
            }
            if kit::inside(ALL, x, y) {
                self.all = !self.all;
                return Response::REBUILD;
            }
        }
        if g.tall && kit::inside(g.max_box(), x, y) {
            self.reset_holds();
            return Response::TAKEN;
        }
        // The transfer curve.
        if kit::inside(g.curve, x, y) {
            let m = Map::square(g.curve);
            let b = self.sel;
            let thr = Self::thr_abs(cx, b).clamp(-60.0, 0.0);
            let (kx, ky) = (m.x(thr), m.y(Self::curve_out(cx, b, thr)));
            let pt = Self::p(cx, b, cb::B_THRESHOLD);
            if (x - kx).abs() <= 9.0 && (y - ky).abs() <= 9.0 {
                if clicks >= 2 {
                    io.set(pt, INFO.params[pt].default);
                    return Response::TAKEN;
                }
                io.begin(pt);
                self.drag = Some(Drag::CurveThr { param: pt, last: x, v: Self::bv(cx, b, cb::B_THRESHOLD), ppd: m.ppd });
                return Response::TAKEN;
            }
            if x > kx {
                let pr = Self::p(cx, b, cb::B_RATIO);
                if clicks >= 2 {
                    io.set(pr, INFO.params[pr].default);
                    return Response::TAKEN;
                }
                io.begin(pr);
                self.drag = Some(Drag::CurveRatio { param: pr, last: y, n: INFO.params[pr].to_normalized(Self::bv(cx, b, cb::B_RATIO)) });
                return Response::TAKEN;
            }
            return Response::TAKEN;
        }
        // The graph.
        if !(x >= X0 - 6.0 && x <= X1 + 6.0 && y >= g.gr_top && y <= CHIP_Y + 22.0) {
            return Response::NONE;
        }
        let e = Self::edges(cx);
        // Band chips: S, B, the name.
        for b in 0..BANDS {
            let (a, z) = Self::band_span(&e, b);
            let w = (z - a - 8.0).min(120.0);
            if w < 24.0 {
                continue;
            }
            let r = ((a + z) * 0.5 - w * 0.5, CHIP_Y, w, 22.0);
            if !kit::inside(r, x, y) {
                continue;
            }
            if w >= 76.0 {
                for (i, off) in [cb::B_SOLO, cb::B_BYPASS].into_iter().enumerate() {
                    if kit::inside(chip_toggle(r, i), x, y) {
                        let p = Self::p(cx, b, off);
                        let on = cx.on(band_param(b, off));
                        if off == cb::B_SOLO && (io.modifiers().alt || clicks >= 2) {
                            // Alt-click (or double-click): this band solo, every other one not.
                            for o in 0..BANDS {
                                io.set(Self::p(cx, o, cb::B_SOLO), if o == b { 1.0 } else { 0.0 });
                            }
                        } else {
                            io.set(p, if on { 0.0 } else { 1.0 });
                        }
                        return Response::TAKEN;
                    }
                }
            }
            if b != self.sel {
                self.sel = b;
                return Response::REBUILD;
            }
            return Response::TAKEN;
        }
        // Split grips and lines.
        let s4 = Self::splits(cx);
        let near_split = (0..4).filter(|&k| (x - e[k + 1]).abs() <= 5.0 && y < CHIP_Y).min_by(|&a, &b| (x - e[a + 1]).abs().total_cmp(&(x - e[b + 1]).abs()));
        if let Some(k) = near_split {
            let p = cx.p(cb::P_SPLIT_1 + k);
            if clicks >= 2 {
                io.set(p, INFO.params[p].default);
                return Response::TAKEN;
            }
            if !Self::split_off(k, s4[k]) {
                self.remembered[k] = s4[k];
            }
            io.begin(p);
            self.drag = Some(Drag::Split { k });
            return Response::TAKEN;
        }
        // Threshold grips.
        for b in 0..BANDS {
            let (a, z) = Self::band_span(&e, b);
            if z - a < 10.0 {
                continue;
            }
            let ty = g.ly(Self::thr_abs(cx, b));
            let mid = (a + z) * 0.5;
            if (x - mid).abs() <= 16.0 && (y - ty).abs() <= 7.0 {
                // Cmd-drag (or ALL) moves all five by the same amount.
                let bands: Vec<usize> = if self.all || io.modifiers().command { (0..BANDS).collect() } else { vec![b] };
                if clicks >= 2 {
                    for o in bands {
                        let p = Self::p(cx, o, cb::B_THRESHOLD);
                        io.set(p, INFO.params[p].default);
                    }
                    return Response::TAKEN;
                }
                let params: Vec<(usize, f32)> = bands.iter().map(|&o| (Self::p(cx, o, cb::B_THRESHOLD), Self::bv(cx, o, cb::B_THRESHOLD))).collect();
                for &(p, _) in &params {
                    io.begin(p);
                }
                self.drag = Some(Drag::Thresh { params, last: y, ppd: g.lv_ppd });
                return if b != self.sel {
                    self.sel = b;
                    Response::REBUILD
                } else {
                    Response::TAKEN
                };
            }
        }
        // Anywhere else: select the band under the pointer.
        let b = Self::band_at(cx, x);
        if b != self.sel {
            self.sel = b;
            return Response::REBUILD;
        }
        Response::TAKEN
    }
}

/// What the three compressor pages share: small painters, the transfer-curve frame, and the
/// editor-driven controls that write several parameters in one gesture (ALL, LINK) or stop
/// short of a parameter's range (POST TRIM).
#[allow(dead_code)]
pub(super) mod common {
    use openplugin::dri::goodlookindri::color::Color as C;
    use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
    use openplugin::dri::goodlookindri_shell::text;

    use crate::ui::kit::{self, Cell, Kind, Size};
    use crate::ui::page::{Ctx, Io};
    use crate::ui::raster::R;
    use crate::ui::theme::Theme;
    use crate::INFO;

    pub fn db_lin(db: f32) -> f32 {
        10f32.powf(db / 20.0)
    }

    pub fn lin_db(v: f32) -> f32 {
        20.0 * v.max(1.0e-9).log10()
    }

    pub fn hz_text(f: f32) -> String {
        if f >= 1000.0 {
            format!("{:.1} kHz", f / 1000.0)
        } else {
            format!("{f:.0} Hz")
        }
    }

    pub fn hz_short(f: f32) -> String {
        if f >= 1000.0 {
            format!("{}k", f / 1000.0)
        } else {
            format!("{f}")
        }
    }

    /// Times the DualSone way: three digits, seconds from 1000 ms, microseconds under 1 ms.
    pub fn time_text(ms: f32) -> String {
        if ms >= 1000.0 {
            let s = ms / 1000.0;
            if s >= 10.0 {
                format!("{s:.1} s")
            } else {
                format!("{s:.2} s")
            }
        } else if ms < 1.0 {
            format!("{:.0} µs", ms * 1000.0)
        } else if ms < 10.0 {
            format!("{ms:.2} ms")
        } else if ms < 100.0 {
            format!("{ms:.1} ms")
        } else {
            format!("{ms:.0} ms")
        }
    }

    pub fn text_fit(label: &str, size: f32, w: f32) -> String {
        text::ellipsize(label, size, w)
    }

    /// Fades a cell whose setting does nothing right now (the shell's cells are built once
    /// per view; this follows the parameters).
    pub fn veil(s: &mut Scene, t: &Theme, r: R) {
        let c = t.panel.with_alpha(0.62);
        s.fill_rect(kit::rect((r.0 - 1.0, r.1 - 1.0, r.2 + 2.0, r.3 + 2.0)), c, c, true);
    }

    /// A knob's label, painted by the page because it follows a parameter.
    pub fn knob_label(s: &mut Scene, t: &Theme, r: R, size: Size, label: &str, faded: bool) {
        let (pt, y) = match size {
            Size::Big => (10.0, r.1 + 7.0),
            Size::Small => (9.0, r.1 + 6.0),
            Size::Mini => (8.5, r.1 + 50.0),
        };
        let label = text::ellipsize(&label.to_uppercase(), pt, r.2 - 4.0);
        s.text(Point::new(r.0 + r.2 * 0.5, y), label, t.dim.with_alpha(if faded { 0.4 } else { 1.0 }), pt, true);
    }

    pub fn sub_caption(s: &mut Scene, t: &Theme, r: R, label: &str) {
        s.text(Point::new(r.0 + r.2 * 0.5, r.1 + r.3 + 5.0), label, t.dim, 8.5, true);
    }

    /// A peak box: the number in an inset.
    pub fn max_box(s: &mut Scene, t: &Theme, r: R, db: f32) {
        kit::inset(s, t, r, 4.0);
        let label = if db < 0.05 { "0.0".to_string() } else { format!("-{db:.1}") };
        s.text(kit::rect(r).center(), label, t.text, 9.5, true);
    }

    /// Diagonal hatching inside `r`.
    pub fn hatch(s: &mut Scene, r: R, c: C, step: f32) {
        if r.2 <= 0.5 || r.3 <= 0.5 {
            return;
        }
        s.clip_round(kit::rect(r), 0.0);
        let mut k = -r.3;
        // In pieces about 8 pt long: one long diagonal costs its whole square box.
        let pieces = (r.3 / 6.0).ceil().max(1.0) as usize;
        let d = r.3 / pieces as f32;
        while k < r.2 {
            for i in 0..pieces {
                let (u, v) = (i as f32 * d, (i + 1) as f32 * d);
                if k + v < 0.0 || k + u > r.2 {
                    continue; // outside the rect: the clip would drop it anyway
                }
                s.line(Point::new(r.0 + k + u, r.1 + r.3 - u), Point::new(r.0 + k + v, r.1 + r.3 - v), c, 1.0);
            }
            k += step;
        }
        s.pop_clip();
    }

    /// A pill the page draws itself (an editor-only switch).
    pub fn pill(s: &mut Scene, t: &Theme, r: R, label: &str, on: bool) {
        let rr = kit::rect(r);
        let radius = r.3 * 0.5;
        if on {
            s.fill_round(rr, radius, t.accent, t.accent, true);
        } else {
            s.fill_round(rr, radius, t.inset, t.inset, true);
            s.stroke_round(rr, radius, t.border, 1.0);
        }
        s.text(rr.center(), label, if on { t.on_accent } else { t.text }, 9.5, true);
    }

    /// A warning chip in `over` with its "!".
    pub fn warn_chip(s: &mut Scene, t: &Theme, r: R, label: &str) {
        let rr = kit::rect(r);
        s.fill_round(rr, r.3 * 0.5, t.over, t.over, true);
        s.text(rr.center(), label, t.on_accent, 9.0, true);
    }

    pub fn round_toggle(s: &mut Scene, t: &Theme, r: R, label: &str, on: bool, colour: C) {
        let c = kit::rect(r).center();
        let e = Scene::ellipse_at(c, r.2 * 0.5);
        if on {
            s.fill_ellipse_solid(e, colour);
        } else {
            s.fill_ellipse_solid(e, t.inset);
            s.stroke_ellipse(e, t.border, 1.0);
        }
        let ink = if !on { t.dim } else if colour == t.text { t.panel } else { t.on_accent };
        s.text(c, label, ink, 8.5, true);
    }

    pub fn arrow(s: &mut Scene, t: &Theme, r: R, dir: f32) {
        let (cx, cy) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5);
        s.stroke_poly(vec![Point::new(cx - 2.0 * dir, cy - 4.0), Point::new(cx + 2.0 * dir, cy), Point::new(cx - 2.0 * dir, cy + 4.0)], t.text, 1.4, false);
    }

    /// A square level map, -60..0 dB on both axes.
    pub struct Map {
        pub r: R,
        pub ppd: f32,
    }

    impl Map {
        pub fn square(r: R) -> Self {
            Self { r, ppd: r.2 / 60.0 }
        }

        pub fn x(&self, db: f32) -> f32 {
            self.r.0 + (db + 60.0) * self.ppd
        }

        pub fn y(&self, db: f32) -> f32 {
            self.r.1 + self.r.3 - (db + 60.0) * self.ppd
        }

        pub fn level_at(&self, x: f32) -> f32 {
            (x - self.r.0) / self.ppd - 60.0
        }
    }

    /// The transfer curve's frame: grid every 12 dB and the unity line. Static.
    pub fn curve_frame(s: &mut Scene, t: &Theme, r: R) {
        kit::plot(s, t, r);
        let m = Map::square(r);
        for k in 1..5 {
            let d = -12.0 * k as f32;
            s.line(Point::new(m.x(d), r.1), Point::new(m.x(d), r.1 + r.3), t.grid, 1.0);
            s.line(Point::new(r.0, m.y(d)), Point::new(r.0 + r.2, m.y(d)), t.grid, 1.0);
        }
        kit::dashed(s, &[Point::new(m.x(-60.0), m.y(-60.0)), Point::new(m.x(0.0), m.y(0.0))], 3.0, 3.0, t.grid_strong, 1.0);
    }

    /// A GR meter column with its scale and max box: bar `r` (36 wide, the bar 14), box below.
    pub fn gr_column(s: &mut Scene, t: &Theme, r: R, label: &str) {
        kit::gr_scale(s, t, r, 20.0);
        s.text(Point::new(r.0 + 7.0, r.1 + r.3 + 34.0), label, t.dim, 8.0, true);
    }

    // ---- controls the page drives itself ----

    #[derive(Clone, Copy, Debug)]
    pub enum GKind {
        Knob(Size),
        Pill,
        Seg(&'static [&'static str]),
    }

    /// One control on several parameters (the first leads), or on one parameter with a
    /// travel shorter than its range.
    #[derive(Clone, Debug)]
    pub struct Gang {
        pub kind: GKind,
        pub rect: R,
        pub params: Vec<usize>,
        pub label: String,
        /// The others move by the lead's change (true) or take its value (false).
        pub offset: bool,
        /// Travel in plain units, when shorter than the range.
        pub range: Option<(f32, f32)>,
        pub format: Option<fn(f32) -> String>,
        pub faded: bool,
    }

    #[derive(Clone, Debug)]
    pub struct GangDrag {
        pub idx: usize,
        pub y: f32,
        pub norm: Vec<f32>,
    }

    impl Gang {
        pub fn knob(size: Size, rect: R, params: Vec<usize>, label: &str, offset: bool) -> Self {
            Self { kind: GKind::Knob(size), rect, params, label: label.into(), offset, range: None, format: None, faded: false }
        }

        pub fn new(kind: GKind, rect: R, params: Vec<usize>, label: &str) -> Self {
            Self { kind, rect, params, label: label.into(), offset: false, range: None, format: None, faded: false }
        }

        pub fn format(mut self, f: Option<fn(f32) -> String>) -> Self {
            self.format = f;
            self
        }

        pub fn faded(mut self, f: bool) -> Self {
            self.faded = f;
            self
        }

        pub fn range(mut self, lo: f32, hi: f32) -> Self {
            self.range = Some((lo, hi));
            self
        }

        fn norm(&self, p: usize, v: f32) -> f32 {
            match self.range {
                Some((lo, hi)) => ((v - lo) / (hi - lo)).clamp(0.0, 1.0),
                None => INFO.params[p].to_normalized(v),
            }
        }

        fn plain(&self, p: usize, n: f32) -> f32 {
            let info = &INFO.params[p];
            match self.range {
                Some((lo, hi)) => info.clamp(lo + (hi - lo) * n.clamp(0.0, 1.0)),
                None => info.from_normalized(n),
            }
        }

        fn default(&self, p: usize) -> f32 {
            let d = INFO.params[p].default;
            match self.range {
                Some((lo, hi)) => d.clamp(lo, hi),
                None => d,
            }
        }

        fn hit(&self, x: f32, y: f32) -> bool {
            let r = self.rect;
            match self.kind {
                GKind::Knob(_) => kit::inside((r.0, r.1 + 12.0, r.2, r.3 - 12.0), x, y),
                _ => kit::inside(r, x, y),
            }
        }

        pub fn paint(&self, s: &mut Scene, t: &Theme, cx: &Ctx) {
            let Some(&lead) = self.params.first() else { return };
            let v = cx.values.get(lead).copied().unwrap_or(0.0);
            let info = &INFO.params[lead];
            let ink = if self.faded { t.dim } else { t.text };
            match self.kind {
                GKind::Knob(size) => {
                    let mut cell = Cell::new(Kind::Knob(size), lead, self.label.clone(), self.rect);
                    cell.dim = self.faded;
                    // The chrome kit draws for a knob cell; this one is the page's.
                    s.fill_rect(kit::rect(self.rect), t.panel, t.panel, true);
                    kit::paint_static(s, t, &cell);
                    let (cx_, cy, r) = kit::knob_circle(&cell, size);
                    let ring = match size {
                        Size::Big => 4.0,
                        Size::Small => 3.0,
                        Size::Mini => 2.0,
                    };
                    let n = self.norm(lead, v);
                    let origin = if self.range.is_none() && info.min < 0.0 && info.max > 0.0 && (info.min + info.max).abs() < 1e-3 { 0.5 } else { 0.0 };
                    let over = self.range.is_some_and(|(_, hi)| v > hi + 1e-4);
                    let colour = if over { t.over } else if self.faded { t.dim.with_alpha(0.45) } else { t.accent };
                    if (n - origin).abs() > 0.004 {
                        s.stroke_arc(Point::new(cx_, cy), r + ring + 2.0, kit::knob_angle(origin.min(n)), kit::knob_angle(origin.max(n)), colour, ring);
                    }
                    let a = kit::knob_angle(n);
                    let width = match size {
                        Size::Big => 3.0,
                        Size::Small => 2.4,
                        Size::Mini => 1.8,
                    };
                    s.line(Point::new(cx_ + a.cos() * r * 0.42, cy + a.sin() * r * 0.42), Point::new(cx_ + a.cos() * r * 0.86, cy + a.sin() * r * 0.86), ink, width);
                    let b = kit::knob_value_box(&cell, size);
                    let pt = match size {
                        Size::Big => 12.0,
                        Size::Small => 10.5,
                        Size::Mini => 9.5,
                    };
                    let shown = match self.format {
                        Some(f) => f(v),
                        None => info.format(v),
                    };
                    s.text(Point::new(b.0 + b.2 * 0.5, b.1 + b.3 * 0.5), text::ellipsize(&shown, pt, b.2 - 4.0), if over { t.over } else { ink }, pt, true);
                    if self.params.len() > 1 {
                        // Ganged: a small mark over the knob.
                        s.fill_ellipse_solid(Scene::ellipse_at(Point::new(cx_ + r + ring + 6.0, cy - r - ring), 2.5), t.accent);
                    }
                }
                GKind::Pill => {
                    s.fill_rect(kit::rect(self.rect), t.panel, t.panel, true);
                    pill(s, t, self.rect, &self.label, v >= (info.min + info.max) * 0.5);
                }
                GKind::Seg(keys) => {
                    s.fill_rect(kit::rect(self.rect), t.panel, t.panel, true);
                    kit::inset(s, t, self.rect, 6.0);
                    let r = kit::rect(self.rect);
                    let kw = r.w / keys.len().max(1) as f32;
                    let chosen = (v - info.min).round().max(0.0) as usize;
                    for (i, label) in keys.iter().enumerate() {
                        let key = Rect::new(r.x + kw * i as f32 + 2.0, r.y + 2.0, kw - 4.0, r.h - 4.0);
                        if i == chosen {
                            s.fill_round(key, 4.0, t.accent, t.accent, true);
                        }
                        s.text(key.center(), text::ellipsize(label, 9.0, kw - 6.0), if i == chosen { t.on_accent } else { t.dim }, 9.0, true);
                    }
                }
            }
        }
    }

    /// A press on one of `gangs`: `None` if none was hit, `Some(Some(drag))` when a knob drag
    /// starts, `Some(None)` when the press was a single change.
    pub fn gang_down(gangs: &[Gang], x: f32, y: f32, clicks: u32, io: &mut Io) -> Option<Option<GangDrag>> {
        let (idx, g) = gangs.iter().enumerate().find(|(_, g)| g.hit(x, y))?;
        let lead = *g.params.first()?;
        let info = &INFO.params[lead];
        match g.kind {
            GKind::Knob(_) => {
                if clicks >= 2 {
                    for &p in &g.params {
                        io.set(p, g.default(if g.offset { p } else { lead }));
                    }
                    return Some(None);
                }
                for &p in &g.params {
                    io.begin(p);
                }
                let norm = g.params.iter().map(|&p| g.norm(p, io.value(p))).collect();
                Some(Some(GangDrag { idx, y, norm }))
            }
            GKind::Pill => {
                let on = io.value(lead) >= (info.min + info.max) * 0.5;
                for &p in &g.params {
                    io.set(p, if on { INFO.params[p].min } else { INFO.params[p].max });
                }
                Some(None)
            }
            GKind::Seg(keys) => {
                let kw = g.rect.2 / keys.len().max(1) as f32;
                let key = ((x - g.rect.0) / kw).floor().clamp(0.0, keys.len() as f32 - 1.0);
                for &p in &g.params {
                    io.set(p, INFO.params[p].min + key);
                }
                Some(None)
            }
        }
    }

    /// A knob drag on a gang moves to `y`; `up` ends it.
    pub fn gang_move(gangs: &[Gang], d: &mut GangDrag, y: f32, io: &mut Io, up: bool) {
        let Some(g) = gangs.get(d.idx) else { return };
        let delta = (d.y - y) / 200.0;
        for (i, &p) in g.params.iter().enumerate() {
            let n = if g.offset { d.norm[i] } else { d.norm[0] } + delta;
            io.perform(p, g.plain(p, n));
            if up {
                io.end(p);
            }
        }
    }

    /// The wheel over a gang knob. True if one took it.
    pub fn gang_wheel(gangs: &[Gang], x: f32, y: f32, dy: f32, io: &mut Io) -> bool {
        let Some(g) = gangs.iter().find(|g| matches!(g.kind, GKind::Knob(_)) && g.hit(x, y)) else { return false };
        let lead = g.params[0];
        let base = g.norm(lead, io.value(lead));
        for &p in &g.params {
            let n = if g.offset { g.norm(p, io.value(p)) } else { base } + dy * 0.02;
            io.set(p, g.plain(p, n));
        }
        true
    }
}

/// Renders the compressor pages with a fed read-out, both themes, STANDARD and TALL:
/// `build/ui-<key>-<std|tall>-<theme>.ppm`. Run with `--ignored`.
#[cfg(test)]
pub(super) mod dump {
    #![allow(dead_code)]
    use crate::ui::tests::{editor, feed};
    use crate::ui::theme::ThemeId;
    use openplugin::core::Editor;
    use crate::LAYOUT;

    pub fn dump(key: &str, view: &[f32], params: &[(&str, f32)]) {
        let slot = LAYOUT.slots.iter().position(|s| s.key == key).expect("slot");
        for (id, name) in ThemeId::ALL.map(|t| (t, t.name())) {
            for (tall, more) in [(false, false), (true, false), (false, true)] {
                let (mut e, shared) = editor(id);
                feed(&shared, 600);
                e.param_changed(LAYOUT.slots[slot].on, 1.0);
                for (pid, v) in params {
                    let i = (0..LAYOUT.slots[slot].count).map(|k| LAYOUT.slots[slot].base + k).find(|&i| crate::INFO.params[i].id.rsplit_once('.').is_some_and(|(_, t)| t == *pid)).expect(pid);
                    e.param_changed(i, *v);
                }
                e.open_page(slot);
                e.set_tall(tall);
                if more && e.ui.page.has_more() {
                    e.ui.more = true;
                    e.rebuild();
                }
                for _ in 0..40 {
                    shared.set_view(slot, view);
                    shared.raise_slot_gr(slot, 3.0);
                    e.tick(1.0 / 30.0);
                }
                let f = e.render(2.0);
                let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                for px in f.pixels {
                    out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
                }
                let mode = if more { "more" } else if tall { "tall" } else { "std" };
                std::fs::write(format!("{}/build/ui-{key}-{mode}-{name}.ppm", env!("CARGO_MANIFEST_DIR")), out).expect("write");
            }
        }
    }

    #[test]
    #[ignore]
    fn dump_bands() {
        let mut v = vec![0.0f32; 36];
        let gr = [1.5, 4.2, 6.8, 2.1, 0.3];
        for b in 0..5 {
            v[b] = gr[b];
            v[5 + b] = if b == 2 { 1.2 } else { 0.0 };
            v[10 + b] = [-12.0, -9.5, -11.0, -16.0, -24.0][b];
            v[15 + b] = v[10 + b] - gr[b] - 1.0;
            v[20 + b] = -gr[b];
            v[25 + 2 * b] = gr[b];
            v[26 + 2 * b] = gr[b] * 0.8;
        }
        dump("bands", &v, &[("bandLimiter", 1.0), ("highMidBypass", 1.0)]);
    }
}
