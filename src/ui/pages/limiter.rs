//! The limiter page (the original engine, slot key ""), and the frame the four limiter pages
//! share (`design/limiters.md` section 0): CURVE, RECOVERY, TRACE, the GR meter, the OUT MAX
//! badge, the editor-side probe that draws CURVE and RECOVERY, and page-drawn knobs for the
//! controls a plain cell cannot do (one knob on two parameters, a knob whose parameter
//! follows a switch, a knob whose travel is shorter than its parameter's range).

use std::sync::mpsc;

use olm_api::Module;
use openplugin::core::editor::{DamageRect, Tooltip};
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use goodlookindri_shell::text;

use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::ui::theme::Theme;
use crate::INFO;

// =====================================================================================
// Shared geometry (design/limiters.md section 0)
// =====================================================================================

/// Columns of the character zone.
pub(super) const COL: [f32; 6] = [520.0, 616.0, 712.0, 808.0, 904.0, 1000.0];
pub(super) const ROW1: f32 = 632.0;
pub(super) const ROW2: f32 = 738.0;
pub(super) const BIG_Y: f32 = 636.0;
pub(super) const DRIVE_X: f32 = 120.0;
pub(super) const CEILING_X: f32 = 248.0;
pub(super) const RELEASE_X: f32 = 376.0;
/// MORE face: knob row and pill row.
pub(super) const MORE_KNOB_Y: f32 = 648.0;
pub(super) const MORE_PILL_Y: f32 = 679.0;
/// The GR meter shows this many dB.
pub(super) const GR_RANGE: f32 = 12.0;
/// The transfer curve's axes, dB.
pub(super) const CURVE_LO: f32 = -36.0;
/// The recovery plot's time axis, ms, and depth, dB.
pub(super) const REC_T0: f32 = 1.0;
pub(super) const REC_T1: f32 = 5000.0;
pub(super) const REC_DEPTH: f32 = 6.0;

/// Where the pictures of a limiter page are, for one mode.
#[derive(Clone, Copy, Debug)]
pub(super) struct Geo {
    /// CURVE panel and its plot, if the page has one.
    pub curve: Option<(R, R)>,
    pub rec: (R, R),
    pub trace: (R, R),
    /// The GR bar (14 wide; the scale is beside it).
    pub meter: R,
    pub maxbox: R,
}

pub(super) fn geo(tall: bool, has_curve: bool) -> Geo {
    let curve = has_curve.then_some(((120.0, 416.0, 200.0, 200.0), (148.0, 424.0, 164.0, 164.0)));
    let rec = match (tall, has_curve) {
        (false, true) => ((328.0, 416.0, 300.0, 200.0), (360.0, 440.0, 256.0, 148.0)),
        (true, true) => ((328.0, 416.0, 700.0, 200.0), (360.0, 440.0, 656.0, 148.0)),
        (false, false) => ((120.0, 416.0, 508.0, 200.0), (152.0, 440.0, 464.0, 148.0)),
        (true, false) => ((120.0, 416.0, 908.0, 200.0), (152.0, 440.0, 864.0, 148.0)),
    };
    let trace = if tall { ((120.0, 244.0, 916.0, 164.0), (152.0, 268.0, 876.0, 124.0)) } else { ((636.0, 416.0, 392.0, 200.0), (668.0, 440.0, 352.0, 148.0)) };
    let meter = if tall { (1044.0, 244.0, 14.0, 348.0) } else { (1044.0, 416.0, 14.0, 176.0) };
    Geo { curve, rec, trace, meter, maxbox: (1036.0, 598.0, 44.0, 18.0) }
}

/// The OUT MAX badge, at the right end of the header slot.
pub(super) fn badge_rect(cx: &Ctx) -> R {
    (764.0, cx.zones.header.1 + 9.0, 136.0, 22.0)
}

/// The rects a limiter page's pictures paint into.
pub(super) fn picture_rects(cx: &Ctx, has_curve: bool) -> Vec<R> {
    let g = geo(cx.zones.tall, has_curve);
    let mut v = vec![g.rec.0, g.trace.0, (1034.0, g.meter.1 - 2.0, 48.0, 618.0 - g.meter.1), badge_rect(cx)];
    if let Some((p, _)) = g.curve {
        v.push(p);
    }
    v
}

// =====================================================================================
// Small helpers
// =====================================================================================

pub(super) fn local(cx: &Ctx, id: &str) -> usize {
    cx.find(id).unwrap_or_else(|| panic!("parameter {id}"))
}

/// Host index of the module's parameter `id`.
pub(super) fn host(cx: &Ctx, id: &str) -> usize {
    cx.p(local(cx, id))
}

pub(super) fn val(cx: &Ctx, id: &str) -> f32 {
    cx.v(local(cx, id))
}

pub(super) fn module_values(cx: &Ctx) -> Vec<f32> {
    (0..cx.count).map(|i| cx.v(i)).collect()
}

pub(super) fn db(x: f32) -> f32 {
    20.0 * x.abs().max(1e-9).log10()
}

pub(super) fn tip(r: R, text: impl Into<String>) -> Tooltip {
    Tooltip { rect: DamageRect::new(r.0, r.1, r.2, r.3), text: text.into() }
}

fn ms_text(ms: f32) -> String {
    if ms >= 1000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else if ms >= 10.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{ms:.1} ms")
    }
}

/// A pill the page paints itself (not a parameter's).
pub(super) fn page_pill(s: &mut Scene, t: &Theme, r: R, label: &str, on: bool, over: bool) {
    let rr = kit::rect(r);
    let radius = r.3 * 0.5;
    if on {
        s.fill_round(rr, radius, t.accent, t.accent, true);
    } else {
        s.fill_round(rr, radius, t.inset, t.inset, true);
        s.stroke_round(rr, radius, if over { t.over } else { t.border }, if over { 1.5 } else { 1.0 });
    }
    let ink = if on { t.on_accent } else if over { t.over } else { t.text };
    s.text(rr.center(), text::ellipsize(label, 9.5, r.2 - 8.0), ink, 9.5, true);
}

/// A bracket under a group of controls, with its caption in the middle.
pub(super) fn bracket(s: &mut Scene, t: &Theme, x0: f32, x1: f32, y: f32, label: &str, ink: C) {
    let w = text::width(label, 9.0) + 12.0;
    let mid = (x0 + x1) * 0.5;
    let yl = y + 6.0;
    s.line(Point::new(x0, y), Point::new(x0, yl), t.grid_strong, 1.0);
    s.line(Point::new(x1, y), Point::new(x1, yl), t.grid_strong, 1.0);
    s.line(Point::new(x0, yl), Point::new(mid - w * 0.5, yl), t.grid_strong, 1.0);
    s.line(Point::new(mid + w * 0.5, yl), Point::new(x1, yl), t.grid_strong, 1.0);
    s.text(Point::new(mid, yl), label, ink, 9.0, true);
}

