//! Dual Compressor (`crates/comp-dual`): transfer curve, GR meters and a band view; two
//! parameter sets for both channels, left / right or mid / side. See `design/compressors.md`
//! section 2.

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

use comp_dual::{index as ix, view as vw};

use super::bands::common::*;
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::INFO;

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    if count != ix::COUNT {
        return Box::new(super::Generic::new(count, &[]));
    }
    Box::new(Dual::default())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let v = |i: usize| values.get(i).copied().unwrap_or(0.0);
    (v(ix::KEY_SOURCE) >= 0.5, v(ix::OUTPUT_GAIN) > 0.0)
}

const CURVE: R = (120.0, 416.0, 200.0, 200.0);
const METERS: [R; 3] = [(328.0, 416.0, 36.0, 176.0), (376.0, 416.0, 36.0, 176.0), (424.0, 416.0, 36.0, 176.0)];
const VIEW: R = (476.0, 416.0, 604.0, 200.0);
const PLOT: R = (508.0, 424.0, 540.0, 168.0);
const CHIP: R = (944.0, 428.0, 100.0, 22.0);
const BIG_X: [f32; 5] = [120.0, 240.0, 360.0, 480.0, 600.0];
const BIG_OFF: [usize; 5] = [ix::THRESHOLD, ix::RATIO, ix::ATTACK, ix::RELEASE_FAST, ix::MAKEUP];
const BIG_LABEL: [&str; 5] = ["THRESHOLD", "RATIO", "ATTACK", "RELEASE", "MAKE-UP"];
const FILTER: R = (728.0, 632.0, 224.0, 24.0);
const FILTER_KEYS: &[&str] = &["LOW", "MID", "HIGH", "FULL"];
const FREQ: R = (728.0, 662.0, 56.0, 60.0);
const WIDTH: R = (784.0, 662.0, 56.0, 60.0);
const KNEE: R = (840.0, 662.0, 56.0, 60.0);
const AUTO_REL: R = (728.0, 731.0, 56.0, 22.0);
const MORE_X: [f32; 5] = [136.0, 216.0, 296.0, 376.0, 456.0];
const MORE_OFF: [usize; 5] = [ix::RELEASE_DELAY, ix::RELEASE_FAST, ix::RELEASE_AVERAGE, ix::RELEASE_SLOW, ix::AUTO_RELEASE_TIME];
const MORE_LABEL: [&str; 5] = ["HOLD", "FAST", "AVERAGE", "SLOW", "AUTO TIME"];
const TRIM: R = (728.0, 644.0, 80.0, 84.0);
const TRIM_BOX: R = (736.0, 706.0, 64.0, 18.0);
const COPY: R = (336.0, 738.0, 120.0, 22.0);
const SWAP: R = (464.0, 738.0, 96.0, 22.0);
const RESET: R = (568.0, 738.0, 96.0, 22.0);

fn tabs(hy: f32) -> [R; 2] {
    [(590.0, hy + 8.0, 66.0, 24.0), (656.0, hy + 8.0, 66.0, 24.0)]
}

fn link_rect(hy: f32) -> R {
    (730.0, hy + 9.0, 56.0, 22.0)
}

fn max_rect(i: usize) -> R {
    (METERS[i].0 - 4.0, 596.0, 44.0, 18.0)
}

fn ratio_text(v: f32) -> String {
    if v >= 999.0 {
        "∞:1".into()
    } else if v >= 1.0 {
        format!("{v:.1}:1")
    } else {
        format!("1:{:.1} UP", 1.0 / v.max(1e-3))
    }
}

fn lookahead_text(v: f32) -> String {
    if v <= 0.0201 {
        "OFF".into()
    } else {
        time_text(v)
    }
}

fn octaves_text(v: f32) -> String {
    format!("{v:.2} oct")
}

fn empty_text(_: f32) -> String {
    String::new()
}

/// The complementary error function (Abramowitz and Stegun 7.1.26, 1.5e-7).
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * z);
    let y = t * (0.254_829_592 + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429)))) * (-z * z).exp();
    if x >= 0.0 {
        y
    } else {
        2.0 - y
    }
}

fn low_share(f: f64, edge: f64) -> f64 {
    0.5 * erfc((f - edge) / comp_dual::edge_width_hz(edge))
}

/// The band's share of a tone at `f`, as the split filter has it.
fn share(ftype: usize, fc: f32, bw: f32, f: f32) -> f32 {
    let (f, fc) = (f as f64, fc as f64 * 0.98);
    (match ftype {
        0 => low_share(f, fc),
        1 => {
            let lo = (fc * 2f64.powf(-0.5 * bw as f64)).max(41.0 * 0.98);
            let hi = fc * 2f64.powf(0.5 * bw as f64);
            (low_share(f, hi) - low_share(f, lo.min(hi))).max(0.0)
        }
        2 => 1.0 - low_share(f, fc),
        _ => 1.0,
    }) as f32
}

fn vx(f: f32) -> f32 {
    PLOT.0 + 180.0 * (f.max(1.0) / 20.0).log10()
}

