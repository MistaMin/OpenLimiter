//! The Mode Limiter page (`design/limiters.md` section 3): a maximizer where the style
//! (ten modes, three types) and one Character amount set how the gain recovers. Core
//! picture: RECOVERY, widened over the CURVE area.
//!
//! LEARN integrates the loudness at this module's input in the editor: the shell gives no
//! loudness at a slot, so the page measures the input tap (`wants_audio`) with the BS.1770
//! meter of `src/loudness.rs`.

use olm_api::Module;
use openplugin::core::editor::Tooltip;
use openplugin::dri::goodlookindri::scene::{Point, Scene};
use goodlookindri_shell::text;

use limiter_modes::tables::STYLE_LABELS;
use limiter_modes::{learned_drive, LEARN_IDLE};

use super::limiter::{self as frame, Common, CurveExtras, Knob, Probe, BIG_Y, CEILING_X, COL, DRIVE_X, MORE_KNOB_Y, MORE_PILL_Y, RELEASE_X, ROW2};
use crate::loudness::Loudness;
use crate::ui::kit::{self, Cell, Size};
use crate::ui::page::{Ctx, Io, Live, Page, Pointer, Response, AUDIO_LEN};
use crate::ui::raster::R;

fn make() -> Box<dyn Module> {
    Box::new(limiter_modes::LimiterModes::default())
}

const LEARN_BUTTON: R = (1000.0, 638.0, 80.0, 24.0);
const LINK_PILL: R = (712.0, ROW2, 80.0, 22.0);
const CLEAR_PILL: R = (1000.0, ROW2, 80.0, 22.0);
const RESET_PILL: R = (936.0, 704.0, 144.0, 22.0);
const LEARN_READOUT: R = (936.0, 652.0, 144.0, 22.0);
/// The style grid and the LEARN column, repainted with the values.
const GRID_AREA: R = (516.0, 620.0, 568.0, 116.0);
const STYLE_READOUT: R = (808.0, 708.0, 176.0, 24.0);

/// Where style `i` is in the grid.
fn style_rect(i: usize) -> R {
    let (col, y) = match i {
        0..=4 => (i, 638.0),
        5..=9 => (i - 5, 666.0),
        _ => (i - 10, 708.0),
    };
    (COL[col], y, 80.0, 24.0)
}

struct Learn {
    meter: Loudness,
    secs: f32,
}

struct Modes {
    common: Common,
    learn: Option<Learn>,
    /// The note under LEARN after a learn ended without a reading.
    note: Option<(String, f32)>,
    /// The previous style's recovery, for two seconds after a change: (short, long, age).
    before: Option<(Vec<f32>, Vec<f32>, f32)>,
}

impl Modes {
    fn trim_max(cx: &Ctx) -> f32 {
        if cx.on(frame::local(cx, "outputLinked")) {
            frame::val(cx, "outputGain")
        } else {
            frame::val(cx, "outputGainLeft").max(frame::val(cx, "outputGainRight"))
        }
    }

    fn knobs(&self, cx: &Ctx) -> Vec<Knob> {
        if !cx.more {
            return Vec::new();
        }
        let h = |id: &str| frame::host(cx, id);
        let in_linked = cx.on(frame::local(cx, "inputLinked"));
        let out_linked = cx.on(frame::local(cx, "outputLinked"));
        let gain = |label: &str, id: &str, x: f32, dim: bool| Knob::new(cx, Size::Small, label, vec![h(id)], x, MORE_KNOB_Y).travel(-24.0, 10.0).dim(dim).boxed();
        let trim = |label: &str, id: &str, x: f32, dim: bool| {
            let v = cx.values[h(id)];
            Knob::new(cx, Size::Small, label, vec![h(id)], x, MORE_KNOB_Y).travel(-24.0, 0.0).dim(dim).over(v > 1e-4).boxed()
        };
        vec![
            gain("INPUT", "inputGain", 216.0, !in_linked),
            gain("INPUT L", "inputGainLeft", 312.0, in_linked),
            gain("INPUT R", "inputGainRight", 408.0, in_linked),
            trim("TRIM", "outputGain", 648.0, !out_linked),
            trim("TRIM L", "outputGainLeft", 744.0, out_linked),
            trim("TRIM R", "outputGainRight", 840.0, out_linked),
        ]
    }

