//! The graph code the Parametric EQ, Dynamic EQ and Resonance Suppressor pages share
//! (`design/tone.md` sections 0 to 3): the frequency axis and its dB range switch, band nodes
//! and band pills, curve fills, the type picker, and the spectra drawn from `Live::audio`.
//!
//! The axis, node, switch and spectrum pieces follow the Equaliser page's toolkit (`eq6.rs`).

#![allow(dead_code)] // the shared toolkit: not every page uses every piece

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use openplugin::dri::goodlookindri_shell::text;

use crate::ui::analyser::{Spectrum, SPECTRUM_FLOOR};
use crate::ui::kit;
use crate::ui::page::{Audio, Io, Zones};
use crate::ui::raster::R;
use crate::ui::theme::Theme;

/// Lowest and highest frequency of every graph.
pub(crate) const F_LO: f32 = 20.0;
pub(crate) const F_HI: f32 = 20_000.0;
/// Points per drawn response curve.
pub(crate) const CURVE_POINTS: usize = 256;
/// Graph node diameter (shell size) and how far from its centre a click still takes it.
pub(crate) const NODE_D: f32 = 14.0;
pub(crate) const NODE_HIT: f32 = 11.0;
/// Band shapes are filled at this alpha, the selected band's at [`FILL_SELECTED`].
pub(crate) const FILL: f32 = 0.18;
pub(crate) const FILL_SELECTED: f32 = 0.30;
/// The spectrum's level scale, dBFS at the plot top and bottom.
pub(crate) const SPEC_TOP: f32 = 0.0;
pub(crate) const SPEC_BOTTOM: f32 = -96.0;

/// The graph range switch: ±6, ±12, ±24, ±30 dB (band gain reaches ±30).
pub(crate) const RANGES: [f32; 4] = [6.0, 12.0, 24.0, 30.0];
pub(crate) const RANGE_KEYS: &[&str] = &["±6", "±12", "±24", "±30"];

/// Placement letters for badges: none for Stereo.
pub(crate) const PLACEMENT_BADGE: [&str; 6] = ["", "M", "S", "L", "R", "MS"];

/// The graph plot rect: the picture zone less the shell margins (L 32, T 8, B 24) and
/// `right` on the right (32, or more where a meter sits beside the graph).
pub(crate) fn graph_rect(z: &Zones, right: f32) -> R {
    let (x, y, w, h) = z.picture;
    (x + 32.0, y + 8.0, w - 32.0 - right, h - 32.0)
}

/// The range switch in the picture zone's top right corner.
pub(crate) fn range_rect(z: &Zones) -> R {
    (z.picture.0 + z.picture.2 - 188.0, z.picture.1 + 6.0, 180.0, 24.0)
}

/// A log-frequency, linear-dB graph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Axis {
    pub plot: R,
    pub lo: f32,
    pub hi: f32,
    /// The plot spans `+range_db` (top) to `-range_db` (bottom).
    pub range_db: f32,
}

impl Axis {
    pub(crate) fn new(plot: R, range_db: f32) -> Self {
        Self { plot, lo: F_LO, hi: F_HI, range_db }
    }

    pub(crate) fn x0(&self) -> f32 {
        self.plot.0
    }

    pub(crate) fn x1(&self) -> f32 {
        self.plot.0 + self.plot.2
    }

    pub(crate) fn top(&self) -> f32 {
        self.plot.1
    }

    pub(crate) fn bottom(&self) -> f32 {
        self.plot.1 + self.plot.3
    }

    pub(crate) fn x(&self, f: f32) -> f32 {
        kit::freq_x(f, self.x0(), self.x1(), self.lo, self.hi)
    }

    /// The frequency at x, clamped to the axis.
    pub(crate) fn freq(&self, x: f32) -> f32 {
        kit::x_freq(x, self.x0(), self.x1(), self.lo, self.hi)
    }

    pub(crate) fn y(&self, db: f32) -> f32 {
        self.plot.1 + (self.range_db - db) / (2.0 * self.range_db) * self.plot.3
    }

    /// y of a gain, kept a hair inside the plot.
    pub(crate) fn y_in(&self, db: f32) -> f32 {
        self.y(db).clamp(self.plot.1 + 1.0, self.plot.1 + self.plot.3 - 1.0)
    }

    pub(crate) fn db(&self, y: f32) -> f32 {
        self.range_db - (y - self.plot.1) / self.plot.3 * 2.0 * self.range_db
    }

    /// The dB grid step that suits the range and the plot height.
    pub(crate) fn step(&self) -> f32 {
        let lines = if self.plot.3 > 250.0 { 8.0 } else { 4.0 };
        let raw = 2.0 * self.range_db / lines;
        [1.0, 2.0, 3.0, 6.0, 10.0, 12.0, 15.0].into_iter().find(|&s| s >= raw - 1e-3).unwrap_or(15.0)
    }

