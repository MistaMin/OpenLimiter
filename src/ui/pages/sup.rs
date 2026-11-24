//! The Resonance Suppressor page (`reso-suppress`, `design/tone.md` section 3): the spectrum
//! with the removed part shaded (input outline, output body, the gap in `accent`), the same
//! cut hanging from the top as a reduction lane, the region span, the floor line and the four
//! focus handles in the graph, a total reduction meter, and a MORE face for the focus regions.

use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use reso_suppress::engine::{display_band_hz, DISPLAY_BANDS};
use reso_suppress::{focus_param, view, FOCUS_REGIONS, NPARAMS, P_ATTACK, P_CENTRE, P_DELTA, P_DEPTH, P_FLOOR, P_HARD, P_LINK, P_MID_SIDE, P_MIX, P_RELEASE, P_SELECTIVITY, P_SHARPNESS, P_SIDECHAIN, P_WIDTH};

use super::eqgraph::{self as g, Axis, Spectra};
use crate::ui::analyser::Spectrum;
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;

const SCALES: [f32; 3] = [6.0, 12.0, 24.0];
const SCALE_KEYS: &[&str] = &["-6", "-12", "-24"];
const SHOW_KEYS: &[&str] = &["L/R", "M/S"];
const READOUT: R = (944.0, 640.0, 136.0, 44.0);
const KEY_METER: R = (944.0, 700.0, 136.0, 30.0);
/// The region width that means "all of it".
const ALL_OCT: f32 = 9.95;
/// Seconds the meter's hold line stays.
const HOLD_S: f32 = 1.5;

static MERGE: [Merge; view::LEN] = {
    let mut m = [Merge::Max; view::LEN];
    m[view::DEEPEST_HZ] = Merge::Last;
    m
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// The region span: the centre (host) and the pointer's frequency offset (octaves).
    Span(usize, f32),
    /// A region edge: width (host), and the centre it measures from.
    Edge(usize, f32),
    /// The floor line (host).
    Floor(usize),
    /// Depth in the reduction lane: host index, value and y at the grab.
    Depth(usize, f32, f32),
    /// A focus handle: its frequency and amount (host), and the baseline y.
    Focus(usize, usize, f32),
}

struct Sup {
    scale: usize,
    /// `None` follows the M/S switch.
    show: Option<usize>,
    drag: Option<Drag>,
    spectra: Spectra,
    hold: (f32, f32),
    max: (f32, f32),
    pulse: f32,
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    debug_assert_eq!(count, NPARAMS);
    let _ = count;
    Box::new(Sup { scale: 1, show: None, drag: None, spectra: Spectra::default(), hold: (0.0, 0.0), max: (0.0, 0.0), pulse: 0.0 })
}

/// `(listens to the sidechain key, output can pass its ceiling)`: the Sidechain switch, and
/// DELTA (a listen mode).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let on = |i: usize| values.get(i).is_some_and(|&v| v >= 0.5);
    (on(P_SIDECHAIN), on(P_DELTA))
}

fn width_text(v: f32) -> String {
    if v >= ALL_OCT {
        "ALL".into()
    } else {
        format!("{v:.1} oct")
    }
}

/// Reduction (dB, positive) of a set at a frequency, read between the display bands.
fn reduction_at(live: &Live, set: usize, f: f32) -> f32 {
    let base = if set == 0 { view::SET_A } else { view::SET_B };
    let pos = ((f / 20.0).ln() / 1000f32.ln() * (DISPLAY_BANDS - 1) as f32).clamp(0.0, (DISPLAY_BANDS - 1) as f32);
    let i = pos.floor() as usize;
    let j = (i + 1).min(DISPLAY_BANDS - 1);
    let m = pos - i as f32;
    let v = |k: usize| live.view.get(base + k).copied().unwrap_or(0.0).max(0.0);
    v(i) + (v(j) - v(i)) * m
}

impl Sup {
    fn plot(cx: &Ctx) -> R {
        g::graph_rect(&cx.zones, 80.0)
    }

    fn lane_h(cx: &Ctx) -> f32 {
        if Self::plot(cx).3 > 250.0 {
            64.0
        } else {
            40.0
        }
    }

