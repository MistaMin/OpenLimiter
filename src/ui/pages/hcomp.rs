//! Harmonic Compressor (`crates/comp-harmonic`): seven compressor behaviours on one static
//! curve, three meters, a 10 s trace and the four harmonic amounts. See
//! `design/new-modules.md` sections 0 and 1.

use std::collections::VecDeque;

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use comp_harmonic::{curve, curve_params, idx, view as vw, Mode};

use super::bands::common::*;
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::ui::theme::Theme;
use crate::INFO;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != idx::COUNT {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(HComp::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). A print with Delta or
/// sidechain Listen on is wrong, so those are the warning.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let v = |i: usize| values.get(i).copied().unwrap_or(0.0);
    (v(idx::KEY_SOURCE) >= 0.5, v(idx::DELTA) >= 0.5 || v(idx::SC_LISTEN) >= 0.5)
}

const CURVE: R = (120.0, 416.0, 200.0, 200.0);
const TX0: f32 = 508.0;
const TX1: f32 = 856.0;
const SPAN_S: f32 = 10.0;
const RATIO_KNOB: R = (240.0, 636.0, 120.0, 124.0);
const MODE_KEYS: &[&str] = &["CLASS-A", "HYBRID", "DIODE", "BUS VCA", "FET", "OPTO", "RMS"];
const MORE_LATENCY: R = (632.0, 660.0, 200.0, 20.0);

/// Where things go: the meters, trace and harmonic panel stretch up in TALL; the curve stays.
struct Geo {
    top: f32,
}

impl Geo {
    fn new(cx: &Ctx) -> Self {
        Self { top: cx.zones.picture.1 }
    }
    fn meter(&self, i: usize) -> R {
        (328.0 + 48.0 * i as f32, self.top, 36.0, 592.0 - self.top)
    }
    fn max_box(&self, i: usize) -> R {
        (324.0 + 48.0 * i as f32, 596.0, 44.0, 18.0)
    }
    fn trace(&self) -> R {
        (476.0, self.top, 388.0, 616.0 - self.top)
    }
    fn gr_lane(&self) -> (f32, f32) {
        let y0 = self.top + 8.0;
        (y0, y0 + (592.0 - y0) * 60.0 / 168.0)
    }
    fn gr_y(&self, db: f32) -> f32 {
        let (a, b) = self.gr_lane();
        (a + 2.0 + (b - a - 2.0) * db.max(0.0) / 20.0).min(b)
    }
    fn level_lane(&self) -> (f32, f32) {
        (self.gr_lane().1 + 8.0, 592.0)
    }
    fn lv_y(&self, l: f32) -> f32 {
        let (a, b) = self.level_lane();
        (a + (b - a) * (-l) / 60.0).clamp(a, b)
    }
    /// Points per dB in the level lane.
    fn lv_ppd(&self) -> f32 {
        let (a, b) = self.level_lane();
        (b - a) / 60.0
    }
    fn harm(&self) -> R {
        (872.0, self.top, 208.0, 616.0 - self.top)
    }
    fn bar_top(&self) -> f32 {
        self.top + 34.0
    }
    fn bar_y(&self, pct: f32) -> f32 {
        570.0 - pct.clamp(0.0, 100.0) / 100.0 * (570.0 - self.bar_top())
    }
    fn bar_grip(&self, i: usize) -> R {
        let t = self.bar_top();
        (884.0 + 48.0 * i as f32, t - 4.0, 40.0, 570.0 - t + 8.0)
    }
    fn chip(&self, k: usize) -> R {
        (760.0, self.top + 12.0 + 26.0 * k as f32, 92.0, 22.0)
    }
}

// ---- the RATIO taper: -2..0 OVER, 0..1 UP, 1..100 on a log travel (1:1 at 20 %, 10:1 at 60 %) ----

fn ratio_norm(r: f32) -> f32 {
    if r < 0.0 {
        0.1 * (r + 2.0) / 2.0
    } else if r < 1.0 {
        0.1 + 0.1 * r
    } else {
        0.2 + 0.8 * (r.min(100.0).log10() / 2.0)
    }
}

fn ratio_plain(n: f32) -> f32 {
    let n = n.clamp(0.0, 1.0);
    let r = if n < 0.1 {
        -2.0 + 2.0 * n / 0.1
    } else if n < 0.2 {
        (n - 0.1) / 0.1
    } else {
        10f32.powf(2.0 * (n - 0.2) / 0.8)
    };
    // The detent at 1:1, and the parameter's 0.1 steps.
    if (n - 0.2).abs() < 0.012 {
        1.0
    } else {
        (r * 10.0).round() / 10.0
    }
}