fn xv(x: f32) -> f32 {
    20.0 * 10f32.powf((x - PLOT.0) / 180.0)
}

fn vy(g: f32) -> f32 {
    (466.0 - 5.25 * g).clamp(PLOT.1, PLOT.1 + PLOT.3)
}

#[derive(Clone, Debug)]
enum Drag {
    Thr { params: Vec<usize>, last: f32, v: f32 },
    Ratio { params: Vec<usize>, last: f32, n: f32 },
    Freq { params: Vec<usize>, last: f32, f: f32 },
    Width { params: Vec<usize>, centre: f32 },
    Gang(GangDrag),
}

pub struct Dual {
    /// The set the knobs edit.
    set: usize,
    link: bool,
    drag: Option<Drag>,
    hold: [f32; 3],
    age: [f32; 3],
    max: [f32; 3],
}

impl Default for Dual {
    fn default() -> Self {
        Self { set: 0, link: false, drag: None, hold: [0.0; 3], age: [0.0; 3], max: [0.0; 3] }
    }
}

fn fine(io: &Io) -> f32 {
    if io.modifiers().shift {
        0.1
    } else {
        1.0
    }
}

impl Dual {
    fn sl(set: usize, off: usize) -> usize {
        ix::SET_BASE + set * ix::SET_LEN + off
    }

    fn sv(cx: &Ctx, set: usize, off: usize) -> f32 {
        cx.v(Self::sl(set, off))
    }

    fn ganged(cx: &Ctx) -> bool {
        cx.choice(ix::CHANNEL_MODE) == 0
    }

    fn linked(&self, cx: &Ctx) -> bool {
        self.link && !Self::ganged(cx)
    }

    /// Host indices an edit of the edited set's `off` writes: both sets while LINK is on.
    fn targets(&self, cx: &Ctx, off: usize) -> Vec<usize> {
        let mut v = vec![cx.p(Self::sl(self.set, off))];
        if self.linked(cx) {
            v.push(cx.p(Self::sl(1 - self.set, off)));
        }
        v
    }

    fn view(live: &Live, i: usize) -> Option<f32> {
        (live.view.len() >= vw::LEN).then(|| live.view[i])
    }

    /// Make-up of a set in dB, auto make-up included.
    fn makeup(cx: &Ctx, set: usize) -> f32 {
        let mut m = Self::sv(cx, set, ix::MAKEUP);
        if cx.on(ix::AUTO_MAKEUP) {
            m += comp_dual::auto_makeup_db(Self::sv(cx, set, ix::THRESHOLD) as f64, Self::sv(cx, set, ix::RATIO) as f64) as f32;
        }
        m
    }

    fn knee_h(cx: &Ctx, set: usize) -> f32 {
        comp_dual::knee_half_width_db(Self::sv(cx, set, ix::THRESHOLD) as f64, Self::sv(cx, set, ix::SOFT_KNEE) as f64) as f32
    }

    fn gain(cx: &Ctx, set: usize, level: f32) -> f32 {
        comp_dual::static_gain_db(level as f64, Self::sv(cx, set, ix::THRESHOLD) as f64, Self::knee_h(cx, set) as f64, Self::sv(cx, set, ix::RATIO) as f64) as f32
    }

    fn curve_out(cx: &Ctx, set: usize, level: f32) -> f32 {
        level + Self::gain(cx, set, level) + Self::makeup(cx, set)
    }

    /// The channels set `set` drives (ganged: the driving set drives both).
    fn channels(cx: &Ctx, set: usize) -> Vec<usize> {
        if Self::ganged(cx) {
            if cx.choice(ix::GANGED_SET) == set {
                vec![0]
            } else {
                vec![]
            }
        } else {
            vec![set]
        }
    }

    fn gangs(&self, cx: &Ctx) -> Vec<Gang> {
        let mut g = Vec::new();
        let both = |off: usize| vec![cx.p(Self::sl(self.set, off)), cx.p(Self::sl(1 - self.set, off))];
        let ftype = cx.choice(Self::sl(self.set, ix::FILTER_TYPE));
        let auto = cx.on(Self::sl(self.set, ix::AUTO_RELEASE));
        if cx.more {
            if self.linked(cx) {
                for k in 0..5 {
                    let faded = match k {
                        2 | 3 => auto,
                        4 => !auto,
                        _ => false,
                    };
                    g.push(Gang::knob(Size::Small, (MORE_X[k], 644.0, 80.0, 84.0), both(MORE_OFF[k]), MORE_LABEL[k], false).format(Some(time_text)).faded(faded));
                }
            }
            g.push(Gang::knob(Size::Small, TRIM, vec![cx.p(ix::OUTPUT_GAIN)], "POST TRIM", false).range(-60.0, 0.0));
            return g;
        }
        if !self.linked(cx) {
            return g;
        }
        for k in 0..5 {
            let f: Option<fn(f32) -> String> = match k {
                1 => Some(ratio_text),
                2 | 3 => Some(time_text),
                _ => None,
            };
            g.push(Gang::knob(Size::Big, (BIG_X[k], 636.0, 120.0, 124.0), both(BIG_OFF[k]), BIG_LABEL[k], false).format(f));
        }
        g.push(Gang::new(GKind::Seg(FILTER_KEYS), FILTER, both(ix::FILTER_TYPE), "BAND"));
        g.push(Gang::knob(Size::Mini, FREQ, both(ix::CENTER_FREQ), "FREQ", false).faded(ftype == 3));
        g.push(Gang::knob(Size::Mini, WIDTH, both(ix::BANDWIDTH), "WIDTH", false).format(Some(octaves_text)).faded(ftype != 1));
        g.push(Gang::knob(Size::Mini, KNEE, both(ix::SOFT_KNEE), "KNEE", false));
        g.push(Gang::new(GKind::Pill, AUTO_REL, both(ix::AUTO_RELEASE), "AUTO REL"));
        g
    }