    fn sr(cx: &Ctx) -> f32 {
        if cx.sample_rate > 1000.0 {
            cx.sample_rate
        } else {
            48_000.0
        }
    }
}

impl Page for Modes {
    fn has_more(&self) -> bool {
        true
    }

    fn wants_audio(&self) -> bool {
        true
    }

    fn rebuild_on(&self, local_index: usize, cx: &Ctx) -> bool {
        cx.more && ["inputLinked", "outputLinked"].iter().any(|id| cx.find(id) == Some(local_index))
    }

    fn cells(&self, cx: &Ctx) -> Vec<Cell> {
        let h = |id| frame::host(cx, id);
        if cx.more {
            let in_linked = cx.on(frame::local(cx, "inputLinked"));
            let out_linked = cx.on(frame::local(cx, "outputLinked"));
            let mut v = vec![
                Cell::pill(h("inputLinked"), "LINKED", (120.0, MORE_PILL_Y, 80.0, 22.0)).tip("One input gain for both channels"),
                Cell::pill(h("outputLinked"), "LINKED", (552.0, MORE_PILL_Y, 80.0, 22.0)).tip("One trim for both channels"),
                Cell::value(h("learnLoudness"), "LEARN", LEARN_READOUT)
                    .format(|v| if v <= LEARN_IDLE + 0.05 { "no learn yet".into() } else { format!("last {v:.1} LUFS") })
                    .tip("The input loudness LEARN measured; setting it sets the drive inside the module"),
            ];
            // The value boxes under the page-drawn knobs: drag, or double-click to type the
            // full range (-144 dB and up).
            for (id, x, dim) in [("inputGain", 216.0, !in_linked), ("inputGainLeft", 312.0, in_linked), ("inputGainRight", 408.0, in_linked), ("outputGain", 648.0, !out_linked), ("outputGainLeft", 744.0, out_linked), ("outputGainRight", 840.0, out_linked)] {
                v.push(Cell::value(h(id), "", (x + 8.0, MORE_KNOB_Y + 62.0, 64.0, 18.0)).dim(dim).format(|v| format!("{v:+.1} dB")));
            }
            return v;
        }
        vec![
            Cell::knob(Size::Big, h("drive"), "DRIVE", DRIVE_X, BIG_Y).format(|v| format!("{v:+.1} dB")),
            Cell::knob(Size::Big, h("ceiling"), "CEILING", CEILING_X, BIG_Y),
            Cell::knob(Size::Big, h("character"), "CHARACTER", RELEASE_X, BIG_Y).format(|v| format!("{v:.1}")).tip("Recovery from fast (0) to slow (10)"),
            Cell::value(h("learnTarget"), "TARGET", (1000.0, 666.0, 80.0, 24.0)).format(|v| format!("{v:.0} LUFS")).tip("The loudness LEARN aims for: drag or type, -32 to -1 LUFS"),
            Cell::value(h("style"), "STYLE", STYLE_READOUT).format(|v| format!("STYLE  {}", STYLE_LABELS[(v.round().max(0.0) as usize).min(STYLE_LABELS.len() - 1)])).tip("The style: click one in the grid, or drag and wheel here to step through all thirteen"),
            Cell::pill(h("truePeak"), "TRUE PEAK", (COL[0], ROW2, 80.0, 22.0)).tip("True-peak ceiling: the header latency updates"),
            Cell::pill(h("unityGain"), "GAIN MATCH", (COL[1], ROW2, 80.0, 22.0)).tip("Turns the output down to match the input loudness; never up, so it cannot pass the ceiling"),
            Cell::grip_h(h("stereoLink"), LINK_PILL).tip("Stereo link: drag sideways, 0 to 100 %"),
        ]
    }

    fn paint_static(&self, s: &mut Scene, cx: &Ctx) {
        let t = cx.t;
        frame::paint_pictures_static(s, t, cx.zones.tall, false);
        if cx.more {
            s.text(Point::new(120.0, 630.0), "MORE", t.dim, 9.0, false);
            s.text(Point::new(216.0, 630.0), "BEFORE THE LIMITER", t.dim, 9.0, false);
            s.line(Point::new(536.0, 632.0), Point::new(536.0, 764.0), t.border, 1.0);
            s.text(Point::new(648.0, 630.0), "AFTER THE LIMITER · TRIM (down only)", t.dim, 9.0, false);
            s.text(Point::new(936.0, 640.0), "LEARN", t.dim, 9.0, false);
        } else {
            frame::divider(s, t, 508.0);
            s.text(Point::new(520.0, 626.0), "MODES", t.dim, 9.0, false);
            s.text(Point::new(520.0, 698.0), "TYPES", t.dim, 9.0, false);
            s.text(Point::new(1000.0, 626.0), "LEARN", t.dim, 9.0, false);
            s.text(Point::new(436.0, 766.0), "recovery: fast to slow", t.dim, 8.0, true);
        }
    }

