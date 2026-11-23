//! The Parametric EQ page (`eq-parametric`, `design/tone.md` section 1): one composite
//! response over the spectrum with a node per band, 32 band pills, the selected band's row,
//! the global trims, and a MORE face for the split engine and the kernel processing.

use std::cell::RefCell;

use olm_api::Module;
use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use eq_parametric::filter_types::{self as ft, FilterType};
use eq_parametric::{band_param, ParametricEq, *};

use super::eqgraph::{self as g, Axis, NodeDrag, PillState, Spectra, Tap};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::ui::raster::R;

const BANDS: usize = 32;
const TYPE_BOX: R = (184.0, 656.0, 180.0, 24.0);
const READOUT: R = (944.0, 656.0, 136.0, 60.0);
const BALANCE: R = (120.0, 744.0, 448.0, 16.0);
const SPLIT_KEYS: &[&str] = &["ALL", "TRANSIENT", "TONAL"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Node(NodeDrag),
    /// A split handle: the host index of the stream gain it moves.
    Split(usize),
}

struct Curves {
    key: (Vec<f32>, [u32; 4]),
    freqs: Vec<f32>,
    total: Vec<f32>,
    bands: Vec<Option<Vec<f32>>>,
    /// Left and right of the placement matrix, when a band is not Stereo.
    chans: Option<[Vec<f32>; 2]>,
}

struct Peq {
    band: usize,
    tap: Tap,
    range: usize,
    picking: bool,
    drag: Option<Drag>,
    spectra: Spectra,
    clip: Option<Vec<f32>>,
    probe: RefCell<ParametricEq>,
    curves: RefCell<Option<Curves>>,
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    debug_assert_eq!(count, NPARAMS);
    let _ = count;
    Box::new(Peq { band: 0, tap: Tap::Post, range: 1, picking: false, drag: None, spectra: Spectra::default(), clip: None, probe: RefCell::new(ParametricEq::default()), curves: RefCell::new(None) })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip: a split
/// stream left soloed is a listen mode.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (false, values.get(P_SPLIT_SOLO).is_some_and(|&v| v >= 0.5))
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

impl Peq {
    fn axis(&self, cx: &Ctx) -> Axis {
        Axis::new(g::graph_rect(&cx.zones, 32.0), g::RANGES[self.range])
    }

    fn tap_rect(cx: &Ctx) -> R {
        (768.0, cx.zones.header.1 + 8.0, 132.0, 24.0)
    }

    fn mode(cx: &Ctx) -> usize {
        cx.choice(P_PROCESSING_MODE)
    }

    fn node(&self, cx: &Ctx, axis: &Axis, b: usize) -> Point {
        let ty = type_of(cx, b);
        let db = if ft::uses_gain(ty) { cx.v(bp(b, F_GAIN)) } else { 0.0 };
        Point::new(axis.x(cx.v(bp(b, F_FREQ))).clamp(axis.x0(), axis.x1()), axis.y_in(db))
    }

    /// Nodes of the bands that are on, and of the selected band.
    fn nodes(&self, cx: &Ctx, axis: &Axis) -> Vec<Option<Point>> {
        (0..BANDS).map(|b| (band_on(cx, b) || b == self.band).then(|| self.node(cx, axis, b))).collect()
    }

    /// The split handles of the selected band: transient (triangle) and tonal (square).
    fn split_handles(&self, cx: &Ctx, axis: &Axis) -> Option<[(Point, usize); 2]> {
        let b = self.band;
        if !cx.on(bp(b, F_SPLIT)) || !band_on(cx, b) {
            return None;
        }
        let x = self.node(cx, axis, b).x;
        Some([(Point::new(x - 9.0, axis.y_in(cx.v(bp(b, F_SPLIT_TRANS)))), cx.p(bp(b, F_SPLIT_TRANS))), (Point::new(x + 9.0, axis.y_in(cx.v(bp(b, F_SPLIT_TAIL)))), cx.p(bp(b, F_SPLIT_TAIL)))])
    }