fn ratio_text(r: f32) -> String {
    if r >= 99.5 || r.abs() < 0.01 {
        "∞:1".into()
    } else if r < 0.0 {
        format!("{r:.1}:1 OVER")
    } else if r < 1.0 {
        format!("1:{:.1} UP", 1.0 / r)
    } else {
        format!("{r:.1}:1")
    }
}

fn db_text(v: f32) -> String {
    format!("{v:+.1} dB")
}

fn pct_text(v: f32) -> String {
    format!("{v:.0}%")
}

fn hz_value(v: f32) -> String {
    format!("{v:.0} Hz")
}

#[derive(Clone, Copy)]
struct Sample {
    age: f32,
    gr: f32,
    det: f32,
    out: f32,
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    /// Threshold, moved by x (curve) or y (trace) at `per` points per dB.
    Thr { last: f32, v: f32, per: f32, along_x: bool },
    /// Ratio, on its taper.
    Ratio { last: f32, n: f32 },
}

pub struct HComp {
    drag: Option<Drag>,
    trace: VecDeque<Sample>,
    hold: f32,
    age: f32,
    max: f32,
    /// Detector level for the curve's read-out, 10 Hz.
    det: f32,
    tile_age: f32,
}

impl Default for HComp {
    fn default() -> Self {
        Self { drag: None, trace: VecDeque::new(), hold: 0.0, age: 0.0, max: 0.0, det: -120.0, tile_age: 1.0 }
    }
}

fn values(cx: &Ctx) -> [f32; idx::COUNT] {
    let mut v = [0.0; idx::COUNT];
    for (i, x) in v.iter_mut().enumerate() {
        *x = cx.v(i);
    }
    v
}

fn mode(cx: &Ctx) -> Mode {
    Mode::from_index(cx.choice(idx::MODE) as i32)
}

fn vw_get(live: &Live, i: usize) -> Option<f32> {
    (live.view.len() >= vw::LEN).then(|| live.view[i])
}

fn harmonics_off(cx: &Ctx) -> bool {
    (idx::H2..=idx::H5).all(|i| cx.v(i) <= 0.0)
}

/// A bipolar meter: 0 dB at mid-height, +-12 dB, up in `text`, down in `accent`.
fn bipolar(s: &mut Scene, t: &Theme, r: R, db: f32, alpha: f32, tick: Option<f32>) {
    let (x, y, _, h) = r;
    let mid = y + h * 0.5;
    let ppd = h * 0.5 / 12.0;
    s.fill_round(Rect::new(x, y, 14.0, h), 3.0, t.seg_off, t.seg_off, true);
    let d = db.clamp(-12.0, 12.0) * ppd;
    if d > 0.5 {
        let c = t.text.with_alpha(0.85 * alpha);
        s.fill_rect(Rect::new(x, mid - d, 14.0, d), c, c, true);
    } else if d < -0.5 {
        let c = t.accent.with_alpha(alpha);
        s.fill_rect(Rect::new(x, mid, 14.0, -d), c, c, true);
    }
    s.line(Point::new(x, mid), Point::new(x + 14.0, mid), t.grid_strong, 1.0);
    if let Some(v) = tick {
        let ty = mid - v.clamp(-12.0, 12.0) * ppd;
        s.stroke_round(Rect::new(x - 1.0, ty - 2.0, 16.0, 4.0), 1.5, t.text.with_alpha(0.8), 1.0);
    }
}

fn bipolar_scale(s: &mut Scene, t: &Theme, r: R) {
    let (x, y, _, h) = r;
    for k in -4i32..=4 {
        let d = k as f32 * 3.0;
        let yy = y + h * 0.5 - d * h * 0.5 / 12.0;
        s.line(Point::new(x + 16.0, yy), Point::new(x + 19.0, yy), t.grid_strong, 1.0);
        if k % 2 == 0 && k.abs() < 4 {
            let label = if k == 0 { "0".to_string() } else { format!("{d:+.0}") };
            s.text(Point::new(x + 21.0, yy), label, t.dim, 8.0, false);
        }
    }
}

fn signed_box(s: &mut Scene, t: &Theme, r: R, db: f32, alpha: f32) {
    kit::inset(s, t, r, 4.0);
    let label = if db.abs() < 0.05 { "0.0".to_string() } else { format!("{db:+.1}") };
    s.text(kit::rect(r).center(), label, t.text.with_alpha(alpha), 9.5, true);
}

fn inset_chip(s: &mut Scene, t: &Theme, r: R, label: &str) {
    let rr = kit::rect(r);
    s.fill_round(rr, r.3 * 0.5, t.inset, t.inset, true);
    s.stroke_round(rr, r.3 * 0.5, t.border, 1.0);
    s.text(rr.center(), label, t.text, 9.0, true);
}