    fn paint(&self, s: &mut Scene, cx: &Ctx, live: &Live) {
        let t = cx.t;
        let g = frame::geo(cx.zones.tall, false);
        let extras = CurveExtras { ceiling: None, lamp: None, note: None };
        frame::paint_pictures(s, cx, live, &self.common, &extras, false);
        if let Some((short, long, _)) = &self.before {
            frame::paint_recovery(s, t, g.rec.1, short, long, t.dim, true);
        }
        let sr = Self::sr(cx);
        let out = self.common.probe.out.as_ref();
        if let Some(o) = out {
            let r = g.rec.1;
            kit::text_right(s, r.0 + r.2 - 6.0, r.1 + 10.0, &format!("look-ahead {:.0} ms", o.latency as f32 / sr * 1000.0), t.dim, 8.5);
        }
        let ceiling = frame::val(cx, "ceiling");
        frame::paint_badge(s, cx, Some(ceiling + Self::trim_max(cx)));
        if cx.more {
            for k in self.knobs(cx) {
                k.paint(s, t, cx);
            }
            let loud = frame::val(cx, "learnLoudness");
            if loud > LEARN_IDLE + 0.05 {
                s.text(Point::new(LEARN_READOUT.0 + 4.0, LEARN_READOUT.1 + 32.0), format!("drive {:+.1} dB", learned_drive(frame::val(cx, "learnTarget"), loud)), t.dim, 9.5, false);
            }
            frame::page_pill(s, t, CLEAR_PILL, "CLEAR", false, false);
            if Self::trim_max(cx) > 1e-4 {
                frame::page_pill(s, t, RESET_PILL, "RESET TO 0 dB", false, true);
            }
            return;
        }
        // The style grid: one choice of thirteen, every style one click.
        let chosen = cx.choice(frame::local(cx, "style"));
        for (i, name) in STYLE_LABELS.iter().enumerate() {
            let r = kit::rect(style_rect(i));
            let on = i == chosen;
            if on {
                s.fill_round(r, 5.0, t.accent, t.accent, true);
            } else {
                s.fill_round(r, 5.0, t.inset, t.inset, true);
                s.stroke_round(r, 5.0, t.border, 1.0);
            }
            let ink = if on { t.on_accent } else { t.text };
            s.text(Point::new(r.x + 7.0, r.y + r.h * 0.5), name.to_uppercase(), ink, 9.0, false);
            if let Some(&l) = out.and_then(|o| o.scan.get(i)) {
                let ms = format!("{:.0}", l as f32 / sr * 1000.0);
                kit::text_right(s, r.x + r.w - 6.0, r.y + r.h * 0.5, &ms, if on { t.on_accent.with_alpha(0.8) } else { t.dim }, 8.5);
            }
        }
        if out.is_some_and(|o| !o.scan.is_empty()) {
            s.text(Point::new(808.0, 698.0), "latency in ms, at this true-peak setting", t.dim, 8.0, false);
        }
        // LEARN
        match &self.learn {
            Some(l) => {
                let secs = l.secs as u32;
                kit::button(s, t, LEARN_BUTTON, &format!("{}:{:02}", secs / 60, secs % 60), true);
            }
            None => kit::button(s, t, LEARN_BUTTON, "LEARN", false),
        }
        if let Some((note, _)) = &self.note {
            kit::text_right(s, 1080.0, 698.0, note, t.over, 8.0);
        }
        // LINK, a slider pill.
        let link = frame::val(cx, "stereoLink");
        let r = kit::rect(LINK_PILL);
        s.fill_round(r, 11.0, t.inset, t.inset, true);
        let w = (r.w * link / 100.0).max(0.0);
        if w > 1.0 {
            s.fill_round(openplugin::dri::goodlookindri::scene::Rect::new(r.x, r.y, w, r.h), 11.0, t.accent.with_alpha(0.25), t.accent.with_alpha(0.25), true);
        }
        s.stroke_round(r, 11.0, t.border, 1.0);
        s.text(r.center(), format!("LINK {link:.0} %"), t.text, 9.5, true);
        let _ = text::width("", 9.0);
    }

