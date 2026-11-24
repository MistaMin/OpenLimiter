//! The Shaper page (`design/tone.md` section 5): the transfer curve with the live level
//! riding on it, four seconds of history, LIFT / PUNCH / CLIP meters, curve tiles and knobs.

use std::collections::VecDeque;

use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use shaper::{view, P_CURVE, P_DRIVE, P_MIX, P_OVERSAMPLE, P_THRESHOLD, P_TRANSIENT, P_UPWARD, P_UPWARD_THRESHOLD, UP_RANGE_DB};

use super::eq6::{fill_between, tip};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::ui::theme::Theme;
use crate::INFO;

/// The transfer plot's axes, dB.
const LO: f32 = -54.0;
const HI: f32 = 6.0;
/// Meter ranges, dB.
const LIFT_RANGE: f32 = 10.0;
const PUNCH_RANGE: f32 = 6.0;
const CLIP_RANGE: f32 = 12.0;
/// The dot's trail, seconds.
const TRAIL_S: f32 = 0.3;
/// History length, seconds.
const HIST_S: f32 = 4.0;
/// Histogram bins across the x axis, one per dB.
const BINS: usize = 60;

/// The curve tiles in display order, with their labels.
const TILES: [(usize, &str); 8] = [
    (shaper::CURVE_OFF, "OFF"),
    (shaper::CURVE_DENSE_LIGHT, "DENSE 1"),
    (shaper::CURVE_DENSE_MEDIUM, "DENSE 2"),
    (shaper::CURVE_DENSE_HEAVY, "DENSE 3"),
    (shaper::CURVE_QUAD_2DB, "KNEE 2"),
    (shaper::CURVE_QUAD_6DB, "KNEE 6"),
    (shaper::CURVE_QUAD_9DB, "KNEE 9"),
    (shaper::CURVE_CONTINUOUS, "SMOOTH"),
];

fn tile_rect(i: usize) -> R {
    (120.0 + 76.0 * (i % 4) as f32, if i < 4 { 640.0 } else { 672.0 }, 72.0, 28.0)
}

const TILE_AREA: R = (118.0, 638.0, 300.0, 64.0);

/// One editor tick of the read-out.
#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    /// Peak into the curve, after drive, dBFS.
    drive_peak: f32,
    out_peak: f32,
    in_peak: f32,
    lift: f32,
    punch: f32,
    clip: f32,
}

struct ShaperPage {
    hist: VecDeque<Sample>,
    trail: VecDeque<(f32, f32, f32)>,
    histogram: [f32; BINS],
    /// Held for the header and the max boxes: lift, punch, clip.
    held: [f32; 3],
    held_age: f32,
    max: [f32; 3],
    drag: Option<Handle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handle {
    Threshold,
    Lift,
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    Box::new(ShaperPage { hist: VecDeque::new(), trail: VecDeque::new(), histogram: [0.0; BINS], held: [0.0; 3], held_age: 0.0, max: [0.0; 3], drag: None })
}

/// Drive with nothing after it to hold the peaks: no curve, or part of the driven dry signal mixed back.
fn can_pass(curve: f32, drive: f32, mix: f32) -> bool {
    drive > 0.0 && (curve.round() as usize == shaper::CURVE_OFF || mix < 100.0)
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let v = |i: usize| values.get(i).copied().unwrap_or(0.0);
    (false, can_pass(v(P_CURVE), v(P_DRIVE), v(P_MIX)))
}

/// The settings the static curve needs.
#[derive(Clone, Copy, Debug)]
struct Static {
    curve: usize,
    threshold: f64,
    drive: f64,
    mix: f64,
    upward: f64,
    upward_thr: f64,
}

impl Static {
    fn read(cx: &Ctx) -> Self {
        Self {
            curve: cx.choice(P_CURVE),
            threshold: cx.v(P_THRESHOLD) as f64,
            drive: cx.v(P_DRIVE) as f64,
            mix: cx.v(P_MIX) as f64 / 100.0,
            upward: cx.v(P_UPWARD) as f64,
            upward_thr: cx.v(P_UPWARD_THRESHOLD) as f64,
        }
    }

