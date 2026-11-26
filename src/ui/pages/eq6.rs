//! The Equaliser page (`tone-six`, `design/tone.md` section 4): a response graph with six
//! numbered nodes and a low-cut handle over the spectrum, and seven knob cards below it.
//!
//! The first half of this file is the graph toolkit the other EQ pages share (frequency axis,
//! curves and fills, nodes, the spectrum, view switches). Everything in it is `pub(crate)`.

#![allow(dead_code)] // the shared graph toolkit: not every piece is used by this page

use std::cell::{Cell as Once, RefCell};

use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use goodlookindri_shell::text;

use tone_six::design::{self, Shape};
use tone_six::{band_param, BANDS, F_DEPTH, F_FREQ, F_GAIN, F_ON, F_TYPE, F_WIDTH, P_EQ_ON, P_LINK, P_LOW_CUT_FREQ, P_LOW_CUT_ON, P_MATRIX};

use crate::ui::analyser::{Spectrum, SPECTRUM_FLOOR};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Audio, Ctx, Io, Live, Page, Pointer, Response, Zones};
use crate::ui::raster::R;
use crate::ui::theme::Theme;

// =====================================================================================
// Shared graph toolkit
// =====================================================================================

/// Lowest and highest frequency of every EQ graph.
pub(crate) const F_LO: f32 = 20.0;
pub(crate) const F_HI: f32 = 20_000.0;
/// Points per drawn response curve.
pub(crate) const CURVE_POINTS: usize = 256;
/// Graph node diameter (shell size).
pub(crate) const NODE_D: f32 = 14.0;
/// How far from a node's centre a click still takes it.
pub(crate) const NODE_HIT: f32 = 11.0;
/// Band shapes are filled at this alpha, the selected band's at [`FILL_SELECTED`].
pub(crate) const FILL: f32 = 0.18;
pub(crate) const FILL_SELECTED: f32 = 0.30;
/// The spectrum's level scale: plot top and bottom, dBFS.
pub(crate) const SPEC_TOP: f32 = 0.0;
pub(crate) const SPEC_BOTTOM: f32 = -96.0;

/// The graph plot rect: the picture zone less the shell margins (L 32, R 32, T 8, B 24).
pub(crate) fn graph_rect(z: &Zones) -> R {
    let (x, y, w, h) = z.picture;
    (x + 32.0, y + 8.0, w - 64.0, h - 32.0)
}

/// A log-frequency, linear-dB graph: where frequencies and gains land in a plot rect.
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

    /// x of a frequency (not clamped).
    pub(crate) fn x(&self, f: f32) -> f32 {
        kit::freq_x(f, self.x0(), self.x1(), self.lo, self.hi)
    }

    /// The frequency at x (clamped to the axis).
    pub(crate) fn freq(&self, x: f32) -> f32 {
        kit::x_freq(x, self.x0(), self.x1(), self.lo, self.hi)
    }

    /// y of a gain (not clamped).
    pub(crate) fn y(&self, db: f32) -> f32 {
        self.plot.1 + (self.range_db - db) / (2.0 * self.range_db) * self.plot.3
    }

    /// y of a gain, kept a hair inside the plot.
    pub(crate) fn y_in(&self, db: f32) -> f32 {
        self.y(db).clamp(self.plot.1 + 1.0, self.plot.1 + self.plot.3 - 1.0)
    }

    /// The gain at y (not clamped).
    pub(crate) fn db(&self, y: f32) -> f32 {
        self.range_db - (y - self.plot.1) / self.plot.3 * 2.0 * self.range_db
    }

    /// The dB grid step that suits the range and the plot height.
    pub(crate) fn step(&self) -> f32 {
        let lines = if self.plot.3 > 250.0 { 8.0 } else { 4.0 };
        let raw = 2.0 * self.range_db / lines;
        [1.0, 2.0, 3.0, 6.0, 10.0, 12.0, 15.0].into_iter().find(|&s| s >= raw - 1e-3).unwrap_or(15.0)
    }

    /// Plot background, frequency grid with labels, dB lines with labels. Static.
    pub(crate) fn paint_grid(&self, s: &mut Scene, t: &Theme) {
        kit::plot(s, t, self.plot);
        kit::freq_grid(s, t, self.plot, self.lo, self.hi, self.range_db, 2.0 * self.range_db, self.step());
    }

    /// `n` log-spaced frequencies across the axis.
    pub(crate) fn freqs(&self, n: usize) -> Vec<f64> {
        let n = n.max(2);
        (0..n).map(|i| self.lo as f64 * (self.hi as f64 / self.lo as f64).powf(i as f64 / (n - 1) as f64)).collect()
    }

    /// A curve through `db` at `freqs`, its y kept inside the plot.
    pub(crate) fn points(&self, freqs: &[f64], db: &[f64]) -> Vec<Point> {
        freqs.iter().zip(db).map(|(&f, &d)| Point::new(self.x(f as f32), self.y_in(d as f32))).collect()
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

/// A graph node: a disc in the band colour with its number, a ring when selected, faded
/// when the band is off. `badge` is a placement letter drawn beside it (M, S, L, R), if any.
#[allow(clippy::too_many_arguments, reason = "a drawing primitive: every argument is one plain value of the stroke")]
pub(crate) fn paint_node(s: &mut Scene, t: &Theme, at: Point, colour: C, label: &str, selected: bool, faded: bool, badge: Option<&str>) {
    let r = NODE_D * 0.5;
    let disc = Scene::ellipse_at(at, r);
    let fill = if faded { colour.with_alpha(0.35) } else { colour };
    s.shadow_ellipse(disc.translated(0.0, 1.0), 2.0, t.shadow);
    s.fill_ellipse_solid(disc, fill);
    s.stroke_ellipse(disc, t.plot_bg, 1.2);
    if selected {
        s.stroke_ellipse(Scene::ellipse_at(at, r + 3.0), t.accent, 1.5);
    }
    s.text(at, label, t.on_accent, 8.5, true);
    if let Some(b) = badge {
        s.text(Point::new(at.x + r + 6.0, at.y - r), b, colour, 8.0, true);
    }
}

/// The nearest node within [`NODE_HIT`] of a point.
pub(crate) fn node_at(nodes: &[Point], x: f32, y: f32) -> Option<usize> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, p)| (i, (p.x - x).hypot(p.y - y)))
        .filter(|&(_, d)| d <= NODE_HIT)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// A handle on the 0 dB line (a cut's corner): a small rounded square.
