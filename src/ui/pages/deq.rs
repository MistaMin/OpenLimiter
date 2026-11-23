//! The Dynamic EQ page (`eq-dynamic`, `design/tone.md` section 2): the static curve (dashed)
//! and the live curve with the area between them filled, a node per band with its `warm`
//! range bracket and a live ring, the detector and relative bells, 32 band pills with the
//! live gain under them, the band's filter and dynamics rows, and a MORE face for its
//! detector. Bands whose detector listens to the external key carry a "K".

use std::cell::RefCell;

use olm_api::Module;
use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use eq_dynamic::filter_types::{self as ft, FilterType};
use eq_dynamic::{band_param, DynamicEq, *};

use super::eqgraph::{self as g, Axis, NodeDrag, PillState, Spectra, Tap};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::ui::raster::R;

const BANDS: usize = 32;
const TYPE_BOX: R = (184.0, 656.0, 180.0, 24.0);
const DET_METER: R = (1008.0, 656.0, 14.0, 60.0);
const READOUT: R = (1028.0, 656.0, 52.0, 60.0);
const KEY_METER: R = (792.0, 700.0, 160.0, 30.0);
const SOURCE_KEYS: &[&str] = &["BAND", "L", "R", "M", "S", "EXT"];
const ACTION_KEYS: &[&str] = &["ABOVE", "BELOW"];
/// Detector sources: index of External.
const EXTERNAL: usize = 5;
/// The detector meter's scale, dB.
const METER_LO: f32 = -60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Node(NodeDrag),
    /// A range bracket end: the range parameter (host) and the band's static gain.
    Range(usize, f32),
    /// A bell moved sideways: its frequency parameter (host).
    Bell(usize),
    /// The threshold caret of the detector meter.
    Thresh(usize),
}

struct Curves {
    key: (Vec<f32>, [u32; 4]),
    freqs: Vec<f32>,
    total: Vec<f32>,
    bands: Vec<Option<Vec<f32>>>,
}

struct Deq {
    band: usize,
    tap: Tap,
    range: usize,
    picking: bool,
    drag: Option<Drag>,
    spectra: Spectra,
    clip: Option<Vec<f32>>,
    probe: RefCell<DynamicEq>,
    curves: RefCell<Option<Curves>>,
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    debug_assert_eq!(count, NPARAMS);
    let _ = count;
    Box::new(Deq { band: 0, tap: Tap::Post, range: 1, picking: false, drag: None, spectra: Spectra::default(), clip: None, probe: RefCell::new(DynamicEq::default()), curves: RefCell::new(None) })
}

/// Band `b` is on, dynamic, and its detector (or its relative detector) reads the key.
fn keyed_in(v: &dyn Fn(usize) -> f32, b: usize) -> bool {
    let f = |field| v(band_param(b, field));
    f(F_ON) >= 0.5 && f(F_DYN) >= 0.5 && (f(F_SOURCE).round() as usize == EXTERNAL || (f(F_REL_SOURCE).round() as usize == EXTERNAL && f(F_RELATIVE) > 0.0))
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    if values.len() < NPARAMS {
        return (false, false);
    }
    let v = |i: usize| values[i];
    ((0..BANDS).any(|b| keyed_in(&v, b)), false)
}

fn bp(b: usize, f: usize) -> usize {
    band_param(b, f)
}

fn type_of(cx: &Ctx, b: usize) -> FilterType {
    FilterType::from_index(cx.choice(bp(b, F_TYPE)))
}

fn band_on(cx: &Ctx, b: usize) -> bool {
    cx.on(bp(b, F_ON))
}

fn any_solo(cx: &Ctx) -> bool {
    (0..BANDS).any(|b| band_on(cx, b) && cx.on(bp(b, F_SOLO)))
}

fn heard(cx: &Ctx, b: usize, solo: bool) -> bool {
    band_on(cx, b) && (!solo || cx.on(bp(b, F_SOLO)))
}

fn dynamic(cx: &Ctx, b: usize) -> bool {
    band_on(cx, b) && cx.on(bp(b, F_DYN)) && ft::supports_dynamics(type_of(cx, b))
}

fn keyed(cx: &Ctx, b: usize) -> bool {
    let v = |i: usize| cx.v(i);
    keyed_in(&v, b)
}

fn ratio_text(v: f32) -> String {
    format!("{v:.0} %")
}

