//! DualSone (`crates/comp-sone`): a slow loudness leveller and a fast transient engine on one
//! loudness transfer curve, their meters, and a 10 s trace. See `design/compressors.md`
//! section 3.

use std::collections::VecDeque;

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use comp_sone::engine::{gain_computer, Core};
use comp_sone::{idx, view as vw};

use super::bands::common::*;
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::INFO;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != idx::COUNT {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(Sone::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). DualSone ignores the key;
/// a print with Delta on is wrong, so Delta is the warning.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (false, values.get(idx::DELTA).is_some_and(|v| *v >= 0.5))
}

const CURVE: R = (120.0, 416.0, 200.0, 200.0);
const METERS: [R; 3] = [(328.0, 416.0, 36.0, 176.0), (376.0, 416.0, 36.0, 176.0), (424.0, 416.0, 36.0, 176.0)];
const TRACE: R = (476.0, 416.0, 476.0, 200.0);
const TX0: f32 = 508.0;
const TX1: f32 = 920.0;
const TILES: [R; 3] = [(960.0, 416.0, 120.0, 60.0), (960.0, 486.0, 120.0, 60.0), (960.0, 556.0, 120.0, 60.0)];
const SPAN_S: f32 = 10.0;

fn max_rect(i: usize) -> R {
    (METERS[i].0 - 4.0, 596.0, 44.0, 18.0)
}

fn lamp(hy: f32) -> R {
    (852.0, hy + 9.0, 48.0, 22.0)
}

fn gr_y(db: f32) -> f32 {
    (426.0 + 2.9 * db.max(0.0)).min(484.0)
}

fn loud_y(l: f32) -> f32 {
    (492.0 - 2.0 * l).clamp(492.0, 592.0)
}

fn ratio_text(v: f32) -> String {
    format!("{v:.0}:1")
}

/// What the trace keeps per editor tick.
#[derive(Clone, Copy)]
struct Sample {
    age: f32,
    slow: f32,
    total: f32,
    loud: f32,
    reference: f32,
    hinge: f32,
}

#[derive(Clone, Debug)]
enum Drag {
    /// One parameter moved by an incremental drag: `along` picks the coordinate (x, y, or
    /// the 45 degree line), `per` is points per unit of the plain value (negative: inverse).
    Plain { param: usize, last: (f32, f32), v: f32, along: Along, per: f32 },
    /// A ratio, moved on its normalized travel.
    Norm { param: usize, last: f32, n: f32 },
}

#[derive(Clone, Copy, Debug)]
enum Along {
    X,
    Y,
    Diagonal,
}

pub struct Sone {
    drag: Option<Drag>,
    trace: VecDeque<Sample>,
    hold: [f32; 3],
    age: [f32; 3],
    max: [f32; 3],
    /// The tiles, refreshed at 10 Hz.
    tiles: [f32; 3],
    tile_age: f32,
}

impl Default for Sone {
    fn default() -> Self {
        Self { drag: None, trace: VecDeque::new(), hold: [0.0; 3], age: [0.0; 3], max: [0.0; 3], tiles: [-100.0, -100.0, 0.0], tile_age: 1.0 }
    }
}

impl Sone {
    fn view(live: &Live, i: usize) -> Option<f32> {
        (live.view.len() >= vw::LEN).then(|| live.view[i])
    }

    fn slow_gain(cx: &Ctx, l: f32) -> f32 {
        if cx.on(idx::SLOW_ON) {
            gain_computer(l, cx.v(idx::THRESHOLD), cx.v(idx::SLOW_RATIO), Core::SLOW_KNEE_DB)
        } else {
            0.0
        }
    }

    /// The fast engine's hinges: `(main, faint)`; the faint one is the FAST-LED hinge in MIX.
    fn hinges(cx: &Ctx, reference: f32) -> (f32, Option<f32>) {
        let (thr, crest) = (cx.v(idx::THRESHOLD), cx.v(idx::FAST_CREST));
        let led = thr + crest;
        match cx.choice(idx::MODE) {
            2 => (led, None),
            3 => ((reference + crest).max(thr), Some(led)),
            _ => ((reference + crest).max(thr), None),
        }
    }

