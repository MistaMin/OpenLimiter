//! The editor, laid out as `design/shell.md` describes.
//!
//! * Left rail: input meters, the input gain faders and their link, the key lamp.
//! * Right rail: output meters (after the dither), the chain's reduction, true peak, the
//!   dither in a few words, the latency.
//! * Top band: the history plot and the loudness radar, or a strip of both while a
//!   graph-first module is open.
//! * The rack: the modules in the chain, in order, then the dither. Click to open, the dot
//!   to switch, drag to reorder, "+" to add, right-click for more.
//! * The module page: a header the shell draws, and the module's own [`page::Page`].
//! * The loudness bar and the setup sheet.
//!
//! Layers: what never moves is painted once into the canvas's cached layer; what follows
//! the parameters is painted where it changed; meters, the band and the loudness numbers
//! are rasterised straight into the frame on every tick that moves them ([`live`]).

pub mod analyser;
mod chrome;
mod detached;
#[cfg(target_os = "macos")]
mod popout;
pub(crate) mod pops;
#[cfg(target_os = "windows")]
mod popwin;
#[cfg(target_os = "linux")]
mod popx11;
mod spectro;
pub mod kit;
mod live;
pub mod page;
pub mod pages;
pub mod raster;
pub mod theme;

use std::collections::VecDeque;
use std::sync::Arc;

use openplugin::core::editor::{AccessAction, AccessNode, AccessRange, DamageRect, DamageTracker, Key, Modifiers, ParamMenu, ParamMenuAction, SizeRange, TextEdit, Tooltip};
use openplugin::core::{Editor, EditorHost, EditorState, Frame};
use goodlookindri_shell::text::GlyphCache;
use openplugin::dri::{Control, ControlSet, SceneCanvas};

use crate::shared::{Entry, Loud, Shared, HIST, LOUD_FLOOR, LOUD_HIST};
use crate::{INFO, LAYOUT, PARAMS, PRESETS};
use kit::{inside, Cell, Kind, Popover};
use page::{Audio, Ctx, Io, Live, Merge, Page, Pointer, Response, Zones, AUDIO_LEN};
use raster::{Target, R};
use theme::{load_target, load_theme, store_target, store_theme, theme, ThemeId};

pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 860;

// ---- the first limiter's parameters the shell knows by index ----

pub(crate) const P_GAIN_L: usize = 0;
pub(crate) const P_GAIN_R: usize = 1;
pub(crate) const P_THRESHOLD: usize = 2;
pub(crate) const P_SOFT_CLIP: usize = 3;
pub(crate) const P_OUTPUT: usize = 7;
pub(crate) const P_DELAY_L: usize = 8;
pub(crate) const P_DELAY_R: usize = 9;
pub(crate) const P_FINE_L: usize = 10;
pub(crate) const P_FINE_R: usize = 11;
pub(crate) const P_PHASE_L: usize = 12;
pub(crate) const P_PHASE_R: usize = 13;
pub(crate) const P_UNIT: usize = 14;
pub(crate) const P_LINK: usize = 16;
pub(crate) const P_HEADROOM: usize = 17;
pub(crate) const P_DITHER: usize = 18;
/// How many of the first limiter's parameters a factory preset holds.
const PRESET_PARAMS: usize = 19;
/// On the setup sheet, not on the limiter's page.
pub(crate) const SETUP_PARAMS: [usize; 8] = [P_DELAY_L, P_DELAY_R, P_FINE_L, P_FINE_R, P_PHASE_L, P_PHASE_R, P_UNIT, P_HEADROOM];

// ---- window geometry (design/shell.md, section 0) ----

const PRESET_PREV: R = (340.0, 10.0, 28.0, 28.0);
const PRESET_BOX: R = (372.0, 10.0, 280.0, 28.0);
const PRESET_NEXT: R = (656.0, 10.0, 28.0, 28.0);
const AB: R = (692.0, 10.0, 64.0, 28.0);
const SETUP_BUTTON: R = (980.0, 10.0, 96.0, 28.0);
/// Listening aids for the whole plugin, and the gain Autogain applies.
const AUTOGAIN_PILL: R = (772.0, 10.0, 92.0, 28.0);
const AUTOGAIN_VALUE: R = (866.0, 10.0, 44.0, 28.0);
const DELTA_PILL: R = (912.0, 10.0, 60.0, 28.0);
const THEME_BUTTON: R = (1084.0, 10.0, 100.0, 28.0);

const LEFT_RAIL: R = (16.0, 56.0, 80.0, 788.0);
const RIGHT_RAIL: R = (1104.0, 56.0, 80.0, 788.0);
const IN_PPM: [R; 2] = [(28.0, 84.0, 14.0, 420.0), (44.0, 84.0, 14.0, 420.0)];
const IN_PEAK: R = (22.0, 512.0, 68.0, 20.0);
const KEY_LAMP: R = (22.0, 536.0, 68.0, 16.0);
const GAIN_FADER: [R; 2] = [(30.0, 572.0, 20.0, 200.0), (62.0, 572.0, 20.0, 200.0)];
const GAIN_VALUE: [R; 2] = [(20.0, 778.0, 36.0, 18.0), (58.0, 778.0, 36.0, 18.0)];
const GAIN_LINK: R = (22.0, 804.0, 68.0, 24.0);
/// 12 wide (the design has 14) so the scale left of them stays inside the rail.
const OUT_PPM: [R; 2] = [(1122.0, 84.0, 12.0, 420.0), (1136.0, 84.0, 12.0, 420.0)];
const CHAIN_GR: R = (1154.0, 84.0, 14.0, 420.0);
const OUT_PEAK: R = (1110.0, 512.0, 68.0, 20.0);
const TP_BOX: R = (1110.0, 548.0, 68.0, 40.0);
/// The output's RMS over the chosen window, under the true peak.
const RMS_BOX: R = (1110.0, 604.0, 68.0, 40.0);
const DITHER_BOX: R = (1110.0, 660.0, 68.0, 56.0);
const LATENCY_BOX: R = (1110.0, 732.0, 68.0, 44.0);
/// What the rails repaint on every tick that moves a meter.
const LEFT_LIVE: R = (22.0, 80.0, 68.0, 474.0);
const RIGHT_LIVE: R = (1108.0, 80.0, 72.0, 510.0);

const BAND_FULL: R = (104.0, 56.0, 992.0, 244.0);
const HIST_PANEL: R = (104.0, 56.0, 676.0, 244.0);
const PLOT: R = (138.0, 84.0, 608.0, 188.0);
const RADAR_PANEL: R = (788.0, 56.0, 308.0, 244.0);
const RADAR_C: (f32, f32) = (942.0, 188.0);
const RADAR_R: f32 = 98.0;
const GR_CHIP: R = (650.0, 62.0, 96.0, 18.0);
const BAND_STRIP: R = (104.0, 56.0, 992.0, 72.0);
const RIBBON: R = (112.0, 62.0, 640.0, 60.0);
const LOUD_RIBBON: R = (764.0, 62.0, 296.0, 60.0);
const CHEVRON: R = (1068.0, 60.0, 22.0, 18.0);
/// Open the history or the radar in a window of its own (macOS).
const HIST_POP: R = (752.0, 61.0, 22.0, 18.0);
const RADAR_POP: R = (1042.0, 60.0, 22.0, 18.0);
const CAN_POP: bool = pops::SUPPORTED;
/// LEVELS | 2D | 3D in the history's header.
const HIST_MODE: R = (476.0, 61.0, 150.0, 18.0);

const RACK_FULL: R = (104.0, 308.0, 992.0, 52.0);
const RACK_STRIP: R = (104.0, 136.0, 992.0, 52.0);

const LOUD_BAR: R = (104.0, 788.0, 992.0, 56.0);
const LOUD_CELL_X: f32 = 112.0;
const LOUD_CELL_PITCH: f32 = 132.0;
const TARGET_CHIP: R = (912.0, 798.0, 84.0, 36.0);
const RESET_CHIP: R = (1004.0, 798.0, 84.0, 36.0);

const SETUP_SHEET: R = (104.0, 56.0, 992.0, 304.0);
const SETUP_DONE: R = (1016.0, 66.0, 64.0, 24.0);
/// The setup sheet's top-band choice.
const SETUP_BAND: R = (560.0, 218.0, 240.0, 24.0);
const SETUP_THEME: R = (560.0, 274.0, 264.0, 24.0);
/// RMS meter: AES (+3 dB, a full-scale sine reads 0) or plain RMS; and its window.
const SETUP_RMS_SCALE: R = (560.0, 318.0, 120.0, 24.0);
const SETUP_RMS_WINDOW: R = (688.0, 318.0, 144.0, 24.0);
/// RMS windows the setup sheet offers, seconds.
pub(crate) const RMS_WINDOWS: [f32; 6] = [0.3, 1.0, 2.0, 3.0, 5.0, 10.0];

/// Where popovers may open: the centre column above the loudness bar.
const POP_BOUNDS: R = (104.0, 56.0, 992.0, 724.0);

/// Loudness targets the bar and the radar measure against.
pub(crate) const TARGETS: [f32; 6] = [-23.0, -18.0, -16.0, -14.0, -11.0, -9.0];
/// The radar: one turn per this many loudness entries.
const RADAR_SLOTS: usize = 600;
const RADAR_BELOW: f32 = 36.0;
const RADAR_SPAN: f32 = 42.0;

/// The history window, seconds and entries.
const WINDOW_S: f32 = 4.0;
const WINDOW: usize = (WINDOW_S * crate::shared::HIST_RATE) as usize;
const PLOT_TOP_DB: f32 = 6.0;
const PLOT_SPAN_DB: f32 = 42.0;
const METER_TOP_DB: f32 = 6.0;
const METER_SPAN_DB: f32 = 60.0;
const METER_SEGMENTS: usize = 60;
const GR_METER_SPAN_DB: f32 = 25.0;
const FLOOR_DB: f32 = -90.0;
/// History entries kept for the RMS meter: the longest window.
const RMS_HIST: usize = (10.0 * crate::shared::HIST_RATE) as usize;
/// How often the RMS and loudness numbers change on screen: slow enough to read (owner's
/// choice, 2026-11-23). The meters and the radar stay live.
const READOUT_PERIOD: f32 = 0.25;
/// Ticks of module reduction the pages get, about four seconds at 60 ticks a second.
const GR_HIST: usize = 256;