/// The divider between the three big knobs and the character zone.
pub(super) fn divider(s: &mut Scene, t: &Theme, x: f32) {
    s.line(Point::new(x, 632.0), Point::new(x, 764.0), t.border, 1.0);
}

// =====================================================================================
// The editor-side probe
// =====================================================================================

/// Builds a fresh instance of the module.
pub(super) type Factory = fn() -> Box<dyn Module>;

/// What the probe measured for one set of parameters.
#[derive(Clone, Debug, Default)]
pub(super) struct ProbeOut {
    /// Settled output peak, dBFS, for inputs of -36..0 dBFS in 1 dB steps.
    pub curve: Vec<f32>,
    /// The same with the page's alternate switch flipped (Clear: Peak Guard).
    pub alt: Vec<f32>,
    /// Output minus input at -36 dBFS.
    pub small_gain: f32,
    /// Gain after a 5 ms and a 500 ms burst, dB relative to before it, per ms after its end.
    pub short: Vec<f32>,
    pub long: Vec<f32>,
    pub latency: usize,
    /// The latency of every choice of the scanned parameter (Mode Limiter: every style).
    pub scan: Vec<usize>,
    /// The values (and sample rate, last) this was measured for.
    pub values: Vec<f32>,
}

impl ProbeOut {
    /// Time to half recovery, ms, of a recovery curve; `None` without a reduction.
    pub fn half_time(rec: &[f32]) -> Option<f32> {
        let deepest = rec.iter().take(3).fold(0.0f32, |a, &b| a.min(b));
        if deepest > -0.1 {
            return None;
        }
        rec.iter().position(|&g| g >= deepest * 0.5).map(|i| (i + 1) as f32)
    }
}

#[derive(Clone)]
struct Job {
    factory: Factory,
    values: Vec<f32>,
    sr: f32,
    overrides: Vec<(usize, f32)>,
    alt: Option<usize>,
    scan: Option<usize>,
}

const BLOCK: usize = 256;

fn build(job: &Job, values: &[f32]) -> Box<dyn Module> {
    let mut m = (job.factory)();
    for (i, v) in values.iter().enumerate() {
        m.set_param(i, *v);
    }
    for &(i, v) in &job.overrides {
        m.set_param(i, v);
    }
    m.prepare(job.sr, BLOCK);
    for (i, v) in values.iter().enumerate() {
        m.set_param(i, *v);
    }
    for &(i, v) in &job.overrides {
        m.set_param(i, v);
    }
    m.reset();
    m
}

/// Runs a mono signal through both channels; returns the left output.
fn run(m: &mut dyn Module, x: &[f32]) -> Vec<f32> {
    let mut out = x.to_vec();
    let mut r = vec![0.0f32; BLOCK];
    for chunk in out.chunks_mut(BLOCK) {
        let r = &mut r[..chunk.len()];
        r.copy_from_slice(chunk);
        m.process(chunk, r);
    }
    out
}

fn tone(n: usize, sr: f32, level: impl Fn(usize) -> f32) -> Vec<f32> {
    let w = std::f32::consts::TAU * 1000.0 / sr;
    (0..n).map(|i| level(i) * (w * i as f32).sin()).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |a, &b| a.max(b.abs()))
}

fn static_curve(job: &Job, values: &[f32]) -> (Vec<f32>, usize) {
    let mut m = build(job, values);
    let lat = m.latency();
    let sr = job.sr;
    let seg = (0.05 * sr) as usize;
    let steps = (-CURVE_LO) as usize + 1;
    let n = steps * seg + lat + seg;
    let x = tone(n, sr, |i| 10f32.powf((CURVE_LO + (i / seg).min(steps - 1) as f32) / 20.0));
    let y = run(m.as_mut(), &x);
    let curve = (0..steps)
        .map(|k| {
            let a = (k * seg + seg * 6 / 10 + lat).min(n);
            let b = ((k + 1) * seg + lat).min(n);
            db(peak(&y[a..b]))
        })
        .collect();
    (curve, lat)
}

/// Gain per ms after a burst of `burst_ms` 6 dB over `limit`, from 12 dB under it.
fn recovery(job: &Job, values: &[f32], limit: f32, burst_ms: f32) -> Vec<f32> {
    let mut m = build(job, values);
    let lat = m.latency();
    let sr = job.sr;
    let bin = (sr / 1000.0).round().max(1.0) as usize;
    let pre = 300 * bin;
    let burst = (burst_ms as usize).max(1) * bin;
    let tail = REC_T1 as usize * bin;
    let n = pre + burst + tail + lat + bin;
    let (lo, hi) = (10f32.powf((limit - 12.0) / 20.0), 10f32.powf((limit + 6.0) / 20.0).min(4.0));
    let x = tone(n, sr, |i| if (pre..pre + burst).contains(&i) { hi } else { lo });
    let y = run(m.as_mut(), &x);
    let gain = |b: usize| {
        let a = b * bin;
        let i = peak(&x[a..a + bin]);
        let o = peak(&y[(a + lat).min(n - bin)..(a + lat + bin).min(n)]);
        db(o) - db(i)
    };
    let before: f32 = ((pre / bin - 100)..pre / bin).map(gain).sum::<f32>() / 100.0;
    let start = (pre + burst) / bin;
    (0..REC_T1 as usize).map(|k| (gain(start + k) - before).min(0.5)).collect()
}

fn measure(job: Job) -> ProbeOut {
    let (curve, latency) = static_curve(&job, &job.values);
    let small_gain = curve[0] - CURVE_LO;
    let limit = curve
        .iter()
        .enumerate()
        .find(|(k, &o)| o - (CURVE_LO + *k as f32) < small_gain - 1.0)
        .map_or(0.0, |(k, _)| CURVE_LO + k as f32 - 1.0);
    let alt = job.alt.map_or_else(Vec::new, |i| {
        let mut v = job.values.clone();
        if let Some(x) = v.get_mut(i) {
            *x = if *x >= 0.5 { 0.0 } else { 1.0 };
        }
        static_curve(&job, &v).0
    });
    let short = recovery(&job, &job.values, limit, 5.0);
    let long = recovery(&job, &job.values, limit, 500.0);
    let scan = job.scan.map_or_else(Vec::new, |i| {
        let mut m = build(&job, &job.values);
        let n = m.params().get(i).map_or(0, |d| (d.max - d.min).round() as usize + 1);
        (0..n)
            .map(|c| {
                m.set_param(i, c as f32);
                m.latency()
            })
            .collect()
    });
    let mut values = job.values;
    values.push(job.sr);
    ProbeOut { curve, alt, small_gain, short, long, latency, scan, values }
}

