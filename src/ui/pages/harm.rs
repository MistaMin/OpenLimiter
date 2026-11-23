//! Harmonics (`crates/harmonics`): chosen amounts of the 2nd to 5th harmonic, or a soft or
//! tape-like curve, on a chosen band. A curve view, the harmonic ladder (set against
//! estimated) and the band strip. See `design/new-modules.md` sections 0 and 4.

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use harmonics::{latency_of, view as vw, Shape, AA_NONE, HIGH_OFF_HZ, H_MAX_DB, H_OFF_DB, LOW_OFF_HZ, POLYNOMIAL};
use harmonics::{P_ANTI_ALIAS as ANTI_ALIAS, P_AUTO_GAIN as AUTO_GAIN, P_BIAS as BIAS, P_CHANNELS as CHANNELS, P_DRIVE as DRIVE, P_FAMILY as FAMILY};
use harmonics::{P_H2 as H2, P_HARM_HIGH as HARM_HIGH, P_HARM_LOW as HARM_LOW, P_LISTEN as LISTEN, P_MIX as MIX, P_SRC_HIGH as SRC_HIGH, P_SRC_LOW as SRC_LOW, P_TRIM as TRIM};

use super::bands::common::*;
use super::eq6::{paint_spectrum, Axis, Listen, Spectra};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::INFO;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != harmonics::NPARAMS {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(Harm::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). No key; a print with
/// LISTEN on is the added signal alone, so LISTEN is the warning.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (false, values.get(LISTEN).is_some_and(|v| *v >= 0.5))
}

const CURVE: R = (120.0, 416.0, 200.0, 200.0);
const PX0: f32 = 744.0;
const PX1: f32 = 1048.0;
const NOTE: R = (120.0, 757.0, 480.0, 14.0);
const MORE_TEXT: R = (440.0, 660.0, 320.0, 36.0);

fn cu_x(u: f32) -> f32 {
    220.0 + 66.67 * u
}

fn cu_y(v: f32) -> f32 {
    516.0 - 66.67 * v
}

fn f_x(f: f32) -> f32 {
    PX0 + 101.33 * (f.max(1.0) / 20.0).log10()
}

fn x_f(x: f32) -> f32 {
    20.0 * 10f32.powf((x - PX0) / 101.33)
}

fn h_text(v: f32) -> String {
    if v <= H_OFF_DB + 0.05 {
        "OFF".into()
    } else {
        format!("{v:.1} dB")
    }
}

fn low_text(v: f32) -> String {
    if v <= LOW_OFF_HZ + 0.01 {
        "OFF".into()
    } else {
        hz_text(v)
    }
}

fn high_text(v: f32) -> String {
    if v >= HIGH_OFF_HZ - 1.0 {
        "OFF".into()
    } else {
        hz_text(v)
    }
}

fn db_text(v: f32) -> String {
    format!("{v:+.1} dB")
}

fn pct_text(v: f32) -> String {
    format!("{v:.0}%")
}

struct Geo {
    top: f32,
}

impl Geo {
    fn new(cx: &Ctx) -> Self {
        Self { top: cx.zones.picture.1 }
    }
    fn ladder(&self) -> R {
        (336.0, self.top, 360.0, 616.0 - self.top)
    }
    fn lad_y(&self, db: f32) -> f32 {
        let y0 = self.top + 32.0;
        y0 + (592.0 - y0) * (-db.clamp(-80.0, 0.0)) / 80.0
    }
    fn col(k: usize) -> f32 {
        400.0 + 60.0 * k as f32
    }
    fn handle_grip(&self, i: usize) -> R {
        let (a, b) = (self.lad_y(H_MAX_DB), self.lad_y(H_OFF_DB));
        (438.0 + 60.0 * i as f32, a - 6.0, 44.0, b - a + 12.0)
    }
    fn strip(&self) -> R {
        (712.0, self.top, 368.0, 616.0 - self.top)
    }
    /// The two lanes: source, harmonics, as plot rects.
    fn lanes(&self) -> [R; 2] {
        let y0 = self.top + 16.0;
        let h = (584.0 - y0 - 16.0) * 0.5;
        [(PX0, y0, PX1 - PX0, h), (PX0, y0 + h + 16.0, PX1 - PX0, h)]
    }
    fn chip(&self) -> R {
        (964.0, self.top + 12.0, 84.0, 22.0)
    }
}