/// One rectangle around all of `rects` (none for none). The canvas replays the whole moving
/// scene once per rectangle and walks every shape's full box each time, so it gets this one;
/// the host still gets the small ones, and the pixels between them repaint unchanged.
fn bounds(rects: &[DamageRect]) -> Vec<DamageRect> {
    let (x0, y0, x1, y1) = rects.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |m, r| (m.0.min(r.x), m.1.min(r.y), m.2.max(r.x + r.w), m.3.max(r.y + r.h)));
    if x1 > x0 && y1 > y0 { vec![DamageRect::new(x0, y0, x1 - x0, y1 - y0)] } else { Vec::new() }
}

fn dmg(r: R) -> DamageRect {
    DamageRect::new(r.0, r.1, r.2, r.3)
}

fn db(linear: f32) -> f32 {
    20.0 * linear.max(1e-6).log10()
}

/// How a module goes by in the rack, and in one line.
pub(crate) fn module_names(key: &str) -> (&'static str, &'static str) {
    match key {
        "" => ("CLASSIC", "Look-ahead limiter with a two-stage release"),
        "shaper" => ("PUNCH", "Lift, punch and saturation"),
        "modes" => ("LOUD MAX", "Maximizer with ten styles and three types"),
        "eq" => ("SIMPLEQ", "Six bands, low cut, L/R or M/S"),
        "sone" => ("DUALSONE", "Leveller plus transient engine"),
        "clear" => ("CLEAN PEAK", "Transparent peak limiter"),
        "dual" => ("COMP/DE-ESS", "Wide or one band"),
        "wall" => ("FINAL WALL", "Wall limiter with input clipper"),
        "bands" => ("MULTIBAND", "Five bands with limiters"),
        "peq" => ("PRECISIONEQ", "32-band EQ, any phase mode"),
        "deq" => ("DYNEQ", "32-band dynamic EQ with key"),
        "sup" => ("SUPPRESS", "Tames ringing resonances"),
        "hcomp" => ("HARM COMP", "7 styles"),
        "decomp" => ("RESTORE", "Restores lost peaks"),
        "deexp" => ("FLOOR LIFT", "Lifts quiet parts and tails"),
        "harm" => ("OVERTONES", "2nd–5th, alias-free"),
        "smart" => ("SMART", "Six switchable limiter engines"),
        _ => ("MODULE", ""),
    }
}

/// The group a module is listed under when adding one.
fn module_group(key: &str) -> &'static str {
    match key {
        "" | "clear" | "modes" | "wall" | "smart" => "LIMITERS",
        "bands" | "dual" | "sone" | "hcomp" => "COMPRESSORS",
        "decomp" | "deexp" => "EXPANDERS",
        "eq" | "peq" | "deq" | "sup" => "EQ / SPECTRAL",
        _ => "COLOUR",
    }
}

/// The page's title: a slot's label, or "Dither".
pub(crate) fn slot_label(slot: Option<usize>) -> &'static str {
    slot.and_then(|s| LAYOUT.slots.get(s)).map_or("Requantise", |s| s.label)
}

/// What the page shows: a slot of the chain, or (one past the last slot) the dither.
pub(crate) fn dither_page() -> usize {
    LAYOUT.slots.len()
}

fn level_text(level_db: f32) -> String {
    if level_db <= FLOOR_DB { "-inf".into() } else { format!("{level_db:+.1}") }
}

/// A level that rises at once and falls at a steady rate, with a held peak.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Meter {
    pub level: f32,
    pub hold: f32,
    hold_age: f32,
}

impl Meter {
    pub const fn new(rest: f32) -> Self {
        Self { level: rest, hold: rest, hold_age: 0.0 }
    }

    pub fn feed(&mut self, value: f32, rest: f32, fall_per_s: f32, dt: f32) {
        self.level = value.max(self.level - fall_per_s * dt).max(rest);
        if self.level >= self.hold {
            self.hold = self.level;
            self.hold_age = 0.0;
        } else {
            self.hold_age += dt;
            if self.hold_age > 1.5 {
                self.hold = (self.hold - 2.0 * fall_per_s * dt).max(self.level);
            }
        }
    }

    fn moved(&self, other: &Meter) -> bool {
        (self.level - other.level).abs() > 0.01 || (self.hold - other.hold).abs() > 0.01
    }
}

/// How the top band behaves when a page opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandPref {
    /// A strip for graph-first modules, full otherwise.
    Auto,
    Full,
    Strip,
}

/// What an open list is for.
#[derive(Clone, Debug, PartialEq)]
enum PopFor {
    /// A dropdown cell's parameter.
    Param,
    /// The factory presets.
    Preset,
    /// Modules to add, by slot.
    Add(Vec<usize>),
    /// A rack chip's menu, for a slot.
    Chip(usize),
    /// The page header's "..." menu: the shell's items, then the page's.
    PageMenu,
}

struct Pop {
    list: Popover,
    what: PopFor,
}

/// A module being dragged along the rack.
#[derive(Clone, Copy, Debug)]
struct Drag {
    slot: usize,
    /// Pointer when it went down, and now.
    from: (f32, f32),
    at: (f32, f32),
    moved: bool,
}

/// Where the rack's parts are, for the modules it shows.
struct RackGeom {
    rect: R,
    input: R,
    output: R,
    dither: R,
    /// One rect per shown slot, in processing order: `(slot, rect)`.
    chips: Vec<(usize, R)>,
    /// The "+" button, when a module can still be added.
    add: Option<R>,
}

const CHIP_H: f32 = 40.0;

impl RackGeom {
    fn new(rect: R, shown_in_order: &[usize], any_hidden: bool) -> Self {
        let y = rect.1 + 6.0;
        let input = (rect.0 + 8.0, y, 24.0, CHIP_H);
        let output = (rect.0 + rect.2 - 32.0, y, 24.0, CHIP_H);
        let dither = (output.0 - 6.0 - 72.0, y, 72.0, CHIP_H);
        let start = input.0 + input.2 + 6.0;
        let end = dither.0 - 6.0 - if any_hidden { 38.0 } else { 0.0 };
        let n = shown_in_order.len();
        let w = if n == 0 { 0.0 } else { ((end - start - 6.0 * (n as f32 - 1.0)) / n as f32).clamp(30.0, 120.0) };
        let chips: Vec<(usize, R)> = shown_in_order.iter().enumerate().map(|(i, &s)| (s, (start + i as f32 * (w + 6.0), y, w, CHIP_H))).collect();
        let add = any_hidden.then(|| (chips.last().map_or(start, |(_, r)| r.0 + r.2 + 6.0), y, 32.0, CHIP_H));
        Self { rect, input, output, dither, chips, add }
    }

    /// The place among the shown chips a chip dropped at `x` takes.
    fn place_at(&self, x: f32) -> usize {
        self.chips.iter().filter(|(_, r)| x > r.0 + r.2 * 0.5).count()
    }
}

/// The power dot's hit area inside a chip.
fn chip_dot(r: R) -> R {
    (r.0 + 2.0, r.1 + 20.0, 22.0, 18.0)
}

pub(crate) struct Ui {
    pub controls: ControlSet,
    /// Each control's value when the editor last agreed with it: a control whose value
    /// moved away from it was moved by the user.
    synced: Vec<f32>,
    pub cells: Vec<Cell>,
    /// The first `shell_cells` cells are the shell's; the rest are the page's.
    shell_cells: usize,
    pub theme: ThemeId,
    keys_active: bool,
    pub values: Vec<f32>,
    /// A slot index, or `dither_page()`.
    pub selected: usize,
    pub page: Box<dyn Page>,
    /// The MORE face is showing.
    more: bool,
    /// The top band is a strip and the page tall.
    tall: bool,
    band_pref: BandPref,
    setup_open: bool,
    pop: Option<Pop>,
    drag: Option<Drag>,
    /// The page took a pointer press and gets the moves until it is let go.
    page_grab: bool,
    /// Bumped whenever the page's static picture may change.
    page_rev: u32,

    // Band and meters.
    history: VecDeque<Entry>,
    /// Entries ever pushed to `history`: the plot's columns are anchored to it.
    hist_count: usize,
    quiet: usize,
    input: [Meter; 2],
    output: [Meter; 2],
    reduction: Meter,
    latency: (u32, f32),
    /// The gain Autogain applies, dB, refreshed with the other readouts.
    pub(crate) autogain_db: f32,
    clip_age: f32,
    target: usize,
    loud_hist: VecDeque<(f32, f32)>,
    loud_count: usize,
    loud_quiet: usize,
    loud: Loud,
    key: (bool, Meter),
    /// Output mean square per history entry (L, R), newest last, up to the longest window.
    rms_ms: VecDeque<[f32; 2]>,
    /// AES scale (+3.01 dB, a full-scale sine reads 0 dB) or plain RMS.
    rms_aes: bool,
    /// Index into `RMS_WINDOWS`.
    rms_window: usize,
    /// The RMS shown, dBFS (the louder channel); `None` while silent. Refreshed with `readout`.
    rms: Option<f32>,
    /// The loudness numbers shown in the bar, taken from `loud` every `READOUT_PERIOD` so
    /// they can be read; the radar and the true peak use `loud` itself.
    readout: Loud,
    /// Seconds since `readout` and `rms` were last taken.
    readout_age: f32,

    // Per slot.
    slot_gr: Vec<Meter>,
    /// Per slot: gain the module added (upward modules), for the chip lane.
    slot_lift: Vec<Meter>,
    slot_latency: Vec<u32>,
    gr_hist: VecDeque<f32>,
    hist_rate: f32,
    view: Vec<f32>,
    view_len: usize,
    audio: Audio,
    live_gr: f32,

    /// The input gains and the first limiter's gains as last seen, for their links.
    in_gains: [f32; 2],
    gains: [f32; 2],
    /// A and B: the parameter values of each, and which one is live.
    ab: [Option<Vec<f32>>; 2],
    ab_on: usize,
    /// The history shows levels (0), the output spectrum as a spectrogram (1) or a waterfall (2).
    hist_mode: u8,
    spec: spectro::SpecHist,
    /// The history and the radar are open in windows of their own.
    popped: [bool; 2],
}

impl Ui {
    /// The value of a parameter: the control being held is the newest word on it.
    pub fn value(&self, param: usize) -> f32 {
        let held = self.controls.pressed().and_then(|id| self.controls.get(id)).filter(|c| c.param == param);
        held.map_or_else(|| self.values.get(param).copied().unwrap_or(0.0), Control::value)
    }

