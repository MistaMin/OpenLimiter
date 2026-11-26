//! The page every module has until it gets its own: its reduction over time, a meter, and
//! its parameters in a row of small knobs, pills and lists, a page at a time.

use openplugin::core::Unit;
use openplugin::dri::goodlookindri::scene::{Point, Scene};

use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::INFO;

/// Columns of the control row.
const COLS: usize = 12;

pub struct Generic {
    /// Local indices the page lists, in order.
    params: Vec<usize>,
    page: usize,
}

impl Generic {
    /// Lists every parameter of a module of `count` parameters but those in `hidden`.
    pub fn new(count: usize, hidden: &[usize]) -> Self {
        Self { params: (0..count).filter(|i| !hidden.contains(i)).collect(), page: 0 }
    }

    fn pages(&self) -> usize {
        self.params.len().div_ceil(COLS).max(1)
    }

    fn arrows(cx: &Ctx) -> (crate::ui::raster::R, crate::ui::raster::R) {
        let (x, y, w, _) = cx.zones.slot;
        ((x + w - 76.0, y, 24.0, 24.0), (x + w - 24.0, y, 24.0, 24.0))
    }

    fn trace(cx: &Ctx) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = cx.zones.picture;
        (x + 32.0, y + 8.0, w - 32.0 - 60.0, h - 32.0)
    }

    fn meter(cx: &Ctx) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = cx.zones.picture;
        (x + w - 36.0, y + 8.0, 36.0, h - 32.0)
    }
}

/// The range the trace and meter show, dB.
const RANGE: f32 = 12.0;

impl Page for Generic {
    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let (x0, y0, w, _) = cx.zones.control;
        let pitch = w / COLS as f32;
        let mut cells = Vec::new();
        for (k, &local) in self.params.iter().skip(self.page * COLS).take(COLS).enumerate() {
            let index = cx.p(local);
            let info = &INFO.params[index];
            let x = x0 + k as f32 * pitch;
            let label = info.name.strip_prefix(crate::ui::slot_label(cx.slot)).map_or(info.name, str::trim_start);
            let cell = if info.unit == Unit::Toggle {
                Cell::pill(index, label, (x + 4.0, y0 + 44.0, pitch - 8.0, 22.0))
            } else if !info.choices.is_empty() {
                Cell::dropdown(index, label, (x + 2.0, y0 + 44.0, pitch - 4.0, 24.0))
            } else {
                Cell::knob(Size::Small, index, label, x + (pitch - 80.0) * 0.5, y0 + 12.0)
            };
            cells.push(cell);
        }
        cells
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        let r = Self::trace(cx);
        kit::plot(s, t, r);
        for d in [3.0, 6.0, 9.0] {
            let y = r.1 + d / RANGE * r.3;
            s.line(Point::new(r.0, y), Point::new(r.0 + r.2, y), t.grid, 1.0);
            kit::text_right(s, r.0 - 4.0, y, &format!("-{d:.0}"), t.dim, 8.5);
        }
        s.text(Point::new(r.0 + 8.0, r.1 + 12.0), "REDUCTION, LAST 4 s", t.dim, 9.0, false);
        let m = Self::meter(cx);
        kit::gr_scale(s, t, m, RANGE);
        if self.pages() > 1 {
            let (a, b) = Self::arrows(cx);
            for (r, dir) in [(a, -1.0f32), (b, 1.0)] {
                kit::button(s, t, r, "", false);
                let (cx_, cy) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5);
                s.stroke_poly(vec![Point::new(cx_ - 2.0 * dir, cy - 4.0), Point::new(cx_ + 2.0 * dir, cy), Point::new(cx_ - 2.0 * dir, cy + 4.0)], t.text, 1.4, false);
            }
            s.text(Point::new(a.0 + 38.0, a.1 + 12.0), format!("{} / {}", self.page + 1, self.pages()), t.dim, 9.0, true);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let r = Self::trace(cx);
        let n = live.gr_hist.len();
        if n >= 2 {
            let span = (live.hist_rate * 4.0).max(2.0);
            let pts: Vec<Point> = live
                .gr_hist
                .iter()
                .enumerate()
                .map(|(i, g)| Point::new(r.0 + r.2 * (1.0 - (n - 1 - i) as f32 / span).max(0.0), r.1 + (g / RANGE).clamp(0.0, 1.0) * r.3))
                .collect();
            kit::area(s, &pts, r.1, t.accent.with_alpha(0.22));
            kit::curve(s, pts, t.accent, 1.5);
        }
        let m = Self::meter(cx);
        let hold = live.gr_hist.iter().rev().take((live.hist_rate * 1.5) as usize).fold(0.0f32, |a, &b| a.max(b));
        kit::gr_meter(s, t, m, live.gr, hold, RANGE);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<(f32, f32, f32, f32)> {
        vec![Self::trace(cx), Self::meter(cx)]
    }

    fn tick(&mut self, _cx: &Ctx, _live: &Live) -> bool {
        true
    }

    fn states(&self) -> usize {
        self.pages()
    }

    fn set_state(&mut self, i: usize) {
        self.page = i % self.pages();
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, _io: &mut Io) -> Response {
        if let Pointer::Down { x, y, .. } = ev {
            if self.pages() > 1 {
                let (a, b) = Self::arrows(cx);
                let step = if kit::inside(a, x, y) {
                    -1
                } else if kit::inside(b, x, y) {
                    1
                } else {
                    0
                };
                if step != 0 {
                    self.page = (self.page as isize + step).rem_euclid(self.pages() as isize) as usize;
                    return Response::REBUILD;
                }
            }
        }
        Response::NONE
    }
}
