//! What a module page is to the shell.
//!
//! The shell draws the window, the rack, the meters, the page header (ON pill, name,
//! latency, the "..." menu, MORE, the KEY switch) and every [`Cell`] a page lists. A page
//! draws its pictures and handles what happens in them that is not a control: dragging a
//! node in a graph, a split line, a curve handle.
//!
//! Geometry is in window points (1200 x 860); see `design/shell.md` for the zones.

#![allow(dead_code)] // the pages' toolbox: not every page uses every piece

use std::collections::VecDeque;

use openplugin::core::editor::Tooltip;
use openplugin::core::EditorHost;
use openplugin::dri::goodlookindri::scene::Scene;

use super::kit::Cell;
use super::raster::R;
use super::theme::Theme;
use crate::INFO;

/// The parts of the module page, for the current mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zones {
    /// The whole page panel.
    pub page: R,
    /// The header row: ON pill, name, the header slot (x 420..900), latency, menu, MORE.
    pub header: R,
    /// Where the page's pictures go: 960 wide, 200 high (STANDARD) or 372 (TALL).
    pub picture: R,
    /// Where its controls go, 960 x 148, the same in both modes. MORE swaps it.
    pub control: R,
    /// The header slot a page may fill with its main mode choice.
    pub slot: R,
    /// TALL: the top band is a strip and the picture zone is 372 high.
    pub tall: bool,
}

impl Zones {
    pub const fn new(tall: bool) -> Self {
        let top = if tall { 196.0 } else { 368.0 };
        Self {
            page: (104.0, top, 992.0, 780.0 - top),
            header: (104.0, top, 992.0, 40.0),
            picture: (120.0, top + 48.0, 960.0, 616.0 - (top + 48.0)),
            control: (120.0, 624.0, 960.0, 148.0),
            slot: (420.0, top + 8.0, 480.0, 24.0),
            tall,
        }
    }
}

/// How the shell merges one read-out value across the audio blocks between two editor frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Merge {
    /// The newest block wins.
    Last,
    /// The largest since the editor last looked (reductions, peaks).
    Max,
    /// The smallest since the editor last looked (signed gains).
    Min,
}

/// What a page knows about the world when it builds or paints.
pub struct Ctx<'a> {
    pub t: &'static Theme,
    /// Every host parameter's plain value, by host index.
    pub values: &'a [f32],
    /// The chain slot shown, or `None` for the dither.
    pub slot: Option<usize>,
    /// Host index of the module's first parameter; local index `i` is host index `base + i`.
    pub base: usize,
    /// How many parameters the module has.
    pub count: usize,
    pub zones: Zones,
    /// The MORE face is showing in the control zone.
    pub more: bool,
    pub sample_rate: f32,
    /// The host feeds a sidechain key.
    pub key_present: bool,
}

impl Ctx<'_> {
    /// Host index of the module's parameter `local`.
    pub fn p(&self, local: usize) -> usize {
        self.base + local
    }

    /// Plain value of the module's parameter `local`.
    pub fn v(&self, local: usize) -> f32 {
        self.values.get(self.base + local).copied().unwrap_or(0.0)
    }

    /// The module's parameter `local` is on (a toggle at its upper half).
    pub fn on(&self, local: usize) -> bool {
        let Some(info) = INFO.params.get(self.base + local) else { return false };
        self.v(local) >= (info.min + info.max) * 0.5
    }

    /// A choice parameter's index.
    pub fn choice(&self, local: usize) -> usize {
        let Some(info) = INFO.params.get(self.base + local) else { return 0 };
        (self.v(local) - info.min).round().max(0.0) as usize
    }

    /// Local index of the module's parameter whose id (without the slot prefix) is `id`.
    pub fn find(&self, id: &str) -> Option<usize> {
        (0..self.count).find(|&i| INFO.params.get(self.base + i).is_some_and(|p| p.id == id || p.id.rsplit_once('.').is_some_and(|(_, tail)| tail == id)))
    }
}

/// Audio around the shown module, newest sample last. Only kept while a page asks for it.
#[derive(Clone, Debug, Default)]
pub struct Audio {
    /// The module's input, L and R.
    pub pre: [Vec<f32>; 2],
    /// Its output.
    pub post: [Vec<f32>; 2],
    /// The sidechain key as the module saw it (zeros without one).
    pub key: [Vec<f32>; 2],
}

/// Samples of each [`Audio`] channel.
pub const AUDIO_LEN: usize = 4096;

/// What moves: the module's meters and read-out, as of the last editor tick.
pub struct Live<'a> {
    /// The module's [`olm_api::Module::view`] values, merged as the page asked.
    pub view: &'a [f32],
    /// The module's deepest reduction since the last tick, dB, positive.
    pub gr: f32,
    /// The module's reduction per tick, newest last, about four seconds of it.
    pub gr_hist: &'a VecDeque<f32>,
    /// Ticks per second of `gr_hist`.
    pub hist_rate: f32,
    pub audio: Option<&'a Audio>,
    /// `(key present, key peak since the last tick, dBFS)`.
    pub key: (bool, f32),
    /// The module's own latency, samples.
    pub latency: u32,
    pub sample_rate: f32,
    /// Seconds since the last tick.
    pub dt: f32,
}

