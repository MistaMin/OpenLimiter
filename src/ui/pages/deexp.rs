//! De-expander (`crates/dyn-deexpand`): brings back what a gate or expander pushed down.
//! Levels between the floor and the threshold are lifted toward the threshold; anything
//! under the floor is left alone. The picture is the transfer curve (−96..0 dB, the floor
//! defaults to −72) between its FLOOR and THRESHOLD markers, two lift meters, a 10 s trace of
//! the gaps being filled, and three tiles. See `design/new-modules.md` sections 0 and 3.
//!
//! Colour roles: `accent` the curve and the threshold, `text` the lift and the "after"
//! level, `warm` the detector ("before"), `dim` the floor and what lies under it.

use std::collections::VecDeque;

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use dyn_deexpand::{index as ix, static_gain_db, view as vw};

use super::bands::common::{hatch, sub_caption, time_text};
use super::decomp::{dot, handle, hp_text, lift_box, lift_fill, lift_meter, lift_scale, lift_scale_for, lp_text, max_rect, meter, mixed, tile_frame, tile_text, time_axis, trace_line, Along, Drag, Hold, SPAN_S};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != ix::COUNT {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(DeExp::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). It only lifts what lies
/// under the threshold toward it, so it never warns.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (values.get(ix::KEY_SOURCE).is_some_and(|v| *v >= 0.5), false)
}

const CURVE: R = (120.0, 416.0, 200.0, 200.0);
const LO: f32 = -96.0;
const METER_X: [f32; 2] = [328.0, 376.0];
const TRACE_X: f32 = 424.0;
const TRACE_W: f32 = 528.0;
const TX0: f32 = 456.0;
const TX1: f32 = 920.0;
const TILES: [R; 3] = [(960.0, 416.0, 120.0, 60.0), (960.0, 486.0, 120.0, 60.0), (960.0, 556.0, 120.0, 60.0)];
const SCALES: [f32; 3] = [12.0, 24.0, 36.0];

/// The square level map, −96..0 dB on both axes.
struct Map;

impl Map {
    const PPD: f32 = 200.0 / 96.0;

    fn x(l: f32) -> f32 {
        CURVE.0 + (l.clamp(LO, 0.0) - LO) * Self::PPD
    }

    fn y(o: f32) -> f32 {
        CURVE.1 + CURVE.3 - (o - LO) * Self::PPD
    }

    fn level_at(x: f32) -> f32 {
        (x - CURVE.0) / Self::PPD + LO
    }
}

/// The trace's lanes, plot y from `y0` to 592, scaled from the STANDARD 168 pt.
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

    fn lift_base(&self) -> f32 {
        self.y0 + 60.0 * self.k
    }

    fn lift_y(&self, db: f32, scale: f32) -> f32 {
        self.lift_base() - (db / scale).clamp(0.0, 1.0) * 56.0 * self.k
    }

    fn level_top(&self) -> f32 {
        self.y0 + 68.0 * self.k
    }

    fn level_y(&self, l: f32) -> f32 {
        self.level_top() - 1.0417 * self.k * l.clamp(LO, 0.0)
    }
}

fn ratio_text(v: f32) -> String {
    if v <= 1.0005 {
        "OFF".into()
    } else {
        format!("{v:.1}:1")
    }
}

#[derive(Clone, Copy)]
struct Sample {
    age: f32,
    lift: f32,
    env: f32,
}

#[derive(Default)]
struct DeExp {
    drag: Option<Drag>,
    trace: VecDeque<Sample>,
    holds: [Hold; 2],
    /// LIFT, LEVEL at 10 Hz (`None` before any read-out).
    tiles: Option<[f32; 2]>,
    tile_age: f32,
}

fn view(live: &Live, i: usize) -> Option<f32> {
    (live.view.len() >= vw::LEN).then(|| live.view[i])
}

impl DeExp {
    fn lift(cx: &Ctx, l: f32) -> f32 {
        let g = static_gain_db(l as f64, cx.v(ix::THRESHOLD) as f64, cx.v(ix::RATIO) as f64, cx.v(ix::KNEE) as f64, cx.v(ix::RANGE) as f64, cx.v(ix::FLOOR) as f64) as f32;
        mixed(g, cx.v(ix::MIX) * 0.01)
    }

    fn scale(cx: &Ctx) -> f32 {
        lift_scale_for(cx.v(ix::RANGE), &SCALES)
    }

