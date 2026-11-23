//! Decompressor (`crates/dyn-decompress`): gives back the peaks a compressor took. Above the
//! threshold the level is expanded upward by at most Range, under a ceiling. The picture is
//! the transfer curve, two lift meters and the ceiling's reduction, a 10 s trace and three
//! tiles. See `design/new-modules.md` sections 0 and 2.
//!
//! Colour roles (section 0): `accent` is the curve and the gain taken away (the ceiling),
//! `text` is lift (gain added), `warm` the detector and the riding threshold, `over` the
//! NO CEILING warning.
//!
//! The ratio is stored in:out (1.0 = OFF, 0.25 = 1:4), so its parameter runs "backwards".
//! The page draws and drives its RATIO knob itself so that turning right (or dragging up)
//! means more decompression; a dimmed grip under the knob's value box keeps the parameter in
//! the control set (host menu on right-click, accessibility).

use std::collections::VecDeque;

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use openplugin::dri::goodlookindri_shell::text;

use dyn_decompress::{index as ix, static_gain_db, view as vw};

use super::bands::common::{curve_frame, max_box, sub_caption, time_text, veil, warn_chip, Map};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::ui::theme::Theme;
use crate::INFO;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != ix::COUNT {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(Decomp::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let v = |i: usize| values.get(i).copied().unwrap_or(0.0);
    (v(ix::KEY_SOURCE) >= 0.5, values.len() >= ix::COUNT && unguarded(v(ix::CEILING_ON) >= 0.5, v(ix::RATIO), v(ix::RANGE), v(ix::MIX)))
}

/// The ceiling is off while the curve can lift: lifted peaks are limited by nothing.
fn unguarded(ceiling_on: bool, ratio: f32, range: f32, mix: f32) -> bool {
    !ceiling_on && ratio < 0.9995 && range > 0.0 && mix > 0.0
}

// =====================================================================================
// Geometry (STANDARD; TALL stretches the meters and the trace upward, section 0)
// =====================================================================================

const CURVE: R = (120.0, 416.0, 200.0, 200.0);
const METER_X: [f32; 3] = [328.0, 376.0, 424.0];
const TRACE_X: f32 = 476.0;
const TRACE_W: f32 = 476.0;
const TX0: f32 = 508.0;
const TX1: f32 = 920.0;
const TILES: [R; 3] = [(960.0, 416.0, 120.0, 60.0), (960.0, 486.0, 120.0, 60.0), (960.0, 556.0, 120.0, 60.0)];
const BIG_Y: f32 = 636.0;
const RATIO_X: f32 = 240.0;

/// Seconds the trace spans.
pub(super) const SPAN_S: f32 = 10.0;

/// The meter bar under the picture top (`top` is 416, or 244 in TALL).
pub(super) fn meter(x: f32, top: f32) -> R {
    (x, top, 36.0, 592.0 - top)
}

pub(super) fn max_rect(x: f32) -> R {
    (x - 4.0, 596.0, 44.0, 18.0)
}

/// The trace's lanes: plot y from `y0` to 592, scaled from the STANDARD 168 pt.
#[derive(Clone, Copy)]
struct Lanes {
    y0: f32,
    k: f32,
}

impl Lanes {
    fn new(top: f32) -> Self {
        let y0 = top + 8.0;
        Self { y0, k: (592.0 - y0) / 168.0 }
    }

    /// The gain lane's zero line.
    fn zero(&self) -> f32 {
        self.y0 + 52.0 * self.k
    }

    fn lift_y(&self, db: f32, scale: f32) -> f32 {
        (self.zero() - db.max(0.0) * 48.0 * self.k / scale).max(self.y0)
    }

    fn ceil_y(&self, db: f32) -> f32 {
        self.zero() + db.clamp(0.0, 6.0) * 4.0 * self.k
    }

    fn level_top(&self) -> f32 {
        self.y0 + 84.0 * self.k
    }

    fn level_y(&self, l: f32) -> f32 {
        self.level_top() - 1.4 * self.k * l.clamp(-60.0, 0.0)
    }

    fn level_at(&self, y: f32) -> f32 {
        (self.level_top() - y) / (1.4 * self.k)
    }
}

fn chip(top: f32) -> R {
    (820.0, top + 12.0, 100.0, 22.0)
}

fn ratio_rect() -> R {
    (RATIO_X, BIG_Y, 120.0, 124.0)
}

// =====================================================================================
// Pieces the De-expander page shares
// =====================================================================================

/// The lift fill: `text` at 12 %, 16 % on the dark plots.
pub(super) fn lift_fill(t: &Theme) -> C {
    let p = t.plot_bg.0;
    let luma = 0.3 * ((p >> 16) & 255) as f32 + 0.59 * ((p >> 8) & 255) as f32 + 0.11 * (p & 255) as f32;
    t.text.with_alpha(if luma < 110.0 { 0.16 } else { 0.12 })
}

/// The smallest of `steps` that holds `range` dB.
pub(super) fn lift_scale_for(range: f32, steps: &[f32]) -> f32 {
    steps.iter().copied().find(|s| range <= *s + 1e-3).unwrap_or(*steps.last().unwrap_or(&12.0))
}

/// The scale beside a lift meter: 0 dB at the bottom, ticks every 3 dB (6 above 12 dB),
/// labels every 6 (12 above 24 dB). Static.
pub(super) fn lift_scale(s: &mut Scene, t: &Theme, r: R, scale: f32) {
    let (x, y, _, h) = r;
    let (tick, every) = if scale > 24.0 { (6.0, 12.0) } else { (3.0, 6.0) };
    let mut d = 0.0;
    while d <= scale + 1e-3 {
        let yy = y + h - d / scale * h;
        s.line(Point::new(x + 16.0, yy), Point::new(x + 19.0, yy), t.grid_strong, 1.0);
        if (d as i32) % (every as i32) == 0 && d > 0.0 {
            s.text(Point::new(x + 20.0, yy), format!("+{d:.0}"), t.dim, 7.5, false);
        }
        d += tick;
    }
}

/// A lift meter: fills upward from 0 dB at the bottom, `text` at 85 %, with a hold line.
pub(super) fn lift_meter(s: &mut Scene, t: &Theme, r: R, lift: f32, hold: f32, scale: f32) {
    let (x, y, _, h) = r;
    s.fill_round(Rect::new(x, y, 14.0, h), 3.0, t.seg_off, t.seg_off, true);
    let up = (lift / scale).clamp(0.0, 1.0) * h;
    if up > 1.5 {
        s.fill_round(Rect::new(x, y + h - up, 14.0, up), 3.0, t.text.with_alpha(0.85), t.text.with_alpha(0.85), true);
    }
    let hy = (hold / scale).clamp(0.0, 1.0) * h;
    if hy > 1.0 {
        s.line(Point::new(x, y + h - hy), Point::new(x + 14.0, y + h - hy), t.text, 1.5);
    }
    if lift > scale + 0.05 {
        s.text(Point::new(x + 7.0, y + 6.0), "▲", t.panel, 7.0, true);
    }
}

/// The peak box under a lift meter: "+4.2".
pub(super) fn lift_box(s: &mut Scene, t: &Theme, r: R, db: f32) {
    kit::inset(s, t, r, 4.0);
    let label = if db < 0.05 { "0.0".to_string() } else { format!("+{db:.1}") };
    s.text(kit::rect(r).center(), label, t.text, 9.5, true);
}

/// A tile's frame and label. Static.
pub(super) fn tile_frame(s: &mut Scene, t: &Theme, r: R, label: &str) {
    kit::inset(s, t, r, 6.0);
    s.text(Point::new(r.0 + 8.0, r.1 + 12.0), label, t.dim, 8.5, false);
}

/// A tile's number, centred, fitted to its width.
pub(super) fn tile_text(s: &mut Scene, r: R, label: &str, c: C, size: f32, dy: f32) {
    let size = if text::width(label, size) > r.2 - 12.0 { size * (r.2 - 12.0) / text::width(label, size) } else { size };
    s.text(Point::new(r.0 + r.2 * 0.5, r.1 + dy), label, c, size, true);
}

/// The trace's vertical grid and time labels. Static.
pub(super) fn time_axis(s: &mut Scene, t: &Theme, x0: f32, x1: f32, y0: f32, y1: f32) {
    for k in 0..=5 {
        let x = x0 + (x1 - x0) * k as f32 / 5.0;
        s.line(Point::new(x, y0), Point::new(x, y1), t.grid, 1.0);
    }
    s.text(Point::new(x0, 604.0), "-10 s", t.dim, 8.5, false);
    kit::text_right(s, x1, 604.0, "now", t.dim, 8.5);
}

/// A dashed level line across the trace with its grip tab at the right end.
pub(super) fn trace_line(s: &mut Scene, x0: f32, x1: f32, y: f32, c: C) {
    kit::dashed_h(s, x0, x1, y, 5.0, 3.0, c, 1.2);
    s.fill_round(Rect::new(x1 - 18.0, y - 5.0, 18.0, 10.0), 3.0, c, c, true);
}

/// A handle on a curve: a hollow ring.
pub(super) fn handle(s: &mut Scene, t: &Theme, p: Point, c: C) {
    s.fill_ellipse_solid(Scene::ellipse_at(p, 4.5), t.panel);
    s.stroke_ellipse(Scene::ellipse_at(p, 4.5), c, 1.5);
}

/// The operating dot (8 pt), outlined `warm` while the detector runs from the key.
pub(super) fn dot(s: &mut Scene, t: &Theme, p: Point, keyed: bool) {
    s.fill_ellipse_solid(Scene::ellipse_at(p, 4.0), t.accent);
    s.stroke_ellipse(Scene::ellipse_at(p, 4.0), if keyed { t.warm } else { t.panel }, if keyed { 2.0 } else { 1.0 });
}

/// Peak holds of the meters: a 1.5 s hold line and a max until clicked.
#[derive(Clone, Copy, Default)]
pub(super) struct Hold {
    pub hold: f32,
    pub age: f32,
    pub max: f32,
}

impl Hold {
    pub fn push(&mut self, v: f32, dt: f32) {
        let v = v.max(0.0);
        if v >= self.hold {
            self.hold = v;
            self.age = 0.0;
        } else {
            self.age += dt;
            if self.age > 1.5 {
                self.hold = v;
            }
        }
        self.max = self.max.max(v);
    }
}

/// Mix-scaled lift in dB: what the audio gets from a curve gain `g` dB at `mix` (0..1).
pub(super) fn mixed(g: f32, mix: f32) -> f32 {
    20.0 * (1.0 + mix * (10f32.powf(g / 20.0) - 1.0)).max(1e-6).log10()
}

// ---- drags the page runs itself ----

#[derive(Clone, Copy, Debug)]
pub(super) enum Along {
    X,
    Y,
    /// Perpendicular to the unity line (up and to the left).
    Perp,
}

#[derive(Clone, Debug)]
pub(super) enum Drag {
    /// A plain value moved by pointer travel: `per` points per unit.
    Plain { param: usize, last: (f32, f32), v: f32, along: Along, per: f32 },
    /// Along the normalized travel, 200 pt for all of it; `inv` turns it round.
    Norm { param: usize, last: f32, n: f32, inv: bool },
}

impl Drag {
    pub fn plain(cx: &Ctx, local: usize, x: f32, y: f32, along: Along, per: f32) -> Self {
        Drag::Plain { param: cx.p(local), last: (x, y), v: cx.v(local), along, per }
    }

    pub fn norm(cx: &Ctx, local: usize, y: f32, inv: bool) -> Self {
        let p = cx.p(local);
        let n = INFO.params[p].to_normalized(cx.v(local));
        Drag::Norm { param: p, last: y, n: if inv { 1.0 - n } else { n }, inv }
    }

    fn param(&self) -> usize {
        match self {
            Drag::Plain { param, .. } | Drag::Norm { param, .. } => *param,
        }
    }

    /// Double-click resets, a press starts the drag.
    pub fn start(self, slot: &mut Option<Drag>, io: &mut Io, clicks: u32) -> Response {
        let p = self.param();
        if clicks >= 2 {
            io.set(p, INFO.params[p].default);
            *slot = None;
        } else {
            io.begin(p);
            *slot = Some(self);
        }
        Response::TAKEN
    }

    /// A move or the release of a drag in `slot`.
    pub fn follow(slot: &mut Option<Drag>, ev: Pointer, io: &mut Io) -> Response {
        let Some(drag) = slot.as_mut() else { return Response::NONE };
        let (x, y, up) = match ev {
            Pointer::Move { x, y } => (x, y, false),
            Pointer::Up { x, y } => (x, y, true),
            _ => return Response::TAKEN,
        };
        let k = if io.modifiers().shift { 0.1 } else { 1.0 };
        match drag {
            Drag::Plain { param, last, v, along, per } => {
                let d = match along {
                    Along::X => x - last.0,
                    Along::Y => last.1 - y,
                    Along::Perp => (last.1 - y) - (x - last.0),
                };
                *v += d / *per * k;
                *v = INFO.params[*param].clamp(*v);
                *last = (x, y);
                io.perform(*param, *v);
            }
            Drag::Norm { param, last, n, inv } => {
                *n = (*n + (*last - y) / 200.0 * k).clamp(0.0, 1.0);
                *last = y;
                io.perform(*param, INFO.params[*param].from_normalized(if *inv { 1.0 - *n } else { *n }));
            }
        }
        if up {
            io.end(drag.param());
            *slot = None;
        }
        Response::TAKEN
    }
}

// ---- the RATIO knob, drawn and driven by the page ----

/// A big knob over `param` whose ring runs from the parameter's maximum (OFF) at the left
/// to its minimum at the right. Looks exactly like the shell's knob cells.
pub(super) fn inverted_knob(s: &mut Scene, t: &Theme, rect: R, param: usize, value: f32, label: &str, shown: &str) {
    let cell = Cell::knob(Size::Big, param, label, rect.0, rect.1);
    s.fill_rect(kit::rect(rect), t.panel, t.panel, true);
    kit::paint_static(s, t, &cell);
    let (cx, cy, r) = kit::knob_circle(&cell, Size::Big);
    let n = 1.0 - INFO.params[param].to_normalized(value);
    if n > 0.004 {
        s.stroke_arc(Point::new(cx, cy), r + 6.0, kit::knob_angle(0.0), kit::knob_angle(n), t.accent, 4.0);
    }
    let a = kit::knob_angle(n);
    s.line(Point::new(cx + a.cos() * r * 0.42, cy + a.sin() * r * 0.42), Point::new(cx + a.cos() * r * 0.86, cy + a.sin() * r * 0.86), t.text, 3.0);
    let b = kit::knob_value_box(&cell, Size::Big);
    s.text(Point::new(b.0 + b.2 * 0.5, b.1 + b.3 * 0.5), text::ellipsize(shown, 12.0, b.2 - 4.0), t.text, 12.0, true);
}

/// The dimmed grip that keeps an inverted knob's parameter among the controls: it sits on
/// the value box, takes no left click (the page has those) and opens the host menu.
pub(super) fn knob_grip(param: usize, rect: R, tip: &str) -> Cell {
    let cell = Cell::knob(Size::Big, param, "", rect.0, rect.1);
    Cell::grip(param, kit::knob_value_box(&cell, Size::Big)).dim(true).tip(tip)
}

/// Pointer on an inverted knob: drag up (or wheel up) moves toward the parameter's minimum.
pub(super) fn knob_pointer(slot: &mut Option<Drag>, ev: Pointer, cx: &Ctx, io: &mut Io, rect: R, local: usize) -> Option<Response> {
    let face = (rect.0, rect.1 + 12.0, rect.2, rect.3 - 12.0);
    match ev {
        Pointer::Down { x, y, clicks } if kit::inside(face, x, y) => Some(Drag::norm(cx, local, y, true).start(slot, io, clicks)),
        Pointer::Wheel { x, y, dy } if kit::inside(rect, x, y) => {
            let p = cx.p(local);
            let info = &INFO.params[p];
            let step = if dy > 0.0 { 0.02 } else { -0.02 } * if io.modifiers().shift { 0.1 } else { 1.0 };
            let n = (1.0 - info.to_normalized(cx.v(local)) + step).clamp(0.0, 1.0);
            io.set(p, info.from_normalized(1.0 - n));
            Some(Response::TAKEN)
        }
        _ => None,
    }
}

// =====================================================================================
// Formats
// =====================================================================================

fn ratio_text(v: f32) -> String {
    if v >= 0.9995 {
        "OFF".into()
    } else {
        format!("1:{:.1}", 1.0 / v.max(1e-3))
    }
}

pub(super) fn hp_text(v: f32) -> String {
    if v <= 10.05 {
        "OFF".into()
    } else {
        super::bands::common::hz_text(v)
    }
}

pub(super) fn lp_text(v: f32) -> String {
    if v >= 21_990.0 {
        "OFF".into()
    } else {
        super::bands::common::hz_text(v)
    }
}

fn db_text(v: f32) -> String {
    format!("{v:.1} dB")
}

fn target_text(v: f32) -> String {
    format!("to {v:.0} dB")
}

// =====================================================================================
// The page
// =====================================================================================

#[derive(Clone, Copy)]
struct Sample {
    age: f32,
    lift: f32,
    ceil: f32,
    env: f32,
    ride: f32,
}

#[derive(Default)]
struct Decomp {
    drag: Option<Drag>,
    trace: VecDeque<Sample>,
    holds: [Hold; 3],
    /// LIFT, CREST, MATCH RANGE, refreshed at 10 Hz (`None` before any read-out).
    tiles: Option<[f32; 3]>,
    tile_age: f32,
}

fn view(live: &Live, i: usize) -> Option<f32> {
    (live.view.len() >= vw::LEN).then(|| live.view[i])
}

impl Decomp {
    /// The range the curve uses now: Match's, or the set one.
    fn range_now(&self, cx: &Ctx) -> f32 {
        let range = cx.v(ix::RANGE);
        match self.tiles {
            Some(t) if cx.on(ix::MATCH) => t[2].clamp(0.0, range),
            _ => range,
        }
    }

    fn slope(cx: &Ctx) -> f32 {
        1.0 / cx.v(ix::RATIO).clamp(1e-3, 1.0) - 1.0
    }

    /// Output level for input `l` at threshold `thr` and range `range`: `(before the
    /// ceiling, after it)`.
    fn out(cx: &Ctx, l: f32, thr: f32, range: f32) -> (f32, f32) {
        let g = static_gain_db(l as f64, thr as f64, cx.v(ix::RATIO) as f64, cx.v(ix::KNEE) as f64, range as f64) as f32;
        let o = l + mixed(g, cx.v(ix::MIX) * 0.01) + cx.v(ix::TRIM);
        (o, if cx.on(ix::CEILING_ON) { o.min(cx.v(ix::CEILING)) } else { o })
    }

    /// Where the curve turns parallel to unity (Range used up), if on the plot.
    fn range_point(cx: &Ctx, range: f32) -> Option<f32> {
        let s = Self::slope(cx);
        let l = cx.v(ix::THRESHOLD) + range / s.max(1e-6);
        (s > 1e-4 && range > 0.05 && l < -0.5).then_some(l)
    }

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let m = Map::square(CURVE);
        let thr = cx.v(ix::THRESHOLD);
        let range = self.range_now(cx);
        let ceil_on = cx.on(ix::CEILING_ON);
        let ceiling = cx.v(ix::CEILING);
        let levels: Vec<f32> = (0..=120).map(|i| -60.0 + i as f32 * 0.5).collect();
        let pts = |thr: f32, range: f32| -> (Vec<Point>, Vec<Point>) { levels.iter().map(|&l| Self::out(cx, l, thr, range)).zip(&levels).map(|((a, b), &l)| (Point::new(m.x(l), m.y(a)), Point::new(m.x(l), m.y(b)))).unzip() };
        let (lifted, capped) = pts(thr, range);
        s.clip_round(kit::rect(CURVE), 4.0);
        // Lift: the area between unity and the curve.
        let mut poly = lifted.clone();
        poly.push(Point::new(m.x(0.0), m.y(0.0)));
        poly.push(Point::new(m.x(-60.0), m.y(-60.0)));
        s.fill_poly(poly, lift_fill(t), lift_fill(t));
        // What the ceiling cuts off.
        if ceil_on {
            let mut cut = lifted.clone();
            cut.extend(capped.iter().rev());
            s.fill_poly(cut, t.accent.with_alpha(0.2), t.accent.with_alpha(0.2));
        }
        // Match: the set range as a dashed line above the curve Match uses.
        if cx.on(ix::MATCH) && (cx.v(ix::RANGE) - range).abs() > 0.05 {
            kit::dashed(s, &pts(thr, cx.v(ix::RANGE)).1, 3.0, 3.0, t.text, 1.0);
        }
        // Transient: the riding threshold's copy of the curve and its marker.
        let ride = view(live, vw::THRESHOLD).zip(view(live, vw::THRESHOLD + 1)).map(|(a, b)| a.max(b));
        if let Some(rt) = ride.filter(|r| cx.choice(ix::DETECTOR) == 2 && *r > thr + 0.05 && *r < 0.0) {
            kit::dashed(s, &pts(rt, range).1, 3.0, 3.0, t.warm, 1.0);
            let (x, y) = (m.x(rt), CURVE.1 + CURVE.3);
            s.fill_poly(vec![Point::new(x - 4.0, y), Point::new(x + 4.0, y), Point::new(x, y - 6.0)], t.warm, t.warm);
        }
        kit::curve(s, capped.clone(), t.accent, 2.0);
        // The ceiling line.
        if ceil_on {
            let y = m.y(ceiling);
            s.line(Point::new(CURVE.0, y), Point::new(CURVE.0 + CURVE.2, y), t.accent.with_alpha(0.8), 1.0);
            s.fill_round(Rect::new(CURVE.0 + CURVE.2 - 18.0, y - 4.0, 18.0, 8.0), 3.0, t.accent, t.accent, true);
            let ly = if y < CURVE.1 + 14.0 { y + 9.0 } else { y - 8.0 };
            s.text(Point::new(CURVE.0 + 8.0, ly), format!("CEILING {ceiling:.1}"), t.dim, 8.0, false);
        }
        // Handles.
        let (_, ty) = Self::out(cx, thr, thr, range);
        handle(s, t, Point::new(m.x(thr), m.y(ty)), t.accent);
        if let Some(l) = Self::range_point(cx, range) {
            let (_, y) = Self::out(cx, l, thr, range);
            handle(s, t, Point::new(m.x(l), m.y(y)), t.text);
        }
        // The operating dots.
        let keyed = view(live, vw::KEYED).is_some_and(|k| k >= 0.5);
        for c in 0..2 {
            if let (Some(e), Some(g)) = (view(live, vw::ENVELOPE + c), view(live, vw::GAIN + c)) {
                if e > -60.0 {
                    let mut o = e + mixed(g, cx.v(ix::MIX) * 0.01) + cx.v(ix::TRIM);
                    if ceil_on {
                        o = o.min(ceiling);
                    }
                    dot(s, t, Point::new(m.x(e.min(0.0)), m.y(o).max(CURVE.1 + 4.0)), keyed);
                }
            }
        }
        s.pop_clip();
        // Off the top with the ceiling off: an arrow where the curve leaves.
        if !ceil_on {
            if let Some(p) = lifted.iter().find(|p| p.y < CURVE.1) {
                s.text(Point::new(p.x.clamp(CURVE.0 + 8.0, CURVE.0 + CURVE.2 - 8.0), CURVE.1 + 8.0), "▲", t.over, 8.0, true);
            }
        }
        let tag = if cx.on(ix::MATCH) { format!("{} · MATCH {:.1} dB", ratio_text(cx.v(ix::RATIO)), range) } else { ratio_text(cx.v(ix::RATIO)) };
        kit::text_right(s, CURVE.0 + CURVE.2 - 8.0, CURVE.1 + CURVE.3 - 12.0, &tag, t.dim, 8.5);
    }

    fn paint_meters(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let top = cx.zones.picture.1;
        let scale = lift_scale_for(cx.v(ix::RANGE), &[12.0, 24.0]);
        for c in 0..2 {
            let r = meter(METER_X[c], top);
            lift_meter(s, t, r, view(live, vw::GAIN + c).unwrap_or(0.0), self.holds[c].hold, scale);
            lift_box(s, t, max_rect(METER_X[c]), self.holds[c].max);
        }
        let r = meter(METER_X[2], top);
        kit::gr_meter(s, t, r, view(live, vw::CEILING).unwrap_or(0.0), self.holds[2].hold, 12.0);
        if !cx.on(ix::CEILING_ON) {
            veil(s, t, (r.0, r.1, 14.0, r.3));
        }
        max_box(s, t, max_rect(METER_X[2]), self.holds[2].max);
    }

    fn paint_trace(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let ln = Lanes::new(cx.zones.picture.1);
        let scale = lift_scale_for(cx.v(ix::RANGE), &[12.0, 24.0]);
        let x = |age: f32| TX1 - age / SPAN_S * (TX1 - TX0);
        let pts = |f: &dyn Fn(&Sample) -> f32| -> Vec<Point> { self.trace.iter().map(|p| Point::new(x(p.age), f(p))).collect() };
        if self.trace.len() >= 2 {
            let lift = pts(&|p| ln.lift_y(p.lift, scale));
            kit::area(s, &lift, ln.zero(), t.text.with_alpha(0.55));
            let ceil = pts(&|p| ln.ceil_y(p.ceil));
            kit::area(s, &ceil, ln.zero(), t.accent.with_alpha(0.7));
            if cx.choice(ix::DETECTOR) == 2 {
                kit::dashed(s, &pts(&|p| ln.level_y(p.ride)), 3.0, 3.0, t.warm.with_alpha(0.7), 1.0);
            }
            kit::curve(s, pts(&|p| ln.level_y(p.env)), t.warm, 1.5);
        }
        trace_line(s, TX0, TX1, ln.level_y(cx.v(ix::THRESHOLD)), t.accent);
    }

    fn paint_tiles(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let matching = cx.on(ix::MATCH);
        let [lift, crest, mrange] = self.tiles.unwrap_or([0.0, f32::NAN, 0.0]);
        tile_text(s, TILES[0], &format!("+{:.1}", lift.max(0.0)), t.text, 22.0, 36.0);
        let crest_text = if crest.is_finite() && crest > 0.0 { format!("{crest:.1}") } else { "-".into() };
        tile_text(s, TILES[1], &crest_text, t.text, 22.0, 36.0);
        if matching {
            kit::text_right(s, TILES[1].0 + TILES[1].2 - 8.0, TILES[1].1 + 12.0, &format!("target {:.0}", cx.v(ix::MATCH_TARGET)), t.dim, 8.0);
            let used = if self.tiles.is_some() { mrange.clamp(0.0, cx.v(ix::RANGE)) } else { cx.v(ix::RANGE) };
            tile_text(s, TILES[2], &format!("{used:.1} of {:.1} dB", cx.v(ix::RANGE)), t.text, 15.0, 36.0);
        } else {
            tile_text(s, TILES[2], &format!("{:.1} dB", cx.v(ix::RANGE)), t.text, 22.0, 36.0);
        }
    }

    fn reset_holds(&mut self) {
        self.holds = [Hold::default(); 3];
    }
}