    /// Plot background, frequency grid and dB lines with labels. Static.
    pub(crate) fn paint_grid(&self, s: &mut Scene, t: &Theme) {
        kit::plot(s, t, self.plot);
        kit::freq_grid(s, t, self.plot, self.lo, self.hi, self.range_db, 2.0 * self.range_db, self.step());
    }

    /// `n` log-spaced frequencies across the axis.
    pub(crate) fn freqs(&self, n: usize) -> Vec<f32> {
        let n = n.max(2);
        (0..n).map(|i| self.lo * (self.hi / self.lo).powf(i as f32 / (n - 1) as f32)).collect()
    }

    /// A curve through `db` at `freqs`, kept inside the plot.
    pub(crate) fn points(&self, freqs: &[f32], db: &[f32]) -> Vec<Point> {
        freqs.iter().zip(db).map(|(&f, &d)| Point::new(self.x(f), self.y_in(d))).collect()
    }

    /// y of a spectrum level on the spectrum's own scale.
    pub(crate) fn spec_y(&self, db: f32) -> f32 {
        let (y, h) = (self.plot.1, self.plot.3);
        (y + (SPEC_TOP - db) / (SPEC_TOP - SPEC_BOTTOM) * h).clamp(y, y + h)
    }
}

/// Clips what follows to the plot (pair with [`unclip`]).
pub(crate) fn clip(s: &mut Scene, axis: &Axis) {
    s.clip_round(kit::rect(axis.plot), 4.0);
}

pub(crate) fn unclip(s: &mut Scene) {
    s.pop_clip();
}

/// A band's shape filled between its curve and the 0 dB line.
pub(crate) fn fill_to_zero(s: &mut Scene, axis: &Axis, pts: &[Point], c: C, alpha: f32) {
    kit::area(s, pts, axis.y(0.0), c.with_alpha(alpha));
}

/// A filled area between two curves sampled at the same x positions.
pub(crate) fn fill_between(s: &mut Scene, upper: &[Point], lower: &[Point], c: C) {
    if upper.len() < 2 || upper.len() != lower.len() {
        return;
    }
    kit::between(s, upper, lower, c);
}

/// The area between two curves split by sign: `below` where `a` is under `b` (cutting),
/// `above` where it is over (boosting). Both curves share the x positions.
pub(crate) fn fill_signed(s: &mut Scene, a: &[Point], b: &[Point], below: C, above: C) {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i + 1 < n {
        let cut = a[i].y > b[i].y;
        let mut j = i + 1;
        while j + 1 < n && (a[j].y > b[j].y) == cut {
            j += 1;
        }
        fill_between(s, &a[i..=j], &b[i..=j], if cut { below } else { above });
        i = j;
    }
}

/// A graph node: a disc in the band colour with its number, a ring when selected, faded
/// when the band is not heard. `badge` is a placement letter beside it.
pub(crate) fn paint_node(s: &mut Scene, t: &Theme, at: Point, colour: C, label: &str, selected: bool, faded: bool, badge: &str) {
    let r = NODE_D * 0.5;
    let disc = Scene::ellipse_at(at, r);
    let fill = if faded { colour.with_alpha(0.35) } else { colour };
    s.shadow_ellipse(disc.translated(0.0, 1.0), 2.0, t.shadow);
    s.fill_ellipse_solid(disc, fill);
    s.stroke_ellipse(disc, t.plot_bg, 1.2);
    if selected {
        s.stroke_ellipse(Scene::ellipse_at(at, r + 3.0), t.accent, 1.5);
    }
    s.text(at, label, t.on_accent, 8.0, true);
    if !badge.is_empty() {
        s.text(Point::new(at.x + r + 7.0, at.y - r), badge, colour, 8.0, true);
    }
}

/// The "K" badge of a band whose detector listens to the key: solid, or hollow while no
/// key signal is present.
pub(crate) fn paint_key_badge(s: &mut Scene, t: &Theme, at: Point, present: bool) {
    let r = Rect::new(at.x - 5.0, at.y - 5.0, 10.0, 10.0);
    if present {
        s.fill_round(r, 2.5, t.warm, t.warm, true);
        s.text(r.center(), "K", t.on_accent, 8.0, true);
    } else {
        s.fill_round(r, 2.5, t.plot_bg, t.plot_bg, true);
        s.stroke_round(r, 2.5, t.warm, 1.2);
        s.text(r.center(), "K", t.warm, 8.0, true);
    }
}