fn shape(cx: &Ctx) -> Shape {
    Shape {
        family: cx.choice(FAMILY),
        harmonics_db: [0, 1, 2, 3].map(|i| cx.v(H2 + i) as f64),
        drive_db: cx.v(DRIVE) as f64,
        bias_pct: cx.v(BIAS) as f64,
    }
}

/// Second-order Butterworth high-pass and low-pass in series, dB (an edge at its OFF end is
/// no filter).
fn band_db(f: f32, lo: f32, hi: f32) -> f32 {
    let mut g = 1.0f32;
    if lo > LOW_OFF_HZ + 0.01 {
        let r = (f / lo).powi(4);
        g *= r / (1.0 + r);
    }
    if hi < HIGH_OFF_HZ - 1.0 {
        let r = (f / hi).powi(4);
        g *= 1.0 / (1.0 + r);
    }
    10.0 * g.max(1e-12).log10()
}

#[derive(Clone, Copy)]
struct EdgeDrag {
    local: usize,
    low: bool,
}

#[derive(Default)]
pub struct Harm {
    drag: Option<EdgeDrag>,
    spectra: Spectra,
    /// THD, added, auto gain at 10 Hz.
    tiles: Option<[f32; 3]>,
    tile_age: f32,
    /// The live reach (curve units) and its 1.5 s hold.
    reach: f32,
    hold: f32,
    hold_age: f32,
}