    fn with_curves<T>(&self, cx: &Ctx, axis: &Axis, f: impl FnOnce(&Curves) -> T) -> T {
        let values: Vec<f32> = (0..cx.count).map(|i| cx.v(i)).collect();
        let key = (values, [axis.plot.0.to_bits(), axis.plot.1.to_bits(), axis.plot.2.to_bits(), axis.plot.3.to_bits()]);
        let mut cache = self.curves.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.key != key) {
            let mut probe = self.probe.borrow_mut();
            for (i, &v) in key.0.iter().enumerate() {
                probe.set_param(i, v);
            }
            let freqs = axis.freqs(g::CURVE_POINTS);
            let mut total = vec![0.0; freqs.len()];
            probe.response_db(&freqs, &mut total);
            let bands = (0..BANDS)
                .map(|b| {
                    band_on(cx, b).then(|| {
                        let mut out = vec![0.0; freqs.len()];
                        probe.band_response_db(b, None, &freqs, &mut out);
                        out
                    })
                })
                .collect();
            let chans = (0..BANDS).any(|b| band_on(cx, b) && cx.choice(bp(b, F_PLACEMENT)) != 0).then(|| {
                let (mut l, mut r) = (vec![0.0; freqs.len()], vec![0.0; freqs.len()]);
                probe.response_channels_db(&freqs, &mut l, &mut r);
                [l, r]
            });
            *cache = Some(Curves { key, freqs, total, bands, chans });
        }
        f(cache.as_ref().expect("built"))
    }

