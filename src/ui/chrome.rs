//! The shell's drawing in the canvas: the cached layer and what follows the parameters.

use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};
use openplugin::dri::goodlookindri_shell::text;
use openplugin::dri::DrawList;

use super::kit::{self, button, caption, inset, panel, rect, text_right};
use super::theme::{theme, Theme};
use super::*;

pub fn paint_static(ui: &Ui, dl: &mut DrawList) {
    let t = theme(ui.theme);
    let mut s = Scene::new();
    s.fill_rect(Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32), t.bg, t.bg, true);
    header(&mut s, t, ui);
    rails(&mut s, t);
    if ui.setup_open {
        setup_sheet(&mut s, t, ui);
    } else {
        band(&mut s, t, ui);
        rack_frame(&mut s, t, ui);
    }
    page_frame(&mut s, t, ui);
    loud_bar(&mut s, t, ui);
    for cell in &ui.cells {
        kit::paint_static(&mut s, t, cell);
    }
    dl.append(s);
}

fn header(s: &mut Scene, t: &Theme, ui: &Ui) {
    s.fill_round(Rect::new(16.0, 10.0, 4.0, 28.0), 2.0, t.accent, t.accent, true);
    s.text(Point::new(28.0, 20.0), "OPENLIMITER", t.text, 16.0, false);
    s.text(Point::new(28.0, 34.0), "MODULAR MASTERING CHAIN", t.dim, 8.0, false);
    text_right(s, PRESET_PREV.0 - 10.0, 24.0, "CHAIN PRESET", t.dim, 9.0);
    for (r, dir) in [(PRESET_PREV, -1.0f32), (PRESET_NEXT, 1.0)] {
        button(s, t, r, "", false);
        let (cx, cy) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5);
        s.stroke_poly(vec![Point::new(cx - 2.5 * dir, cy - 5.0), Point::new(cx + 2.5 * dir, cy), Point::new(cx - 2.5 * dir, cy + 5.0)], t.text, 1.6, false);
    }
    inset(s, t, PRESET_BOX, 6.0);
    inset(s, t, AB, 6.0);
    button(s, t, SETUP_BUTTON, "", ui.setup_open);
    s.text(Point::new(SETUP_BUTTON.0 + SETUP_BUTTON.2 * 0.5 + 4.0, SETUP_BUTTON.1 + SETUP_BUTTON.3 * 0.5), "SETUP", if ui.setup_open { t.on_accent } else { t.text }, 10.0, true);
    button(s, t, THEME_BUTTON, "", false);
    let (tx, ty) = (THEME_BUTTON.0, THEME_BUTTON.1 + THEME_BUTTON.3 * 0.5);
    s.fill_ellipse_solid(Scene::ellipse_at(Point::new(tx + 16.0, ty), 5.0), t.accent);
    if ui.theme.is_dark() {
        s.fill_ellipse_solid(Scene::ellipse_at(Point::new(tx + 18.5, ty - 2.0), 4.0), t.panel);
    }
    s.text(Point::new(tx + 58.0, ty), ui.theme.name().to_uppercase(), t.text, 10.0, true);
}

fn rails(s: &mut Scene, t: &Theme) {
    panel(s, t, LEFT_RAIL);
    panel(s, t, RIGHT_RAIL);
    s.text(Point::new(IN_PPM[0].0 + (IN_PPM[1].0 + IN_PPM[1].2 - IN_PPM[0].0) * 0.5, 70.0), "IN", t.dim, 9.0, true);
    s.text(Point::new(OUT_PPM[0].0 + (OUT_PPM[1].0 + OUT_PPM[1].2 - OUT_PPM[0].0) * 0.5, 70.0), "OUT", t.dim, 9.0, true);
    s.text(Point::new(CHAIN_GR.0 + 7.0, 70.0), "GR", t.accent, 9.0, true);
    for level in [6.0f32, 0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0] {
        let y = IN_PPM[0].1 + (METER_TOP_DB - level) / METER_SPAN_DB * IN_PPM[0].3;
        let label = if level > 0.0 { format!("+{level:.0}") } else { format!("{level:.0}") };
        // Text ink, not the caption grey: the dim grey all but vanished at 8 pt on the light theme.
        let ink = if level == 0.0 { t.text } else { t.text.with_alpha(0.72) };
        s.text(Point::new(62.0, y), label.clone(), ink, 8.0, false);
        text_right(s, OUT_PPM[0].0 - 3.0, y, &label, ink, 8.0);
    }
    for d in [0.0f32, 6.0, 12.0, 18.0, 24.0] {
        let y = CHAIN_GR.1 + d / GR_METER_SPAN_DB * CHAIN_GR.3;
        s.text(Point::new(CHAIN_GR.0 + 17.0, y), format!("{d:.0}"), t.accent, 8.0, false);
    }
    for r in [IN_PEAK, OUT_PEAK] {
        inset(s, t, r, 5.0);
    }
    s.text(Point::new(LEFT_RAIL.0 + 40.0, 562.0), "INPUT GAIN", t.dim, 8.5, true);
    caption(s, t, TP_BOX.0 + 2.0, TP_BOX.1 + 6.0, "TRUE PEAK");
    caption(s, t, DITHER_BOX.0 + 2.0, DITHER_BOX.1 + 6.0, "REQUANTISE");
    caption(s, t, LATENCY_BOX.0 + 2.0, LATENCY_BOX.1 + 6.0, "LATENCY");
    s.line(Point::new(1110.0, 598.0), Point::new(1178.0, 598.0), t.border, 1.0);
    s.line(Point::new(1110.0, 652.0), Point::new(1178.0, 652.0), t.border, 1.0);
    s.line(Point::new(1110.0, 724.0), Point::new(1178.0, 724.0), t.border, 1.0);
}