impl Harm {
    /// Edge x on the strip: an OFF edge sits at its plot end.
    fn edge_x(cx: &Ctx, local: usize, low: bool) -> f32 {
        let f = cx.v(local);
        if low {
            if f <= LOW_OFF_HZ + 0.01 { PX0 } else { f_x(f).clamp(PX0, PX1) }
        } else if f >= HIGH_OFF_HZ - 1.0 {
            PX1
        } else {
            f_x(f).clamp(PX0, PX1)
        }
    }

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let c = shape(cx).curve();
        s.clip_round(kit::rect(CURVE), 4.0);
        // The live reach behind everything.
        if self.reach > 1e-3 {
            let r = self.reach.min(1.6);
            let (a, b) = (cu_x(-r), cu_x(r));
            let fill = t.accent.with_alpha(0.10);
            s.fill_rect(Rect::new(a, CURVE.1, b - a, CURVE.3), fill, fill, true);
        }
        if self.hold > 1e-3 && self.hold < 1.55 {
            for sx in [-1.0, 1.0] {
                let x = cu_x(sx * self.hold);
                s.line(Point::new(x, CURVE.1 + CURVE.3 - 22.0), Point::new(x, CURVE.1 + CURVE.3 - 14.0), t.accent, 1.5);
            }
        }
        for u in [-1.0f32, 1.0] {
            kit::dashed(s, &[Point::new(cu_x(u), CURVE.1 + 18.0), Point::new(cu_x(u), CURVE.1 + CURVE.3)], 3.0, 3.0, t.grid_strong, 1.0);
        }
        kit::dashed(s, &[Point::new(cu_x(-1.5), cu_y(-1.5)), Point::new(cu_x(1.5), cu_y(1.5))], 3.0, 3.0, t.grid_strong, 1.0);
        let us: Vec<f32> = (0..=120).map(|k| -1.5 + 3.0 * k as f32 / 120.0).collect();
        let phi: Vec<f32> = us.iter().map(|&u| c.phi(u as f64) as f32).collect();
        kit::curve(s, us.iter().zip(&phi).map(|(&u, &v)| Point::new(cu_x(u), cu_y(v.clamp(-1.6, 1.6)))).collect(), t.text, 1.0);
        // The residual, magnified to fit.
        let worst = us.iter().zip(&phi).filter(|(u, _)| u.abs() <= 1.0).map(|(u, v)| (v - u).abs()).fold(0.0f32, f32::max);
        let k = [300.0f32, 100.0, 30.0, 10.0, 3.0, 1.0].into_iter().find(|k| worst * k <= 1.4).unwrap_or(1.0);
        if worst > 1e-6 {
            kit::curve(s, us.iter().zip(&phi).map(|(&u, &v)| Point::new(cu_x(u), cu_y(((v - u) * k).clamp(-1.6, 1.6)))).collect(), t.accent, 2.0);
        }
        s.pop_clip();
        for u in [-1.0f32, 1.0] {
            backed(s, cx, cu_x(u), CURVE.1 + 11.0, "REF", t.dim, true);
        }
        let label = if worst > 1e-6 { format!("ADDED ×{k:.0}") } else { "NOTHING ADDED".into() };
        backed(s, cx, CURVE.0 + 6.0, CURVE.1 + 30.0, &label, t.accent, false);
        let ch = match cx.choice(CHANNELS) {
            2 => Some("MID ONLY"),
            3 => Some("SIDE ONLY"),
            _ => None,
        };
        if let Some(ch) = ch {
            backed(s, cx, CURVE.0 + CURVE.2 - 6.0 - kit_width(ch) * 8.5 / 8.0 - 4.0, CURVE.1 + 30.0, ch, t.dim, false);
        }
        if self.reach > 1e-3 {
            let db = 20.0 * self.reach.log10();
            backed(s, cx, CURVE.0 + CURVE.2 * 0.5, CURVE.1 + CURVE.3 - 8.0, &format!("PEAK {db:+.1} dB re REF"), t.dim, true);
        }
    }

    fn paint_ladder(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let g = Geo::new(cx);
        let r = g.ladder();
        // Read-out row.
        let row = match self.tiles {
            Some([thd, added, auto]) => {
                let pct = 100.0 * 10f32.powf(thd / 20.0);
                let added = if added <= -119.0 { "-".to_string() } else { format!("{added:.0}") };
                format!("THD {thd:.1} dB ({pct:.1}%) · ADDED {added} · AUTO {auto:+.1}")
            }
            None => "THD - · ADDED - · AUTO -".into(),
        };
        s.text(Point::new(r.0 + 10.0, r.1 + 15.0), text_fit(&row, 8.5, r.2 - 44.0), t.text, 8.5, false);
        kit::text_right(s, r.0 + r.2 - 10.0, r.1 + 15.0, "EST", t.dim, 8.0);
        // F is the reference, 0 dB.
        let x0 = Geo::col(0);
        let y0 = g.lad_y(0.0);
        let c = t.dim.with_alpha(0.5);
        s.fill_round(Rect::new(x0 - 14.0, y0, 28.0, 592.0 - y0), 2.0, c, c, true);
        let est: Option<[f32; 4]> = (live.view.len() >= vw::LEN).then(|| [vw::H2_DB, vw::H3_DB, vw::H4_DB, vw::H5_DB].map(|i| live.view[i]));
        for i in 0..4 {
            let x = Geo::col(i + 1);
            if let Some(e) = est {
                if e[i] > -80.0 {
                    let y = g.lad_y(e[i]);
                    let c = t.text.with_alpha(0.6);
                    s.fill_round(Rect::new(x - 14.0, y, 28.0, 592.0 - y), 2.0, c, c, true);
                }
            }
            let v = cx.v(H2 + i);
            let off = v <= H_OFF_DB + 0.05;
            let y = g.lad_y(v).min(586.0);
            let c = if off { t.accent.with_alpha(0.35) } else { t.accent };
            s.fill_round(Rect::new(x - 18.0, y - 5.0, 36.0, 10.0), 3.0, c, c, true);
            let label = if off { "OFF".to_string() } else { format!("{v:.0}") };
            s.text(Point::new(x, 610.0), label, if off { t.dim } else { t.text }, 8.5, true);
        }
        s.text(Point::new(x0, 610.0), "0", t.dim, 8.5, true);
    }

    fn paint_strip(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let g = Geo::new(cx);
        let lanes = g.lanes();
        let edges = [(SRC_LOW, SRC_HIGH), (HARM_LOW, HARM_HIGH)];
        for (k, lane) in lanes.iter().enumerate() {
            let axis = Axis::new(*lane, 12.0);
            s.clip_round(kit::rect(*lane), 3.0);
            if k == 0 {
                paint_spectrum(s, &axis, &self.spectra.pre, t.in_fill, t.in_fill_alpha * 0.6, true);
            } else if cx.on(LISTEN) {
                paint_spectrum(s, &axis, &self.spectra.post, t.out_fill, t.out_fill_alpha * 0.8, true);
            }
            let (lo, hi) = (cx.v(edges[k].0), cx.v(edges[k].1));
            let y = |db: f32| lane.1 + 6.0 + (lane.3 - 6.0) * (-db).clamp(0.0, 24.0) / 24.0;
            let pts: Vec<Point> = (0..=96).map(|i| {
                let x = PX0 + (PX1 - PX0) * i as f32 / 96.0;
                Point::new(x, y(band_db(x_f(x), lo, hi)))
            }).collect();
            kit::area(s, &pts, lane.1 + lane.3, t.accent.with_alpha(0.10));
            kit::curve(s, pts, t.accent, 2.0);
            let (xl, xh) = (Self::edge_x(cx, edges[k].0, true), Self::edge_x(cx, edges[k].1, false));
            hatch(s, (PX0, lane.1, xl - PX0, lane.3), t.dim.with_alpha(0.35), 6.0);
            hatch(s, (xh, lane.1, PX1 - xh, lane.3), t.dim.with_alpha(0.35), 6.0);
            s.pop_clip();
            for (x, local, low) in [(xl, edges[k].0, true), (xh, edges[k].1, false)] {
                let gy = lane.1 + lane.3 * 0.5;
                s.fill_round(Rect::new(x - 3.0, gy - 7.0, 6.0, 14.0), 3.0, t.accent, t.accent, true);
                let label = if low { low_text(cx.v(local)) } else { high_text(cx.v(local)) };
                let lx = if low { (x + 6.0).min(PX1 - 50.0) } else { x - 6.0 - kit_width(&label) };
                s.text(Point::new(lx.max(PX0 + 2.0), lane.1 + lane.3 - 8.0), label, t.dim, 8.0, false);
            }
        }
        if cx.on(LISTEN) {
            warn_chip(s, t, g.chip(), "! LISTEN");
        }
    }
}