    fn notice(cx: &Ctx) -> Option<&'static str> {
        let mode = Self::mode(cx);
        let on = |b: usize| band_on(cx, b);
        if mode != 0 && (0..BANDS).any(|b| on(b) && cx.on(bp(b, F_SPLIT))) {
            Some("Split needs Zero Latency")
        } else if mode != 0 && mode != 4 && (0..BANDS).any(|b| on(b) && ft::is_all_pass(type_of(cx, b))) {
            Some("Allpass bands are not held by the kernel modes")
        } else {
            None
        }
    }

    fn band_cells(&self, cx: &Ctx, cells: &mut Vec<Cell>) {
        let b = self.band;
        let p = |f| cx.p(bp(b, f));
        let ty = type_of(cx, b);
        let mode = Self::mode(cx);
        let placement = cx.choice(bp(b, F_PLACEMENT));
        let split = cx.on(bp(b, F_SPLIT));
        let n = b + 1;
        cells.push(Cell::pill(p(F_ON), "ON", (120.0, 656.0, 56.0, 22.0)).tip(format!("Band {n} on")));
        cells.push(Cell::pill(p(F_SOLO), "SOLO", (120.0, 684.0, 56.0, 22.0)).tip("Hear only the soloed bands"));
        // The page draws the type box and opens its own grouped list, so a cut picked there
        // gets its 12 dB/oct slope; the hit-less grip keeps the parameter for the host.
        cells.push(Cell::grip(p(F_TYPE), TYPE_BOX).dim(true).tip("Band type: click for the list by family"));
        cells.push(Cell::dropdown(p(F_PLACEMENT), "PLACEMENT", (184.0, 684.0, 180.0, 24.0)).tip("Which channels the band works on"));
        let slope_label = if ty == FilterType::VtLowShelf { "ATTEN" } else { "SLOPE" };
        cells.push(Cell::knob(Size::Mini, p(F_FREQ), "FREQ", 380.0, 656.0).format(g::hz_short));
        cells.push(Cell::knob(Size::Mini, p(F_GAIN), "GAIN", 440.0, 656.0).format(g::gain_text).dim(!ft::uses_gain(ty)));
        cells.push(Cell::knob(Size::Mini, p(F_Q), "Q", 500.0, 656.0).format(g::q_text).dim(!ft::uses_q(ty)));
        cells.push(Cell::knob(Size::Mini, p(F_SLOPE), slope_label, 560.0, 656.0).format(g::pct_text).dim(!ft::uses_slope(ty)).tip("Filter order: 50 % is 12 dB/oct for a cut"));
        cells.push(Cell::knob(Size::Mini, p(F_PAN), "PAN", 620.0, 656.0).format(g::pct_text).dim(placement != 0 && placement != 5).tip("Stereo and Mid Side bands: where the band sits"));
        cells.push(Cell::knob(Size::Mini, p(F_PHASE), "PHASE", 680.0, 656.0).format(g::pct_text).dim(mode == 0).tip("Kernel modes: -100 inverse, 0 linear, +100 minimum phase"));
        cells.push(Cell::pill(p(F_SPLIT), "SPLIT", (752.0, 656.0, 56.0, 22.0)).tip("Treat the transient and tonal parts apart (Zero Latency)"));
        cells.push(Cell::knob(Size::Mini, p(F_SPLIT_TRANS), "TRANS", 816.0, 656.0).format(g::gain_text).dim(!split).tip("Gain of the band's transient part"));
        cells.push(Cell::knob(Size::Mini, p(F_SPLIT_TAIL), "TONAL", 876.0, 656.0).format(g::gain_text).dim(!split).tip("Gain of the band's tonal part"));
        let row = 746.0;
        for (k, (id, label)) in [(P_INPUT_GAIN, "TRIM"), (P_OUTPUT_GAIN, "OUTPUT"), (P_GAIN_SCALE, "SCALE"), (P_DRIVE, "DRIVE"), (P_STEREO_WIDTH, "WIDTH"), (P_OUTPUT_PAN, "BALANCE"), (P_SIDE_GAIN, "SIDE")].into_iter().enumerate() {
            cells.push(Cell::value(cx.p(id), label, (120.0 + 72.0 * k as f32, row, 64.0, 22.0)));
        }
        cells.push(Cell::pill(cx.p(P_AUTO_GAIN), "AUTO GAIN", (632.0, row, 88.0, 22.0)).tip("Keep the level about the same"));
        cells.push(Cell::pill(cx.p(P_POLARITY), "INVERT", (728.0, row, 64.0, 22.0)).tip("Polarity invert"));
        cells.push(Cell::pill(cx.p(P_BYPASS), "BYPASS", (800.0, row, 64.0, 22.0)).tip("Bypass with the latency kept"));
    }

    fn more_cells(cx: &Ctx, cells: &mut Vec<Cell>) {
        cells.push(Cell::dropdown(cx.p(P_SPLIT_SOURCE), "SOURCE", (120.0, 652.0, 160.0, 24.0)).tip("What the split listens for"));
        cells.push(Cell::segmented(cx.p(P_SPLIT_SOLO), "LISTEN", (120.0, 684.0, 192.0, 24.0)).keys(SPLIT_KEYS).tip("Hear both parts, or one alone"));
        cells.push(Cell::knob(Size::Small, cx.p(P_SPLIT_SEPARATION), "SEPARATION", 328.0, 648.0).format(g::pct_text));
        cells.push(Cell::knob(Size::Small, cx.p(P_SPLIT_DECAY), "DECAY", 408.0, 648.0).format(g::pct_text));
        cells.push(Cell::knob(Size::Small, cx.p(P_SPLIT_SMOOTHING), "SMOOTH", 488.0, 648.0).format(g::ms_text));
        cells.push(Cell::dropdown(cx.p(P_WINDOW_SHAPE), "WINDOW", (592.0, 652.0, 160.0, 24.0)).tip("FIR window of the kernel modes"));
        cells.push(Cell::value(cx.p(P_WINDOW_PARAMETER), "TIGHTEN", (840.0, 653.0, 64.0, 22.0)).tip("Window tightening"));
        cells.push(Cell::segmented(cx.p(P_DIGITAL_PLUS), "IIR PHASE", (592.0, 700.0, 220.0, 24.0)).tip("Digital+ phase steps"));
        cells.push(Cell::pill(cx.p(P_ULTRA_PRECISION), "117-BIT", (840.0, 701.0, 72.0, 22.0)).tip("Double-double recursion"));
        cells.push(Cell::pill(cx.p(P_OVERSAMPLE2X), "2X", (920.0, 701.0, 56.0, 22.0)).tip("Run the bands at twice the rate"));
    }

    fn handle(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let axis = self.axis(cx);
        let mods = io.modifiers();
        match ev {
            Pointer::Down { x, y, clicks } => {
                if self.picking {
                    self.picking = false;
                    if let Some(ty) = g::picker_hit(g::picker_rect(&axis), x, y) {
                        let b = self.band;
                        io.set(cx.p(bp(b, F_TYPE)), ty as f32);
                        if let Some(s) = g::slope_on_pick(ty, cx.v(bp(b, F_SLOPE))) {
                            io.set(cx.p(bp(b, F_SLOPE)), s);
                        }
                        return Response::REBUILD;
                    }
                    return Response::TAKEN;
                }
                if let Some(k) = g::switch_key(g::range_rect(&cx.zones), g::RANGES.len(), x, y) {
                    self.range = k;
                    return Response::REBUILD;
                }
                if let Some(k) = g::switch_key(Self::tap_rect(cx), Tap::LABELS.len(), x, y) {
                    self.tap = Tap::from_index(k);
                    self.spectra.clear();
                    return Response::TAKEN;
                }
                if !cx.more {
                    if let Some(b) = g::pill_at(x, y) {
                        let on = cx.p(bp(b, F_ON));
                        if mods.alt {
                            io.set(on, if band_on(cx, b) { 0.0 } else { 1.0 });
                        } else if !band_on(cx, b) {
                            io.set(on, 1.0);
                        }
                        self.band = b;
                        return Response::REBUILD;
                    }
                    if kit::inside(TYPE_BOX, x, y) {
                        self.picking = true;
                        return Response::TAKEN;
                    }
                }
                if !kit::inside(axis.plot, x, y) {
                    return Response::NONE;
                }
                if let Some(handles) = self.split_handles(cx, &axis) {
                    for (at, p) in handles {
                        if (at.x - x).abs() <= 7.0 && (at.y - y).abs() <= 7.0 {
                            if clicks >= 2 {
                                io.set(p, 0.0);
                                return Response::TAKEN;
                            }
                            io.begin(p);
                            self.drag = Some(Drag::Split(p));
                            return Response::TAKEN;
                        }
                    }
                }
                let nodes = self.nodes(cx, &axis);
                if let Some(b) = g::node_at(&nodes, x, y) {
                    let changed = b != self.band;
                    self.band = b;
                    let ty = type_of(cx, b);
                    let gain = ft::uses_gain(ty).then(|| cx.p(bp(b, F_GAIN)));
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
                    // Add a bell in the next free band.
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
                Some(Drag::Split(p)) => {
                    io.perform(p, axis.db(y).clamp(-30.0, 30.0));
                    Response::TAKEN
                }
                None => Response::NONE,
            },
            Pointer::Up { .. } => {
                match self.drag.take() {
                    Some(Drag::Node(d)) => d.end(io),
                    Some(Drag::Split(p)) => io.end(p),
                    None => return Response::NONE,
                }
                Response::TAKEN
            }
            Pointer::Wheel { x, y, dy } => {
                let nodes = self.nodes(cx, &axis);
                let Some(b) = g::node_at(&nodes, x, y) else { return Response::NONE };
                let ty = type_of(cx, b);
                let up = dy > 0.0;
                if (mods.command || !ft::uses_q(ty)) && ft::uses_slope(ty) {
                    g::nudge(io, cx.p(bp(b, F_SLOPE)), if up { 5.0 } else { -5.0 });
                } else if ft::uses_q(ty) {
                    g::nudge_log(io, cx.p(bp(b, F_Q)), if up { 0.125 } else { -0.125 });
                }
                Response::TAKEN
            }
        }
    }
}