    fn set_cells(&self, cx: &Ctx, v: &mut Vec<Cell>) {
        let e = self.set;
        let p = |off: usize| cx.p(Self::sl(e, off));
        let ftype = cx.choice(Self::sl(e, ix::FILTER_TYPE));
        for k in 0..5 {
            let mut c = Cell::knob(Size::Big, p(BIG_OFF[k]), BIG_LABEL[k], BIG_X[k], 636.0);
            c = match k {
                1 => c.format(ratio_text).tip("Below 1:1 the gain goes up"),
                2 | 3 => c.format(time_text),
                _ => c,
            };
            v.push(c);
        }
        v.push(Cell::segmented(p(ix::FILTER_TYPE), "BAND", FILTER).keys(FILTER_KEYS).tip("Compress only a band of the spectrum, or the full band"));
        v.push(Cell::knob(Size::Mini, p(ix::CENTER_FREQ), "FREQ", FREQ.0, FREQ.1).dim(ftype == 3));
        v.push(Cell::knob(Size::Mini, p(ix::BANDWIDTH), "WIDTH", WIDTH.0, WIDTH.1).format(octaves_text).dim(ftype != 1).tip("Band width in octaves (MID)"));
        v.push(Cell::knob(Size::Mini, p(ix::SOFT_KNEE), "KNEE", KNEE.0, KNEE.1));
        v.push(Cell::pill(p(ix::AUTO_RELEASE), "AUTO REL", AUTO_REL));
    }

    fn reset_holds(&mut self) {
        self.hold = [0.0; 3];
        self.age = [0.0; 3];
        self.max = [0.0; 3];
    }

    // ---- painting ----

    fn paint_curve(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let m = Map::square(CURVE);
        s.clip_round(kit::rect(CURVE), 4.0);
        let e = self.set;
        // The soft-knee zone.
        let thr = Self::sv(cx, e, ix::THRESHOLD);
        let h = Self::knee_h(cx, e);
        if h > 0.1 {
            let (a, b) = (m.x((thr - h).max(-60.0)), m.x((thr + h).min(6.0)));
            s.fill_rect(Rect::new(a, CURVE.1, b - a, CURVE.3), t.accent.with_alpha(0.08), t.accent.with_alpha(0.08), true);
        }
        let pts = |set: usize| -> Vec<Point> {
            let mut v = Vec::with_capacity(64);
            let mut l = -60.0;
            while l <= 0.01 {
                v.push(Point::new(m.x(l), m.y(Self::curve_out(cx, set, l).clamp(-61.0, 0.5))));
                l += 1.0;
            }
            v
        };
        if !Self::ganged(cx) {
            kit::dashed(s, &pts(1 - e), 4.0, 3.0, t.text.with_alpha(0.5), 1.0);
        }
        let mine = pts(e);
        let clipped = mine.iter().any(|p| p.y <= CURVE.1 + 0.6);
        kit::curve(s, mine, t.accent, 2.0);
        // The knee handle.
        let kt = thr.clamp(-60.0, 0.0);
        let (kx, ky) = (m.x(kt), m.y(Self::curve_out(cx, e, kt).clamp(-60.0, 0.0)));
        s.fill_ellipse_solid(Scene::ellipse_at(Point::new(kx, ky), 4.5), t.panel);
        s.stroke_ellipse(Scene::ellipse_at(Point::new(kx, ky), 4.5), t.accent, 1.5);
        // The operating dots.
        let keyed = Self::view(live, vw::KEYED).is_some_and(|v| v >= 0.5);
        let mut det_text = None;
        for set in 0..2 {
            for c in Self::channels(cx, set) {
                let (Some(det), Some(g)) = (Self::view(live, vw::DETECTOR + c), Self::view(live, vw::GAIN + c)) else { continue };
                if det < -60.0 {
                    continue;
                }
                let mk = Self::view(live, vw::MAKEUP + set).unwrap_or_else(|| Self::makeup(cx, set));
                let p = Point::new(m.x(det.min(0.0)), m.y((det + g + mk).clamp(-60.0, 0.0)));
                let e_ = Scene::ellipse_at(p, 4.0);
                s.fill_ellipse_solid(e_, if set == e { t.accent } else { t.text.with_alpha(0.6) });
                if keyed {
                    s.stroke_ellipse(Scene::ellipse_at(p, 5.0), t.warm, 1.5);
                }
                if set == e || det_text.is_none() {
                    det_text = Some(det);
                }
            }
        }
        s.pop_clip();
        if clipped {
            s.text(Point::new(CURVE.0 + CURVE.2 - 10.0, CURVE.1 + 10.0), "▲", t.accent, 8.0, true);
        }
        if let Some(d) = det_text {
            s.text(Point::new(128.0, 432.0), format!("DET {d:.1}"), if keyed { t.warm } else { t.dim }, 9.0, false);
        }
    }