    fn lift(&self, level: f64) -> f64 {
        if self.upward > 0.0 {
            shaper::upward_boost_db(self.upward, self.upward_thr, level)
        } else {
            0.0
        }
    }

    /// `(level without the curve, output)` in dB for a steady input `level` dB.
    fn out(&self, level: f64) -> (f64, f64) {
        let linear = level + self.lift(level) + self.drive;
        let a = 10f64.powf(linear / 20.0);
        let y = if self.curve == shaper::CURVE_OFF {
            a
        } else {
            let w = shaper::transfer(self.curve, a, 10f64.powf(self.threshold / 20.0));
            self.mix * w + (1.0 - self.mix) * a
        };
        (linear, 20.0 * y.abs().max(1e-9).log10())
    }
}

impl ShaperPage {
    fn transfer(cx: &Ctx) -> R {
        let (x, y, _, h) = cx.zones.picture;
        (x, y, h, h)
    }

    fn history(cx: &Ctx) -> R {
        let (x, y, _, h) = cx.zones.picture;
        let left = x + h + 16.0;
        (left, y, 928.0 - left, h)
    }

    fn meter(cx: &Ctx, i: usize) -> R {
        let (_, y, _, h) = cx.zones.picture;
        (944.0 + 48.0 * i as f32, y + 16.0, 36.0, h - 58.0)
    }

    fn max_box(cx: &Ctx, i: usize) -> R {
        let (_, y, _, h) = cx.zones.picture;
        (940.0 + 48.0 * i as f32, y + h - 26.0, 44.0, 18.0)
    }

    fn readout(cx: &Ctx) -> R {
        (420.0, cx.zones.header.1 + 6.0, 480.0, 28.0)
    }

    fn tx(r: R, db: f32) -> f32 {
        r.0 + (db - LO) / (HI - LO) * r.2
    }

    fn ty(r: R, db: f32) -> f32 {
        r.1 + (HI - db) / (HI - LO) * r.3
    }

    fn x_db(r: R, x: f32) -> f32 {
        LO + (x - r.0) / r.2 * (HI - LO)
    }

    fn y_db(r: R, y: f32) -> f32 {
        HI - (y - r.1) / r.3 * (HI - LO)
    }

    /// Where the threshold and lift handles sit, window points.
    fn handles(cx: &Ctx) -> (Point, Point) {
        let r = Self::transfer(cx);
        let st = Static::read(cx);
        let at = |level: f64, y_db: f64| Point::new(Self::tx(r, level as f32).clamp(r.0 + 5.0, r.0 + r.2 - 5.0), Self::ty(r, y_db as f32).clamp(r.1 + 5.0, r.1 + r.3 - 5.0));
        let lt = st.threshold - st.drive;
        let thr = at(lt, st.out(lt).1);
        let ll = st.upward_thr - UP_RANGE_DB;
        let lift = at(ll, ll + st.drive + st.upward);
        (thr, lift)
    }

    fn read(live: &Live) -> Option<Sample> {
        if live.view.len() < view::LEN {
            return None;
        }
        let v = live.view;
        Some(Sample { drive_peak: v[view::DRIVE_PEAK_DB], out_peak: v[view::OUT_PEAK_DB], in_peak: v[view::IN_PEAK_DB], lift: v[view::UPWARD_DB], punch: v[view::TRANSIENT_DB], clip: live.gr })
    }