pub(crate) fn paint_handle(s: &mut Scene, t: &Theme, at: Point, colour: C, faded: bool) {
    let r = Rect::new(at.x - 5.0, at.y - 5.0, 10.0, 10.0);
    let c = if faded { colour.with_alpha(0.4) } else { colour };
    s.fill_round(r, 2.5, t.plot_bg, t.plot_bg, true);
    s.stroke_round(r, 2.5, c, 1.6);
}

/// Which audio the spectrum reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Listen {
    /// L and R averaged.
    Both,
    Left,
    Right,
    Mid,
    Side,
}

/// Where the spectrum taps: off, the module's input, or input and output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tap {
    Off,
    Pre,
    Post,
}

impl Tap {
    pub(crate) const LABELS: &'static [&'static str] = &["OFF", "PRE", "POST"];
    pub(crate) fn from_index(i: usize) -> Self {
        [Tap::Off, Tap::Pre, Tap::Post][i.min(2)]
    }
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// The input and output spectra of the shown module (FFT 4096, 1/6 octave, falling).
pub(crate) struct Spectra {
    pub pre: Spectrum,
    pub post: Spectrum,
    scratch: [Vec<f32>; 2],
    /// Something above the floor has been read since the last clear.
    pub live: bool,
}

impl Default for Spectra {
    fn default() -> Self {
        Self { pre: Spectrum::new(4096, 160, F_LO, F_HI), post: Spectrum::new(4096, 160, F_LO, F_HI), scratch: [Vec::new(), Vec::new()], live: false }
    }
}

impl Spectra {
    /// Reads the newest audio. Returns true if either spectrum shows something.
    pub(crate) fn update(&mut self, audio: &Audio, listen: Listen, sample_rate: f32, dt: f32) -> bool {
        let sr = if sample_rate > 1000.0 { sample_rate } else { 48_000.0 };
        let [a, b] = &mut self.scratch;
        for (spec, [l, r]) in [(&mut self.pre, &audio.pre), (&mut self.post, &audio.post)] {
            match listen {
                Listen::Both => spec.update(l, Some(r), sr, dt),
                Listen::Left => spec.update(l, None, sr, dt),
                Listen::Right => spec.update(r, None, sr, dt),
                Listen::Mid | Listen::Side => {
                    let sign = if listen == Listen::Mid { 1.0 } else { -1.0 };
                    a.clear();
                    a.extend(l.iter().zip(r.iter()).map(|(x, y)| 0.5 * (x + sign * y)));
                    spec.update(a, None, sr, dt);
                }
            }
        }
        b.clear();
        self.live = self.pre.db.iter().chain(&self.post.db).any(|&d| d > SPECTRUM_FLOOR + 1.0);
        self.live
    }

    pub(crate) fn clear(&mut self) {
        self.pre.clear();
        self.post.clear();
        self.live = false;
    }
}

