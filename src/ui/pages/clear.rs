//! The Clear Limiter page (`design/limiters.md` section 2): a transparent peak limiter whose
//! colour comes from knee, Lift and the peak guard. Core picture: CURVE.

use olm_api::Module;
use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Scene};

use super::limiter::{self as frame, Common, CurveExtras, Probe, BIG_Y, CEILING_X, COL, DRIVE_X, MORE_KNOB_Y, RELEASE_X, ROW1, ROW2};
use crate::ui::kit::{Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::INFO;

fn make() -> Box<dyn Module> {
    Box::new(limiter_clear::ClearLimiter::default())
}

struct Clear {
    common: Common,
}

impl Page for Clear {
    fn has_more(&self) -> bool {
        true
    }

    fn wants_audio(&self) -> bool {
        true
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let h = |id| frame::host(cx, id);
        if cx.more {
            return vec![Cell::knob(Size::Small, h("outputTrim"), "TRIM", 408.0, MORE_KNOB_Y).tip("Trim after the limiter, down only")];
        }
        let mut v = vec![
            Cell::knob(Size::Big, h("inputGain"), "DRIVE", DRIVE_X, BIG_Y).format(|v| format!("{v:+.1} dB")),
            Cell::knob(Size::Big, h("threshold"), "CEILING", CEILING_X, BIG_Y).tip("The output limit; there is no make-up"),
            Cell::knob(Size::Big, h("release"), "RELEASE", RELEASE_X, BIG_Y),
            Cell::knob(Size::Small, h("attack"), "ATTACK", COL[0], ROW1),
            Cell::knob(Size::Small, h("knee"), "KNEE", COL[1], ROW1),
            Cell::knob(Size::Small, h("lift"), "LIFT", COL[2], ROW1).tip("Raises everything below the ceiling; above 100 % it adds shaping"),
            Cell::pill(h("truePeak"), "TRUE PEAK", (COL[0], ROW2, 80.0, 22.0)).tip("Holds the ceiling on inter-sample peaks"),
            Cell::pill(h("peakGuard"), "PEAK GUARD", (COL[1], ROW2, 80.0, 22.0)).tip("No attack overshoot above the ceiling; CURVE shows the other state dashed"),
            Cell::pill(h("slowHold"), "SLOW HOLD", (COL[5], ROW2, 80.0, 22.0)).tip("The slow, programme-dependent recovery"),
        ];
        if let Some((_, p)) = frame::geo(cx.zones.tall, true).curve {
            let th = h("threshold");
            let info = &INFO.params[th];
            let y = |d: f32| p.1 + p.3 * (1.0 - (d - frame::CURVE_LO) / -frame::CURVE_LO);
            let (top, bottom) = (y(info.max), y(info.min));
            v.push(Cell::grip(th, (p.0 + p.2 - 40.0, top, 40.0, bottom - top)).tip("Ceiling: drag the line"));
        }
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        frame::paint_pictures_static(s, t, cx.zones.tall, true);
        if cx.more {
            s.text(Point::new(120.0, 630.0), "MORE", t.dim, 9.0, false);
            s.text(Point::new(408.0, 630.0), "AFTER THE LIMITER · TRIM (down only)", t.dim, 9.0, false);
        } else {
            frame::divider(s, t, 508.0);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let ceiling = frame::val(cx, "threshold");
        let lift = frame::val(cx, "lift");
        let note = (lift > 0.0).then(|| format!("lift {lift:.0} %{}", if lift > 100.0 { ", shaping" } else { "" }));
        let extras = CurveExtras { ceiling: Some(ceiling), lamp: None, note };
        frame::paint_pictures(s, cx, live, &self.common, &extras, true);
        frame::paint_badge(s, cx, Some(ceiling + frame::val(cx, "outputTrim")));
        if let Some((_, p)) = frame::geo(cx.zones.tall, true).curve {
            if self.common.probe.out.as_ref().is_some_and(|o| !o.alt.is_empty()) {
                let other = if cx.on(frame::local(cx, "peakGuard")) { "dashed: guard off" } else { "dashed: guard on" };
                s.text(Point::new(p.0 + 6.0, p.1 + 22.0), other, t_dim(cx), 8.0, false);
            }
        }
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        frame::picture_rects(cx, true)
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        self.common.tick(cx, live)
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        self.common.pointer(ev, cx, io, &[])
    }

    fn warning(&self, cx: &Ctx) -> bool {
        frame::val(cx, "outputTrim") > 1e-4
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let g = frame::geo(cx.zones.tall, true);
        vec![
            frame::tip(frame::badge_rect(cx), frame::BADGE_TIP),
            frame::tip(g.maxbox, "Deepest reduction; click to reset"),
            frame::tip(g.rec.0, "Thin: the fast release after a 5 ms peak. Thick: the Slow Hold tail after a 500 ms passage"),
            frame::tip(g.meter, "The module's own meter: instant rise, 20 dB/s fall"),
        ]
    }
}

fn t_dim(cx: &Ctx) -> openplugin::dri::goodlookindri::color::Color {
    cx.t.dim
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    Box::new(Clear { common: Common::new(Probe::new(make).alt(limiter_clear::P_PEAK_GUARD)) })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). The trim ends at 0 dB.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    (false, values.get(limiter_clear::P_OUTPUT_TRIM).is_some_and(|&v| v > 1e-4))
}