impl HComp {
    fn curve_points(cx: &Ctx) -> Vec<(f32, f32)> {
        let (mode, p) = curve_params(&values(cx));
        (0..=120).map(|k| {
            let l = -60.0 + 0.5 * k as f32;
            (l, curve::output_db(mode, &p, l))
        }).collect()
    }

    fn out_at(cx: &Ctx, l: f32) -> f32 {
        let (mode, p) = curve_params(&values(cx));
        curve::output_db(mode, &p, l)
    }

    /// The chips on the trace: `(rect, label, parameter, value a click writes, warning look)`.
    fn chips(cx: &Ctx) -> Vec<(R, String, usize, bool)> {
        let g = Geo::new(cx);
        let mut v = Vec::new();
        if cx.on(idx::DELTA) {
            v.push((g.chip(v.len()), "! DELTA".to_string(), idx::DELTA, true));
        }
        if cx.on(idx::SC_LISTEN) {
            v.push((g.chip(v.len()), "! LISTEN".to_string(), idx::SC_LISTEN, true));
        }
        let level = cx.v(idx::OUTPUT_GAIN);
        if level.abs() >= 0.05 {
            v.push((g.chip(v.len()), format!("LEVEL {level:+.1}"), idx::OUTPUT_GAIN, false));
        }
        v
    }

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let m = Map::square(CURVE);
        let pts = Self::curve_points(cx);
        s.clip_round(kit::rect(CURVE), 4.0);
        let line: Vec<Point> = pts.iter().map(|&(l, o)| Point::new(m.x(l), m.y(o.clamp(-61.0, 0.5)))).collect();
        let mut poly = line.clone();
        poly.push(Point::new(m.x(0.0), m.y(0.0)));
        poly.push(Point::new(m.x(-60.0), m.y(-60.0)));
        s.fill_poly(poly, t.accent.with_alpha(0.2), t.accent.with_alpha(0.2));
        kit::curve(s, line, t.accent, 2.0);
        // The part above 0 dBFS clips at the top: an arrow where it leaves.
        if let Some(&(l, _)) = pts.iter().find(|p| p.1 > 0.0) {
            let x = m.x(l);
            let y = CURVE.1 + 3.0;
            s.fill_poly(vec![Point::new(x - 4.0, y + 6.0), Point::new(x + 4.0, y + 6.0), Point::new(x, y)], t.accent, t.accent);
        }
        // The threshold handle.
        let thr = cx.v(idx::THRESHOLD);
        let th = Point::new(m.x(thr), m.y(Self::out_at(cx, thr).clamp(-60.0, 0.0)));
        s.fill_ellipse_solid(Scene::ellipse_at(th, 4.5), t.panel);
        s.stroke_ellipse(Scene::ellipse_at(th, 4.5), t.accent, 1.5);
        // The operating dot.
        if let (Some(d), Some(o)) = (vw_get(live, vw::DETECTOR_DB), vw_get(live, vw::CURVE_OUTPUT_DB)) {
            if d > -60.0 {
                let p = Point::new(m.x(d.min(0.0)), m.y(o.clamp(-60.0, 0.0)));
                s.fill_ellipse_solid(Scene::ellipse_at(p, 4.0), t.accent);
                let keyed = vw_get(live, vw::KEY_ACTIVE).is_some_and(|k| k >= 0.5);
                s.stroke_ellipse(Scene::ellipse_at(p, 5.0), if keyed { t.warm } else { t.panel }, 1.5);
            }
        }
        s.pop_clip();
        let det = if self.det > -99.0 { format!("DET {:.1}", self.det) } else { "DET -".into() };
        s.text(Point::new(CURVE.0 + 8.0, CURVE.1 + 16.0), det, t.warm, 9.0, false);
        let knee = if mode(cx).uses_knee() { format!("{} · knee {:.0} dB", mode(cx).label(), cx.v(idx::KNEE)) } else { mode(cx).label().to_string() };
        kit::text_right(s, CURVE.0 + CURVE.2 - 8.0, CURVE.1 + CURVE.3 - 10.0, &knee, t.dim, 8.0);
    }

    fn paint_meters(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let g = Geo::new(cx);
        let gr = vw_get(live, vw::DEEPEST_REDUCTION_DB).unwrap_or(0.0).max(0.0);
        kit::gr_meter(s, t, g.meter(0), gr, self.hold, 20.0);
        max_box(s, t, g.max_box(0), self.max);
        let trans_on = cx.on(idx::TRANSIENT_ENABLED);
        let ta = if trans_on { 1.0 } else { 0.25 };
        bipolar(s, t, g.meter(1), vw_get(live, vw::TRANSIENT_EXTREME_DB).unwrap_or(0.0), ta, None);
        signed_box(s, t, g.max_box(1), vw_get(live, vw::TRANSIENT_GAIN_DB).unwrap_or(0.0), ta);
        let auto_on = cx.on(idx::AUTO_GAIN);
        let target = vw_get(live, vw::AUTO_GAIN_TARGET_DB).filter(|_| !auto_on);
        bipolar(s, t, g.meter(2), vw_get(live, vw::AUTO_GAIN_DB).unwrap_or(0.0), 1.0, target);
        signed_box(s, t, g.max_box(2), vw_get(live, vw::AUTO_GAIN_DB).unwrap_or(0.0), if auto_on { 1.0 } else { 0.5 });
    }

    fn paint_trace(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let g = Geo::new(cx);
        let x = |age: f32| TX1 - age / SPAN_S * (TX1 - TX0);
        let pts = |f: &dyn Fn(&Sample) -> f32| -> Vec<Point> { self.trace.iter().map(|p| Point::new(x(p.age), f(p))).collect() };
        if self.trace.len() >= 2 {
            kit::area(s, &pts(&|p| g.gr_y(p.gr)), g.gr_y(0.0), t.accent.with_alpha(0.55));
            kit::curve(s, pts(&|p| g.lv_y(p.out)), t.out_line, 1.0);
            kit::curve(s, pts(&|p| g.lv_y(p.det)), t.warm, 1.5);
        }
        let ty = g.lv_y(cx.v(idx::THRESHOLD));
        kit::dashed_h(s, TX0, TX1, ty, 5.0, 3.0, t.accent, 1.2);
        s.fill_round(Rect::new(TX1 - 18.0, ty - 5.0, 18.0, 10.0), 3.0, t.accent, t.accent, true);
        // The context line.
        let ext = cx.choice(idx::KEY_SOURCE) == 1;
        let note = if ext && !cx.key_present {
            Some("External key selected, no key connected")
        } else if ext && live.key.0 && live.key.1 < -90.0 {
            Some("Key is silent: nothing is compressed")
        } else if cx.on(idx::SC_LISTEN) {
            Some("Sidechain listen is on")
        } else {
            None
        };
        if let Some(n) = note {
            s.text(Point::new(TX0 + 8.0, 584.0), n, t.dim, 9.0, false);
        }
        for (r, label, _, warn) in Self::chips(cx) {
            if warn {
                warn_chip(s, t, r, &label);
            } else {
                inset_chip(s, t, r, &label);
            }
        }
    }

    fn paint_harmonics(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let g = Geo::new(cx);
        let r = g.harm();
        let off = harmonics_off(cx);
        s.text(Point::new(r.0 + 10.0, r.1 + 12.0), if off { "HARMONICS OFF (bit-exact)" } else { "HARMONICS" }, t.dim, 8.5, false);
        for i in 0..4 {
            let v = cx.v(idx::H2 + i);
            let gr = g.bar_grip(i);
            let cxm = gr.0 + gr.2 * 0.5;
            let y = g.bar_y(v);
            if 570.0 - y > 0.5 {
                s.fill_round(Rect::new(cxm - 14.0, y, 28.0, 570.0 - y), 2.0, t.accent, t.accent, true);
            }
            s.line(Point::new(cxm - 16.0, y), Point::new(cxm + 16.0, y), t.text, 1.5);
            s.text(Point::new(cxm, 584.0), format!("{v:.0}%"), if v > 0.0 { t.text } else { t.dim }, 9.0, true);
        }
    }

    fn paint_ratio_knob(s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let cell = Cell::knob(Size::Big, cx.p(idx::RATIO), "RATIO", RATIO_KNOB.0, RATIO_KNOB.1);
        let (kx, ky, r) = kit::knob_circle(&cell, Size::Big);
        let c = Point::new(kx, ky);
        let v = cx.v(idx::RATIO);
        let n = ratio_norm(v);
        let origin = ratio_norm(1.0);
        if (n - origin).abs() > 0.004 {
            s.stroke_arc(c, r + 6.0, kit::knob_angle(origin.min(n)), kit::knob_angle(origin.max(n)), t.accent, 4.0);
        }
        // The 1:1 detent.
        let a = kit::knob_angle(origin);
        s.line(Point::new(kx + a.cos() * (r + 9.0), ky + a.sin() * (r + 9.0)), Point::new(kx + a.cos() * (r + 12.0), ky + a.sin() * (r + 12.0)), t.dim, 1.2);
        let a = kit::knob_angle(n);
        s.line(Point::new(kx + a.cos() * r * 0.42, ky + a.sin() * r * 0.42), Point::new(kx + a.cos() * r * 0.86, ky + a.sin() * r * 0.86), t.text, 3.0);
        let b = kit::knob_value_box(&cell, Size::Big);
        s.text(Point::new(b.0 + b.2 * 0.5, b.1 + b.3 * 0.5), text_fit(&ratio_text(v), 12.0, b.2 - 4.0), t.text, 12.0, true);
    }

    fn ratio_drag(&mut self, io: &mut Io, cx: &Ctx, y: f32) {
        let p = cx.p(idx::RATIO);
        io.begin(p);
        self.drag = Some(Drag::Ratio { last: y, n: ratio_norm(cx.v(idx::RATIO)) });
    }
}