/// A second instance of the module, run off the editor thread whenever the page's
/// parameters have been still for 120 ms. Under `cfg(test)` it runs in place, so renders
/// are repeatable.
pub(super) struct Probe {
    factory: Factory,
    overrides: Vec<(usize, f32)>,
    alt: Option<usize>,
    scan: Option<usize>,
    pub out: Option<ProbeOut>,
    /// The measurement `out` replaced, until the page takes it.
    pub prev: Option<ProbeOut>,
    /// The values (and sample rate) last seen on the page.
    wanted: Vec<f32>,
    quiet: f32,
    running: Option<(Vec<f32>, mpsc::Receiver<ProbeOut>)>,
}

impl Probe {
    pub fn new(factory: Factory) -> Self {
        Self { factory, overrides: Vec::new(), alt: None, scan: None, out: None, prev: None, wanted: Vec::new(), quiet: 0.0, running: None }
    }

    /// Parameters the probe holds at a value whatever the page says (local index, value).
    pub fn overrides(mut self, o: Vec<(usize, f32)>) -> Self {
        self.overrides = o;
        self
    }

    /// Also measure the static curve with this toggle flipped.
    pub fn alt(mut self, local: usize) -> Self {
        self.alt = Some(local);
        self
    }

    /// Also read the latency for every choice of this parameter.
    pub fn scan(mut self, local: usize) -> Self {
        self.scan = Some(local);
        self
    }

    /// The picture on screen is not for the present values.
    pub fn stale(&self) -> bool {
        self.out.as_ref().is_none_or(|o| o.values != self.wanted)
    }