/// The nearest point within [`NODE_HIT`] of `(x, y)`; `None` entries are skipped.
pub(crate) fn node_at(nodes: &[Option<Point>], x: f32, y: f32) -> Option<usize> {
    nodes
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|p| (i, (p.x - x).hypot(p.y - y))))
        .filter(|&(_, d)| d <= NODE_HIT)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// What the spectrum shows: nothing, the module's input, input and output, or the key over
/// the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tap {
    Off,
    Pre,
    Post,
    Key,
}

impl Tap {
    pub(crate) const LABELS: &'static [&'static str] = &["OFF", "PRE", "POST"];
    pub(crate) const LABELS_KEY: &'static [&'static str] = &["OFF", "PRE", "POST", "KEY"];
    pub(crate) fn from_index(i: usize) -> Self {
        [Tap::Off, Tap::Pre, Tap::Post, Tap::Key][i.min(3)]
    }
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// Spectra of the shown module's input, output and key (FFT 4096, 1/6 octave, falling).
pub(crate) struct Spectra {
    pub pre: Spectrum,
    pub post: Spectrum,
    pub key: Spectrum,
    scratch: Vec<f32>,
    /// Something above the floor has been read since the last clear.
    pub live: bool,
}

impl Default for Spectra {
    fn default() -> Self {
        Self { pre: Spectrum::new(4096, 160, F_LO, F_HI), post: Spectrum::new(4096, 160, F_LO, F_HI), key: Spectrum::new(4096, 160, F_LO, F_HI), scratch: Vec::new(), live: false }
    }
}

/// Which channels a spectrum reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Listen {
    Both,
    Mid,
    Side,
}

impl Spectra {
    /// Reads the newest audio (`key` too when asked). Returns true while anything shows.
    pub(crate) fn update(&mut self, audio: &Audio, listen: Listen, key: bool, sample_rate: f32, dt: f32) -> bool {
        let sr = if sample_rate > 1000.0 { sample_rate } else { 48_000.0 };
        let scratch = &mut self.scratch;
        let mut one = |spec: &mut Spectrum, [l, r]: &[Vec<f32>; 2]| match listen {
            Listen::Both => spec.update(l, Some(r), sr, dt),
            Listen::Mid | Listen::Side => {
                let sign = if listen == Listen::Mid { 1.0 } else { -1.0 };
                scratch.clear();
                scratch.extend(l.iter().zip(r.iter()).map(|(x, y)| 0.5 * (x + sign * y)));
                spec.update(scratch, None, sr, dt);
            }
        };
        one(&mut self.pre, &audio.pre);
        one(&mut self.post, &audio.post);
        if key {
            one(&mut self.key, &audio.key);
        }
        let shows = |s: &Spectrum| s.db.iter().any(|&d| d > SPECTRUM_FLOOR + 1.0);
        self.live = shows(&self.pre) || shows(&self.post) || (key && shows(&self.key));
        self.live
    }

    pub(crate) fn clear(&mut self) {
        self.pre.clear();
        self.post.clear();
        self.key.clear();
        self.live = false;
    }
}

/// A spectrum's points on the graph's spectrum scale (`None` while it is silent).
pub(crate) fn spectrum_points(axis: &Axis, spec: &Spectrum) -> Option<Vec<Point>> {
    let n = spec.db.len();
    if n < 2 || spec.db.iter().all(|&d| d <= SPECTRUM_FLOOR + 1.0) {
        return None;
    }
    Some((0..n).map(|i| Point::new(axis.x(spec.freq(i)), axis.spec_y(spec.db[i]))).collect())
}

/// A spectrum filled from the plot bottom, or as a line.
pub(crate) fn paint_spectrum(s: &mut Scene, axis: &Axis, spec: &Spectrum, c: C, alpha: f32, fill: bool) {
    let Some(pts) = spectrum_points(axis, spec) else { return };
    if fill {
        kit::area(s, &pts, axis.bottom(), c.with_alpha(alpha));
    } else {
        kit::curve(s, pts, c.with_alpha(alpha), 1.2);
    }
}

/// The spectra for a tap: input fill; output line; or the key line in `warm` over the input.
pub(crate) fn paint_spectra(s: &mut Scene, t: &Theme, axis: &Axis, spectra: &Spectra, tap: Tap) {
    match tap {
        Tap::Off => {}
        Tap::Pre => paint_spectrum(s, axis, &spectra.pre, t.in_fill, t.in_fill_alpha * 0.6, true),
        Tap::Post => {
            paint_spectrum(s, axis, &spectra.pre, t.in_fill, t.in_fill_alpha * 0.45, true);
            paint_spectrum(s, axis, &spectra.post, t.out_line, 0.8, false);
        }
        Tap::Key => {
            paint_spectrum(s, axis, &spectra.pre, t.in_fill, t.in_fill_alpha * 0.35, true);
            paint_spectrum(s, axis, &spectra.key, t.warm, 0.95, false);
        }
    }
}

