//! The Wall Limiter page (`design/limiters.md` section 4): a look-ahead wall in two styles,
//! manual or automatic release, an input soft clipper and an optional strict true-peak stage.

use olm_api::Module;
use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Scene};
use goodlookindri_shell::text;

use super::limiter::{self as frame, Common, CurveExtras, Probe, BIG_Y, CEILING_X, COL, DRIVE_X, RELEASE_X, ROW1, ROW2};
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response};
use crate::ui::raster::R;
use crate::INFO;

fn make() -> Box<dyn Module> {
    Box::new(limiter_wall::WallLimiter::default())
}

/// Where the moving captions go: the two brackets, and the AUTO tag on RELEASE.
const CAPTIONS: R = (604.0, 718.0, 480.0, 20.0);
const AUTO_TAG: R = (464.0, 636.0, 32.0, 13.0);

struct Wall {
    common: Common,
}

impl Wall {
    fn clipper_on(cx: &Ctx) -> bool {
        cx.on(frame::local(cx, "softClip"))
    }

    fn profile_used(cx: &Ctx) -> bool {
        cx.choice(frame::local(cx, "style")) == 0 && cx.on(frame::local(cx, "autoRelease"))
    }
}

impl Page for Wall {
    fn wants_audio(&self) -> bool {
        true
    }

    fn rebuild_on(&self, local_index: usize, cx: &Ctx) -> bool {
        ["softClip", "style", "autoRelease"].iter().any(|id| cx.find(id) == Some(local_index))
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let h = |id| frame::host(cx, id);
        let slot = cx.zones.slot;
        // Dimmed while they do nothing; the shell rebuilds the cells when a switch they
        // follow changes (`rebuild_on`).
        let clip = !Self::clipper_on(cx);
        let profile = !Self::profile_used(cx);
        let mut v = vec![
            Cell::segmented(h("style"), "STYLE", (420.0, slot.1, 176.0, 24.0)).keys(&["CONTOUR", "LEVELLER"]).tip("The header latency updates"),
            Cell::knob(Size::Big, h("drive"), "DRIVE", DRIVE_X, BIG_Y).format(|v| format!("{v:+.1} dB")),
            Cell::knob(Size::Big, h("ceiling"), "CEILING", CEILING_X, BIG_Y),
            Cell::knob(Size::Big, h("release"), "RELEASE", RELEASE_X, BIG_Y).tip("Manual release; while AUTO is on it is kept for later"),
            Cell::knob(Size::Small, h("threshold"), "THRESHOLD", COL[0], ROW1).tip("Lower threshold adds drive"),
            Cell::knob(Size::Small, h("softClipThreshold"), "CLIP AT", COL[1], ROW1).dim(clip).tip("Input clipper threshold (with SOFT CLIP on)"),
            Cell::segmented(h("softClipCurve"), "CURVE", (712.0, 662.0, 180.0, 24.0)).keys(&["WIDE", "CONT", "NARROW"]).dim(clip).tip("Input clipper knee (with SOFT CLIP on)"),
            Cell::dropdown(h("profile"), "PROFILE", (960.0, 662.0, 120.0, 24.0)).dim(profile).tip("Contour auto release only"),
            Cell::pill(h("strictTruePeak"), "STRICT TP", (COL[0], ROW2, 80.0, 22.0)).tip("A strict true-peak stage; adds 160 samples at 48 kHz"),
            Cell::pill(h("softClip"), "SOFT CLIP", (COL[1], ROW2, 80.0, 22.0)).tip("Soft clipper at the input"),
            Cell::pill(h("autoRelease"), "AUTO", (COL[5], ROW2, 80.0, 22.0)).tip("Automatic release"),
        ];
        if let Some((_, p)) = frame::geo(cx.zones.tall, true).curve {
            let c = h("ceiling");
            let info = &INFO.params[c];
            let y = |d: f32| p.1 + p.3 * (1.0 - (d - frame::CURVE_LO) / -frame::CURVE_LO);
            v.push(Cell::grip(c, (p.0 + p.2 - 40.0, y(info.max), 40.0, y(info.min) - y(info.max))).tip("Ceiling: drag the line"));
        }
        v
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        frame::paint_pictures_static(s, t, cx.zones.tall, true);
        frame::divider(s, t, 508.0);
        s.text(Point::new(712.0, 646.0), "CURVE", t.dim, 9.0, false);
        s.text(Point::new(960.0, 646.0), "PROFILE", t.dim, 9.0, false);
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let ceiling = frame::val(cx, "ceiling");
        let note = self.common.probe.out.as_ref().map(|o| format!("below limit {:+.1} dB", o.small_gain));
        let extras = CurveExtras { ceiling: Some(ceiling), lamp: None, note };
        frame::paint_pictures(s, cx, live, &self.common, &extras, true);
        frame::paint_badge(s, cx, Some(ceiling));
        let clip = Self::clipper_on(cx);
        frame::bracket(s, t, 616.0, 892.0, 722.0, if clip { "INPUT CLIPPER" } else { "INPUT CLIPPER · OFF" }, if clip { t.dim } else { t.dim.with_alpha(0.6) });
        let used = Self::profile_used(cx);
        frame::bracket(s, t, 960.0, 1080.0, 722.0, if used { "AUTO RELEASE" } else { "NOT IN USE" }, if used { t.dim } else { t.dim.with_alpha(0.6) });
        if cx.on(frame::local(cx, "autoRelease")) {
            let r = kit::rect(AUTO_TAG);
            s.fill_round(r, 6.5, t.inset, t.inset, true);
            s.stroke_round(r, 6.5, t.accent, 1.0);
            s.text(r.center(), "AUTO", t.accent, 8.0, true);
        }
        let _ = text::width("", 9.0);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let mut v = frame::picture_rects(cx, true);
        v.push(CAPTIONS);
        v.push(AUTO_TAG);
        v
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        self.common.tick(cx, live)
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        self.common.pointer(ev, cx, io, &[])
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let g = frame::geo(cx.zones.tall, true);
        vec![
            frame::tip(frame::badge_rect(cx), "The most this module can put out: its ceiling"),
            frame::tip(g.maxbox, "Deepest reduction; click to reset"),
            frame::tip(g.rec.0, "Gain recovery after a 6 dB overshoot; follows AUTO, PROFILE and STYLE"),
            frame::tip(AUTO_TAG, "Automatic release is on: the knob keeps the manual value for later"),
        ]
    }
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    Box::new(Wall { common: Common::new(Probe::new(make)) })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter). There is no gain after
/// the wall.
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    let _ = values;
    (false, false)
}