fn chevron(s: &mut Scene, t: &Theme, up: bool) {
    let r = CHEVRON;
    kit::inset(s, t, r, 5.0);
    let (cx, cy) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5);
    let d = if up { -1.0 } else { 1.0 };
    s.stroke_poly(vec![Point::new(cx - 5.0, cy - 2.0 * d), Point::new(cx, cy + 2.0 * d), Point::new(cx + 5.0, cy - 2.0 * d)], t.text, 1.5, false);
}

fn band(s: &mut Scene, t: &Theme, ui: &Ui) {
    if ui.tall {
        panel(s, t, BAND_STRIP);
        s.fill_rect(rect(super::RIBBON), t.plot_bg, t.plot_bg, true);
        s.fill_rect(rect(super::LOUD_RIBBON), t.plot_bg, t.plot_bg, true);
        chevron(s, t, false);
        return;
    }
    panel(s, t, HIST_PANEL);
    panel(s, t, RADAR_PANEL);
    title(s, t, HIST_PANEL, "HISTORY");
    let mut lx = 196.0;
    for (label, colour) in [("IN", t.in_line), ("OUT", t.out_line), ("GR", t.accent)] {
        s.fill_round(Rect::new(lx, 67.0, 12.0, 6.0), 2.0, colour, colour, true);
        s.text(Point::new(lx + 17.0, 70.0), label, t.dim, 9.0, false);
        lx += 26.0 + text::width(label, 9.0) + 10.0;
    }
    s.fill_rect(rect(PLOT), t.plot_bg, t.plot_bg, true);
    let mut level = PLOT_TOP_DB;
    while level >= PLOT_TOP_DB - PLOT_SPAN_DB {
        let y = PLOT.1 + live::plot_y(level, PLOT);
        let label = if level == 0.0 { "0".to_string() } else { format!("{level:+.0}") };
        text_right(s, PLOT.0 - 6.0, y, &label, if level == 0.0 { t.text } else { t.dim }, 9.0);
        let depth = PLOT_TOP_DB - level;
        let label = if depth == 0.0 { "0".to_string() } else { format!("-{depth:.0}") };
        s.text(Point::new(PLOT.0 + PLOT.2 + 6.0, y), label, t.accent.with_alpha(0.75), 9.0, false);
        level -= 6.0;
    }
    for sec in 0..=WINDOW_S as usize {
        let x = PLOT.0 + PLOT.2 * (1.0 - sec as f32 / WINDOW_S);
        let label = if sec == 0 { "now".to_string() } else { format!("-{sec} s") };
        let y = PLOT.1 + PLOT.3 + 13.0;
        if sec == 0 {
            text_right(s, x, y, &label, t.dim, 9.0);
        } else {
            s.text(Point::new(x, y), label, t.dim, 9.0, sec != WINDOW_S as usize);
        }
    }
    title(s, t, RADAR_PANEL, "LOUDNESS RADAR");
    let centre = Point::new(RADAR_C.0, RADAR_C.1);
    s.stroke_ellipse(Scene::ellipse_at(centre, RADAR_R + 4.0), t.border, 1.5);
    for i in 0..6 {
        let a = i as f32 / 6.0 * std::f32::consts::TAU;
        let (dx, dy) = (a.sin(), -a.cos());
        s.line(Point::new(centre.x + dx * (RADAR_R + 4.0), centre.y + dy * (RADAR_R + 4.0)), Point::new(centre.x + dx * (RADAR_R + 9.0), centre.y + dy * (RADAR_R + 9.0)), t.grid_strong, 1.0);
    }
    let y = RADAR_PANEL.1 + RADAR_PANEL.3 - 12.0;
    s.text(Point::new(RADAR_PANEL.0 + 14.0, y), format!("{:.0} s PER TURN", RADAR_SLOTS as f32 / crate::shared::LOUD_RATE), t.dim, 8.5, false);
    let x = RADAR_PANEL.0 + RADAR_PANEL.2 - 98.0;
    s.fill_round(Rect::new(x, y - 3.0, 12.0, 6.0), 2.0, t.ok, t.ok, true);
    s.text(Point::new(x + 17.0, y), "OUT", t.dim, 8.5, false);
    s.line(Point::new(x + 46.0, y), Point::new(x + 58.0, y), t.text, 1.5);
    s.text(Point::new(x + 63.0, y), "IN", t.dim, 8.5, false);
    chevron(s, t, true);
}