    /// Loudness, reference, slow gain: from the read-out, or a resting picture without one.
    fn state(cx: &Ctx, live: &Live) -> (f32, f32, f32, bool) {
        match (Self::view(live, vw::LOUDNESS), Self::view(live, vw::REFERENCE), Self::view(live, vw::SLOW_GAIN)) {
            (Some(l), Some(r), Some(g)) if l > -99.0 => (l, if r > -99.0 { r } else { l }, g, true),
            _ => {
                let l = cx.v(idx::THRESHOLD) - 6.0;
                (l, l, Self::slow_gain(cx, l), false)
            }
        }
    }

    fn reset_holds(&mut self) {
        self.hold = [0.0; 3];
        self.age = [0.0; 3];
        self.max = [0.0; 3];
    }

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let m = Map::square(CURVE);
        let slow_a = if cx.on(idx::SLOW_ON) { 1.0 } else { 0.25 };
        let fast_a = if cx.on(idx::FAST_ON) { 1.0 } else { 0.25 };
        s.clip_round(kit::rect(CURVE), 4.0);
        // The slow curve and the gap to unity.
        let mut pts = Vec::with_capacity(64);
        let mut l = -60.0;
        while l <= 0.01 {
            pts.push(Point::new(m.x(l), m.y(l + Self::slow_gain(cx, l))));
            l += 1.0;
        }
        kit::between(s, &pts, &[Point::new(m.x(-60.0), m.y(-60.0)), Point::new(m.x(0.0), m.y(0.0))], t.accent.with_alpha(0.2 * slow_a));
        kit::curve(s, pts, t.accent.with_alpha(slow_a), 2.0);
        // The fast branch from the slow dot.
        let (loud, reference, gs, live_data) = Self::state(cx, live);
        let fr = cx.v(idx::FAST_RATIO);
        let branch = |hinge: f32| -> Vec<Point> {
            let mut v = Vec::new();
            let mut l = loud.max(-60.0);
            while l <= 0.01 {
                v.push(Point::new(m.x(l), m.y(l + gs + gain_computer(l, hinge, fr, Core::FAST_KNEE_DB))));
                l += 0.5;
            }
            v
        };
        let (hinge, faint) = Self::hinges(cx, reference);
        if let Some(h) = faint {
            kit::dashed(s, &branch(h), 3.0, 3.0, t.text.with_alpha(0.35 * fast_a), 1.0);
        }
        kit::curve(s, branch(hinge), t.text.with_alpha(fast_a), 1.5);
        // Handles: the threshold knee and the hinge.
        let thr = cx.v(idx::THRESHOLD);
        let th = Point::new(m.x(thr), m.y(thr + Self::slow_gain(cx, thr)));
        s.fill_ellipse_solid(Scene::ellipse_at(th, 4.5), t.panel);
        s.stroke_ellipse(Scene::ellipse_at(th, 4.5), t.accent.with_alpha(slow_a), 1.5);
        let hp = Point::new(m.x(hinge.min(0.0)), m.y((hinge + gs).min(0.0)));
        s.fill_ellipse_solid(Scene::ellipse_at(hp, 3.5), t.panel);
        s.stroke_ellipse(Scene::ellipse_at(hp, 3.5), t.text.with_alpha(fast_a), 1.2);
        // The dots.
        if live_data {
            s.fill_ellipse_solid(Scene::ellipse_at(Point::new(m.x(loud), m.y(loud + gs)), 4.0), t.accent.with_alpha(slow_a));
            if let (Some(fl), Some(gf)) = (Self::view(live, vw::FAST_LEVEL), Self::view(live, vw::FAST_GAIN)) {
                if fl > -60.0 {
                    s.stroke_ellipse(Scene::ellipse_at(Point::new(m.x(fl.min(0.0)), m.y((fl + gs + gf).clamp(-60.0, 0.0))), 4.0), t.text.with_alpha(fast_a), 1.5);
                }
            }
        }
        s.pop_clip();
        // The crest bracket on the x axis: what the fast engine leaves alone.
        let (a, b) = (m.x(loud.clamp(-60.0, 0.0)), m.x(hinge.clamp(-60.0, 0.0)));
        if b - a > 6.0 {
            let y = CURVE.1 + CURVE.3 - 10.0;
            s.line(Point::new(a, y), Point::new(b, y), t.text.with_alpha(0.6 * fast_a), 1.0);
            for x in [a, b] {
                s.line(Point::new(x, y - 3.0), Point::new(x, y + 3.0), t.text.with_alpha(0.6 * fast_a), 1.0);
            }
            let label = format!("CREST {:.0} dB", cx.v(idx::FAST_CREST));
            let lx = ((a + b) * 0.5).clamp(CURVE.0 + 36.0, CURVE.0 + CURVE.2 - 36.0);
            s.text(Point::new(lx, y - 9.0), label, t.dim, 8.0, true);
        }
        let unit = if cx.choice(idx::DETECTOR) == 0 { "LUFS" } else { "dB RMS" };
        s.text(Point::new(CURVE.0 + 8.0, CURVE.1 + 12.0), unit, t.dim, 8.5, false);
    }

    fn paint_meters(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let v = [vw::SLOW_GAIN, vw::FAST_GAIN, vw::TOTAL_GAIN].map(|i| Self::view(live, i).map_or(0.0, |g| (-g).max(0.0)));
        let on = [cx.on(idx::SLOW_ON), cx.on(idx::FAST_ON), true];
        for i in 0..3 {
            kit::gr_meter(s, t, METERS[i], v[i], self.hold[i], 20.0);
            if !on[i] {
                veil(s, t, (METERS[i].0, METERS[i].1, 14.0, METERS[i].3));
            }
            max_box(s, t, max_rect(i), self.max[i]);
        }
        for (i, l) in ["SLOW", "FAST", "TOTAL"].iter().enumerate() {
            s.text(Point::new(METERS[i].0 + 28.0, 588.0), *l, t.text, 8.0, true);
        }
    }

    fn paint_trace(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let x = |age: f32| TX1 - age / SPAN_S * (TX1 - TX0);
        let pts = |f: &dyn Fn(&Sample) -> f32| -> Vec<Point> { self.trace.iter().map(|p| Point::new(x(p.age), f(p))).collect() };
        if self.trace.len() >= 2 {
            let slow = pts(&|p| gr_y(-p.slow));
            kit::area(s, &slow, 426.0, t.accent.with_alpha(0.55));
            kit::curve(s, pts(&|p| gr_y(-p.total)), t.text, 1.0);
            kit::dashed(s, &pts(&|p| loud_y(p.hinge)), 3.0, 3.0, t.warm.with_alpha(0.6), 1.0);
            kit::curve(s, pts(&|p| loud_y(p.loud)), t.warm, 1.5);
        }
        let ty = loud_y(cx.v(idx::THRESHOLD));
        kit::dashed_h(s, TX0, TX1, ty, 5.0, 3.0, t.accent, 1.2);
        s.fill_round(Rect::new(TX1 - 18.0, ty - 5.0, 18.0, 10.0), 3.0, t.accent, t.accent, true);
        if Self::view(live, vw::HOLD).is_some_and(|v| v >= 0.5) {
            s.text(Point::new(TX1 - 24.0, 432.0), "HOLD", t.dim, 8.0, true);
        }
    }

    fn paint_tiles(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let unit = if cx.choice(idx::DETECTOR) == 0 { "LUFS" } else { "dB" };
        let texts = [
            if self.tiles[0] > -99.0 { format!("{:.1}", self.tiles[0]) } else { "-".into() },
            if self.tiles[1] > -99.0 { format!("{:.1}", self.tiles[1].max(0.0)) } else { "-".into() },
            if cx.on(idx::AUTO_GAIN) { format!("{:+.1}", self.tiles[2]) } else { "off".into() },
        ];
        for (i, r) in TILES.iter().enumerate() {
            s.text(Point::new(r.0 + r.2 * 0.5, r.1 + 36.0), texts[i].clone(), if i == 0 { t.warm } else { t.text }, 22.0, true);
            if i == 0 {
                kit::text_right(s, r.0 + r.2 - 8.0, r.1 + 12.0, unit, t.dim, 8.0);
            }
        }
    }
}