    fn on(&self, param: usize) -> bool {
        self.value(param) >= 0.5
    }

    fn first_on(&self) -> bool {
        self.on(LAYOUT.slots[0].on)
    }

    /// Slot indices in processing order.
    fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..LAYOUT.slots.len()).collect();
        order.sort_by(|&a, &b| self.value(LAYOUT.slots[a].position).total_cmp(&self.value(LAYOUT.slots[b].position)).then(a.cmp(&b)));
        order
    }

    /// The rack shows a slot: it is on, or the user put it there.
    fn shown(&self, slot: usize) -> bool {
        let mask = self.value(LAYOUT.rack_shown).max(0.0) as u32;
        mask & (1 << slot) != 0 || self.on(LAYOUT.slots[slot].on)
    }

    fn rack(&self) -> RackGeom {
        let order: Vec<usize> = self.order().into_iter().filter(|&s| self.shown(s)).collect();
        let hidden = order.len() < LAYOUT.slots.len();
        RackGeom::new(if self.tall { RACK_STRIP } else { RACK_FULL }, &order, hidden)
    }

    fn zones(&self) -> Zones {
        Zones::new(self.tall)
    }

    fn slot(&self) -> Option<usize> {
        (self.selected < LAYOUT.slots.len()).then_some(self.selected)
    }

    /// The first parameter and the count of the module shown.
    fn module_range(&self) -> (usize, usize) {
        match self.slot() {
            Some(s) => (LAYOUT.slots[s].base, LAYOUT.slots[s].count),
            None => (LAYOUT.dither_base, LAYOUT.dither_count),
        }
    }

    fn ctx<'a>(&self, values: &'a [f32]) -> Ctx<'a> {
        let (base, count) = self.module_range();
        Ctx {
            t: theme(self.theme),
            values,
            slot: self.slot(),
            base,
            count,
            zones: self.zones(),
            more: self.more,
            sample_rate: self.latency.1,
            key_present: self.key.0,
        }
    }

    fn live(&self) -> Live<'_> {
        Live {
            view: &self.view[..self.view_len],
            gr: self.live_gr,
            gr_hist: &self.gr_hist,
            hist_rate: self.hist_rate,
            audio: self.page.wants_audio().then_some(&self.audio),
            key: (self.key.0, self.key.1.level),
            latency: self.slot().map_or(0, |s| self.slot_latency.get(s).copied().unwrap_or(0)),
            sample_rate: self.latency.1,
            dt: 1.0 / self.hist_rate.max(1.0),
        }
    }

    /// The output RMS over the chosen window, dBFS, on the chosen scale: the louder channel.
    fn rms_now(&self) -> Option<f32> {
        let n = ((RMS_WINDOWS[self.rms_window.min(RMS_WINDOWS.len() - 1)] * crate::shared::HIST_RATE).round() as usize).clamp(1, RMS_HIST);
        let take = n.min(self.rms_ms.len());
        if take == 0 {
            return None;
        }
        let mut sum = [0.0f64; 2];
        for e in self.rms_ms.iter().rev().take(take) {
            sum[0] += f64::from(e[0]);
            sum[1] += f64::from(e[1]);
        }
        let ms = sum[0].max(sum[1]) / take as f64;
        if ms <= 1e-12 {
            return None;
        }
        let db = 10.0 * ms.log10() as f32;
        Some(if self.rms_aes { db + 3.0103 } else { db })
    }

    fn clipping(&self) -> bool {
        self.clip_age < 0.3
    }

    fn target(&self) -> f32 {
        TARGETS[self.target.min(TARGETS.len() - 1)]
    }

    /// A setup-sheet control is away from its default.
    fn setup_in_use(&self) -> bool {
        SETUP_PARAMS.iter().filter(|&&p| p != P_UNIT).any(|&p| (self.value(p) - PARAMS[p].clamp(PARAMS[p].default)).abs() > 1e-4)
    }

    /// The factory preset whose values are the current ones.
    fn preset(&self) -> Option<usize> {
        PRESETS.iter().position(|(_, values)| (0..PRESET_PARAMS).all(|i| (self.value(i) - PARAMS[i].clamp(values[i])).abs() < 1e-3))
    }

    /// What the static layer depends on.
    fn static_key(&self) -> u64 {
        self.theme as u64
            | (self.setup_open as u64) << 2
            | (self.target as u64) << 3
            | (self.selected as u64) << 6
            | (self.tall as u64) << 12
            | (self.more as u64) << 13
            | (self.band_pref as u64) << 14
            | (self.page_rev as u64) << 16
            | (self.hist_mode as u64) << 48
            | (self.popped[0] as u64) << 50
            | (self.popped[1] as u64) << 51
    }

    fn page_cells(&self) -> &[Cell] {
        &self.cells[self.shell_cells..]
    }

    /// The word length stage in a few words, one per line.
    fn dither_lines(&self) -> [String; 3] {
        let d = LAYOUT.dither_base;
        let text = |i: usize| INFO.params[d + i].format(self.value(d + i));
        if self.value(d) < 0.5 {
            return ["Off".into(), String::new(), String::new()];
        }
        let shaping = if self.value(d + 2) < 0.5 { "Flat".to_string() } else { text(2) };
        [text(0), text(1), shaping]
    }
}

/// The shell's own controls: input gains, their link, the limiter's threshold grip, the
/// page's ON and KEY switches, and the setup sheet's.
fn shell_cells(ui: &Ui) -> Vec<Cell> {
    let mut v = Vec::new();
    for c in 0..2 {
        v.push(Cell::new(Kind::FaderV, LAYOUT.input_gain[c], if c == 0 { "L" } else { "R" }, GAIN_FADER[c]).tip(format!("Input gain {}, ahead of the whole chain", if c == 0 { "left" } else { "right" })));
        v.push(Cell::value(LAYOUT.input_gain[c], "", GAIN_VALUE[c]).format(|v| format!("{v:+.1}")));
    }
    v.push(Cell::pill(LAYOUT.input_link, "LINK", GAIN_LINK).tip("Link the input gains"));
    v.push(Cell::pill(LAYOUT.autogain, "AUTOGAIN", AUTOGAIN_PILL).tip("Listen at the input's loudness. Meters still show the real output. Switch off before printing"));
    v.push(Cell::pill(LAYOUT.delta, "DELTA", DELTA_PILL).tip("Listen to what the plugin changes: output minus input. Switch off before printing"));
    let z = ui.zones();
    if let Some(slot) = ui.slot() {
        v.push(Cell::pill(LAYOUT.slots[slot].on, "ON", (120.0, z.header.1 + 9.0, 44.0, 22.0)).tip("Switch the module on or off"));
    }
    if ui.setup_open {
        let x0 = 136.0;
        v.push(Cell::new(Kind::FaderH, P_DELAY_L, "DELAY L", (x0, 116.0, 380.0, 26.0)));
        v.push(Cell::new(Kind::FaderH, P_DELAY_R, "DELAY R", (x0, 148.0, 380.0, 26.0)));
        v.push(Cell::new(Kind::FaderH, P_FINE_L, "FINE L", (x0, 180.0, 380.0, 26.0)));
        v.push(Cell::new(Kind::FaderH, P_FINE_R, "FINE R", (x0, 212.0, 380.0, 26.0)));
        v.push(Cell::segmented(P_UNIT, "UNIT", (x0 + 70.0, 248.0, 310.0, 24.0)).keys(&["ms", "24 fps", "25 fps", "30 fps"]));
        v.push(Cell::pill(P_PHASE_L, "PHASE L", (x0, 286.0, 120.0, 22.0)));
        v.push(Cell::pill(P_PHASE_R, "PHASE R", (x0 + 130.0, 286.0, 120.0, 22.0)));
        v.push(Cell::segmented(P_HEADROOM, "HEADROOM", (x0 + 70.0, 320.0, 310.0, 24.0)));
    } else if !ui.tall && ui.hist_mode == 0 && ui.first_on() {
        // The threshold is a grip on the plot while the first limiter is in the chain.
        let top = PLOT.1 + live::plot_y(PARAMS[P_THRESHOLD].max, PLOT);
        let bottom = PLOT.1 + live::plot_y(PARAMS[P_THRESHOLD].min, PLOT);
        v.push(Cell::grip(P_THRESHOLD, (PLOT.0 + PLOT.2 - 96.0, top, 96.0, bottom - top)).tip("Threshold of the first limiter: drag the line"));
    }
    v
}

pub struct OpenLimiterEditor {
    host: EditorHost,
    pub(crate) ui: Ui,
    shared: Option<Arc<Shared>>,
    read: usize,
    loud_read: usize,
    modifiers: Modifiers,
    canvas: SceneCanvas,
    damage: DamageTracker,
    radar: Vec<(f32, f32)>,
    radar_scale: f32,
    /// The rails, the band and the loudness numbers have to be painted again.
    live_dirty: bool,
    out: Vec<u32>,
    out_size: (usize, usize),
    glyphs: GlyphCache,
    /// Window size in pixels the host gave; the design is scaled to it.
    size: (u32, u32),
    /// A page's cells depend on a value that changed.
    needs_rebuild: bool,
}

impl OpenLimiterEditor {
    pub fn new(host: EditorHost) -> Self {
        Self::with_theme(host, load_theme())
    }