/// A segmented switch for a view state (not a parameter), painted like the shell's.
pub(crate) fn paint_switch(s: &mut Scene, t: &Theme, r: R, labels: &[&str], chosen: usize, enabled: bool) {
    kit::inset(s, t, r, 6.0);
    let n = labels.len().max(1);
    let kw = r.2 / n as f32;
    for (i, label) in labels.iter().enumerate() {
        let key = Rect::new(r.0 + kw * i as f32 + 2.0, r.1 + 2.0, kw - 4.0, r.3 - 4.0);
        let on = i == chosen;
        if on {
            let c = if enabled { t.accent } else { t.dim.with_alpha(0.5) };
            s.fill_round(key, 4.0, c, c, true);
        }
        let ink = if on { t.on_accent } else if enabled { t.dim } else { t.dim.with_alpha(0.5) };
        s.text(key.center(), text::ellipsize(label, 9.0, kw - 6.0), ink, 9.0, true);
    }
}

/// The key of a switch of `n` keys under a point.
pub(crate) fn switch_key(r: R, n: usize, x: f32, y: f32) -> Option<usize> {
    kit::inside(r, x, y).then(|| (((x - r.0) / (r.2 / n as f32)).floor() as usize).min(n - 1))
}

/// A box that looks like a dropdown, for a choice the page handles itself.
pub(crate) fn paint_box(s: &mut Scene, t: &Theme, r: R, label: &str, faded: bool) {
    kit::inset(s, t, r, 6.0);
    let ink = if faded { t.dim } else { t.text };
    s.text(Point::new(r.0 + 10.0, r.1 + r.3 * 0.5), text::ellipsize(label, 10.5, r.2 - 28.0), ink, 10.5, false);
    let (cx, cy) = (r.0 + r.2 - 12.0, r.1 + r.3 * 0.5);
    s.stroke_poly(vec![Point::new(cx - 4.0, cy - 2.0), Point::new(cx, cy + 2.0), Point::new(cx + 4.0, cy - 2.0)], t.dim, 1.5, false);
}

pub(crate) fn tip(r: R, text: impl Into<String>) -> Tooltip {
    Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() }
}

/// A node drag in progress: the parameters it moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NodeDrag {
    pub freq: Option<usize>,
    pub gain: Option<usize>,
    /// Pointer offset from the node centre at the grab, so the node does not jump.
    pub offset: (f32, f32),
    /// Cmd held at the grab: the drag keeps to the axis it first moves along.
    pub lock: bool,
    pub from: (f32, f32),
}

impl NodeDrag {
    pub(crate) fn begin(self, io: &mut Io) -> Self {
        for p in [self.freq, self.gain].into_iter().flatten() {
            io.begin(p);
        }
        self
    }

    pub(crate) fn update(&self, axis: &Axis, io: &mut Io, x: f32, y: f32) {
        let (mut x, mut y) = (x, y);
        if self.lock {
            if (x - self.from.0).abs() > (y - self.from.1).abs() {
                y = self.from.1;
            } else {
                x = self.from.0;
            }
        }
        let (x, y) = (x - self.offset.0, y - self.offset.1);
        if let Some(p) = self.freq {
            io.perform(p, axis.freq(x));
        }
        if let Some(p) = self.gain {
            io.perform(p, axis.db(y).clamp(-axis.range_db, axis.range_db));
        }
    }

    pub(crate) fn end(&self, io: &mut Io) {
        for p in [self.freq, self.gain].into_iter().flatten() {
            io.end(p);
        }
    }
}

/// Multiplies a log parameter by `2^(octaves)` in one gesture (wheel on a node: Q).
pub(crate) fn nudge_log(io: &mut Io, param: usize, octaves: f32) {
    let v = io.value(param);
    io.set(param, v * 2f32.powf(octaves));
}

/// Adds `by` to a parameter in one gesture.
pub(crate) fn nudge(io: &mut Io, param: usize, by: f32) {
    let v = io.value(param);
    io.set(param, v + by);
}

// ---- band pills ----

pub(crate) const BAND_PILLS: usize = 32;

/// Band pill `i` (0..32): 26 x 18 at y 628.
pub(crate) fn pill_rect(i: usize) -> R {
    (122.0 + 30.0 * i as f32, 628.0, 26.0, 18.0)
}

/// The strip of band pills, for damage.
pub(crate) const PILL_STRIP: R = (120.0, 626.0, 960.0, 26.0);