    fn paint_rects(&self, cx: &Ctx) -> Vec<R> {
        let mut v = frame::picture_rects(cx, false);
        if cx.more {
            v.extend(self.knobs(cx).iter().map(|k| (k.rect.0 - 3.0, k.rect.1 - 3.0, k.rect.2 + 6.0, k.rect.3 + 6.0)));
            v.push((LEARN_READOUT.0, LEARN_READOUT.1 + 22.0, LEARN_READOUT.2, 20.0));
            v.push(CLEAR_PILL);
            v.push(RESET_PILL);
        } else {
            v.push(GRID_AREA);
            v.push(LINK_PILL);
        }
        v
    }

    fn tick(&mut self, cx: &Ctx, live: &Live) -> bool {
        self.common.tick(cx, live);
        let style = frame::local(cx, "style");
        if let (Some(old), Some(new)) = (self.common.probe.prev.take(), self.common.probe.out.as_ref()) {
            if old.values.get(style) != new.values.get(style) {
                self.before = Some((old.short, old.long, 0.0));
            }
        }
        if let Some(b) = &mut self.before {
            b.2 += live.dt;
            if b.2 > 2.0 {
                self.before = None;
            }
        }
        if let Some(n) = &mut self.note {
            n.1 += live.dt;
            if n.1 > 4.0 {
                self.note = None;
            }
        }
        if let (Some(l), Some(a)) = (&mut self.learn, live.audio) {
            l.secs += live.dt;
            let sr = Self::sr(cx);
            let n = ((live.dt * sr).round() as usize).min(AUDIO_LEN).min(a.pre[0].len()).min(a.pre[1].len());
            let (left, right) = (&a.pre[0][a.pre[0].len() - n..], &a.pre[1][a.pre[1].len() - n..]);
            for (x, y) in left.iter().zip(right) {
                l.meter.process(*x, *y, true);
            }
        }
        true
    }

    fn pointer(&mut self, ev: Pointer, cx: &Ctx, io: &mut Io) -> Response {
        if let Pointer::Down { x, y, .. } = ev {
            if cx.more {
                if kit::inside(CLEAR_PILL, x, y) {
                    io.set(frame::host(cx, "learnLoudness"), LEARN_IDLE);
                    return Response::TAKEN;
                }
                if Self::trim_max(cx) > 1e-4 && kit::inside(RESET_PILL, x, y) {
                    for id in ["outputGain", "outputGainLeft", "outputGainRight"] {
                        let p = frame::host(cx, id);
                        if io.value(p) > 0.0 {
                            io.set(p, 0.0);
                        }
                    }
                    return Response::TAKEN;
                }
            } else {
                if let Some(i) = (0..STYLE_LABELS.len()).find(|&i| kit::inside(style_rect(i), x, y)) {
                    io.set(frame::host(cx, "style"), i as f32);
                    return Response::TAKEN;
                }
                if kit::inside(LEARN_BUTTON, x, y) {
                    match self.learn.take() {
                        None => {
                            self.learn = Some(Learn { meter: Loudness::new(Self::sr(cx) as f64), secs: 0.0 });
                            self.note = None;
                        }
                        Some(l) => {
                            let loud = l.meter.readings().integrated;
                            if loud > LEARN_IDLE && loud.is_finite() {
                                let target = frame::val(cx, "learnTarget");
                                // The module sets its drive from the loudness; the host's drive
                                // is written to match.
                                io.set(frame::host(cx, "learnLoudness"), loud);
                                io.set(frame::host(cx, "drive"), learned_drive(target, loud));
                            } else {
                                self.note = Some(("too short or silent".into(), 0.0));
                            }
                        }
                    }
                    return Response::TAKEN;
                }
            }
        }
        let knobs = self.knobs(cx);
        self.common.pointer(ev, cx, io, &knobs)
    }

    fn warning(&self, cx: &Ctx) -> bool {
        Self::trim_max(cx) > 1e-4
    }