    /// Where the lift reaches Range (the flat top), if it does before the floor takes over.
    fn range_point(cx: &Ctx) -> Option<f32> {
        let s = 1.0 - 1.0 / cx.v(ix::RATIO).max(1.0);
        let (thr, range, floor) = (cx.v(ix::THRESHOLD), cx.v(ix::RANGE), cx.v(ix::FLOOR));
        let l = thr - range / s.max(1e-6);
        (s > 1e-3 && range > 0.05 && l > floor + range && l > LO + 2.0).then_some(l)
    }

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let (thr, floor) = (cx.v(ix::THRESHOLD), cx.v(ix::FLOOR));
        let pts: Vec<Point> = (0..=192).map(|i| LO + i as f32 * 0.5).map(|l| Point::new(Map::x(l), Map::y(l + Self::lift(cx, l)))).collect();
        s.clip_round(kit::rect(CURVE), 4.0);
        // Under the floor: untouched.
        let xf = Map::x(floor);
        if xf > CURVE.0 + 1.0 {
            hatch(s, (CURVE.0, CURVE.1, xf - CURVE.0, CURVE.3), t.dim.with_alpha(0.35), 7.0);
        }
        kit::between(s, &pts, &[Point::new(Map::x(LO), Map::y(LO)), Point::new(Map::x(0.0), Map::y(0.0))], lift_fill(t));
        // The two markers.
        kit::dashed(s, &[Point::new(xf, CURVE.1), Point::new(xf, CURVE.1 + CURVE.3)], 3.0, 3.0, t.dim, 1.0);
        let xt = Map::x(thr);
        kit::dashed(s, &[Point::new(xt, CURVE.1), Point::new(xt, CURVE.1 + CURVE.3)], 3.0, 3.0, t.accent.with_alpha(0.6), 1.0);
        kit::curve(s, pts, t.accent, 2.0);
        // Handles: the threshold on the curve, the flat top, the floor tab at the bottom.
        handle(s, t, Point::new(xt, Map::y(thr + Self::lift(cx, thr))), t.accent);
        if let Some(l) = Self::range_point(cx) {
            handle(s, t, Point::new(Map::x(l), Map::y(l + Self::lift(cx, l))), t.text);
        }
        s.fill_round(Rect::new(xf - 4.0, CURVE.1 + CURVE.3 - 14.0, 8.0, 14.0), 2.5, t.dim, t.dim, true);
        let keyed = view(live, vw::KEYED).is_some_and(|k| k >= 0.5);
        for c in 0..2 {
            if let (Some(e), Some(g)) = (view(live, vw::ENVELOPE + c), view(live, vw::GAIN + c)) {
                if e > LO {
                    dot(s, t, Point::new(Map::x(e), Map::y(e + mixed(g, cx.v(ix::MIX) * 0.01))), keyed);
                }
            }
        }
        s.pop_clip();
        // Labels.
        if xf - CURVE.0 > 56.0 {
            s.text(Point::new((CURVE.0 + xf) * 0.5, CURVE.1 + 14.0), "UNTOUCHED", t.dim, 8.0, true);
        }
        if floor < LO {
            s.text(Point::new(CURVE.0 + 6.0, CURVE.1 + CURVE.3 - 22.0), format!("◀ {floor:.0}"), t.dim, 8.0, false);
        } else {
            let fx = (xf + 7.0).min(CURVE.0 + CURVE.2 - 30.0);
            s.text(Point::new(fx, CURVE.1 + CURVE.3 - 8.0), "FLOOR", t.dim, 8.0, false);
        }
        let tx = xt.clamp(CURVE.0 + 26.0, CURVE.0 + CURVE.2 - 26.0);
        s.text(Point::new(tx, CURVE.1 + 10.0), "THRESHOLD", t.accent, 8.0, true);
    }

    fn paint_meters(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let top = cx.zones.picture.1;
        for c in 0..2 {
            lift_meter(s, t, meter(METER_X[c], top), view(live, vw::GAIN + c).unwrap_or(0.0), self.holds[c].hold, Self::scale(cx));
            lift_box(s, t, max_rect(METER_X[c]), self.holds[c].max);
        }
    }

    fn paint_trace(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let ln = Lanes::new(cx.zones.picture.1);
        let scale = Self::scale(cx);
        let mix = cx.v(ix::MIX) * 0.01;
        let x = |age: f32| TX1 - age / SPAN_S * (TX1 - TX0);
        let pts = |f: &dyn Fn(&Sample) -> f32| -> Vec<Point> { self.trace.iter().map(|p| Point::new(x(p.age), f(p))).collect() };
        // The floor and what lies under it.
        let fy = ln.level_y(cx.v(ix::FLOOR));
        if fy < 591.0 {
            hatch(s, (TX0, fy, TX1 - TX0, 592.0 - fy), t.dim.with_alpha(0.3), 7.0);
        }
        if self.trace.len() >= 2 {
            kit::area(s, &pts(&|p| ln.lift_y(p.lift, scale)), ln.lift_base(), t.text.with_alpha(0.55));
            let before = pts(&|p| ln.level_y(p.env));
            let after = pts(&|p| ln.level_y(p.env + mixed(p.lift, mix)));
            kit::between(s, &after, &before, lift_fill(t));
            kit::curve(s, before, t.warm, 1.5);
            kit::curve(s, after, t.text, 1.0);
        }
        trace_line(s, TX0, TX1, fy.min(592.0), t.dim);
        trace_line(s, TX0, TX1, ln.level_y(cx.v(ix::THRESHOLD)), t.accent);
    }

    fn paint_tiles(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let Some([lift, env]) = self.tiles else {
            for r in TILES {
                tile_text(s, r, "-", t.dim, 22.0, 36.0);
            }
            return;
        };
        tile_text(s, TILES[0], &format!("+{:.1}", lift.max(0.0)), t.text, 22.0, 36.0);
        let level = if env > -150.0 { format!("{env:.1}") } else { "-".into() };
        tile_text(s, TILES[1], &level, t.warm, 22.0, 36.0);
        let zone = if env >= cx.v(ix::THRESHOLD) {
            "ABOVE THRESHOLD"
        } else if env <= cx.v(ix::FLOOR) {
            "UNDER FLOOR"
        } else {
            "LIFTING"
        };
        tile_text(s, TILES[2], zone, if zone == "LIFTING" { t.text } else { t.dim }, 12.0, 36.0);
    }
}