    fn paint_transfer(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let r = Self::transfer(cx);
        let st = Static::read(cx);
        s.clip_round(kit::rect(r), 4.0);
        // Histogram of input peaks along the bottom.
        let top = self.histogram.iter().copied().fold(0.0f32, f32::max);
        if top > 1e-3 {
            let bw = r.2 / BINS as f32;
            for (i, &n) in self.histogram.iter().enumerate() {
                let h = (n / top) * 18.0;
                if h > 0.5 {
                    let c = t.in_fill.with_alpha(t.in_fill_alpha);
                    s.fill_rect(Rect::new(r.0 + i as f32 * bw + 0.5, r.1 + r.3 - h, bw - 1.0, h), c, c, true);
                }
            }
        }
        let n = 121;
        let levels: Vec<f64> = (0..n).map(|i| LO as f64 + (HI - LO) as f64 * i as f64 / (n - 1) as f64).collect();
        let pt = |l: f64, y: f64| Point::new(Self::tx(r, l as f32), Self::ty(r, (y as f32).clamp(LO - 6.0, HI + 6.0)));
        let outs: Vec<(f64, f64)> = levels.iter().map(|&l| st.out(l)).collect();
        let curve: Vec<Point> = levels.iter().zip(&outs).map(|(&l, o)| pt(l, o.1)).collect();
        let linear: Vec<Point> = levels.iter().zip(&outs).map(|(&l, o)| pt(l, o.0)).collect();
        let plain: Vec<Point> = levels.iter().map(|&l| pt(l, l + st.drive)).collect();
        // Lift: between the driven level and the lifted level.
        if st.upward > 0.0 {
            fill_between(s, &linear, &plain, t.warm.with_alpha(0.2));
        }
        // Clip: where the curve takes gain away.
        let below: Vec<Point> = curve.iter().zip(&linear).map(|(c, l)| Point::new(c.x, c.y.max(l.y))).collect();
        if below.iter().zip(&linear).any(|(b, l)| b.y - l.y > 0.5) {
            fill_between(s, &linear, &below, t.accent.with_alpha(0.2));
        }
        // Unity, dashed.
        kit::dashed(s, &[pt(LO as f64, LO as f64), pt(HI as f64, HI as f64)], 4.0, 4.0, t.dim, 1.0);
        kit::curve(s, curve, t.accent, 2.0);
        // Trail and dot.
        let n = self.trail.len();
        for (i, &(x, y, _)) in self.trail.iter().enumerate() {
            let a = 0.15 + 0.5 * i as f32 / n.max(1) as f32;
            s.fill_ellipse_solid(Scene::ellipse_at(Point::new(Self::tx(r, x), Self::ty(r, y)), 2.0), t.text.with_alpha(a));
        }
        if let Some(&(x, y, _)) = self.trail.back() {
            let at = Point::new(Self::tx(r, x), Self::ty(r, y));
            s.fill_ellipse_solid(Scene::ellipse_at(at, 4.0), t.text);
            s.stroke_ellipse(Scene::ellipse_at(at, 4.0), t.plot_bg, 1.0);
        }
        s.pop_clip();
        // Handles.
        let (thr, lift) = Self::handles(cx);
        let d = 5.5;
        let diamond = vec![Point::new(thr.x, thr.y - d), Point::new(thr.x + d, thr.y), Point::new(thr.x, thr.y + d), Point::new(thr.x - d, thr.y)];
        let fill = if st.curve == shaper::CURVE_OFF { t.dim } else { t.accent };
        s.fill_poly(diamond.clone(), fill, fill);
        s.stroke_poly(diamond, t.plot_bg, 1.0, true);
        let disc = Scene::ellipse_at(lift, 5.0);
        s.fill_ellipse_solid(disc, if st.upward > 0.0 { t.warm } else { t.warm.with_alpha(0.45) });
        s.stroke_ellipse(disc, t.plot_bg, 1.0);
    }