    /// The spectrum's axis: the plot below the reduction lane.
    fn axis(cx: &Ctx) -> Axis {
        let p = Self::plot(cx);
        let lane = Self::lane_h(cx);
        Axis::new((p.0, p.1 + lane, p.2, p.3 - lane), 12.0)
    }

    fn lane(cx: &Ctx) -> R {
        let p = Self::plot(cx);
        (p.0, p.1, p.2, Self::lane_h(cx))
    }

    fn meter(cx: &Ctx) -> R {
        let p = Self::plot(cx);
        (1040.0, p.1, 36.0, p.3 - 10.0)
    }

    fn max_box(cx: &Ctx) -> R {
        let z = cx.zones.picture;
        (1036.0, z.1 + z.3 - 22.0, 44.0, 18.0)
    }

    fn key_chip(cx: &Ctx) -> R {
        (420.0, cx.zones.header.1 + 9.0, 140.0, 22.0)
    }

    fn show_rect(cx: &Ctx) -> R {
        (568.0, cx.zones.header.1 + 8.0, 112.0, 24.0)
    }

    fn scale_rect(cx: &Ctx) -> R {
        (688.0, cx.zones.header.1 + 8.0, 132.0, 24.0)
    }

    fn show(&self, cx: &Ctx) -> usize {
        self.show.unwrap_or(usize::from(cx.on(P_MID_SIDE)))
    }

    /// The region's edges in Hz, or `None` for ALL.
    fn region(cx: &Ctx) -> Option<(f32, f32)> {
        let w = cx.v(P_WIDTH);
        (w < ALL_OCT).then(|| {
            let c = cx.v(P_CENTRE);
            let k = 2f32.powf(w * 0.5);
            (c / k, c * k)
        })
    }

    fn floor_y(cx: &Ctx) -> f32 {
        Self::axis(cx).spec_y(cx.v(P_FLOOR))
    }

    fn focus_base(cx: &Ctx) -> f32 {
        Self::axis(cx).top() + 24.0
    }

    /// Focus handles that are shown: `(point, region)`.
    fn focus_points(cx: &Ctx) -> Vec<(Point, usize)> {
        let axis = Self::axis(cx);
        let base = Self::focus_base(cx);
        (0..FOCUS_REGIONS).map(|r| (Point::new(axis.x(cx.v(focus_param(r, 1))), base - cx.v(focus_param(r, 0)) / 100.0 * 16.0), r)).collect()
    }

    fn lane_y(&self, cx: &Ctx, db: f32) -> f32 {
        let l = Self::lane(cx);
        l.1 + (db / SCALES[self.scale]).clamp(0.0, 1.0) * (l.3 - 4.0)
    }