    /// True when a new measurement arrived.
    pub fn tick(&mut self, cx: &Ctx, dt: f32) -> bool {
        let mut now = module_values(cx);
        now.push(if cx.sample_rate > 1000.0 { cx.sample_rate } else { 48_000.0 });
        if now != self.wanted {
            self.wanted = now;
            self.quiet = 0.0;
        } else {
            self.quiet += dt;
        }
        let mut arrived = false;
        if let Some((_, rx)) = &self.running {
            match rx.try_recv() {
                Ok(out) => {
                    self.prev = self.out.replace(out);
                    self.running = None;
                    arrived = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.running = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.running.is_none() && self.stale() && (self.out.is_none() || self.quiet >= 0.12) {
            let mut values = self.wanted.clone();
            let sr = values.pop().unwrap_or(48_000.0);
            let job = Job { factory: self.factory, values, sr, overrides: self.overrides.clone(), alt: self.alt, scan: self.scan };
            if cfg!(test) {
                self.prev = self.out.replace(measure(job));
                arrived = true;
            } else {
                let (tx, rx) = mpsc::channel();
                let spawned = std::thread::Builder::new().name("olm-probe".into()).spawn(move || {
                    let _ = tx.send(measure(job));
                });
                if spawned.is_ok() {
                    self.running = Some((self.wanted.clone(), rx));
                }
            }
        }
        arrived
    }
}

// =====================================================================================
// What moves: the shared state of a limiter page
// =====================================================================================

/// The live parts every limiter page has.
pub(super) struct Common {
    pub probe: Probe,
    /// Deepest reduction since the max box was clicked.
    pub max_gr: f32,
    /// Peak at the module's input and output over the last tap, dBFS.
    pub peaks: (f32, f32),
    pub drag: Option<KnobDrag>,
}

impl Common {
    pub fn new(probe: Probe) -> Self {
        Self { probe, max_gr: 0.0, peaks: (-120.0, -120.0), drag: None }
    }

    pub fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        self.probe.tick(cx, live.dt);
        self.max_gr = self.max_gr.max(live.gr);
        if let Some(a) = live.audio {
            let tail = |v: &Vec<f32>| peak(&v[v.len().saturating_sub(2048)..]);
            self.peaks = (db(tail(&a.pre[0]).max(tail(&a.pre[1]))), db(tail(&a.post[0]).max(tail(&a.post[1]))));
        }
        true
    }

    /// The max box, and the page-drawn knobs.
    pub fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io, knobs: &[Knob]) -> Response {
        if let Pointer::Down { x, y, .. } = ev {
            if kit::inside(geo(cx.zones.tall, true).maxbox, x, y) {
                self.max_gr = 0.0;
                return Response::TAKEN;
            }
        }
        knob_pointer(&mut self.drag, ev, io, knobs)
    }
}

/// Paints what never moves: plot frames, grids, axis text, the meter scale.
pub(super) fn paint_pictures_static(s: &mut Scene, t: &Theme, tall: bool, has_curve: bool) {
    let g = geo(tall, has_curve);
    if let Some((_, p)) = g.curve {
        kit::plot(s, t, p);
        let (x0, y0, w, h) = p;
        for d in [-12.0f32, -24.0] {
            let f = (d - CURVE_LO) / -CURVE_LO;
            s.line(Point::new(x0 + w * f, y0), Point::new(x0 + w * f, y0 + h), t.grid, 1.0);
            s.line(Point::new(x0, y0 + h * (1.0 - f)), Point::new(x0 + w, y0 + h * (1.0 - f)), t.grid, 1.0);
        }
        kit::dashed(s, &[Point::new(x0, y0 + h), Point::new(x0 + w, y0)], 4.0, 3.0, t.grid_strong, 1.0);
        for d in [-36.0f32, -24.0, -12.0, 0.0] {
            let f = (d - CURVE_LO) / -CURVE_LO;
            s.text(Point::new(x0 + w * f, y0 + h + 9.0), format!("{d:.0}"), t.dim, 8.5, true);
            kit::text_right(s, x0 - 4.0, y0 + h * (1.0 - f), &format!("{d:.0}"), t.dim, 8.5);
        }
        s.text(Point::new(x0 + w * 0.5, y0 + h + 21.0), "IN dBFS", t.dim, 8.0, true);
    }
    // RECOVERY
    let (rp, r) = g.rec;
    s.text(Point::new(rp.0 + 8.0, rp.1 + 12.0), "RECOVERY", t.dim, 9.0, false);
    s.text(Point::new(rp.0 + 8.0 + text::width("RECOVERY", 9.0) + 8.0, rp.1 + 12.0), "thin 5 ms peak · thick 500 ms", t.dim, 8.5, false);
    kit::plot(s, t, r);
    for d in [0.0f32, 3.0, 6.0] {
        let y = r.1 + d / REC_DEPTH * r.3;
        if d > 0.0 {
            s.line(Point::new(r.0, y), Point::new(r.0 + r.2, y), t.grid, 1.0);
        }
        kit::text_right(s, r.0 - 4.0, y, &if d == 0.0 { "0".to_string() } else { format!("-{d:.0}") }, t.dim, 8.5);
    }
    for (ms, label) in [(1.0f32, "1 ms"), (10.0, "10 ms"), (100.0, "100 ms"), (1000.0, "1 s"), (5000.0, "5 s")] {
        let x = kit::freq_x(ms, r.0, r.0 + r.2, REC_T0, REC_T1);
        s.line(Point::new(x, r.1), Point::new(x, r.1 + r.3), t.grid, 1.0);
        let lx = x.clamp(r.0 + 12.0, r.0 + r.2 - 10.0);
        s.text(Point::new(lx, r.1 + r.3 + 10.0), label, t.dim, 8.5, true);
    }
    // TRACE
    let (tp, tr) = g.trace;
    s.text(Point::new(tp.0 + 8.0, tp.1 + 12.0), "TRACE", t.dim, 9.0, false);
    s.text(Point::new(tp.0 + 8.0 + text::width("TRACE", 9.0) + 8.0, tp.1 + 12.0), "this module's reduction, 4 s", t.dim, 8.5, false);
    kit::plot(s, t, tr);
    for d in [3.0f32, 6.0, 9.0] {
        let y = tr.1 + d / GR_RANGE * tr.3;
        s.line(Point::new(tr.0, y), Point::new(tr.0 + tr.2, y), t.grid, 1.0);
        kit::text_right(s, tr.0 - 4.0, y, &format!("-{d:.0}"), t.dim, 8.5);
    }
    for k in 1..4 {
        let x = tr.0 + tr.2 * k as f32 / 4.0;
        s.line(Point::new(x, tr.1), Point::new(x, tr.1 + tr.3), t.grid, 1.0);
    }
    for (k, label) in ["-4 s", "-3", "-2", "-1", "now"].iter().enumerate() {
        let x = (tr.0 + tr.2 * k as f32 / 4.0).clamp(tr.0 + 10.0, tr.0 + tr.2 - 10.0);
        s.text(Point::new(x, tr.1 + tr.3 + 10.0), *label, t.dim, 8.5, true);
    }
    // GR meter
    kit::gr_scale(s, t, g.meter, GR_RANGE);
    kit::inset(s, t, g.maxbox, 4.0);
}

/// What CURVE shows besides the probe's curve.
pub(super) struct CurveExtras {
    /// The ceiling line, dBFS (None: no ceiling).
    pub ceiling: Option<f32>,
    /// A lamp in the plot's corner: (label, lit).
    pub lamp: Option<(&'static str, bool)>,
    /// A note on the lower straight part.
    pub note: Option<String>,
}

fn curve_xy(p: R, i: f32, o: f32) -> Point {
    let (x0, y0, w, h) = p;
    Point::new(x0 + w * ((i - CURVE_LO) / -CURVE_LO), y0 + h * (1.0 - (o.clamp(CURVE_LO - 2.0, 2.0) - CURVE_LO) / -CURVE_LO))
}

/// Paints the moving parts of the pictures.
pub(super) fn paint_pictures(s: &mut Scene, cx: &Ctx, live: &Live, common: &Common, extras: &CurveExtras, has_curve: bool) {
    let t = cx.t;
    let g = geo(cx.zones.tall, has_curve);
    let out = common.probe.out.as_ref();
    let stale = common.probe.stale();
    let fade = if stale { 0.45 } else { 1.0 };
    // CURVE
    if let Some((_, p)) = g.curve {
        s.text(Point::new(p.0 + 6.0, p.1 + 10.0), "CURVE", t.dim, 8.5, false);
        s.clip_round(kit::rect(p), 4.0);
        if let Some(o) = out {
            let pts: Vec<Point> = o.curve.iter().enumerate().map(|(k, &y)| curve_xy(p, CURVE_LO + k as f32, y)).collect();
            for (k, pair) in o.curve.windows(2).enumerate() {
                let (i0, i1) = (CURVE_LO + k as f32, CURVE_LO + k as f32 + 1.0);
                let up = (pair[0] - i0) + (pair[1] - i1) > 0.0;
                let c = if up { t.accent.with_alpha(0.15 * fade) } else { t.accent.with_alpha(0.20 * fade) };
                s.fill_poly(vec![curve_xy(p, i0, i0), curve_xy(p, i0, pair[0]), curve_xy(p, i1, pair[1]), curve_xy(p, i1, i1)], c, c);
            }
            if !o.alt.is_empty() {
                let alt: Vec<Point> = o.alt.iter().enumerate().map(|(k, &y)| curve_xy(p, CURVE_LO + k as f32, y)).collect();
                kit::dashed(s, &alt, 3.0, 3.0, t.dim.with_alpha(fade), 1.0);
            }
            kit::curve(s, pts, t.accent.with_alpha(fade), 2.0);
        }
        if let Some(c) = extras.ceiling {
            let y = curve_xy(p, 0.0, c).y;
            s.line(Point::new(p.0, y), Point::new(p.0 + p.2, y), t.accent.with_alpha(0.7), 1.0);
            s.fill_round(Rect::new(p.0 + p.2 - 14.0, y - 4.0, 12.0, 8.0), 3.0, t.accent, t.accent, true);
        }
        let (pi, po) = common.peaks;
        if pi > CURVE_LO && live.audio.is_some() {
            let d = curve_xy(p, pi.min(0.0), po);
            s.fill_ellipse_solid(Scene::ellipse_at(d, 4.0), t.text);
        }
        s.pop_clip();
        if let Some((label, lit)) = extras.lamp {
            let w = text::width(label, 8.0) + 12.0;
            let r = Rect::new(p.0 + p.2 - w - 4.0, p.1 + p.3 - 18.0, w, 14.0);
            if lit {
                s.fill_round(r, 7.0, t.over, t.over, true);
            } else {
                s.stroke_round(r, 7.0, t.border, 1.0);
            }
            s.text(r.center(), label, if lit { t.on_accent } else { t.dim }, 8.0, true);
        }
        if let Some(note) = &extras.note {
            s.text(Point::new(p.0 + p.2 - 4.0 - text::width(note, 8.5), p.1 + p.3 - 10.0), note.clone(), t.dim, 8.5, false);
        }
    }
    // RECOVERY
    let (rp, r) = g.rec;
    if let Some(o) = out {
        paint_recovery(s, t, r, &o.short, &o.long, t.accent.with_alpha(fade), false);
        let half = |v: &[f32]| ProbeOut::half_time(v).map_or_else(|| "–".to_string(), ms_text);
        let label = format!("50 %: {} / {}", half(&o.short), half(&o.long));
        kit::text_right(s, rp.0 + rp.2 - 8.0, rp.1 + 12.0, &label, t.text, 9.5);
    } else {
        s.text(kit::rect(r).center(), "measuring…", t.dim, 9.5, true);
    }
    // TRACE
    let (tp, tr) = g.trace;
    let n = live.gr_hist.len();
    let span = (live.hist_rate * 4.0).max(2.0);
    if n >= 2 {
        let pts: Vec<Point> = live
            .gr_hist
            .iter()
            .enumerate()
            .filter(|(i, _)| ((n - 1 - i) as f32) <= span)
            .map(|(i, g)| Point::new(tr.0 + tr.2 * (1.0 - (n - 1 - i) as f32 / span), tr.1 + (g / GR_RANGE).clamp(0.0, 1.0) * tr.3))
            .collect();
        kit::area(s, &pts, tr.1, t.accent.with_alpha(0.25));
        kit::curve(s, pts, t.accent, 1.5);
    }
    let window: Vec<f32> = live.gr_hist.iter().rev().take(span as usize).copied().collect();
    let avg = if window.is_empty() { 0.0 } else { window.iter().sum::<f32>() / window.len() as f32 };
    let max = window.iter().fold(0.0f32, |a, &b| a.max(b));
    kit::text_right(s, tp.0 + tp.2 - 8.0, tp.1 + 12.0, &format!("AVG {avg:.1} · MAX {max:.1} dB"), t.text, 9.5);
    // GR meter
    let hold = live.gr_hist.iter().rev().take((live.hist_rate * 1.5) as usize).fold(0.0f32, |a, &b| a.max(b));
    kit::gr_meter(s, t, g.meter, live.gr, hold, GR_RANGE);
    s.text(kit::rect(g.maxbox).center(), format!("{:.1}", common.max_gr), t.text, 9.5, true);
}

/// Two recovery curves into the plot `r`.
pub(super) fn paint_recovery(s: &mut Scene, t: &Theme, r: R, short: &[f32], long: &[f32], c: C, dashed: bool) {
    let _ = t;
    let pts = |v: &[f32]| -> Vec<Point> {
        let n = 160;
        (0..=n)
            .map(|k| {
                let ms = REC_T0 * (REC_T1 / REC_T0).powf(k as f32 / n as f32);
                let i = (ms.round() as usize).saturating_sub(1).min(v.len().saturating_sub(1));
                let g = v.get(i).copied().unwrap_or(0.0);
                Point::new(kit::freq_x(ms, r.0, r.0 + r.2, REC_T0, REC_T1), r.1 + (-g / REC_DEPTH).clamp(0.0, 1.06) * r.3)
            })
            .collect()
    };
    s.clip_round(kit::rect(r), 4.0);
    for (v, w) in [(short, 1.0f32), (long, 2.5)] {
        if v.is_empty() {
            continue;
        }
        if dashed {
            kit::dashed(s, &pts(v), 4.0, 3.0, c, w.min(1.5));
        } else {
            kit::curve(s, pts(v), c, w);
        }
    }
    s.pop_clip();
}

/// The OUT MAX badge: ceiling plus the largest trim after the limiter. `None`: no ceiling.
pub(super) fn paint_badge(s: &mut Scene, cx: &Ctx, out_max: Option<f32>) {
    let t = cx.t;
    let r = badge_rect(cx);
    let over = out_max.is_some_and(|v| v > 0.0);
    let rr = kit::rect(r);
    s.fill_round(rr, 5.0, t.inset, t.inset, true);
    s.stroke_round(rr, 5.0, if over { t.over } else { t.border }, if over { 1.5 } else { 1.0 });
    let label = match out_max {
        None => "NO CEILING".to_string(),
        Some(v) if over => format!("! OUT MAX {v:+.1}"),
        Some(v) => format!("OUT MAX {v:.1} dBFS"),
    };
    s.text(rr.center(), label, if over { t.over } else { t.text }, 10.0, true);
}

pub(super) const BADGE_TIP: &str = "The most this module can put out: its ceiling plus the trim after it (MORE). Red when a trim above 0 dB lets the output pass the ceiling.";

// =====================================================================================
// Page-drawn knobs
// =====================================================================================

/// A knob the page draws and drives itself.
#[derive(Clone, Debug)]
pub(super) struct Knob {
    pub rect: R,
    pub size: Size,
    pub label: String,
    /// Host parameters it moves together, keeping their offsets. The first sets the ring.
    pub params: Vec<usize>,
    /// The travel, plain values; may be shorter than the parameter's range.
    pub lo: f32,
    pub hi: f32,
    pub dim: bool,
    /// The value is outside the travel (a trim above 0 dB): ring and box in `over`.
    pub over: bool,
    pub text: String,
    /// A value cell covers the box and shows the text.
    pub boxed: bool,
}

impl Knob {
    pub fn new(cx: &Ctx, size: Size, label: &str, params: Vec<usize>, x: f32, y: f32) -> Self {
        let info = &INFO.params[params[0]];
        let (w, h) = size.cell();
        let v = cx.values[params[0]];
        Self { rect: (x, y, w, h), size, label: label.into(), params, lo: info.min, hi: info.max, dim: false, over: false, text: info.format(v), boxed: false }
    }