/// A label on a plain patch of the plot background, so lines under it do not cross it.
fn backed(s: &mut Scene, cx: &Ctx, x: f32, y: f32, label: &str, c: openplugin::dri::goodlookindri::color::Color, centred: bool) {
    let w = kit_width(label) * 8.5 / 8.0 + 6.0;
    let x0 = if centred { x - w * 0.5 } else { x - 3.0 };
    let bg = cx.t.plot_bg.with_alpha(0.9);
    s.fill_round(Rect::new(x0, y - 7.0, w, 14.0), 3.0, bg, bg, true);
    s.text(Point::new(if centred { x } else { x0 + 3.0 }, y), label, c, 8.5, centred);
}

fn kit_width(label: &str) -> f32 {
    openplugin::dri::goodlookindri_shell::text::width(label, 8.0)
}

impl Page for Harm {
    fn has_more(&self) -> bool {
        true
    }

    fn wants_audio(&self) -> bool {
        true
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        matches!(local, FAMILY | LISTEN)
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let hy = cx.zones.header.1;
        let p = |i: usize| cx.p(i);
        let g = Geo::new(cx);
        let mut v = vec![
            Cell::segmented(p(FAMILY), "CURVE", (420.0, hy + 8.0, 252.0, 24.0)).keys(&["POLYNOMIAL", "SATURATOR", "TAPE KNEE"]).tip("The curve family the harmonics are built on"),
            Cell::segmented(p(CHANNELS), "CHANNELS", (680.0, hy + 8.0, 192.0, 24.0)).keys(&["STEREO", "M/S", "MID", "SIDE"]),
        ];
        for i in 0..4 {
            v.push(Cell::grip(p(H2 + i), g.handle_grip(i)).tip(format!("H{}: drag the handle; double-click to type", i + 2)));
        }
        if cx.more {
            v.push(Cell::knob(Size::Small, p(TRIM), "TRIM", 136.0, 644.0).format(db_text).tip("Output trim after Auto Gain, down only"));
            v.push(Cell::dropdown(p(ANTI_ALIAS), "ANTI-ALIAS", (248.0, 664.0, 180.0, 24.0)));
            return v;
        }
        let listen = if cx.on(LISTEN) { "! LISTEN" } else { "LISTEN" };
        v.extend([
            Cell::knob(Size::Big, p(H2), "H2", 120.0, 636.0).format(h_text),
            Cell::knob(Size::Big, p(H2 + 1), "H3", 240.0, 636.0).format(h_text),
            Cell::knob(Size::Big, p(H2 + 2), "H4", 360.0, 636.0).format(h_text),
            Cell::knob(Size::Big, p(H2 + 3), "H5", 480.0, 636.0).format(h_text),
            Cell::knob(Size::Big, p(DRIVE), "DRIVE", 600.0, 636.0).format(db_text).tip("Level into the curve: sets where the reference level sits"),
            Cell::knob(Size::Mini, p(BIAS), "BIAS", 728.0, 636.0).format(pct_text).dim(cx.choice(FAMILY) == POLYNOMIAL).tip("Asymmetry of the curve (not in POLYNOMIAL)"),
            Cell::knob(Size::Mini, p(MIX), "MIX", 784.0, 636.0).format(pct_text),
            Cell::pill(p(LISTEN), listen, (728.0, 719.0, 80.0, 22.0)).tip("Harmonics only. Do not print with it on"),
            Cell::pill(p(AUTO_GAIN), "AUTO GAIN", (812.0, 719.0, 84.0, 22.0)),
            Cell::knob(Size::Mini, p(SRC_LOW), "SRC LOW", 912.0, 636.0).format(low_text).tip("Low cut of what feeds the curve"),
            Cell::knob(Size::Mini, p(SRC_HIGH), "SRC HIGH", 968.0, 636.0).format(high_text).tip("High cut of what feeds the curve"),
            Cell::knob(Size::Mini, p(HARM_LOW), "HARM LOW", 912.0, 700.0).format(low_text).tip("Low cut of the added harmonics"),
            Cell::knob(Size::Mini, p(HARM_HIGH), "HARM HIGH", 968.0, 700.0).format(high_text).tip("High cut of the added harmonics"),
        ]);
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let g = Geo::new(cx);
        kit::plot(s, t, CURVE);
        for k in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
            let c = if k == 0.0 { t.grid_strong } else { t.grid };
            s.line(Point::new(cu_x(k), CURVE.1), Point::new(cu_x(k), CURVE.1 + CURVE.3), c, 1.0);
            s.line(Point::new(CURVE.0, cu_y(k)), Point::new(CURVE.0 + CURVE.2, cu_y(k)), c, 1.0);
        }
        let r = g.ladder();
        kit::plot(s, t, r);
        for db in [0.0f32, -20.0, -40.0, -60.0, -80.0] {
            let y = g.lad_y(db);
            s.line(Point::new(380.0, y), Point::new(r.0 + r.2 - 8.0, y), t.grid, 1.0);
            kit::text_right(s, 374.0, y, &format!("{db:.0}"), t.dim, 8.0);
        }
        let ym = g.lad_y(H_MAX_DB);
        kit::dashed_h(s, 380.0, r.0 + r.2 - 8.0, ym, 4.0, 3.0, t.dim, 1.0);
        kit::text_right(s, r.0 + r.2 - 8.0, ym - 7.0, "max", t.dim, 8.0);
        for (k, l) in ["F", "H2", "H3", "H4", "H5"].iter().enumerate() {
            s.text(Point::new(Geo::col(k), 599.0), *l, t.dim, 8.5, true);
        }
        let st = g.strip();
        kit::plot(s, t, st);
        for (lane, label) in g.lanes().iter().zip(["FEEDS THE CURVE", "HARMONICS KEPT"]) {
            s.fill_round(kit::rect(*lane), 3.0, t.inset, t.inset, true);
            for f in [100.0f32, 1000.0, 10000.0] {
                s.line(Point::new(f_x(f), lane.1), Point::new(f_x(f), lane.1 + lane.3), t.grid, 1.0);
            }
            s.text(Point::new(lane.0, lane.1 - 7.0), label, t.dim, 8.0, false);
        }
        for f in [20.0f32, 100.0, 1000.0, 10000.0] {
            s.text(Point::new(f_x(f).min(PX1 - 8.0), 604.0), hz_short(f), t.dim, 8.5, true);
        }
        if cx.more {
            s.text(Point::new(136.0, 632.0), "AFTER AUTO GAIN", t.dim, 9.0, false);
            s.text(Point::new(248.0, 648.0), "ANTI-ALIAS", t.dim, 9.0, false);
        } else {
            sub_caption(s, t, (600.0, 636.0, 120.0, 124.0), "sets the reference level");
            for x in [724.0, 904.0] {
                s.line(Point::new(x, 630.0), Point::new(x, 764.0), t.border, 1.0);
            }
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        self.paint_curve(s, cx);
        self.paint_ladder(s, cx, live);
        self.paint_strip(s, cx);
        if cx.more {
            let aa = cx.choice(ANTI_ALIAS);
            s.text(Point::new(MORE_TEXT.0, MORE_TEXT.1 + 10.0), format!("latency {} smp", latency_of(aa)), t.dim, 9.5, false);
            if aa == AA_NONE {
                s.text(Point::new(MORE_TEXT.0, MORE_TEXT.1 + 26.0), "None: aliasing is not suppressed", t.warm, 9.5, false);
            }
        } else if cx.choice(FAMILY) != POLYNOMIAL {
            s.text(Point::new(NOTE.0 + NOTE.2 * 0.5, NOTE.1 + 8.0), "added on top of the curve's own harmonics", t.dim, 8.5, true);
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let g = Geo::new(cx);
        vec![CURVE, g.ladder(), g.strip(), if cx.more { MORE_TEXT } else { NOTE }]
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max};
        &[Max, Max, Max, Max, Max, Last, Max, Last, Max, Max]
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.on(LISTEN)
    }