fn title(s: &mut Scene, t: &Theme, panel: R, label: &str) {
    s.fill_round(Rect::new(panel.0 + 12.0, panel.1 + 9.0, 3.0, 10.0), 1.5, t.accent, t.accent, true);
    s.text(Point::new(panel.0 + 20.0, panel.1 + 14.0), label, t.dim, 9.0, false);
}

fn rack_frame(s: &mut Scene, t: &Theme, ui: &Ui) {
    let rack = ui.rack();
    panel(s, t, rack.rect);
    let y = rack.rect.1 + rack.rect.3 * 0.5;
    s.line(Point::new(rack.input.0 + rack.input.2, y), Point::new(rack.output.0, y), t.grid_strong, 1.0);
    for (r, label) in [(rack.input, "IN"), (rack.output, "OUT")] {
        s.text(rect(r).center(), label, t.dim, 9.0, true);
    }
}

/// The page's one-line description as the header shows it: whole, cut where a phrase
/// ends, or not at all, in the room the page leaves between the name and its controls.
pub(crate) fn header_line(ui: &Ui) -> Option<String> {
    let key = ui.slot().map_or("dither", |s| LAYOUT.slots[s].key);
    let (name, line) = if ui.slot().is_none() { ("REQUANTISE", "Word-length reduction and dither, always last") } else { (slot_label(ui.slot()), module_names(key).1) };
    let x = if ui.slot().is_some() { 172.0 } else { 120.0 };
    let after = x + text::width(&name.to_uppercase(), 13.0) + 12.0;
    // The latency text (up to "88888 smp") on module pages, else the menu button.
    let end = if ui.slot().is_some() { 980.0 - text::width("88888 smp", 10.0) } else { 988.0 };
    let free_until = header_taken(ui).iter().filter(|(l, _)| *l >= after).fold(end, |a, (l, _)| a.min(*l));
    fit_words(line, 10.5, free_until - 12.0 - after)
}

fn page_frame(s: &mut Scene, t: &Theme, ui: &Ui) {
    let z = ui.zones();
    panel(s, t, z.page);
    s.line(Point::new(z.page.0 + 12.0, z.header.1 + z.header.3), Point::new(z.page.0 + z.page.2 - 12.0, z.header.1 + z.header.3), t.border, 1.0);
    let key = ui.slot().map_or("dither", |s| LAYOUT.slots[s].key);
    let (name, line) = if ui.slot().is_none() { ("REQUANTISE", "Word-length reduction and dither, always last") } else { (slot_label(ui.slot()), module_names(key).1) };
    let x = if ui.slot().is_some() { 172.0 } else { 120.0 };
    let name = name.to_uppercase();
    let y = z.header.1 + 20.0;
    s.text(Point::new(x, y), name.clone(), t.text, 13.0, false);
    let after = x + text::width(&name, 13.0) + 12.0;
    let _ = line;
    let taken = header_taken(ui);
    if let Some(line) = header_line(ui) {
        s.text(Point::new(after, y + 1.0), line, t.dim, 10.5, false);
    }
    // The KEY caption left of the shell's INT | EXT switch, where the page leaves room.
    if let Some(key) = ui.cells.iter().find(|c| c.label == "KEY" && c.kind == Kind::Segmented) {
        let right = key.rect.0 - 6.0;
        let left = right - text::width("KEY", 9.0);
        if !taken.iter().any(|(l, r)| *r > left - 2.0 && *l < right) {
            text_right(s, right, y, "KEY", t.dim, 9.0);
        }
    }
    // The "..." menu button.
    let m = (988.0, z.header.1 + 9.0, 24.0, 22.0);
    button(s, t, m, "", false);
    for i in 0..3 {
        s.fill_ellipse_solid(Scene::ellipse_at(Point::new(m.0 + 6.5 + i as f32 * 5.5, m.1 + m.3 * 0.5), 1.4), t.text);
    }
    if ui.setup_open {
        return;
    }
    let values = &ui.values;
    let cx = ui.ctx(values);
    ui.page.paint_static(s, &cx);
}