pub(crate) fn pill_at(x: f32, y: f32) -> Option<usize> {
    (0..BAND_PILLS).find(|&i| kit::inside(pill_rect(i), x, y))
}

/// What a band pill shows.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PillState<'a> {
    pub used: bool,
    pub selected: bool,
    pub badge: &'a str,
    /// The detector listens to the key: a `warm` corner.
    pub keyed: bool,
    /// Live dynamic gain over its range, -1..1, drawn as a 2 pt under-bar.
    pub bar: Option<f32>,
}

pub(crate) fn paint_pill(s: &mut Scene, t: &Theme, i: usize, p: PillState) {
    let r = pill_rect(i);
    let rr = kit::rect(r);
    let c = t.band(i);
    if p.used {
        s.fill_round(rr, 5.0, c.with_alpha(0.85), c.with_alpha(0.85), true);
    } else {
        s.fill_round(rr, 5.0, t.inset, t.inset, true);
        s.stroke_round(rr, 5.0, t.border, 1.0);
    }
    if p.selected {
        s.stroke_round(Rect::new(r.0 - 2.0, r.1 - 2.0, r.2 + 4.0, r.3 + 4.0), 6.0, t.text, 1.5);
    }
    let ink = if p.used { t.on_accent } else { t.dim };
    let label = if p.badge.is_empty() { format!("{}", i + 1) } else { format!("{}{}", i + 1, p.badge) };
    s.text(rr.center(), text::ellipsize(&label, 8.0, r.2 - 2.0), ink, 8.0, true);
    if p.keyed {
        s.fill_poly(vec![Point::new(r.0 + r.2 - 8.0, r.1), Point::new(r.0 + r.2, r.1), Point::new(r.0 + r.2, r.1 + 8.0)], t.warm, t.warm);
    }
    if let Some(b) = p.bar {
        let w = (b.abs().min(1.0) * r.2).max(if b.abs() > 0.01 { 2.0 } else { 0.0 });
        if w > 0.0 {
            let colour = if b < 0.0 { t.accent } else { c };
            s.fill_rect(Rect::new(r.0, r.1 + r.3 + 2.0, w, 2.0), colour, colour, true);
        }
    }
}

// ---- the type picker ----

/// The 48 filter types by family, in the picker's columns (crate indices).
pub(crate) const TYPE_GROUPS: [(&str, &[usize]); 6] = [
    ("BASIC", &[0, 1, 2, 7, 5, 6, 8, 16]),
    ("CUT / SHAPE", &[3, 4, 13, 14, 9, 10, 11, 12, 15]),
    ("E / G", &[17, 18, 19, 20, 21, 22, 23, 24, 25]),
    ("BRITN / 250", &[26, 27, 28, 29, 30, 31, 32]),
    ("VT", &[33, 34, 35, 36, 37, 38, 39]),
    ("TONE / BLUE / G", &[40, 41, 42, 45, 43, 44, 46, 47]),
];

/// The picker panel over the graph.
/// It starts under the range switch (which [`graph_rect`] leaves 22 pt into the plot), so the
/// switch never pokes out above or beside it.
pub(crate) fn picker_rect(axis: &Axis) -> R {
    let (x, y, w, h) = axis.plot;
    (x + 8.0, y + 28.0, w - 16.0, h - 34.0)
}

fn picker_row(r: R) -> f32 {
    ((r.3 - 30.0) / 9.0).clamp(12.0, 22.0)
}

/// Item `k` of column `col`.
fn picker_item(r: R, col: usize, k: usize) -> R {
    let cw = r.2 / TYPE_GROUPS.len() as f32;
    let row = picker_row(r);
    (r.0 + col as f32 * cw + 4.0, r.1 + 24.0 + k as f32 * row, cw - 8.0, row - 1.0)
}

pub(crate) fn paint_picker(s: &mut Scene, t: &Theme, r: R, names: &[&str], current: usize, title: &str) {
    s.shadow_round(kit::rect(r).translated(0.0, 3.0), 8.0, 8.0, t.shadow);
    s.fill_round(kit::rect(r), 8.0, t.panel, t.panel, true);
    s.stroke_round(kit::rect(r), 8.0, t.border, 1.0);
    let cw = r.2 / TYPE_GROUPS.len() as f32;
    let size = if picker_row(r) < 15.0 { 9.0 } else { 10.0 };
    for (col, (head, items)) in TYPE_GROUPS.iter().enumerate() {
        s.text(Point::new(r.0 + col as f32 * cw + 10.0, r.1 + 13.0), *head, t.dim, 8.5, false);
        for (k, &ty) in items.iter().enumerate() {
            let it = picker_item(r, col, k);
            if ty == current {
                s.fill_round(kit::rect(it), 4.0, t.accent, t.accent, true);
            }
            let ink = if ty == current { t.on_accent } else { t.text };
            let name = names.get(ty).copied().unwrap_or("");
            s.text(Point::new(it.0 + 6.0, it.1 + it.3 * 0.5), text::ellipsize(name, size, it.2 - 8.0), ink, size, false);
        }
    }
    // The title under the last column (8 items, so its ninth row is free), apart from the heads.
    let last = picker_item(r, TYPE_GROUPS.len() - 1, TYPE_GROUPS[TYPE_GROUPS.len() - 1].1.len());
    kit::text_right(s, last.0 + last.2 - 2.0, last.1 + last.3 * 0.5, title, t.dim, 8.5);
}