    fn tick(&mut self, _cx: &Ctx, live: &Live) -> bool {
        if let Some(a) = live.audio {
            self.spectra.update(a, Listen::Both, live.sample_rate, live.dt);
        }
        if live.view.len() >= vw::LEN {
            let v = live.view;
            self.reach = if v[vw::DRIVE_DB] <= vw::FLOOR_DB + 1.0 { 0.0 } else { 10f32.powf(v[vw::DRIVE_DB] / 20.0) };
            if self.reach >= self.hold {
                self.hold = self.reach;
                self.hold_age = 0.0;
            } else {
                self.hold_age += live.dt;
                if self.hold_age > 1.5 {
                    self.hold = self.reach;
                }
            }
            self.tile_age += live.dt;
            if self.tile_age >= 0.1 || self.tiles.is_none() {
                self.tile_age = 0.0;
                self.tiles = Some([v[vw::THD_DB], v[vw::ADDED_DB], v[vw::AUTO_GAIN_DB]]);
            }
        }
        true
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let g = Geo::new(cx);
        let mut v = vec![
            tip(CURVE, "The curve (thin), what it adds magnified (thick) and how far along it the programme reaches; REF is the level the H settings hold at"),
            tip(g.ladder(), "Each harmonic below the fundamental: the handle is the setting, the fill an estimate for the programme right now"),
            tip(g.strip(), "Top: the band that feeds the curve. Bottom: the band of harmonics kept. Drag an edge; to the end is OFF; double-click resets"),
        ];
        if cx.on(LISTEN) {
            v.push(tip(g.chip(), "Listen is on: you hear only the added harmonics. Click to switch it off"));
        }
        v
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        if let Some(d) = self.drag {
            let (x, up) = match ev {
                Pointer::Move { x, .. } => (x, false),
                Pointer::Up { x, .. } => (x, true),
                _ => return Response::TAKEN,
            };
            let f = if d.low && x <= PX0 + 2.0 {
                LOW_OFF_HZ
            } else if !d.low && x >= PX1 - 2.0 {
                HIGH_OFF_HZ
            } else {
                x_f(x.clamp(PX0, PX1))
            };
            io.perform(cx.p(d.local), f);
            if up {
                io.end(cx.p(d.local));
                self.drag = None;
            }
            return Response::TAKEN;
        }
        let Pointer::Down { x, y, clicks } = ev else { return Response::NONE };
        let g = Geo::new(cx);
        if cx.on(LISTEN) && kit::inside(g.chip(), x, y) {
            io.set(cx.p(LISTEN), 0.0);
            return Response::TAKEN;
        }
        let edges = [(SRC_LOW, SRC_HIGH), (HARM_LOW, HARM_HIGH)];
        for (lane, (lo, hi)) in g.lanes().iter().zip(edges) {
            if y < lane.1 || y > lane.1 + lane.3 || x < PX0 - 8.0 || x > PX1 + 8.0 {
                continue;
            }
            let (xl, xh) = (Self::edge_x(cx, lo, true), Self::edge_x(cx, hi, false));
            // The nearer edge within 7 pt; on a tie the one that can move inward.
            let pick = match ((x - xl).abs() <= 7.0, (x - xh).abs() <= 7.0) {
                (true, true) => Some(if (x - xl).abs() <= (x - xh).abs() { (lo, true) } else { (hi, false) }),
                (true, false) => Some((lo, true)),
                (false, true) => Some((hi, false)),
                _ => None,
            };
            let Some((local, low)) = pick else { return Response::TAKEN };
            let p = cx.p(local);
            if clicks >= 2 {
                io.set(p, INFO.params[p].default);
            } else {
                io.begin(p);
                self.drag = Some(EdgeDrag { local, low });
            }
            return Response::TAKEN;
        }
        Response::NONE
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_harm() {
        use harmonics::view as vw;
        let view = |k: usize| {
            let ph = k as f32 / 30.0;
            let drive = -4.0 + 3.0 * (ph * 1.7).sin();
            let mut v = vec![0.0f32; vw::LEN];
            v[vw::H2_DB] = -33.0 + drive;
            v[vw::H3_DB] = -46.0 + 2.0 * drive;
            v[vw::H4_DB] = -78.0;
            v[vw::H5_DB] = -120.0;
            v[vw::THD_DB] = -32.5 + drive;
            v[vw::ADDED_DB] = -31.0;
            v[vw::DRIVE_DB] = drive;
            v[vw::AUTO_GAIN_DB] = -0.3;
            v[vw::IN_PEAK_DB] = -6.0;
            v[vw::OUT_PEAK_DB] = -6.3;
            v
        };
        super::super::hcomp::dump::dump("harm", &view, &[("family", 1.0), ("h2", -30.0), ("h3", -40.0), ("h4", -62.0), ("srcLow", 80.0), ("srcHigh", 9000.0), ("harmLow", 400.0), ("listen", 1.0), ("channels", 2.0), ("antiAlias", 4.0)]);
    }
}