impl Page for Decomp {
    fn has_more(&self) -> bool {
        true
    }

    fn key_param(&self, _cx: &Ctx) -> Option<usize> {
        Some(ix::KEY_SOURCE)
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        matches!(local, ix::DETECTOR | ix::MATCH | ix::CEILING_ON)
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let hy = cx.zones.header.1;
        let p = |i: usize| cx.p(i);
        let mut v = vec![
            Cell::segmented(p(ix::DETECTOR), "DETECTOR", (420.0, hy + 8.0, 216.0, 24.0)).keys(&["PEAK", "RMS", "TRANSIENT"]).tip("Peak, RMS, or Transient: the threshold rides a crest above the programme's body"),
            Cell::pill(p(ix::MATCH), "MATCH", (648.0, hy + 9.0, 64.0, 22.0)).tip("Set the range from the programme's crest factor, up to the set range"),
            Cell::value(p(ix::MATCH_TARGET), "TARGET", (716.0, hy + 9.0, 72.0, 22.0)).format(target_text).dim(!cx.on(ix::MATCH)).tip("The crest factor Match aims for"),
        ];
        if cx.more {
            v.extend([
                Cell::knob(Size::Small, p(ix::STEREO_LINK), "STEREO LINK", 136.0, 644.0),
                Cell::knob(Size::Small, p(ix::KEY_HIGH_PASS), "DET HP", 232.0, 644.0).format(hp_text).tip("High-pass on the detector; OFF at the bottom"),
                Cell::knob(Size::Small, p(ix::KEY_LOW_PASS), "DET LP", 312.0, 644.0).format(lp_text).tip("Low-pass on the detector; OFF at the top"),
                Cell::knob(Size::Small, p(ix::TRIM), "TRIM", 408.0, 644.0).tip("Down only, before the ceiling: it can never pass it"),
            ]);
            return v;
        }
        let ceil_on = cx.on(ix::CEILING_ON);
        let det = cx.choice(ix::DETECTOR);
        v.extend([
            Cell::knob(Size::Big, p(ix::THRESHOLD), "THRESHOLD", 120.0, BIG_Y).tip("Above this level the peaks are lifted"),
            knob_grip(p(ix::RATIO), ratio_rect(), "Ratio 1:x: drag the knob up or right for more; right-click for the host menu"),
            Cell::knob(Size::Big, p(ix::RANGE), "RANGE", 360.0, BIG_Y).tip("The most it lifts"),
            Cell::knob(Size::Big, p(ix::ATTACK), "ATTACK", 480.0, BIG_Y).format(time_text),
            Cell::knob(Size::Big, p(ix::RELEASE), "RELEASE", 600.0, BIG_Y).format(time_text),
            Cell::pill(p(ix::CEILING_ON), "CEILING", (728.0, 633.0, 72.0, 22.0)).tip("A look-ahead limiter after the lift"),
            Cell::knob(Size::Mini, p(ix::CEILING), "CEILING", 728.0, 662.0).format(db_text).dim(!ceil_on),
            Cell::knob(Size::Mini, p(ix::KNEE), "KNEE", 784.0, 662.0),
            Cell::knob(Size::Mini, p(ix::LOOKAHEAD), "L-AHEAD", 840.0, 662.0),
            Cell::knob(Size::Mini, p(ix::MIX), "MIX", 896.0, 662.0),
            Cell::knob(Size::Mini, p(ix::DETECTOR_WINDOW), "WINDOW", 952.0, 662.0).format(time_text).dim(det == 0).tip("RMS window (also the body level in Transient)"),
            Cell::knob(Size::Mini, p(ix::TRANSIENT_CREST), "CREST", 1008.0, 662.0).dim(det != 2).tip("Transient: how far above the body the threshold rides"),
            Cell::segmented(p(ix::CEILING_TYPE), "CEILING TYPE", (728.0, 730.0, 176.0, 24.0)).keys(&["SAMPLE PEAK", "TRUE PEAK"]).dim(!ceil_on),
        ]);
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let top = cx.zones.picture.1;
        curve_frame(s, t, CURVE);
        let scale = lift_scale_for(cx.v(ix::RANGE), &[12.0, 24.0]);
        for (i, label) in ["L", "R", "CEIL"].iter().enumerate() {
            let r = meter(METER_X[i], top);
            if i < 2 {
                lift_scale(s, t, r, scale);
            } else {
                kit::gr_scale(s, t, r, 12.0);
            }
            let tag = if i < 2 { format!("▲{label}") } else { (*label).to_string() };
            // Between two labelled ticks: 3 dB up from the bottom, 9 dB down from the top.
            let y = if i < 2 { r.1 + r.3 * (1.0 - 3.0 / scale) } else { r.1 + r.3 * 0.75 };
            s.text(Point::new(r.0 + 28.0, y), tag, t.text, 7.0, true);
        }
        let ln = Lanes::new(top);
        kit::plot(s, t, (TRACE_X, top, TRACE_W, 616.0 - top));
        s.line(Point::new(TX0, ln.zero()), Point::new(TX1, ln.zero()), t.grid_strong, 1.0);
        for d in [scale * 0.5, scale] {
            let y = ln.lift_y(d, scale);
            s.line(Point::new(TX0, y), Point::new(TX1, y), t.grid, 1.0);
            kit::text_right(s, TX0 - 4.0, y, &format!("+{d:.0}"), t.dim, 8.0);
        }
        kit::text_right(s, TX0 - 4.0, ln.zero(), "0", t.dim, 8.0);
        let y6 = ln.ceil_y(6.0);
        s.line(Point::new(TX0, y6), Point::new(TX1, y6), t.grid, 1.0);
        for l in [0.0, -12.0, -24.0, -36.0, -48.0, -60.0] {
            let y = ln.level_y(l);
            s.line(Point::new(TX0, y), Point::new(TX1, y), t.grid, 1.0);
            if l < 0.0 && l > -60.0 {
                kit::text_right(s, TX0 - 4.0, y, &format!("{l:.0}"), t.dim, 8.0);
            }
        }
        time_axis(s, t, TX0, TX1, ln.y0, 592.0);
        s.text(Point::new(TX0 + 6.0, ln.y0 + 8.0), "LIFT", t.dim, 8.0, false);
        s.text(Point::new(TX0 + 6.0, ln.zero() + 10.0), "CEILING", t.dim, 8.0, false);
        s.text(Point::new(TX0 + 6.0, ln.level_top() + 8.0), "ENVELOPE", t.dim, 8.0, false);
        for (r, label) in TILES.iter().zip(["LIFT", "CREST", "RANGE"]) {
            tile_frame(s, t, *r, label);
        }
        if cx.more {
            kit::caption(s, t, 136.0, 634.0, "CHANNELS");
            kit::caption(s, t, 232.0, 634.0, "DETECTOR FILTER");
            kit::caption(s, t, 408.0, 634.0, "BEFORE THE CEILING");
        } else {
            s.line(Point::new(724.0, 628.0), Point::new(724.0, 764.0), t.border, 1.0);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        self.paint_curve(s, cx, live);
        self.paint_meters(s, cx, live);
        self.paint_trace(s, cx);
        self.paint_tiles(s, cx);
        if self.warning(cx) {
            warn_chip(s, t, chip(cx.zones.picture.1), "! NO CEILING");
        }
        if !cx.more {
            let r = ratio_rect();
            inverted_knob(s, t, r, cx.p(ix::RATIO), cx.v(ix::RATIO), "RATIO", &ratio_text(cx.v(ix::RATIO)));
            if cx.on(ix::MATCH) {
                let range = (360.0, BIG_Y, 120.0, 124.0);
                s.fill_rect(kit::rect((range.0, range.1 + range.3 + 0.5, range.2, 10.0)), t.panel, t.panel, true);
                sub_caption(s, t, range, "limit for Match");
            }
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let top = cx.zones.picture.1;
        let c = chip(top);
        let mut v = vec![CURVE, (324.0, top, 144.0, 616.0 - top), (TRACE_X, top, TRACE_W, 592.0 - top), TILES[0], TILES[1], TILES[2], (c.0 - 2.0, c.1 - 2.0, c.2 + 4.0, c.3 + 4.0)];
        if !cx.more {
            let r = ratio_rect();
            v.push((r.0 - 3.0, r.1 - 3.0, r.2 + 6.0, r.3 + 6.0));
            v.push((360.0, 760.0, 120.0, 12.0));
        }
        v
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max};
        &[Max, Max, Max, Max, Last, Last, Last, Last, Max, Last, Last, Last]
    }

    fn warning(&self, cx: &Ctx) -> bool {
        unguarded(cx.on(ix::CEILING_ON), cx.v(ix::RATIO), cx.v(ix::RANGE), cx.v(ix::MIX))
    }

    fn tick(&mut self, _cx: &Ctx, live: &Live) -> bool {
        for p in self.trace.iter_mut() {
            p.age += live.dt;
        }
        while self.trace.front().is_some_and(|p| p.age > SPAN_S) {
            self.trace.pop_front();
        }
        if live.view.len() < vw::LEN {
            return true;
        }
        let v = live.view;
        let lift = v[vw::GAIN].max(v[vw::GAIN + 1]);
        let env = v[vw::ENVELOPE].max(v[vw::ENVELOPE + 1]);
        self.trace.push_back(Sample { age: 0.0, lift, ceil: v[vw::CEILING], env, ride: v[vw::THRESHOLD].max(v[vw::THRESHOLD + 1]) });
        for (h, x) in self.holds.iter_mut().zip([v[vw::GAIN], v[vw::GAIN + 1], v[vw::CEILING]]) {
            h.push(x, live.dt);
        }
        self.tile_age += live.dt;
        if self.tiles.is_none() || self.tile_age >= 0.1 {
            self.tile_age = 0.0;
            self.tiles = Some([lift, v[vw::MATCH_CREST], v[vw::MATCH_RANGE]]);
        }
        true
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let top = cx.zones.picture.1;
        let mut v = vec![
            tip(CURVE, "Drag the threshold handle sideways, the rising part up for more ratio, the range handle (where the curve turns parallel) across unity, the ceiling line up or down; wheel on the knee; double-click resets"),
            tip((TRACE_X, top, TRACE_W, 616.0 - top), "The last 10 s: lift (up), ceiling reduction (down), the detector envelope and the threshold; drag the threshold line"),
            tip((324.0, top, 96.0, 616.0 - top), "Lift per channel; the boxes hold the most; click a box to reset"),
            tip(meter(METER_X[2], top), "What the ceiling takes off"),
            tip(TILES[1], "The programme's crest factor (peak against mean over 3 s)"),
        ];
        if self.warning(cx) {
            v.push(tip(chip(top), "The ceiling is off: lifted peaks are limited by nothing. Click to switch it on"));
        }
        v
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        if self.drag.is_some() {
            return Drag::follow(&mut self.drag, ev, io);
        }
        if !cx.more {
            if let Some(r) = knob_pointer(&mut self.drag, ev, cx, io, ratio_rect(), ix::RATIO) {
                return r;
            }
        }
        let m = Map::square(CURVE);
        if let Pointer::Wheel { x, y, dy } = ev {
            // The knee, over the knee zone of the curve.
            let l = m.level_at(x);
            if kit::inside(CURVE, x, y) && (l - cx.v(ix::THRESHOLD)).abs() <= cx.v(ix::KNEE) * 0.5 + 6.0 {
                io.set(cx.p(ix::KNEE), cx.v(ix::KNEE) + if dy > 0.0 { 1.0 } else { -1.0 });
                return Response::TAKEN;
            }
            return Response::NONE;
        }
        let Pointer::Down { x, y, clicks } = ev else { return Response::NONE };
        let top = cx.zones.picture.1;
        if self.warning(cx) && kit::inside(chip(top), x, y) {
            io.set(cx.p(ix::CEILING_ON), 1.0);
            return Response::TAKEN;
        }
        if (0..3).any(|i| kit::inside(max_rect(METER_X[i]), x, y)) {
            self.reset_holds();
            return Response::TAKEN;
        }
        let ln = Lanes::new(top);
        if kit::inside((TRACE_X, top, TRACE_W, 616.0 - top), x, y) {
            let ty = ln.level_y(cx.v(ix::THRESHOLD));
            if (y - ty).abs() <= 6.0 && (TX0..=TX1).contains(&x) {
                let _ = ln.level_at(y);
                return Drag::plain(cx, ix::THRESHOLD, x, y, Along::Y, 1.4 * ln.k).start(&mut self.drag, io, clicks);
            }
            return Response::NONE;
        }
        if !kit::inside(CURVE, x, y) {
            return Response::NONE;
        }
        let thr = cx.v(ix::THRESHOLD);
        let range = self.range_now(cx);
        let near = |px: f32, py: f32| (x - px).abs() <= 8.0 && (y - py).abs() <= 8.0;
        if cx.on(ix::CEILING_ON) && (y - m.y(cx.v(ix::CEILING))).abs() <= 5.0 {
            return Drag::plain(cx, ix::CEILING, x, y, Along::Y, m.ppd).start(&mut self.drag, io, clicks);
        }
        if let Some(l) = Self::range_point(cx, range) {
            if near(m.x(l), m.y(Self::out(cx, l, thr, range).1)) {
                return Drag::plain(cx, ix::RANGE, x, y, Along::Perp, m.ppd).start(&mut self.drag, io, clicks);
            }
        }
        if near(m.x(thr), m.y(Self::out(cx, thr, thr, range).1)) {
            return Drag::plain(cx, ix::THRESHOLD, x, y, Along::X, m.ppd).start(&mut self.drag, io, clicks);
        }
        if m.level_at(x) > thr {
            return Drag::norm(cx, ix::RATIO, y, true).start(&mut self.drag, io, clicks);
        }
        Response::TAKEN
    }
}

#[cfg(test)]
pub(super) mod dump {
    use crate::ui::tests::{editor, feed};
    use crate::ui::theme::ThemeId;
    use crate::LAYOUT;
    use openplugin::core::Editor;