    fn paint_history(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let r = Self::history(cx);
        let n = self.hist.len();
        if n < 2 {
            return;
        }
        let span = (live.hist_rate * HIST_S).max(2.0);
        let x = |i: usize| r.0 + r.2 * (1.0 - (n - 1 - i) as f32 / span).max(0.0);
        let ly = |db: f32| Self::ty(r, db.clamp(LO, HI));
        s.clip_round(kit::rect(r), 4.0);
        let input: Vec<Point> = self.hist.iter().enumerate().map(|(i, h)| Point::new(x(i), ly(h.drive_peak))).collect();
        kit::area(s, &input, r.1 + r.3, t.in_fill.with_alpha(t.in_fill_alpha));
        let output: Vec<Point> = self.hist.iter().enumerate().map(|(i, h)| Point::new(x(i), ly(h.out_peak))).collect();
        kit::curve(s, output, t.out_line, 1.3);
        // Gains: lift and punch rise from the bottom, clip hangs from the top (12 dB over half the height).
        let g = |db: f32| (db / CLIP_RANGE).clamp(0.0, 1.0) * r.3 * 0.5;
        for (pick, colour, alpha) in [(0usize, t.warm, 1.0f32), (1, t.warm, 0.6)] {
            let pts: Vec<Point> = self.hist.iter().enumerate().map(|(i, h)| Point::new(x(i), r.1 + r.3 - g(if pick == 0 { h.lift } else { h.punch }))).collect();
            if pts.iter().any(|p| p.y < r.1 + r.3 - 0.5) {
                kit::curve(s, pts, colour.with_alpha(alpha), 1.4);
            }
        }
        let clip: Vec<Point> = self.hist.iter().enumerate().map(|(i, h)| Point::new(x(i), r.1 + g(h.clip))).collect();
        if clip.iter().any(|p| p.y > r.1 + 0.5) {
            kit::area(s, &clip, r.1, t.accent.with_alpha(0.22));
            kit::curve(s, clip, t.accent, 1.4);
        }
        s.pop_clip();
    }

    fn paint_meter(s: &mut Scene, t: &Theme, r: R, value: f32, range: f32, colour: C, down: bool) {
        let (x, y, _, h) = r;
        s.fill_round(Rect::new(x, y, 14.0, h), 3.0, t.seg_off, t.seg_off, true);
        let d = (value / range).clamp(0.0, 1.0) * h;
        if d > 0.5 {
            let bar = if down { Rect::new(x, y, 14.0, d) } else { Rect::new(x, y + h - d, 14.0, d) };
            s.fill_round(bar, 3.0, colour, colour, true);
        }
    }

    fn meter_scale(s: &mut Scene, t: &Theme, r: R, range: f32, down: bool) {
        let (x, y, _, h) = r;
        let step = if range > 8.0 { 2.0 } else { 1.0 };
        let mut d = 0.0;
        while d <= range + 1e-3 {
            let yy = if down { y + d / range * h } else { y + h - d / range * h };
            s.line(Point::new(x + 16.0, yy), Point::new(x + 19.0, yy), t.grid_strong, 1.0);
            if (d as i32) % (2 * step as i32) == 0 {
                s.text(Point::new(x + 21.0, yy), format!("{d:.0}"), t.dim, 8.0, false);
            }
            d += step;
        }
    }