    fn handle(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let axis = Self::axis(cx);
        match ev {
            Pointer::Down { x, y, clicks } => {
                if let Some(k) = g::switch_key(Self::scale_rect(cx), SCALES.len(), x, y) {
                    self.scale = k;
                    return Response::REBUILD;
                }
                if let Some(k) = g::switch_key(Self::show_rect(cx), 2, x, y) {
                    self.show = Some(k);
                    self.spectra.clear();
                    return Response::TAKEN;
                }
                if kit::inside(Self::key_chip(cx), x, y) {
                    io.set(cx.p(P_SIDECHAIN), if cx.on(P_SIDECHAIN) { 0.0 } else { 1.0 });
                    return Response::REBUILD;
                }
                if kit::inside(Self::max_box(cx), x, y) {
                    self.max = (0.0, 0.0);
                    return Response::TAKEN;
                }
                let plot = Self::plot(cx);
                if !kit::inside(plot, x, y) {
                    return Response::NONE;
                }
                for (at, r) in Self::focus_points(cx) {
                    if (at.x - x).abs() <= 7.0 && (at.y - y).abs() <= 7.0 {
                        let (f, a) = (cx.p(focus_param(r, 1)), cx.p(focus_param(r, 0)));
                        if clicks >= 2 {
                            io.set(a, 0.0);
                            return Response::TAKEN;
                        }
                        io.begin(f);
                        io.begin(a);
                        self.drag = Some(Drag::Focus(f, a, Self::focus_base(cx)));
                        return Response::TAKEN;
                    }
                }
                if kit::inside(Self::lane(cx), x, y) {
                    let p = cx.p(P_DEPTH);
                    io.begin(p);
                    self.drag = Some(Drag::Depth(p, cx.v(P_DEPTH), y));
                    return Response::TAKEN;
                }
                if (y - Self::floor_y(cx)).abs() <= 4.0 {
                    let p = cx.p(P_FLOOR);
                    io.begin(p);
                    self.drag = Some(Drag::Floor(p));
                    return Response::TAKEN;
                }
                let centre = cx.v(P_CENTRE);
                if let Some((lo, hi)) = Self::region(cx) {
                    let (xl, xh) = (axis.x(lo), axis.x(hi));
                    if (x - xl).abs() <= 5.0 || (x - xh).abs() <= 5.0 {
                        let p = cx.p(P_WIDTH);
                        io.begin(p);
                        self.drag = Some(Drag::Edge(p, centre));
                        return Response::TAKEN;
                    }
                    if x > xl && x < xh {
                        if clicks >= 2 {
                            io.set(cx.p(P_WIDTH), 10.0);
                            return Response::TAKEN;
                        }
                        let p = cx.p(P_CENTRE);
                        io.begin(p);
                        self.drag = Some(Drag::Span(p, (axis.freq(x) / centre).log2()));
                        return Response::TAKEN;
                    }
                } else if clicks >= 2 {
                    // From ALL, a double-click makes a two-octave region there.
                    io.set(cx.p(P_CENTRE), axis.freq(x));
                    io.set(cx.p(P_WIDTH), 2.0);
                    return Response::TAKEN;
                }
                Response::TAKEN
            }
            Pointer::Move { x, y } => {
                match self.drag {
                    Some(Drag::Span(p, off)) => io.perform(p, axis.freq(x) / 2f32.powf(off)),
                    Some(Drag::Edge(p, centre)) => io.perform(p, (2.0 * (axis.freq(x) / centre).log2().abs()).max(0.1)),
                    Some(Drag::Floor(p)) => {
                        let db = g::SPEC_TOP - (y - axis.top()) / axis.plot.3 * (g::SPEC_TOP - g::SPEC_BOTTOM);
                        io.perform(p, db);
                    }
                    Some(Drag::Depth(p, from, y0)) => io.perform(p, from + (y - y0) * 0.6),
                    Some(Drag::Focus(f, a, base)) => {
                        io.perform(f, axis.freq(x));
                        io.perform(a, ((base - y) / 16.0 * 100.0).clamp(-100.0, 100.0));
                    }
                    None => return Response::NONE,
                }
                Response::TAKEN
            }
            Pointer::Up { .. } => {
                match self.drag.take() {
                    Some(Drag::Span(p, _) | Drag::Edge(p, _) | Drag::Floor(p) | Drag::Depth(p, _, _)) => io.end(p),
                    Some(Drag::Focus(f, a, _)) => {
                        io.end(f);
                        io.end(a);
                    }
                    None => return Response::NONE,
                }
                Response::TAKEN
            }
            Pointer::Wheel { x, y, dy } => {
                let up = dy > 0.0;
                for (at, r) in Self::focus_points(cx) {
                    if (at.x - x).abs() <= 7.0 && (at.y - y).abs() <= 7.0 {
                        g::nudge_log(io, cx.p(focus_param(r, 2)), if up { 0.125 } else { -0.125 });
                        return Response::TAKEN;
                    }
                }
                if let Some((lo, hi)) = Self::region(cx) {
                    if x > axis.x(lo) && x < axis.x(hi) && kit::inside(Self::plot(cx), x, y) {
                        g::nudge(io, cx.p(P_WIDTH), if up { 0.25 } else { -0.25 });
                        return Response::TAKEN;
                    }
                }
                Response::NONE
            }
        }
    }
}

impl Page for Sup {
    fn tall(&self) -> bool {
        true
    }

    fn has_more(&self) -> bool {
        true
    }