    pub fn with_theme(host: EditorHost, theme_id: ThemeId) -> Self {
        let values: Vec<f32> = INFO.params.iter().map(|p| p.clamp(p.default)).collect();
        let mut canvas = SceneCanvas::new(WIDTH as f32, HEIGHT as f32);
        canvas.set_background(theme(theme_id).bg.0 & 0x00ff_ffff);
        let slots = LAYOUT.slots.len();
        let mut e = Self {
            ui: Ui {
                controls: ControlSet::new(host.clone()),
                synced: Vec::new(),
                cells: Vec::new(),
                shell_cells: 0,
                theme: theme_id,
                keys_active: true,
                in_gains: [values[LAYOUT.input_gain[0]], values[LAYOUT.input_gain[1]]],
                gains: [values[P_GAIN_L], values[P_GAIN_R]],
                values,
                selected: 0,
                page: pages::page_for(Some(0)),
                more: false,
                tall: false,
                band_pref: live::load_band_pref(),
                setup_open: false,
                pop: None,
                drag: None,
                page_grab: false,
                page_rev: 0,
                history: VecDeque::with_capacity(WINDOW + 1),
                hist_count: 0,
                quiet: 0,
                input: [Meter::new(FLOOR_DB); 2],
                output: [Meter::new(FLOOR_DB); 2],
                reduction: Meter::new(0.0),
                latency: (0, 48_000.0),
                autogain_db: 0.0,
                clip_age: f32::MAX,
                target: load_target(),
                loud_hist: VecDeque::with_capacity(RADAR_SLOTS + 1),
                loud_count: 0,
                loud_quiet: 0,
                loud: Loud::SILENT,
                key: (false, Meter::new(FLOOR_DB)),
                rms_ms: VecDeque::with_capacity(RMS_HIST + 1),
                rms_aes: live::load_rms_pref().0,
                rms_window: live::load_rms_pref().1,
                rms: None,
                readout: Loud::SILENT,
                readout_age: READOUT_PERIOD,
                slot_gr: vec![Meter::new(0.0); slots],
                slot_lift: vec![Meter::new(0.0); slots],
                slot_latency: vec![0; slots],
                gr_hist: VecDeque::with_capacity(GR_HIST + 1),
                hist_rate: 60.0,
                view: vec![0.0; crate::shared::VIEW_LEN],
                view_len: 0,
                audio: Audio::default(),
                live_gr: 0.0,
                ab: [None, None],
                ab_on: 0,
                hist_mode: detached::load_hist_mode("histmode"),
                spec: spectro::SpecHist::default(),
                popped: [false; 2],
            },
            host,
            shared: None,
            read: 0,
            loud_read: 0,
            modifiers: Modifiers::NONE,
            canvas,
            damage: DamageTracker::new(),
            radar: Vec::new(),
            radar_scale: 0.0,
            live_dirty: true,
            out: Vec::new(),
            out_size: (0, 0),
            glyphs: GlyphCache::default(),
            size: (WIDTH, HEIGHT),
            needs_rebuild: false,
        };
        e.open_page(0);
        e
    }

    pub fn set_theme(&mut self, id: ThemeId, remember: bool) {
        if remember {
            store_theme(id);
        }
        self.ui.theme = id;
        self.canvas.set_background(theme(id).bg.0 & 0x00ff_ffff);
        self.damage.add_all();
        self.live_dirty = true;
    }

    /// Builds the controls again for what is showing, from the values as last seen.
    fn rebuild(&mut self) {
        let ui = &mut self.ui;
        ui.selected = ui.selected.min(dither_page());
        let mut cells = shell_cells(ui);
        let shell = cells.len();
        let values = ui.values.clone();
        let cx = ui.ctx(&values);
        if !ui.setup_open {
            if let Some(local) = ui.page.key_param(&cx) {
                let z = cx.zones;
                cells.push(Cell::segmented(cx.p(local), "KEY", (836.0, z.header.1 + 9.0, 64.0, 22.0)).keys(&["INT", "EXT"]).tip("Detect from the module's own input or from the sidechain key"));
            }
            cells.extend(ui.page.cells(&cx));
        }
        let mut controls = ControlSet::new(self.host.clone());
        for cell in &cells {
            let mut c = kit::control_for(cell);
            if cell.dim {
                c = c.tooltip(cell.tip.clone().unwrap_or_else(|| "Not in use with the present settings".into()));
            }
            controls.add(c);
        }
        for cell in &cells {
            controls.param_changed(cell.param, ui.values[cell.param]);
        }
        controls.set_modifiers(self.modifiers);
        ui.synced = controls.iter().map(Control::value).collect();
        ui.controls = controls;
        ui.cells = cells;
        ui.shell_cells = shell;
        ui.page_rev = ui.page_rev.wrapping_add(1) & 0xffff;
        let rules: Vec<u8> = ui
            .page
            .view_merge()
            .iter()
            .map(|m| match m {
                Merge::Last => 0,
                Merge::Max => 1,
                Merge::Min => 2,
            })
            .collect();
        if let Some(shared) = &self.shared {
            shared.set_view_rules(&rules);
            shared.watch(ui.slot());
        }
        self.damage.add_all();
        self.live_dirty = true;
    }

    /// Opens a module's page (a slot index, or `dither_page()`).
    pub fn open_page(&mut self, selected: usize) {
        let selected = selected.min(dither_page());
        let ui = &mut self.ui;
        let new = selected != ui.selected || ui.cells.is_empty();
        ui.selected = selected;
        if new {
            ui.page = pages::page_for(ui.slot());
            ui.more = false;
            ui.gr_hist.clear();
            ui.view_len = 0;
            ui.tall = match ui.band_pref {
                BandPref::Auto => ui.page.tall(),
                BandPref::Full => false,
                BandPref::Strip => true,
            };
        }
        ui.pop = None;
        self.rebuild();
    }

    /// Sets a parameter from the editor itself: the host, the controls and the values.
    fn set_value(&mut self, index: usize, value: f32) {
        let value = INFO.params[index].clamp(value);
        self.host.set(index, value);
        self.ui.values[index] = value;
        self.sync_param(index, value);
        self.value_damage(index);
    }

    /// What a new value of `index` repaints.
    fn value_damage(&mut self, index: usize) {
        self.damage.add(dmg(self.ui.rack().rect));
        self.damage.add(dmg(PRESET_BOX));
        self.damage.add(dmg(DITHER_BOX));
        self.damage.add(dmg(LATENCY_BOX));
        self.damage.add(dmg(self.ui.zones().header));
        let (base, count) = self.ui.module_range();
        if (base..base + count).contains(&index) {
            let values = self.ui.values.clone();
            let cx = self.ui.ctx(&values);
            for r in self.ui.page.paint_rects(&cx) {
                self.damage.add(dmg(r));
            }
            if self.ui.page.rebuild_on(index - base, &cx) {
                self.needs_rebuild = true;
            }
        }
        self.live_dirty = true;
    }

    /// Builds the controls again if a value asked for it, once nothing is held.
    fn rebuild_if_asked(&mut self) {
        if self.needs_rebuild && self.ui.controls.pressed().is_none() && !self.ui.page_grab {
            self.needs_rebuild = false;
            self.rebuild();
        }
    }

    pub fn set_setup(&mut self, open: bool) {
        self.ui.setup_open = open;
        self.ui.pop = None;
        self.rebuild();
    }

    pub fn set_target(&mut self, index: usize, remember: bool) {
        self.ui.target = index % TARGETS.len();
        if remember {
            store_target(self.ui.target);
        }
        self.damage.add_all();
        self.live_dirty = true;
    }

    fn set_tall(&mut self, tall: bool) {
        if self.ui.tall != tall {
            self.ui.tall = tall;
            self.ui.pop = None;
            self.rebuild();
        }
    }

    /// Loads a factory preset: one gesture per parameter it stores.
    pub fn load_preset(&mut self, index: usize) {
        let Some((_, values)) = PRESETS.get(index) else { return };
        for (i, v) in values.iter().enumerate().take(PRESET_PARAMS) {
            let v = PARAMS[i].clamp(*v);
            if (self.ui.value(i) - v).abs() > 1e-6 {
                self.set_value(i, v);
            }
        }
        self.after_input();
    }

    fn step_preset(&mut self, by: isize) {
        let n = PRESETS.len() as isize;
        let next = match self.ui.preset() {
            Some(i) => (i as isize + by).rem_euclid(n),
            None => 0,
        };
        self.load_preset(next as usize);
    }

    /// Switches between the A and B snapshots: what is live is kept in its slot, the other
    /// is written to the host (only the values that differ).
    fn switch_ab(&mut self) {
        let (now, other) = (self.ui.ab_on, 1 - self.ui.ab_on);
        self.ui.ab[now] = Some(self.ui.values.clone());
        if let Some(target) = self.ui.ab[other].clone() {
            for (i, v) in target.into_iter().enumerate() {
                if i != LAYOUT.rack_shown && (self.ui.values[i] - v).abs() > 1e-7 {
                    self.set_value(i, v);
                }
            }
            if let Some(mask) = self.ui.ab[other].as_ref().map(|t| t[LAYOUT.rack_shown]) {
                self.set_value(LAYOUT.rack_shown, mask);
            }
        }
        self.ui.ab_on = other;
        self.damage.add(dmg(AB));
        self.rebuild();
    }

    /// Copies the live values into the other slot of A/B.
    fn copy_ab(&mut self) {
        let other = 1 - self.ui.ab_on;
        self.ui.ab[other] = Some(self.ui.values.clone());
    }

    // ---- the rack ----

    fn set_shown(&mut self, slot: usize, shown: bool) {
        let mask = self.ui.values[LAYOUT.rack_shown].max(0.0) as u32;
        let mask = if shown { mask | 1 << slot } else { mask & !(1 << slot) };
        self.set_value(LAYOUT.rack_shown, mask as f32);
    }

    /// Puts slot `slot` at `place` among the shown slots; hidden slots go after them.
    fn move_slot(&mut self, slot: usize, place: usize) {
        let mut shown: Vec<usize> = self.ui.order().into_iter().filter(|&s| s != slot && self.ui.shown(s)).collect();
        let hidden: Vec<usize> = self.ui.order().into_iter().filter(|&s| s != slot && !self.ui.shown(s)).collect();
        shown.insert(place.min(shown.len()), slot);
        for (p, s) in shown.into_iter().chain(hidden).enumerate() {
            let index = LAYOUT.slots[s].position;
            if (self.ui.values[index] - (p + 1) as f32).abs() > 1e-6 {
                self.set_value(index, (p + 1) as f32);
            }
        }
    }

    fn add_module(&mut self, slot: usize) {
        self.set_shown(slot, true);
        let n = self.ui.order().into_iter().filter(|&s| self.ui.shown(s)).count();
        self.move_slot(slot, n);
        self.set_value(LAYOUT.slots[slot].on, 1.0);
        self.open_page(slot);
    }

    fn remove_module(&mut self, slot: usize) {
        self.set_value(LAYOUT.slots[slot].on, 0.0);
        self.set_shown(slot, false);
        if self.ui.selected == slot {
            let next = self.ui.rack().chips.first().map_or(dither_page(), |(s, _)| *s);
            self.open_page(next);
        } else {
            self.rebuild();
        }
    }