impl Page for Sone {
    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let hy = cx.zones.header.1;
        let p = |i: usize| cx.p(i);
        vec![
            Cell::segmented(p(idx::MODE), "MODE", (420.0, hy + 8.0, 288.0, 24.0)).keys(&["PARALLEL", "SERIES", "FAST-LED", "MIX"]).tip("How the two engines combine"),
            Cell::segmented(p(idx::DETECTOR), "DETECTOR", (716.0, hy + 8.0, 128.0, 24.0)).keys(&["LUFS (K)", "RMS"]),
            // MAIN
            Cell::knob(Size::Big, p(idx::THRESHOLD), "THRESHOLD", 120.0, 636.0).tip("The level the slow engine brings the programme toward"),
            Cell::knob(Size::Mini, p(idx::INPUT), "DRIVE", 248.0, 648.0),
            Cell::knob(Size::Mini, p(idx::MIX), "MIX", 304.0, 648.0),
            Cell::knob(Size::Mini, p(idx::OUTPUT), "MAKE-UP", 360.0, 648.0),
            Cell::pill(p(idx::AUTO_GAIN), "AUTOGAIN", (248.0, 728.0, 80.0, 22.0)),
            Cell::pill(p(idx::DELTA), "DELTA", (336.0, 728.0, 72.0, 22.0)).tip("Hear what the compressor takes away. Do not print with it on"),
            // SLOW
            Cell::pill(p(idx::SLOW_ON), "ON", (664.0, 622.0, 44.0, 22.0)).tip("Slow engine on"),
            Cell::knob(Size::Big, p(idx::SLOW_RATIO), "RATIO", 424.0, 636.0).format(ratio_text),
            Cell::knob(Size::Mini, p(idx::SLOW_ATTACK), "ATTACK", 552.0, 648.0).format(time_text),
            Cell::knob(Size::Mini, p(idx::SLOW_RELEASE), "RELEASE", 608.0, 648.0).format(time_text),
            Cell::segmented(p(idx::SLOW_WINDOW), "WINDOW", (552.0, 727.0, 156.0, 24.0)).keys(&["200 ms", "400 ms", "3 s"]).tip("Loudness window of the slow engine"),
            // FAST
            Cell::pill(p(idx::FAST_ON), "ON", (1028.0, 622.0, 44.0, 22.0)).tip("Fast engine on"),
            Cell::knob(Size::Big, p(idx::FAST_CREST), "CREST", 720.0, 636.0).tip("How far above the loudness a transient may stand before the fast engine acts"),
            Cell::knob(Size::Mini, p(idx::FAST_RATIO), "RATIO", 848.0, 648.0).format(ratio_text),
            Cell::knob(Size::Mini, p(idx::FAST_ATTACK), "ATTACK", 904.0, 648.0).format(time_text),
            Cell::knob(Size::Mini, p(idx::FAST_RELEASE), "RELEASE", 960.0, 648.0).format(time_text),
            Cell::knob(Size::Mini, p(idx::FAST_LOOKAHEAD), "L-AHEAD", 1016.0, 648.0),
        ]
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        curve_frame(s, t, CURVE);
        for r in METERS {
            kit::gr_scale(s, t, r, 20.0);
        }
        kit::plot(s, t, TRACE);
        for d in [0.0, 10.0, 20.0] {
            let y = gr_y(d);
            s.line(Point::new(TX0, y), Point::new(TX1, y), if d == 0.0 { t.grid_strong } else { t.grid }, 1.0);
            if d < 20.0 {
                kit::text_right(s, TX0 - 4.0, y, &format!("{d:.0}"), t.dim, 8.0);
            }
        }
        for l in [0.0, -12.0, -24.0, -36.0, -48.0] {
            let y = loud_y(l);
            s.line(Point::new(TX0, y), Point::new(TX1, y), t.grid, 1.0);
            if l < 0.0 {
                kit::text_right(s, TX0 - 4.0, y, &format!("{l:.0}"), t.dim, 8.0);
            }
        }
        for k in 0..=5 {
            let x = TX0 + (TX1 - TX0) * k as f32 / 5.0;
            s.line(Point::new(x, 424.0), Point::new(x, 592.0), t.grid, 1.0);
        }
        s.text(Point::new(TX0, 604.0), "-10 s", t.dim, 8.5, false);
        kit::text_right(s, TX1, 604.0, "now", t.dim, 8.5);
        // Lane names at the top of each lane.
        s.text(Point::new(TX0 + 6.0, 434.0), "GR", t.dim, 8.0, false);
        s.text(Point::new(TX0 + 6.0, 500.0), "LOUDNESS", t.dim, 8.0, false);
        for (r, label) in TILES.iter().zip(["LOUDNESS", "CREST", "AUTOGAIN"]) {
            kit::inset(s, t, *r, 6.0);
            s.text(Point::new(r.0 + 8.0, r.1 + 12.0), label, t.dim, 8.5, false);
        }
        // Control groups.
        // Group captions sit above the mini knobs (design 3.2: 552 and 848), clear of the big
        // knobs' own captions.
        s.text(Point::new(248.0, 631.0), "MAIN", t.dim, 10.0, false);
        s.text(Point::new(552.0, 631.0), "SLOW · loudness", t.dim, 10.0, false);
        s.text(Point::new(848.0, 631.0), "FAST · transients", t.dim, 10.0, false);
        for x in [420.0, 716.0] {
            s.line(Point::new(x, 628.0), Point::new(x, 764.0), t.border, 1.0);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        self.paint_curve(s, cx, live);
        self.paint_meters(s, cx, live);
        self.paint_trace(s, cx, live);
        self.paint_tiles(s, cx);
        let l = lamp(cx.zones.header.1);
        if cx.on(idx::DELTA) {
            warn_chip(s, t, l, "! DELTA");
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let l = lamp(cx.zones.header.1);
        vec![CURVE, (324.0, 416.0, 144.0, 200.0), (TRACE.0, TRACE.1, TRACE.2, 184.0), TILES[0], TILES[1], TILES[2], (l.0 - 2.0, l.1 - 2.0, l.2 + 4.0, l.3 + 4.0)]
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max, Min};
        &[Last, Max, Last, Last, Min, Min, Min, Last, Max]
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.on(idx::DELTA)
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        for p in self.trace.iter_mut() {
            p.age += live.dt;
        }
        while self.trace.front().is_some_and(|p| p.age > SPAN_S) {
            self.trace.pop_front();
        }
        if live.view.len() >= vw::LEN {
            let v = live.view;
            let reference = if v[vw::REFERENCE] > -99.0 { v[vw::REFERENCE] } else { v[vw::LOUDNESS] };
            self.trace.push_back(Sample { age: 0.0, slow: v[vw::SLOW_GAIN], total: v[vw::TOTAL_GAIN], loud: v[vw::LOUDNESS], reference, hinge: Self::hinges(cx, reference).0 });
            let r = [-v[vw::SLOW_GAIN], -v[vw::FAST_GAIN], -v[vw::TOTAL_GAIN]];
            for i in 0..3 {
                let r = r[i].max(0.0);
                if r >= self.hold[i] {
                    self.hold[i] = r;
                    self.age[i] = 0.0;
                } else {
                    self.age[i] += live.dt;
                    if self.age[i] > 1.5 {
                        self.hold[i] = r;
                    }
                }
                self.max[i] = self.max[i].max(r);
            }
            self.tile_age += live.dt;
            if self.tile_age >= 0.1 {
                self.tile_age = 0.0;
                self.tiles = [v[vw::LOUDNESS], v[vw::CREST], v[vw::AUTO_GAIN]];
            }
        }
        true
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let mut v = vec![
            tip(CURVE, "Drag the knee sideways for the threshold, the slow curve above it for its ratio, the fast hinge along its line for the crest, the bent part for the fast ratio; double-click resets"),
            tip(TRACE, "The last 10 s: reduction (slow filled, total line), loudness and the threshold; drag the threshold line"),
        ];
        if cx.on(idx::DELTA) {
            v.push(tip(lamp(cx.zones.header.1), "Delta is on: you hear only what is taken away. Click to switch it off"));
        }
        v
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        if let Some(drag) = self.drag.clone() {
            let (x, y, up) = match ev {
                Pointer::Move { x, y } => (x, y, false),
                Pointer::Up { x, y } => (x, y, true),
                _ => return Response::TAKEN,
            };
            let k = if io.modifiers().shift { 0.1 } else { 1.0 };
            match drag {
                Drag::Plain { param, last, mut v, along, per } => {
                    let d = match along {
                        Along::X => x - last.0,
                        Along::Y => last.1 - y,
                        Along::Diagonal => ((x - last.0) + (last.1 - y)) * 0.5,
                    };
                    v += d / per * k;
                    io.perform(param, v);
                    if up {
                        io.end(param);
                    }
                    self.drag = Some(Drag::Plain { param, last: (x, y), v, along, per });
                }
                Drag::Norm { param, last, mut n } => {
                    n = (n - (last - y) / 200.0 * k).clamp(0.0, 1.0);
                    io.perform(param, INFO.params[param].from_normalized(n));
                    if up {
                        io.end(param);
                    }
                    self.drag = Some(Drag::Norm { param, last: y, n });
                }
            }
            if up {
                self.drag = None;
            }
            return Response::TAKEN;
        }
        let Pointer::Down { x, y, clicks } = ev else { return Response::NONE };
        if cx.on(idx::DELTA) && kit::inside(lamp(cx.zones.header.1), x, y) {
            io.set(cx.p(idx::DELTA), 0.0);
            return Response::TAKEN;
        }
        if (0..3).any(|i| kit::inside(max_rect(i), x, y)) {
            self.reset_holds();
            return Response::TAKEN;
        }
        let start = |me: &mut Self, io: &mut Io, local: usize, drag: Drag| {
            let p = cx.p(local);
            if clicks >= 2 {
                io.set(p, INFO.params[p].default);
            } else {
                io.begin(p);
                me.drag = Some(drag);
            }
            Response::TAKEN
        };
        if kit::inside(TRACE, x, y) {
            let ty = loud_y(cx.v(idx::THRESHOLD));
            if (y - ty).abs() <= 6.0 && (TX0..=TX1).contains(&x) {
                let d = Drag::Plain { param: cx.p(idx::THRESHOLD), last: (x, y), v: cx.v(idx::THRESHOLD), along: Along::Y, per: 2.0 };
                return start(self, io, idx::THRESHOLD, d);
            }
            return Response::NONE;
        }
        if !kit::inside(CURVE, x, y) {
            return Response::NONE;
        }
        let m = Map::square(CURVE);
        let gs = Self::view_free_gain(cx, self.trace.back());
        let reference = self.trace.back().filter(|p| p.loud > -99.0).map_or(cx.v(idx::THRESHOLD) - 6.0, |p| p.reference);
        let (hinge, _) = Self::hinges(cx, reference);
        let hp = (m.x(hinge.min(0.0)), m.y((hinge + gs).min(0.0)));
        let thr = cx.v(idx::THRESHOLD);
        let th = (m.x(thr), m.y(thr + Self::slow_gain(cx, thr)));
        let near = |p: (f32, f32)| (x - p.0).abs() <= 8.0 && (y - p.1).abs() <= 8.0;
        if near(hp) {
            let d = Drag::Plain { param: cx.p(idx::FAST_CREST), last: (x, y), v: cx.v(idx::FAST_CREST), along: Along::Diagonal, per: m.ppd };
            return start(self, io, idx::FAST_CREST, d);
        }
        if near(th) {
            let d = Drag::Plain { param: cx.p(idx::THRESHOLD), last: (x, y), v: thr, along: Along::X, per: m.ppd };
            return start(self, io, idx::THRESHOLD, d);
        }
        let l = m.level_at(x);
        let fast_y = m.y(l + gs + gain_computer(l, hinge, cx.v(idx::FAST_RATIO), Core::FAST_KNEE_DB));
        if l > hinge && (y - fast_y).abs() <= 10.0 {
            let p = cx.p(idx::FAST_RATIO);
            let d = Drag::Norm { param: p, last: y, n: INFO.params[p].to_normalized(cx.v(idx::FAST_RATIO)) };
            return start(self, io, idx::FAST_RATIO, d);
        }
        if l > thr {
            let p = cx.p(idx::SLOW_RATIO);
            let d = Drag::Norm { param: p, last: y, n: INFO.params[p].to_normalized(cx.v(idx::SLOW_RATIO)) };
            return start(self, io, idx::SLOW_RATIO, d);
        }
        Response::TAKEN
    }
}

impl Sone {
    /// The slow gain the curve's fast branch starts from, from the newest trace sample.
    fn view_free_gain(cx: &Ctx, last: Option<&Sample>) -> f32 {
        match last {
            Some(p) => p.slow,
            None => Self::slow_gain(cx, cx.v(idx::THRESHOLD) - 6.0),
        }
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_sone() {
        let v = [-14.2, 7.1, 2.4, -15.0, -3.1, -1.8, -4.9, 0.0, -8.5];
        super::super::bands::dump::dump("sone", &v, &[("threshold", -20.0), ("delta", 1.0), ("autoGain", 1.0)]);
    }
}