    pub fn travel(mut self, lo: f32, hi: f32) -> Self {
        self.lo = lo;
        self.hi = hi;
        self
    }

    pub fn text(mut self, s: String) -> Self {
        self.text = s;
        self
    }

    pub fn dim(mut self, d: bool) -> Self {
        self.dim = d;
        self
    }

    pub fn over(mut self, o: bool) -> Self {
        self.over = o;
        self
    }

    pub fn boxed(mut self) -> Self {
        self.boxed = true;
        self
    }

    fn info(&self) -> &'static openplugin::core::ParamInfo {
        &INFO.params[self.params[0]]
    }

    /// Position 0..1 of a plain value along the travel.
    pub fn norm(&self, v: f32) -> f32 {
        let i = self.info();
        let (a, b) = (i.to_normalized(self.lo), i.to_normalized(self.hi));
        if (b - a).abs() < 1e-9 {
            return 0.0;
        }
        ((i.to_normalized(v.clamp(i.min, i.max)) - a) / (b - a)).clamp(0.0, 1.0)
    }

    pub fn plain(&self, n: f32) -> f32 {
        let i = self.info();
        let (a, b) = (i.to_normalized(self.lo), i.to_normalized(self.hi));
        i.from_normalized(a + (b - a) * n.clamp(0.0, 1.0))
    }

    /// Same geometry as the cell knobs (`kit::knob_circle`), so the ring clears the label.
    pub fn circle(&self) -> (f32, f32, f32) {
        let (x, y, w, _) = self.rect;
        match self.size {
            Size::Big => (x + w * 0.5, y + 59.0, 33.0),
            Size::Small | Size::Mini => (x + w * 0.5, y + 37.0, 17.0),
        }
    }

    pub fn value_box(&self) -> R {
        let (x, y, w, _) = self.rect;
        match self.size {
            Size::Big => (x + (w - 84.0) * 0.5, y + 98.0, 84.0, 22.0),
            Size::Small | Size::Mini => (x + (w - 64.0) * 0.5, y + 62.0, 64.0, 18.0),
        }
    }