    /// Every setting of `slot` back to its default; it stays switched on and in its place.
    fn reset_slot(&mut self, slot: usize) {
        let s = &LAYOUT.slots[slot];
        for i in (s.base..s.base + s.count).filter(|&i| i != s.on && i != s.position) {
            let d = INFO.params[i].clamp(INFO.params[i].default);
            if (self.ui.values[i] - d).abs() > 1e-7 {
                self.set_value(i, d);
            }
        }
        self.rebuild();
    }

    fn reset_module(&mut self) {
        let (base, count) = self.ui.module_range();
        for i in base..base + count {
            let d = INFO.params[i].clamp(INFO.params[i].default);
            if (self.ui.values[i] - d).abs() > 1e-7 {
                self.set_value(i, d);
            }
        }
        self.rebuild();
    }

    fn rack_down(&mut self, x: f32, y: f32) {
        let rack = self.ui.rack();
        if inside(rack.dither, x, y) {
            self.open_page(dither_page());
            return;
        }
        if let Some(add) = rack.add.filter(|r| inside(*r, x, y)) {
            let mut items = Vec::new();
            let mut slots = Vec::new();
            for group in ["LIMITERS", "COMPRESSORS", "EXPANDERS", "EQ / SPECTRAL", "COLOUR"] {
                for s in (0..LAYOUT.slots.len()).filter(|&s| !self.ui.shown(s) && module_group(LAYOUT.slots[s].key) == group) {
                    let (_, line) = module_names(LAYOUT.slots[s].key);
                    items.push(format!("{}  ·  {}", LAYOUT.slots[s].label, line));
                    slots.push(s);
                }
            }
            self.open_list(items, add, PopFor::Add(slots), usize::MAX, 360.0);
            return;
        }
        let Some(&(slot, r)) = rack.chips.iter().find(|(_, r)| inside(*r, x, y)) else { return };
        // Alt-click anywhere on a chip takes the module out of the chain.
        if self.modifiers.alt {
            self.remove_module(slot);
            self.after_input();
            return;
        }
        // Cmd-click (Ctrl-click off macOS, where it is not a right click) resets the module.
        if self.modifiers.command || (!cfg!(target_os = "macos") && self.modifiers.ctrl) {
            self.reset_slot(slot);
            self.after_input();
            return;
        }
        if inside(chip_dot(r), x, y) {
            let on = LAYOUT.slots[slot].on;
            let v = if self.ui.values[on] >= 0.5 { 0.0 } else { 1.0 };
            self.set_value(on, v);
            self.after_input();
            return;
        }
        self.ui.drag = Some(Drag { slot, from: (x, y), at: (x, y), moved: false });
    }

    fn rack_up(&mut self, drag: Drag) {
        let rack = self.ui.rack();
        self.damage.add(dmg(rack.rect));
        if !drag.moved {
            self.open_page(drag.slot);
            return;
        }
        if drag.at.1 > rack.rect.1 + rack.rect.3 + 40.0 {
            self.remove_module(drag.slot);
            return;
        }
        let place = rack.chips.iter().filter(|(s, r)| *s != drag.slot && drag.at.0 > r.0 + r.2 * 0.5).count();
        self.move_slot(drag.slot, place);
        self.rebuild();
    }

    fn open_list(&mut self, items: Vec<String>, anchor: R, what: PopFor, chosen: usize, min_w: f32) {
        let list = Popover::list(items, anchor, chosen, min_w, POP_BOUNDS);
        self.ui.pop = Some(Pop { list, what });
        self.damage.add_all();
    }

    /// The page header's "..." menu.
    fn page_menu_items(&self) -> Vec<String> {
        let mut items = vec!["Reset module".to_string()];
        if self.ui.slot().is_some() {
            items.push("Remove from chain".into());
        }
        let values = self.ui.values.clone();
        let cx = self.ui.ctx(&values);
        items.extend(self.ui.page.menu(&cx));
        items
    }

    fn menu_button(&self) -> R {
        (988.0, self.ui.zones().header.1 + 9.0, 24.0, 22.0)
    }

    fn more_button(&self) -> R {
        (1016.0, self.ui.zones().header.1 + 9.0, 64.0, 22.0)
    }

    /// An item of an open list was chosen.
    fn choose(&mut self, item: usize) {
        let Some(pop) = self.ui.pop.take() else { return };
        self.damage.add_all();
        match pop.what {
            PopFor::Param => {
                let param = pop.list.param;
                let v = INFO.params[param].min + item as f32;
                self.set_value(param, v);
                self.after_input();
            }
            PopFor::Preset => self.load_preset(item),
            PopFor::Add(slots) => {
                if let Some(&s) = slots.get(item) {
                    self.add_module(s);
                }
            }
            PopFor::Chip(slot) => match item {
                0 => {
                    let on = LAYOUT.slots[slot].on;
                    let v = if self.ui.values[on] >= 0.5 { 0.0 } else { 1.0 };
                    self.set_value(on, v);
                    self.after_input();
                }
                1 => self.remove_module(slot),
                2 => {
                    self.move_slot(slot, 0);
                    self.rebuild();
                }
                3 => {
                    self.move_slot(slot, usize::MAX);
                    self.rebuild();
                }
                _ => {
                    self.open_page(slot);
                    self.reset_module();
                }
            },
            PopFor::PageMenu => {
                let own = if self.ui.slot().is_some() { 2 } else { 1 };
                match item {
                    0 => self.reset_module(),
                    1 if own == 2 => {
                        if let Some(s) = self.ui.slot() {
                            self.remove_module(s);
                        }
                    }
                    _ => {
                        let values = self.ui.values.clone();
                        let r = {
                            let ui = &mut self.ui;
                            let cx = ui.ctx(&values);
                            let mut touched = Vec::new();
                            let mut vals = ui.values.clone();
                            let mut io = Io { host: &self.host, mods: self.modifiers, values: &mut vals, touched: &mut touched };
                            let r = ui.page.menu_action(item - own, &cx, &mut io);
                            for &i in &touched {
                                ui.values[i] = vals[i];
                            }
                            (r, touched)
                        };
                        let (r, touched) = r;
                        for i in touched {
                            let v = self.ui.values[i];
                            self.sync_param(i, v);
                            self.value_damage(i);
                        }
                        self.page_response(r);
                    }
                }
            }
        }
    }

    // ---- the page ----

    /// Hands a pointer event to the page.
    fn page_pointer(&mut self, ev: Pointer) -> Response {
        let values = self.ui.values.clone();
        let (r, touched) = {
            let ui = &mut self.ui;
            let cx = ui.ctx(&values);
            let mut touched = Vec::new();
            let mut vals = ui.values.clone();
            let r = {
                let mut io = Io { host: &self.host, mods: self.modifiers, values: &mut vals, touched: &mut touched };
                ui.page.pointer(ev, &cx, &mut io)
            };
            for &i in &touched {
                ui.values[i] = vals[i];
            }
            (r, touched)
        };
        for i in touched {
            let v = self.ui.values[i];
            self.sync_param(i, v);
            self.value_damage(i);
        }
        self.page_response(r);
        r
    }

    fn page_response(&mut self, r: Response) {
        if r.rebuild {
            self.rebuild();
        } else if r.repaint {
            let values = self.ui.values.clone();
            let cx = self.ui.ctx(&values);
            for r in self.ui.page.paint_rects(&cx) {
                self.damage.add(dmg(r));
            }
        }
    }

    /// Link gangs two gains: the one that moved takes the other with it.
    fn gang(&mut self, pair: [usize; 2], link: usize, first_limiter: bool) {
        let now = [self.ui.value(pair[0]), self.ui.value(pair[1])];
        let was = if first_limiter { self.ui.gains } else { self.ui.in_gains };
        if first_limiter {
            self.ui.gains = now;
        } else {
            self.ui.in_gains = now;
        }
        if self.ui.value(link) < 0.5 || (now[0] - now[1]).abs() < 1e-6 {
            return;
        }
        let (moved, other) = match ((now[0] - was[0]).abs() > 1e-6, (now[1] - was[1]).abs() > 1e-6) {
            (true, false) => (0, 1),
            (false, true) => (1, 0),
            _ => return,
        };
        self.set_value(pair[other], now[moved]);
        let v = self.ui.values[pair[other]];
        if first_limiter {
            self.ui.gains[other] = v;
        } else {
            self.ui.in_gains[other] = v;
        }
    }

    /// Tells every control on `param` its new value and takes it as agreed.
    fn sync_param(&mut self, param: usize, value: f32) {
        let ui = &mut self.ui;
        ui.controls.param_changed(param, value);
        for (id, c) in ui.controls.iter().enumerate() {
            if c.param == param {
                if let Some(s) = ui.synced.get_mut(id) {
                    *s = c.value();
                }
            }
        }
    }

    /// After anything that can change a value.
    fn after_input(&mut self) {
        // A control the user moved is the newest word on its parameter.
        let mut changed: Vec<(usize, f32, bool)> = Vec::new();
        for (id, c) in self.ui.controls.iter().enumerate() {
            let was = self.ui.synced.get(id).copied().unwrap_or(f32::NAN);
            if c.value() != was && !changed.iter().any(|(p, _, _)| *p == c.param) {
                // A dimmed cell takes no input (wheel, keys); a GUI range bounds what it may set.
                let cell = self.ui.cells.get(id);
                let v = match cell {
                    Some(cell) if cell.dim => self.ui.values[c.param],
                    Some(Cell { gui: Some((lo, hi)), .. }) => c.value().clamp(lo.min(self.ui.values[c.param]), *hi),
                    _ => c.value(),
                };
                changed.push((c.param, v, v != c.value()));
            }
        }
        for (p, v, corrected) in changed {
            if corrected {
                self.set_value(p, v);
            } else {
                self.ui.values[p] = v;
                self.sync_param(p, v);
                self.value_damage(p);
            }
        }
        self.gang([P_GAIN_L, P_GAIN_R], P_LINK, true);
        self.gang(LAYOUT.input_gain, LAYOUT.input_link, false);
        // A module switched on is in the rack; the threshold grip follows the first limiter.
        let grip = self.ui.cells[..self.ui.shell_cells].iter().any(|c| c.kind == Kind::Grip);
        if grip != (!self.ui.tall && !self.ui.setup_open && self.ui.hist_mode == 0 && self.ui.first_on()) {
            self.rebuild();
        }
        self.rebuild_if_asked();
    }