/// The type under a point of the picker.
pub(crate) fn picker_hit(r: R, x: f32, y: f32) -> Option<usize> {
    for (col, (_, items)) in TYPE_GROUPS.iter().enumerate() {
        for (k, &ty) in items.iter().enumerate() {
            if kit::inside(picker_item(r, col, k), x, y) {
                return Some(ty);
            }
        }
    }
    None
}

/// High Pass, Low Pass and Band Pass come with slope 0 (order 0, as in OpenEQ); choosing one
/// in the editor sets the slope to 50 %, a 12 dB/oct cut, as OpenEQ's editor does.
pub(crate) fn slope_on_pick(ty: usize, slope: f32) -> Option<f32> {
    (matches!(ty, 3 | 4 | 6) && slope <= 0.0).then_some(50.0)
}

// ---- readouts, captions, the key chip ----

pub(crate) fn hz_text(v: f32) -> String {
    if v >= 10_000.0 {
        format!("{:.1} kHz", v / 1000.0)
    } else if v >= 1000.0 {
        format!("{:.2} kHz", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

pub(crate) fn hz_short(v: f32) -> String {
    if v >= 10_000.0 {
        format!("{:.1}k", v / 1000.0)
    } else if v >= 1000.0 {
        format!("{:.2}k", v / 1000.0)
    } else {
        format!("{v:.0}")
    }
}

pub(crate) fn gain_text(v: f32) -> String {
    format!("{v:+.1} dB")
}

pub(crate) fn db_text(v: f32) -> String {
    format!("{v:.1} dB")
}

pub(crate) fn q_text(v: f32) -> String {
    if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

pub(crate) fn pct_text(v: f32) -> String {
    format!("{v:.0} %")
}

pub(crate) fn ms_text(v: f32) -> String {
    if v >= 1000.0 {
        format!("{:.2} s", v / 1000.0)
    } else if v >= 100.0 {
        format!("{v:.0} ms")
    } else {
        format!("{v:.1} ms")
    }
}

/// Captions above boxes: `(x, y, text)`. Static.
pub(crate) fn captions(s: &mut Scene, t: &Theme, items: &[(f32, f32, &str)]) {
    for &(x, y, label) in items {
        kit::caption(s, t, x, y, label);
    }
}

/// A read-only box of up to three lines.
pub(crate) fn paint_readout(s: &mut Scene, t: &Theme, r: R, lines: &[(String, C, f32)]) {
    kit::inset(s, t, r, 6.0);
    let n = lines.len().max(1) as f32;
    let step = r.3 / n;
    for (i, (line, c, size)) in lines.iter().enumerate() {
        let y = r.1 + step * (i as f32 + 0.5);
        s.text(Point::new(r.0 + 8.0, y), text::ellipsize(line, *size, r.2 - 12.0), *c, *size, false);
    }
}

/// The key chip: "KEY", a level bar and the peak in `warm`, or "NO KEY" dim.
pub(crate) fn paint_key_chip(s: &mut Scene, t: &Theme, r: R, present: bool, level_db: f32, active: bool) {
    let rr = kit::rect(r);
    s.fill_round(rr, r.3 * 0.5, t.inset, t.inset, true);
    s.stroke_round(rr, r.3 * 0.5, if active { t.warm } else { t.border }, 1.0);
    let heard = present && level_db > -90.0;
    if !heard {
        s.text(rr.center(), if present { "KEY SILENT" } else { "NO KEY" }, t.dim, 9.0, true);
        return;
    }
    let y = r.1 + r.3 * 0.5;
    s.text(Point::new(r.0 + 10.0, y), "KEY", t.warm, 9.0, false);
    let bar = (r.0 + 36.0, y - 3.0, 48.0, 6.0);
    s.fill_round(kit::rect(bar), 2.0, t.seg_off, t.seg_off, true);
    let fill = ((level_db + 60.0) / 60.0).clamp(0.0, 1.0) * bar.2;
    if fill > 0.5 {
        s.fill_round(Rect::new(bar.0, bar.1, fill, bar.3), 2.0, t.warm, t.warm, true);
    }
    kit::text_right(s, r.0 + r.2 - 10.0, y, &format!("{level_db:.0}"), t.warm, 9.0);
}

/// A horizontal key meter with a caption ("EXT KEY -18 dB" or "NO KEY SIGNAL").
pub(crate) fn paint_key_meter(s: &mut Scene, t: &Theme, r: R, present: bool, level_db: f32) {
    let heard = present && level_db > -90.0;
    let caption = if heard { format!("EXT KEY {level_db:.1} dB") } else { "NO KEY SIGNAL".to_string() };
    s.text(Point::new(r.0, r.1 + 6.0), caption, if heard { t.warm } else { t.dim }, 9.0, false);
    let bar = Rect::new(r.0, r.1 + r.3 - 10.0, r.2, 8.0);
    s.fill_round(bar, 3.0, t.seg_off, t.seg_off, true);
    if heard {
        let w = ((level_db + 60.0) / 60.0).clamp(0.0, 1.0) * r.2;
        if w > 0.5 {
            s.fill_round(Rect::new(bar.x, bar.y, w, bar.h), 3.0, t.warm, t.warm, true);
        }
    }
}

/// The local default of a parameter (for delete and reset).
pub(crate) fn default_of(host: usize) -> f32 {
    crate::INFO.params.get(host).map_or(0.0, |p| p.default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_lists_every_type_once() {
        let mut all: Vec<usize> = TYPE_GROUPS.iter().flat_map(|(_, v)| v.iter().copied()).collect();
        all.sort_unstable();
        assert_eq!(all, (0..48).collect::<Vec<_>>());
        let r = (160.0, 258.0, 880.0, 328.0);
        let it = picker_item(r, 1, 0);
        assert_eq!(picker_hit(r, it.0 + 3.0, it.1 + 3.0), Some(3));
        assert_eq!(slope_on_pick(3, 0.0), Some(50.0));
        assert_eq!(slope_on_pick(3, 20.0), None);
        assert_eq!(slope_on_pick(0, 0.0), None);
    }

    #[test]
    fn the_axis_maps_both_ways() {
        let a = Axis::new((152.0, 252.0, 896.0, 340.0), 12.0);
        assert!((a.x(20.0) - 152.0).abs() < 1e-3 && (a.x(20_000.0) - 1048.0).abs() < 1e-3);
        assert!((a.freq(a.x(1000.0)) - 1000.0).abs() < 0.5);
        assert!((a.db(a.y(-7.5)) + 7.5).abs() < 1e-4);
        assert_eq!(node_at(&[Some(Point::new(10.0, 10.0)), None, Some(Point::new(30.0, 10.0))], 27.0, 12.0), Some(2));
        assert!(pill_rect(31).0 + pill_rect(31).2 <= 1080.0);
    }

    /// Writes `build/ui-eqx-<key>-<theme>-<mode>.ppm` for the three pages in both modes, with
    /// real module output in the taps, the read-out and the key. Run with `--ignored`.
    #[test]
    #[ignore]
    fn dump_eq_pages() {
        use crate::ui::tests::{editor, feed};
        use crate::ui::theme::ThemeId;
        use crate::LAYOUT;
        use olm_api::Module;
        use openplugin::core::Editor;
        let dir = format!("{}/build", env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir_all(&dir).expect("dir");
        let peq = |b: usize, f: usize| eq_parametric::band_param(b, f);
        let deq = |b: usize, f: usize| eq_dynamic::band_param(b, f);
        use eq_dynamic as d;
        use eq_parametric as p;
        let cases: Vec<(&str, Vec<(usize, f32)>, usize, bool)> = vec![
            ("peq", vec![(peq(0, p::F_TYPE), 3.0), (peq(0, p::F_SLOPE), 50.0), (peq(0, p::F_FREQ), 40.0), (peq(1, p::F_GAIN), 4.0), (peq(1, p::F_FREQ), 120.0), (peq(2, p::F_GAIN), -6.0), (peq(2, p::F_FREQ), 900.0), (peq(2, p::F_Q), 3.0), (peq(3, p::F_ON), 1.0), (peq(3, p::F_TYPE), 2.0), (peq(3, p::F_FREQ), 8000.0), (peq(3, p::F_GAIN), 3.0), (peq(3, p::F_PLACEMENT), 2.0)], 2, false),
            ("deq", vec![(deq(0, d::F_GAIN), 2.0), (deq(1, d::F_DYN), 1.0), (deq(1, d::F_FREQ), 250.0), (deq(1, d::F_THRESH), -40.0), (deq(1, d::F_ABOVE_ON), 1.0), (deq(1, d::F_ABOVE_RANGE), -9.0), (deq(2, d::F_DYN), 1.0), (deq(2, d::F_FREQ), 3000.0), (deq(2, d::F_SOURCE), 5.0), (deq(2, d::F_THRESH), -45.0), (deq(2, d::F_RANGE), -8.0), (deq(2, d::F_DET_LINK), 0.0), (deq(2, d::F_DET_FREQ), 6000.0)], 1, true),
            ("sup", vec![(reso_suppress::P_DEPTH, 80.0), (reso_suppress::P_WIDTH, 4.0), (reso_suppress::P_CENTRE, 2500.0), (reso_suppress::focus_param(0, 0), 60.0)], 0, true),
        ];
        for (key, settings, state, keyed) in cases {
            let slot = LAYOUT.slots.iter().position(|s| s.key == key).expect("slot");
            let base = LAYOUT.slots[slot].base;
            for (id, theme) in ThemeId::ALL.map(|t| (t, t.name())) {
                for tall in [true, false] {
                    for more in [false, true] {
                        let (mut e, shared) = editor(id);
                        feed(&shared, 500);
                        e.param_changed(LAYOUT.slots[slot].on, 1.0);
                        for &(local, v) in &settings {
                            e.param_changed(base + local, v);
                        }
                        e.open_page(slot);
                        e.set_tall(tall);
                        if more && !e.ui.page.has_more() {
                            continue;
                        }
                        e.ui.page.set_state(state);
                        e.ui.more = more;
                        e.rebuild();
                        let mut module: Box<dyn Module> = match key {
                            "peq" => Box::new(eq_parametric::ParametricEq::default()),
                            "deq" => Box::new(eq_dynamic::DynamicEq::default()),
                            _ => Box::new(reso_suppress::ResonanceSuppressor::default()),
                        };
                        module.prepare(48_000.0, 512);
                        for &(local, v) in &settings {
                            module.set_param(local, v);
                        }
                        shared.set_key_present(keyed);
                        let (mut seed, mut phase, mut kp) = (1u32, 0.0f32, 0.0f32);
                        for frame in 0..40 {
                            for _ in 0..4 {
                                let swell = 0.15 + 0.35 * ((frame as f32) * 0.31).sin().abs();
                                let mut l = vec![0.0f32; 512];
                                let mut k = vec![0.0f32; 512];
                                for (v, kv) in l.iter_mut().zip(k.iter_mut()) {
                                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                    let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5;
                                    phase += 2.0 * std::f32::consts::PI * 110.0 / 48_000.0;
                                    kp += 2.0 * std::f32::consts::PI * 2900.0 / 48_000.0;
                                    *v = swell * (0.6 * noise + 0.4 * phase.sin() + 0.25 * kp.sin());
                                    *kv = 0.3 * kp.sin() + 0.1 * noise;
                                }
                                let r: Vec<f32> = l.iter().map(|x| x * 0.8).collect();
                                let (pre_l, pre_r) = (l.clone(), r.clone());
                                let (mut ol, mut or) = (l, r);
                                if keyed {
                                    module.process_keyed(&mut ol, &mut or, &k, &k);
                                    shared.raise_key(0.4);
                                } else {
                                    module.process(&mut ol, &mut or);
                                }
                                shared.push_taps([&pre_l, &pre_r], [&ol, &or], keyed.then_some([&k[..], &k[..]]));
                                let mut view = vec![0.0f32; 512];
                                let n = module.view(&mut view);
                                shared.set_view(slot, &view[..n]);
                                shared.raise_slot_gr(slot, module.meters().reduction_db);
                                shared.set_slot_latency(slot, module.latency());
                            }
                            e.tick(1.0 / 30.0);
                        }
                        let mode = if tall { "tall" } else { "standard" };
                        let face = if more { "-more" } else { "" };
                        let write = |e: &mut crate::ui::OpenLimiterEditor, name: String| {
                            let f = e.render(2.0);
                            let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                            for px in f.pixels {
                                out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
                            }
                            std::fs::write(name, out).expect("write");
                        };
                        write(&mut e, format!("{dir}/ui-eqx-{key}-{theme}-{mode}{face}.ppm"));
                        // The type picker, opened from the type box (peq and deq).
                        if key != "sup" && !more {
                            e.mouse_down(274.0, 668.0, 1);
                            e.mouse_up(274.0, 668.0);
                            e.tick(1.0 / 30.0);
                            write(&mut e, format!("{dir}/ui-eqx-{key}-{theme}-{mode}-pick.ppm"));
                        }
                    }
                }
            }
        }
    }
}
