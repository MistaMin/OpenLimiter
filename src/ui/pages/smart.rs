//! The Smart Limiter page (`design/smart-limiter-page.md`): one limiter slot with six
//! engines that reach the same ceiling in different ways. A sibling of the four limiter
//! pages: the shared frame of `limiter.rs` (CURVE / RECOVERY / TRACE, GR meter, OUT MAX
//! badge, the editor-side probe, brackets, page pills) with the engine's own pictures.
//!
//! What the framework does not have, and how this page works around it:
//! - The engine keys and the DELTA pill are painted by the page over a dimmed, invisible
//!   grip cell (the parameter stays reachable; a dimmed cell takes no click, so the click
//!   reaches the page). That gives "click the lit key to go back" and press-and-hold on
//!   DELTA. The wheel on them still steps the parameter through the grip.
//! - The shell sends no hover to a page, so the hovered engine's ghost in RECOVERY is the
//!   previous engine's (the A/B partner) instead; the key's tooltip carries its description.
//! - A page cannot open MORE: the SPLITS read-out says where the splits are edited.
//! - No right-click: a click on the GR bars switches their range between 12 and 24 dB.

use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Instant;

use olm_api::Module;
use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use goodlookindri_shell::text;

use limiter_smart::{idx, view, SmartLimiter};

use super::limiter::{self as frame, Probe, ProbeOut, BIG_Y, CEILING_X, COL, CURVE_LO, DRIVE_X, MORE_KNOB_Y, RELEASE_X, REC_DEPTH, REC_T0, REC_T1, ROW1, ROW2};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Merge, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::ui::theme::Theme;
use crate::INFO;

fn make() -> Box<dyn Module> {
    Box::new(SmartLimiter::default())
}

// =====================================================================================
// Names and geometry
// =====================================================================================

const KEYS: [&str; 6] = ["PURE", "SMOOTH", "PRECISE", "PUNCH", "MULTI", "GLIDE"];
const SHORT: [&str; 6] = ["PU", "SM", "PR", "PN", "MB", "GL"];
const DETAIL: [&str; 6] = ["SLOW BODY", "CREST", "CREST", "CLIPPER", "BANDS", "BRIDGE"];
const DESC: [&str; 6] = [
    "Pure: a slow body stage under a fast peak lane, oversampled; the cleanest bass",
    "Smooth: one smooth stage at 1x with a crest-adaptive release",
    "Precise: Smooth inside the oversampler",
    "Punch: leveller, limiter and a soft clipper",
    "Multiband: four bands share the reduction; a broadband catcher keeps the ceiling",
    "Glide: bridges short gaps between peaks, so there is no pumping between hits",
];
const PURE: usize = 0;
const SMOOTH: usize = 1;
const PRECISE: usize = 2;
const PUNCH: usize = 3;
const MULTI: usize = 4;
const GLIDE: usize = 5;

/// TRACE depth, dB.
const TRACE_RANGE: f32 = 12.0;
/// Seconds of TRACE and of Pure's slow-body strip.
const TRACE_S: f32 = 4.0;
const STRIP_S: f32 = 12.0;
/// Depth of Pure's slow-body strip, dB.
const STRIP_DB: f32 = 6.0;

const PK_RECT: R = (616.0, ROW2, 80.0, 22.0);
const DELTA_RECT: R = (808.0, ROW2, 80.0, 22.0);
const NOW_RECT: R = (376.0, 757.0, 120.0, 14.0);
const MATCH_RECT: R = (712.0, 761.0, 80.0, 11.0);
const CONTEXT_RECT: R = (900.0, 628.0, 184.0, 144.0);
const RESET_SPLITS: R = (528.0, 742.0, 96.0, 22.0);
const MORE_NOTES: R = (116.0, 692.0, 392.0, 20.0);
const MORE_ENGINE_NOTE: R = (524.0, 694.0, 560.0, 18.0);

fn engine_keys(cx: &Ctx) -> R {
    (420.0, cx.zones.slot.1, 336.0, 24.0)
}

fn key_rect(cx: &Ctx, i: usize) -> R {
    let r = engine_keys(cx);
    (r.0 + 56.0 * i as f32, r.1, 56.0, r.3)
}

fn more_dot(cx: &Ctx) -> R {
    (1066.0, cx.zones.header.1 + 10.0, 12.0, 10.0)
}

/// The shell's latency read-out, for a tooltip.
fn latency_rect(cx: &Ctx) -> R {
    (924.0, cx.zones.header.1 + 12.0, 58.0, 16.0)
}

/// Picture panels and plots for a mode.
#[derive(Clone, Copy)]
struct Geo {
    curve: (R, R),
    /// RECOVERY (STANDARD: the middle panel when its tab is RECOVERY).
    rec: Option<(R, R)>,
    /// DETAIL (STANDARD: the middle panel when its tab is DETAIL; TALL: right of RECOVERY).
    detail: Option<(R, R)>,
    trace: (R, R),
    /// The two bars: x, top, height.
    bars: (f32, f32, f32),
    maxbox: R,
}

const MID: (R, R) = ((328.0, 416.0, 300.0, 200.0), (360.0, 440.0, 256.0, 148.0));
const RIGHT: (R, R) = ((636.0, 416.0, 392.0, 200.0), (668.0, 440.0, 352.0, 148.0));

fn geo(tall: bool, detail_tab: bool) -> Geo {
    let f = frame::geo(tall, true);
    let curve = f.curve.unwrap_or(((120.0, 416.0, 200.0, 200.0), (148.0, 424.0, 164.0, 164.0)));
    let (rec, detail) = if tall {
        (Some(MID), Some(RIGHT))
    } else if detail_tab {
        (None, Some(MID))
    } else {
        (Some(MID), None)
    };
    let bars = if tall { (1044.0, 244.0, 332.0) } else { (1044.0, 416.0, 160.0) };
    Geo { curve, rec, detail, trace: f.trace, bars, maxbox: f.maxbox }
}

fn meter_area(g: &Geo) -> R {
    (1034.0, g.bars.1 - 2.0, 48.0, 618.0 - g.bars.1)
}

fn mid_tabs() -> [R; 2] {
    [(328.0, 418.0, 72.0, 16.0), (404.0, 418.0, 72.0, 16.0)]
}

fn trace_tabs(g: &Geo) -> [R; 2] {
    let p = g.trace.0;
    [(p.0 + 56.0, p.1 + 2.0, 56.0, 16.0), (p.0 + 116.0, p.1 + 2.0, 44.0, 16.0)]
}

// =====================================================================================
// Small helpers
// =====================================================================================

fn v(view: &[f32], slot: usize) -> f32 {
    view.get(slot).copied().filter(|x| x.is_finite()).unwrap_or(0.0)
}

fn lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
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

fn hz_text(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1} kHz", f / 1000.0)
    } else {
        format!("{f:.0} Hz")
    }
}

/// A filled band between two polylines over the same x positions.
fn band(s: &mut Scene, top: &[Point], bottom: &[Point], c: C) {
    if top.len() < 2 || top.len() != bottom.len() {
        return;
    }
    kit::between(s, top, bottom, c);
}

/// A bracket that opens downward onto the controls under it, caption in the middle.
fn bracket_down(s: &mut Scene, t: &Theme, x0: f32, x1: f32, y: f32, label: &str) {
    let w = text::width(label, 9.0) + 12.0;
    let mid = (x0 + x1) * 0.5;
    let yl = y + 6.0;
    s.line(Point::new(x0, yl), Point::new(x0, y + 12.0), t.grid_strong, 1.0);
    s.line(Point::new(x1, yl), Point::new(x1, y + 12.0), t.grid_strong, 1.0);
    s.line(Point::new(x0, yl), Point::new(mid - w * 0.5, yl), t.grid_strong, 1.0);
    s.line(Point::new(mid + w * 0.5, yl), Point::new(x1, yl), t.grid_strong, 1.0);
    s.text(Point::new(mid, yl), label, t.dim, 9.0, true);
}

/// A text tab: the chosen one in `text` with a 2 pt accent underline.
fn tab(s: &mut Scene, t: &Theme, r: R, label: &str, on: bool) {
    let w = text::width(label, 9.0);
    s.text(Point::new(r.0 + 4.0, r.1 + 8.0), label, if on { t.text } else { t.dim }, 9.0, false);
    if on {
        s.fill_round(Rect::new(r.0 + 4.0, r.1 + 14.0, w, 2.0), 1.0, t.accent, t.accent, true);
    }
}

fn is_default(cx: &Ctx, local: usize) -> bool {
    let p = cx.p(local);
    (cx.values[p] - INFO.params[p].default).abs() < 1e-4
}

/// Snapshot of the read-out, one per editor tick.
#[derive(Clone, Copy, Default)]
struct Snap {
    total: f32,
    slow: f32,
    fast: f32,
    clip: f32,
    trim: f32,
    band: [f32; 4],
    catcher: f32,
    ch: [f32; 2],
    engine: u8,
}

impl Snap {
    fn from(view: &[f32]) -> Self {
        let g = |s| v(view, s).min(0.0);
        Self {
            total: g(view::TOTAL_GAIN),
            slow: g(view::SLOW_GAIN),
            fast: g(view::FAST_GAIN),
            clip: g(view::CLIP_GAIN),
            trim: g(view::TRIM_GAIN),
            band: [g(view::BAND_GAIN_0), g(view::BAND_GAIN_1), g(view::BAND_GAIN_2), g(view::BAND_GAIN_3)],
            catcher: g(view::CATCHER_GAIN),
            ch: [g(view::CH0_GAIN), g(view::CH1_GAIN)],
            engine: v(view, view::ENGINE).round().clamp(0.0, 5.0) as u8,
        }
    }
}

// =====================================================================================
// Glide's BRIDGE picture: a probe with three bursts
// =====================================================================================

#[derive(Clone, Debug, Default)]
struct BridgeOut {
    /// Gain needed and the engine's gain, dB, per bin over -1..+8 lookaheads.
    need: Vec<f32>,
    gain: Vec<f32>,
    /// Burst starts and their length, in lookaheads.
    bursts: [f32; 3],
    burst_len: f32,
    lookahead_ms: f32,
    key: Vec<f32>,
}

const BRIDGE_BINS: usize = 360;

fn bridge_measure(values: Vec<f32>, sr: f32) -> BridgeOut {
    let mut m = SmartLimiter::default();
    let set = |m: &mut SmartLimiter| {
        for (i, x) in values.iter().enumerate() {
            m.set_param(i, *x);
        }
        m.set_param(idx::ENGINE, GLIDE as f32);
        m.set_param(idx::DRIVE, 0.0);
        m.set_param(idx::GAIN_MATCH, 0.0);
        m.set_param(idx::DELTA, 0.0);
    };
    set(&mut m);
    m.prepare(sr, 512);
    set(&mut m);
    m.reset();
    let lat = m.latency();
    let ceiling = values.get(idx::CEILING).copied().unwrap_or(-0.2);
    let la_ms = values.get(idx::LOOKAHEAD).copied().unwrap_or(1.5).max(0.1);
    let la = la_ms * 0.001 * sr;
    let len = 0.001 * sr;
    let pre = (0.3 * sr) as usize;
    let b1 = pre as f32;
    let b2 = b1 + len + 0.6 * la;
    let b3 = b2 + len + 3.0 * la;
    let (lo, hi) = (lin(ceiling - 12.0), lin(ceiling + 6.0));
    let span = (9.0 * la).max(len * 4.0);
    let n = (b1 + 8.0 * la).max(b3 + len * 2.0) as usize + lat + (0.05 * sr) as usize;
    let amp = |i: usize| {
        let x = i as f32;
        if [b1, b2, b3].iter().any(|&b| x >= b && x < b + len) {
            hi
        } else {
            lo
        }
    };
    let w = std::f32::consts::TAU * 2000.0 / sr;
    let mut l: Vec<f32> = (0..n).map(|i| amp(i) * (w * i as f32).sin()).collect();
    let mut r = l.clone();
    let mut g = vec![0.0f32; n];
    let mut pos = 0;
    while pos < n {
        let e = (pos + 512).min(n);
        m.process_traced(&mut l[pos..e], &mut r[pos..e], Some(&mut g[pos..e]));
        pos = e;
    }
    let t0 = b1 - la;
    let mut need = vec![0.0f32; BRIDGE_BINS];
    let mut gain = vec![0.0f32; BRIDGE_BINS];
    for k in 0..BRIDGE_BINS {
        let a = (t0 + span * k as f32 / BRIDGE_BINS as f32).max(0.0) as usize;
        let b = ((t0 + span * (k + 1) as f32 / BRIDGE_BINS as f32) as usize).max(a + 1);
        let mut nd = 0.0f32;
        let mut gd = 0.0f32;
        for i in a..b.min(n) {
            nd = nd.min((ceiling - 20.0 * amp(i).log10()).min(0.0));
            gd = gd.min(g.get(i + lat).copied().unwrap_or(0.0));
        }
        need[k] = nd;
        gain[k] = gd;
    }
    let at = |b: f32| (b - b1) / la;
    BridgeOut { need, gain, bursts: [at(b1), at(b2), at(b3)], burst_len: len / la, lookahead_ms: la_ms, key: Vec::new() }
}