    fn key_param(&self, _cx: &Ctx) -> Option<usize> {
        Some(P_SIDECHAIN)
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        if cx.more {
            let mut cells = Vec::new();
            for r in 0..FOCUS_REGIONS {
                let x0 = 120.0 + 240.0 * r as f32;
                cells.push(Cell::knob(Size::Small, cx.p(focus_param(r, 0)), "AMOUNT", x0, 648.0).format(g::pct_text).tip("Above 0 works harder here, below 0 lighter"));
                cells.push(Cell::knob(Size::Small, cx.p(focus_param(r, 1)), "FREQ", x0 + 76.0, 648.0).format(g::hz_text));
                cells.push(Cell::knob(Size::Small, cx.p(focus_param(r, 2)), "Q", x0 + 152.0, 648.0).format(g::q_text));
            }
            return cells;
        }
        vec![
            Cell::knob(Size::Big, cx.p(P_DEPTH), "DEPTH", 120.0, 636.0).format(g::pct_text).tip("How much of each resonance comes out; 0 is off"),
            Cell::knob(Size::Big, cx.p(P_SHARPNESS), "SHARP", 248.0, 636.0).format(g::pct_text).tip("How narrow a peak must be"),
            Cell::knob(Size::Big, cx.p(P_SELECTIVITY), "SELECT", 376.0, 636.0).format(g::pct_text).tip("How far a peak must stand out of its surroundings"),
            Cell::knob(Size::Small, cx.p(P_FLOOR), "FLOOR", 512.0, 636.0).format(g::db_text).tip("Nothing below this level is touched"),
            Cell::knob(Size::Small, cx.p(P_ATTACK), "ATTACK", 596.0, 636.0).format(g::ms_text),
            Cell::knob(Size::Small, cx.p(P_RELEASE), "RELEASE", 680.0, 636.0).format(g::ms_text),
            Cell::knob(Size::Small, cx.p(P_LINK), "LINK", 764.0, 636.0).format(g::pct_text).tip("Stereo link of the two channel sets"),
            Cell::knob(Size::Small, cx.p(P_MIX), "MIX", 848.0, 636.0).format(g::pct_text),
            Cell::value(cx.p(P_CENTRE), "FREQ", (512.0, 742.0, 64.0, 22.0)).format(g::hz_short).tip("Region centre"),
            Cell::value(cx.p(P_WIDTH), "WIDTH", (584.0, 742.0, 64.0, 22.0)).format(width_text).tip("Region width in octaves; the top is ALL"),
            Cell::pill(cx.p(P_MID_SIDE), "M/S", (680.0, 742.0, 56.0, 22.0)).tip("Work on mid and side instead of left and right"),
            Cell::pill(cx.p(P_HARD), "HARD", (744.0, 742.0, 56.0, 22.0)).tip("Harder, faster cuts"),
            Cell::pill(cx.p(P_DELTA), "DELTA", (808.0, 742.0, 56.0, 22.0)).tip("Hear only what is removed"),
        ]
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let plot = Self::plot(cx);
        kit::plot(s, t, plot);
        let axis = Self::axis(cx);
        kit::freq_grid(s, t, axis.plot, g::F_LO, g::F_HI, 0.0, 96.0, 24.0);
        let lane = Self::lane(cx);
        s.line(Point::new(lane.0, lane.1 + lane.3), Point::new(lane.0 + lane.2, lane.1 + lane.3), t.grid_strong, 1.0);
        kit::text_right(s, plot.0 - 4.0, lane.1 + 6.0, "0", t.dim, 8.5);
        // No scale mark inside the lane: at 1x it ran into the spectrum's "+0"; the header's
        // -6 / -12 / -24 switch shows the lane's scale.
        let m = Self::meter(cx);
        kit::gr_scale(s, t, m, SCALES[self.scale]);
        if cx.more {
            for r in 0..FOCUS_REGIONS {
                kit::caption(s, t, 120.0 + 240.0 * r as f32, 640.0, &format!("FOCUS {}", r + 1));
            }
        } else {
            g::captions(s, t, &[(512.0, 732.0, "REGION"), (584.0, 732.0, "WIDTH")]);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let axis = Self::axis(cx);
        let plot = Self::plot(cx);
        let delta = cx.on(P_DELTA);
        let show = self.show(cx);
        s.clip_round(kit::rect(plot), 4.0);
        // Input outline, output body (input less the reduction), the removed part between.
        if let Some(pre) = g::spectrum_points(&axis, &self.spectra.pre) {
            let spec: &Spectrum = &self.spectra.pre;
            let body: Vec<Point> = (0..spec.db.len()).map(|i| Point::new(pre[i].x, axis.spec_y(spec.db[i] - reduction_at(live, 0, spec.freq(i))))).collect();
            let bottom = axis.bottom();
            if delta {
                g::fill_between(s, &pre, &body, t.accent.with_alpha(0.55));
            } else {
                kit::area(s, &body, bottom, t.out_fill.with_alpha(t.out_fill_alpha * 0.35));
                g::fill_between(s, &pre, &body, t.accent.with_alpha(0.5));
            }
            kit::curve(s, pre, t.in_line, 1.0);
        }
        if cx.on(P_SIDECHAIN) {
            g::paint_spectrum(s, &axis, &self.spectra.key, t.warm, 0.9, false);
        }
        // The region: frequencies outside it dimmed, its edges as handles.
        if let Some((lo, hi)) = Self::region(cx) {
            let (xl, xh) = (axis.x(lo).max(plot.0), axis.x(hi).min(plot.0 + plot.2));
            let veil = t.plot_bg.with_alpha(0.6);
            if xl > plot.0 {
                s.fill_rect(Rect::new(plot.0, axis.top(), xl - plot.0, axis.plot.3), veil, veil, true);
            }
            if xh < plot.0 + plot.2 {
                s.fill_rect(Rect::new(xh, axis.top(), plot.0 + plot.2 - xh, axis.plot.3), veil, veil, true);
            }
            for x in [axis.x(lo), axis.x(hi)] {
                s.line(Point::new(x, axis.top()), Point::new(x, axis.bottom()), t.text.with_alpha(0.5), 1.0);
                // Below the focus handles' travel (top + 3 .. top + 45).
                s.fill_round(Rect::new(x - 3.0, axis.top() + 46.0, 6.0, 16.0), 2.0, t.text, t.text, true);
            }
        }
        // The floor.
        kit::dashed_h(s, plot.0, plot.0 + plot.2, Self::floor_y(cx), 6.0, 4.0, t.dim, 1.0);
        s.text(Point::new(plot.0 + plot.2 - 60.0, Self::floor_y(cx) - 7.0), format!("FLOOR {:.0}", cx.v(P_FLOOR)), t.dim, 8.0, false);
        // The reduction lane: set A solid, set B dashed (when the sets can differ).
        let lane = Self::lane(cx);
        let freqs: Vec<f32> = (0..DISPLAY_BANDS).map(display_band_hz).collect();
        for set in 0..2 {
            if set == 1 && !(show == 1 || cx.v(P_LINK) < 99.5) {
                continue;
            }
            let pts: Vec<Point> = freqs.iter().map(|&f| Point::new(axis.x(f), self.lane_y(cx, reduction_at(live, set, f)))).collect();
            if set == 0 {
                kit::area(s, &pts, lane.1, t.accent.with_alpha(0.3));
                kit::curve(s, pts, t.accent, 1.5);
            } else {
                kit::dashed(s, &pts, 4.0, 3.0, t.accent, 1.0);
            }
        }
        let names = if cx.on(P_MID_SIDE) { ["MID", "SIDE"] } else { ["L", "R"] };
        s.text(Point::new(plot.0 + 8.0, lane.1 + 10.0), format!("CUT {} solid, {} dashed · DEPTH {:.0} %", names[0], names[1], cx.v(P_DEPTH)), t.dim, 8.5, false);
        // Focus handles on their baseline.
        let base = Self::focus_base(cx);
        kit::dashed_h(s, plot.0, plot.0 + plot.2, base, 2.0, 4.0, t.grid_strong, 1.0);
        for (at, r) in Self::focus_points(cx) {
            let amount = cx.v(focus_param(r, 0));
            let c = if amount.abs() < 0.5 { t.dim } else { t.text };
            s.line(Point::new(at.x, base), Point::new(at.x, at.y), c, 1.5);
            s.fill_ellipse_solid(Scene::ellipse_at(at, 5.0), c);
            s.text(Point::new(at.x + 10.0, at.y), format!("{}", r + 1), c, 8.0, true);
        }
        if delta {
            let a = 0.6 + 0.4 * self.pulse.sin().abs();
            s.text(Point::new(plot.0 + plot.2 * 0.5, axis.top() + 76.0), "LISTENING TO THE REMOVED PART", t.accent.with_alpha(a), 10.0, true);
        }
        s.pop_clip();
        // Meter, max box, header switches, the key chip.
        let m = Self::meter(cx);
        kit::gr_meter(s, t, m, live.gr, self.hold.0, SCALES[self.scale]);
        let mb = Self::max_box(cx);
        kit::inset(s, t, mb, 4.0);
        s.text(kit::rect(mb).center(), format!("{:.1}", -self.max.0 + 0.0), t.text, 9.5, true);
        g::paint_switch(s, t, Self::show_rect(cx), SHOW_KEYS, show, true);
        g::paint_switch(s, t, Self::scale_rect(cx), SCALE_KEYS, self.scale, true);
        let key_on = cx.key_present && live.key.0;
        g::paint_key_chip(s, t, Self::key_chip(cx), key_on, live.key.1, cx.on(P_SIDECHAIN));
        if !cx.more {
            let at = if self.max.0 > 0.05 { format!("at {}", g::hz_text(self.max.1)) } else { "no cut yet".into() };
            g::paint_readout(s, t, READOUT, &[(format!("MAX {:.1} dB", -self.max.0 + 0.0), t.text, 13.0), (at, t.dim, 9.5)]);
            g::paint_key_meter(s, t, KEY_METER, key_on, live.key.1);
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let mut r = vec![cx.zones.picture, Self::key_chip(cx), Self::show_rect(cx), Self::scale_rect(cx)];
        if !cx.more {
            r.extend([READOUT, KEY_METER]);
        }
        r
    }

    fn wants_audio(&self) -> bool {
        true
    }

    fn view_merge(&self) -> &'static [Merge] {
        &MERGE
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        if let Some(audio) = live.audio {
            let listen = if self.show(cx) == 1 { g::Listen::Mid } else { g::Listen::Both };
            self.spectra.update(audio, listen, cx.on(P_SIDECHAIN), live.sample_rate, live.dt);
        }
        if live.gr >= self.hold.0 {
            self.hold = (live.gr, 0.0);
        } else {
            self.hold.1 += live.dt;
            if self.hold.1 > HOLD_S {
                self.hold = (live.gr, 0.0);
            }
        }
        let deepest = live.view.get(view::DEEPEST_DB).copied().unwrap_or(0.0);
        if deepest > self.max.0 {
            self.max = (deepest, live.view.get(view::DEEPEST_HZ).copied().unwrap_or(0.0));
        }
        self.pulse += live.dt * 2.0;
        true
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        self.handle(ev, cx, io)
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.on(P_DELTA)
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let axis = Self::axis(cx);
        let mut tips = vec![
            g::tip(Self::lane(cx), "The cut per frequency. Drag up or down here for DEPTH"),
            g::tip(Self::key_chip(cx), "Key level. Click: detect from the key, or from the input again"),
            g::tip(Self::show_rect(cx), "Show left/right or mid/side"),
            g::tip(Self::scale_rect(cx), "Scale of the cut lane and the meter"),
            g::tip(Self::max_box(cx), "Deepest cut so far: click to reset"),
            g::tip((axis.x0(), Self::floor_y(cx) - 4.0, axis.plot.2, 8.0), "Floor: drag up or down"),
        ];
        for (at, r) in Self::focus_points(cx) {
            tips.push(g::tip((at.x - 6.0, at.y - 6.0, 12.0, 12.0), format!("Focus {}: drag sideways for frequency, up or down for amount; wheel for Q; double-click for 0", r + 1)));
        }
        tips
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chip_reads_the_key_switch_and_delta() {
        let mut v: Vec<f32> = reso_suppress::PARAMS.iter().map(|p| p.default).collect();
        assert_eq!(chip_flags(&v), (false, false));
        v[P_SIDECHAIN] = 1.0;
        v[P_DELTA] = 1.0;
        assert_eq!(chip_flags(&v), (true, true));
        assert_eq!(width_text(10.0), "ALL");
        assert_eq!(reso_suppress::PARAMS[focus_param(3, 2)].id, "focus4Q");
    }
}