    fn paint_tile(s: &mut Scene, t: &Theme, r: R, kind: usize, label: &str, on: bool) {
        let fill = if on { t.accent } else { t.panel };
        s.fill_round(kit::rect(r), 6.0, fill, fill, true);
        s.stroke_round(kit::rect(r), 6.0, if on { t.accent } else { t.border }, 1.0);
        let ink = if on { t.on_accent } else { t.text };
        // A glyph of the curve, 18 x 18, from the public transfer function.
        let g = (r.0 + 6.0, r.1 + 5.0, 18.0, 18.0);
        let pts: Vec<Point> = (0..=18)
            .map(|i| {
                let u = 2.0 * i as f64 / 18.0;
                let y = if kind == shaper::CURVE_OFF { u } else { shaper::transfer(kind, u, 1.0) };
                Point::new(g.0 + i as f32, g.1 + g.3 - (y as f32 / 2.0).clamp(0.0, 1.0) * g.3)
            })
            .collect();
        kit::curve(s, pts, ink, 1.3);
        s.text(Point::new(r.0 + 28.0, r.1 + r.3 * 0.5), label, ink, 9.0, false);
    }
}

impl Page for ShaperPage {
    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        vec![
            // The tiles are painted and clicked by the page; this hit-less cell gives the host,
            // automation and accessibility a control for the curve choice.
            Cell::grip(cx.p(P_CURVE), (120.0, 640.0, 0.0, 0.0)).tip("Curve"),
            Cell::pill(cx.p(P_OVERSAMPLE), "4X", (120.0, 712.0, 56.0, 22.0)).tip("Run the curve at four times the rate (adds 32 samples)"),
            Cell::knob(Size::Big, cx.p(P_DRIVE), "DRIVE", 440.0, 636.0).tip("Gain into the curve"),
            Cell::knob(Size::Big, cx.p(P_THRESHOLD), "THRESHOLD", 568.0, 636.0).tip("Where the curve bends"),
            Cell::knob(Size::Big, cx.p(P_MIX), "MIX", 696.0, 636.0).tip("Curve against the driven dry signal"),
            Cell::knob(Size::Small, cx.p(P_UPWARD), "AMOUNT", 832.0, 636.0).tip("Lift: most boost for quiet passages"),
            Cell::knob(Size::Small, cx.p(P_UPWARD_THRESHOLD), "BELOW", 912.0, 636.0).tip("Lift: levels below this are raised"),
            Cell::knob(Size::Small, cx.p(P_TRANSIENT), "AMOUNT", 1000.0, 636.0).tip("Punch: boost on attacks"),
        ]
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let r = Self::transfer(cx);
        kit::plot(s, t, r);
        for db in [-48.0f32, -36.0, -24.0, -12.0, 0.0] {
            let (x, y) = (Self::tx(r, db), Self::ty(r, db));
            let c = if db == 0.0 { t.grid_strong } else { t.grid };
            s.line(Point::new(x, r.1), Point::new(x, r.1 + r.3), c, 1.0);
            s.line(Point::new(r.0, y), Point::new(r.0 + r.2, y), c, 1.0);
        }
        s.text(Point::new(r.0 + 6.0, r.1 + 9.0), "OUT +6", t.dim, 8.0, false);
        // The bottom-left corner is always on the unity diagonal: the input range goes bottom right.
        kit::text_right(s, r.0 + r.2 - 6.0, r.1 + r.3 - 28.0, "IN -54 TO +6", t.dim, 8.0);
        let h = Self::history(cx);
        kit::plot(s, t, h);
        for db in [-48.0f32, -36.0, -24.0, -12.0, 0.0] {
            let y = Self::ty(h, db);
            s.line(Point::new(h.0, y), Point::new(h.0 + h.2, y), if db == 0.0 { t.grid_strong } else { t.grid }, 1.0);
            s.text(Point::new(h.0 + 6.0, y - 7.0), format!("{db:.0}"), t.dim, 8.0, false);
        }
        kit::text_right(s, h.0 + h.2 - 8.0, h.1 + 12.0, "LAST 4 s", t.dim, 9.0);
        for (i, (label, range, down)) in [("LIFT", LIFT_RANGE, false), ("PUNCH", PUNCH_RANGE, false), ("CLIP", CLIP_RANGE, true)].into_iter().enumerate() {
            let m = Self::meter(cx, i);
            s.text(Point::new(m.0 + 7.0, m.1 - 9.0), label, t.dim, 8.5, true);
            Self::meter_scale(s, t, m, range, down);
        }
        // Group captions with a bracket line.
        for (r, label) in [((832.0, 728.0, 160.0, 14.0), "LIFT"), ((1000.0, 728.0, 80.0, 14.0), "PUNCH")] {
            let (x, y, w, _) = r;
            let mid = x + w * 0.5;
            let half = kit::rect(r).h * 0.5;
            let tw = openplugin::dri::goodlookindri_shell::text::width(label, 9.0) * 0.5 + 6.0;
            s.line(Point::new(x + 8.0, y + half), Point::new(mid - tw, y + half), t.border, 1.0);
            s.line(Point::new(mid + tw, y + half), Point::new(x + w - 8.0, y + half), t.border, 1.0);
            s.line(Point::new(x + 8.0, y + half - 4.0), Point::new(x + 8.0, y + half), t.border, 1.0);
            s.line(Point::new(x + w - 8.0, y + half - 4.0), Point::new(x + w - 8.0, y + half), t.border, 1.0);
            s.text(Point::new(mid, y + half), label, t.dim, 9.0, true);
        }
        kit::caption(s, t, 120.0, 630.0, "CURVE");
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        self.paint_transfer(s, cx);
        self.paint_history(s, cx, live);
        // Threshold, dashed, on the history.
        let h = Self::history(cx);
        let thr = cx.v(P_THRESHOLD);
        if thr > LO && thr < HI {
            kit::dashed_h(s, h.0, h.0 + h.2, Self::ty(h, thr), 5.0, 4.0, t.accent.with_alpha(0.7), 1.0);
        }
        let now = self.hist.back().copied().unwrap_or_default();
        for (i, (v, range, colour, down)) in [(now.lift, LIFT_RANGE, t.warm, false), (now.punch, PUNCH_RANGE, t.warm, false), (now.clip, CLIP_RANGE, t.accent, true)].into_iter().enumerate() {
            Self::paint_meter(s, t, Self::meter(cx, i), v, range, colour, down);
            let b = Self::max_box(cx, i);
            kit::inset(s, t, b, 4.0);
            let text = if i == 2 { format!("{:.1}", -self.max[i] + 0.0) } else { format!("{:.1}", self.max[i]) };
            s.text(kit::rect(b).center(), text, t.text, 9.5, true);
        }
        // Header read-outs.
        let r = Self::readout(cx);
        let y = r.1 + 14.0;
        // Live amounts in whole dB, like the other moving readouts.
        let whole = |v: f32| v.round() + 0.0;
        let words = [("LIFT", format!("{:+.0}", whole(self.held[0])), t.warm), ("PUNCH", format!("{:+.0}", whole(self.held[1])), t.warm), ("CLIP", format!("{:.0} dB", whole(-self.held[2])), t.accent)];
        let mut x = r.0;
        for (label, value, colour) in words {
            s.text(Point::new(x, y), label, t.dim, 10.0, false);
            let lx = x + openplugin::dri::goodlookindri_shell::text::width(label, 10.0) + 6.0;
            s.text(Point::new(lx, y), value.clone(), colour, 12.0, false);
            x = lx + openplugin::dri::goodlookindri_shell::text::width(&value, 12.0) + 22.0;
        }
        // Curve tiles.
        let chosen = cx.choice(P_CURVE);
        for (i, &(kind, label)) in TILES.iter().enumerate() {
            Self::paint_tile(s, t, tile_rect(i), kind, label, kind == chosen);
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let p = cx.zones.picture;
        vec![p, Self::readout(cx), TILE_AREA]
    }