impl Page for Peq {
    fn tall(&self) -> bool {
        true
    }

    fn has_more(&self) -> bool {
        true
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        local == P_PROCESSING_MODE || (local < BANDS * PER_BAND && matches!(local % PER_BAND, F_TYPE | F_PLACEMENT | F_SPLIT))
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let y = cx.zones.header.1;
        let mode = Self::mode(cx);
        let mut cells = vec![
            Cell::dropdown(cx.p(P_PROCESSING_MODE), "MODE", (420.0, y + 8.0, 140.0, 24.0)).tip("Phase mode: Zero Latency, Linear Phase, Analog, Mix, Analog ZL"),
            Cell::dropdown(cx.p(P_LINEAR_PHASE_QUALITY), "QUALITY", (568.0, y + 8.0, 96.0, 24.0)).dim(mode == 0 || mode == 4).tip("Kernel length (latency) of the kernel modes"),
            Cell::dropdown(cx.p(P_MIX_CROSSOVER), "XOVER", (672.0, y + 8.0, 88.0, 24.0)).dim(mode != 3).tip("Mix mode: Analog below, Linear above this frequency"),
        ];
        if cx.more {
            Self::more_cells(cx, &mut cells);
        } else {
            self.band_cells(cx, &mut cells);
        }
        cells
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        self.axis(cx).paint_grid(s, t);
        if cx.more {
            kit::caption(s, t, 120.0, 634.0, "MORE · SPLIT ENGINE");
            kit::caption(s, t, 592.0, 634.0, "PROCESSING");
            g::captions(s, t, &[(840.0, 644.0, "TIGHTEN"), (592.0, 690.0, "IIR PHASE"), (120.0, 736.0, "TRANSIENT / TONAL ENERGY")]);
        } else {
            let labels = ["TRIM", "OUTPUT", "SCALE", "DRIVE", "WIDTH", "BALANCE", "SIDE"];
            for (k, l) in labels.iter().enumerate() {
                kit::caption(s, t, 120.0 + 72.0 * k as f32, 737.0, l);
            }
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let axis = self.axis(cx);
        let solo = any_solo(cx);
        g::clip(s, &axis);
        g::paint_spectra(s, t, &axis, &self.spectra, self.tap);
        let mut chan_labels: Vec<(Point, &str)> = Vec::new();
        self.with_curves(cx, &axis, |c| {
            for (b, curve) in c.bands.iter().enumerate() {
                let Some(curve) = curve else { continue };
                let alpha = if b == self.band { g::FILL_SELECTED } else { g::FILL };
                let alpha = if heard(cx, b, solo) { alpha } else { alpha * 0.4 };
                g::fill_to_zero(s, &axis, &axis.points(&c.freqs, curve), t.band(b), alpha);
            }
            if let Some([l, r]) = &c.chans {
                let (pl, pr) = (axis.points(&c.freqs, l), axis.points(&c.freqs, r));
                if let (Some(el), Some(er)) = (pl.last().copied(), pr.last().copied()) {
                    // Each letter on the far side of its line from the other one, so neither
                    // sits on a line; the upper one above, the lower one below.
                    let (dl, dr) = if el.y <= er.y { (-9.0, 9.0) } else { (9.0, -9.0) };
                    let y = |e: Point, d: f32| (e.y + d).clamp(axis.top() + 7.0, axis.bottom() - 7.0);
                    chan_labels.push((Point::new(el.x - 12.0, y(el, dl)), "L"));
                    chan_labels.push((Point::new(er.x - 12.0, y(er, dr)), "R"));
                }
                kit::dashed(s, &pl, 5.0, 4.0, t.dim, 1.0);
                kit::dashed(s, &pr, 5.0, 4.0, t.dim, 1.0);
            }
            kit::curve(s, axis.points(&c.freqs, &c.total), t.text, 2.0);
        });
        g::unclip(s);
        for (at, label) in chan_labels {
            s.text(at, label, t.dim, 9.0, true);
        }
        let nodes = self.nodes(cx, &axis);
        for (b, at) in nodes.iter().enumerate() {
            if let Some(at) = at {
                let badge = g::PLACEMENT_BADGE[cx.choice(bp(b, F_PLACEMENT)).min(5)];
                g::paint_node(s, t, *at, t.band(b), &format!("{}", b + 1), b == self.band, !heard(cx, b, solo), badge);
            }
        }
        if let Some([(tr, _), (tn, _)]) = self.split_handles(cx, &axis) {
            let c = t.band(self.band);
            s.line(Point::new(tr.x + 9.0, axis.top()), Point::new(tr.x + 9.0, axis.bottom()), c.with_alpha(0.35), 1.0);
            s.fill_poly(vec![Point::new(tr.x, tr.y - 6.0), Point::new(tr.x + 6.0, tr.y + 5.0), Point::new(tr.x - 6.0, tr.y + 5.0)], c, c);
            s.fill_round(Rect::new(tn.x - 5.0, tn.y - 5.0, 10.0, 10.0), 1.5, t.plot_bg, t.plot_bg, true);
            s.stroke_round(Rect::new(tn.x - 5.0, tn.y - 5.0, 10.0, 10.0), 1.5, c, 1.6);
        }
        if let Some(n) = Self::notice(cx) {
            s.text(Point::new(axis.x0() + 8.0, axis.top() + 12.0), n, t.warm, 9.5, false);
        }
        g::paint_switch(s, t, g::range_rect(&cx.zones), g::RANGE_KEYS, self.range, true);
        g::paint_switch(s, t, Self::tap_rect(cx), Tap::LABELS, self.tap.index(), true);
        if self.picking {
            g::paint_picker(s, t, g::picker_rect(&axis), TYPES, cx.choice(bp(self.band, F_TYPE)), &format!("BAND {} TYPE", self.band + 1));
        }
        if cx.more {
            let share = live.view.get(view::SPLIT_TRANSIENT_SHARE).copied().unwrap_or(0.5);
            let active = live.view.get(view::SPLIT_ACTIVE).copied().unwrap_or(0.0) > 0.5;
            let r = BALANCE;
            s.fill_round(kit::rect(r), 4.0, t.seg_off, t.seg_off, true);
            if active {
                let w = share.clamp(0.0, 1.0) * r.2;
                s.fill_round(Rect::new(r.0, r.1, w, r.3), 4.0, t.band(self.band), t.band(self.band), true);
                s.text(Point::new(r.0 + 8.0, r.1 + r.3 * 0.5), format!("TRANSIENT {:.0} %", share * 100.0), t.text, 8.5, false);
            } else {
                s.text(kit::rect(r).center(), "No band is split", t.dim, 8.5, true);
            }
            return;
        }
        for b in 0..BANDS {
            let badge = g::PLACEMENT_BADGE[cx.choice(bp(b, F_PLACEMENT)).min(5)];
            g::paint_pill(s, t, b, PillState { used: band_on(cx, b), selected: b == self.band, badge, keyed: false, bar: None });
        }
        let b = self.band;
        let ty = type_of(cx, b);
        g::paint_box(s, t, TYPE_BOX, ty.name(), false);
        let gain = if ft::uses_gain(ty) { g::gain_text(cx.v(bp(b, F_GAIN))) } else { String::new() };
        let q = if ft::uses_q(ty) { format!("Q {}", g::q_text(cx.v(bp(b, F_Q)))) } else { String::new() };
        let place = PLACEMENTS[cx.choice(bp(b, F_PLACEMENT)).min(5)];
        g::paint_readout(s, t, READOUT, &[(format!("{} {gain}", g::hz_text(cx.v(bp(b, F_FREQ)))), t.text, 11.0), (format!("{} {q}", ty.name()), t.dim, 9.0), (format!("Band {} · {place}", b + 1), t.dim, 9.0)]);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let mut r = vec![cx.zones.picture, Self::tap_rect(cx)];
        if cx.more {
            r.push(BALANCE);
        } else {
            r.extend([g::PILL_STRIP, TYPE_BOX, READOUT]);
        }
        r
    }

    fn wants_audio(&self) -> bool {
        self.tap != Tap::Off
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        let mut dirty = cx.more;
        if let (Some(audio), true) = (live.audio, self.tap != Tap::Off) {
            let was = self.spectra.live;
            dirty |= self.spectra.update(audio, g::Listen::Both, false, live.sample_rate, live.dt) || was;
        }
        dirty
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        self.handle(ev, cx, io)
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.choice(P_SPLIT_SOLO) != 0
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
        vec![format!("Band {n}: gain to 0 dB"), format!("Band {n}: invert gain"), format!("Band {n}: copy"), format!("Band {n}: paste"), format!("Band {n}: delete")]
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        let b = self.band;
        let p = |f| cx.p(bp(b, f));
        match item {
            0 => io.set(p(F_GAIN), 0.0),
            1 => io.set(p(F_GAIN), -cx.v(bp(b, F_GAIN))),
            2 => self.clip = Some((0..PER_BAND).map(|f| cx.v(bp(b, f))).collect()),
            3 => {
                let Some(clip) = self.clip.clone() else { return Response::NONE };
                for (f, v) in clip.into_iter().enumerate() {
                    io.set(p(f), v);
                }
            }
            4 => {
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
        let mut tips = vec![g::tip(g::range_rect(&cx.zones), "Graph range"), g::tip(Self::tap_rect(cx), "Spectrum: off, the module's input, or input and output")];
        for (b, at) in self.nodes(cx, &axis).iter().enumerate() {
            let Some(at) = at else { continue };
            let ty = type_of(cx, b);
            let r = (at.x - 7.0, at.y - 7.0, 14.0, 14.0);
            tips.push(g::tip(r, format!("Band {} {}: {}. Drag; wheel for Q (Cmd: slope); double-click for 0 dB; Alt-click on/off", b + 1, ty.name(), g::hz_text(cx.v(bp(b, F_FREQ))))));
        }
        if !cx.more {
            for b in 0..BANDS {
                tips.push(g::tip(g::pill_rect(b), format!("Band {}: click to pick (an empty band is created); Alt-click on/off", b + 1)));
            }
        }
        tips
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_constants_are_the_crate_ids() {
        assert_eq!(PARAMS[band_param(4, F_SPLIT_TAIL)].id, "b5SplitTail");
        assert_eq!(PARAMS[P_SPLIT_SMOOTHING].id, "splitSmoothing");
        assert_eq!(TYPES[3], "High Pass");
        assert!(chip_flags(&[0.0; NPARAMS]) == (false, false));
    }
}