/// A spectrum on the graph: filled from the bottom (`fill`), or as a line.
pub(crate) fn paint_spectrum(s: &mut Scene, axis: &Axis, spec: &Spectrum, c: C, alpha: f32, fill: bool) {
    let n = spec.db.len();
    if n < 2 || spec.db.iter().all(|&d| d <= SPECTRUM_FLOOR + 1.0) {
        return;
    }
    let pts: Vec<Point> = (0..n).map(|i| Point::new(axis.x(spec.freq(i)), axis.spec_y(spec.db[i]))).collect();
    if fill {
        kit::area(s, &pts, axis.plot.1 + axis.plot.3, c.with_alpha(alpha));
    } else {
        kit::curve(s, pts, c.with_alpha(alpha), 1.2);
    }
}

/// The input fill and, for [`Tap::Post`], the output line.
pub(crate) fn paint_spectra(s: &mut Scene, t: &Theme, axis: &Axis, spectra: &Spectra, tap: Tap) {
    match tap {
        Tap::Off => {}
        Tap::Pre => paint_spectrum(s, axis, &spectra.pre, t.in_fill, t.in_fill_alpha * 0.6, true),
        Tap::Post => {
            paint_spectrum(s, axis, &spectra.pre, t.in_fill, t.in_fill_alpha * 0.45, true);
            paint_spectrum(s, axis, &spectra.post, t.out_line, 0.8, false);
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

/// A tooltip rect.
pub(crate) fn tip(r: R, text: impl Into<String>) -> Tooltip {
    Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() }
}

/// A node drag in progress: the parameters it moves and the axis it moves them on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NodeDrag {
    /// Host index of the frequency, if the drag moves it.
    pub freq: Option<usize>,
    /// Host index of the gain (or depth), if the drag moves it.
    pub gain: Option<usize>,
    /// Pointer offset from the node centre at the grab, so the node does not jump.
    pub offset: (f32, f32),
}

impl NodeDrag {
    pub(crate) fn begin(self, io: &mut Io) -> Self {
        for p in [self.freq, self.gain].into_iter().flatten() {
            io.begin(p);
        }
        self
    }

    /// Moves the parameters to the pointer.
    pub(crate) fn update(&self, axis: &Axis, io: &mut Io, x: f32, y: f32) {
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

/// Multiplies a log parameter by `2^(octaves)` in one gesture (wheel on a node: width, Q).
pub(crate) fn nudge_log(io: &mut Io, param: usize, octaves: f32) {
    let v = io.value(param);
    io.set(param, v * 2f32.powf(octaves));
}

/// Fills of the cards under a graph: a panel with a band-colour bar on top. Static.
pub(crate) fn paint_card(s: &mut Scene, t: &Theme, r: R, bar: Option<C>) {
    s.fill_round(kit::rect(r), 8.0, t.inset, t.inset, true);
    s.stroke_round(kit::rect(r), 8.0, t.border, 1.0);
    if let Some(c) = bar {
        s.fill_round(Rect::new(r.0 + 8.0, r.1 + 1.0, r.2 - 16.0, 3.0), 1.5, c, c, true);
    }
}

// =====================================================================================
// The Equaliser page
// =====================================================================================

/// Short type labels for the card dropdowns, in `SHAPE_LABELS` order.
const SHORT: [&str; 10] = ["Bell", "Reject", "Lo shelf 3", "Lo shelf 6", "Lo shelf 12", "Hi shelf 3", "Hi shelf 6", "Hi shelf 12", "High-pass", "Low-pass"];
const MATRIX_KEYS: &[&str] = &["L/R", "M/S"];
const RANGES: [f32; 2] = [6.0, 12.0];
const RANGE_KEYS: &[&str] = &["±6", "±12"];

fn short_type(v: f32) -> String {
    SHORT[(v.round().max(0.0) as usize).min(SHORT.len() - 1)].to_string()
}

fn width_text(v: f32) -> String {
    format!("{v:.2} oct")
}

fn gain_text(v: f32) -> String {
    format!("{v:+.1} dB")
}

fn depth_text(v: f32) -> String {
    if v <= -99.5 {
        "-inf".into()
    } else {
        format!("{v:.0} dB")
    }
}

fn hz_text(v: f32) -> String {
    if v >= 10_000.0 {
        format!("{:.1} k", v / 1000.0)
    } else if v >= 1000.0 {
        format!("{:.2} k", v / 1000.0)
    } else {
        format!("{v:.0} Hz")
    }
}

/// x of band card `b` (0..6).
fn card_x(b: usize) -> f32 {
    232.0 + b as f32 * 142.0
}

const LOW_CUT_CARD: R = (120.0, 624.0, 104.0, 148.0);

/// One band's settings as the design function takes them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BandSet {
    on: bool,
    shape: Shape,
    freq: f32,
    gain: f32,
    width: f32,
    depth: f32,
}

impl BandSet {
    fn read(cx: &Ctx, set: usize, b: usize) -> Self {
        let v = |f| cx.v(band_param(set, b, f));
        Self { on: v(F_ON) >= 0.5, shape: Shape::from_index(v(F_TYPE).round().max(0.0) as usize), freq: v(F_FREQ), gain: v(F_GAIN), width: v(F_WIDTH), depth: v(F_DEPTH) }
    }

    fn cascade(&self, fs: f64) -> design::Cascade {
        design::band(self.shape, fs, self.freq as f64, self.gain as f64, self.width as f64, self.depth as f64)
    }

    /// The gain the node sits at.
    fn node_db(&self) -> f32 {
        match self.shape {
            Shape::Reject => self.depth,
            Shape::HighPass | Shape::LowPass => 0.0,
            _ => self.gain,
        }
    }
}

/// Curves for the current values, recomputed when a value, the rate or the plot changes.
struct Curves {
    key: (Vec<f32>, u32, [u32; 4], u32),
    freqs: Vec<f64>,
    /// Each band of the shown set alone (zeros for a band that is off).
    bands: [Vec<f64>; BANDS],
    /// The whole response of the shown set, low cut included.
    shown: Vec<f64>,
    /// The other set's, when the sets are not linked.
    other: Option<Vec<f64>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Node(NodeDrag),
    LowCut(usize),
}

struct Eq6 {
    /// The channel set the cards edit: 0 (L or M), 1 (R or S).
    edit: usize,
    tap: Tap,
    range: usize,
    /// For the parameter walk: the GAIN knobs show DEPTH whatever the type.
    depth_face: bool,
    drag: Option<Drag>,
    spectra: Spectra,
    curves: RefCell<Option<Curves>>,
    /// What the cells were built from: when it changes, the next pointer event rebuilds.
    built: Once<u64>,
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    debug_assert_eq!(count, tone_six::NPARAMS);
    let _ = count;
    Box::new(Eq6 { edit: 0, tap: Tap::Post, range: 1, depth_face: false, drag: None, spectra: Spectra::default(), curves: RefCell::new(None), built: Once::new(0) })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let _ = values;
    (false, false)
}

impl Eq6 {
    fn linked(cx: &Ctx) -> bool {
        cx.on(P_LINK)
    }

    /// The set the graph and cards show.
    fn set(&self) -> usize {
        self.edit
    }

    fn set_letters(cx: &Ctx) -> [&'static str; 2] {
        if cx.choice(P_MATRIX) == 1 {
            ["M", "S"]
        } else {
            ["L", "R"]
        }
    }

    fn axis(&self, cx: &Ctx) -> Axis {
        Axis::new(graph_rect(&cx.zones), RANGES[self.range])
    }

    fn edit_rect(cx: &Ctx) -> R {
        (604.0, cx.zones.header.1 + 8.0, 88.0, 24.0)
    }

    fn tap_rect(cx: &Ctx) -> R {
        (768.0, cx.zones.header.1 + 8.0, 132.0, 24.0)
    }

    fn range_rect(cx: &Ctx) -> R {
        (980.0, cx.zones.picture.1 + 6.0, 92.0, 24.0)
    }

    fn fs(cx: &Ctx) -> f64 {
        if cx.sample_rate > 1000.0 {
            cx.sample_rate as f64
        } else {
            48_000.0
        }
    }

    /// What the cells depend on beyond the page's own state: the edited set's types.
    fn signature(&self, cx: &Ctx) -> u64 {
        let mut h = self.edit as u64 | (self.depth_face as u64) << 1;
        for b in 0..BANDS {
            h = h.wrapping_mul(31).wrapping_add(cx.choice(band_param(self.edit, b, F_TYPE)) as u64 + 1);
        }
        h
    }

    fn listen(&self, cx: &Ctx) -> Listen {
        if Self::linked(cx) {
            Listen::Both
        } else {
            match (cx.choice(P_MATRIX) == 1, self.edit) {
                (false, 0) => Listen::Left,
                (false, _) => Listen::Right,
                (true, 0) => Listen::Mid,
                (true, _) => Listen::Side,
            }
        }
    }

    fn with_curves<T>(&self, cx: &Ctx, axis: &Axis, f: impl FnOnce(&Curves) -> T) -> T {
        let values: Vec<f32> = (0..cx.count).map(|i| cx.v(i)).collect();
        let fs = Self::fs(cx);
        let key = (values, fs as u32, [axis.plot.0.to_bits(), axis.plot.1.to_bits(), axis.plot.2.to_bits(), axis.plot.3.to_bits()], self.edit as u32);
        let mut cache = self.curves.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.key != key) {
            let freqs = axis.freqs(CURVE_POINTS);
            let low_cut = cx.on(P_LOW_CUT_ON).then(|| design::low_cut(fs, cx.v(P_LOW_CUT_FREQ) as f64));
            let set_curves = |set: usize| {
                let bands: [Vec<f64>; BANDS] = std::array::from_fn(|b| {
                    let band = BandSet::read(cx, set, b);
                    if !band.on {
                        return vec![0.0; freqs.len()];
                    }
                    let c = band.cascade(fs);
                    freqs.iter().map(|&f| c.magnitude_db(f, fs)).collect()
                });
                let total: Vec<f64> = (0..freqs.len())
                    .map(|i| bands.iter().map(|b| b[i]).sum::<f64>() + low_cut.as_ref().map_or(0.0, |s| 20.0 * design::sos_magnitude(s, freqs[i], fs).max(1e-12).log10()))
                    .collect();
                (bands, total)
            };
            let (bands, shown) = set_curves(self.edit);
            let other = (!Self::linked(cx)).then(|| set_curves(1 - self.edit).1);
            *cache = Some(Curves { key, freqs, bands, shown, other });
        }
        f(cache.as_ref().expect("built"))
    }

    /// Node centres of the shown set, band order.
    fn nodes(&self, cx: &Ctx, axis: &Axis) -> [Point; BANDS] {
        std::array::from_fn(|b| {
            let band = BandSet::read(cx, self.edit, b);
            Point::new(axis.x(band.freq).clamp(axis.x0(), axis.x1()), axis.y_in(band.node_db()))
        })
    }

    fn low_cut_handle(cx: &Ctx, axis: &Axis) -> Point {
        Point::new(axis.x(cx.v(P_LOW_CUT_FREQ)).max(axis.x0() + 6.0), axis.y(0.0))
    }

    fn band_cells(&self, cx: &Ctx, cells: &mut Vec<Cell>) {
        let set = self.set();
        let letters = Self::set_letters(cx);
        for b in 0..BANDS {
            let x0 = card_x(b);
            let p = |f| cx.p(band_param(set, b, f));
            let label = if set == 0 { format!("{}", b + 1) } else { format!("{}{}", b + 1, letters[1]) };
            cells.push(Cell::pill(p(F_ON), label, (x0 + 4.0, 630.0, 26.0, 18.0)).tip(format!("Band {} on", b + 1)));
            cells.push(Cell::dropdown(p(F_TYPE), "TYPE", (x0 + 34.0, 628.0, 96.0, 22.0)).format(short_type).tip(format!("Band {} shape", b + 1)));
            let shape = Shape::from_index(cx.choice(band_param(set, b, F_TYPE)));
            if shape == Shape::Reject || self.depth_face {
                cells.push(Cell::knob(Size::Small, p(F_DEPTH), "DEPTH", x0, 662.0).format(depth_text).tip("How deep the band reject cuts"));
            } else {
                let tip = if shape.uses_gain() { "Band gain" } else { "A cut has no gain; the node moves only its frequency" };
                cells.push(Cell::knob(Size::Small, p(F_GAIN), "GAIN", x0, 662.0).format(gain_text).tip(tip));
            }
            // Two minis stacked in the card: their rings clear the type box and each other's caption.
            cells.push(Cell::knob(Size::Mini, p(F_FREQ), "FREQ", x0 + 78.0, 655.0).format(hz_text));
            cells.push(Cell::knob(Size::Mini, p(F_WIDTH), "WIDTH", x0 + 78.0, 714.0).format(width_text).tip("Width in octaves: bells and band rejects only"));
        }
    }
}

impl Page for Eq6 {
    fn tall(&self) -> bool {
        true
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let y = cx.zones.header.1;
        let mut cells = vec![
            Cell::segmented(cx.p(P_MATRIX), "MATRIX", (420.0, y + 8.0, 112.0, 24.0)).keys(MATRIX_KEYS).tip("Equalise left and right, or mid and side"),
            Cell::pill(cx.p(P_LINK), "LINK", (540.0, y + 9.0, 56.0, 22.0)).tip("One set of bands for both channels"),
            Cell::pill(cx.p(P_EQ_ON), "EQ IN", (700.0, y + 9.0, 56.0, 22.0)).tip("The bands and the low cut in or out"),
            Cell::pill(cx.p(P_LOW_CUT_ON), "LOW CUT", (144.0, 632.0, 56.0, 22.0)).tip("First-order low cut, both channels"),
            Cell::knob(Size::Small, cx.p(P_LOW_CUT_FREQ), "FREQ", 132.0, 662.0).format(hz_text).tip("Low cut corner, -3 dB"),
        ];
        self.band_cells(cx, &mut cells);
        self.built.set(self.signature(cx));
        cells
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        self.axis(cx).paint_grid(s, t);
        paint_card(s, t, LOW_CUT_CARD, Some(t.dim));
        for b in 0..BANDS {
            paint_card(s, t, (card_x(b), 624.0, 134.0, 148.0), Some(t.band(b)));
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, _live: &Live) {
        let t = cx.t;
        let axis = self.axis(cx);
        let letters = Self::set_letters(cx);
        let linked = Self::linked(cx);
        let eq_in = cx.on(P_EQ_ON);
        clip(s, &axis);
        paint_spectra(s, t, &axis, &self.spectra, self.tap);
        self.with_curves(cx, &axis, |c| {
            for b in 0..BANDS {
                if c.bands[b].iter().any(|d| d.abs() > 0.01) {
                    let pts = axis.points(&c.freqs, &c.bands[b]);
                    fill_to_zero(s, &axis, &pts, t.band(b), if eq_in { FILL } else { FILL * 0.5 });
                }
            }
            if let Some(other) = &c.other {
                let pts = axis.points(&c.freqs, other);
                kit::dashed(s, &pts, 5.0, 4.0, t.dim, 1.0);
            }
            let pts = axis.points(&c.freqs, &c.shown);
            kit::curve(s, pts, if eq_in { t.text } else { t.dim }, 2.0);
        });
        unclip(s);
        if !linked {
            // Which set the solid curve is, and the dashed one.
            let (x, y) = (axis.x0() + 10.0, axis.plot.1 + 12.0);
            s.text(Point::new(x, y), format!("{} solid, {} dashed", letters[self.edit], letters[1 - self.edit]), t.dim, 9.0, false);
        }
        if !eq_in {
            kit::text_right(s, axis.x1() - 110.0, axis.plot.1 + 18.0, "EQ OUT", t.dim, 10.0);
        }
        // Low cut handle, then the nodes on top.
        let lc = Self::low_cut_handle(cx, &axis);
        paint_handle(s, t, lc, t.text, !cx.on(P_LOW_CUT_ON));
        let nodes = self.nodes(cx, &axis);
        let dragged = match self.drag {
            Some(Drag::Node(d)) => (0..BANDS).find(|&b| d.freq == Some(cx.p(band_param(self.edit, b, F_FREQ)))),
            _ => None,
        };
        for (b, &at) in nodes.iter().enumerate() {
            let band = BandSet::read(cx, self.edit, b);
            paint_node(s, t, at, t.band(b), &format!("{}", b + 1), dragged == Some(b), !band.on || !eq_in, None);
        }
        // View switches: EDIT (only without LINK), analyser, range.
        let edit_keys: [&str; 2] = [letters[0], letters[1]];
        paint_switch(s, t, Self::edit_rect(cx), &edit_keys, if linked { 0 } else { self.edit }, !linked);
        paint_switch(s, t, Self::tap_rect(cx), Tap::LABELS, self.tap.index(), true);
        paint_switch(s, t, Self::range_rect(cx), RANGE_KEYS, self.range, true);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let p = cx.zones.picture;
        vec![(p.0, p.1, p.2, p.3), Self::edit_rect(cx), Self::tap_rect(cx)]
    }

    fn wants_audio(&self) -> bool {
        self.tap != Tap::Off
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        let Some(audio) = live.audio else { return false };
        if self.tap == Tap::Off {
            return false;
        }
        let was = self.spectra.live;
        let listen = self.listen(cx);
        self.spectra.update(audio, listen, live.sample_rate, live.dt) || was
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let axis = self.axis(cx);
        let stale = self.built.get() != self.signature(cx);
        let r = self.handle(ev, cx, io, &axis);
        // The cells follow the band types; the shell rebuilds them only when asked.
        if stale && matches!(ev, Pointer::Down { .. } | Pointer::Wheel { .. } | Pointer::Up { .. }) {
            Response { rebuild: true, repaint: true, taken: r.taken }
        } else {
            r
        }
    }

    fn states(&self) -> usize {
        4
    }

    fn set_state(&mut self, i: usize) {
        self.edit = i % 2;
        self.depth_face = i / 2 == 1;
    }

    fn menu(&self, cx: &Ctx) -> Vec<String> {
        let l = Self::set_letters(cx);
        vec![format!("Copy {} to {}", l[self.edit], l[1 - self.edit])]
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        if item != 0 {
            return Response::NONE;
        }
        let (from, to) = (self.edit, 1 - self.edit);
        for b in 0..BANDS {
            for f in [F_ON, F_TYPE, F_FREQ, F_GAIN, F_WIDTH, F_DEPTH] {
                io.set(cx.p(band_param(to, b, f)), cx.v(band_param(from, b, f)));
            }
        }
        Response::REBUILD
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let axis = self.axis(cx);
        let mut tips = vec![
            tip(Self::edit_rect(cx), "Which channel set the cards and nodes edit (with LINK off)"),
            tip(Self::tap_rect(cx), "Spectrum: off, the module's input, or input and output"),
            tip(Self::range_rect(cx), "Graph range"),
        ];
        for (b, at) in self.nodes(cx, &axis).iter().enumerate() {
            let band = BandSet::read(cx, self.edit, b);
            let what = match band.shape {
                Shape::Reject => format!("{} {}", hz_text(band.freq), depth_text(band.depth)),
                Shape::HighPass | Shape::LowPass => hz_text(band.freq),
                _ => format!("{} {}", hz_text(band.freq), gain_text(band.gain)),
            };
            let r = (at.x - NODE_D * 0.5, at.y - NODE_D * 0.5, NODE_D, NODE_D);
            tips.push(tip(r, format!("Band {} {}: {what}. Drag; wheel for width; double-click for 0 dB", b + 1, SHORT[band.shape as usize])));
        }
        let lc = Self::low_cut_handle(cx, &axis);
        tips.push(tip((lc.x - 6.0, lc.y - 6.0, 12.0, 12.0), format!("Low cut {}: drag sideways", hz_text(cx.v(P_LOW_CUT_FREQ)))));
        tips
    }
}

impl Eq6 {
    fn handle(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io, axis: &Axis) -> Response {
        match ev {
            Pointer::Down { x, y, clicks } => {
                if let Some(k) = switch_key(Self::range_rect(cx), RANGE_KEYS.len(), x, y) {
                    if k != self.range {
                        self.range = k;
                        return Response::REBUILD;
                    }
                    return Response::TAKEN;
                }
                if let Some(k) = switch_key(Self::tap_rect(cx), Tap::LABELS.len(), x, y) {
                    self.tap = Tap::from_index(k);
                    self.spectra.clear();
                    return Response::TAKEN;
                }
                if let Some(k) = switch_key(Self::edit_rect(cx), 2, x, y) {
                    if (Self::linked(cx) && k == 1) || k == self.edit {
                        return Response::TAKEN;
                    }
                    self.edit = k;
                    return Response::REBUILD;
                }
                if !kit::inside(axis.plot, x, y) {
                    return Response::NONE;
                }
                let nodes = self.nodes(cx, axis);
                if let Some(b) = node_at(&nodes, x, y) {
                    let band = BandSet::read(cx, self.edit, b);
                    let p = |f| cx.p(band_param(self.edit, b, f));
                    let level = match band.shape {
                        Shape::Reject => Some(p(F_DEPTH)),
                        Shape::HighPass | Shape::LowPass => None,
                        _ => Some(p(F_GAIN)),
                    };
                    if clicks >= 2 {
                        if let Some(g) = level {
                            io.set(g, 0.0);
                        }
                        return Response::TAKEN;
                    }
                    let d = NodeDrag { freq: Some(p(F_FREQ)), gain: level, offset: (x - nodes[b].x, y - nodes[b].y) }.begin(io);
                    self.drag = Some(Drag::Node(d));
                    return Response::TAKEN;
                }
                let lc = Self::low_cut_handle(cx, axis);
                if (lc.x - x).abs() <= 8.0 && (lc.y - y).abs() <= 8.0 {
                    let p = cx.p(P_LOW_CUT_FREQ);
                    io.begin(p);
                    self.drag = Some(Drag::LowCut(p));
                    return Response::TAKEN;
                }
                Response::NONE
            }
            Pointer::Move { x, y } => match self.drag {
                Some(Drag::Node(d)) => {
                    d.update(axis, io, x, y);
                    Response::TAKEN
                }
                Some(Drag::LowCut(p)) => {
                    // The handle's corner may sit below the graph's 20 Hz edge.
                    let f = if x <= axis.x0() + 6.0 { kit::x_freq(x, axis.x0() - 120.0, axis.x0() + 6.0, 2.0, axis.freq(axis.x0() + 6.0)) } else { axis.freq(x) };
                    io.perform(p, f);
                    Response::TAKEN
                }
                None => Response::NONE,
            },
            Pointer::Up { .. } => {
                match self.drag.take() {
                    Some(Drag::Node(d)) => d.end(io),
                    Some(Drag::LowCut(p)) => io.end(p),
                    None => return Response::NONE,
                }
                Response::TAKEN
            }
            Pointer::Wheel { x, y, dy } => {
                let nodes = self.nodes(cx, axis);
                let Some(b) = node_at(&nodes, x, y) else { return Response::NONE };
                nudge_log(io, cx.p(band_param(self.edit, b, F_WIDTH)), if dy > 0.0 { 0.125 } else { -0.125 });
                Response::TAKEN
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_axis_maps_both_ways() {
        let a = Axis::new((152.0, 252.0, 896.0, 340.0), 12.0);
        assert!((a.x(20.0) - 152.0).abs() < 1e-3 && (a.x(20_000.0) - 1048.0).abs() < 1e-3);
        assert!((a.freq(a.x(1000.0)) - 1000.0).abs() < 0.5);
        assert!((a.y(0.0) - 422.0).abs() < 1e-3);
        assert!((a.db(a.y(-7.5)) + 7.5).abs() < 1e-4);
        assert_eq!(node_at(&[Point::new(10.0, 10.0), Point::new(30.0, 10.0)], 27.0, 12.0), Some(1));
        assert_eq!(node_at(&[Point::new(10.0, 10.0)], 40.0, 40.0), None);
        assert_eq!(switch_key((0.0, 0.0, 90.0, 20.0), 3, 61.0, 5.0), Some(2));
    }

    #[test]
    fn short_labels_cover_every_shape() {
        assert_eq!(SHORT.len(), design::SHAPE_LABELS.len());
        // The card parameter constants are the crate's own ids.
        assert_eq!(tone_six::PARAMS[band_param(1, 5, F_GAIN)].id, "b6GainCh2");
        assert_eq!(tone_six::PARAMS[P_LOW_CUT_FREQ].id, "lowCutFreq");
    }

    /// Writes `build/ui-tone-<key>-<theme>-<mode>.ppm` for the Equaliser and the Shaper in
    /// both modes, with real module output in the taps and read-out. Run with `--ignored`.
    #[test]
    #[ignore]
    fn dump_tone_pages() {
        use crate::ui::tests::{editor, feed};
        use crate::ui::theme::ThemeId;
        use crate::LAYOUT;
        use olm_api::Module;
        use openplugin::core::Editor;
        let dir = format!("{}/build", env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir_all(&dir).expect("dir");
        for (key, settings) in [
            ("eq", vec![(band_param(0, 0, F_GAIN), 4.0), (band_param(0, 1, F_GAIN), -3.0), (band_param(0, 2, F_TYPE), 1.0), (band_param(0, 2, F_DEPTH), -9.0), (band_param(0, 3, F_GAIN), 2.5), (band_param(0, 4, F_GAIN), -5.0), (band_param(0, 5, F_GAIN), 3.0), (P_LOW_CUT_ON, 1.0), (P_LOW_CUT_FREQ, 40.0), (P_LINK, 0.0), (band_param(1, 3, F_GAIN), -4.0)]),
            ("shaper", vec![(shaper::P_CURVE, 4.0), (shaper::P_THRESHOLD, -6.0), (shaper::P_DRIVE, 3.0), (shaper::P_UPWARD, 4.0), (shaper::P_UPWARD_THRESHOLD, -30.0), (shaper::P_TRANSIENT, 80.0)]),
        ] {
            let slot = LAYOUT.slots.iter().position(|s| s.key == key).expect("slot");
            let base = LAYOUT.slots[slot].base;
            for (id, theme) in ThemeId::ALL.map(|t| (t, t.name())) {
                for tall in [false, true] {
                    let (mut e, shared) = editor(id);
                    feed(&shared, 500);
                    e.param_changed(LAYOUT.slots[slot].on, 1.0);
                    for &(local, v) in &settings {
                        e.param_changed(base + local, v);
                    }
                    e.open_page(slot);
                    e.set_tall(tall);
                    let mut module: Box<dyn Module> = if key == "eq" { Box::new(tone_six::ToneSix::default()) } else { Box::new(shaper::Shaper::default()) };
                    module.prepare(48_000.0, 512);
                    for &(local, v) in &settings {
                        module.set_param(local, v);
                    }
                    let mut seed = 1u32;
                    let mut phase = 0.0f32;
                    for frame in 0..40 {
                        for _ in 0..4 {
                            let swell = 0.15 + 0.35 * ((frame as f32) * 0.31).sin().abs();
                            let mut l = vec![0.0f32; 512];
                            for v in l.iter_mut() {
                                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                                let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5;
                                phase += 2.0 * std::f32::consts::PI * 110.0 / 48_000.0;
                                *v = swell * (0.6 * noise + 0.5 * phase.sin());
                            }
                            let r: Vec<f32> = l.iter().map(|x| x * 0.8).collect();
                            let (pre_l, pre_r) = (l.clone(), r.clone());
                            let (mut ol, mut or) = (l, r);
                            module.process(&mut ol, &mut or);
                            shared.push_taps([&pre_l, &pre_r], [&ol, &or], None);
                            let mut view = [0.0f32; 16];
                            let n = module.view(&mut view);
                            shared.set_view(slot, &view[..n]);
                            shared.raise_slot_gr(slot, module.meters().reduction_db);
                        }
                        e.tick(1.0 / 30.0);
                    }
                    let f = e.render(2.0);
                    let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                    for px in f.pixels {
                        out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
                    }
                    let mode = if tall { "tall" } else { "standard" };
                    std::fs::write(format!("{dir}/ui-tone-{key}-{theme}-{mode}.ppm"), out).expect("write");
                }
            }
        }
    }
}