impl Page for HComp {
    fn has_more(&self) -> bool {
        true
    }

    fn key_param(&self, cx: &Ctx) -> Option<usize> {
        cx.find("keySource")
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        matches!(local, idx::MODE | idx::TRANSIENT_ENABLED | idx::SC_ENABLED | idx::SC_LISTEN | idx::DELTA | idx::OS_MODE)
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let hy = cx.zones.header.1;
        let p = |i: usize| cx.p(i);
        let g = Geo::new(cx);
        let mut v = vec![Cell::segmented(p(idx::MODE), "MODE", (420.0, hy + 8.0, 364.0, 24.0)).keys(MODE_KEYS).tip("The compressor's behaviour: each has its own curve and timing")];
        for i in 0..4 {
            v.push(Cell::grip(p(idx::H2 + i), g.bar_grip(i)).tip(format!("H{}: drag up for more of this harmonic; double-click to type", i + 2)));
        }
        if cx.more {
            v.push(Cell::knob(Size::Small, p(idx::OUTPUT_GAIN), "LEVEL", 136.0, 644.0).format(db_text).tip("Output level after the mix, full +-24 dB"));
            v.push(Cell::segmented(p(idx::OS_MODE), "OVERSAMPLING", (248.0, 664.0, 176.0, 24.0)).keys(&["1x", "2x", "4x", "8x"]));
            v.push(Cell::segmented(p(idx::OS_FILTER), "FILTER", (440.0, 664.0, 176.0, 24.0)).keys(&["MIN PHASE", "LIN PHASE"]).dim(cx.choice(idx::OS_MODE) == 0));
            return v;
        }
        let m = mode(cx);
        let times = m.uses_times();
        let trans = cx.on(idx::TRANSIENT_ENABLED);
        let listen = if cx.on(idx::SC_LISTEN) { "! LISTEN" } else { "LISTEN" };
        let delta = if cx.on(idx::DELTA) { "! DELTA" } else { "DELTA" };
        v.extend([
            Cell::knob(Size::Big, p(idx::THRESHOLD), "THRESHOLD", 120.0, 636.0),
            // RATIO is drawn and dragged by the page on its own taper; this cell keeps it
            // reachable (host menu, accessibility) and takes no input itself.
            Cell::grip(p(idx::RATIO), (RATIO_KNOB.0, RATIO_KNOB.1 + 98.0, RATIO_KNOB.2, 22.0)).dim(true).tip("Ratio: drag the knob; 1:1 is the detent, below it upward and over-compression"),
            Cell::knob(Size::Big, p(idx::ATTACK), "ATTACK", 360.0, 636.0).format(time_text).dim(!times),
            Cell::knob(Size::Big, p(idx::RELEASE), "RELEASE", 480.0, 636.0).format(time_text).dim(!times),
            Cell::knob(Size::Big, p(idx::MAKEUP), "MAKE-UP", 600.0, 636.0).format(db_text).tip("Gain before the harmonics: it drives them"),
            Cell::pill(p(idx::AUTO_ATTACK), "AUTO ATT", (728.0, 633.0, 60.0, 22.0)).dim(!times),
            Cell::pill(p(idx::AUTO_RELEASE), "AUTO REL", (792.0, 633.0, 60.0, 22.0)).dim(!times),
            Cell::knob(Size::Mini, p(idx::KNEE), "KNEE", 728.0, 662.0).dim(!m.uses_knee()).tip("Soft knee (CLASS-A only)"),
            Cell::knob(Size::Mini, p(idx::INPUT_GAIN), "DRIVE", 784.0, 662.0).format(db_text),
            Cell::knob(Size::Mini, p(idx::MIX), "MIX", 840.0, 662.0).format(pct_text),
            Cell::pill(p(idx::SC_ENABLED), "SC HP", (728.0, 731.0, 56.0, 22.0)).tip("High-pass the detector"),
            Cell::value(p(idx::SC_HP_FREQ), "SC HP", (788.0, 731.0, 56.0, 22.0)).format(hz_value).dim(!cx.on(idx::SC_ENABLED)),
            Cell::pill(p(idx::SC_LISTEN), listen, (848.0, 731.0, 52.0, 22.0)).tip("Hear the detector signal. Do not print with it on"),
            Cell::pill(p(idx::TRANSIENT_ENABLED), "TRANSIENT", (912.0, 633.0, 104.0, 22.0)),
            Cell::knob(Size::Mini, p(idx::TRANSIENT_ATTACK), "ATTACK", 912.0, 662.0).format(pct_text).dim(!trans),
            Cell::knob(Size::Mini, p(idx::TRANSIENT_SUSTAIN), "SUSTAIN", 968.0, 662.0).format(pct_text).dim(!trans),
            Cell::knob(Size::Mini, p(idx::TRANSIENT_SPEED), "SPEED", 1024.0, 662.0).format(time_text).dim(!trans),
            Cell::pill(p(idx::AUTO_GAIN), "AUTO GAIN", (912.0, 731.0, 80.0, 22.0)),
            Cell::pill(p(idx::DELTA), delta, (996.0, 731.0, 84.0, 22.0)).tip("Hear what the compressor takes away. Do not print with it on"),
        ]);
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let g = Geo::new(cx);
        curve_frame(s, t, CURVE);
        kit::gr_scale(s, t, g.meter(0), 20.0);
        bipolar_scale(s, t, g.meter(1));
        bipolar_scale(s, t, g.meter(2));
        for (i, l) in ["GR", "TRANS", "AUTO"].iter().enumerate() {
            s.text(Point::new(g.meter(i).0 + 28.0, 588.0), *l, t.text, 7.0, true);
        }
        let tr = g.trace();
        kit::plot(s, t, tr);
        let (g0, g1) = g.gr_lane();
        let (l0, l1) = g.level_lane();
        for d in [0.0, 10.0, 20.0] {
            let y = g.gr_y(d);
            s.line(Point::new(TX0, y), Point::new(TX1, y), if d == 0.0 { t.grid_strong } else { t.grid }, 1.0);
            if d < 20.0 {
                kit::text_right(s, TX0 - 4.0, y, &format!("{d:.0}"), t.dim, 8.0);
            }
        }
        for l in [0.0, -12.0, -24.0, -36.0, -48.0] {
            let y = g.lv_y(l);
            s.line(Point::new(TX0, y), Point::new(TX1, y), t.grid, 1.0);
            kit::text_right(s, TX0 - 4.0, y, &format!("{l:.0}"), t.dim, 8.0);
        }
        for k in 0..=5 {
            let x = TX0 + (TX1 - TX0) * k as f32 / 5.0;
            s.line(Point::new(x, g0), Point::new(x, l1), t.grid, 1.0);
        }
        let _ = (g1, l0);
        s.text(Point::new(TX0, 604.0), "-10 s", t.dim, 8.5, false);
        kit::text_right(s, TX1, 604.0, "now", t.dim, 8.5);
        s.text(Point::new(TX0 + 6.0, g0 + 10.0), "GR", t.dim, 8.0, false);
        s.text(Point::new(TX0 + 6.0, l0 + 10.0), "DETECTOR · THRESHOLD · OUT", t.dim, 8.0, false);
        let h = g.harm();
        kit::plot(s, t, h);
        for i in 0..4 {
            let gr = g.bar_grip(i);
            let cxm = gr.0 + gr.2 * 0.5;
            s.fill_round(Rect::new(cxm - 14.0, g.bar_top(), 28.0, 570.0 - g.bar_top()), 2.0, t.seg_off, t.seg_off, true);
            s.text(Point::new(cxm, g.bar_top() - 9.0), format!("H{}", i + 2), t.dim, 8.5, true);
        }
        if cx.more {
            s.text(Point::new(136.0, 632.0), "AFTER THE MIX", t.dim, 9.0, false);
            s.text(Point::new(248.0, 648.0), "OVERSAMPLING", t.dim, 9.0, false);
            s.text(Point::new(440.0, 648.0), "FILTER", t.dim, 9.0, false);
        } else {
            kit::paint_static(s, t, &Cell::knob(Size::Big, cx.p(idx::RATIO), "RATIO", RATIO_KNOB.0, RATIO_KNOB.1));
            sub_caption(s, t, (600.0, 636.0, 120.0, 124.0), "drives the harmonics");
            for x in [724.0, 904.0] {
                s.line(Point::new(x, 630.0), Point::new(x, 764.0), t.border, 1.0);
            }
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        self.paint_curve(s, cx, live);
        self.paint_meters(s, cx, live);
        self.paint_trace(s, cx, live);
        self.paint_harmonics(s, cx);
        if cx.more {
            let lat = vw_get(live, vw::LATENCY_SAMPLES).map_or(live.latency as f32, |l| l);
            s.text(Point::new(MORE_LATENCY.0, MORE_LATENCY.1 + 10.0), format!("latency {lat:.0} smp"), cx.t.dim, 9.5, false);
        } else {
            Self::paint_ratio_knob(s, cx);
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let g = Geo::new(cx);
        let mut v = vec![CURVE, (320.0, g.top, 152.0, 616.0 - g.top), g.trace(), g.harm()];
        v.push(if cx.more { MORE_LATENCY } else { (RATIO_KNOB.0, RATIO_KNOB.1 + 12.0, RATIO_KNOB.2, RATIO_KNOB.3 - 12.0) });
        v
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max};
        &[Last, Max, Max, Last, Last, Last, Last, Max, Max]
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.on(idx::DELTA) || cx.on(idx::SC_LISTEN)
    }

    fn tick(&mut self, _cx: &Ctx, live: &Live) -> bool {
        for p in self.trace.iter_mut() {
            p.age += live.dt;
        }
        while self.trace.front().is_some_and(|p| p.age > SPAN_S) {
            self.trace.pop_front();
        }
        if live.view.len() >= vw::LEN {
            let v = live.view;
            let gr = v[vw::DEEPEST_REDUCTION_DB].max(0.0);
            self.trace.push_back(Sample { age: 0.0, gr, det: v[vw::DETECTOR_DB], out: v[vw::OUTPUT_PEAK_DB] });
            if gr >= self.hold {
                self.hold = gr;
                self.age = 0.0;
            } else {
                self.age += live.dt;
                if self.age > 1.5 {
                    self.hold = gr;
                }
            }
            self.max = self.max.max(gr);
            self.tile_age += live.dt;
            if self.tile_age >= 0.1 {
                self.tile_age = 0.0;
                self.det = v[vw::DETECTOR_DB];
            }
        }
        true
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let g = Geo::new(cx);
        let mut v = vec![
            tip(CURVE, "The mode's steady-state curve. Drag the handle sideways for the threshold, above it up and down for the ratio; wheel over the knee (CLASS-A) for the knee; double-click resets"),
            tip(g.trace(), "The last 10 s: reduction, detector level, output peak and the threshold; drag the threshold line"),
            tip(g.meter(0), "Gain reduction; click the box to reset its maximum"),
            tip(g.meter(1), "Transient designer gain"),
            tip(g.meter(2), "Auto Gain applied; the hollow tick is its target while Auto Gain is off"),
            tip(g.harm(), "Harmonics 2 to 5 added after the compressor; all at 0 % is bit-exact"),
        ];
        for (r, label, _, _) in Self::chips(cx) {
            v.push(tip(r, &format!("{label}: click to set it back")));
        }
        v
    }

    fn menu(&self, _cx: &Ctx) -> Vec<String> {
        vec!["Load this mode's timing".into()]
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        if item != 0 {
            return Response::NONE;
        }
        let (a, r, k) = mode(cx).defaults();
        io.set(cx.p(idx::ATTACK), a);
        io.set(cx.p(idx::RELEASE), r);
        io.set(cx.p(idx::KNEE), k);
        Response::TAKEN
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let g = Geo::new(cx);
        if let Some(drag) = self.drag {
            let (x, y, up) = match ev {
                Pointer::Move { x, y } => (x, y, false),
                Pointer::Up { x, y } => (x, y, true),
                _ => return Response::TAKEN,
            };
            let k = if io.modifiers().shift { 0.1 } else { 1.0 };
            match drag {
                Drag::Thr { last, mut v, per, along_x } => {
                    let pos = if along_x { x } else { y };
                    let d = if along_x { pos - last } else { last - pos };
                    v += d / per * k;
                    io.perform(cx.p(idx::THRESHOLD), v);
                    if up {
                        io.end(cx.p(idx::THRESHOLD));
                    }
                    self.drag = Some(Drag::Thr { last: pos, v, per, along_x });
                }
                Drag::Ratio { last, mut n } => {
                    n = (n + (last - y) / 200.0 * k).clamp(0.0, 1.0);
                    io.perform(cx.p(idx::RATIO), ratio_plain(n));
                    if up {
                        io.end(cx.p(idx::RATIO));
                    }
                    self.drag = Some(Drag::Ratio { last: y, n });
                }
            }
            if up {
                self.drag = None;
            }
            return Response::TAKEN;
        }
        if let Pointer::Wheel { x, y, dy } = ev {
            let step = if dy > 0.0 { 1.0 } else { -1.0 } * if io.modifiers().shift { 0.1 } else { 1.0 };
            if !cx.more && kit::inside(RATIO_KNOB, x, y) {
                io.set(cx.p(idx::RATIO), ratio_plain(ratio_norm(cx.v(idx::RATIO)) + 0.01 * step));
                return Response::TAKEN;
            }
            if kit::inside(CURVE, x, y) && mode(cx).uses_knee() {
                let m = Map::square(CURVE);
                let half = cx.v(idx::KNEE).max(6.0) * 0.5;
                if (m.level_at(x) - cx.v(idx::THRESHOLD)).abs() <= half {
                    io.set(cx.p(idx::KNEE), cx.v(idx::KNEE) + 0.5 * step);
                    return Response::TAKEN;
                }
            }
            return Response::NONE;
        }
        let Pointer::Down { x, y, clicks } = ev else { return Response::NONE };
        for (r, _, param, _) in Self::chips(cx) {
            if kit::inside(r, x, y) {
                io.set(cx.p(param), 0.0);
                return Response::TAKEN;
            }
        }
        if kit::inside(g.max_box(0), x, y) {
            self.hold = 0.0;
            self.max = 0.0;
            return Response::TAKEN;
        }
        if !cx.more && kit::inside(RATIO_KNOB, x, y) {
            if clicks >= 2 {
                let p = cx.p(idx::RATIO);
                io.set(p, INFO.params[p].default);
            } else {
                self.ratio_drag(io, cx, y);
            }
            return Response::TAKEN;
        }
        let thr = cx.v(idx::THRESHOLD);
        let thr_p = cx.p(idx::THRESHOLD);
        if kit::inside(g.trace(), x, y) {
            if (y - g.lv_y(thr)).abs() <= 6.0 && x >= TX0 && x <= TX1 {
                if clicks >= 2 {
                    io.set(thr_p, INFO.params[thr_p].default);
                } else {
                    io.begin(thr_p);
                    self.drag = Some(Drag::Thr { last: y, v: thr, per: g.lv_ppd(), along_x: false });
                }
                return Response::TAKEN;
            }
            return Response::NONE;
        }
        if !kit::inside(CURVE, x, y) {
            return Response::NONE;
        }
        let m = Map::square(CURVE);
        let th = (m.x(thr), m.y(Self::out_at(cx, thr).clamp(-60.0, 0.0)));
        if (x - th.0).abs() <= 8.0 && (y - th.1).abs() <= 8.0 {
            if clicks >= 2 {
                io.set(thr_p, INFO.params[thr_p].default);
            } else {
                io.begin(thr_p);
                self.drag = Some(Drag::Thr { last: x, v: thr, per: m.ppd, along_x: true });
            }
            return Response::TAKEN;
        }
        if m.level_at(x) > thr {
            if clicks >= 2 {
                let p = cx.p(idx::RATIO);
                io.set(p, INFO.params[p].default);
            } else {
                self.ratio_drag(io, cx, y);
            }
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

    /// Renders a page with a moving read-out (`view(tick)`) for 10 s of ticks, every theme,
    /// STANDARD, TALL and MORE: `build/ui-<key>-<std|tall|more>-<theme>.ppm`.
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
                for k in 0..300 {
                    shared.set_view(slot, &view(k));
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
    fn dump_hcomp() {
        use comp_harmonic::view as vw;
        let view = |k: usize| {
            let ph = k as f32 / 30.0;
            let det = -14.0 + 6.0 * (ph * 2.1).sin() * (ph * 0.7).cos();
            let gr = ((det + 24.0) * 0.75).max(0.0);
            let mut v = vec![0.0f32; vw::LEN];
            v[vw::GAIN_REDUCTION_DB] = -gr;
            v[vw::DEEPEST_REDUCTION_DB] = gr;
            v[vw::DETECTOR_DB] = det;
            v[vw::AUTO_GAIN_DB] = 2.4;
            v[vw::AUTO_GAIN_TARGET_DB] = 3.1;
            v[vw::TRANSIENT_GAIN_DB] = 1.8;
            v[vw::TRANSIENT_EXTREME_DB] = 4.2 * (ph * 5.0).sin();
            v[vw::INPUT_PEAK_DB] = det + 2.0;
            v[vw::OUTPUT_PEAK_DB] = det - gr + 3.0 + 4.0;
            v[vw::CURVE_OUTPUT_DB] = det - gr + 3.0;
            v[vw::KEY_ACTIVE] = 1.0;
            v[vw::OS_FACTOR] = 2.0;
            v[vw::LATENCY_SAMPLES] = 3.137;
            v
        };
        dump("hcomp", &view, &[("threshold", -24.0), ("ratio", 4.0), ("makeupGain", 3.0), ("harmonic2", 12.0), ("harmonic3", 8.0), ("harmonic4", 3.0), ("transientEnabled", 1.0), ("transientAttack", 40.0), ("delta", 1.0), ("outputGain", 3.0), ("keySource", 1.0)]);
    }
}
