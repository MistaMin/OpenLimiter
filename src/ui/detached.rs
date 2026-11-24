//! The history or the loudness radar in a window of its own, at any size. It reads only the
//! shared rings (never the `take_*` values the main editor consumes), so both can run at once,
//! and it keeps running with the main editor closed.

use openplugin::dri::goodlookindri_shell::text::{self as text, GlyphCache};

use super::live::{paint_plot, paint_radar, plot_y, radar_lut};
use super::raster::{rgb, Align, Target, R};
use super::theme::{theme, ThemeId};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    History,
    Radar,
}

pub struct Detached {
    kind: Kind,
    pub(super) ui: Ui,
    shared: Arc<Shared>,
    read: usize,
    loud_read: usize,
    size: (u32, u32),
    out: Vec<u32>,
    glyphs: GlyphCache,
    lut: Vec<(f32, f32)>,
    lut_key: (u32, u32, u32),
    dirty: bool,
    /// New data arrived but is not drawn yet; seconds since the last frame and its cost.
    pending: bool,
    since: f32,
    cost: f32,
}

/// The mode switch in the history window's header, from its right edge.
const MODE_W: f32 = 156.0;

impl Detached {
    pub fn new(kind: Kind, shared: Arc<Shared>) -> Self {
        let (theme_i, target) = shared.look().unwrap_or((0, load_target()));
        let mut ui = OpenLimiterEditor::with_theme(EditorHost::detached(), ThemeId::from_index(theme_i)).ui;
        ui.target = target;
        ui.hist_mode = load_hist_mode("pop-histmode");
        let size = match kind {
            Kind::History => (900, 360),
            Kind::Radar => (520, 560),
        };
        // Start with what the rings already hold, so the window opens full.
        let read = shared.head().wrapping_sub(WINDOW.min(shared.head()));
        let loud_read = shared.loud_head().wrapping_sub(RADAR_SLOTS.min(shared.loud_head()));
        Self { kind, ui, shared, read, loud_read, size, out: Vec::new(), glyphs: GlyphCache::default(), lut: Vec::new(), lut_key: (0, 0, 0), dirty: true, pending: false, since: 0.0, cost: 0.0 }
    }

    pub fn title(kind: Kind) -> &'static str {
        match kind {
            Kind::History => "OpenLimiter History",
            Kind::Radar => "OpenLimiter Loudness Radar",
        }
    }

    fn plot(&self) -> R {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        (44.0, 30.0, w - 88.0, h - 54.0)
    }

    fn radar(&self) -> ((f32, f32), f32) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let r = ((w - 40.0).min(h - 76.0) * 0.5).max(40.0);
        ((w * 0.5, 30.0 + (h - 60.0) * 0.5), r)
    }

    fn mode_rect(&self) -> R {
        (self.size.0 as f32 - MODE_W - 12.0, 5.0, MODE_W, 18.0)
    }
}

impl Editor for Detached {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn size_range(&self) -> SizeRange {
        let min = match self.kind {
            Kind::History => (420, 180),
            Kind::Radar => (240, 280),
        };
        SizeRange::between(min, (4096, 4096))
    }

    fn resize(&mut self, width: u32, height: u32) {
        if (width, height) != self.size {
            self.size = (width, height);
            self.dirty = true;
        }
    }

    fn param_changed(&mut self, _index: usize, _plain: f32) {}

    fn mouse_down(&mut self, x: f32, y: f32, _clicks: u32) {
        let r = self.mode_rect();
        if self.kind == Kind::History && inside(r, x, y) {
            let mode = (((x - r.0) / (r.2 / 3.0)) as u8).min(2);
            if mode != self.ui.hist_mode {
                self.ui.hist_mode = mode;
                store_hist_mode("pop-histmode", mode);
                self.dirty = true;
            }
        }
    }