/// The ends of a band's range bracket: `(range parameter, signed range)`; the single action
/// range while neither Above nor Below is on.
fn bracket_ends(cx: &Ctx, b: usize) -> Vec<(usize, f32)> {
    let mut ends = Vec::new();
    if cx.on(bp(b, F_ABOVE_ON)) {
        ends.push((bp(b, F_ABOVE_RANGE), cx.v(bp(b, F_ABOVE_RANGE))));
    }
    if cx.on(bp(b, F_BELOW_ON)) {
        ends.push((bp(b, F_BELOW_RANGE), cx.v(bp(b, F_BELOW_RANGE))));
    }
    if ends.is_empty() {
        ends.push((bp(b, F_RANGE), cx.v(bp(b, F_RANGE))));
    }
    ends
}

fn view_of(live: &Live, slot: usize, b: usize, rest: f32) -> f32 {
    live.view.get(slot + b).copied().unwrap_or(rest)
}

impl Deq {
    fn axis(&self, cx: &Ctx) -> Axis {
        Axis::new(g::graph_rect(&cx.zones, 32.0), g::RANGES[self.range])
    }

    fn tap_rect(cx: &Ctx) -> R {
        (708.0, cx.zones.header.1 + 8.0, 176.0, 24.0)
    }

    fn key_chip(cx: &Ctx) -> R {
        (560.0, cx.zones.header.1 + 9.0, 140.0, 22.0)
    }

    fn node(&self, cx: &Ctx, axis: &Axis, b: usize) -> Point {
        let db = if ft::uses_gain(type_of(cx, b)) { cx.v(bp(b, F_GAIN)) } else { 0.0 };
        Point::new(axis.x(cx.v(bp(b, F_FREQ))).clamp(axis.x0(), axis.x1()), axis.y_in(db))
    }

    fn nodes(&self, cx: &Ctx, axis: &Axis) -> Vec<Option<Point>> {
        (0..BANDS).map(|b| (band_on(cx, b) || b == self.band).then(|| self.node(cx, axis, b))).collect()
    }

    /// Bracket end points of every dynamic band: `(point, range param, band gain)`.
    fn bracket_points(&self, cx: &Ctx, axis: &Axis) -> Vec<(Point, usize, f32)> {
        let mut out = Vec::new();
        for b in (0..BANDS).filter(|&b| dynamic(cx, b)) {
            let at = self.node(cx, axis, b);
            let gain = cx.v(bp(b, F_GAIN));
            for (p, r) in bracket_ends(cx, b) {
                out.push((Point::new(at.x, axis.y_in(gain + r)), p, gain));
            }
        }
        out
    }

    /// The selected band's detector bell (with LINK off) and relative bell (with FREE on).
    fn bells(&self, cx: &Ctx, axis: &Axis) -> [Option<(Point, usize, usize)>; 2] {
        let b = self.band;
        if !dynamic(cx, b) {
            return [None, None];
        }
        let y = axis.y(0.0);
        let det = (!cx.on(bp(b, F_DET_LINK))).then(|| (Point::new(axis.x(cx.v(bp(b, F_DET_FREQ))), y + 16.0), bp(b, F_DET_FREQ), bp(b, F_DET_Q)));
        let rel = cx.on(bp(b, F_REL_FREE)).then(|| (Point::new(axis.x(cx.v(bp(b, F_REL_FREQ))), y + 32.0), bp(b, F_REL_FREQ), bp(b, F_REL_Q)));
        [det, rel]
    }