impl Page for DeExp {
    fn has_more(&self) -> bool {
        true
    }

    fn key_param(&self, _cx: &Ctx) -> Option<usize> {
        Some(ix::KEY_SOURCE)
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        local == ix::DETECTOR
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let hy = cx.zones.header.1;
        let p = |i: usize| cx.p(i);
        let mut v = vec![Cell::segmented(p(ix::DETECTOR), "DETECTOR", (420.0, hy + 8.0, 128.0, 24.0)).keys(&["PEAK", "RMS"])];
        if cx.more {
            v.extend([
                Cell::knob(Size::Small, p(ix::STEREO_LINK), "STEREO LINK", 136.0, 644.0),
                Cell::knob(Size::Small, p(ix::KEY_HIGH_PASS), "DET HP", 232.0, 644.0).format(hp_text).tip("High-pass on the detector; OFF at the bottom"),
                Cell::knob(Size::Small, p(ix::KEY_LOW_PASS), "DET LP", 312.0, 644.0).format(lp_text).tip("Low-pass on the detector; OFF at the top"),
            ]);
            return v;
        }
        v.extend([
            Cell::knob(Size::Big, p(ix::THRESHOLD), "THRESHOLD", 120.0, 636.0).tip("Below this level the quiet parts are lifted toward it"),
            Cell::knob(Size::Big, p(ix::RATIO), "RATIO", 240.0, 636.0).format(ratio_text).tip("Undoes an r:1 expander exactly"),
            Cell::knob(Size::Big, p(ix::RANGE), "RANGE", 360.0, 636.0).tip("The most it lifts"),
            Cell::knob(Size::Big, p(ix::FLOOR), "FLOOR", 480.0, 636.0).tip("Nothing under this level is lifted"),
            Cell::knob(Size::Big, p(ix::RELEASE), "RELEASE", 600.0, 636.0).format(time_text),
            Cell::knob(Size::Mini, p(ix::ATTACK), "ATTACK", 728.0, 662.0).format(time_text).tip("How fast the lift drops when the music comes back"),
            Cell::knob(Size::Mini, p(ix::HOLD), "HOLD", 784.0, 662.0).format(time_text),
            Cell::knob(Size::Mini, p(ix::LOOKAHEAD), "L-AHEAD", 840.0, 662.0),
            Cell::knob(Size::Mini, p(ix::KNEE), "KNEE", 896.0, 662.0),
            Cell::knob(Size::Mini, p(ix::DETECTOR_WINDOW), "WINDOW", 952.0, 662.0).format(time_text).dim(cx.choice(ix::DETECTOR) == 0).tip("RMS window"),
            Cell::knob(Size::Mini, p(ix::MIX), "MIX", 1008.0, 662.0),
        ]);
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let top = cx.zones.picture.1;
        kit::plot(s, t, CURVE);
        for d in [-72.0, -48.0, -24.0] {
            s.line(Point::new(Map::x(d), CURVE.1), Point::new(Map::x(d), CURVE.1 + CURVE.3), t.grid, 1.0);
            s.line(Point::new(CURVE.0, Map::y(d)), Point::new(CURVE.0 + CURVE.2, Map::y(d)), t.grid, 1.0);
        }
        kit::dashed(s, &[Point::new(Map::x(LO), Map::y(LO)), Point::new(Map::x(0.0), Map::y(0.0))], 3.0, 3.0, t.grid_strong, 1.0);
        let scale = Self::scale(cx);
        for (i, label) in ["▲L", "▲R"].iter().enumerate() {
            let r = meter(METER_X[i], top);
            lift_scale(s, t, r, scale);
            s.text(Point::new(r.0 + 28.0, r.1 + r.3 * (1.0 - 3.0 / scale)), *label, t.text, 8.0, true);
        }
        let ln = Lanes::new(top);
        kit::plot(s, t, (TRACE_X, top, TRACE_W, 616.0 - top));
        s.line(Point::new(TX0, ln.lift_base()), Point::new(TX1, ln.lift_base()), t.grid_strong, 1.0);
        kit::text_right(s, TX0 - 4.0, ln.lift_base(), "0", t.dim, 8.0);
        for d in [scale * 0.5, scale] {
            let y = ln.lift_y(d, scale);
            s.line(Point::new(TX0, y), Point::new(TX1, y), t.grid, 1.0);
            kit::text_right(s, TX0 - 4.0, y, &format!("+{d:.0}"), t.dim, 8.0);
        }
        for l in [0.0, -24.0, -48.0, -72.0, -96.0] {
            let y = ln.level_y(l);
            s.line(Point::new(TX0, y), Point::new(TX1, y), t.grid, 1.0);
            if l < 0.0 && l > LO {
                kit::text_right(s, TX0 - 4.0, y, &format!("{l:.0}"), t.dim, 8.0);
            }
        }
        time_axis(s, t, TX0, TX1, ln.y0, 592.0);
        s.text(Point::new(TX0 + 6.0, ln.y0 + 8.0), "LIFT", t.dim, 8.0, false);
        s.text(Point::new(TX0 + 6.0, ln.level_top() + 8.0), "BEFORE", t.warm, 8.0, false);
        s.text(Point::new(TX0 + 46.0, ln.level_top() + 8.0), "AFTER", t.text, 8.0, false);
        for (r, label) in TILES.iter().zip(["LIFT", "LEVEL", "ZONE"]) {
            tile_frame(s, t, *r, label);
        }
        if cx.more {
            kit::caption(s, t, 136.0, 634.0, "CHANNELS");
            kit::caption(s, t, 232.0, 634.0, "DETECTOR FILTER");
        } else {
            s.line(Point::new(724.0, 628.0), Point::new(724.0, 764.0), t.border, 1.0);
            kit::caption(s, t, 732.0, 640.0, "TIMING AND DETECTOR");
            sub_caption(s, t, (480.0, 636.0, 120.0, 124.0), "noise stays below");
            sub_caption(s, t, (600.0, 636.0, 120.0, 124.0), "how fast the lift grows");
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        self.paint_curve(s, cx, live);
        self.paint_meters(s, cx, live);
        self.paint_trace(s, cx);
        self.paint_tiles(s, cx);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let top = cx.zones.picture.1;
        vec![CURVE, (324.0, top, 96.0, 616.0 - top), (TRACE_X, top, TRACE_W, 592.0 - top), TILES[0], TILES[1], TILES[2]]
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max};
        &[Max, Max, Max, Max, Last, Last, Last, Last, Last]
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
        self.trace.push_back(Sample { age: 0.0, lift, env });
        for (h, x) in self.holds.iter_mut().zip([v[vw::GAIN], v[vw::GAIN + 1]]) {
            h.push(x, live.dt);
        }
        self.tile_age += live.dt;
        if self.tiles.is_none() || self.tile_age >= 0.1 {
            self.tile_age = 0.0;
            self.tiles = Some([lift, env]);
        }
        true
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let top = cx.zones.picture.1;
        vec![
            tip(CURVE, "Drag the threshold handle or the floor line sideways, the lifted part up for more ratio, the flat-top handle up or down for the range; wheel on the knee; double-click resets"),
            tip((TRACE_X, top, TRACE_W, 616.0 - top), "The last 10 s: lift (top), the level before and after, threshold and floor; drag either line"),
            tip((324.0, top, 96.0, 616.0 - top), "Lift per channel; the boxes hold the most; click a box to reset"),
            tip(TILES[2], "Where the detector is: above the threshold, lifting, or under the floor"),
        ]
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        if self.drag.is_some() {
            return Drag::follow(&mut self.drag, ev, io);
        }
        if let Pointer::Wheel { x, y, dy } = ev {
            let l = Map::level_at(x);
            if kit::inside(CURVE, x, y) && (l - cx.v(ix::THRESHOLD)).abs() <= cx.v(ix::KNEE) * 0.5 + 8.0 {
                io.set(cx.p(ix::KNEE), cx.v(ix::KNEE) + if dy > 0.0 { 1.0 } else { -1.0 });
                return Response::TAKEN;
            }
            return Response::NONE;
        }
        let Pointer::Down { x, y, clicks } = ev else { return Response::NONE };
        let top = cx.zones.picture.1;
        if (0..2).any(|i| kit::inside(max_rect(METER_X[i]), x, y)) {
            self.holds = [Hold::default(); 2];
            return Response::TAKEN;
        }
        let ln = Lanes::new(top);
        if kit::inside((TRACE_X, top, TRACE_W, 616.0 - top), x, y) && (TX0..=TX1).contains(&x) {
            let per = 1.0417 * ln.k;
            if (y - ln.level_y(cx.v(ix::THRESHOLD))).abs() <= 6.0 {
                return Drag::plain(cx, ix::THRESHOLD, x, y, Along::Y, per).start(&mut self.drag, io, clicks);
            }
            if (y - ln.level_y(cx.v(ix::FLOOR)).min(592.0)).abs() <= 6.0 {
                return Drag::plain(cx, ix::FLOOR, x, y, Along::Y, per).start(&mut self.drag, io, clicks);
            }
            return Response::NONE;
        }
        if !kit::inside(CURVE, x, y) {
            return Response::NONE;
        }
        let (thr, floor) = (cx.v(ix::THRESHOLD), cx.v(ix::FLOOR));
        let near = |px: f32, py: f32| (x - px).abs() <= 8.0 && (y - py).abs() <= 8.0;
        let ppd = Map::PPD;
        if let Some(l) = Self::range_point(cx) {
            if near(Map::x(l), Map::y(l + Self::lift(cx, l))) {
                return Drag::plain(cx, ix::RANGE, x, y, Along::Y, ppd).start(&mut self.drag, io, clicks);
            }
        }
        if near(Map::x(thr), Map::y(thr + Self::lift(cx, thr))) {
            return Drag::plain(cx, ix::THRESHOLD, x, y, Along::X, ppd).start(&mut self.drag, io, clicks);
        }
        if (x - Map::x(floor)).abs() <= 5.0 {
            return Drag::plain(cx, ix::FLOOR, x, y, Along::X, ppd).start(&mut self.drag, io, clicks);
        }
        if (x - Map::x(thr)).abs() <= 4.0 {
            return Drag::plain(cx, ix::THRESHOLD, x, y, Along::X, ppd).start(&mut self.drag, io, clicks);
        }
        let l = Map::level_at(x);
        if l > floor && l < thr {
            return Drag::norm(cx, ix::RATIO, y, false).start(&mut self.drag, io, clicks);
        }
        Response::TAKEN
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_deexp() {
        // A gated vocal: phrases at -18 with gaps the gate pushed toward -60.
        let view = |k: usize| {
            let ph = ((k + 20) % 60) as f32 / 60.0;
            let env = if ph < 0.45 { -18.0 - 6.0 * (ph * 20.0).sin().abs() } else { -18.0 - 44.0 * (1.0 - (-(ph - 0.45) * 12.0).exp()) };
            let lift = dyn_deexpand::static_gain_db(env as f64, -40.0, 2.0, 6.0, 12.0, -72.0) as f32;
            vec![lift, lift * 0.95, env + 1.0, env + 0.5, -40.0, -72.0, env, env - 0.6, 0.0]
        };
        super::super::decomp::dump::dump("deexp", &view, &[]);
    }
}