/// What a page may do to the host: gestures on parameters, by host index.
pub struct Io<'a> {
    pub(super) host: &'a EditorHost,
    pub(super) mods: openplugin::core::editor::Modifiers,
    pub(super) values: &'a mut [f32],
    pub(super) touched: &'a mut Vec<usize>,
}

impl Io<'_> {
    fn store(&mut self, index: usize, value: f32) -> f32 {
        let value = INFO.params.get(index).map_or(value, |p| p.clamp(value));
        if let Some(v) = self.values.get_mut(index) {
            *v = value;
        }
        self.touched.push(index);
        value
    }

    /// Starts a drag on a parameter.
    pub fn begin(&mut self, index: usize) {
        self.host.begin(index);
    }

    /// Moves it, inside a drag.
    pub fn perform(&mut self, index: usize, value: f32) {
        let v = self.store(index, value);
        self.host.perform(index, v);
    }

    /// Ends the drag.
    pub fn end(&mut self, index: usize) {
        self.host.end(index);
    }

    /// Sets a value in one gesture.
    pub fn set(&mut self, index: usize, value: f32) {
        let v = self.store(index, value);
        self.host.set(index, v);
    }

    pub fn value(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    /// The modifier keys held during this event (Alt-click, Shift for fine drags).
    pub fn modifiers(&self) -> openplugin::core::editor::Modifiers {
        self.mods
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Down { x: f32, y: f32, clicks: u32 },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32 },
    Wheel { x: f32, y: f32, dy: f32 },
}

/// What a page did with a pointer event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    /// The event was the page's; nothing else gets it.
    pub taken: bool,
    /// The page's cells changed (a band was picked): build the controls again.
    pub rebuild: bool,
    /// The page's pictures have to be painted again.
    pub repaint: bool,
}

impl Response {
    pub const NONE: Self = Self { taken: false, rebuild: false, repaint: false };
    pub const TAKEN: Self = Self { taken: true, rebuild: false, repaint: true };
    pub const REBUILD: Self = Self { taken: true, rebuild: true, repaint: true };
}

pub trait Page: Send {
    /// Graph-first: by default the top band becomes a strip and the picture zone is tall.
    fn tall(&self) -> bool {
        false
    }

    /// The page has a MORE face (advanced controls swapped into the control zone).
    fn has_more(&self) -> bool {
        false
    }

    /// The module's internal / external key choice, as a local parameter index. The shell
    /// draws its KEY INT | EXT switch in the header for it.
    fn key_param(&self, _cx: &Ctx) -> Option<usize> {
        None
    }

    /// Every control on the page for the current mode and face. Window points.
    fn cells(&self, cx: &Ctx) -> Vec<Cell>;

    /// What never moves: plot frames, axes, captions. Cached until the page, mode, face or
    /// theme changes.
    fn paint_static(&self, _s: &mut Scene, _cx: &Ctx) {}

    /// Pictures: curves from the parameters and what moves. Painted under the cells,
    /// inside [`Page::paint_rects`], when a parameter of the module changes or a tick asks.
    fn paint(&self, _s: &mut Scene, _cx: &Ctx, _live: &Live) {}

    /// Where [`Page::paint`] draws. The picture zone by default.
    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        vec![cx.zones.picture]
    }

    /// The page wants the audio taps (spectra).
    fn wants_audio(&self) -> bool {
        false
    }

    /// How each read-out value merges between frames. Missing entries are `Merge::Last`.
    fn view_merge(&self) -> &'static [Merge] {
        &[]
    }

    /// Called every editor tick while the page is shown. True: paint again.
    fn tick(&mut self, _cx: &Ctx, _live: &Live) -> bool {
        false
    }

    /// A pointer event the controls did not take (or, for `Move`/`Up`, during a drag the
    /// page started).
    fn pointer(&mut self, _ev: Pointer, _cx: &Ctx, _io: &mut Io) -> Response {
        Response::NONE
    }

    /// The module is set up so its output can pass its own ceiling (a gain after a
    /// limiter above 0 dB, a listen mode left on). The rack chip shows a warning.
    fn warning(&self, _cx: &Ctx) -> bool {
        false
    }

    /// Tool tips for the page's own pictures.
    fn tips(&self, _cx: &Ctx) -> Vec<Tooltip> {
        Vec::new()
    }

    /// Items for the header's "..." menu, after the shell's own. Return their labels.
    fn menu(&self, _cx: &Ctx) -> Vec<String> {
        Vec::new()
    }

    /// The page's cells depend on the module's parameter `local` (a knob that changes
    /// meaning with a type, a control that appears with a mode): the shell builds the
    /// controls again when it changes.
    fn rebuild_on(&self, _local: usize, _cx: &Ctx) -> bool {
        false
    }

    /// How many views the page has that show different cells (selected band, tab, page).
    /// Over every view, with and without MORE, the cells must reach every parameter of
    /// the module; a test walks them.
    fn states(&self) -> usize {
        1
    }

    /// Shows view `i` of [`Page::states`].
    fn set_state(&mut self, _i: usize) {}

    /// One of [`Page::menu`]'s items was chosen.
    fn menu_action(&mut self, _item: usize, _cx: &Ctx, _io: &mut Io) -> Response {
        Response::NONE
    }
}