    fn tips(&self, cx: &Ctx) -> Vec<Tooltip> {
        let g = frame::geo(cx.zones.tall, false);
        let mut v = vec![
            frame::tip(frame::badge_rect(cx), frame::BADGE_TIP),
            frame::tip(g.maxbox, "Deepest reduction; click to reset"),
            frame::tip(g.meter, "The module's meter does not count the final clamp, so fast transients can read slightly low"),
            frame::tip(g.rec.0, "Recovery of the present style; after a change the previous style shows dashed for two seconds"),
        ];
        if !cx.more {
            v.push(frame::tip(LEARN_BUTTON, "Click to measure the input loudness, click again to set DRIVE for the target below"));
            for (i, name) in STYLE_LABELS.iter().enumerate() {
                v.push(frame::tip(style_rect(i), format!("{name}: click to select")));
            }
        } else {
            v.push(frame::tip(CLEAR_PILL, "Forget the last learn"));
        }
        v
    }
}

/// The page for this module: `count` parameters from host index `base`.
pub fn page(count: usize) -> Box<dyn Page> {
    let _ = count;
    Box::new(Modes { common: Common::new(Probe::new(make).scan(limiter_modes::P_STYLE)), learn: None, note: None, before: None })
}

/// `(listens to the sidechain key, output can pass its ceiling)` for the rack chip, from
/// the module's own values (`values[0]` is its first parameter).
pub fn chip_flags(values: &[f32]) -> (bool, bool) {
    use limiter_modes::{P_OUTPUT_GAIN, P_OUTPUT_GAIN_LEFT, P_OUTPUT_GAIN_RIGHT, P_OUTPUT_LINKED};
    let v = |i: usize| values.get(i).copied().unwrap_or(0.0);
    let trim = if v(P_OUTPUT_LINKED) >= 0.5 { v(P_OUTPUT_GAIN) } else { v(P_OUTPUT_GAIN_LEFT).max(v(P_OUTPUT_GAIN_RIGHT)) };
    (false, trim > 1e-4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trim_above_zero_warns_the_rack() {
        let mut v: Vec<f32> = make().params().iter().map(|p| p.default).collect();
        assert!(!chip_flags(&v).1);
        v[limiter_modes::P_OUTPUT_GAIN] = 2.0;
        assert!(chip_flags(&v).1);
        v[limiter_modes::P_OUTPUT_LINKED] = 0.0;
        assert!(!chip_flags(&v).1, "unlinked: the side gains count");
        v[limiter_modes::P_OUTPUT_GAIN_RIGHT] = 0.5;
        assert!(chip_flags(&v).1);
    }

    #[test]
    fn learn_measures_the_input_and_writes_loudness_and_drive() {
        use crate::ui::tests::editor;
        use crate::ui::theme::ThemeId;
        use crate::ui::LAYOUT;
        use openplugin::core::Editor;
        let (mut e, shared) = editor(ThemeId::Light);
        let slot = LAYOUT.slots.iter().position(|s| s.key == "modes").unwrap();
        e.param_changed(LAYOUT.slots[slot].on, 1.0);
        e.open_page(slot);
        shared.set_timing(0, 48_000.0);
        e.tick(1.0 / 30.0);
        let (x, y) = (LEARN_BUTTON.0 + 10.0, LEARN_BUTTON.1 + 10.0);
        e.mouse_down(x, y, 1);
        e.mouse_up(x, y);
        // Two seconds of a 1 kHz tone at -20 dBFS on both channels at the module's input.
        let w = std::f32::consts::TAU * 1000.0 / 48_000.0;
        let mut i = 0usize;
        for _ in 0..60 {
            let block: Vec<f32> = (0..1600).map(|k| 0.1 * (w * (i + k) as f32).sin()).collect();
            i += 1600;
            shared.push_taps([&block, &block], [&block, &block], None);
            e.tick(1.0 / 30.0);
        }
        e.mouse_down(x, y, 1);
        e.mouse_up(x, y);
        let base = LAYOUT.slots[slot].base;
        let loud = e.ui.values[base + limiter_modes::P_LEARN_LOUDNESS];
        // 1 kHz sine at -20 dBFS peak in both channels: about -20 LUFS.
        assert!((loud + 20.0).abs() < 1.5, "{loud}");
        let drive = e.ui.values[base + limiter_modes::P_DRIVE];
        assert!((drive - learned_drive(-11.0, loud)).abs() < 1e-3, "{drive}");
    }
}