    /// Marks what changed since the last call.
    fn collect_damage(&mut self) {
        let changed = self.ui.controls.take_changed();
        if changed.is_empty() {
            return;
        }
        self.damage.add(dmg(PRESET_BOX));
        self.damage.add(dmg(self.ui.rack().rect));
        self.damage.add(dmg(self.ui.zones().header));
        self.live_dirty = true;
        let mut repaint_page = false;
        let (base, count) = self.ui.module_range();
        for id in changed {
            let Some(cell) = self.ui.cells.get(id).cloned() else { continue };
            self.damage.add(dmg(cell.damage_rect()));
            if (base..base + count).contains(&cell.param) {
                repaint_page = true;
            }
            // Every cell on the same parameter.
            for other in self.ui.cells.iter().filter(|c| c.param == cell.param) {
                self.damage.add(dmg(other.damage_rect()));
            }
        }
        if repaint_page {
            let values = self.ui.values.clone();
            let cx = self.ui.ctx(&values);
            for r in self.ui.page.paint_rects(&cx) {
                self.damage.add(dmg(r));
            }
        }
    }

    /// Design points of a window point.
    fn to_design(&self, x: f32, y: f32) -> (f32, f32) {
        let k = self.size.0 as f32 / WIDTH as f32;
        (x / k, y / k)
    }

    fn zoom(&self) -> f32 {
        self.size.0 as f32 / WIDTH as f32
    }

    fn tick_meters(&mut self, shared: &Shared, dt: f32) {
        let ui = &mut self.ui;
        let head = shared.head();
        if head.wrapping_sub(self.read) > HIST {
            self.read = head.wrapping_sub(HIST);
        }
        let mut reduction = 0.0f32;
        let mut fresh = 0;
        while self.read != head {
            let e = shared.entry(self.read);
            self.read = self.read.wrapping_add(1);
            fresh += 1;
            reduction = reduction.max(-db(e.gain));
            let silent = db(e.input) <= FLOOR_DB && db(e.output) <= FLOOR_DB && e.gain > 0.9999;
            ui.quiet = if silent { ui.quiet + 1 } else { 0 };
            if ui.history.len() == WINDOW {
                ui.history.pop_front();
            }
            ui.history.push_back(e);
            ui.hist_count += 1;
            if ui.rms_ms.len() == RMS_HIST {
                ui.rms_ms.pop_front();
            }
            ui.rms_ms.push_back(e.ms);
        }
        let window_silent = ui.quiet >= ui.history.len();
        let before = (ui.input, ui.output, ui.reduction, ui.key.1);
        let (peak_in, peak_out) = shared.take_peaks();
        for ch in 0..2 {
            ui.input[ch].feed(db(peak_in[ch]).max(FLOOR_DB), FLOOR_DB, 30.0, dt);
            ui.output[ch].feed(db(peak_out[ch]).max(FLOOR_DB), FLOOR_DB, 30.0, dt);
        }
        ui.reduction.feed(reduction, 0.0, 14.0, dt);
        let (present, key_peak) = shared.take_key();
        let key_was = ui.key.0;
        ui.key.0 = present;
        ui.key.1.feed(db(key_peak).max(FLOOR_DB), FLOOR_DB, 30.0, dt);
        let moved = (0..2).any(|c| before.0[c].moved(&ui.input[c]) || before.1[c].moved(&ui.output[c])) || before.2.moved(&ui.reduction) || before.3.moved(&ui.key.1) || key_was != present;
        if (fresh > 0 && !window_silent) || moved {
            self.live_dirty = true;
        }
        if key_was != present {
            self.damage.add(dmg(self.ui.rack().rect));
        }
    }

    fn tick_loudness(&mut self, shared: &Shared) {
        let ui = &mut self.ui;
        let head = shared.loud_head();
        if head.wrapping_sub(self.loud_read) > LOUD_HIST {
            self.loud_read = head.wrapping_sub(LOUD_HIST);
        }
        let was_quiet = ui.loud_quiet >= ui.loud_hist.len();
        let arrived = self.loud_read != head;
        while self.loud_read != head {
            let e = shared.loud_entry(self.loud_read);
            self.loud_read = self.loud_read.wrapping_add(1);
            ui.loud_quiet = if e.0 <= LOUD_FLOOR && e.1 <= LOUD_FLOOR { ui.loud_quiet + 1 } else { 0 };
            if ui.loud_hist.len() == RADAR_SLOTS {
                ui.loud_hist.pop_front();
            }
            ui.loud_hist.push_back(e);
            ui.loud_count += 1;
        }
        let loud = shared.loud();
        if (arrived && !(was_quiet && ui.loud_quiet >= ui.loud_hist.len())) || loud != ui.loud {
            self.live_dirty = true;
        }
        ui.loud = loud;
        let was_clipping = ui.clipping();
        ui.clip_age = if shared.take_soft_clip() { 0.0 } else { (ui.clip_age + 0.0f32).min(1e6) };
        if ui.clipping() != was_clipping {
            self.live_dirty = true;
        }
        let latency = shared.timing();
        if latency != ui.latency {
            ui.latency = latency;
            self.damage.add(dmg(LATENCY_BOX));
            self.damage.add(dmg(self.ui.zones().header));
        }
    }

    /// The RMS and loudness numbers, taken every `READOUT_PERIOD`.
    fn tick_readout(&mut self, dt: f32, shared: &Shared) {
        let ui = &mut self.ui;
        ui.readout_age += dt;
        if ui.readout_age < READOUT_PERIOD {
            return;
        }
        ui.readout_age = 0.0;
        // Shown to a tenth: a change smaller than that is not repainted.
        let ag = (shared.autogain_db() * 10.0).round() / 10.0;
        if ag != ui.autogain_db {
            ui.autogain_db = ag;
            self.damage.add(dmg(AUTOGAIN_VALUE));
        }
        let rms = ui.rms_now();
        if rms != ui.rms || ui.loud != ui.readout {
            ui.rms = rms;
            ui.readout = ui.loud;
            self.live_dirty = true;
        }
    }

    /// The chain's slots: chip meters, and the shown module's read-out and audio.
    fn tick_slots(&mut self, shared: &Shared, dt: f32) {
        let mut rack_moved = false;
        for s in 0..LAYOUT.slots.len() {
            let (gr, latency) = shared.take_slot(s);
            let before = self.ui.slot_gr[s];
            self.ui.slot_gr[s].feed(gr, 0.0, 14.0, dt);
            let lift_before = self.ui.slot_lift[s];
            self.ui.slot_lift[s].feed(shared.take_slot_lift(s), 0.0, 14.0, dt);
            rack_moved |= lift_before.moved(&self.ui.slot_lift[s]);
            rack_moved |= before.moved(&self.ui.slot_gr[s]);
            if self.ui.slot_latency[s] != latency {
                self.ui.slot_latency[s] = latency;
                self.damage.add(dmg(self.ui.zones().header));
            }
            if Some(s) == self.ui.slot() {
                // Peak, then a 60 dB/s fall like the levels: a frame with no audio block since the
                // last read takes 0, which would otherwise notch the trace and the meter.
                let last = self.ui.gr_hist.back().copied().unwrap_or(0.0);
                self.ui.live_gr = gr.max(last - 60.0 * dt);
                if self.ui.gr_hist.len() == GR_HIST {
                    self.ui.gr_hist.pop_front();
                }
                self.ui.gr_hist.push_back(self.ui.live_gr);
            }
        }
        if rack_moved {
            self.damage.add(dmg(self.ui.rack().rect));
        }
        if dt > 0.0 {
            self.ui.hist_rate += (1.0 / dt - self.ui.hist_rate) * 0.05;
        }
        if let Some(s) = self.ui.slot() {
            let ui = &mut self.ui;
            ui.view_len = shared.view(s, &mut ui.view);
            if ui.page.wants_audio() {
                let a = &mut ui.audio;
                let ([p0, p1], [q0, q1], [k0, k1]) = (&mut a.pre, &mut a.post, &mut a.key);
                for (c, buf) in [p0, p1, q0, q1, k0, k1].into_iter().enumerate() {
                    buf.resize(AUDIO_LEN, 0.0);
                    shared.read_tap(c, buf);
                }
            }
        }
        let values = self.ui.values.clone();
        let dirty = {
            let ui = &mut self.ui;
            let cx = Ctx { t: theme(ui.theme), values: &values, slot: None, base: 0, count: 0, zones: ui.zones(), more: ui.more, sample_rate: ui.latency.1, key_present: ui.key.0 };
            let (base, count) = ui.module_range();
            let cx = Ctx { slot: ui.slot(), base, count, ..cx };
            let live = Live {
                view: &ui.view[..ui.view_len],
                gr: ui.live_gr,
                gr_hist: &ui.gr_hist,
                hist_rate: ui.hist_rate,
                audio: ui.page.wants_audio().then_some(&ui.audio),
                key: (ui.key.0, ui.key.1.level),
                latency: ui.slot().map_or(0, |s| ui.slot_latency[s]),
                sample_rate: ui.latency.1,
                dt,
            };
            ui.page.tick(&cx, &live)
        };
        if dirty && !self.ui.setup_open {
            let cx = self.ui.ctx(&values);
            for r in self.ui.page.paint_rects(&cx) {
                self.damage.add(dmg(r));
            }
        }
    }
}

impl Editor for OpenLimiterEditor {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn size_range(&self) -> SizeRange {
        SizeRange::between((WIDTH * 3 / 4, HEIGHT * 3 / 4), (WIDTH * 2, HEIGHT * 2)).with_aspect(WIDTH, HEIGHT)
    }

    fn resize(&mut self, width: u32, height: u32) {
        let w = width.clamp(WIDTH * 3 / 4, WIDTH * 2);
        let h = (w as u64 * HEIGHT as u64 / WIDTH as u64) as u32;
        let _ = height;
        if (w, h) != self.size {
            self.size = (w, h);
            self.damage.add_all();
            self.live_dirty = true;
        }
    }