    fn with_curves<T>(&self, cx: &Ctx, axis: &Axis, f: impl FnOnce(&Curves, &DynamicEq) -> T) -> T {
        let values: Vec<f32> = (0..cx.count).map(|i| cx.v(i)).collect();
        let key = (values, [axis.plot.0.to_bits(), axis.plot.1.to_bits(), axis.plot.2.to_bits(), axis.plot.3.to_bits()]);
        let mut cache = self.curves.borrow_mut();
        let mut probe = self.probe.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.key != key) {
            for (i, &v) in key.0.iter().enumerate() {
                probe.set_param(i, v);
            }
            let freqs = axis.freqs(g::CURVE_POINTS);
            let mut total = vec![0.0; freqs.len()];
            probe.response_db(&freqs, &mut total, false);
            let bands = (0..BANDS)
                .map(|b| {
                    band_on(cx, b).then(|| {
                        let mut out = vec![0.0; freqs.len()];
                        probe.band_response_db(b, None, &freqs, &mut out);
                        out
                    })
                })
                .collect();
            *cache = Some(Curves { key, freqs, total, bands });
        }
        f(cache.as_ref().expect("built"), &probe)
    }

    fn band_cells(&self, cx: &Ctx, cells: &mut Vec<Cell>) {
        let b = self.band;
        let p = |f| cx.p(bp(b, f));
        let ty = type_of(cx, b);
        let auto = cx.on(bp(b, F_AUTO));
        let placement = cx.choice(bp(b, F_PLACEMENT));
        let n = b + 1;
        cells.push(Cell::pill(p(F_ON), "ON", (120.0, 656.0, 56.0, 22.0)).tip(format!("Band {n} on")));
        cells.push(Cell::pill(p(F_DYN), "DYN", (120.0, 684.0, 56.0, 22.0)).dim(!ft::supports_dynamics(ty)).tip("The band's gain follows its detector (gain types only)"));
        cells.push(Cell::grip(p(F_TYPE), TYPE_BOX).dim(true).tip("Band type: click for the list by family"));
        cells.push(Cell::dropdown(p(F_PLACEMENT), "PLACEMENT", (184.0, 684.0, 180.0, 24.0)).tip("Which channels the band works on"));
        let slope_label = if ty == FilterType::VtLowShelf { "ATTEN" } else { "SLOPE" };
        cells.push(Cell::knob(Size::Mini, p(F_FREQ), "FREQ", 380.0, 656.0).format(g::hz_short));
        cells.push(Cell::knob(Size::Mini, p(F_GAIN), "GAIN", 440.0, 656.0).format(g::gain_text).dim(!ft::uses_gain(ty)));
        cells.push(Cell::knob(Size::Mini, p(F_Q), "Q", 500.0, 656.0).format(g::q_text).dim(!ft::uses_q(ty)));
        cells.push(Cell::knob(Size::Mini, p(F_SLOPE), slope_label, 560.0, 656.0).format(g::pct_text).dim(!ft::uses_slope(ty)));
        cells.push(Cell::knob(Size::Mini, p(F_THRESH), "THRESH", 632.0, 656.0).format(g::db_text).dim(auto).tip("Detector threshold (AUTO sets it)"));
        cells.push(Cell::knob(Size::Mini, p(F_ATTACK), "ATTACK", 692.0, 656.0).format(g::ms_text).dim(auto));
        cells.push(Cell::knob(Size::Mini, p(F_RELEASE), "RELEASE", 752.0, 656.0).format(g::ms_text).dim(auto));
        cells.push(Cell::knob(Size::Mini, p(F_KNEE), "KNEE", 812.0, 656.0).format(g::db_text));
        cells.push(Cell::knob(Size::Mini, p(F_TRANSIENT), "ONSETS", 872.0, 656.0).format(g::pct_text).tip("Weight the detector toward onsets"));
        cells.push(Cell::pill(p(F_AUTO), "AUTO", (940.0, 656.0, 56.0, 22.0)).tip("Automatic threshold and timing"));
        let row = 738.0;
        let above = cx.on(bp(b, F_ABOVE_ON));
        let below = cx.on(bp(b, F_BELOW_ON));
        cells.push(Cell::pill(p(F_ABOVE_ON), "ABOVE", (120.0, row, 64.0, 22.0)).tip("Act above the threshold"));
        cells.push(Cell::value(p(F_ABOVE_RANGE), "RANGE", (192.0, row, 64.0, 22.0)).format(g::gain_text).dim(!above));
        cells.push(Cell::value(p(F_ABOVE_RATIO), "RATIO", (264.0, row, 64.0, 22.0)).format(ratio_text).dim(!above));
        cells.push(Cell::pill(p(F_BELOW_ON), "BELOW", (352.0, row, 64.0, 22.0)).tip("Act below the threshold"));
        cells.push(Cell::value(p(F_BELOW_RANGE), "RANGE", (424.0, row, 64.0, 22.0)).format(g::gain_text).dim(!below));
        cells.push(Cell::value(p(F_BELOW_RATIO), "RATIO", (496.0, row, 64.0, 22.0)).format(ratio_text).dim(!below));
        cells.push(Cell::segmented(p(F_SOURCE), "SOURCE", (584.0, row - 1.0, 264.0, 24.0)).keys(SOURCE_KEYS).tip("Detector source: the band, L, R, M, S, or the external key"));
        cells.push(Cell::value(p(F_PAN), "PAN", (856.0, row, 64.0, 22.0)).dim(placement != 0 && placement != 5).tip("Stereo and Mid Side bands: where the band sits"));
        cells.push(Cell::pill(p(F_SOLO), "SOLO", (928.0, row, 56.0, 22.0)).tip("Hear only the soloed bands"));
    }

    fn more_cells(&self, cx: &Ctx, cells: &mut Vec<Cell>) {
        let b = self.band;
        let p = |f| cx.p(bp(b, f));
        let link = cx.on(bp(b, F_DET_LINK));
        let free = cx.on(bp(b, F_REL_FREE));
        let single = !cx.on(bp(b, F_ABOVE_ON)) && !cx.on(bp(b, F_BELOW_ON));
        cells.push(Cell::pill(p(F_DET_LINK), "LINK", (120.0, 652.0, 56.0, 22.0)).tip("The detector follows the band's frequency and Q"));
        cells.push(Cell::knob(Size::Small, p(F_DET_FREQ), "DET FREQ", 192.0, 654.0).format(g::hz_text).dim(link));
        cells.push(Cell::knob(Size::Small, p(F_DET_Q), "DET Q", 272.0, 654.0).format(g::q_text).dim(link));
        cells.push(Cell::knob(Size::Small, p(F_LOOKAHEAD), "BAND AHEAD", 352.0, 654.0).format(g::ms_text).dim(cx.on(P_LOOKAHEAD_ON)).tip("This band's lookahead (the global one wins while on)"));
        cells.push(Cell::knob(Size::Small, p(F_RELATIVE), "RELATIVE", 464.0, 654.0).format(g::pct_text).tip("Blend toward a detector relative to another band"));
        cells.push(Cell::pill(p(F_REL_FREE), "FREE", (552.0, 652.0, 56.0, 22.0)).tip("The relative detector has its own frequency and Q"));
        cells.push(Cell::knob(Size::Small, p(F_REL_FREQ), "REL FREQ", 616.0, 654.0).format(g::hz_text).dim(!free));
        cells.push(Cell::knob(Size::Small, p(F_REL_Q), "REL Q", 696.0, 654.0).format(g::q_text).dim(!free));
        cells.push(Cell::dropdown(p(F_REL_SOURCE), "REL SOURCE", (792.0, 652.0, 160.0, 24.0)).tip("What the relative detector reads"));
        cells.push(Cell::segmented(p(F_ACTION), "ACTION", (960.0, 652.0, 120.0, 24.0)).keys(ACTION_KEYS).dim(!single).tip("Single action, while ABOVE and BELOW are off"));
        cells.push(Cell::value(p(F_RANGE), "RANGE", (1016.0, 700.0, 64.0, 22.0)).format(g::gain_text).dim(!single).tip("Single action range"));
    }

    fn pick_type(&mut self, ty: usize, cx: &Ctx, io: &mut Io) {
        let b = self.band;
        io.set(cx.p(bp(b, F_TYPE)), ty as f32);
        if let Some(s) = g::slope_on_pick(ty, cx.v(bp(b, F_SLOPE))) {
            io.set(cx.p(bp(b, F_SLOPE)), s);
        }
    }

    fn handle(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let axis = self.axis(cx);
        let mods = io.modifiers();
        match ev {
            Pointer::Down { x, y, clicks } => {
                if self.picking {
                    self.picking = false;
                    if let Some(ty) = g::picker_hit(g::picker_rect(&axis), x, y) {
                        self.pick_type(ty, cx, io);
                        return Response::REBUILD;
                    }
                    return Response::TAKEN;
                }
                if let Some(k) = g::switch_key(g::range_rect(&cx.zones), g::RANGES.len(), x, y) {
                    self.range = k;
                    return Response::REBUILD;
                }
                if let Some(k) = g::switch_key(Self::tap_rect(cx), Tap::LABELS_KEY.len(), x, y) {
                    self.tap = Tap::from_index(k);
                    self.spectra.clear();
                    return Response::TAKEN;
                }
                if kit::inside(Self::key_chip(cx), x, y) {
                    let src = cx.p(bp(self.band, F_SOURCE));
                    io.set(src, if cx.choice(bp(self.band, F_SOURCE)) == EXTERNAL { 0.0 } else { EXTERNAL as f32 });
                    return Response::REBUILD;
                }
                if let Some(b) = g::pill_at(x, y) {
                    let on = cx.p(bp(b, F_ON));
                    if mods.alt {
                        let dynp = cx.p(bp(b, F_DYN));
                        io.set(dynp, if cx.on(bp(b, F_DYN)) { 0.0 } else { 1.0 });
                    } else if !band_on(cx, b) {
                        io.set(on, 1.0);
                    }
                    self.band = b;
                    return Response::REBUILD;
                }
                if !cx.more {
                    if kit::inside(TYPE_BOX, x, y) {
                        self.picking = true;
                        return Response::TAKEN;
                    }
                    if kit::inside((DET_METER.0 - 4.0, DET_METER.1, DET_METER.2 + 8.0, DET_METER.3), x, y) {
                        let p = cx.p(bp(self.band, F_THRESH));
                        io.begin(p);
                        io.perform(p, Self::meter_db(y));
                        self.drag = Some(Drag::Thresh(p));
                        return Response::TAKEN;
                    }
                }
                if !kit::inside(axis.plot, x, y) {
                    return Response::NONE;
                }
                for (at, p, gain) in self.bracket_points(cx, &axis) {
                    if (at.x - x).abs() <= 8.0 && (at.y - y).abs() <= 5.0 {
                        let hp = cx.p(p);
                        if clicks >= 2 {
                            io.set(hp, g::default_of(hp));
                            return Response::TAKEN;
                        }
                        io.begin(hp);
                        self.drag = Some(Drag::Range(hp, gain));
                        return Response::TAKEN;
                    }
                }
                for (at, f, _) in self.bells(cx, &axis).into_iter().flatten() {
                    if (at.x - x).abs() <= 8.0 && (at.y - y).abs() <= 8.0 {
                        let hp = cx.p(f);
                        io.begin(hp);
                        self.drag = Some(Drag::Bell(hp));
                        return Response::TAKEN;
                    }
                }
                let nodes = self.nodes(cx, &axis);
                if let Some(b) = g::node_at(&nodes, x, y) {
                    let changed = b != self.band;
                    self.band = b;
                    let gain = ft::uses_gain(type_of(cx, b)).then(|| cx.p(bp(b, F_GAIN)));
                    if mods.alt {
                        io.set(cx.p(bp(b, F_ON)), if band_on(cx, b) { 0.0 } else { 1.0 });
                    } else if clicks >= 2 {
                        if let Some(gp) = gain {
                            io.set(gp, 0.0);
                        }
                    } else {
                        let at = nodes[b].unwrap_or(Point::new(x, y));
                        let d = NodeDrag { freq: Some(cx.p(bp(b, F_FREQ))), gain, offset: (x - at.x, y - at.y), lock: mods.command, from: (x, y) }.begin(io);
                        self.drag = Some(Drag::Node(d));
                    }
                    return if changed { Response::REBUILD } else { Response::TAKEN };
                }
                if clicks >= 2 {
                    if let Some(b) = (0..BANDS).find(|&b| !band_on(cx, b)) {
                        io.set(cx.p(bp(b, F_TYPE)), 0.0);
                        io.set(cx.p(bp(b, F_FREQ)), axis.freq(x));
                        io.set(cx.p(bp(b, F_GAIN)), axis.db(y).clamp(-30.0, 30.0));
                        io.set(cx.p(bp(b, F_ON)), 1.0);
                        self.band = b;
                        return Response::REBUILD;
                    }
                }
                Response::TAKEN
            }
            Pointer::Move { x, y } => match self.drag {
                Some(Drag::Node(d)) => {
                    d.update(&axis, io, x, y);
                    Response::TAKEN
                }
                Some(Drag::Range(p, gain)) => {
                    io.perform(p, (axis.db(y) - gain).clamp(-30.0, 30.0));
                    Response::TAKEN
                }
                Some(Drag::Bell(p)) => {
                    io.perform(p, axis.freq(x));
                    Response::TAKEN
                }
                Some(Drag::Thresh(p)) => {
                    io.perform(p, Self::meter_db(y));
                    Response::TAKEN
                }
                None => Response::NONE,
            },
            Pointer::Up { .. } => {
                match self.drag.take() {
                    Some(Drag::Node(d)) => d.end(io),
                    Some(Drag::Range(p, _) | Drag::Bell(p) | Drag::Thresh(p)) => io.end(p),
                    None => return Response::NONE,
                }
                Response::TAKEN
            }
            Pointer::Wheel { x, y, dy } => {
                let up = dy > 0.0;
                for (at, _, q) in self.bells(cx, &axis).into_iter().flatten() {
                    if (at.x - x).abs() <= 8.0 && (at.y - y).abs() <= 8.0 {
                        g::nudge_log(io, cx.p(q), if up { 0.125 } else { -0.125 });
                        return Response::TAKEN;
                    }
                }
                let nodes = self.nodes(cx, &axis);
                let Some(b) = g::node_at(&nodes, x, y) else { return Response::NONE };
                let ty = type_of(cx, b);
                if (mods.command || !ft::uses_q(ty)) && ft::uses_slope(ty) {
                    g::nudge(io, cx.p(bp(b, F_SLOPE)), if up { 5.0 } else { -5.0 });
                } else if ft::uses_q(ty) {
                    g::nudge_log(io, cx.p(bp(b, F_Q)), if up { 0.125 } else { -0.125 });
                }
                Response::TAKEN
            }
        }
    }

    fn meter_db(y: f32) -> f32 {
        let r = DET_METER;
        (METER_LO * ((y - r.1) / r.3).clamp(0.0, 1.0)).clamp(-60.0, 0.0)
    }

    fn meter_y(db: f32) -> f32 {
        DET_METER.1 + (db / METER_LO).clamp(0.0, 1.0) * DET_METER.3
    }
}