    fn tick(&mut self, dt: f32) {
        self.since += dt;
        if let Some((theme_i, target)) = self.shared.look() {
            let theme_id = ThemeId::from_index(theme_i);
            if (theme_id, target) != (self.ui.theme, self.ui.target) {
                (self.ui.theme, self.ui.target) = (theme_id, target);
                self.dirty = true;
            }
        }
        let shared = self.shared.clone();
        for (i, v) in self.ui.values.iter_mut().enumerate() {
            if let Some(now) = shared.value(i).filter(|now| now != v) {
                *v = now;
                self.dirty = true;
            }
        }
        let ui = &mut self.ui;
        match self.kind {
            Kind::History => {
                let head = shared.head();
                if head.wrapping_sub(self.read) > HIST {
                    self.read = head.wrapping_sub(HIST);
                }
                while self.read != head {
                    if ui.history.len() == WINDOW {
                        ui.history.pop_front();
                    }
                    ui.history.push_back(shared.entry(self.read));
                    ui.hist_count += 1;
                    self.read = self.read.wrapping_add(1);
                    self.pending = true;
                }
                if ui.hist_mode != 0 && ui.spec.tick(&shared, dt) {
                    self.pending = true;
                }
            }
            Kind::Radar => {
                let head = shared.loud_head();
                if head.wrapping_sub(self.loud_read) > LOUD_HIST {
                    self.loud_read = head.wrapping_sub(LOUD_HIST);
                }
                while self.loud_read != head {
                    if ui.loud_hist.len() == RADAR_SLOTS {
                        ui.loud_hist.pop_front();
                    }
                    ui.loud_hist.push_back(shared.loud_entry(self.loud_read));
                    ui.loud_count += 1;
                    self.loud_read = self.loud_read.wrapping_add(1);
                    self.pending = true;
                }
            }
        }
        // A big window costs tens of ms a frame: spend at most a quarter of the thread on it.
        if self.pending && self.since >= 3.0 * self.cost {
            self.dirty = true;
        }
    }

    fn damage(&mut self) -> Option<Vec<DamageRect>> {
        if self.dirty { None } else { Some(Vec::new()) }
    }