    fn view_merge(&self) -> &'static [Merge] {
        &[Merge::Max; view::LEN]
    }

    fn warning(&self, cx: &Ctx) -> bool {
        can_pass(cx.v(P_CURVE), cx.v(P_DRIVE), cx.v(P_MIX))
    }

    fn tick(&mut self, _cx: &Ctx, live: &Live) -> bool {
        let Some(now) = Self::read(live) else { return false };
        let keep = (live.hist_rate * HIST_S).ceil().max(8.0) as usize;
        while self.hist.len() >= keep {
            self.hist.pop_front();
        }
        self.hist.push_back(now);
        let dt = live.dt.max(0.0);
        for p in self.trail.iter_mut() {
            p.2 += dt;
        }
        while self.trail.front().is_some_and(|p| p.2 > TRAIL_S) {
            self.trail.pop_front();
        }
        if now.in_peak > LO - 6.0 {
            self.trail.push_back((now.in_peak.clamp(LO, HI), now.out_peak.clamp(LO, HI), 0.0));
            let bin = (((now.in_peak - LO) / (HI - LO) * BINS as f32).floor().max(0.0) as usize).min(BINS - 1);
            for v in self.histogram.iter_mut() {
                *v *= 0.995;
            }
            self.histogram[bin] += 1.0;
        }
        let vals = [now.lift, now.punch, now.clip];
        self.held_age += dt;
        let fresh = vals.iter().zip(&self.held).any(|(v, h)| v > h);
        if fresh || self.held_age > 0.5 {
            self.held = vals;
            self.held_age = 0.0;
        }
        for (m, v) in self.max.iter_mut().zip(vals) {
            *m = m.max(v);
        }
        true
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let r = Self::transfer(cx);
        match ev {
            Pointer::Down { x, y, clicks } => {
                for i in 0..3 {
                    if kit::inside(Self::max_box(cx, i), x, y) {
                        self.max[i] = 0.0;
                        return Response::TAKEN;
                    }
                }
                for (i, &(kind, _)) in TILES.iter().enumerate() {
                    if kit::inside(tile_rect(i), x, y) {
                        io.set(cx.p(P_CURVE), kind as f32);
                        return Response::TAKEN;
                    }
                }
                let (thr, lift) = Self::handles(cx);
                let near = |p: Point| (p.x - x).hypot(p.y - y) <= 9.0;
                let which = if near(lift) {
                    Some(Handle::Lift)
                } else if near(thr) {
                    Some(Handle::Threshold)
                } else {
                    None
                };
                let Some(h) = which else { return Response::NONE };
                let params: &[usize] = match h {
                    Handle::Threshold => &[P_THRESHOLD],
                    Handle::Lift => &[P_UPWARD_THRESHOLD, P_UPWARD],
                };
                if clicks >= 2 {
                    for &p in params {
                        io.set(cx.p(p), INFO.params[cx.p(p)].default);
                    }
                    return Response::TAKEN;
                }
                for &p in params {
                    io.begin(cx.p(p));
                }
                self.drag = Some(h);
                Response::TAKEN
            }
            Pointer::Move { x, y } => {
                let Some(h) = self.drag else { return Response::NONE };
                let level = Self::x_db(r, x);
                let drive = cx.v(P_DRIVE);
                match h {
                    Handle::Threshold => io.perform(cx.p(P_THRESHOLD), level + drive),
                    Handle::Lift => {
                        io.perform(cx.p(P_UPWARD_THRESHOLD), level + UP_RANGE_DB as f32);
                        io.perform(cx.p(P_UPWARD), Self::y_db(r, y) - (level + drive));
                    }
                }
                Response::TAKEN
            }
            Pointer::Up { .. } => {
                let Some(h) = self.drag.take() else { return Response::NONE };
                let params: &[usize] = match h {
                    Handle::Threshold => &[P_THRESHOLD],
                    Handle::Lift => &[P_UPWARD_THRESHOLD, P_UPWARD],
                };
                for &p in params {
                    io.end(cx.p(p));
                }
                Response::TAKEN
            }
            Pointer::Wheel { .. } => Response::NONE,
        }
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let (thr, lift) = Self::handles(cx);
        let mut v = vec![
            tip((thr.x - 6.0, thr.y - 6.0, 12.0, 12.0), "Threshold: drag sideways, double-click resets"),
            tip((lift.x - 6.0, lift.y - 6.0, 12.0, 12.0), "Lift: sideways sets where it starts, up and down how much; double-click resets"),
            tip(Self::history(cx), "Last 4 s: level into the curve (fill), output (line), lift and punch from the bottom, clip from the top"),
        ];
        for (i, name) in ["Largest lift", "Largest punch", "Deepest clip"].into_iter().enumerate() {
            v.push(tip(Self::max_box(cx, i), format!("{name}, dB. Click to reset")));
        }
        for (i, &(_, label)) in TILES.iter().enumerate() {
            v.push(tip(tile_rect(i), format!("Curve: {}", label.to_lowercase())));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_show_every_curve_once() {
        let mut kinds: Vec<usize> = TILES.iter().map(|t| t.0).collect();
        kinds.sort_unstable();
        assert_eq!(kinds, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn the_static_curve_bends_at_the_threshold() {
        let st = Static { curve: shaper::CURVE_QUAD_6DB, threshold: -6.0, drive: 0.0, mix: 1.0, upward: 0.0, upward_thr: -16.0 };
        assert!((st.out(-20.0).1 + 20.0).abs() < 1e-6, "identity below");
        assert!(st.out(0.0).1 < -0.1, "bent above");
        let lifted = Static { upward: 6.0, ..st };
        assert!((lifted.out(-40.0).0 + 34.0).abs() < 1e-6, "full lift far below");
    }
}