impl Page for Deq {
    fn tall(&self) -> bool {
        true
    }

    fn has_more(&self) -> bool {
        true
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        local == P_LOOKAHEAD_ON || (local < BANDS * PER_BAND && matches!(local % PER_BAND, F_TYPE | F_PLACEMENT | F_AUTO | F_ABOVE_ON | F_BELOW_ON | F_DET_LINK | F_REL_FREE))
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let y = cx.zones.header.1;
        let mut cells = vec![
            Cell::pill(cx.p(P_LOOKAHEAD_ON), "AHEAD", (420.0, y + 9.0, 64.0, 22.0)).tip("Global lookahead for every dynamic band (20 ms latency)"),
            Cell::value(cx.p(P_LOOKAHEAD_MS), "AHEAD", (488.0, y + 9.0, 64.0, 22.0)).format(g::ms_text).dim(!cx.on(P_LOOKAHEAD_ON)).tip("Global lookahead time"),
        ];
        if cx.more {
            self.more_cells(cx, &mut cells);
        } else {
            self.band_cells(cx, &mut cells);
        }
        cells
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        self.axis(cx).paint_grid(s, t);
        if cx.more {
            g::captions(s, t, &[(960.0, 690.0, "SINGLE ACTION"), (960.0, 711.0, "RANGE")]);
        } else {
            g::captions(s, t, &[(192.0, 728.0, "RANGE"), (264.0, 728.0, "RATIO"), (424.0, 728.0, "RANGE"), (496.0, 728.0, "RATIO"), (584.0, 728.0, "DETECTOR SOURCE"), (856.0, 728.0, "PAN")]);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let axis = self.axis(cx);
        let solo = any_solo(cx);
        let dyn_gain = |b: usize| view_of(live, view::GAIN_DB, b, 0.0);
        g::clip(s, &axis);
        g::paint_spectra(s, t, &axis, &self.spectra, self.tap);
        self.with_curves(cx, &axis, |c, probe| {
            for (b, curve) in c.bands.iter().enumerate() {
                let Some(curve) = curve else { continue };
                let alpha = if b == self.band { g::FILL_SELECTED } else { g::FILL };
                let alpha = if heard(cx, b, solo) { alpha } else { alpha * 0.4 };
                g::fill_to_zero(s, &axis, &axis.points(&c.freqs, curve), t.band(b), alpha);
            }
            // The live curve: the static one plus each dynamic band's change.
            let mut live_total = c.total.clone();
            let mut moved = false;
            let mut tmp = vec![0.0; c.freqs.len()];
            for b in (0..BANDS).filter(|&b| dynamic(cx, b) && heard(cx, b, solo)) {
                let gdb = dyn_gain(b);
                let Some(Some(stat)) = c.bands.get(b) else { continue };
                if gdb.abs() < 0.01 {
                    continue;
                }
                probe.band_response_db(b, Some(cx.v(bp(b, F_GAIN)) + gdb), &c.freqs, &mut tmp);
                for ((o, n), st) in live_total.iter_mut().zip(&tmp).zip(stat) {
                    *o += n - st;
                }
                moved = true;
            }
            let stat_pts = axis.points(&c.freqs, &c.total);
            if moved {
                let live_pts = axis.points(&c.freqs, &live_total);
                g::fill_signed(s, &live_pts, &stat_pts, t.accent.with_alpha(0.25), t.band(self.band).with_alpha(g::FILL));
                kit::dashed(s, &stat_pts, 5.0, 4.0, t.dim, 1.0);
                kit::curve(s, live_pts, t.text, 2.0);
            } else {
                kit::curve(s, stat_pts, t.text, 2.0);
            }
        });
        g::unclip(s);
        // Brackets, then rings, then the nodes.
        for (at, _, _) in self.bracket_points(cx, &axis) {
            s.line(Point::new(at.x - 6.0, at.y), Point::new(at.x + 6.0, at.y), t.warm, 2.0);
        }
        for b in (0..BANDS).filter(|&b| dynamic(cx, b)) {
            let at = self.node(cx, &axis, b);
            for (_, r) in bracket_ends(cx, b) {
                let end = axis.y_in(cx.v(bp(b, F_GAIN)) + r);
                s.line(Point::new(at.x, at.y), Point::new(at.x, end), t.warm.with_alpha(0.6), 1.0);
            }
            let ring = Point::new(at.x, axis.y_in(cx.v(bp(b, F_GAIN)) + dyn_gain(b)));
            s.stroke_ellipse(Scene::ellipse_at(ring, 9.0), t.text.with_alpha(0.8), 1.4);
        }
        let key_heard = cx.key_present && live.key.0 && live.key.1 > -90.0;
        for (b, at) in self.nodes(cx, &axis).iter().enumerate() {
            let Some(at) = at else { continue };
            let badge = g::PLACEMENT_BADGE[cx.choice(bp(b, F_PLACEMENT)).min(5)];
            g::paint_node(s, t, *at, t.band(b), &format!("{}", b + 1), b == self.band, !heard(cx, b, solo), badge);
            if keyed(cx, b) {
                g::paint_key_badge(s, t, Point::new(at.x - 12.0, at.y - 12.0), key_heard);
            }
        }
        let [det, rel] = self.bells(cx, &axis);
        if let Some((at, _, _)) = det {
            s.line(Point::new(at.x, axis.top()), Point::new(at.x, axis.bottom()), t.warm.with_alpha(0.35), 1.0);
            s.stroke_ellipse(Scene::ellipse_at(at, 6.0), t.warm, 1.6);
            s.text(Point::new(at.x + 14.0, at.y), "DET", t.warm, 8.0, true);
        }
        if let Some((at, _, _)) = rel {
            kit::dashed(s, &[Point::new(at.x, axis.top()), Point::new(at.x, axis.bottom())], 2.0, 3.0, t.warm.with_alpha(0.5), 1.0);
            s.stroke_ellipse(Scene::ellipse_at(at, 6.0), t.warm.with_alpha(0.7), 1.0);
            s.text(Point::new(at.x + 14.0, at.y), "REL", t.warm, 8.0, true);
        }
        if let Some(b) = (0..BANDS).find(|&b| keyed(cx, b) && !key_heard) {
            s.text(Point::new(axis.x0() + 8.0, axis.top() + 12.0), format!("Band {} waits for a key signal", b + 1), t.warm, 9.5, false);
        }
        g::paint_switch(s, t, g::range_rect(&cx.zones), g::RANGE_KEYS, self.range, true);
        g::paint_switch(s, t, Self::tap_rect(cx), Tap::LABELS_KEY, self.tap.index(), true);
        g::paint_key_chip(s, t, Self::key_chip(cx), cx.key_present && live.key.0, live.key.1, (0..BANDS).any(|b| keyed(cx, b)));
        if self.picking {
            g::paint_picker(s, t, g::picker_rect(&axis), TYPES, cx.choice(bp(self.band, F_TYPE)), &format!("BAND {} TYPE", self.band + 1));
        }
        for b in 0..BANDS {
            let badge = g::PLACEMENT_BADGE[cx.choice(bp(b, F_PLACEMENT)).min(5)];
            let bar = dynamic(cx, b).then(|| {
                let span = bracket_ends(cx, b).iter().fold(0.0f32, |m, (_, r)| m.max(r.abs())).max(0.5);
                dyn_gain(b) / span
            });
            g::paint_pill(s, t, b, PillState { used: band_on(cx, b), selected: b == self.band, badge, keyed: keyed(cx, b), bar });
        }
        let b = self.band;
        if cx.more {
            g::paint_key_meter(s, t, KEY_METER, cx.key_present && live.key.0, live.key.1);
            return;
        }
        let ty = type_of(cx, b);
        g::paint_box(s, t, TYPE_BOX, ty.name(), false);
        // Detector meter: envelope bar and the threshold caret.
        let env = view_of(live, view::ENVELOPE_DB, b, -100.0);
        let thr = if cx.on(bp(b, F_AUTO)) { view_of(live, view::THRESHOLD_DB, b, cx.v(bp(b, F_THRESH))) } else { cx.v(bp(b, F_THRESH)) };
        let m = DET_METER;
        s.fill_round(kit::rect(m), 3.0, t.seg_off, t.seg_off, true);
        let top = Self::meter_y(env);
        if top < m.1 + m.3 - 0.5 {
            s.fill_round(Rect::new(m.0, top, m.2, m.1 + m.3 - top), 3.0, t.warm, t.warm, true);
        }
        let cy = Self::meter_y(thr);
        s.fill_poly(vec![Point::new(m.0 - 4.0, cy - 4.0), Point::new(m.0 + 2.0, cy), Point::new(m.0 - 4.0, cy + 4.0)], t.text, t.text);
        s.line(Point::new(m.0, cy), Point::new(m.0 + m.2, cy), t.text, 1.5);
        let g_now = dyn_gain(b);
        let show = |v: f32| if v <= -99.0 { "--".to_string() } else { format!("{v:.1}") };
        g::paint_readout(s, t, READOUT, &[(format!("E {}", show(env)), t.warm, 9.0), (format!("T {}", show(thr)), t.dim, 9.0), (format!("G {g_now:+.1}"), if g_now < 0.0 { t.accent } else { t.text }, 9.0)]);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let mut r = vec![cx.zones.picture, Self::tap_rect(cx), Self::key_chip(cx), g::PILL_STRIP];
        if cx.more {
            r.push(KEY_METER);
        } else {
            r.extend([TYPE_BOX, (DET_METER.0 - 5.0, DET_METER.1 - 5.0, DET_METER.2 + 10.0, DET_METER.3 + 10.0), READOUT]);
        }
        r
    }

    fn wants_audio(&self) -> bool {
        self.tap != Tap::Off
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        if let (Some(audio), true) = (live.audio, self.tap != Tap::Off) {
            self.spectra.update(audio, g::Listen::Both, self.tap == Tap::Key, live.sample_rate, live.dt);
        }
        let _ = cx;
        // The live curve, rings, pill bars, meters and the key chip move with the audio.
        true
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        self.handle(ev, cx, io)
    }

    fn states(&self) -> usize {
        BANDS
    }

    fn set_state(&mut self, i: usize) {
        self.band = i % BANDS;
        self.picking = false;
    }

    fn menu(&self, _cx: &Ctx) -> Vec<String> {
        let n = self.band + 1;
        vec![
            format!("Band {n} → External key"),
            format!("Band {n} → Follow band"),
            "All dynamic bands → External key".into(),
            "All dynamic bands → Follow band".into(),
            "Show key spectrum".into(),
            format!("Band {n}: gain to 0 dB"),
            format!("Band {n}: copy"),
            format!("Band {n}: paste"),
            format!("Band {n}: delete"),
        ]
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        let b = self.band;
        let p = |f| cx.p(bp(b, f));
        match item {
            0 => io.set(p(F_SOURCE), EXTERNAL as f32),
            1 => io.set(p(F_SOURCE), 0.0),
            2 | 3 => {
                for k in (0..BANDS).filter(|&k| cx.on(bp(k, F_DYN))) {
                    io.set(cx.p(bp(k, F_SOURCE)), if item == 2 { EXTERNAL as f32 } else { 0.0 });
                }
            }
            4 => {
                self.tap = Tap::Key;
                self.spectra.clear();
                return Response::TAKEN;
            }
            5 => io.set(p(F_GAIN), 0.0),
            6 => {
                self.clip = Some((0..PER_BAND).map(|f| cx.v(bp(b, f))).collect());
                return Response::NONE;
            }
            7 => {
                let Some(clip) = self.clip.clone() else { return Response::NONE };
                for (f, v) in clip.into_iter().enumerate() {
                    io.set(p(f), v);
                }
            }
            8 => {
                for f in 0..PER_BAND {
                    io.set(p(f), g::default_of(p(f)));
                }
                io.set(p(F_ON), 0.0);
            }
            _ => return Response::NONE,
        }
        Response::REBUILD
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let axis = self.axis(cx);
        let mut tips = vec![
            g::tip(g::range_rect(&cx.zones), "Graph range"),
            g::tip(Self::tap_rect(cx), "Spectrum: off, input, input and output, or the key over the input"),
            g::tip(Self::key_chip(cx), "Key level. Click: the selected band's detector reads the key, or the band again"),
        ];
        for (b, at) in self.nodes(cx, &axis).iter().enumerate() {
            let Some(at) = at else { continue };
            let r = (at.x - 7.0, at.y - 7.0, 14.0, 14.0);
            tips.push(g::tip(r, format!("Band {} {}: drag; wheel for Q; double-click for 0 dB; Alt-click on/off", b + 1, type_of(cx, b).name())));
        }
        for (at, _, _) in self.bracket_points(cx, &axis) {
            tips.push(g::tip((at.x - 8.0, at.y - 5.0, 16.0, 10.0), "Range: drag up or down; double-click resets"));
        }
        for b in 0..BANDS {
            tips.push(g::tip(g::pill_rect(b), format!("Band {}: click to pick (an empty band is created); Alt-click: dynamics on/off", b + 1)));
        }
        if !cx.more {
            tips.push(g::tip(DET_METER, "Detector level and threshold: drag the caret"));
        }
        tips
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_bands_light_the_chip() {
        let mut v: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        assert_eq!(chip_flags(&v), (false, false));
        v[band_param(2, F_ON)] = 1.0;
        v[band_param(2, F_DYN)] = 1.0;
        v[band_param(2, F_SOURCE)] = EXTERNAL as f32;
        assert_eq!(chip_flags(&v), (true, false));
        v[band_param(2, F_DYN)] = 0.0;
        assert_eq!(chip_flags(&v), (false, false));
        assert_eq!(SOURCES[EXTERNAL], "External");
        assert_eq!(PARAMS[band_param(31, F_REL_SOURCE)].id, "b32RelSource");
    }
}