    fn paint_meters(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let values = [
            Self::view(live, vw::GAIN).map_or(0.0, |g| (-g).max(0.0)),
            Self::view(live, vw::GAIN + 1).map_or(0.0, |g| (-g).max(0.0)),
            Self::view(live, vw::LIMIT).unwrap_or(0.0),
        ];
        for i in 0..3 {
            kit::gr_meter(s, t, METERS[i], values[i], self.hold[i], 20.0);
            if i == 2 {
                let d = (values[i] / 20.0).clamp(0.0, 1.0) * METERS[i].3;
                hatch(s, (METERS[i].0, METERS[i].1, 14.0, d), t.panel.with_alpha(0.6), 4.0);
            }
            max_box(s, t, max_rect(i), self.max[i]);
        }
        let labels = match cx.choice(ix::CHANNEL_MODE) {
            2 => ["M", "S"],
            _ => ["L", "R"],
        };
        for (i, l) in labels.iter().enumerate() {
            s.text(Point::new(METERS[i].0 + 28.0, 588.0), *l, t.text, 8.5, true);
        }
        s.text(Point::new(METERS[2].0 + 28.0, 588.0), "LIM", t.text, 7.5, true);
    }

    fn paint_band(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let e = self.set;
        let ft = |set: usize| cx.choice(Self::sl(set, ix::FILTER_TYPE));
        let shape = |set: usize, f: f32| share(ft(set), Self::sv(cx, set, ix::CENTER_FREQ), Self::sv(cx, set, ix::BANDWIDTH), f);
        let bottom = PLOT.1 + PLOT.3;
        let xs: Vec<f32> = (0..=135).map(|i| PLOT.0 + i as f32 * 4.0).collect();
        s.clip_round(kit::rect(PLOT), 0.0);
        // LISTEN: what is outside the band is not heard.
        if cx.on(ix::BAND_SOLO) && ft(e) != 3 {
            let mut start: Option<f32> = None;
            for &x in &xs {
                let out = shape(e, xv(x)) < 0.5;
                match (out, start) {
                    (true, None) => start = Some(x),
                    (false, Some(a)) => {
                        hatch(s, (a, PLOT.1, x - a, PLOT.3), t.warm.with_alpha(0.55), 6.0);
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(a) = start {
                hatch(s, (a, PLOT.1, PLOT.0 + PLOT.2 - a, PLOT.3), t.warm.with_alpha(0.55), 6.0);
            }
        }
        // The other set's band, dashed.
        if !Self::ganged(cx) && ft(1 - e) != 3 {
            let pts: Vec<Point> = xs.iter().map(|&x| Point::new(x, bottom - shape(1 - e, xv(x)) * PLOT.3 * 0.9)).collect();
            kit::dashed(s, &pts, 4.0, 3.0, t.text.with_alpha(0.5), 1.0);
        }
        // This set's band.
        if ft(e) != 3 {
            let pts: Vec<Point> = xs.iter().map(|&x| Point::new(x, bottom - shape(e, xv(x)) * PLOT.3 * 0.9)).collect();
            kit::area(s, &pts, bottom, t.accent.with_alpha(0.1));
            let filtered_key = cx.on(ix::KEY_SOURCE) && cx.on(ix::KEY_FILTER);
            kit::curve(s, pts, if filtered_key { t.warm } else { t.accent.with_alpha(0.55) }, 1.0);
        }
        // The response line: make-up, dipping by the reduction where the band is.
        let c = if Self::ganged(cx) { 0 } else { e };
        let driving = !Self::ganged(cx) || cx.choice(ix::GANGED_SET) == e;
        let g = if driving { Self::view(live, vw::GAIN + c).unwrap_or(0.0) } else { 0.0 };
        let mk = Self::view(live, vw::MAKEUP + e).unwrap_or_else(|| Self::makeup(cx, e));
        let line: Vec<Point> = xs.iter().map(|&x| Point::new(x, vy(mk + g * shape(e, xv(x))))).collect();
        kit::curve(s, line, t.text, 2.0);
        s.pop_clip();
        // Context line.
        let ext = cx.on(ix::KEY_SOURCE);
        let context = if cx.on(ix::BAND_SOLO) {
            Some("LISTEN is on")
        } else if ext && !cx.key_present {
            Some("External key selected, no key connected")
        } else if ft(0) != 3 || ft(1) != 3 {
            Some("Band selected: latency +26 ms")
        } else {
            None
        };
        if let Some(c) = context {
            s.text(Point::new(516.0, 584.0), c, if c.starts_with("LISTEN") { t.warm } else { t.dim }, 9.0, false);
        }
        let trim = cx.v(ix::OUTPUT_GAIN);
        if trim > 0.0 {
            warn_chip(s, t, CHIP, &format!("! POST TRIM +{trim:.1}"));
        }
    }

    fn paint_header(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let hy = cx.zones.header.1;
        let [a, b] = tabs(hy);
        let r = (a.0, a.1, a.2 + b.2, a.3);
        s.fill_rect(kit::rect((r.0 - 2.0, r.1 - 2.0, r.2 + 4.0, r.3 + 4.0)), t.panel, t.panel, true);
        kit::inset(s, t, r, 6.0);
        let labels = match cx.choice(ix::CHANNEL_MODE) {
            0 => ["SET 1", "SET 2"],
            1 => ["L · 1", "R · 2"],
            _ => ["MID · 1", "SIDE · 2"],
        };
        let driving = cx.choice(ix::GANGED_SET);
        for (i, k) in [a, b].into_iter().enumerate() {
            let key = Rect::new(k.0 + 2.0, k.1 + 2.0, k.2 - 4.0, k.3 - 4.0);
            let on = i == self.set;
            if on {
                s.fill_round(key, 4.0, t.accent, t.accent, true);
            }
            s.text(key.center(), labels[i], if on { t.on_accent } else { t.dim }, 9.0, true);
            if Self::ganged(cx) && i == driving {
                s.fill_ellipse_solid(Scene::ellipse_at(Point::new(key.x + 6.0, key.y + key.h * 0.5), 2.0), if on { t.on_accent } else { t.accent });
            }
        }
        let l = link_rect(hy);
        s.fill_rect(kit::rect((l.0 - 2.0, l.1 - 2.0, l.2 + 4.0, l.3 + 4.0)), t.panel, t.panel, true);
        if !Self::ganged(cx) {
            pill(s, t, l, "LINK", self.link);
        }
    }

    fn paint_controls(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        if cx.more {
            let dir = if self.set == 0 { "COPY SET 1 → 2" } else { "COPY SET 2 → 1" };
            kit::button(s, t, COPY, dir, false);
            kit::button(s, t, SWAP, "SWAP SETS", false);
            kit::button(s, t, RESET, "RESET SET", false);
        } else {
            sub_caption(s, t, (480.0, 636.0, 120.0, 124.0), "fast stage");
            if cx.choice(Self::sl(self.set, ix::FILTER_TYPE)) != 3 {
                sub_caption(s, t, (600.0, 636.0, 120.0, 124.0), "on the band");
            }
        }
        for g in self.gangs(cx) {
            g.paint(s, t, cx);
        }
    }

    fn control_rects(&self, cx: &Ctx) -> Vec<R> {
        if cx.more {
            let mut v = vec![(330.0, 734.0, 340.0, 30.0), TRIM];
            if self.linked(cx) {
                v.push((136.0, 644.0, 400.0, 84.0));
            }
            v
        } else {
            let mut v = vec![(480.0, 760.0, 240.0, 12.0)];
            if self.linked(cx) {
                v.push((120.0, 632.0, 840.0, 128.0));
            }
            v
        }
    }
}

impl Page for Dual {
    fn has_more(&self) -> bool {
        true
    }

    fn key_param(&self, cx: &Ctx) -> Option<usize> {
        cx.find("keySource")
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let hy = cx.zones.header.1;
        let mut v = vec![Cell::segmented(cx.p(ix::CHANNEL_MODE), "CHANNELS", (420.0, hy + 8.0, 162.0, 24.0)).keys(&["GANGED", "L / R", "M / S"]).tip("One set for both channels, a set per channel, or mid and side")];
        let e = self.set;
        let p = |off: usize| cx.p(Self::sl(e, off));
        if cx.more {
            if !self.linked(cx) {
                let auto = cx.on(Self::sl(e, ix::AUTO_RELEASE));
                for k in 0..5 {
                    let dim = match k {
                        2 | 3 => auto,
                        4 => !auto,
                        _ => false,
                    };
                    v.push(Cell::knob(Size::Small, p(MORE_OFF[k]), MORE_LABEL[k], MORE_X[k], 644.0).format(time_text).dim(dim));
                }
            }
            v.push(Cell::knob(Size::Small, cx.p(ix::RMS_TIME), "RMS TIME", 552.0, 644.0).format(time_text).dim(cx.choice(ix::DETECTOR) == 0));
            v.push(Cell::knob(Size::Small, cx.p(ix::LIMITER_GAIN), "LIMIT DRIVE", 648.0, 644.0));
            // The POST TRIM knob is the page's (its travel stops at 0 dB); this box keeps the
            // parameter a control for the host menu and assistive tools.
            v.push(Cell::value(cx.p(ix::OUTPUT_GAIN), "POST TRIM", TRIM_BOX).format(empty_text).dim(true).gui_range(-60.0, 0.0).tip("Post trim, after the safety limiter: drag the knob; it stops at 0 dB"));
            v.push(Cell::pill(cx.p(ix::LIMITER_ON), "SAFETY LIMITER", (824.0, 644.0, 136.0, 22.0)));
            v.push(Cell::dropdown(cx.p(ix::LIMITER_TYPE), "CEILING", (824.0, 674.0, 180.0, 24.0)));
            v.push(Cell::pill(cx.p(ix::KEY_LINK), "KEY LINK", (136.0, 738.0, 80.0, 22.0)).tip("Per channel: both channels take the deeper reduction"));
            v.push(Cell::pill(cx.p(ix::KEY_FILTER), "KEY FILTER", (224.0, 738.0, 88.0, 22.0)).tip("The external key passes the band filter"));
            v.push(Cell::segmented(cx.p(ix::GANGED_SET), "GANGED SET", (680.0, 737.0, 120.0, 24.0)).keys(&["SET 1", "SET 2"]).tip("The set that drives both channels in GANGED"));
            return v;
        }
        if !self.linked(cx) {
            self.set_cells(cx, &mut v);
        }
        v.push(Cell::pill(cx.p(ix::BAND_SOLO), "LISTEN", (960.0, 633.0, 60.0, 22.0)).tip("Hear the band alone"));
        v.push(Cell::knob(Size::Mini, cx.p(ix::LOOKAHEAD), "L-AHEAD", 896.0, 662.0).format(lookahead_text));
        v.push(Cell::knob(Size::Mini, cx.p(ix::INPUT_GAIN), "DRIVE", 952.0, 662.0));
        v.push(Cell::knob(Size::Mini, cx.p(ix::PARALLEL_MIX), "MIX", 1008.0, 662.0).dim(cx.choice(ix::PARALLEL_MODE) != 2).tip("Parallel mix (Blend)"));
        v.push(Cell::pill(cx.p(ix::AUTO_MAKEUP), "AUTO MK", (788.0, 731.0, 56.0, 22.0)).tip("Make-up that brings a full-scale input back to full scale"));
        v.push(Cell::segmented(cx.p(ix::DETECTOR), "DETECTOR", (848.0, 730.0, 96.0, 24.0)).keys(&["PEAK", "RMS"]));
        v.push(Cell::dropdown(cx.p(ix::PARALLEL_MODE), "PARALLEL", (948.0, 730.0, 132.0, 24.0)));
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        curve_frame(s, t, CURVE);
        for r in METERS {
            kit::gr_scale(s, t, r, 20.0);
        }
        kit::plot(s, t, VIEW);
        let mut f = 10f32;
        while f <= 20000.0 {
            for m in 1..10 {
                let ff = f * m as f32;
                if !(20.0..=20000.0).contains(&ff) {
                    continue;
                }
                let x = vx(ff);
                s.line(Point::new(x, PLOT.1), Point::new(x, PLOT.1 + PLOT.3), if m == 1 { t.grid_strong } else { t.grid }, 1.0);
                if matches!(m, 1 | 2 | 5) {
                    s.text(Point::new(x, 604.0), hz_short(ff), t.dim, 8.5, true);
                }
            }
            f *= 10.0;
        }
        for g in [6.0, 0.0, -6.0, -12.0, -18.0, -24.0] {
            let y = vy(g);
            s.line(Point::new(PLOT.0, y), Point::new(PLOT.0 + PLOT.2, y), if g == 0.0 { t.grid_strong } else { t.grid }, 1.0);
            kit::text_right(s, PLOT.0 - 4.0, y, &format!("{g:+.0}"), t.dim, 8.0);
        }
        if !cx.more {
            s.line(Point::new(724.0, 632.0), Point::new(724.0, 760.0), t.border, 1.0);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        self.paint_header(s, cx);
        self.paint_curve(s, cx, live);
        self.paint_meters(s, cx, live);
        self.paint_band(s, cx, live);
        self.paint_controls(s, cx);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let hy = cx.zones.header.1;
        let mut v = vec![CURVE, (324.0, 416.0, 144.0, 200.0), VIEW, (588.0, hy + 6.0, 200.0, 28.0)];
        v.extend(self.control_rects(cx));
        v
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last, Max, Min};
        &[Max, Max, Min, Min, Max, Last, Last, Last]
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        if matches!(local, ix::CHANNEL_MODE | ix::DETECTOR | ix::PARALLEL_MODE) {
            return true;
        }
        local >= ix::SET_BASE && matches!((local - ix::SET_BASE) % ix::SET_LEN, ix::FILTER_TYPE | ix::AUTO_RELEASE)
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.v(ix::OUTPUT_GAIN) > 0.0
    }

    fn tick(&mut self, _cx: &Ctx, live: &Live) -> bool {
        if live.view.len() >= vw::LEN {
            let values = [(-live.view[vw::GAIN]).max(0.0), (-live.view[vw::GAIN + 1]).max(0.0), live.view[vw::LIMIT]];
            for i in 0..3 {
                let r = values[i];
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
        }
        true
    }

    fn states(&self) -> usize {
        2
    }

    fn set_state(&mut self, i: usize) {
        self.set = i % 2;
    }

    fn menu(&self, _cx: &Ctx) -> Vec<String> {
        vec![format!("Copy set {} to set {}", self.set + 1, 2 - self.set), "Swap the sets".into(), format!("Reset set {}", self.set + 1)]
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        self.action(item, cx, io);
        Response::TAKEN
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let tip = |r: R, text: &str| Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() };
        let hy = cx.zones.header.1;
        let mut v = vec![
            tip(CURVE, "Drag the knee sideways for the threshold, the curve above it up or down for the ratio; the wheel near the knee sets its width"),
            tip(PLOT, "Drag the band sideways for its frequency, a MID band's edge for its width; the wheel sets the width; double-click resets"),
            tip(tabs(hy)[0], "The set the knobs edit; in GANGED also the set that drives both channels"),
        ];
        if !Self::ganged(cx) {
            v.push(tip(link_rect(hy), "While on, every set edit is written to both sets"));
        }
        if cx.v(ix::OUTPUT_GAIN) > 0.0 {
            v.push(tip(CHIP, "Post trim is above 0 dB, after the safety limiter: the output can pass the ceiling. Click for 0 dB"));
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
            let k = fine(io);
            let next = match drag {
                Drag::Gang(mut d) => {
                    let gangs = self.gangs(cx);
                    gang_move(&gangs, &mut d, y, io, up);
                    Some(Drag::Gang(d))
                }
                Drag::Thr { params, last, mut v } => {
                    v += (x - last) / Map::square(CURVE).ppd * k;
                    for &p in &params {
                        io.perform(p, v);
                    }
                    Some(Drag::Thr { params, last: x, v })
                }
                Drag::Ratio { params, last, mut n } => {
                    n = (n - (last - y) / 200.0 * k).clamp(0.0, 1.0);
                    let v = INFO.params[params[0]].from_normalized(n);
                    for &p in &params {
                        io.perform(p, v);
                    }
                    Some(Drag::Ratio { params, last: y, n })
                }
                Drag::Freq { params, last, mut f } => {
                    f *= 10f32.powf((x - last) / 180.0 * k);
                    for &p in &params {
                        io.perform(p, f);
                    }
                    Some(Drag::Freq { params, last: x, f })
                }
                Drag::Width { params, centre } => {
                    let w = 2.0 * ((xv(x) / centre).abs().log2()).abs();
                    for &p in &params {
                        io.perform(p, w);
                    }
                    Some(Drag::Width { params, centre })
                }
            };
            if up {
                if let Some(d) = &next {
                    let ps: Vec<usize> = match d {
                        Drag::Thr { params, .. } | Drag::Ratio { params, .. } | Drag::Freq { params, .. } | Drag::Width { params, .. } => params.clone(),
                        Drag::Gang(_) => Vec::new(),
                    };
                    for p in ps {
                        io.end(p);
                    }
                }
                self.drag = None;
            } else {
                self.drag = next;
            }
            return Response::TAKEN;
        }
        match ev {
            Pointer::Down { x, y, clicks } => self.down(x, y, clicks, cx, io),
            Pointer::Wheel { x, y, dy } => {
                let gangs = self.gangs(cx);
                if gang_wheel(&gangs, x, y, dy, io) {
                    return Response::TAKEN;
                }
                let e = self.set;
                if kit::inside(PLOT, x, y) {
                    let w = (Self::sv(cx, e, ix::BANDWIDTH) * 2f32.powf(dy * 0.25)).max(1.0 / 6.0);
                    for p in self.targets(cx, ix::BANDWIDTH) {
                        io.set(p, w);
                    }
                    return Response::TAKEN;
                }
                if kit::inside(CURVE, x, y) {
                    let m = Map::square(CURVE);
                    let l = m.level_at(x);
                    let thr = Self::sv(cx, e, ix::THRESHOLD);
                    if (l - thr).abs() <= Self::knee_h(cx, e).max(6.0) {
                        let k = Self::sv(cx, e, ix::SOFT_KNEE) + dy * 5.0;
                        for p in self.targets(cx, ix::SOFT_KNEE) {
                            io.set(p, k);
                        }
                        return Response::TAKEN;
                    }
                }
                Response::NONE
            }
            _ => Response::NONE,
        }
    }
}

impl Dual {
    fn action(&mut self, item: usize, cx: &Ctx, io: &mut Io) {
        let (a, b) = (self.set, 1 - self.set);
        match item {
            0 => {
                for off in 0..ix::SET_LEN {
                    io.set(cx.p(Self::sl(b, off)), Self::sv(cx, a, off));
                }
            }
            1 => {
                for off in 0..ix::SET_LEN {
                    let (va, vb) = (Self::sv(cx, a, off), Self::sv(cx, b, off));
                    io.set(cx.p(Self::sl(a, off)), vb);
                    io.set(cx.p(Self::sl(b, off)), va);
                }
            }
            _ => {
                for off in 0..ix::SET_LEN {
                    let p = cx.p(Self::sl(a, off));
                    io.set(p, INFO.params[p].default);
                }
            }
        }
    }

    fn down(&mut self, x: f32, y: f32, clicks: u32, cx: &Ctx, io: &mut Io) -> Response {
        let gangs = self.gangs(cx);
        if let Some(d) = gang_down(&gangs, x, y, clicks, io) {
            if let Some(d) = d {
                self.drag = Some(Drag::Gang(d));
            }
            return Response::TAKEN;
        }
        let hy = cx.zones.header.1;
        for (i, r) in tabs(hy).into_iter().enumerate() {
            if kit::inside(r, x, y) {
                if Self::ganged(cx) {
                    io.set(cx.p(ix::GANGED_SET), i as f32);
                }
                if i != self.set {
                    self.set = i;
                    return Response::REBUILD;
                }
                return Response::TAKEN;
            }
        }
        if !Self::ganged(cx) && kit::inside(link_rect(hy), x, y) {
            self.link = !self.link;
            return Response::REBUILD;
        }
        if cx.v(ix::OUTPUT_GAIN) > 0.0 && kit::inside(CHIP, x, y) {
            io.set(cx.p(ix::OUTPUT_GAIN), 0.0);
            return Response::TAKEN;
        }
        if (0..3).any(|i| kit::inside(max_rect(i), x, y)) {
            self.reset_holds();
            return Response::TAKEN;
        }
        if cx.more {
            for (i, r) in [COPY, SWAP, RESET].into_iter().enumerate() {
                if kit::inside(r, x, y) {
                    self.action(i, cx, io);
                    return Response::TAKEN;
                }
            }
            return Response::NONE;
        }
        let e = self.set;
        if kit::inside(CURVE, x, y) {
            let m = Map::square(CURVE);
            let thr = Self::sv(cx, e, ix::THRESHOLD);
            let kt = thr.clamp(-60.0, 0.0);
            let (kx, ky) = (m.x(kt), m.y(Self::curve_out(cx, e, kt).clamp(-60.0, 0.0)));
            if (x - kx).abs() <= 9.0 && (y - ky).abs() <= 9.0 {
                let params = self.targets(cx, ix::THRESHOLD);
                if clicks >= 2 {
                    for p in params {
                        io.set(p, INFO.params[p].default);
                    }
                    return Response::TAKEN;
                }
                for &p in &params {
                    io.begin(p);
                }
                self.drag = Some(Drag::Thr { params, last: x, v: thr });
            } else if x > kx {
                let params = self.targets(cx, ix::RATIO);
                if clicks >= 2 {
                    for p in params {
                        io.set(p, INFO.params[p].default);
                    }
                    return Response::TAKEN;
                }
                for &p in &params {
                    io.begin(p);
                }
                let n = INFO.params[params[0]].to_normalized(Self::sv(cx, e, ix::RATIO));
                self.drag = Some(Drag::Ratio { params, last: y, n });
            }
            return Response::TAKEN;
        }
        if kit::inside(PLOT, x, y) {
            let ftype = cx.choice(Self::sl(e, ix::FILTER_TYPE));
            if ftype == 3 {
                return Response::TAKEN;
            }
            let fc = Self::sv(cx, e, ix::CENTER_FREQ);
            if clicks >= 2 {
                for off in [ix::CENTER_FREQ, ix::BANDWIDTH] {
                    for p in self.targets(cx, off) {
                        io.set(p, INFO.params[p].default);
                    }
                }
                return Response::TAKEN;
            }
            if ftype == 1 {
                let bw = Self::sv(cx, e, ix::BANDWIDTH);
                let (lo, hi) = (vx(fc * 2f32.powf(-0.5 * bw)), vx(fc * 2f32.powf(0.5 * bw)));
                if (x - lo).abs() <= 6.0 || (x - hi).abs() <= 6.0 {
                    let params = self.targets(cx, ix::BANDWIDTH);
                    for &p in &params {
                        io.begin(p);
                    }
                    self.drag = Some(Drag::Width { params, centre: fc });
                    return Response::TAKEN;
                }
            }
            let params = self.targets(cx, ix::CENTER_FREQ);
            for &p in &params {
                io.begin(p);
            }
            self.drag = Some(Drag::Freq { params, last: x, f: fc });
            return Response::TAKEN;
        }
        Response::NONE
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_dual() {
        let v = [-14.2, -20.5, -4.1, -1.2, 0.8, 0.0, 2.0, 0.0];
        super::super::bands::dump::dump("dual", &v, &[("channelMode", 1.0), ("threshold1", -30.0), ("ratio1", 3.0), ("filterType1", 1.0), ("centerFreq1", 5000.0), ("threshold2", -24.0), ("makeup2", 2.0), ("outputGain", 2.5)]);
    }
}