    fn bind(&mut self, state: Option<EditorState>) {
        self.shared = state.and_then(|s| s.downcast::<Shared>().ok());
        self.read = self.shared.as_ref().map_or(0, |s| s.head());
        self.loud_read = self.shared.as_ref().map_or(0, |s| s.loud_head());
        if let Some(shared) = self.shared.clone() {
            for (i, v) in self.ui.values.iter_mut().enumerate() {
                if let Some(now) = shared.value(i) {
                    *v = now;
                }
            }
            self.ui.gains = [self.ui.values[P_GAIN_L], self.ui.values[P_GAIN_R]];
            self.ui.in_gains = [self.ui.values[LAYOUT.input_gain[0]], self.ui.values[LAYOUT.input_gain[1]]];
            // Open the first module in the chain.
            let first = self.ui.rack().chips.first().map_or(dither_page(), |(s, _)| *s);
            self.ui.cells.clear();
            self.open_page(first);
        }
    }

    fn param_changed(&mut self, index: usize, plain: f32) {
        let Some(slot) = self.ui.values.get_mut(index) else { return };
        *slot = plain;
        self.sync_param(index, plain);
        if index == P_GAIN_L || index == P_GAIN_R {
            self.ui.gains = [self.ui.values[P_GAIN_L], self.ui.values[P_GAIN_R]];
        }
        if LAYOUT.input_gain.contains(&index) {
            self.ui.in_gains = [self.ui.values[LAYOUT.input_gain[0]], self.ui.values[LAYOUT.input_gain[1]]];
        }
        if index == LAYOUT.autogain {
            self.damage.add(dmg(AUTOGAIN_VALUE));
        }
        self.value_damage(index);
        for cell in self.ui.cells.iter().filter(|c| c.param == index) {
            self.damage.add(dmg(cell.damage_rect()));
        }
        self.after_input();
    }

    fn tick(&mut self, dt: f32) {
        let Some(shared) = self.shared.clone() else { return };
        let dt = dt.clamp(0.0, 0.25);
        self.tick_meters(&shared, dt);
        self.tick_loudness(&shared);
        self.tick_readout(dt, &shared);
        self.ui.clip_age = (self.ui.clip_age + dt).min(1e6);
        self.tick_slots(&shared, dt);
        self.rebuild_if_asked();
        let theme_i = ThemeId::ALL.iter().position(|t| *t == self.ui.theme).unwrap_or(0);
        shared.set_look(theme_i, self.ui.target);
        let popped = [pops::is_open(&shared, detached::Kind::History), pops::is_open(&shared, detached::Kind::Radar)];
        if popped != self.ui.popped {
            self.ui.popped = popped;
            self.damage.add_all();
        }
        if self.ui.hist_mode != 0 && !self.ui.tall && self.ui.spec.tick(&shared, dt) {
            self.live_dirty = true;
        }
    }

    fn damage(&mut self) -> Option<Vec<DamageRect>> {
        self.collect_damage();
        let mut rects = self.damage.pending()?.to_vec();
        if self.live_dirty {
            for r in live::live_rects(&self.ui) {
                rects.push(dmg(r));
            }
        }
        let k = self.zoom();
        if (k - 1.0).abs() > 1e-6 {
            for r in &mut rects {
                *r = DamageRect::new(r.x * k, r.y * k, r.w * k, r.h * k);
            }
        }
        Some(rects)
    }