    /// The part a press takes: the label and the knob, not the value box.
    pub fn hit(&self) -> R {
        let (x, y, w, h) = self.rect;
        let b = self.value_box();
        if self.boxed {
            (x, y, w, b.1 - y)
        } else {
            (x, y, w, h)
        }
    }

    pub fn paint(&self, s: &mut Scene, t: &Theme, cx: &Ctx) {
        let (x, y, w, _) = self.rect;
        let (cxp, cyp, r) = self.circle();
        let fade = if self.dim { 0.4 } else { 1.0 };
        let (ring, label_size, label_y, value_size) = match self.size {
            Size::Big => (4.0, 10.0, y + 7.0, 12.0),
            _ => (3.0, 9.0, y + 6.0, 10.5),
        };
        let c = Point::new(cxp, cyp);
        s.stroke_arc(c, r + ring + 2.0, kit::knob_angle(0.0), kit::knob_angle(1.0), t.grid_strong.with_alpha(fade), ring);
        let face = Scene::ellipse_at(c, r);
        s.shadow_ellipse(face.translated(0.0, 2.0), 3.0, t.shadow);
        s.fill_ellipse(face, t.knob_top, t.knob_bottom);
        s.stroke_ellipse(face, t.border, 1.0);
        let label = text::ellipsize(&self.label.to_uppercase(), label_size, w - 4.0);
        s.text(Point::new(x + w * 0.5, label_y), label, t.dim.with_alpha(fade), label_size, true);
        let v = cx.values[self.params[0]];
        let n = self.norm(v);
        let info = self.info();
        let _ = info;
        let origin = if self.lo < 0.0 && self.hi > 0.0 { self.norm(0.0) } else if self.lo < 0.0 && self.hi <= 0.0 { 1.0 } else { 0.0 };
        let colour = if self.over { t.over } else if self.dim { t.dim.with_alpha(0.45) } else { t.accent };
        if self.over {
            s.stroke_arc(c, r + ring + 2.0, kit::knob_angle(0.0), kit::knob_angle(1.0), t.over, ring);
        } else if (n - origin).abs() > 0.004 {
            s.stroke_arc(c, r + ring + 2.0, kit::knob_angle(origin.min(n)), kit::knob_angle(origin.max(n)), colour, ring);
        }
        let a = kit::knob_angle(n);
        let (dx, dy) = (a.cos(), a.sin());
        let ink = if self.dim { t.dim } else { t.text };
        s.line(Point::new(cxp + dx * r * 0.42, cyp + dy * r * 0.42), Point::new(cxp + dx * r * 0.86, cyp + dy * r * 0.86), ink, if self.size == Size::Big { 3.0 } else { 2.4 });
        if !self.boxed {
            let b = self.value_box();
            kit::inset(s, t, b, 5.0);
            if self.over {
                s.stroke_round(kit::rect(b), 5.0, t.over, 1.5);
            }
            let shown = text::ellipsize(&self.text, value_size, b.2 - 4.0);
            s.text(Point::new(b.0 + b.2 * 0.5, b.1 + b.3 * 0.5), shown, if self.over { t.over } else { ink }, value_size, true);
        }
    }
}

/// A drag on a page-drawn knob.
#[derive(Clone, Debug)]
pub(super) struct KnobDrag {
    knob: Knob,
    starts: Vec<f32>,
    /// Pointer y of the last move, and the knob position so far.
    y: f32,
    n: f32,
}

fn knob_move(k: &Knob, starts: &[f32], n: f32, io: &mut Io, drag: bool) {
    let target = k.plain(n);
    let set = |io: &mut Io, p: usize, v: f32| if drag { io.perform(p, v) } else { io.set(p, v) };
    if k.params.len() == 1 {
        set(io, k.params[0], target);
        return;
    }
    // Several parameters keep their offsets: move all by the same amount, inside the travel.
    let mut delta = target - starts[0];
    let lo = starts.iter().map(|s| k.lo - s).fold(f32::MIN, f32::max);
    let hi = starts.iter().map(|s| k.hi - s).fold(f32::MAX, f32::min);
    if lo <= hi {
        delta = delta.clamp(lo, hi);
    }
    for (p, s) in k.params.iter().zip(starts) {
        set(io, *p, (s + delta).clamp(k.lo.min(*s), k.hi.max(*s)));
    }
}

/// Drag (200 pt for the travel), wheel (1 %), double-click (default) on page-drawn knobs.
pub(super) fn knob_pointer(drag: &mut Option<KnobDrag>, ev: Pointer, io: &mut Io, knobs: &[Knob]) -> Response {
    match ev {
        Pointer::Down { x, y, clicks } => {
            let Some(k) = knobs.iter().find(|k| !k.dim && kit::inside(k.hit(), x, y)) else { return Response::NONE };
            if clicks >= 2 {
                for &p in &k.params {
                    let d = INFO.params[p].default;
                    io.set(p, d.clamp(k.lo, k.hi));
                }
                *drag = None;
                return Response::TAKEN;
            }
            let starts: Vec<f32> = k.params.iter().map(|&p| io.value(p)).collect();
            for &p in &k.params {
                io.begin(p);
            }
            *drag = Some(KnobDrag { n: k.norm(starts[0]), knob: k.clone(), starts, y });
            Response::TAKEN
        }
        Pointer::Move { y, .. } => {
            let fine = io.modifiers().shift;
            let Some(d) = drag.as_mut() else { return Response::NONE };
            d.n = (d.n + (d.y - y) / 200.0 * if fine { 0.1 } else { 1.0 }).clamp(0.0, 1.0);
            d.y = y;
            knob_move(&d.knob, &d.starts, d.n, io, true);
            Response::TAKEN
        }
        Pointer::Up { .. } => {
            let Some(d) = drag.take() else { return Response::NONE };
            for &p in &d.knob.params {
                io.end(p);
            }
            Response::TAKEN
        }
        Pointer::Wheel { x, y, dy } => {
            let Some(k) = knobs.iter().find(|k| !k.dim && kit::inside(k.rect, x, y)) else { return Response::NONE };
            let starts: Vec<f32> = k.params.iter().map(|&p| io.value(p)).collect();
            let step = if dy > 0.0 { 0.01 } else { -0.01 } * if io.modifiers().shift { 0.1 } else { 1.0 };
            knob_move(k, &starts, k.norm(starts[0]) + step, io, false);
            Response::TAKEN
        }
    }
}

// =====================================================================================
// The limiter (original engine)
// =====================================================================================

fn classic() -> Box<dyn Module> {
    Box::new(crate::Classic::new(None))
}

fn threshold_text(v: f32) -> String {
    if v > 0.05 {
        "OFF".into()
    } else {
        format!("{v:.1} dB")
    }
}

fn soft_clip_text(v: f32) -> String {
    if v >= 24.0 {
        "OFF".into()
    } else {
        format!("{v:+.0} dB")
    }
}

fn release_step_text(v: f32) -> String {
    let s = v.round() as i32;
    match s {
        0 => "Normal".into(),
        s if s < 0 => format!("Slow {}", -s),
        s => format!("Fast +{s}"),
    }
}

struct Limiter {
    common: Common,
}

/// Where the ALIGN tag sits.
fn align_rect(cx: &Ctx) -> R {
    (420.0, cx.zones.header.1 + 9.0, 56.0, 22.0)
}

impl Limiter {
    fn knobs(&self, cx: &Ctx) -> Vec<Knob> {
        if cx.more {
            return Vec::new();
        }
        let (gl, gr) = (host(cx, "gainL"), host(cx, "gainR"));
        let (vl, vr) = (cx.values[gl], cx.values[gr]);
        let drive_text = if (vl - vr).abs() < 0.05 { format!("{vl:+.1} dB") } else { format!("L {vl:+.1} R {vr:+.1}") };
        vec![Knob::new(cx, Size::Big, "DRIVE", vec![gl, gr], DRIVE_X, BIG_Y).text(drive_text)]
    }