/// The x spans the page and the shell's switches take in the header row: cells, and what
/// the page paints there (static and moving), ignoring anything that also reaches left of
/// the module name (frames around the whole page).
fn header_taken(ui: &Ui) -> Vec<(f32, f32)> {
    let z = ui.zones();
    let (top, bottom) = (z.header.1 + 4.0, z.header.1 + z.header.3 - 4.0);
    let mut v: Vec<(f32, f32)> = Vec::new();
    let mut add = |l: f32, r: f32, y0: f32, y1: f32| {
        if y1 > top && y0 < bottom && l > 180.0 && r > l {
            v.push((l, r));
        }
    };
    for c in ui.page_cells() {
        add(c.rect.0, c.rect.0 + c.rect.2, c.rect.1, c.rect.1 + c.rect.3);
    }
    if ui.setup_open {
        return v;
    }
    let values = &ui.values;
    let cx = ui.ctx(values);
    let mut scratch = Scene::new();
    ui.page.paint_static(&mut scratch, &cx);
    let live = ui.live();
    ui.page.paint(&mut scratch, &cx, &live);
    use openplugin::dri::goodlookindri::scene::Cmd;
    // What a clip hides takes no room: a plot's fill can run past its frame into the header rows.
    let mut clips: Vec<Rect> = Vec::new();
    for cmd in &scratch.cmds {
        let clip = clips.last().copied();
        let mut rect_of = |r: Rect| match clip {
            Some(c) => add(r.x.max(c.x), (r.x + r.w).min(c.x + c.w), r.y.max(c.y), (r.y + r.h).min(c.y + c.h)),
            None => add(r.x, r.x + r.w, r.y, r.y + r.h),
        };
        match cmd {
            Cmd::ClipRound { rect, .. } => clips.push(match clip {
                Some(c) => Rect::new(rect.x.max(c.x), rect.y.max(c.y), (rect.x + rect.w).min(c.x + c.w) - rect.x.max(c.x), (rect.y + rect.h).min(c.y + c.h) - rect.y.max(c.y)),
                None => *rect,
            }),
            Cmd::PopClip => {
                clips.pop();
            }
            Cmd::FillEllipse { rect, .. } | Cmd::StrokeEllipse { rect, .. } | Cmd::FillRoundRect { rect, .. } | Cmd::StrokeRoundRect { rect, .. } | Cmd::FillRect { rect, .. } => rect_of(*rect),
            Cmd::Line { a, b, .. } => rect_of(Rect::new(a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs().max(1.0))),
            Cmd::Text { at, text: label, size, center, .. } => {
                let w = text::width(label, *size);
                let l = if *center { at.x - w * 0.5 } else { at.x };
                rect_of(Rect::new(l, at.y - size * 0.5, w, *size));
            }
            Cmd::FillPoly { pts, .. } | Cmd::StrokePoly { pts, .. } if !pts.is_empty() => {
                let (l, r) = pts.iter().fold((f32::MAX, f32::MIN), |(l, r), p| (l.min(p.x), r.max(p.x)));
                let (t0, t1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
                rect_of(Rect::new(l, t0, r - l, (t1 - t0).max(1.0)));
            }
            _ => {}
        }
    }
    v
}

/// `line` cut after the last whole word that fits in `room` points, without a dangling
/// "with", "and" or comma. `None` when not even the first words fit.
fn fit_words(line: &str, size: f32, room: f32) -> Option<String> {
    if room <= 24.0 || line.is_empty() {
        return None;
    }
    if text::width(line, size) <= room {
        return Some(line.to_string());
    }
    let words: Vec<&str> = line.split(' ').collect();
    const LOOSE: [&str; 14] = ["with", "and", "in", "a", "an", "the", "of", "or", "for", "to", "any", "its", "by", "per"];
    for n in (1..words.len()).rev() {
        // Cut only where a phrase ends: before a joining word or after a comma, so the
        // line never stops mid-phrase ("with band" of "with band limiters").
        if !LOOSE.contains(&words[n].to_lowercase().as_str()) && !words[n - 1].ends_with([',', ';', ':']) {
            continue;
        }
        let mut cut = &words[..n];
        while let Some((last, rest)) = cut.split_last() {
            if LOOSE.contains(&last.to_lowercase().as_str()) {
                cut = rest;
            } else {
                break;
            }
        }
        if cut.is_empty() {
            continue;
        }
        let s = cut.join(" ").trim_end_matches([',', ';', ':']).to_string();
        if text::width(&s, size) <= room {
            return Some(s);
        }
    }
    None
}

fn loud_bar(s: &mut Scene, t: &Theme, ui: &Ui) {
    panel(s, t, LOUD_BAR);
    let labels = ["MOMENTARY", "SHORT-TERM", "INTEGRATED", "RANGE", "LOUDNESS GAIN", "PEAK / LOUD"];
    let units = ["LUFS", "LUFS", "LUFS", "LU", "LU", "dB"];
    for (i, (label, unit)) in labels.into_iter().zip(units).enumerate() {
        let x = LOUD_CELL_X + i as f32 * LOUD_CELL_PITCH;
        s.text(Point::new(x + 8.0, LOUD_BAR.1 + 12.0), label, t.dim, 8.5, false);
        text_right(s, x + LOUD_CELL_PITCH - 14.0, LOUD_BAR.1 + 12.0, unit, t.dim, 8.0);
        if i > 0 {
            s.line(Point::new(x - 2.0, LOUD_BAR.1 + 8.0), Point::new(x - 2.0, LOUD_BAR.1 + LOUD_BAR.3 - 8.0), t.border, 1.0);
        }
        if i < 3 {
            let w = LOUD_CELL_PITCH - 20.0;
            s.fill_round(Rect::new(x + 8.0, LOUD_BAR.1 + 46.0, w, 3.0), 1.5, t.grid_strong, t.grid_strong, true);
            let tx = x + 8.0 + w * RADAR_BELOW / RADAR_SPAN;
            s.line(Point::new(tx, LOUD_BAR.1 + 42.0), Point::new(tx, LOUD_BAR.1 + 52.0), t.accent, 1.5);
        }
    }
    inset(s, t, TARGET_CHIP, 6.0);
    s.stroke_round(rect(TARGET_CHIP), 6.0, t.accent.with_alpha(0.6), 1.0);
    s.text(Point::new(TARGET_CHIP.0 + TARGET_CHIP.2 * 0.5, TARGET_CHIP.1 + 11.0), "TARGET", t.dim, 8.0, true);
    s.text(Point::new(TARGET_CHIP.0 + TARGET_CHIP.2 * 0.5, TARGET_CHIP.1 + 25.0), format!("{:.0} LUFS", ui.target()), t.accent, 11.0, true);
    button(s, t, RESET_CHIP, "RESET", false);
}

fn setup_sheet(s: &mut Scene, t: &Theme, ui: &Ui) {
    let r = SETUP_SHEET;
    s.shadow_round(rect(r).translated(0.0, 3.0), 10.0, 8.0, t.shadow);
    panel(s, t, r);
    title(s, t, r, "SETUP");
    button(s, t, SETUP_DONE, "DONE", false);
    s.text(Point::new(136.0, 96.0), "ALIGNMENT · FIRST LIMITER", t.text, 9.5, false);
    caption(s, t, 136.0, 260.0, "UNIT");
    caption(s, t, 136.0, 332.0, "HEADROOM");
    s.text(Point::new(560.0, 96.0), "WINDOW", t.text, 9.5, false);
    caption(s, t, 560.0, SETUP_BAND.1 - 10.0, "TOP BAND WHEN A PAGE OPENS");
    caption(s, t, 560.0, SETUP_THEME.1 - 10.0, "THEME");
    caption(s, t, 560.0, SETUP_RMS_SCALE.1 - 10.0, "RMS METER · SCALE AND WINDOW (s)");
    s.text(Point::new(560.0, 118.0), "The window scales from its corner, keeping its shape.", t.dim, 9.5, false);
    s.text(Point::new(560.0, 136.0), "Graph-first modules open with the band as a strip.", t.dim, 9.5, false);
    s.text(Point::new(560.0, 154.0), "Headroom is kept so old sessions load; it has no effect.", t.dim, 9.5, false);
    s.text(Point::new(560.0, 172.0), "The longer channel delay is added to the latency.", t.dim, 9.5, false);
    s.text(Point::new(840.0, 96.0), "LATENCY PER MODULE", t.text, 9.5, false);
    s.text(Point::new(840.0, 340.0), format!("{} {}", crate::INFO.name, crate::INFO.version), t.dim, 9.0, false);
    let _ = ui;
}

/// A segmented choice the shell paints itself (no parameter behind it).
fn segmented(s: &mut Scene, t: &Theme, r: R, labels: &[&str], chosen: usize) {
    inset(s, t, r, 6.0);
    let w = r.2 / labels.len() as f32;
    for (i, label) in labels.iter().enumerate() {
        let key = Rect::new(r.0 + w * i as f32 + 2.0, r.1 + 2.0, w - 4.0, r.3 - 4.0);
        if i == chosen {
            s.fill_round(key, 4.0, t.accent, t.accent, true);
        }
        s.text(key.center(), *label, if i == chosen { t.on_accent } else { t.dim }, 9.0, true);
    }
}

pub fn paint_dynamic(ui: &Ui, dl: &mut DrawList, touched: &dyn Fn(&DamageRect) -> bool) {
    let t = theme(ui.theme);
    let mut s = Scene::new();
    if touched(&dmg(PRESET_BOX)) {
        let name = ui.preset().map_or("Custom", |i| PRESETS[i].0);
        let (x, y, w, h) = PRESET_BOX;
        s.text(Point::new(x + w * 0.5, y + h * 0.5), text::ellipsize(name, 11.0, w - 16.0), t.text, 11.0, true);
    }
    if touched(&dmg(AB)) {
        segmented(&mut s, t, AB, &["A", "B"], ui.ab_on);
    }
    if touched(&dmg(SETUP_BUTTON)) && ui.setup_in_use() {
        let c = Point::new(SETUP_BUTTON.0 + 16.0, SETUP_BUTTON.1 + SETUP_BUTTON.3 * 0.5);
        s.fill_ellipse_solid(Scene::ellipse_at(c, 3.0), if ui.setup_open { t.on_accent } else { t.accent });
    }
    if touched(&dmg(AUTOGAIN_VALUE)) && ui.value(LAYOUT.autogain) >= 0.5 {
        let r = AUTOGAIN_VALUE;
        let db = ui.autogain_db;
        let text = if db.abs() < 0.05 { "0.0".to_string() } else { format!("{db:+.1}") };
        s.text(Point::new(r.0 + r.2 * 0.5, r.1 + r.3 * 0.5), text, t.text, 10.0, true);
    }
    if touched(&dmg(RMS_BOX)) {
        // The caption names the AES scale when it is on; the setup sheet changes it.
        caption(&mut s, t, RMS_BOX.0 + 2.0, RMS_BOX.1 + 6.0, "RMS");
        // Only the AES scale is named; plain RMS needs no tag.
        if ui.rms_aes {
            text_right(&mut s, RMS_BOX.0 + RMS_BOX.2, RMS_BOX.1 + 6.0, "AES +3", t.dim, 8.0);
        }
    }
    if touched(&dmg(DITHER_BOX)) {
        for (i, line) in ui.dither_lines().iter().enumerate() {
            s.text(Point::new(DITHER_BOX.0 + 2.0, DITHER_BOX.1 + 22.0 + i as f32 * 13.0), text::ellipsize(line, 10.0, DITHER_BOX.2 - 4.0), if i == 0 { t.text } else { t.dim }, 10.0, false);
        }
    }
    if touched(&dmg(LATENCY_BOX)) {
        let (samples, rate) = ui.latency;
        let ms = samples as f32 * 1000.0 / rate.max(1.0);
        s.text(Point::new(LATENCY_BOX.0 + 2.0, LATENCY_BOX.1 + 22.0), format!("{ms:.2} ms"), t.text, 11.0, false);
        s.text(Point::new(LATENCY_BOX.0 + 2.0, LATENCY_BOX.1 + 36.0), format!("{samples} smp"), t.dim, 9.5, false);
    }
    if ui.setup_open {
        if touched(&dmg(SETUP_BAND)) {
            segmented(&mut s, t, SETUP_BAND, &["AUTO", "FULL", "STRIP"], ui.band_pref as usize);
        }
        if touched(&dmg(SETUP_RMS_SCALE)) {
            segmented(&mut s, t, SETUP_RMS_SCALE, &["AES +3", "RMS"], usize::from(!ui.rms_aes));
        }
        if touched(&dmg(SETUP_RMS_WINDOW)) {
            segmented(&mut s, t, SETUP_RMS_WINDOW, &["0.3", "1", "2", "3", "5", "10"], ui.rms_window);
        }
        if touched(&dmg(SETUP_THEME)) {
            segmented(&mut s, t, SETUP_THEME, &["LIGHT", "DARK", "GRAPHITE", "PAPER"], ui.theme as usize);
        }
        let mut y = 116.0;
        for (slot, info) in LAYOUT.slots.iter().enumerate() {
            if !ui.on(info.on) {
                continue;
            }
            let r = (840.0, y - 8.0, 240.0, 16.0);
            if touched(&dmg(r)) {
                s.text(Point::new(840.0, y), info.label, t.dim, 9.5, false);
                text_right(&mut s, 1080.0, y, &format!("{} smp", ui.slot_latency[slot]), t.text, 9.5);
            }
            y += 17.0;
        }
    } else {
        let rack = ui.rack();
        if touched(&dmg(rack.rect)) {
            paint_rack(&mut s, t, ui, &rack);
        }
    }
    // The page header's moving parts.
    let z = ui.zones();
    if touched(&dmg(z.header)) {
        if let Some(slot) = ui.slot() {
            let samples = ui.slot_latency.get(slot).copied().unwrap_or(0);
            text_right(&mut s, 980.0, z.header.1 + 20.0, &format!("{samples} smp"), t.dim, 10.0);
        }
        if ui.page.has_more() && !ui.setup_open {
            let r = (1016.0, z.header.1 + 9.0, 64.0, 22.0);
            let on = ui.more;
            let fill = if on { t.accent } else { t.inset };
            s.fill_round(rect(r), 11.0, fill, fill, true);
            s.stroke_round(rect(r), 11.0, if on { t.accent } else { t.border }, 1.0);
            s.text(rect(r).center(), "MORE", if on { t.on_accent } else { t.text }, 9.5, true);
        }
    }
    if !ui.setup_open {
        let values = &ui.values;
        let cx = ui.ctx(values);
        if ui.page.paint_rects(&cx).iter().any(|r| touched(&dmg(*r))) {
            let live = ui.live();
            ui.page.paint(&mut s, &cx, &live);
        }
    }
    for (id, cell) in ui.cells.iter().enumerate() {
        if !touched(&dmg(cell.damage_rect())) {
            continue;
        }
        let Some(c) = ui.controls.get(id) else { continue };
        let focused = c.focused() && ui.keys_active;
        let value = ui.value(cell.param);
        kit::paint_value(&mut s, t, cell, c, value, focused);
    }
    if let Some(pop) = &ui.pop {
        pop.list.paint(&mut s, t);
    }
    dl.append(s);
}

/// The chips in processing order, the dither, the "+" and a chip being dragged.
fn paint_rack(s: &mut Scene, t: &Theme, ui: &Ui, rack: &RackGeom) {
    let dragging = ui.drag.filter(|d| d.moved);
    let order_selected = |slot: usize| ui.selected == slot;
    let chip = |s: &mut Scene, r: R, slot: usize| {
        let info = &LAYOUT.slots[slot];
        let on = ui.on(info.on);
        let picked = order_selected(slot);
        let fill = if picked { t.accent } else if on { t.panel } else { t.inset };
        s.fill_round(rect(r), 7.0, fill, fill, true);
        let edge = if picked { t.accent } else if on { t.accent.with_alpha(0.7) } else { t.border };
        s.stroke_round(rect(r), 7.0, edge, 1.0);
        let ink = if picked { t.on_accent } else if on { t.text } else { t.dim };
        let (name, _) = module_names(info.key);
        // The name stays clear of the badges in the top-right corner: centred in the chip
        // when it fits there, else centred in the room left of them.
        let (key, warn) = pages::chip_flags(slot, &ui.values);
        let badges = 14.0 * (usize::from(key) + usize::from(warn)) as f32;
        // A full rack has narrow chips: a smaller name keeps it readable.
        let size = if r.2 < 70.0 { 7.5 } else { 9.0 };
        let full = text::width(name, size);
        let (left, right) = (r.0 + 4.0, r.0 + r.2 - 4.0 - if badges > 0.0 { badges + 2.0 } else { 0.0 });
        let centre = if r.0 + r.2 * 0.5 + full * 0.5 <= right { r.0 + r.2 * 0.5 } else { (left + right) * 0.5 };
        s.text(Point::new(centre, r.1 + 12.0), text::ellipsize(name, size, right - left), ink, size, true);
        let dot = Point::new(r.0 + 11.0, r.1 + 29.0);
        let dot_ink = if picked { t.on_accent } else { t.accent };
        if on {
            s.fill_ellipse_solid(Scene::ellipse_at(dot, 4.0), dot_ink);
        } else {
            s.stroke_ellipse(Scene::ellipse_at(dot, 4.0), ink, 1.2);
        }
        // The activity lane: this module's reduction, 0 to 12 dB, with a held tick.
        let lane = (r.0 + 22.0, r.1 + 26.0, r.2 - 30.0, 6.0);
        let lane_bg = if picked { t.on_accent.with_alpha(0.25) } else { t.seg_off };
        s.fill_round(rect(lane), 2.0, lane_bg, lane_bg, true);
        let m = ui.slot_gr[slot];
        let up = ui.slot_lift[slot];
        // Reduction in the accent; gain a module adds (upward modules) in the ink colour.
        // When both move, reduction takes the upper half of the lane and lift the lower.
        let both = up.hold > 0.05 && m.hold > 0.05;
        let half = if both { lane.3 * 0.5 } else { lane.3 };
        let w = (m.level / 12.0).clamp(0.0, 1.0) * lane.2;
        if w > 0.5 && on {
            let c = if picked { t.on_accent } else { t.accent };
            s.fill_round(Rect::new(lane.0, lane.1, w, half), 2.0, c, c, true);
        }
        let hold = (m.hold / 12.0).clamp(0.0, 1.0) * lane.2;
        if hold > 1.0 && on {
            s.line(Point::new(lane.0 + hold, lane.1 - 1.0), Point::new(lane.0 + hold, lane.1 + half + 1.0), ink, 1.2);
        }
        let lw = (up.level / 12.0).clamp(0.0, 1.0) * lane.2;
        if lw > 0.5 && on {
            let y = lane.1 + lane.3 - half;
            s.fill_round(Rect::new(lane.0, y, lw, half), 2.0, ink, ink, true);
        }
    };
    for &(slot, r) in &rack.chips {
        if dragging.is_some_and(|d| d.slot == slot) {
            continue;
        }
        chip(s, r, slot);
        if order_selected(slot) {
            let z = ui.zones();
            let tab = Rect::new(r.0 + r.2 * 0.5 - 12.0, r.1 + r.3, 24.0, z.page.1 - (r.1 + r.3) + 1.0);
            s.fill_rect(tab, t.accent, t.accent, true);
        }
    }
    // The dither, pinned.
    let d = rack.dither;
    let picked = ui.selected == dither_page();
    let fill = if picked { t.accent } else { t.inset };
    s.fill_round(rect(d), 7.0, fill, fill, true);
    s.stroke_round(rect(d), 7.0, if picked { t.accent } else { t.border }, 1.0);
    let ink = if picked { t.on_accent } else { t.text };
    s.text(Point::new(d.0 + d.2 * 0.5, d.1 + 12.0), "REQUANTISE", ink, 8.5, true);
    let lines = ui.dither_lines();
    let summary = if lines[0] == "Off" { "OFF".to_string() } else { format!("{} · {}", lines[0].replace(" bit", ""), lines[2]) };
    s.text(Point::new(d.0 + d.2 * 0.5, d.1 + 28.0), text::ellipsize(&summary, 8.5, d.2 - 6.0), if picked { t.on_accent } else { t.dim }, 8.5, true);
    if picked {
        let z = ui.zones();
        let tab = Rect::new(d.0 + d.2 * 0.5 - 12.0, d.1 + d.3, 24.0, z.page.1 - (d.1 + d.3) + 1.0);
        s.fill_rect(tab, t.accent, t.accent, true);
    }
    if let Some(a) = rack.add {
        inset(s, t, a, 7.0);
        let c = rect(a).center();
        s.line(Point::new(c.x - 6.0, c.y), Point::new(c.x + 6.0, c.y), t.text, 1.6);
        s.line(Point::new(c.x, c.y - 6.0), Point::new(c.x, c.y + 6.0), t.text, 1.6);
    }
    if let Some(dr) = dragging {
        let place = rack.place_at(dr.at.0);
        let x = rack.chips.get(place).map_or_else(|| rack.chips.last().map_or(rack.input.0 + 30.0, |(_, r)| r.0 + r.2 + 3.0), |(_, r)| r.0 - 3.0);
        let removing = dr.at.1 > rack.rect.1 + rack.rect.3 + 40.0;
        if !removing {
            s.line(Point::new(x, rack.rect.1 + 4.0), Point::new(x, rack.rect.1 + rack.rect.3 - 4.0), t.accent, 2.0);
        }
        if let Some(&(_, home)) = rack.chips.iter().find(|(s, _)| *s == dr.slot) {
            let r = (home.0 + dr.at.0 - dr.from.0, home.1 - 2.0 + (dr.at.1 - dr.from.1).max(0.0), home.2, home.3);
            s.shadow_round(rect(r).translated(0.0, 2.0), 7.0, 4.0, t.shadow);
            chip(s, r, dr.slot);
            if removing {
                s.line(Point::new(r.0 + 6.0, r.1 + r.3 * 0.5), Point::new(r.0 + r.2 - 6.0, r.1 + r.3 * 0.5), t.over, 2.0);
            }
        }
    }
    // A "K" for modules that listen to the key, a "!" for one whose output can pass its ceiling.
    for &(slot, r) in &rack.chips {
        if dragging.is_some_and(|d| d.slot == slot) {
            continue;
        }
        let (key, warn) = pages::chip_flags(slot, &ui.values);
        let picked = ui.selected == slot;
        let on = ui.on(LAYOUT.slots[slot].on);
        // On the selected chip the badges sit on the accent fill: a ring of the chip's ink
        // keeps them apart from it (in the light theme the warning red is the accent).
        let ring = |s: &mut Scene, c: Point| {
            if picked {
                s.stroke_ellipse(Scene::ellipse_at(c, 6.5), t.on_accent, 1.2);
            }
        };
        let fade = if on { 1.0 } else { 0.5 };
        let mut x = r.0 + r.2 - 9.0;
        if warn {
            let c = Point::new(x, r.1 + 9.0);
            ring(s, c);
            s.fill_ellipse_solid(Scene::ellipse_at(c, 5.5), t.over.with_alpha(fade));
            s.text(c, "!", t.ink_on(t.over), 8.5, true);
            x -= 14.0;
        }
        if key {
            let c = Point::new(x, r.1 + 9.0);
            if ui.key.0 {
                ring(s, c);
                s.fill_ellipse_solid(Scene::ellipse_at(c, 5.5), t.warm.with_alpha(fade));
                s.text(c, "K", t.ink_on(t.warm), 8.0, true);
            } else {
                // External key chosen, none connected: hollow, struck through.
                let k = if picked { t.on_accent } else { t.dim };
                s.stroke_ellipse(Scene::ellipse_at(c, 5.5), k, 1.0);
                s.text(c, "K", k, 8.0, true);
                s.line(Point::new(c.x - 4.0, c.y + 4.0), Point::new(c.x + 4.0, c.y - 4.0), k, 1.0);
            }
        }
    }
    // The key line along the top of the rack, to the last chip that listens.
    if ui.key.0 {
        let last = rack.chips.iter().filter(|(slot, _)| pages::chip_flags(*slot, &ui.values).0).map(|(_, r)| r.0 + r.2 - 9.0).fold(f32::NAN, f32::max);
        if last.is_finite() {
            kit::dashed_h(s, rack.input.0 + 12.0, last, rack.rect.1 + 3.0, 4.0, 3.0, t.warm, 1.0);
            // A tick drops from the line into each listening chip.
            for (_, r) in rack.chips.iter().filter(|(slot, _)| pages::chip_flags(*slot, &ui.values).0) {
                let x = r.0 + r.2 - 9.0;
                s.line(Point::new(x, rack.rect.1 + 3.0), Point::new(x, r.1 + 2.0), t.warm, 1.0);
            }
        }
    }
}