    fn render(&mut self, scale: f32) -> Frame<'_> {
        let start = std::time::Instant::now();
        let k = scale;
        let (w, h) = (((self.size.0 as f32 * k).round() as usize).max(1), ((self.size.1 as f32 * k).round() as usize).max(1));
        let t = theme(self.ui.theme);
        self.out.clear();
        self.out.resize(w * h, rgb(t.panel));
        let (p, (c, r), mode_r) = (self.plot(), self.radar(), self.mode_rect());
        let mut f = Target { px: &mut self.out, w, h };
        let g = &mut self.glyphs;
        let right = |g: &mut GlyphCache, f: &mut Target, x: f32, y: f32, s: &str, size: f32, c| {
            f.text(g, (x - text::width(s, size)) * k, y * k, s, size * k, c, Align::Left);
        };
        match self.kind {
            Kind::History => {
                let mode = self.ui.hist_mode;
                f.fill((p.0 * k) as usize, (p.1 * k) as usize, ((p.0 + p.2) * k) as usize, ((p.1 + p.3) * k) as usize, rgb(t.plot_bg));
                f.text(g, 14.0 * k, 14.0 * k, "HISTORY", 11.0 * k, t.dim, Align::Left);
                match mode {
                    0 => {
                        paint_plot(&self.ui, &mut f, t, k, p, true);
                        let mut lx = 110.0;
                        for (label, colour) in [("IN", t.in_line), ("OUT", t.out_line), ("GR", t.accent)] {
                            f.round((lx * k, 11.0 * k, 12.0 * k, 6.0 * k), 2.0 * k, 0.0, colour, 1.0);
                            f.text(g, (lx + 17.0) * k, 14.0 * k, label, 10.0 * k, t.dim, Align::Left);
                            lx += 30.0 + text::width(label, 10.0) + 10.0;
                        }
                        let mut level = PLOT_TOP_DB;
                        while level >= PLOT_TOP_DB - PLOT_SPAN_DB {
                            let y = p.1 + plot_y(level, p);
                            let label = if level == 0.0 { "0".to_string() } else { format!("{level:+.0}") };
                            right(g, &mut f, p.0 - 6.0, y, &label, 10.0, if level == 0.0 { t.text } else { t.dim });
                            let depth = PLOT_TOP_DB - level;
                            let label = if depth == 0.0 { "0".to_string() } else { format!("-{depth:.0}") };
                            f.text(g, (p.0 + p.2 + 6.0) * k, y * k, &label, 10.0 * k, t.accent, Align::Left);
                            level -= 6.0;
                        }
                    }
                    1 => spectro::paint_2d(&self.ui.spec, &mut f, t, k, p),
                    _ => spectro::paint_3d(&self.ui.spec, &mut f, t, k, p),
                }
                if mode != 0 {
                    f.text(g, 110.0 * k, 14.0 * k, "OUTPUT SPECTRUM", 10.0 * k, t.dim, Align::Left);
                    for (x, y, s, r) in spectro::labels(mode, p) {
                        if r {
                            right(g, &mut f, x, y, &s, 10.0, t.dim);
                        } else {
                            f.text(g, (x - text::width(&s, 10.0) * 0.5) * k, y * k, &s, 10.0 * k, t.dim, Align::Left);
                        }
                    }
                }
                if mode != 2 {
                    for sec in 0..=WINDOW_S as usize {
                        let x = p.0 + p.2 * (1.0 - sec as f32 / WINDOW_S);
                        let y = p.1 + p.3 + 16.0;
                        if sec == 0 {
                            right(g, &mut f, x, y, "now", 10.0, t.dim);
                        } else {
                            let s = format!("-{sec} s");
                            let x = if sec == WINDOW_S as usize { x } else { x - text::width(&s, 10.0) * 0.5 };
                            f.text(g, x * k, y * k, &s, 10.0 * k, t.dim, Align::Left);
                        }
                    }
                }
                // LEVELS | 2D | 3D
                f.round((mode_r.0 * k, mode_r.1 * k, mode_r.2 * k, mode_r.3 * k), 6.0 * k, 0.0, t.inset, 1.0);
                let cw = mode_r.2 / 3.0;
                for (i, label) in ["LEVELS", "2D", "3D"].iter().enumerate() {
                    let on = i == mode as usize;
                    let x = mode_r.0 + cw * i as f32;
                    if on {
                        f.round(((x + 2.0) * k, (mode_r.1 + 2.0) * k, (cw - 4.0) * k, (mode_r.3 - 4.0) * k), 4.0 * k, 0.0, t.accent, 1.0);
                    }
                    f.text(g, (x + cw * 0.5) * k, (mode_r.1 + mode_r.3 * 0.5) * k, label, 9.0 * k, if on { t.on_accent } else { t.dim }, Align::Centre);
                }
            }
            Kind::Radar => {
                let key = ((c.0 * 8.0) as u32 ^ ((c.1 * 8.0) as u32) << 16, (r * 8.0) as u32, k.to_bits());
                if key != self.lut_key {
                    self.lut = radar_lut(k, c, r);
                    self.lut_key = key;
                }
                f.round(((c.0 - r - 4.0) * k, (c.1 - r - 4.0) * k, (2.0 * r + 8.0) * k, (2.0 * r + 8.0) * k), (r + 4.0) * k, 1.5 * k, t.border, 1.0);
                paint_radar(&self.ui, &mut f, g, t, k, &self.lut, (c, r));
                f.text(g, 14.0 * k, 14.0 * k, "LOUDNESS RADAR", 11.0 * k, t.dim, Align::Left);
                let y = self.size.1 as f32 - 12.0;
                let s = format!("{:.0} s PER TURN · TARGET {:.0} LUFS", RADAR_SLOTS as f32 / crate::shared::LOUD_RATE, self.ui.target());
                f.text(g, 14.0 * k, y * k, &s, 10.0 * k, t.dim, Align::Left);
                let x = self.size.0 as f32 - 104.0;
                f.round((x * k, (y - 3.0) * k, 12.0 * k, 6.0 * k), 2.0 * k, 0.0, t.ok, 1.0);
                f.text(g, (x + 17.0) * k, y * k, "OUT", 10.0 * k, t.dim, Align::Left);
                f.round(((x + 50.0) * k, (y - 0.75) * k, 12.0 * k, 1.5 * k), 0.0, 0.0, t.text, 1.0);
                f.text(g, (x + 67.0) * k, y * k, "IN", 10.0 * k, t.dim, Align::Left);
            }
        }
        self.dirty = false;
        self.pending = false;
        self.since = 0.0;
        self.cost = start.elapsed().as_secs_f32();
        Frame { pixels: &self.out, width: w, height: h }
    }
}

/// The history's mode, 0 levels, 1 2D spectrum, 2 3D spectrum, from the preference `name`.
pub fn load_hist_mode(name: &str) -> u8 {
    // Tests must not depend on the user's preferences.
    if cfg!(test) {
        return 0;
    }
    theme::pref_file(name).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| s.trim().parse::<u8>().ok()).unwrap_or(0).min(2)
}

pub fn store_hist_mode(name: &str, mode: u8) {
    if cfg!(test) {
        return;
    }
    if let Some(path) = theme::pref_file(name) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, mode.to_string());
    }
}