    fn setup_in_use(cx: &Ctx) -> bool {
        ["delayL", "delayR", "delayFineL", "delayFineR", "phaseL", "phaseR", "headroom"].iter().any(|id| {
            let p = host(cx, id);
            (cx.values[p] - INFO.params[p].default).abs() > 1e-4
        })
    }

    fn ceiling(cx: &Ctx) -> Option<f32> {
        let th = val(cx, "threshold");
        (th <= 0.05).then_some(th.min(0.0))
    }
}

impl Page for Limiter {
    fn has_more(&self) -> bool {
        true
    }

    fn wants_audio(&self) -> bool {
        true
    }

    fn rebuild_on(&self, local_index: usize, cx: &Ctx) -> bool {
        Some(local_index) == cx.find("autoRelease")
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let mut v = Vec::new();
        if cx.more {
            v.push(Cell::knob(Size::Small, host(cx, "gainL"), "GAIN L", 120.0, MORE_KNOB_Y).format(|v| format!("{v:+.1} dB")));
            v.push(Cell::knob(Size::Small, host(cx, "gainR"), "GAIN R", 216.0, MORE_KNOB_Y).format(|v| format!("{v:+.1} dB")));
            v.push(Cell::knob(Size::Small, host(cx, "output"), "TRIM", 408.0, MORE_KNOB_Y).tip("Trim after the limiter, down only"));
            v.push(Cell::knob(Size::Small, host(cx, "releaseA"), "AUTO STEP", 600.0, MORE_KNOB_Y).format(release_step_text).tip("What RELEASE sets while AUTO is on"));
            v.push(Cell::knob(Size::Small, host(cx, "releaseM"), "MANUAL", 696.0, MORE_KNOB_Y).tip("What RELEASE sets while AUTO is off"));
            return v;
        }
        // RELEASE edits the auto step while AUTO is on and the time while it is off; the
        // shell rebuilds the cells when AUTO changes (`rebuild_on`).
        if cx.on(local(cx, "autoRelease")) {
            v.push(Cell::knob(Size::Big, host(cx, "releaseA"), "RELEASE", RELEASE_X, BIG_Y).format(release_step_text).tip("Auto release step: Slow 3 to Normal to Fast +3"));
        } else {
            v.push(Cell::knob(Size::Big, host(cx, "releaseM"), "RELEASE", RELEASE_X, BIG_Y).tip("Manual release time"));
        }
        v.push(Cell::knob(Size::Big, host(cx, "threshold"), "CEILING", CEILING_X, BIG_Y).format(threshold_text).tip("The output limit (this engine has no make-up); the top position is OFF"));
        v.push(Cell::knob(Size::Small, host(cx, "softClip"), "SOFT CLIP", COL[0], ROW1).format(soft_clip_text).tip("Soft clipper ahead of the limiter; the top position is OFF"));
        v.push(Cell::pill(host(cx, "upsample"), "TP DETECT", (COL[0], ROW2, 80.0, 22.0)).tip("Limits on inter-sample peaks"));
        v.push(Cell::pill(host(cx, "link"), "LINK", (COL[1], ROW2, 80.0, 22.0)).tip("Link the two channels"));
        v.push(Cell::pill(host(cx, "noLatency"), "NO LATENCY", (COL[2], ROW2, 80.0, 22.0)).tip("No look-ahead: the header latency updates"));
        v.push(Cell::pill(host(cx, "autoRelease"), "AUTO", (COL[5], ROW2, 80.0, 22.0)).tip("Programme-dependent release: switches what RELEASE edits"));
        // The ceiling grip on CURVE, while the limiter is switched on (switched off, the
        // shell takes every threshold grip away).
        let on = cx.slot.and_then(|s| crate::LAYOUT.slots.get(s)).is_some_and(|s| cx.values.get(s.on).is_some_and(|&v| v >= 0.5));
        if let Some((_, p)) = geo(cx.zones.tall, true).curve.filter(|_| on) {
            let th = host(cx, "threshold");
            let info = &INFO.params[th];
            let y = |d: f32| curve_xy(p, 0.0, d).y;
            let (top, bottom) = (y(info.max), y(info.min));
            v.push(Cell::grip(th, (p.0 + p.2 - 40.0, top, 40.0, bottom - top)).tip("Ceiling: drag the line"));
        }
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        paint_pictures_static(s, t, cx.zones.tall, true);
        if cx.more {
            s.text(Point::new(120.0, 630.0), "MORE  ·  CHANNEL GAIN", t.dim, 9.0, false);
            s.text(Point::new(408.0, 630.0), "AFTER THE LIMITER · TRIM (down only)", t.dim, 9.0, false);
            s.text(Point::new(600.0, 630.0), "RELEASE SETTINGS", t.dim, 9.0, false);
        } else {
            divider(s, t, 508.0);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let gain = val(cx, "gainL").max(val(cx, "gainR"));
        let clip = val(cx, "softClip");
        let lit = clip < 24.0 && live.audio.is_some() && self.common.peaks.0 + gain > clip;
        let extras = CurveExtras { ceiling: Self::ceiling(cx), lamp: (clip < 24.0).then_some(("SOFT CLIP", lit)), note: None };
        paint_pictures(s, cx, live, &self.common, &extras, true);
        paint_badge(s, cx, Self::ceiling(cx).map(|c| c + val(cx, "output")));
        if Self::setup_in_use(cx) {
            let r = align_rect(cx);
            s.fill_round(kit::rect(r), 5.0, cx.t.inset, cx.t.inset, true);
            s.stroke_round(kit::rect(r), 5.0, cx.t.accent, 1.0);
            s.text(kit::rect(r).center(), "ALIGN", cx.t.accent, 9.5, true);
        }
        for k in self.knobs(cx) {
            k.paint(s, cx.t, cx);
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let mut v = picture_rects(cx, true);
        v.push(align_rect(cx));
        v.extend(self.knobs(cx).iter().map(|k| (k.rect.0 - 3.0, k.rect.1 - 3.0, k.rect.2 + 6.0, k.rect.3 + 6.0)));
        v
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        self.common.tick(cx, live)
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let knobs = self.knobs(cx);
        self.common.pointer(ev, cx, io, &knobs)
    }

    fn warning(&self, cx: &Ctx) -> bool {
        Self::ceiling(cx).is_some_and(|c| c + val(cx, "output") > c + 1e-4)
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let g = geo(cx.zones.tall, true);
        let mut v = vec![tip(badge_rect(cx), BADGE_TIP), tip(g.maxbox, "Deepest reduction; click to reset"), tip(g.rec.0, "Gain recovery after a 6 dB overshoot, measured on a copy of this limiter")];
        if Self::setup_in_use(cx) {
            v.push(tip(align_rect(cx), "Alignment (delay, phase or headroom) is in use: see SETUP"));
        }
        if !cx.more {
            v.push(tip(self.knobs(cx)[0].rect, "Drive: moves both channel gains and keeps their offset (single channels under MORE)"));
        }
        v
    }
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    // The probe holds the alignment at its defaults and the old dither off: they are not
    // the limiter.
    let overrides = crate::ui::SETUP_PARAMS.iter().chain(std::iter::once(&crate::ui::P_DITHER)).map(|&p| (p, if p == crate::ui::P_DITHER { 25.0 } else { crate::PARAMS[p].default })).collect();
    Box::new(Limiter { common: Common::new(Probe::new(classic).overrides(overrides)) })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). The trim after this
/// limiter ends at 0 dB, so it never can.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let output = values.get(crate::ui::P_OUTPUT).copied().unwrap_or(0.0);
    (false, values.get(crate::ui::P_THRESHOLD).is_some_and(|&t| t <= 0.05) && output > 1e-4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(factory: Factory, values: Vec<f32>) -> Job {
        Job { factory, values, sr: 48_000.0, overrides: Vec::new(), alt: None, scan: None }
    }

    fn defaults(m: &dyn Module) -> Vec<f32> {
        m.params().iter().map(|p| p.default).collect()
    }

    #[test]
    fn the_probe_draws_a_limit_and_a_recovery() {
        let make: Factory = || Box::new(limiter_clear::ClearLimiter::default());
        let mut v = defaults(make().as_ref());
        v[limiter_clear::P_THRESHOLD] = -6.0;
        let t0 = std::time::Instant::now();
        let out = measure(job(make, v));
        eprintln!("clear probe: {:?}", t0.elapsed());
        assert!((out.curve[0] + 36.0).abs() < 0.5, "unity well below the limit: {}", out.curve[0]);
        assert!((out.curve[36] + 6.0).abs() < 0.7, "the ceiling holds at 0 dBFS in: {}", out.curve[36]);
        let deepest = out.long.iter().take(3).fold(0.0f32, |a, &b| a.min(b));
        assert!(deepest < -3.0, "a 6 dB overshoot is pulled down: {deepest}");
        assert!(out.long.last().unwrap().abs() < 0.5, "and comes back");
        assert!(ProbeOut::half_time(&out.short).is_some());
    }

    #[test]
    fn the_probe_scans_latency_per_style() {
        let make: Factory = || Box::new(limiter_modes::LimiterModes::default());
        let v = defaults(make().as_ref());
        let t0 = std::time::Instant::now();
        let out = measure(Job { scan: Some(limiter_modes::P_STYLE), ..job(make, v) });
        eprintln!("modes probe: {:?}", t0.elapsed());
        assert_eq!(out.scan.len(), 13);
        assert!(out.scan.iter().any(|&l| l != out.scan[0]), "{:?}", out.scan);
    }

    #[test]
    fn the_classic_probe_runs() {
        let mut v: Vec<f32> = crate::PARAMS.iter().map(|p| p.default).collect();
        v[crate::ui::P_THRESHOLD] = -6.0;
        let t0 = std::time::Instant::now();
        let out = measure(job(classic, v));
        eprintln!("classic probe: {:?}", t0.elapsed());
        assert!((out.curve[36] + 6.0).abs() < 1.0, "{:?}", out.curve);
    }

    #[test]
    fn the_wall_probe_runs() {
        let make: Factory = || Box::new(limiter_wall::WallLimiter::default());
        let mut v = defaults(make().as_ref());
        v[limiter_wall::P_STRICT_TRUE_PEAK] = 1.0;
        let t0 = std::time::Instant::now();
        let out = measure(job(make, v));
        eprintln!("wall probe: {:?}", t0.elapsed());
        assert!(out.curve[36] < 0.1);
    }

    /// Writes `build/ui-<key>-<theme>-<mode>.ppm` for the four limiter pages and the dither,
    /// STANDARD and TALL, main face and MORE. Run with `--ignored`.
    #[test]
    #[ignore]
    fn dump_limiter_pages() {
        use crate::ui::tests::{editor, feed};
        use openplugin::core::Editor;
        use crate::ui::theme::ThemeId;
        use crate::ui::{dither_page, LAYOUT};
        for (id, name) in ThemeId::ALL.map(|t| (t, t.name())) {
            for key in ["", "clear", "modes", "wall", "dither"] {
                let selected = LAYOUT.slots.iter().position(|s| s.key == key).unwrap_or(dither_page());
                for (tall, more) in [(false, false), (true, false), (false, true)] {
                    let (mut e, shared) = editor(id);
                    feed(&shared, 4000);
                    if selected < dither_page() {
                        e.param_changed(LAYOUT.slots[selected].on, 1.0);
                    }
                    e.param_changed(LAYOUT.dither_base, 5.0);
                    e.param_changed(LAYOUT.dither_base + 2, 6.0);
                    e.open_page(selected);
                    e.set_tall(tall);
                    if more {
                        if !e.ui.page.has_more() {
                            continue;
                        }
                        e.ui.more = true;
                        e.rebuild();
                    }
                    for _ in 0..6 {
                        feed(&shared, 7);
                        shared.raise_slot_gr(selected.min(dither_page() - 1), 3.0);
                        e.tick(1.0 / 30.0);
                    }
                    let f = e.render(2.0);
                    let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                    for px in f.pixels {
                        out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
                    }
                    let k = if key.is_empty() { "limiter" } else { key };
                    let mode = if more { "more" } else if tall { "tall" } else { "std" };
                    std::fs::write(format!("{}/build/ui-{k}-{name}-{mode}.ppm", env!("CARGO_MANIFEST_DIR")), out).expect("write");
                }
            }
        }
    }
}