#[derive(Default)]
struct Bridge {
    out: Option<BridgeOut>,
    running: Option<(Vec<f32>, mpsc::Receiver<BridgeOut>)>,
}

impl Bridge {
    fn tick(&mut self, values: Vec<f32>, sr: f32) -> bool {
        let mut key = values.clone();
        key.push(sr);
        let mut arrived = false;
        if let Some((_, rx)) = &self.running {
            match rx.try_recv() {
                Ok(out) => {
                    self.out = Some(out);
                    self.running = None;
                    arrived = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.running = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let stale = self.out.as_ref().is_none_or(|o| o.key != key);
        if stale && self.running.is_none() {
            if cfg!(test) {
                let mut o = bridge_measure(values, sr);
                o.key = key;
                self.out = Some(o);
                return true;
            }
            let (tx, rx) = mpsc::channel();
            let k2 = key.clone();
            let spawned = std::thread::Builder::new().name("olm-bridge".into()).spawn(move || {
                let mut o = bridge_measure(values, sr);
                o.key = k2;
                let _ = tx.send(o);
            });
            if spawned.is_ok() {
                self.running = Some((key, rx));
            }
        }
        arrived
    }
}

// =====================================================================================
// The page
// =====================================================================================

struct Smart {
    /// One probe per engine: the selected one first, the others filled in lazily.
    probes: Vec<Probe>,
    bridge: Bridge,
    /// The view shown by `set_state` (the reachability walk); `None`: the parameter's.
    forced: Option<usize>,
    /// The engine last seen and the one before it (the lit key's A/B partner).
    seen: Option<usize>,
    prev: usize,
    /// Seconds since the last engine switch.
    since_switch: f32,
    /// Middle panel shows DETAIL, per engine (STANDARD).
    detail_tab: [bool; 6],
    /// TRACE shows the two channels instead of the stages.
    trace_ch: bool,
    seen_ms: Option<bool>,
    ring: VecDeque<Snap>,
    /// Pure's slow gain and pause-hold flag, 12 s.
    strip: VecDeque<(f32, bool)>,
    /// Multiband band holds: (depth, age s).
    band_hold: [(f32, f32); 4],
    max_gr: f32,
    /// Held output peak (true peak with TRUE PEAK on), dB.
    pk_hold: f32,
    /// Seconds the OVER state still shows, and by how much.
    over: (f32, f32),
    gr_range: f32,
    /// DELTA pressed while it was off: when, for the hold gesture.
    delta_press: Option<Instant>,
    /// Smoothed read-outs for the text (10 Hz feel).
    crest: f32,
    rel_ms: f32,
    /// Gain-match loudness in and out, LUFS, for the tooltip.
    loud: (f32, f32),
    /// Editor ticks per second.
    rate: f32,
}

impl Smart {
    fn new() -> Self {
        let probes = (0..6).map(|_| Probe::new(make).overrides(vec![(idx::GAIN_MATCH, 0.0), (idx::DELTA, 0.0)])).collect();
        Self {
            probes,
            bridge: Bridge::default(),
            forced: None,
            seen: None,
            prev: SMOOTH,
            since_switch: 99.0,
            detail_tab: [false, false, false, false, true, true],
            trace_ch: false,
            seen_ms: None,
            ring: VecDeque::new(),
            strip: VecDeque::new(),
            band_hold: [(0.0, 0.0); 4],
            max_gr: 0.0,
            pk_hold: -120.0,
            over: (0.0, 0.0),
            gr_range: 12.0,
            delta_press: None,
            crest: 0.0,
            rel_ms: 0.0,
            loud: (-70.0, -70.0),
            rate: 60.0,
        }
    }

    fn engine(&self, cx: &Ctx) -> usize {
        self.forced.unwrap_or_else(|| cx.choice(idx::ENGINE)).min(5)
    }

    fn geo(&self, cx: &Ctx) -> Geo {
        geo(cx.zones.tall, self.detail_tab[self.engine(cx)])
    }

    fn ms(cx: &Ctx) -> bool {
        cx.choice(idx::CHANNEL_MODE) == 1
    }

    fn sr(cx: &Ctx) -> f32 {
        if cx.sample_rate > 1000.0 {
            cx.sample_rate
        } else {
            48_000.0
        }
    }

    fn crest_mode(cx: &Ctx) -> bool {
        cx.choice(idx::SLOW_MODE) == 1
    }

    /// Something that lives only on MORE is off its default: the MORE dot lights.
    fn more_in_use(cx: &Ctx) -> bool {
        [idx::OVERSAMPLING, idx::OVERSAMPLING_OFFLINE, idx::SLOW_MODE, idx::SLOW_WINDOW, idx::BASS_HOLD_MAX, idx::K_WEIGHTING, idx::CLIP_KNEE, idx::SPLIT1, idx::SPLIT2, idx::SPLIT3]
            .iter()
            .any(|&l| !is_default(cx, l))
    }

    // ---- DETAIL geometry, shared by cells and painting ----

    /// Pure: the post-hold bar and the join plot inside the detail plot.
    fn pure_parts(p: R) -> (R, R, R) {
        let strip = (p.0, p.1, p.2, (p.3 * 0.5 - 8.0).floor());
        let lower = p.1 + strip.3 + 22.0;
        let bar = (p.0, lower + 14.0, (p.2 * 0.44).floor(), 10.0);
        let join = (p.0 + (p.2 * 0.54).floor(), lower, (p.2 * 0.46).floor(), p.1 + p.3 - lower);
        (strip, bar, join)
    }

    /// Punch's CLIPPER axes, dB relative to the ceiling.
    const CLIP_LO: f32 = -8.0;
    const CLIP_HI: f32 = 5.0;

    fn clip_x(p: R, d: f32) -> f32 {
        p.0 + p.2 * (d - Self::CLIP_LO) / (Self::CLIP_HI - Self::CLIP_LO)
    }

    fn clip_y(p: R, d: f32) -> f32 {
        p.1 + p.3 * (1.0 - (d.clamp(Self::CLIP_LO, Self::CLIP_HI) - Self::CLIP_LO) / (Self::CLIP_HI - Self::CLIP_LO))
    }

    fn knee_edge_db(k: f32) -> f32 {
        20.0 * (1.0 - k).max(10f32.powf(Self::CLIP_LO / 20.0)).log10()
    }

    fn band_x(p: R, f: f32) -> f32 {
        kit::freq_x(f, p.0, p.0 + p.2, 20.0, 20_000.0)
    }

    /// Grips in the DETAIL picture that is on screen.
    fn detail_cells(&self, cx: &Ctx, v: &mut Vec<Cell>) {
        let e = self.engine(cx);
        let Some((_, p)) = self.geo(cx).detail else { return };
        match e {
            PURE => {
                let (_, bar, join) = Self::pure_parts(p);
                v.push(Cell::grip_h(cx.p(idx::BASS_HOLD_MAX), (bar.0, bar.1 - 6.0, bar.2, bar.3 + 12.0)).tip("Bass hold maximum: drag sideways, 0 to 20 ms"));
                v.push(Cell::grip_h(cx.p(idx::JOIN_WIDTH), join).tip("Join width: drag sideways, 0 to 12 dB"));
            }
            PUNCH => {
                let x0 = Self::clip_x(p, 0.0);
                let x3 = Self::clip_x(p, 3.0);
                v.push(Cell::grip_h(cx.p(idx::CLIP_HEADROOM), (x0, p.1, x3 - x0, p.3)).tip("Where the limiter stops and the clipper takes over: drag sideways, 0 to 3 dB"));
                let k = Self::knee_edge_db(cx.v(idx::CLIP_KNEE));
                let xk = Self::clip_x(p, k);
                let left = (xk - 10.0).max(p.0);
                let right = (xk + 10.0).min(x0 - 1.0);
                v.push(Cell::grip(cx.p(idx::CLIP_KNEE), (left, p.1, (right - left).max(6.0), p.3)).tip("Clipper knee: drag up for a softer knee"));
            }
            MULTI if cx.more => {
                for (i, (l, lo, hi)) in [(idx::SPLIT1, 40.0, 400.0), (idx::SPLIT2, 250.0, 2000.0), (idx::SPLIT3, 1500.0, 10_000.0)].into_iter().enumerate() {
                    let (a, b) = (Self::band_x(p, lo), Self::band_x(p, hi));
                    // Each grip spans its split's range, so the line follows the pointer.
                    let _ = i;
                    v.push(Cell::grip_h(cx.p(l), (a, p.1 + 12.0 * i as f32, b - a, p.3 - 12.0 * i as f32)).tip("Split: drag sideways"));
                }
            }
            _ => {}
        }
    }

    fn tick_probes(&mut self, cx: &Ctx, dt: f32) -> bool {
        let e = self.engine(cx);
        let mut order = vec![e];
        if self.prev != e {
            order.push(self.prev);
        }
        order.extend((0..6).filter(|k| *k != e && *k != self.prev));
        let mut vals = cx.values.to_vec();
        let mut arrived = false;
        for k in order {
            vals[cx.p(idx::ENGINE)] = k as f32;
            let c2 = Ctx { t: cx.t, values: &vals, slot: cx.slot, base: cx.base, count: cx.count, zones: cx.zones, more: cx.more, sample_rate: cx.sample_rate, key_present: cx.key_present };
            arrived |= self.probes[k].tick(&c2, dt);
            if self.probes[k].stale() {
                // One measurement at a time: the next engine waits for this one.
                break;
            }
        }
        arrived
    }
}

impl Page for Smart {
    fn has_more(&self) -> bool {
        true
    }

    fn states(&self) -> usize {
        6
    }

    fn set_state(&mut self, i: usize) {
        self.forced = Some(i.min(5));
    }

    fn rebuild_on(&self, local: usize, _cx: &Ctx) -> bool {
        [idx::ENGINE, idx::SLOW_MODE, idx::CHANNEL_MODE].contains(&local)
    }

    fn view_merge(&self) -> &'static [Merge] {
        use Merge::{Last as L, Max as X, Min as N};
        // gains: deepest; peaks: largest; the rest: newest
        &[N, N, N, N, N, N, N, N, N, N, L, L, N, N, X, X, X, X, X, L, L, L, L, L, L, L, L, X]
    }

    fn warning(&self, cx: &Ctx) -> bool {
        cx.on(idx::DELTA)
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let h = |l: usize| cx.p(l);
        let e = self.engine(cx);
        let mut v = vec![
            // The page paints the six keys; the grip keeps the parameter reachable and
            // lets the wheel step it.
            Cell::grip(h(idx::ENGINE), engine_keys(cx)).dim(true).tip("Engine: click a key; click the lit key to go back to the previous engine (the dot)"),
        ];
        if cx.more {
            v.push(Cell::segmented(h(idx::OVERSAMPLING), "LIVE", (120.0, 666.0, 144.0, 24.0)).keys(&["4x", "8x", "16x"]).tip("Oversampling while playing (Pure, Precise, Punch)"));
            v.push(Cell::segmented(h(idx::OVERSAMPLING_OFFLINE), "OFFLINE BOUNCE", (280.0, 666.0, 208.0, 24.0)).keys(&["SAME", "4x", "8x", "16x"]).tip("Oversampling for an offline bounce; the latency stays the same"));
            match e {
                PURE => {
                    let crest = Self::crest_mode(cx);
                    v.push(Cell::segmented(h(idx::SLOW_MODE), "SLOW DRIVER", (528.0, 666.0, 216.0, 24.0)).keys(&["PEAK BODY", "LOUDNESS CREST"]).tip("What drives the slow body stage"));
                    v.push(Cell::segmented(h(idx::SLOW_WINDOW), "SLOW WINDOW", (528.0, 714.0, 216.0, 24.0)).keys(&["200 ms", "400 ms", "3 s"]).tip("The slow stage's window; its attack and release scale with it"));
                    v.push(Cell::knob(Size::Small, h(idx::BODY_SHARE), "BODY", 768.0, MORE_KNOB_Y).tip(if crest { "Body share (used by Peak Body)" } else { "How much of a steady excess the slow stage takes" }));
                    v.push(Cell::knob(Size::Small, h(idx::CREST_HEADROOM), "CREST ROOM", 864.0, MORE_KNOB_Y).tip(if crest { "Crest headroom: the slow threshold is this far under the ceiling" } else { "Crest headroom (used by Loudness Crest)" }));
                    v.push(Cell::knob(Size::Small, h(idx::BASS_HOLD_MAX), "BASS HOLD", 960.0, MORE_KNOB_Y).tip("Longest post-hold on bass, 0 to 20 ms"));
                    v.push(Cell::pill(h(idx::K_WEIGHTING), "K-WEIGHTED", (768.0, 742.0, 80.0, 22.0)).tip("The slow stage listens K-weighted"));
                }
                PUNCH => {
                    v.push(Cell::knob(Size::Small, h(idx::CLIP_KNEE), "CLIP KNEE", 528.0, MORE_KNOB_Y).format(|x| format!("{x:.2}")).tip("Clipper knee: 0.1 hard to 1.0 soft"));
                    v.push(Cell::knob(Size::Small, h(idx::BODY_SHARE), "BODY", 624.0, MORE_KNOB_Y).tip("Depth of the slow leveller"));
                }
                MULTI => {
                    v.push(Cell::value(h(idx::SPLIT1), "SPLIT 1", (528.0, 666.0, 96.0, 22.0)).tip("Split 1, 40 to 400 Hz: drag, or double-click to type"));
                    v.push(Cell::value(h(idx::SPLIT2), "SPLIT 2", (640.0, 666.0, 96.0, 22.0)).tip("Split 2, 250 Hz to 2 kHz"));
                    v.push(Cell::value(h(idx::SPLIT3), "SPLIT 3", (752.0, 666.0, 96.0, 22.0)).tip("Split 3, 1.5 to 10 kHz"));
                    v.push(Cell::segmented(h(idx::CROSSOVER_MODE), "CROSSOVER", (880.0, 666.0, 200.0, 24.0)).keys(&["LR4", "LINEAR PHASE"]).tip("Crossover; the latency stays the same"));
                }
                _ => {}
            }
            self.detail_cells(cx, &mut v);
            return v;
        }
        v.push(Cell::knob(Size::Big, h(idx::DRIVE), "DRIVE", DRIVE_X, BIG_Y).format(|x| format!("{x:+.1} dB")));
        v.push(Cell::knob(Size::Big, h(idx::CEILING), "CEILING", CEILING_X, BIG_Y).format(|x| format!("{x:.1} dB")).tip("The output ceiling; the badge in the header reads it"));
        v.push(Cell::knob(Size::Big, h(idx::RELEASE), "RELEASE", RELEASE_X, BIG_Y).tip("Release time; with AUTO it follows the crest factor, x1/4 to x4"));
        v.push(Cell::knob(Size::Small, h(idx::LOOKAHEAD), "LOOKAHEAD", COL[0], ROW1).tip("Look-ahead. Does not change latency"));
        v.push(Cell::knob(Size::Small, h(idx::KNEE), "KNEE", COL[1], ROW1).tip("Soft knee around the ceiling, 0 to 10 dB"));
        v.push(Cell::segmented(h(idx::CHANNEL_MODE), "CHANNELS", (712.0, 638.0, 176.0, 24.0)).keys(&["STEREO", "MID-SIDE"]).tip("Limit left and right, or mid and side"));
        v.push(Cell::value(h(idx::TRANSIENT_LINK), "LINK PEAKS", (712.0, 686.0, 84.0, 22.0)).tip("How much the two channels share the fast reduction; drag, or double-click to type"));
        v.push(Cell::value(h(idx::RELEASE_LINK), "LINK RELEASE", (804.0, 686.0, 84.0, 22.0)).tip("How much the two channels share the release"));
        v.push(Cell::pill(h(idx::TRUE_PEAK), "TRUE PEAK", (COL[0], ROW2, 80.0, 22.0)).tip("Hold the true (inter-sample) peak, not only the samples"));
        v.push(Cell::pill(h(idx::GAIN_MATCH), "GAIN MATCH", (712.0, ROW2, 80.0, 22.0)).tip("Turn-down only: cannot pass the ceiling"));
        v.push(Cell::grip(h(idx::DELTA), DELTA_RECT).dim(true).tip(if e == PUNCH {
            "Delta: hear what the limiter removes. Click to latch; press and hold to audition. Delta does not null in Punch"
        } else {
            "Delta: hear what the limiter removes. Click to latch; press and hold to audition"
        }));
        v.push(Cell::pill(h(idx::AUTO_RELEASE), "AUTO", (COL[5], ROW2, 80.0, 22.0)).tip("Release follows the crest factor, x1/4 to x4"));
        match e {
            PURE => {
                if Self::crest_mode(cx) {
                    v.push(Cell::knob(Size::Small, h(idx::CREST_HEADROOM), "CREST ROOM", COL[4], ROW1).tip("Crest headroom: the slow threshold is this far under the ceiling"));
                } else {
                    v.push(Cell::knob(Size::Small, h(idx::BODY_SHARE), "BODY", COL[4], ROW1).tip("How much of a steady excess the slow stage takes"));
                }
                v.push(Cell::knob(Size::Small, h(idx::JOIN_WIDTH), "JOIN", COL[5], ROW1).tip("Join width: how gently a second peak joins a held one"));
                v.push(Cell::pill(h(idx::PAUSE_HOLD), "PAUSE HOLD", (COL[4], ROW2, 80.0, 22.0)).tip("Freeze the slow gain through pauses and fades"));
            }
            PUNCH => {
                v.push(Cell::knob(Size::Small, h(idx::BODY_SHARE), "BODY", COL[4], ROW1).tip("Depth of the slow leveller"));
                v.push(Cell::knob(Size::Small, h(idx::CLIP_HEADROOM), "CLIP ROOM", COL[5], ROW1).tip("How far above the ceiling the limiter stops and the clipper takes over"));
            }
            MULTI => {
                v.push(Cell::knob(Size::Small, h(idx::BAND_FOCUS), "FOCUS", COL[4], ROW1).tip("0 %: every band takes its share; 100 %: the band that causes the peak takes it"));
                v.push(Cell::pill(h(idx::CROSSOVER_MODE), "LIN PHASE", (COL[4], ROW2, 80.0, 22.0)).tip("Linear-phase crossover (off: LR4); the latency stays the same"));
            }
            _ => {}
        }
        // CURVE: the ceiling line and the knee corner are grips.
        let (_, p) = self.geo(cx).curve;
        let c = h(idx::CEILING);
        let info = &INFO.params[c];
        let y = |d: f32| p.1 + p.3 * (1.0 - (d - CURVE_LO) / -CURVE_LO);
        let knee_y = y(cx.v(idx::CEILING)).clamp(p.1 + 40.0, p.1 + p.3 - 40.0);
        let knee_x = (p.0 + p.2 * ((cx.v(idx::CEILING) - cx.v(idx::DRIVE) - CURVE_LO) / -CURVE_LO)).clamp(p.0 + 12.0, p.0 + p.2 - 40.0 - 12.0);
        v.push(Cell::grip(h(idx::KNEE), (knee_x - 12.0, knee_y - 40.0, 24.0, 80.0)).tip("Knee: drag the corner up or down, 0 to 10 dB"));
        v.push(Cell::grip(c, (p.0 + p.2 - 40.0, y(info.max), 40.0, y(info.min) - y(info.max))).tip("Ceiling: drag the line; double-click resets"));
        self.detail_cells(cx, &mut v);
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let e = self.engine(cx);
        let g = self.geo(cx);
        // CURVE
        let (_, p) = g.curve;
        kit::plot(s, t, p);
        let (x0, y0, w, hh) = p;
        for d in [-12.0f32, -24.0] {
            let f = (d - CURVE_LO) / -CURVE_LO;
            s.line(Point::new(x0 + w * f, y0), Point::new(x0 + w * f, y0 + hh), t.grid, 1.0);
            s.line(Point::new(x0, y0 + hh * (1.0 - f)), Point::new(x0 + w, y0 + hh * (1.0 - f)), t.grid, 1.0);
        }
        kit::dashed(s, &[Point::new(x0, y0 + hh), Point::new(x0 + w, y0)], 4.0, 3.0, t.grid_strong, 1.0);
        for d in [-36.0f32, -24.0, -12.0, 0.0] {
            let f = (d - CURVE_LO) / -CURVE_LO;
            s.text(Point::new(x0 + w * f, y0 + hh + 9.0), format!("{d:.0}"), t.dim, 8.5, true);
            kit::text_right(s, x0 - 4.0, y0 + hh * (1.0 - f), &format!("{d:.0}"), t.dim, 8.5);
        }
        s.text(Point::new(x0 + w * 0.5, y0 + hh + 21.0), "IN dBFS", t.dim, 8.0, true);
        // RECOVERY
        if let Some((_, r)) = g.rec {
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
        }
        // DETAIL
        if let Some((_, r)) = g.detail {
            paint_detail_static(s, t, cx, e, r);
        }
        // TRACE
        let (_, tr) = g.trace;
        kit::plot(s, t, tr);
        for d in [3.0f32, 6.0, 9.0] {
            let y = tr.1 + d / TRACE_RANGE * tr.3;
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
        kit::inset(s, t, g.maxbox, 4.0);
        // The control zone.
        if cx.more {
            s.text(Point::new(120.0, 630.0), "MORE", t.dim, 9.0, false);
            s.text(Point::new(170.0, 630.0), "QUALITY", t.dim, 9.0, false);
            s.text(Point::new(120.0, 656.0), "LIVE", t.dim, 9.0, false);
            s.text(Point::new(280.0, 656.0), "OFFLINE BOUNCE", t.dim, 9.0, false);
            s.line(Point::new(512.0, 632.0), Point::new(512.0, 764.0), t.border, 1.0);
            s.text(Point::new(528.0, 630.0), format!("{} ADVANCED", KEYS[e]), t.dim, 9.0, false);
            match e {
                PURE => {
                    s.text(Point::new(528.0, 656.0), "SLOW DRIVER", t.dim, 9.0, false);
                    s.text(Point::new(528.0, 704.0), "SLOW WINDOW", t.dim, 9.0, false);
                }
                MULTI => {
                    for (x, l) in [(528.0, "SPLIT 1"), (640.0, "SPLIT 2"), (752.0, "SPLIT 3"), (880.0, "CROSSOVER")] {
                        s.text(Point::new(x, 656.0), l, t.dim, 9.0, false);
                    }
                }
                PUNCH => {}
                _ => {
                    s.text(Point::new(528.0, 660.0), "No advanced controls for this engine.", t.dim, 10.0, false);
                }
            }
            return;
        }
        frame::divider(s, t, 508.0);
        s.text(Point::new(712.0, 628.0), "CHANNELS", t.dim, 9.0, false);
        s.text(Point::new(712.0, 677.0), "LINK PEAKS", t.dim, 9.0, false);
        s.text(Point::new(804.0, 677.0), "LINK RELEASE", t.dim, 9.0, false);
        bracket_down(s, t, 712.0, 888.0, 722.0, "LISTEN");
        let name = match e {
            PURE => format!("PURE · {}", if Self::crest_mode(cx) { "LOUDNESS CREST" } else { "PEAK BODY" }),
            _ => KEYS[e].replace("MULTI", "MULTIBAND"),
        };
        frame::bracket(s, t, 904.0, 1080.0, 722.0, &name, t.dim);
        if matches!(e, SMOOTH | PRECISE | GLIDE) {
            kit::inset(s, t, (904.0, 632.0, 176.0, 84.0), 6.0);
        }
        if e == MULTI {
            kit::inset(s, t, (1000.0, 632.0, 80.0, 84.0), 6.0);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let e = self.engine(cx);
        let g = self.geo(cx);
        let view = live.view;
        let delta = cx.on(idx::DELTA);
        let ceiling = cx.v(idx::CEILING);
        let tp_on = cx.on(idx::TRUE_PEAK);

        // ---- header: engine keys, badge, MORE dot ----
        let keys = engine_keys(cx);
        kit::inset(s, t, keys, 6.0);
        for (i, name) in KEYS.iter().enumerate() {
            let r = key_rect(cx, i);
            let key = Rect::new(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0);
            if i == e {
                s.fill_round(key, 4.0, t.accent, t.accent, true);
            }
            s.text(key.center(), *name, if i == e { t.on_accent } else { t.text }, 9.0, true);
            if i == self.prev && i != e {
                s.fill_ellipse_solid(Scene::ellipse_at(Point::new(r.0 + r.2 - 7.0, r.1 + 6.0), 2.0), t.accent);
            }
        }
        paint_badge(s, cx, self, view, delta, ceiling, tp_on);
        if Self::more_in_use(cx) {
            let r = more_dot(cx);
            s.fill_ellipse_solid(Scene::ellipse_at(Point::new(r.0 + 6.0, r.1 + 5.0), 3.0), if cx.more { t.on_accent } else { t.accent });
        }

        // ---- CURVE ----
        paint_curve(s, cx, &g, view, e, delta);

        // ---- RECOVERY ----
        if let Some((rp, r)) = g.rec {
            if !cx.zones.tall {
                let [a, b] = mid_tabs();
                tab(s, t, a, "RECOVERY", true);
                tab(s, t, b, DETAIL[e], false);
            } else {
                s.text(Point::new(rp.0 + 8.0, rp.1 + 12.0), "RECOVERY", t.dim, 9.0, false);
            }
            s.text(Point::new(rp.0 + 8.0, rp.1 + 196.0 - 4.0), "thin 5 ms peak · thick 500 ms", t.dim, 8.0, false);
            let _ = rp;
            if cx.on(idx::AUTO_RELEASE) {
                let rel = cx.v(idx::RELEASE);
                let (a, b) = (kit::freq_x((rel * 0.25).max(REC_T0), r.0, r.0 + r.2, REC_T0, REC_T1), kit::freq_x((rel * 4.0).min(REC_T1), r.0, r.0 + r.2, REC_T0, REC_T1));
                s.fill_round(Rect::new(a, r.1 + 1.0, (b - a).max(1.0), r.3 - 2.0), 0.0, t.accent.with_alpha(0.10), t.accent.with_alpha(0.10), true);
            }
            let ghost = self.probes[self.prev].out.as_ref().filter(|_| self.prev != e);
            if let Some(o) = ghost {
                let a = if self.since_switch < 2.0 { 0.9 } else { 0.55 };
                frame::paint_recovery(s, t, r, &o.short, &o.long, t.dim.with_alpha(a), true);
            }
            let probe = &self.probes[e];
            if let Some(o) = probe.out.as_ref() {
                let fade = if probe.stale() { 0.45 } else { 1.0 };
                frame::paint_recovery(s, t, r, &o.short, &o.long, t.accent.with_alpha(fade), false);
                let half = |x: &[f32]| ProbeOut::half_time(x).map_or_else(|| "–".to_string(), ms_text);
                kit::text_right(s, rp.0 + rp.2 - 8.0, rp.1 + 12.0, &format!("50 %: {} / {}", half(&o.short), half(&o.long)), t.text, 9.5);
                if let Some(go) = ghost {
                    let label = format!("- - {} {}", SHORT[self.prev], ProbeOut::half_time(&go.long).map_or_else(|| "–".to_string(), ms_text));
                    kit::text_right(s, r.0 + r.2 - 6.0, r.1 + r.3 - 8.0, &label, t.dim, 8.0);
                }
            } else {
                s.text(kit::rect(r).center(), "measuring…", t.dim, 9.5, true);
            }
        }

        // ---- DETAIL ----
        if let Some((dp, r)) = g.detail {
            if !cx.zones.tall {
                let [a, b] = mid_tabs();
                tab(s, t, a, "RECOVERY", false);
                tab(s, t, b, DETAIL[e], true);
            } else {
                s.text(Point::new(dp.0 + 8.0, dp.1 + 12.0), DETAIL[e], t.dim, 9.0, false);
            }
            paint_detail(s, cx, self, e, dp, r, view);
        }

        // ---- TRACE ----
        paint_trace(s, cx, self, &g, live, e, delta);

        // ---- GR meter ----
        let ms = Self::ms(cx);
        let (bx, by, bh) = g.bars;
        let range = self.gr_range;
        let hold_n = (live.hist_rate * 1.5) as usize;
        for c in 0..2 {
            let x = bx + 8.0 * c as f32;
            s.fill_round(Rect::new(x, by, 6.0, bh), 2.0, t.seg_off, t.seg_off, true);
            let now = -self.ring.back().map_or(0.0, |sn| sn.ch[c]);
            let depth = (now / range).clamp(0.0, 1.0) * bh;
            if depth > 0.5 {
                s.fill_round(Rect::new(x, by, 6.0, depth), 2.0, t.accent, t.accent, true);
            }
            let hold = self.ring.iter().rev().take(hold_n).fold(0.0f32, |a, sn| a.max(-sn.ch[c]));
            let hy = (hold / range).clamp(0.0, 1.0) * bh;
            if hy > 1.0 {
                s.line(Point::new(x, by + hy), Point::new(x + 6.0, by + hy), t.text, 1.5);
            }
            let label = if ms { ["M", "S"][c] } else { ["L", "R"][c] };
            s.text(Point::new(x + 3.0, by + bh + 9.0), label, t.dim, 8.5, true);
        }
        kit::gr_scale(s, t, (bx, by, 14.0, bh), range);
        s.text(kit::rect(g.maxbox).center(), format!("{:.1}", self.max_gr), t.text, 9.5, true);

        if cx.more {
            paint_more(s, cx, e, view);
            return;
        }

        // ---- the control zone's moving parts ----
        if cx.on(idx::AUTO_RELEASE) && self.rel_ms > 0.0 {
            s.text(Point::new(NOW_RECT.0 + NOW_RECT.2 * 0.5, NOW_RECT.1 + 7.0), format!("now {}", ms_text(self.rel_ms.max(0.1))), t.dim, 9.0, true);
        }
        // PK read-out: held output peak.
        let over = self.pk_hold > ceiling + 0.05;
        kit::inset(s, t, PK_RECT, 5.0);
        let label = if self.pk_hold <= -119.0 { format!("{} –", if tp_on { "TP" } else { "PK" }) } else { format!("{} {:.2}", if tp_on { "TP" } else { "PK" }, self.pk_hold) };
        s.text(kit::rect(PK_RECT).center(), label, if over { t.over } else { t.text }, 10.0, true);
        // Gain match amount, under its pill.
        if cx.on(idx::GAIN_MATCH) {
            s.text(Point::new(MATCH_RECT.0 + MATCH_RECT.2 * 0.5, MATCH_RECT.1 + 5.0), format!("MATCH {:.1} dB", v(view, view::MATCH_GAIN)), t.dim, 8.5, true);
        }
        // DELTA: never by colour alone.
        let rr = kit::rect(DELTA_RECT);
        if delta {
            s.fill_round(rr, 11.0, t.over, t.over, true);
            s.stroke_round(rr, 11.0, t.text, 1.5);
            s.text(rr.center(), "! DELTA", t.on_accent, 9.5, true);
        } else {
            frame::page_pill(s, t, DELTA_RECT, "DELTA", false, false);
        }
        // Columns 5-6 for the engines with no controls there, and the splits.
        let crest = self.crest;
        let rel = self.rel_ms;
        let card = |s: &mut Scene, a: String, b: String| {
            s.text(Point::new(912.0, 660.0), a, t.dim, 11.0, false);
            s.text(Point::new(912.0, 684.0), b, t.dim, 11.0, false);
        };
        let crest_line = || if cx.on(idx::AUTO_RELEASE) { format!("crest {crest:.1} dB, release {}", ms_text(rel.max(0.1))) } else { format!("crest {crest:.1} dB, release fixed") };
        match e {
            SMOOTH => card(s, "1x, no oversampling".into(), crest_line()),
            PRECISE => {
                let os = v(view, view::OS_FACTOR).max(1.0);
                let os = if os < 2.0 { [4.0, 8.0, 16.0][cx.choice(idx::OVERSAMPLING).min(2)] } else { os };
                card(s, format!("runs at {os:.0}x ({:.0} kHz)", os * Self::sr(cx) / 1000.0), crest_line());
            }
            GLIDE => card(s, format!("bridges gaps up to {}", ms_text(cx.v(idx::LOOKAHEAD))), "never starts more than 2.5 ms early".into()),
            MULTI => {
                let sp = limiter_smart::ordered_splits([cx.v(idx::SPLIT1) as f64, cx.v(idx::SPLIT2) as f64, cx.v(idx::SPLIT3) as f64], Self::sr(cx) as f64);
                s.text(Point::new(1040.0, 642.0), "SPLITS", t.dim, 8.5, true);
                for (k, f) in sp.iter().enumerate() {
                    s.text(Point::new(1040.0, 662.0 + 16.0 * k as f32), hz_text(*f as f32), t.text, 10.0, true);
                }
                s.text(Point::new(1040.0, 708.0), "edit in MORE", t.dim, 8.0, true);
            }
            _ => {}
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let g = self.geo(cx);
        let mut v = vec![g.curve.0, g.trace.0, meter_area(&g), frame::badge_rect(cx), engine_keys(cx), more_dot(cx)];
        v.extend(g.rec.map(|r| r.0));
        v.extend(g.detail.map(|r| r.0));
        if cx.more {
            v.push(MORE_NOTES);
            v.push(MORE_ENGINE_NOTE);
            v.push((RESET_SPLITS.0 - 2.0, RESET_SPLITS.1 - 2.0, RESET_SPLITS.2 + 4.0, RESET_SPLITS.3 + 4.0));
        } else {
            v.extend([NOW_RECT, PK_RECT, (DELTA_RECT.0 - 2.0, DELTA_RECT.1 - 2.0, DELTA_RECT.2 + 4.0, DELTA_RECT.3 + 4.0), MATCH_RECT, CONTEXT_RECT]);
        }
        v
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        let e = self.engine(cx);
        let dt = live.dt.clamp(0.0, 0.5);
        self.rate = live.hist_rate;
        // Engine switches: remember the A/B partner.
        match self.seen {
            Some(s) if s != e => {
                self.prev = s;
                self.since_switch = 0.0;
            }
            _ => self.since_switch += dt,
        }
        self.seen = Some(e);
        // Mid-Side picks the channel view once; back to Stereo picks the stages.
        let ms = Self::ms(cx);
        if self.seen_ms.is_some_and(|m| m != ms) {
            self.trace_ch = ms;
        }
        self.seen_ms = Some(ms);
        self.tick_probes(cx, dt);
        if e == GLIDE {
            let vals: Vec<f32> = (0..cx.count).map(|i| cx.v(i)).collect();
            self.bridge.tick(vals, Self::sr(cx));
        }
        let view = live.view;
        let delta = cx.on(idx::DELTA);
        if !view.is_empty() {
            let snap = Snap::from(view);
            let cap = (live.hist_rate.max(10.0) * TRACE_S).ceil() as usize + 2;
            self.ring.push_back(snap);
            while self.ring.len() > cap {
                self.ring.pop_front();
            }
            let cap = (live.hist_rate.max(10.0) * STRIP_S).ceil() as usize + 2;
            self.strip.push_back((v(view, view::SLOW_GAIN).min(0.0), v(view, view::PAUSE_HOLD) >= 0.5));
            while self.strip.len() > cap {
                self.strip.pop_front();
            }
            for (b, hold) in self.band_hold.iter_mut().enumerate() {
                let d = -snap.band[b];
                hold.1 += dt;
                if d >= hold.0 || hold.1 > 1.5 {
                    *hold = (d, 0.0);
                }
            }
            self.loud = (v(view, view::LOUDNESS_IN), v(view, view::LOUDNESS_OUT));
            let k = (dt * 10.0).min(1.0);
            self.crest += (v(view, view::CREST) - self.crest) * k;
            let rel = v(view, view::RELEASE_MS);
            self.rel_ms = if self.rel_ms <= 0.0 { rel } else { self.rel_ms + (rel - self.rel_ms) * k };
            // Held output peak: the true peak after the trim, or the sample peak.
            let pk = if cx.on(idx::TRUE_PEAK) { v(view, view::OUT_TRUE_PEAK) } else { v(view, view::OUT_PEAK_L).max(v(view, view::OUT_PEAK_R)) };
            let pk = if view.len() > view::OUT_TRUE_PEAK || !cx.on(idx::TRUE_PEAK) { pk } else { v(view, view::OUT_PEAK_L).max(v(view, view::OUT_PEAK_R)) };
            if pk > -119.0 && !delta {
                self.pk_hold = self.pk_hold.max(pk);
                let excess = pk - cx.v(idx::CEILING);
                if excess > 0.05 {
                    self.over = (2.0, excess.max(self.over.1 * f32::from(self.over.0 > 0.0)));
                }
            }
        }
        self.over.0 = (self.over.0 - dt).max(0.0);
        self.max_gr = self.max_gr.max(live.gr);
        true
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        let e = self.engine(cx);
        let g = self.geo(cx);
        match ev {
            Pointer::Down { x, y, .. } => {
                if kit::inside(engine_keys(cx), x, y) {
                    let i = (((x - engine_keys(cx).0) / 56.0).floor().max(0.0) as usize).min(5);
                    let target = if i == e { self.prev } else { i };
                    self.forced = None;
                    io.set(cx.p(idx::ENGINE), target as f32);
                    return Response::REBUILD;
                }
                if kit::inside(frame::badge_rect(cx), x, y) && cx.on(idx::DELTA) {
                    io.set(cx.p(idx::DELTA), 0.0);
                    return Response::TAKEN;
                }
                if kit::inside(g.maxbox, x, y) {
                    self.max_gr = 0.0;
                    return Response::TAKEN;
                }
                if kit::inside((g.bars.0 - 2.0, g.bars.1, 18.0, g.bars.2), x, y) {
                    self.gr_range = if self.gr_range > 12.0 { 12.0 } else { 24.0 };
                    return Response::REBUILD;
                }
                if !cx.zones.tall {
                    let [a, b] = mid_tabs();
                    for (r, d) in [(a, false), (b, true)] {
                        if kit::inside(r, x, y) && self.detail_tab[e] != d {
                            self.detail_tab[e] = d;
                            return Response::REBUILD;
                        }
                    }
                }
                let [a, b] = trace_tabs(&g);
                for (r, c) in [(a, false), (b, true)] {
                    if kit::inside(r, x, y) {
                        self.trace_ch = c;
                        return Response::TAKEN;
                    }
                }
                if cx.more {
                    if e == MULTI && kit::inside(RESET_SPLITS, x, y) {
                        for l in [idx::SPLIT1, idx::SPLIT2, idx::SPLIT3] {
                            let p = cx.p(l);
                            io.set(p, INFO.params[p].default);
                        }
                        return Response::TAKEN;
                    }
                    return Response::NONE;
                }
                if kit::inside(PK_RECT, x, y) {
                    self.pk_hold = -120.0;
                    self.over = (0.0, 0.0);
                    return Response::TAKEN;
                }
                if kit::inside(DELTA_RECT, x, y) {
                    let p = cx.p(idx::DELTA);
                    if cx.on(idx::DELTA) {
                        io.set(p, 0.0);
                        self.delta_press = None;
                    } else {
                        io.set(p, 1.0);
                        self.delta_press = Some(Instant::now());
                    }
                    return Response::TAKEN;
                }
                Response::NONE
            }
            Pointer::Up { .. } => {
                if let Some(t0) = self.delta_press.take() {
                    // Held: an audition, released to off. A short click latches.
                    if t0.elapsed().as_secs_f32() > 0.25 {
                        io.set(cx.p(idx::DELTA), 0.0);
                    }
                    return Response::TAKEN;
                }
                Response::NONE
            }
            Pointer::Wheel { x, y, dy } => {
                if kit::inside(engine_keys(cx), x, y) {
                    let next = if dy > 0.0 { (e + 5) % 6 } else { (e + 1) % 6 };
                    io.set(cx.p(idx::ENGINE), next as f32);
                    return Response::REBUILD;
                }
                Response::NONE
            }
            Pointer::Move { .. } => Response::NONE,
        }
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let g = self.geo(cx);
        let lat = limiter_smart::module_latency(Self::sr(cx));
        let mut v = vec![
            frame::tip(frame::badge_rect(cx), "The most this module can put out: its ceiling (dBTP with TRUE PEAK on). Shows OVER for 2 s if the output passed it; with delta on, click to switch delta off"),
            frame::tip(latency_rect(cx), format!("{:.1} ms, the same for all six engines and every setting", lat as f32 / Self::sr(cx) * 1000.0)),
            frame::tip(g.maxbox, "Deepest reduction; click to reset"),
            frame::tip((g.bars.0, g.bars.1, 14.0, g.bars.2), "Reduction per channel (L/R or M/S); click for a 12 or 24 dB range"),
            frame::tip(g.trace.0, "This module's reduction, 4 s, split into the stages that take it; L/R shows the two channels"),
        ];
        for (i, d) in DESC.iter().enumerate() {
            v.push(frame::tip(key_rect(cx, i), *d));
        }
        if let Some((r, _)) = g.rec {
            v.push(frame::tip(r, "Gain recovery after a 6 dB overshoot, measured on a copy of this module; dashed: the previous engine"));
        }
        if !cx.more {
            v.push(frame::tip(PK_RECT, "Held output peak after the trim; click to reset"));
            let e = self.engine(cx);
            if cx.on(idx::GAIN_MATCH) {
                v.push(frame::tip(MATCH_RECT, format!("Applied gain match; in {:.1} · out {:.1} LUFS", self.loud.0, self.loud.1)));
            }
            if e == MULTI {
                v.push(frame::tip((1000.0, 632.0, 80.0, 84.0), "The split frequencies: edit them under MORE"));
            }
        }
        v
    }

    fn menu(&self, cx: &Ctx) -> Vec<String> {
        if self.engine(cx) == MULTI {
            vec!["Reset splits to factory".into()]
        } else {
            Vec::new()
        }
    }

    fn menu_action(&mut self, item: usize, cx: &Ctx, io: &mut Io) -> Response {
        if item == 0 && self.engine(cx) == MULTI {
            for l in [idx::SPLIT1, idx::SPLIT2, idx::SPLIT3] {
                let p = cx.p(l);
                io.set(p, INFO.params[p].default);
            }
            return Response::TAKEN;
        }
        Response::NONE
    }
}

// =====================================================================================
// Painting
// =====================================================================================

fn paint_badge(s: &mut Scene, cx: &Ctx, page: &Smart, view: &[f32], delta: bool, ceiling: f32, tp_on: bool) {
    let _ = view;
    let t = cx.t;
    let r = kit::rect(frame::badge_rect(cx));
    let over = page.over.0 > 0.0;
    let alarm = delta || over;
    s.fill_round(r, 5.0, if delta { t.over } else { t.inset }, if delta { t.over } else { t.inset }, true);
    s.stroke_round(r, 5.0, if alarm { if delta { t.text } else { t.over } } else { t.border }, if alarm { 1.5 } else { 1.0 });
    let (label, ink) = if delta {
        ("! DELTA ON".to_string(), t.on_accent)
    } else if over {
        (format!("! OVER {:+.2}", page.over.1), t.over)
    } else {
        (format!("OUT MAX {ceiling:.1} {}", if tp_on { "dBTP" } else { "dBFS" }), t.text)
    };
    s.text(r.center(), label, ink, 10.0, true);
}

fn curve_xy(p: R, i: f32, o: f32) -> Point {
    let (x0, y0, w, h) = p;
    Point::new(x0 + w * ((i - CURVE_LO) / -CURVE_LO), y0 + h * (1.0 - (o.clamp(CURVE_LO - 2.0, 2.0) - CURVE_LO) / -CURVE_LO))
}

fn paint_curve(s: &mut Scene, cx: &Ctx, g: &Geo, view: &[f32], e: usize, delta: bool) {
    let t = cx.t;
    let (_, p) = g.curve;
    let (drive, c, knee) = (cx.v(idx::DRIVE) as f64, cx.v(idx::CEILING) as f64, cx.v(idx::KNEE) as f64);
    let (hr, ck) = (cx.v(idx::CLIP_HEADROOM) as f64, cx.v(idx::CLIP_KNEE) as f64);
    let out = |i: f64| if e == PUNCH { limiter_smart::punch_static_db(i, drive, c, knee, hr, ck) } else { limiter_smart::static_curve_db(i, drive, c, knee) };
    s.text(Point::new(p.0 + 6.0, p.1 + 10.0), "CURVE", t.dim, 8.5, false);
    s.clip_round(kit::rect(p), 4.0);
    let steps = 144;
    let pts: Vec<Point> = (0..=steps)
        .map(|k| {
            let i = CURVE_LO + -CURVE_LO * k as f32 / steps as f32;
            curve_xy(p, i, out(i as f64) as f32)
        })
        .collect();
    let unity: Vec<Point> = (0..=steps)
        .map(|k| {
            let i = CURVE_LO + -CURVE_LO * k as f32 / steps as f32;
            curve_xy(p, i, i)
        })
        .collect();
    band(s, &unity, &pts, t.accent.with_alpha(0.12));
    let cf = c as f32;
    match e {
        PUNCH => {
            let y = curve_xy(p, 0.0, cf + hr as f32).y;
            kit::dashed_h(s, p.0, p.0 + p.2, y, 3.0, 3.0, t.dim, 1.0);
            s.text(Point::new(p.0 + 6.0, y + 7.0), "LIMIT", t.dim, 8.0, false);
        }
        PURE if cx.choice(idx::SLOW_MODE) == 0 => {
            // The wedge above the ceiling, split into what the body and the peak lane take.
            let b = cx.v(idx::BODY_SHARE) as f64 * 0.01;
            let split: Vec<Point> = (0..=steps)
                .filter_map(|k| {
                    let i = CURVE_LO + -CURVE_LO * k as f32 / steps as f32;
                    let x = i as f64 + drive;
                    (x > c).then(|| {
                        let (slow, _) = limiter_smart::body_split_db(x - c, b);
                        curve_xy(p, i, (x + slow) as f32)
                    })
                })
                .collect();
            if split.len() >= 2 {
                kit::dashed(s, &split, 3.0, 3.0, t.dim, 1.0);
                let last = split[split.len() - 1];
                let y_c = curve_xy(p, 0.0, cf).y;
                if last.y < y_c - 22.0 {
                    kit::text_right(s, last.x - 4.0, (last.y + y_c) * 0.5, "PEAK", t.dim, 8.0);
                    kit::text_right(s, last.x - 4.0, last.y - 8.0, "BODY", t.dim, 8.0);
                }
            }
        }
        PURE => {
            let th = cf - cx.v(idx::CREST_HEADROOM);
            let y = curve_xy(p, 0.0, th).y;
            s.line(Point::new(p.0, y), Point::new(p.0 + 8.0, y), t.dim, 1.5);
            s.text(Point::new(p.0 + 11.0, y), "CREST", t.dim, 8.0, false);
            let loud = v(view, view::SLOW_LOUDNESS);
            if loud > CURVE_LO && !delta {
                s.fill_ellipse_solid(Scene::ellipse_at(curve_xy(p, (loud - drive as f32).min(0.0), CURVE_LO - 0.0).translated_y(-3.0), 3.0), t.warm);
            }
        }
        _ => {}
    }
    kit::curve(s, pts, t.accent, 2.0);
    // the ceiling line and its grip handle
    let y = curve_xy(p, 0.0, cf).y;
    s.line(Point::new(p.0, y), Point::new(p.0 + p.2, y), t.accent.with_alpha(0.7), 1.0);
    s.fill_round(Rect::new(p.0 + p.2 - 14.0, y - 4.0, 12.0, 8.0), 3.0, t.accent, t.accent, true);
    // the knee corner
    let corner = (cf - drive as f32).clamp(CURVE_LO, 0.0);
    let kp = curve_xy(p, corner, cf);
    let kx = kp.x.clamp(p.0 + 12.0, p.0 + p.2 - 52.0);
    s.stroke_ellipse(Scene::ellipse_at(Point::new(kx, kp.y), 4.0), t.accent, 1.5);
    // the live dot, gain match taken out
    let pin = v(view, view::IN_PEAK_L).max(v(view, view::IN_PEAK_R));
    let pout = v(view, view::OUT_PEAK_L).max(v(view, view::OUT_PEAK_R)) - v(view, view::MATCH_GAIN).min(0.0);
    if pin > CURVE_LO && !delta && !view.is_empty() {
        s.fill_ellipse_solid(Scene::ellipse_at(curve_xy(p, pin.min(0.0), pout), 4.0), t.text);
    }
    s.pop_clip();
    if knee > 0.05 {
        kit::text_right(s, p.0 + p.2 - 4.0, p.1 + p.3 - 10.0, &format!("knee {knee:.1} dB"), t.dim, 8.0);
    }
}

trait TranslateY {
    fn translated_y(self, dy: f32) -> Self;
}

impl TranslateY for Point {
    fn translated_y(self, dy: f32) -> Self {
        Point::new(self.x, self.y + dy)
    }
}

fn paint_trace(s: &mut Scene, cx: &Ctx, page: &Smart, g: &Geo, live: &Live, e: usize, delta: bool) {
    let t = cx.t;
    let (tp, tr) = g.trace;
    let ms = Smart::ms(cx);
    s.text(Point::new(tp.0 + 8.0, tp.1 + 12.0), "TRACE", t.dim, 9.0, false);
    let [ta, tb] = trace_tabs(g);
    tab(s, t, ta, "STAGES", !page.trace_ch);
    tab(s, t, tb, if ms { "M/S" } else { "L/R" }, page.trace_ch);
    let ring = &page.ring;
    let n = ring.len();
    let span = (live.hist_rate.max(10.0) * TRACE_S).max(2.0);
    let shown: Vec<(f32, &Snap)> = ring.iter().enumerate().filter(|(i, _)| ((n - 1 - i) as f32) <= span).map(|(i, sn)| (tr.0 + tr.2 * (1.0 - (n - 1 - i) as f32 / span), sn)).collect();
    let y = |d: f32| tr.1 + (d / TRACE_RANGE).clamp(0.0, 1.0) * tr.3;
    let top: Vec<Point> = shown.iter().map(|(x, _)| Point::new(*x, tr.1)).collect();
    let line = |f: &dyn Fn(&Snap) -> f32| -> Vec<Point> { shown.iter().map(|(x, sn)| Point::new(*x, y(f(sn)))).collect() };
    s.clip_round(kit::rect(tr), 4.0);
    if shown.len() >= 2 {
        if page.trace_ch {
            let c0 = line(&|sn| -sn.ch[0]);
            let c1 = line(&|sn| -sn.ch[1]);
            band(s, &top, &c0, t.accent.with_alpha(0.20));
            kit::curve(s, c0, t.accent, 1.5);
            kit::curve(s, c1, t.text.with_alpha(0.7), 1.0);
        } else {
            let total = line(&|sn| -sn.total);
            // Stage shares scaled to the total (the stage values are separate minima).
            let share = |sn: &Snap, parts: &[f32]| -> Vec<f32> {
                let sum: f32 = parts.iter().map(|p| -p).sum();
                if sum <= 1e-6 {
                    return vec![0.0; parts.len()];
                }
                parts.iter().map(|p| -p / sum * -sn.total).collect()
            };
            match e {
                PURE | PUNCH => {
                    let stacked = |k: usize| -> Vec<Point> {
                        shown
                            .iter()
                            .map(|(x, sn)| {
                                let parts = if e == PUNCH { vec![sn.slow, sn.fast, sn.clip] } else { vec![sn.slow, sn.fast] };
                                let d: f32 = share(sn, &parts).iter().take(k).sum();
                                Point::new(*x, y(d))
                            })
                            .collect()
                    };
                    let (b, pk) = (stacked(1), stacked(2));
                    band(s, &top, &b, t.text.with_alpha(0.18));
                    band(s, &b, &pk, t.accent.with_alpha(0.28));
                    kit::curve(s, pk.clone(), t.accent, 1.5);
                    if e == PUNCH {
                        let cl = stacked(3);
                        band(s, &pk, &cl, t.text.with_alpha(0.45));
                    }
                }
                GLIDE => {
                    band(s, &top, &total, t.accent.with_alpha(0.28));
                }
                MULTI => {
                    // Graphite's teal sits close to band colour 4: a lighter total.
                    let a = if std::ptr::eq(cx.t, crate::ui::theme::theme(crate::ui::theme::ThemeId::Graphite)) { 0.12 } else { 0.15 };
                    band(s, &top, &total, t.accent.with_alpha(a));
                    let catcher = line(&|sn| -sn.catcher);
                    band(s, &top, &catcher, t.text.with_alpha(0.40));
                    for b in 0..4 {
                        let w = if a < 0.15 { 2.0 } else { 1.5 };
                        kit::curve(s, line(&|sn| -sn.band[b]), t.bands[b], w);
                    }
                }
                _ => {
                    band(s, &top, &total, t.accent.with_alpha(0.25));
                    kit::curve(s, line(&|sn| -sn.slow), t.text.with_alpha(0.7), 1.0);
                }
            }
            kit::curve(s, total, t.accent, 1.5);
        }
        // Engine switch marks.
        for w in shown.windows(2) {
            if w[0].1.engine != w[1].1.engine {
                let x = w[1].0;
                s.line(Point::new(x, tr.1), Point::new(x, tr.1 + 14.0), t.text, 1.0);
                let label = format!("{} {}", SHORT[w[0].1.engine as usize], SHORT[w[1].1.engine as usize]);
                let lx = (x + 3.0).min(tr.0 + tr.2 - text::width(&label, 8.0) - 2.0);
                s.text(Point::new(lx, tr.1 + 8.0), label, t.text, 8.0, false);
            }
        }
        // The trim lane: where the shared trim acted.
        let lane_y = tr.1 + tr.3 - 6.0;
        for (x, sn) in &shown {
            let d = (-sn.trim).min(1.0);
            if d > 0.01 {
                s.line(Point::new(*x, lane_y + 6.0), Point::new(*x, lane_y + 6.0 - 6.0 * d), t.text, 1.0);
            }
        }
    }
    s.pop_clip();
    let lane_y = tr.1 + tr.3 - 6.0;
    s.line(Point::new(tr.0, lane_y), Point::new(tr.0 + tr.2, lane_y), t.grid_strong, 1.0);
    s.text(Point::new(tr.0 + 4.0, lane_y - 5.0), if cx.on(idx::TRUE_PEAK) { "TP TRIM" } else { "PEAK TRIM" }, t.dim, 8.0, false);
    // Legend chips for the layers this engine has.
    if !page.trace_ch {
        let chips: &[(&str, C)] = match e {
            PURE => &[("BODY", t.text.with_alpha(0.18)), ("PEAK", t.accent.with_alpha(0.5))],
            PUNCH => &[("BODY", t.text.with_alpha(0.18)), ("LIMIT", t.accent.with_alpha(0.5)), ("CLIP", t.text.with_alpha(0.45))],
            MULTI => &[("BANDS", t.bands[1]), ("CATCH", t.text.with_alpha(0.4))],
            GLIDE => &[("PEAK", t.accent.with_alpha(0.5))],
            _ => &[("TOTAL", t.accent.with_alpha(0.4)), ("SLOW", t.text.with_alpha(0.7))],
        };
        let width: f32 = chips.iter().map(|(l, _)| 16.0 + text::width(l, 8.0)).sum();
        let mut x = tr.0 + tr.2 - 4.0 - width;
        let yy = tr.1 + tr.3 - 14.0;
        for (label, c) in chips {
            s.fill_round(Rect::new(x, yy - 3.5, 7.0, 7.0), 1.5, *c, *c, true);
            s.text(Point::new(x + 10.0, yy), *label, t.dim, 8.0, false);
            x += 16.0 + text::width(label, 8.0);
        }
    }
    // Title read-out.
    let window: Vec<&Snap> = ring.iter().rev().take(span as usize).collect();
    let label = if page.trace_ch {
        let last = ring.back().copied().unwrap_or_default();
        let (a, b) = if ms { ("M", "S") } else { ("L", "R") };
        format!("{a} {:.1} · {b} {:.1} dB", last.ch[0], last.ch[1])
    } else {
        let avg = if window.is_empty() { 0.0 } else { window.iter().map(|s| -s.total).sum::<f32>() / window.len() as f32 };
        let max = window.iter().fold(0.0f32, |a, s| a.max(-s.total));
        format!("AVG {avg:.1} · MAX {max:.1} dB")
    };
    kit::text_right(s, tp.0 + tp.2 - 8.0, tp.1 + 12.0, &label, t.text, 9.5);
    if delta {
        s.stroke_round(kit::rect(tr), 4.0, t.over, 1.5);
        let msg = "DELTA: you hear what was removed";
        let w = text::width(msg, 10.0) + 16.0;
        let box_r = Rect::new(tr.0 + (tr.2 - w) * 0.5, tr.1 + tr.3 - 40.0, w, 18.0);
        s.fill_round(box_r, 5.0, t.plot_bg, t.plot_bg, true);
        s.stroke_round(box_r, 5.0, t.over, 1.0);
        s.text(box_r.center(), msg, t.over, 10.0, true);
    }
}

fn paint_detail_static(s: &mut Scene, t: &Theme, cx: &Ctx, e: usize, p: R) {
    match e {
        PURE => {
            let (strip, bar, join) = Smart::pure_parts(p);
            kit::plot(s, t, strip);
            for d in [2.0f32, 4.0] {
                let y = strip.1 + d / STRIP_DB * strip.3;
                s.line(Point::new(strip.0, y), Point::new(strip.0 + strip.2, y), t.grid, 1.0);
            }
            kit::text_right(s, strip.0 - 4.0, strip.1 + strip.3, "-6", t.dim, 8.0);
            kit::text_right(s, strip.0 - 4.0, strip.1 + 4.0, "0", t.dim, 8.0);
            s.text(Point::new(strip.0 + 4.0, strip.1 + strip.3 + 8.0), "-12 s", t.dim, 8.0, false);
            kit::text_right(s, strip.0 + strip.2 - 2.0, strip.1 + strip.3 + 8.0, "now", t.dim, 8.0);
            s.text(Point::new(bar.0, bar.1 - 10.0), "POST-HOLD", t.dim, 8.0, false);
            kit::inset(s, t, bar, 3.0);
            for k in [0.0f32, 10.0, 20.0] {
                let x = bar.0 + bar.2 * k / 20.0;
                s.text(Point::new(x.clamp(bar.0 + 6.0, bar.0 + bar.2 - 8.0), bar.1 + bar.3 + 9.0), format!("{k:.0}"), t.dim, 8.0, true);
            }
            kit::plot(s, t, join);
            s.text(Point::new(join.0 + join.2 * 0.5, join.1 + join.3 + 9.0), "distance, dB", t.dim, 8.0, true);
        }
        SMOOTH | PRECISE => {
            kit::plot(s, t, p);
            for c in [6.0f32, 9.0, 12.0, 15.0, 18.0] {
                let x = p.0 + p.2 * (c - 3.0) / 18.0;
                s.line(Point::new(x, p.1), Point::new(x, p.1 + p.3), t.grid, 1.0);
            }
            for c in [3.0f32, 12.0, 21.0] {
                let x = (p.0 + p.2 * (c - 3.0) / 18.0).clamp(p.0 + 8.0, p.0 + p.2 - 8.0);
                s.text(Point::new(x, p.1 + p.3 + 10.0), format!("{c:.0}"), t.dim, 8.5, true);
            }
            s.text(Point::new(p.0 + p.2 * 0.5, p.1 + p.3 + 21.0), "crest factor, dB", t.dim, 8.0, true);
        }
        PUNCH => {
            kit::plot(s, t, p);
            for d in [-6.0f32, -3.0, 0.0, 3.0] {
                let x = Smart::clip_x(p, d);
                let y = Smart::clip_y(p, d);
                s.line(Point::new(x, p.1), Point::new(x, p.1 + p.3), if d == 0.0 { t.grid_strong } else { t.grid }, 1.0);
                s.line(Point::new(p.0, y), Point::new(p.0 + p.2, y), if d == 0.0 { t.grid_strong } else { t.grid }, 1.0);
                s.text(Point::new(x, p.1 + p.3 + 10.0), format!("{d:+.0}"), t.dim, 8.5, true);
                kit::text_right(s, p.0 - 4.0, y, &format!("{d:+.0}"), t.dim, 8.5);
            }
            s.text(Point::new(p.0 + p.2 * 0.5, p.1 + p.3 + 21.0), "in, dB from the ceiling", t.dim, 8.0, true);
        }
        MULTI => {
            kit::plot(s, t, p);
            for f in [100.0f32, 1000.0, 10_000.0] {
                let x = Smart::band_x(p, f);
                s.line(Point::new(x, p.1), Point::new(x, p.1 + p.3), t.grid, 1.0);
                s.text(Point::new(x, p.1 + p.3 + 10.0), if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") }, t.dim, 8.5, true);
            }
            for d in [3.0f32, 6.0, 9.0] {
                let y = p.1 + d / 12.0 * p.3;
                s.line(Point::new(p.0, y), Point::new(p.0 + p.2, y), t.grid, 1.0);
                kit::text_right(s, p.0 - 4.0, y, &format!("-{d:.0}"), t.dim, 8.5);
            }
        }
        _ => {
            kit::plot(s, t, p);
            for k in 0..=8 {
                let x = p.0 + p.2 * (k as f32 + 1.0) / 9.0;
                s.line(Point::new(x, p.1), Point::new(x, p.1 + p.3), if k == 0 { t.grid_strong } else { t.grid }, 1.0);
                if k % 2 == 0 {
                    s.text(Point::new(x, p.1 + p.3 + 10.0), format!("{k}"), t.dim, 8.5, true);
                }
            }
            s.text(Point::new(p.0 + 6.0, p.1 + p.3 + 10.0), "-1", t.dim, 8.5, true);
            for d in [3.0f32, 6.0] {
                let y = p.1 + d / 8.0 * p.3;
                s.line(Point::new(p.0, y), Point::new(p.0 + p.2, y), t.grid, 1.0);
                kit::text_right(s, p.0 - 4.0, y, &format!("-{d:.0}"), t.dim, 8.5);
            }
        }
    }
    let _ = cx;
}

fn paint_detail(s: &mut Scene, cx: &Ctx, page: &Smart, e: usize, dp: R, p: R, view: &[f32]) {
    let t = cx.t;
    match e {
        PURE => {
            let (strip, bar, join) = Smart::pure_parts(p);
            // 12 s of the slow gain, held spans in warm.
            let n = page.strip.len();
            if n >= 2 {
                let span = (page.rate.max(10.0) * STRIP_S).max(2.0);
                let pts: Vec<(Point, bool)> = page
                    .strip
                    .iter()
                    .enumerate()
                    .map(|(i, (g, held))| (Point::new(strip.0 + strip.2 * (1.0 - (n - 1 - i) as f32 / span), strip.1 + (-g / STRIP_DB).clamp(0.0, 1.0) * strip.3), *held))
                    .collect();
                s.clip_round(kit::rect(strip), 4.0);
                for w in pts.windows(2) {
                    if w[1].1 {
                        let x0 = w[0].0.x;
                        s.fill_round(Rect::new(x0, strip.1, (w[1].0.x - x0).max(1.0), strip.3), 0.0, t.warm.with_alpha(0.22), t.warm.with_alpha(0.22), true);
                    }
                }
                let line: Vec<Point> = pts.iter().map(|(p, _)| *p).collect();
                let top: Vec<Point> = line.iter().map(|p| Point::new(p.x, strip.1)).collect();
                band(s, &top, &line, t.text.with_alpha(0.18));
                kit::curve(s, line, t.text, 1.5);
                s.pop_clip();
            }
            let held = v(view, view::PAUSE_HOLD) >= 0.5;
            let lamp = Rect::new(strip.0 + strip.2 - 40.0, strip.1 + 4.0, 36.0, 13.0);
            if held {
                s.fill_round(lamp, 6.5, t.warm, t.warm, true);
            } else {
                s.stroke_round(lamp, 6.5, t.border, 1.0);
            }
            s.text(lamp.center(), "HELD", if held { t.on_accent } else { t.dim }, 8.0, true);
            s.text(Point::new(strip.0 + 4.0, strip.1 + 9.0), format!("slow {:.1} dB", v(view, view::SLOW_GAIN).min(0.0)), t.dim, 8.0, false);
            // post-hold bar
            let post = v(view, view::POST_HOLD_MS).clamp(0.0, 20.0);
            let max = cx.v(idx::BASS_HOLD_MAX);
            s.fill_round(Rect::new(bar.0 + 1.0, bar.1 + 1.0, ((bar.2 - 2.0) * post / 20.0).max(0.0), bar.3 - 2.0), 2.0, t.accent.with_alpha(0.6), t.accent.with_alpha(0.6), true);
            let mx = bar.0 + bar.2 * max / 20.0;
            s.line(Point::new(mx, bar.1 - 3.0), Point::new(mx, bar.1 + bar.3 + 3.0), t.text, 2.0);
            kit::text_right(s, bar.0 + bar.2, bar.1 - 10.0, &format!("{post:.1} / {max:.0} ms"), t.text, 8.0);
            // join shape: closeness squared against distance
            let w = cx.v(idx::JOIN_WIDTH).max(0.01);
            let jx = |d: f32| join.0 + join.2 * d / 12.0;
            let pts: Vec<Point> = (0..=60)
                .map(|k| {
                    let d = 12.0 * k as f32 / 60.0;
                    let u = (1.0 - d / w).clamp(0.0, 1.0);
                    Point::new(jx(d), join.1 + 4.0 + (join.3 - 8.0) * (1.0 - u * u))
                })
                .collect();
            kit::curve(s, pts, t.accent, 1.5);
            let gx = jx(cx.v(idx::JOIN_WIDTH));
            s.line(Point::new(gx, join.1 + 2.0), Point::new(gx, join.1 + join.3 - 2.0), t.text, 1.5);
            s.text(Point::new(join.0 + 4.0, join.1 - 8.0), format!("JOIN {:.1} dB", cx.v(idx::JOIN_WIDTH)), t.dim, 8.0, false);
        }
        SMOOTH | PRECISE => {
            let rel = cx.v(idx::RELEASE);
            let auto = cx.on(idx::AUTO_RELEASE);
            let (lo, hi) = (rel / 4.5, rel * 4.5);
            let y = |ms: f32| p.1 + p.3 * (1.0 - ((ms.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0));
            let x = |c: f32| p.0 + p.2 * ((c - 3.0) / 18.0).clamp(0.0, 1.0);
            for (m, l) in [(0.25f32, "x1/4"), (1.0, "x1"), (4.0, "x4")] {
                kit::text_right(s, p.0 - 4.0, y(rel * m), l, t.dim, 8.0);
            }
            s.clip_round(kit::rect(p), 4.0);
            let pts: Vec<Point> = (0..=72)
                .map(|k| {
                    let c = 3.0 + 18.0 * k as f32 / 72.0;
                    let f = if auto { limiter_smart::crest_release_factor(c as f64) as f32 } else { 1.0 };
                    Point::new(x(c), y(rel * f))
                })
                .collect();
            kit::curve(s, pts, t.accent, 2.0);
            if !view.is_empty() {
                let d = Point::new(x(page.crest), y(page.rel_ms.max(lo)));
                s.fill_ellipse_solid(Scene::ellipse_at(d, 4.0), t.text);
            }
            s.pop_clip();
            kit::text_right(s, dp.0 + dp.2 - 8.0, dp.1 + 12.0, &if auto { format!("release {} at crest {:.1} dB", ms_text(page.rel_ms.max(0.1)), page.crest) } else { "AUTO off: fixed release".into() }, t.text, 9.0);
        }
        PUNCH => {
            let c = cx.v(idx::CEILING) as f64;
            let (knee, hr, k) = (cx.v(idx::KNEE) as f64, cx.v(idx::CLIP_HEADROOM), cx.v(idx::CLIP_KNEE));
            s.clip_round(kit::rect(p), 4.0);
            // shaded knee region of the clipper
            let (ka, kb) = (Smart::knee_edge_db(k), 20.0 * (1.0 + k).log10());
            let (xa, xb) = (Smart::clip_x(p, ka), Smart::clip_x(p, kb.min(hr.max(0.0)).max(ka)));
            s.fill_round(Rect::new(xa, p.1, (xb - xa).max(1.0), p.3), 0.0, t.accent.with_alpha(0.10), t.accent.with_alpha(0.10), true);
            // hard corner
            kit::dashed(s, &[Point::new(Smart::clip_x(p, Smart::CLIP_LO), Smart::clip_y(p, Smart::CLIP_LO)), Point::new(Smart::clip_x(p, 0.0), Smart::clip_y(p, 0.0)), Point::new(p.0 + p.2, Smart::clip_y(p, 0.0))], 3.0, 3.0, t.dim, 1.0);
            let pts: Vec<Point> = (0..=130)
                .map(|i| {
                    let d = Smart::CLIP_LO + (Smart::CLIP_HI - Smart::CLIP_LO) * i as f32 / 130.0;
                    let o = limiter_smart::punch_static_db(c + d as f64, 0.0, c, knee, hr as f64, k as f64) - c;
                    Point::new(Smart::clip_x(p, d), Smart::clip_y(p, o as f32))
                })
                .collect();
            kit::curve(s, pts, t.accent, 2.0);
            let lx = Smart::clip_x(p, hr);
            s.line(Point::new(lx, p.1), Point::new(lx, p.1 + p.3), t.text, 1.5);
            s.text(Point::new((lx + 4.0).min(p.0 + p.2 - 60.0), p.1 + p.3 - 10.0), "limiter stops", t.text, 8.0, false);
            let kx = Smart::clip_x(p, ka);
            kit::dashed(s, &[Point::new(kx, p.1), Point::new(kx, p.1 + p.3)], 3.0, 2.0, t.text, 1.0);
            s.text(Point::new(kx - 2.0 - text::width("knee", 8.0), p.1 + 10.0), "knee", t.text, 8.0, false);
            // the deepest clip of the last block, on the clipper's curve
            let cg = v(view, view::CLIP_GAIN).min(0.0) as f64;
            if cg < -0.01 {
                let mut u = 1.0f64;
                for _ in 0..60 {
                    let gdb = 20.0 * (limiter_smart::clip_curve(u, k as f64) / u).log10();
                    if gdb <= cg {
                        break;
                    }
                    u *= 1.01;
                }
                let d = 20.0 * u.log10();
                let o = 20.0 * limiter_smart::clip_curve(u, k as f64).log10();
                s.fill_ellipse_solid(Scene::ellipse_at(Point::new(Smart::clip_x(p, d as f32), Smart::clip_y(p, o as f32)), 4.0), t.text);
            }
            s.pop_clip();
            kit::text_right(s, dp.0 + dp.2 - 8.0, dp.1 + 12.0, &format!("clip {:.1} dB · knee {k:.2}", v(view, view::CLIP_GAIN).min(0.0)), t.text, 9.0);
        }
        MULTI => {
            let sr = Smart::sr(cx) as f64;
            let sp = limiter_smart::ordered_splits([cx.v(idx::SPLIT1) as f64, cx.v(idx::SPLIT2) as f64, cx.v(idx::SPLIT3) as f64], sr);
            let edges = [20.0f32, sp[0] as f32, sp[1] as f32, sp[2] as f32, 20_000.0];
            let snap = page.ring.back().copied().unwrap_or_default();
            s.clip_round(kit::rect(p), 4.0);
            for b in 0..4 {
                let (a, z) = (Smart::band_x(p, edges[b]) + 2.0, Smart::band_x(p, edges[b + 1]) - 2.0);
                let z = if b == 3 { z - 10.0 } else { z };
                let d = (-snap.band[b] / 12.0).clamp(0.0, 1.0) * p.3;
                let c = t.bands[b];
                s.fill_round(Rect::new(a, p.1, (z - a).max(1.0), d.max(0.0)), 0.0, c.with_alpha(0.45), c.with_alpha(0.45), true);
                s.line(Point::new(a, p.1 + d), Point::new(z, p.1 + d), c, 1.5);
                let hd = (page.band_hold[b].0 / 12.0).clamp(0.0, 1.0) * p.3;
                if hd > 1.0 {
                    s.line(Point::new(a, p.1 + hd), Point::new(z, p.1 + hd), t.text, 1.0);
                }
            }
            // the broadband catcher, a thin bar at the right
            let cd = (-snap.catcher / 12.0).clamp(0.0, 1.0) * p.3;
            s.fill_round(Rect::new(p.0 + p.2 - 8.0, p.1, 5.0, cd.max(0.0)), 0.0, t.text.with_alpha(0.4), t.text.with_alpha(0.4), true);
            for (i, f) in sp.iter().enumerate() {
                let x = Smart::band_x(p, *f as f32);
                let movable = cx.more;
                s.line(Point::new(x, p.1), Point::new(x, p.1 + p.3), if movable { t.accent } else { t.grid_strong }, if movable { 2.0 } else { 1.0 });
                if movable {
                    s.fill_round(Rect::new(x - 4.0, p.1 + p.3 * 0.5 - 8.0 + 12.0 * i as f32, 8.0, 16.0), 3.0, t.accent, t.accent, true);
                }
            }
            if Smart::ms(cx) {
                let x = Smart::band_x(p, 20.0) + 4.0;
                s.text(Point::new(x, p.1 + p.3 - 8.0), "LINKED", t.dim, 8.0, false);
            }
            s.pop_clip();
            let label = sp.iter().map(|f| hz_text(*f as f32)).collect::<Vec<_>>().join(" · ");
            kit::text_right(s, dp.0 + dp.2 - 8.0, dp.1 + 12.0, &label, t.text, 9.0);
        }
        _ => {
            // BRIDGE
            let Some(o) = page.bridge.out.as_ref() else {
                s.text(kit::rect(p).center(), "measuring…", t.dim, 9.5, true);
                return;
            };
            let x = |la: f32| p.0 + p.2 * (la + 1.0) / 9.0;
            let y = |d: f32| p.1 + p.3 * (-d / 8.0).clamp(0.0, 1.0);
            s.clip_round(kit::rect(p), 4.0);
            for b in o.bursts {
                let (a, z) = (x(b), x(b + o.burst_len));
                s.fill_round(Rect::new(a, p.1, (z - a).max(1.0), p.3), 0.0, t.text.with_alpha(0.06), t.text.with_alpha(0.06), true);
            }
            let n = o.need.len().max(1) as f32;
            let at = |k: usize| -1.0 + 9.0 * k as f32 / n;
            let need: Vec<Point> = o.need.iter().enumerate().map(|(k, d)| Point::new(x(at(k)), y(*d))).collect();
            let gain: Vec<Point> = o.gain.iter().enumerate().map(|(k, d)| Point::new(x(at(k)), y(*d))).collect();
            kit::curve(s, need, t.dim, 1.0);
            let top: Vec<Point> = gain.iter().map(|q| Point::new(q.x, p.1)).collect();
            band(s, &top, &gain, t.accent.with_alpha(0.18));
            kit::curve(s, gain, t.accent, 2.0);
            // the 2.5 ms attack limit before the first burst
            let early = -2.5 / o.lookahead_ms.max(0.1);
            if early > -1.0 {
                let ex = x(early);
                kit::dashed(s, &[Point::new(ex, p.1), Point::new(ex, p.1 + p.3)], 3.0, 2.0, t.text, 1.0);
                s.text(Point::new(ex + 3.0, p.1 + p.3 - 8.0), "2.5 ms", t.text, 8.0, false);
            }
            s.pop_clip();
            // the lookahead bracket under the first burst
            let (a, z) = (x(-1.0), x(0.0));
            s.line(Point::new(a, p.1 + p.3 - 3.0), Point::new(z, p.1 + p.3 - 3.0), t.text, 1.0);
            s.text(Point::new(a + 3.0, p.1 + p.3 - 10.0), "L", t.text, 8.0, false);
            kit::text_right(s, dp.0 + dp.2 - 8.0, dp.1 + 12.0, &format!("L = {} · thin: needed", ms_text(o.lookahead_ms)), t.text, 9.0);
        }
    }
}

fn paint_more(s: &mut Scene, cx: &Ctx, e: usize, view: &[f32]) {
    let t = cx.t;
    let sr = Smart::sr(cx);
    let req = [4usize, 8, 16][cx.choice(idx::OVERSAMPLING).min(2)];
    let eff = limiter_smart::effective_oversampling(req, sr as f64);
    let line = if eff < req { format!("{req}x asked, {eff}x at {:.0} kHz", sr / 1000.0) } else { format!("now {eff}x · {:.0} kHz internal", eff as f32 * sr / 1000.0) };
    let _ = view;
    s.text(Point::new(120.0, 704.0), line, t.text, 10.0, false);
    if matches!(e, SMOOTH | MULTI | GLIDE) {
        s.text(Point::new(280.0, 704.0), "not used by this engine", t.dim, 9.0, false);
    }
    match e {
        MULTI => {
            let raw = [cx.v(idx::SPLIT1), cx.v(idx::SPLIT2), cx.v(idx::SPLIT3)];
            let sp = limiter_smart::ordered_splits(raw.map(f64::from), sr as f64);
            if let Some(i) = (0..3).find(|&i| (sp[i] as f32 - raw[i]).abs() > 0.5) {
                s.text(Point::new(528.0, 704.0), format!("kept 1.25x apart: split {} is {}", i + 1, hz_text(sp[i] as f32)), t.dim, 9.0, false);
            }
            frame::page_pill(s, t, RESET_SPLITS, "RESET SPLITS", false, false);
            s.text(Point::new(640.0, 753.0), "drag the split lines in BANDS", t.dim, 8.5, false);
        }
        PURE => {
            let crest = Smart::crest_mode(cx);
            let note = if crest { (768.0, "used by Peak Body") } else { (864.0, "used by Loudness Crest") };
            s.text(Point::new(note.0 + 40.0, 738.0), note.1, t.dim, 8.0, true);
        }
        _ => {}
    }
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    Box::new(Smart::new())
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). Delta passes what the
/// limiter removed, not a limited signal.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (false, values.get(idx::DELTA).is_some_and(|&d| d >= 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probes_and_the_bridge_run_for_every_engine() {
        let mut values: Vec<f32> = SmartLimiter::default().params().iter().map(|p| p.default).collect();
        values[idx::CEILING] = -1.0;
        let t0 = Instant::now();
        let o = bridge_measure(values.clone(), 48_000.0);
        eprintln!("bridge: {:?}", t0.elapsed());
        assert_eq!(o.need.len(), BRIDGE_BINS);
        let deepest = o.gain.iter().fold(0.0f32, |a, &b| a.min(b));
        assert!(deepest < -4.0, "the bursts are pulled down: {deepest}");
        let needed = o.need.iter().fold(0.0f32, |a, &b| a.min(b));
        assert!((needed + 6.0).abs() < 0.01, "{needed}");
    }

    /// Writes `build/smart/ui-smart-<engine>-<theme>-<mode>.ppm` with live read-out from a
    /// real Smart Limiter and the probe's curves, for every engine, STANDARD, TALL and MORE,
    /// in all four themes. Run with `--ignored`.
    #[test]
    #[ignore]
    fn dump_smart_pages() {
        use crate::ui::tests::{editor, feed};
        use crate::ui::theme::ThemeId;
        use crate::ui::LAYOUT;
        use openplugin::core::Editor;
        let dir = format!("{}/build/smart", env!("CARGO_MANIFEST_DIR"));
        std::fs::create_dir_all(&dir).expect("dir");
        let slot = LAYOUT.slots.iter().position(|s| s.key == "smart").expect("slot");
        let info = &LAYOUT.slots[slot];
        let sr = 48_000.0f32;
        // A loop with a bass line, kicks, snares and a little top.
        let signal = |i: usize| -> (f32, f32) {
            let t = i as f32 / sr;
            let beat = t % 0.5;
            let off = (t + 0.25) % 0.5;
            let noise = |k: usize| ((k.wrapping_mul(2654435761) >> 7) & 0xffff) as f32 / 32768.0 - 1.0;
            let kick = 0.85 * (-beat / 0.05).exp() * (std::f32::consts::TAU * (50.0 + 80.0 * (-beat / 0.02).exp()) * beat).sin();
            let snare = 0.55 * (-off / 0.07).exp() * noise(i);
            let bass = 0.28 * (std::f32::consts::TAU * 55.0 * t).sin();
            let top = 0.05 * noise(i * 7 + 3) + 0.08 * (std::f32::consts::TAU * 3000.0 * t).sin();
            (kick + snare * 0.9 + bass + top, kick + snare + bass * 0.9 - top)
        };
        for id in ThemeId::ALL {
            let name = id.name();
            let (mut e, shared) = editor(id);
            feed(&shared, 4000);
            e.param_changed(info.on, 1.0);
            e.open_page(slot);
            let mut m = SmartLimiter::default();
            let sets = [(idx::DRIVE, 6.0f32), (idx::CEILING, -1.0), (idx::GAIN_MATCH, if id == ThemeId::Paper { 1.0 } else { 0.0 })];
            for (l, x) in sets {
                m.set_param(l, x);
                e.param_changed(info.base + l, x);
            }
            m.prepare(sr, 800);
            let mut pos = 0usize;
            let mut run = |e: &mut crate::ui::OpenLimiterEditor, m: &mut SmartLimiter, ticks: usize| {
                let mut l = vec![0.0f32; 800];
                let mut r = vec![0.0f32; 800];
                let mut vbuf = vec![0.0f32; view::LEN];
                for _ in 0..ticks {
                    for k in 0..800 {
                        (l[k], r[k]) = signal(pos + k);
                    }
                    pos += 800;
                    m.process(&mut l, &mut r);
                    m.view(&mut vbuf);
                    shared.set_view(slot, &vbuf);
                    shared.raise_slot_gr(slot, m.meters().reduction_db);
                    shared.set_slot_latency(slot, m.latency());
                    e.tick(1.0 / 60.0);
                }
            };
            for engine in [SMOOTH, PURE, PRECISE, PUNCH, MULTI, GLIDE] {
                m.set_param(idx::ENGINE, engine as f32);
                e.param_changed(info.base + idx::ENGINE, engine as f32);
                for (tall, more) in [(false, false), (true, false), (false, true)] {
                    e.set_tall(tall);
                    e.ui.more = more;
                    e.rebuild();
                    run(&mut e, &mut m, if (tall, more) == (false, false) { 200 } else { 20 });
                    let f = e.render(2.0);
                    let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                    for px in f.pixels {
                        out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
                    }
                    let mode = if more { "more" } else if tall { "tall" } else { "std" };
                    std::fs::write(format!("{dir}/ui-smart-{}-{name}-{mode}.ppm", KEYS[engine].to_lowercase()), out).expect("write");
                }
                e.ui.more = false;
            }
            // Pure / Loudness Crest in Mid-Side with a knee, 16x, delta on: the MORE dot,
            // the M/S trace, the delta warnings.
            for (l, x) in [(idx::ENGINE, 0.0f32), (idx::CHANNEL_MODE, 1.0), (idx::SLOW_MODE, 1.0), (idx::KNEE, 3.0), (idx::OVERSAMPLING, 2.0), (idx::DELTA, 1.0)] {
                m.set_param(l, x);
                e.param_changed(info.base + l, x);
            }
            for (tall, more) in [(false, false), (false, true)] {
                e.set_tall(tall);
                e.ui.more = more;
                e.rebuild();
                run(&mut e, &mut m, 120);
                let f = e.render(2.0);
                let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                for px in f.pixels {
                    out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
                }
                std::fs::write(format!("{dir}/ui-smart-alt-{name}-{}.ppm", if more { "more" } else { "std" }), out).expect("write");
            }
        }
    }
}