    /// Renders a page in every theme, STANDARD, TALL and MORE, after 9 s of read-out from
    /// `view(tick)`: `build/ui-<key>-<std|tall|more>-<theme>.ppm`.
    pub fn dump(key: &str, view: &dyn Fn(usize) -> Vec<f32>, params: &[(&str, f32)]) {
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
                for k in 0..270 {
                    shared.set_view(slot, &view(k));
                    shared.raise_slot_gr(slot, 1.0);
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
    fn dump_decomp() {
        // Drum-like hits every 0.6 s: the envelope jumps over the threshold and the lift
        // follows, the ceiling catching the biggest.
        let view = |k: usize| {
            let ph = ((k + 3) % 18) as f32 / 18.0;
            let hit = (-ph * 5.0).exp();
            let big = if ((k + 3) / 18) % 3 == 0 { 1.0 } else { 0.6 };
            let env = -30.0 + 26.0 * hit * big;
            let lift = (dyn_decompress::static_gain_db(env as f64, -18.0, 0.5, 6.0, 6.0) as f32).max(0.0);
            let ceil = (env + lift + 1.0).max(0.0);
            vec![lift, lift * 0.9, env + 2.0, env + 1.0, -14.5, -14.0, env, env - 0.8, ceil, 9.4, 2.6, 0.0]
        };
        // A second set: ceiling off (the warning), PEAK.
        let plain = |k: usize| {
            let ph = (k % 24) as f32 / 24.0;
            let env = -26.0 + 20.0 * (-ph * 6.0).exp();
            let lift = (dyn_decompress::static_gain_db(env as f64, -18.0, 0.4, 6.0, 12.0) as f32).max(0.0);
            vec![lift, lift, env + 1.0, env + 1.0, -18.0, -18.0, env, env, 0.0, 8.1, 0.0, 1.0]
        };
        dump_as("decomp", "decomp-off", &plain, &[("ratio", 0.4), ("range", 12.0), ("ceilingOn", 0.0), ("keySource", 1.0)]);
        dump("decomp", &view, &[("ratio", 0.5), ("detector", 2.0), ("match", 1.0)]);
    }

    /// As [`dump`], written under another name.
    pub fn dump_as(key: &str, name: &str, view: &dyn Fn(usize) -> Vec<f32>, params: &[(&str, f32)]) {
        dump(key, view, params);
        for theme in ThemeId::ALL.map(|t| t.name()) {
            for mode in ["std", "tall", "more"] {
                let dir = env!("CARGO_MANIFEST_DIR");
                let _ = std::fs::rename(format!("{dir}/build/ui-{key}-{mode}-{theme}.ppm"), format!("{dir}/build/ui-{name}-{mode}-{theme}.ppm"));
            }
        }
    }
}