    fn render(&mut self, scale: f32) -> Frame<'_> {
        self.collect_damage();
        if self.damage.pending().is_some_and(|d| d.len() > 8) {
            self.damage.add_all();
        }
        let scale = scale * self.zoom();
        let ui = &self.ui;
        let key = ui.static_key();
        let damage = self.damage.pending().map(bounds);
        let touched = |r: &DamageRect| damage.as_ref().is_none_or(|d| d.iter().any(|x| x.touches(r)));
        let frame = self.canvas.frame_damaged(scale, key, |dl| chrome::paint_static(ui, dl), |dl| chrome::paint_dynamic(ui, dl, &touched), damage.as_deref());
        let (w, h) = (frame.width, frame.height);
        let k = w as f32 / WIDTH as f32;
        // Our frame is the canvas's picture with the live parts painted over it.
        let fresh = self.out_size != (w, h) || damage.is_none();
        if fresh {
            self.out.clear();
            self.out.extend_from_slice(frame.pixels);
            self.out_size = (w, h);
        } else {
            let mut copy = |r: R| {
                let (x0, x1) = (((r.0 - 2.0) * k).floor().max(0.0) as usize, (((r.0 + r.2 + 2.0) * k).ceil() as usize).min(w));
                let (y0, y1) = (((r.1 - 2.0) * k).floor().max(0.0) as usize, (((r.1 + r.3 + 2.0) * k).ceil() as usize).min(h));
                for y in y0..y1 {
                    self.out[y * w + x0..y * w + x1].copy_from_slice(&frame.pixels[y * w + x0..y * w + x1]);
                }
            };
            for d in damage.iter().flatten() {
                copy((d.x, d.y, d.w, d.h));
            }
            if self.live_dirty {
                for r in live::live_rects(ui) {
                    copy(r);
                }
            }
        }
        if self.radar_scale != k {
            self.radar = live::radar_lut(k, RADAR_C, RADAR_R);
            self.radar_scale = k;
        }
        let repaint_live = fresh || self.live_dirty || damage.as_ref().is_some_and(|d| live::live_rects(ui).iter().any(|r| d.iter().any(|x| x.touches(&dmg(*r)))));
        if repaint_live {
            let mut target = Target { px: &mut self.out, w, h };
            live::paint(ui, &mut target, &mut self.glyphs, k, &self.radar);
        }
        self.live_dirty = false;
        self.damage.clear();
        Frame::new(&self.out, w, h)
    }

    fn modifiers_changed(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
        self.ui.controls.set_modifiers(modifiers);
    }

    fn mouse_down(&mut self, x: f32, y: f32, clicks: u32) {
        let (x, y) = self.to_design(x, y);
        // An open list takes the click: an item, or anywhere else closes it.
        if let Some(pop) = &self.ui.pop {
            let item = pop.list.item_at(x, y);
            match item {
                Some(i) => self.choose(i),
                None => {
                    self.ui.pop = None;
                    self.damage.add_all();
                }
            }
            return;
        }
        if inside(PRESET_PREV, x, y) {
            self.step_preset(-1);
        } else if inside(PRESET_NEXT, x, y) {
            self.step_preset(1);
        } else if inside(PRESET_BOX, x, y) {
            let items = PRESETS.iter().map(|p| p.0.to_string()).collect();
            let chosen = self.ui.preset().unwrap_or(usize::MAX);
            self.open_list(items, PRESET_BOX, PopFor::Preset, chosen, 280.0);
        } else if inside(AB, x, y) {
            if self.modifiers.alt {
                self.copy_ab();
            } else {
                self.switch_ab();
            }
        } else if inside(THEME_BUTTON, x, y) {
            let next = self.ui.theme.next();
            self.set_theme(next, true);
        } else if inside(SETUP_BUTTON, x, y) || (self.ui.setup_open && inside(SETUP_DONE, x, y)) {
            self.set_setup(!self.ui.setup_open);
        } else if self.ui.setup_open && inside(SETUP_BAND, x, y) {
            let i = ((x - SETUP_BAND.0) / (SETUP_BAND.2 / 3.0)).floor().clamp(0.0, 2.0) as usize;
            self.ui.band_pref = [BandPref::Auto, BandPref::Full, BandPref::Strip][i];
            live::store_band_pref(self.ui.band_pref);
            self.damage.add_all();
        } else if self.ui.setup_open && inside(SETUP_RMS_SCALE, x, y) {
            self.ui.rms_aes = x < SETUP_RMS_SCALE.0 + SETUP_RMS_SCALE.2 * 0.5;
            live::store_rms_pref(self.ui.rms_aes, self.ui.rms_window);
            self.ui.rms = self.ui.rms_now();
            self.damage.add_all();
            self.live_dirty = true;
        } else if self.ui.setup_open && inside(SETUP_RMS_WINDOW, x, y) {
            let i = ((x - SETUP_RMS_WINDOW.0) / (SETUP_RMS_WINDOW.2 / RMS_WINDOWS.len() as f32)).floor().max(0.0) as usize;
            self.ui.rms_window = i.min(RMS_WINDOWS.len() - 1);
            live::store_rms_pref(self.ui.rms_aes, self.ui.rms_window);
            self.ui.rms = self.ui.rms_now();
            self.damage.add_all();
            self.live_dirty = true;
        } else if self.ui.setup_open && inside(SETUP_THEME, x, y) {
            let i = ((x - SETUP_THEME.0) / (SETUP_THEME.2 / ThemeId::ALL.len() as f32)).floor().max(0.0) as usize;
            self.set_theme(ThemeId::from_index(i.min(ThemeId::ALL.len() - 1)), true);
        } else if self.ui.setup_open && !inside(SETUP_SHEET, x, y) && inside((104.0, 56.0, 992.0, 788.0), x, y) {
            self.set_setup(false);
        } else if CAN_POP && !self.ui.setup_open && !self.ui.tall && (inside(HIST_POP, x, y) || inside(RADAR_POP, x, y)) {
            let kind = if inside(HIST_POP, x, y) { detached::Kind::History } else { detached::Kind::Radar };
            if let Some(shared) = self.shared.clone() {
                pops::toggle(&shared, kind);
                self.ui.popped[kind as usize] = pops::is_open(&shared, kind);
                self.damage.add_all();
            }
        } else if !self.ui.setup_open && !self.ui.tall && inside(HIST_MODE, x, y) {
            let mode = (((x - HIST_MODE.0) / (HIST_MODE.2 / 3.0)) as u8).min(2);
            if mode != self.ui.hist_mode {
                self.ui.hist_mode = mode;
                detached::store_hist_mode("histmode", mode);
                self.damage.add_all();
                self.live_dirty = true;
            }
        } else if !self.ui.setup_open && inside(CHEVRON, x, y) {
            let tall = !self.ui.tall;
            self.set_tall(tall);
        } else if !self.ui.setup_open && !self.ui.tall && clicks >= 2 && (inside(HIST_PANEL, x, y) || inside(RADAR_PANEL, x, y)) && !inside(PLOT, x, y) {
            self.set_tall(true);
        } else if !self.ui.setup_open && self.ui.tall && clicks >= 2 && inside(BAND_STRIP, x, y) {
            self.set_tall(false);
        } else if inside(TARGET_CHIP, x, y) {
            let next = self.ui.target + 1;
            self.set_target(next, true);
        } else if inside(RESET_CHIP, x, y) {
            if let Some(shared) = &self.shared {
                shared.request_loud_reset();
            }
            // Show the restarted numbers on the next tick, not up to a period later.
            self.ui.readout_age = READOUT_PERIOD;
        } else if !self.ui.setup_open && inside(self.ui.rack().rect, x, y) {
            self.rack_down(x, y);
        } else if !self.ui.setup_open && inside(self.menu_button(), x, y) {
            let items = self.page_menu_items();
            let anchor = self.menu_button();
            self.open_list(items, (anchor.0 - 160.0, anchor.1, anchor.2 + 160.0, anchor.3), PopFor::PageMenu, usize::MAX, 200.0);
        } else if !self.ui.setup_open && self.ui.page.has_more() && inside(self.more_button(), x, y) {
            self.ui.more = !self.ui.more;
            self.rebuild();
        } else {
            // A dropdown opens its list.
            if let Some(cell) = self.ui.cells.iter().find(|c| c.kind == Kind::Dropdown && !c.dim && inside(c.rect, x, y)).cloned() {
                let v = self.ui.values[cell.param];
                let list = Popover::open(&cell, v, POP_BOUNDS);
                self.ui.pop = Some(Pop { list, what: PopFor::Param });
                self.damage.add_all();
                return;
            }
            // A segmented choice over a stepped parameter: one key per step.
            let stepped = self.ui.cells.iter().find(|c| !c.dim && inside(c.rect, x, y)).and_then(|c| kit::segment_click(c, x).map(|v| (c.param, v)));
            if let Some((param, v)) = stepped {
                self.set_value(param, v);
                self.after_input();
                return;
            }
            // A dimmed cell takes nothing; the page may still use the click.
            let dimmed = self.ui.cells.iter().any(|c| c.dim && inside(c.rect, x, y));
            if !dimmed {
                self.ui.controls.mouse_down(x, y, clicks);
            }
            let taken = self.ui.controls.pressed().is_some();
            self.after_input();
            if !taken && !self.ui.setup_open && inside(self.ui.zones().page, x, y) {
                let r = self.page_pointer(Pointer::Down { x, y, clicks });
                self.ui.page_grab = r.taken;
            }
        }
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        let (x, y) = self.to_design(x, y);
        if let Some(pop) = self.ui.pop.as_mut() {
            let hover = pop.list.item_at(x, y);
            if hover != pop.list.hover {
                pop.list.hover = hover;
                let r = pop.list.rect;
                self.damage.add(dmg(r));
            }
            return;
        }
        if let Some(d) = self.ui.drag.as_mut() {
            d.at = (x, y);
            d.moved |= (x - d.from.0).abs() > 4.0 || (y - d.from.1).abs() > 4.0;
            self.damage.add_all();
            return;
        }
        if self.ui.page_grab {
            self.page_pointer(Pointer::Move { x, y });
            return;
        }
        self.ui.controls.mouse_move(x, y);
        self.after_input();
    }

    fn mouse_up(&mut self, x: f32, y: f32) {
        let (x, y) = self.to_design(x, y);
        if let Some(mut d) = self.ui.drag.take() {
            d.at = (x, y);
            self.rack_up(d);
            return;
        }
        if self.ui.page_grab {
            self.ui.page_grab = false;
            self.page_pointer(Pointer::Up { x, y });
            return;
        }
        self.ui.controls.mouse_up(x, y);
        self.after_input();
    }

    fn wheel(&mut self, x: f32, y: f32, dy: f32) {
        let (x, y) = self.to_design(x, y);
        if let Some(pop) = self.ui.pop.as_mut() {
            pop.list.wheel(dy);
            self.damage.add_all();
            return;
        }
        let on_control = self.ui.controls.iter().any(|c| c.area.contains(x, y));
        if on_control {
            self.ui.controls.wheel(x, y, dy);
            self.after_input();
        } else if inside(self.ui.zones().page, x, y) && !self.ui.setup_open {
            self.page_pointer(Pointer::Wheel { x, y, dy });
        }
    }

    fn wants_keys(&self) -> bool {
        self.ui.pop.is_some() || self.ui.controls.wants_keys()
    }

    fn key_down(&mut self, key: Key, modifiers: Modifiers) -> bool {
        if key == Key::Escape {
            if self.ui.pop.take().is_some() {
                self.damage.add_all();
                return true;
            }
            if self.ui.setup_open {
                self.set_setup(false);
                return true;
            }
        }
        let used = self.ui.controls.key_down(key, modifiers);
        self.after_input();
        used
    }

    fn key_up(&mut self, key: Key, modifiers: Modifiers) -> bool {
        self.ui.controls.key_up(key, modifiers)
    }

    fn focus_changed(&mut self, focused: bool) {
        self.ui.keys_active = focused;
        if let Some(cell) = self.ui.controls.focused().and_then(|id| self.ui.cells.get(id)) {
            self.damage.add(dmg(cell.damage_rect()));
        }
        if !focused {
            self.ui.controls.release_keys();
        }
    }

    fn right_mouse_down(&mut self, x: f32, y: f32) -> bool {
        let (x, y) = self.to_design(x, y);
        let rack = self.ui.rack();
        if !self.ui.setup_open && inside(rack.rect, x, y) {
            if let Some(&(slot, r)) = rack.chips.iter().find(|(_, r)| inside(*r, x, y)) {
                let on = self.ui.on(LAYOUT.slots[slot].on);
                let items = vec![if on { "Bypass".to_string() } else { "Switch on".to_string() }, "Remove from chain".into(), "Move first".into(), "Move last".into(), "Reset module".into()];
                self.open_list(items, r, PopFor::Chip(slot), usize::MAX, 180.0);
                return true;
            }
        }
        self.ui.controls.context_click(x, y)
    }

    fn take_param_menu(&mut self) -> Option<ParamMenu> {
        let mut menu = self.ui.controls.take_param_menu()?;
        let k = self.zoom();
        menu.x *= k;
        menu.y *= k;
        Some(menu)
    }

    fn param_menu_action(&mut self, param: u32, action: ParamMenuAction) -> Option<String> {
        let text = self.ui.controls.param_menu_action(param, action);
        self.after_input();
        text
    }

    fn take_text_edit(&mut self) -> Option<TextEdit> {
        let mut edit = self.ui.controls.take_text_edit()?;
        let k = self.zoom();
        edit.rect = DamageRect::new(edit.rect.x * k, edit.rect.y * k, edit.rect.w * k, edit.rect.h * k);
        Some(edit)
    }

    fn text_edit_commit(&mut self, text: &str) -> bool {
        let ok = self.ui.controls.text_edit_commit(text);
        self.after_input();
        ok
    }

    fn text_edit_cancel(&mut self) {
        self.ui.controls.text_edit_cancel();
    }

    fn tooltips(&self) -> Vec<Tooltip> {
        let mut tips = self.ui.controls.tooltips();
        let rack = self.ui.rack();
        for &(slot, r) in &rack.chips {
            tips.push(Tooltip { rect: dmg(chip_dot(r)), text: format!("Switch {} on or off", LAYOUT.slots[slot].label) });
            tips.push(Tooltip { rect: dmg((r.0 + 24.0, r.1, r.2 - 24.0, r.3)), text: format!("{}: click to open, drag to move, Alt-click to remove, {}-click to reset, right-click for more", LAYOUT.slots[slot].label, if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" }) });
        }
        if let Some(add) = rack.add {
            tips.push(Tooltip { rect: dmg(add), text: "Add a module to the chain".into() });
        }
        tips.push(Tooltip { rect: dmg(rack.dither), text: "Requantise: word length and dither, always last in the chain".into() });
        tips.push(Tooltip { rect: dmg(SETUP_BUTTON), text: "Alignment of the first limiter, metering and window".into() });
        tips.push(Tooltip { rect: dmg(AB), text: "Switch between two settings. Option-click copies this one to the other".into() });
        tips.push(Tooltip { rect: dmg(THEME_BUTTON), text: "Next theme: light, dark, graphite, paper".into() });
        tips.push(Tooltip { rect: dmg(TARGET_CHIP), text: "Loudness target: click for the next one".into() });
        tips.push(Tooltip { rect: dmg(RESET_CHIP), text: "Start the integrated loudness, range and true peak again".into() });
        if CAN_POP && !self.ui.tall {
            tips.push(Tooltip { rect: dmg(HIST_POP), text: "Open or close the history in its own window".into() });
            tips.push(Tooltip { rect: dmg(RADAR_POP), text: "Open or close the loudness radar in its own window".into() });
            tips.push(Tooltip { rect: dmg(HIST_MODE), text: "Levels, or the output spectrum as a 2D spectrogram or a 3D waterfall".into() });
        }
        tips.push(Tooltip { rect: dmg(CHEVRON), text: "Make the top band a strip, or full height".into() });
        tips.push(Tooltip { rect: dmg(RMS_BOX), text: "Output RMS after Requantise, louder channel. Scale and window are in SETUP".into() });
        tips.push(Tooltip { rect: dmg(KEY_LAMP), text: "Sidechain key: lit while the host feeds one".into() });
        let values = self.ui.values.clone();
        let cx = self.ui.ctx(&values);
        tips.extend(self.ui.page.tips(&cx));
        let k = self.zoom();
        if (k - 1.0).abs() > 1e-6 {
            for t in &mut tips {
                t.rect = DamageRect::new(t.rect.x * k, t.rect.y * k, t.rect.w * k, t.rect.h * k);
            }
        }
        tips
    }

    fn accessibility_nodes(&self) -> Vec<AccessNode> {
        self.ui.controls.accessibility_nodes()
    }

    fn accessibility_action(&mut self, node: usize, action: AccessAction) -> bool {
        let done = self.ui.controls.accessibility_action(node, action);
        self.after_input();
        done
    }

    fn accessibility_range(&self, node: usize) -> Option<AccessRange> {
        self.ui.controls.accessibility_range(node)
    }

    fn accessibility_set_value(&mut self, node: usize, value: f64) -> bool {
        let done = self.ui.controls.accessibility_set_value(node, value);
        self.after_input();
        done
    }

    fn accessibility_focus(&mut self, node: usize) -> bool {
        self.ui.controls.accessibility_focus(node)
    }
}

/// The editor factory for every format.
pub fn make_editor(host: EditorHost) -> Option<Box<dyn Editor>> {
    Some(Box::new(OpenLimiterEditor::new(host)))
}

#[cfg(test)]
mod tests;
