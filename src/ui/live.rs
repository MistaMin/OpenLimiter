//! What moves on every tick, rasterised straight into the frame: the rails' meters and
//! readouts, the top band (history and radar, or their strips) and the loudness numbers.

use openplugin::dri::goodlookindri::color::Color as C;
use openplugin::dri::goodlookindri_shell::text::GlyphCache;

use super::raster::{blend, rgb, Align, Target, R};
use super::theme::{loud_colour, pref_file, theme, Theme};
use super::*;

/// Y of a level inside a plot, in points from the plot's top.
pub fn plot_y(level_db: f32, plot: R) -> f32 {
    (PLOT_TOP_DB - level_db) / PLOT_SPAN_DB * plot.3
}

fn loud_norm(lufs: f32, target: f32) -> f32 {
    ((lufs - (target - RADAR_BELOW)) / RADAR_SPAN).clamp(0.0, 1.0)
}

pub fn load_band_pref() -> BandPref {
    match pref_file("band").and_then(|p| std::fs::read_to_string(p).ok()).as_deref().map(str::trim) {
        Some("full") => BandPref::Full,
        Some("strip") => BandPref::Strip,
        _ => BandPref::Auto,
    }
}

/// `(AES scale, window index)` for the RMS meter. Default: plain RMS, 2 s.
pub fn load_rms_pref() -> (bool, usize) {
    let text = pref_file("rms").and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut parts = text.split_whitespace();
    let aes = parts.next() == Some("aes");
    let window = parts.next().and_then(|w| w.parse::<f32>().ok()).and_then(|w| RMS_WINDOWS.iter().position(|v| (v - w).abs() < 1e-3)).unwrap_or(2);
    (aes, window)
}

pub fn store_rms_pref(aes: bool, window: usize) {
    // Tests must not write the user's preferences.
    if cfg!(test) {
        return;
    }
    if let Some(path) = pref_file("rms") {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("{} {}", if aes { "aes" } else { "rms" }, RMS_WINDOWS[window.min(RMS_WINDOWS.len() - 1)]));
    }
}

pub fn store_band_pref(p: BandPref) {
    if cfg!(test) {
        return;
    }
    if let Some(path) = pref_file("band") {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, match p {
            BandPref::Auto => "auto",
            BandPref::Full => "full",
            BandPref::Strip => "strip",
        });
    }
}

/// The band is live: not covered by the setup sheet or an open list.
fn band_live(ui: &Ui) -> bool {
    let band = if ui.tall { BAND_STRIP } else { BAND_FULL };
    // The list's rect and its shadow.
    let clear = |r: R| r.0 - 12.0 > band.0 + band.2 || r.0 + r.2 + 12.0 < band.0 || r.1 - 12.0 > band.1 + band.3 || r.1 + r.3 + 12.0 < band.1;
    !ui.setup_open && ui.pop.as_ref().is_none_or(|p| clear(p.list.rect))
}

/// Where the live layer paints.
pub fn live_rects(ui: &Ui) -> Vec<R> {
    let mut v = vec![LEFT_LIVE, RIGHT_LIVE, (RMS_BOX.0, RMS_BOX.1 + 12.0, RMS_BOX.2, RMS_BOX.3 - 12.0), (LOUD_CELL_X, LOUD_BAR.1 + 4.0, 6.0 * LOUD_CELL_PITCH, LOUD_BAR.3 - 8.0)];
    if band_live(ui) {
        if ui.tall {
            v.push(RIBBON);
            v.push(LOUD_RIBBON);
        } else {
            v.push(PLOT);
            v.push(GR_CHIP);
            let r = RADAR_R + 1.0;
            v.push((RADAR_C.0 - r, RADAR_C.1 - r, 2.0 * r, 2.0 * r));
        }
    }
    v
}

pub fn paint(ui: &Ui, f: &mut Target, glyphs: &mut GlyphCache, k: f32, lut: &[(f32, f32)]) {
    let t = theme(ui.theme);
    let threshold = ui.value(P_THRESHOLD);
    let limiting = threshold <= 0.05 && ui.first_on();

    // Rails: meters, the chain's reduction, readouts.
    for c in 0..2 {
        let from = if limiting { threshold } else { f32::INFINITY };
        meter_bar(f, t, k, IN_PPM[c], ui.input[c], from);
        meter_bar(f, t, k, OUT_PPM[c], ui.output[c], 0.0);
    }
    reduction_bar(f, t, k, CHAIN_GR, ui.reduction);
    let hold_in = ui.input[0].hold.max(ui.input[1].hold);
    let hold_out = ui.output[0].hold.max(ui.output[1].hold);
    for (r, level) in [(IN_PEAK, hold_in), (OUT_PEAK, hold_out)] {
        let colour = if level > 0.0 { t.over } else { t.text };
        f.text(glyphs, (r.0 + r.2 * 0.5) * k, (r.1 + r.3 * 0.5) * k, &level_text(level), 11.0 * k, colour, Align::Centre);
    }
    // The key lamp.
    let (present, key) = (ui.key.0, ui.key.1);
    let lamp = KEY_LAMP;
    let (label, colour) = if !present {
        ("NO KEY", t.dim)
    } else {
        ("KEY", t.warm)
    };
    f.round((lamp.0 * k, lamp.1 * k, lamp.2 * k, lamp.3 * k), 4.0 * k, k, colour, if present { 1.0 } else { 0.5 });
    // The outline and the bar carry the warm role; the word is in text ink so it reads.
    f.text(glyphs, (lamp.0 + 6.0) * k, (lamp.1 + lamp.3 * 0.5) * k, label, 8.5 * k, if present { t.text } else { t.dim }, Align::Left);
    if present && key.level > FLOOR_DB {
        let n = ((key.level + 60.0) / 60.0).clamp(0.0, 1.0);
        let (bx, bw) = (lamp.0 + 34.0, 28.0);
        f.round((bx * k, (lamp.1 + 6.0) * k, bw * k, 4.0 * k), 2.0 * k, 0.0, t.seg_off, 1.0);
        f.round((bx * k, (lamp.1 + 6.0) * k, (bw * n).max(0.5) * k, 4.0 * k), 2.0 * k, 0.0, t.warm, 1.0);
    }
    // True peak since the reset.
    let (label, colour) = match ui.loud.true_peak {
        v if v <= -100.0 => ("--".to_string(), t.dim),
        v => (format!("{v:+.1}"), if v > 0.0 { t.over } else if v > -1.0 { t.warm } else { t.text }),
    };
    f.text(glyphs, (TP_BOX.0 + TP_BOX.2 * 0.5) * k, (TP_BOX.1 + 26.0) * k, &label, 14.0 * k, colour, Align::Centre);
    // The output RMS over the chosen window.
    let (label, colour) = match ui.rms {
        Some(v) => (format!("{v:+.1}"), if v > 0.0 { t.over } else { t.text }),
        None => ("--".to_string(), t.dim),
    };
    f.text(glyphs, (RMS_BOX.0 + RMS_BOX.2 * 0.5) * k, (RMS_BOX.1 + 26.0) * k, &label, 14.0 * k, colour, Align::Centre);

    if band_live(ui) {
        if ui.tall {
            paint_plot(ui, f, t, k, RIBBON, false);
            paint_loud_ribbon(ui, f, glyphs, t, k);
        } else {
            paint_plot(ui, f, t, k, PLOT, true);
            paint_radar(ui, f, glyphs, t, k, lut);
            paint_band_marks(ui, f, glyphs, t, k, threshold, limiting);
        }
    }
    paint_loud_bar(ui, f, glyphs, t, k);
}

/// The reduction chip, the soft clip label and the threshold tag over the full plot.
fn paint_band_marks(ui: &Ui, f: &mut Target, glyphs: &mut GlyphCache, t: &Theme, k: f32, threshold: f32, limiting: bool) {
    let gr = ui.reduction.hold;
    let active = gr > 0.05;
    let chip = (GR_CHIP.0 * k, GR_CHIP.1 * k, GR_CHIP.2 * k, GR_CHIP.3 * k);
    if active {
        f.round(chip, 5.0 * k, 0.0, t.accent, 0.14);
    }
    f.round(chip, 5.0 * k, k, if active { t.accent } else { t.border }, if active { 0.6 } else { 1.0 });
    f.text(glyphs, (GR_CHIP.0 + GR_CHIP.2 * 0.5) * k, (GR_CHIP.1 + GR_CHIP.3 * 0.5) * k, &format!("CHAIN GR {:.1}", -gr.max(0.0) + 0.0), 9.5 * k, if active { t.accent } else { t.dim }, Align::Centre);
    if !ui.first_on() {
        return;
    }
    let soft = ui.value(P_SOFT_CLIP);
    if soft < 24.0 {
        let y = PLOT.1 + plot_y(soft, PLOT);
        if y > PLOT.1 + 12.0 && y < PLOT.1 + PLOT.3 - 4.0 {
            let colour = if ui.clipping() { t.over } else { t.dim };
            f.text(glyphs, (PLOT.0 + 8.0) * k, (y - 8.0) * k, "SOFT CLIP", 8.0 * k, colour, Align::Left);
        }
    }
    let grip = ui.controls.iter().zip(&ui.cells).find(|(_, cell)| cell.kind == Kind::Grip && cell.param == P_THRESHOLD).map(|(c, _)| c);
    let lit = grip.is_some_and(|c| c.hover() || c.pressed());
    let y = (PLOT.1 + plot_y(threshold, PLOT)).clamp(PLOT.1 + 9.0, PLOT.1 + PLOT.3 - 9.0);
    let tag = (PLOT.0 + PLOT.2 - 92.0, y - 9.0, 88.0, 18.0);
    if lit {
        f.round(((tag.0 - 2.0) * k, (tag.1 - 2.0) * k, (tag.2 + 4.0) * k, (tag.3 + 4.0) * k), 7.0 * k, 0.0, t.accent, 0.3);
    }
    f.round((tag.0 * k, tag.1 * k, tag.2 * k, tag.3 * k), 5.0 * k, 0.0, t.accent, 1.0);
    for i in 0..3 {
        let gx = ((tag.0 + 8.0 + i as f32 * 3.0) * k).round() as usize;
        for dx in 0..(k.round() as usize).max(1) {
            f.vspan(gx + dx, (y - 3.5) * k, (y + 3.5) * k, 0, usize::MAX, t.on_accent, 0.75);
        }
    }
    let label = if limiting { format!("THR {threshold:.1}") } else { "THR Off".to_string() };
    f.text(glyphs, (tag.0 + 52.0) * k, y * k, &label, 10.0 * k, t.on_accent, Align::Centre);
}

/// The history: input and output as filled areas, gain reduction hanging from the top.
fn paint_plot(ui: &Ui, f: &mut Target, t: &Theme, k: f32, plot: R, grid_lines: bool) {
    let (x0, y0) = ((plot.0 * k).round() as usize, (plot.1 * k).round() as usize);
    let (w, h) = ((plot.2 * k).round().max(1.0) as usize, (plot.3 * k).round().max(1.0) as usize);
    if x0 + w > f.w || y0 + h > f.h {
        return;
    }
    let y_of = |level_db: f32| plot_y(level_db, plot) * k;
    let len = ui.history.len();
    let lead = WINDOW - len.min(WINDOW);
    let ys: Vec<(f32, f32, f32)> = ui.history.iter().map(|e| (y_of(db(e.input)), y_of(db(e.output)), -db(e.gain) / PLOT_SPAN_DB * h as f32)).collect();
    let none = (h as f32 + 4.0, h as f32 + 4.0, 0.0);
    let columns: Vec<(f32, f32, f32)> = (0..w)
        .map(|x| {
            let e = (x as f32 + 0.5) / w as f32 * WINDOW as f32 - lead as f32 - 0.5;
            if len == 0 || e < 0.0 {
                return none;
            }
            let i0 = (e.floor() as usize).min(len - 1);
            let i1 = (i0 + 1).min(len - 1);
            let m = (e - i0 as f32).clamp(0.0, 1.0);
            let mix = |a: f32, b: f32| a + (b - a) * m;
            (mix(ys[i0].0, ys[i1].0), mix(ys[i0].1, ys[i1].1), mix(ys[i0].2, ys[i1].2))
        })
        .collect();
    let bg = rgb(t.plot_bg);
    let mut table = [0u32; 8];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = bg;
        if i & 1 != 0 {
            c = blend(c, t.in_fill, t.in_fill_alpha);
        }
        if i & 2 != 0 {
            c = blend(c, t.out_fill, t.out_fill_alpha);
        }
        if i & 4 != 0 {
            c = blend(c, t.accent, 0.16);
        }
        *slot = c;
    }
    let mut grid_rows = vec![0u32; h];
    let mut grid_cols = vec![false; w];
    let step = if grid_lines { 6.0 } else { 12.0 };
    let mut level = PLOT_TOP_DB - 6.0;
    while level > PLOT_TOP_DB - PLOT_SPAN_DB {
        let row = y_of(level).round() as usize;
        if row < h && (grid_lines || level == 0.0) {
            grid_rows[row] = rgb(if level == 0.0 { t.grid_strong } else { t.grid });
        }
        level -= step;
    }
    if grid_lines {
        for sec in 1..WINDOW_S as usize {
            let x = (w as f32 * (1.0 - sec as f32 / WINDOW_S)).round() as usize;
            if x < w {
                grid_cols[x] = true;
            }
        }
    }
    let grid = rgb(t.grid);
    for y in 0..h {
        let yc = y as f32 + 0.5;
        let row = &mut f.px[(y0 + y) * f.w + x0..(y0 + y) * f.w + x0 + w];
        let grid_row = grid_rows[y];
        for (x, (px, col)) in row.iter_mut().zip(&columns).enumerate() {
            let i = usize::from(yc >= col.0) | usize::from(yc >= col.1) << 1 | usize::from(yc < col.2) << 2;
            *px = if i != 0 {
                table[i]
            } else if grid_row != 0 {
                grid_row
            } else if grid_cols[x] {
                grid
            } else {
                bg
            };
        }
    }
    let line = (1.3 * k).max(1.0);
    let (top, bottom) = (y0, y0 + h);
    let oy = y0 as f32;
    let mut prev = columns.first().copied().unwrap_or(none);
    for (x, now) in columns.iter().copied().enumerate() {
        f.vspan(x0 + x, oy + prev.0.min(now.0) - line * 0.4, oy + prev.0.max(now.0) + line * 0.4, top, bottom, t.in_line, 1.0);
        f.vspan(x0 + x, oy + prev.1.min(now.1) - line * 0.5, oy + prev.1.max(now.1) + line * 0.5, top, bottom, t.out_line, 1.0);
        if now.2 > 0.3 || prev.2 > 0.3 {
            f.vspan(x0 + x, oy + prev.2.min(now.2) - line * 0.5, oy + prev.2.max(now.2) + line * 0.5, top, bottom, t.accent, 1.0);
        }
        prev = now;
    }
    let dashed = |f: &mut Target, level: f32, on: f32, off: f32, thick: f32, c: C, alpha: f32| {
        let (on, off) = (((on * k) as usize).max(1), ((off * k) as usize).max(1));
        let y = oy + y_of(level);
        for x in (0..w).filter(|x| x % (on + off) < on) {
            f.vspan(x0 + x, y - thick * 0.5, y + thick * 0.5, top, bottom, c, alpha);
        }
    };
    if !ui.first_on() {
        return;
    }
    let soft = ui.value(P_SOFT_CLIP);
    if soft < 24.0 {
        dashed(f, soft, 2.0, 4.0, k, t.dim, 0.9);
    }
    let threshold = ui.value(P_THRESHOLD);
    if threshold <= 0.05 {
        dashed(f, threshold, 7.0, 4.0, 1.5 * k, t.accent, 1.0);
    }
}

/// The radar unrolled: the last 30 s of momentary output loudness as columns coloured by
/// their distance from the target, the input as a thin line, the target as a line.
fn paint_loud_ribbon(ui: &Ui, f: &mut Target, glyphs: &mut GlyphCache, t: &Theme, k: f32) {
    let r = LOUD_RIBBON;
    let (x0, y0) = ((r.0 * k).round() as usize, (r.1 * k).round() as usize);
    let (w, h) = ((r.2 * k).round().max(1.0) as usize, (r.3 * k).round().max(1.0) as usize);
    if x0 + w > f.w || y0 + h > f.h {
        return;
    }
    let target = ui.target();
    f.fill(x0, y0, x0 + w, y0 + h, rgb(t.plot_bg));
    let n = ui.loud_hist.len();
    let per = RADAR_SLOTS as f32 / w as f32;
    for x in 0..w {
        let age = ((w - 1 - x) as f32 * per) as usize;
        if age >= n {
            continue;
        }
        let (input, output) = ui.loud_hist[n - 1 - age];
        if output > LOUD_FLOOR {
            let top = h as f32 * (1.0 - loud_norm(output, target));
            f.vspan(x0 + x, y0 as f32 + top, (y0 + h) as f32, y0, y0 + h, loud_colour(t, output - target), 0.75);
        }
        if input > LOUD_FLOOR {
            let y = y0 as f32 + h as f32 * (1.0 - loud_norm(input, target));
            f.vspan(x0 + x, y - 0.6 * k, y + 0.6 * k, y0, y0 + h, t.text, 0.6);
        }
    }
    let ty = y0 as f32 + h as f32 * (1.0 - RADAR_BELOW / RADAR_SPAN);
    let (period, on) = (((6.0 * k) as usize).max(2), ((3.0 * k) as usize).max(1));
    for x in (0..w).filter(|x| x % period < on) {
        f.vspan(x0 + x, ty - 0.6 * k, ty + 0.6 * k, y0, y0 + h, t.accent, 1.0);
    }
    // The caption sits on a plate of the plot's colour, so the columns never run through it.
    f.round(((r.0 + 2.0) * k, (r.1 + 2.0) * k, 140.0 * k, 13.0 * k), 3.0 * k, 0.0, t.plot_bg, 0.92);
    f.text(glyphs, (r.0 + 6.0) * k, (r.1 + 9.0) * k, &format!("LOUDNESS 30 s · TARGET {target:.0}"), 8.0 * k, t.dim, Align::Left);
}

/// The radar: momentary loudness around a clock face, one turn per `RADAR_SLOTS` entries.
fn paint_radar(ui: &Ui, f: &mut Target, glyphs: &mut GlyphCache, t: &Theme, k: f32, lut: &[(f32, f32)]) {
    let (x0, y0, side) = radar_box(k, RADAR_C, RADAR_R);
    if lut.len() != side * side || x0 + side > f.w || y0 + side > f.h {
        return;
    }
    let n = RADAR_SLOTS;
    let target = ui.target();
    let rk = RADAR_R * k;
    let mut slots = vec![(0.0f32, 0.0f32, u32::MAX); n];
    for (age, &(input, output)) in ui.loud_hist.iter().rev().enumerate().take(n.min(ui.loud_count)) {
        slots[(ui.loud_count - 1 - age) % n] = (loud_norm(input, target) * rk, loud_norm(output, target) * rk, age as u32);
    }
    let hand = (ui.loud_count % n) as f32 / n as f32;
    let ramp: Vec<u32> = (0..=64).map(|i| rgb(loud_colour(t, i as f32 / 64.0 * RADAR_SPAN - RADAR_BELOW))).collect();
    let colour_at = |r: f32| C(0xff00_0000 | ramp[((r / rk).clamp(0.0, 1.0) * 64.0) as usize]);
    let rings = [(RADAR_BELOW - 24.0) / RADAR_SPAN, (RADAR_BELOW - 12.0) / RADAR_SPAN];
    let target_ring = RADAR_BELOW / RADAR_SPAN * rk;
    let bg = rgb(t.plot_bg);
    let line = 0.5 * k + 0.5;
    for y in 0..side {
        for x in 0..side {
            let (a, d) = lut[y * side + x];
            let edge = (rk - d + 0.5).clamp(0.0, 1.0);
            if edge <= 0.0 {
                continue;
            }
            let mut c = bg;
            for ring in rings {
                c = blend(c, t.grid_strong, (line - (d - ring * rk).abs()).clamp(0.0, 1.0));
            }
            let e = a * n as f32 - 0.5;
            let s0 = (e.floor() as isize).rem_euclid(n as isize) as usize;
            let (p, q) = (slots[s0], slots[(s0 + 1) % n]);
            let m = e - e.floor();
            let value = if p.2 != u32::MAX && q.2 != u32::MAX && p.2.abs_diff(q.2) == 1 {
                Some((p.0 + (q.0 - p.0) * m, p.1 + (q.1 - p.1) * m, p.2))
            } else {
                let near = if m < 0.5 { p } else { q };
                (near.2 != u32::MAX).then_some(near)
            };
            if let Some((r_in, r_out, age)) = value {
                let fade = 1.0 - 0.65 * age as f32 / n as f32;
                if r_out > 0.5 {
                    c = blend(c, colour_at(d), (r_out - d + 0.5).clamp(0.0, 1.0) * 0.6 * fade);
                    c = blend(c, colour_at(r_out), (line + 0.3 - (d - r_out).abs()).clamp(0.0, 1.0) * (0.4 + 0.6 * fade));
                }
                if r_in > 0.5 {
                    c = blend(c, t.text, (line - (d - r_in).abs()).clamp(0.0, 1.0) * (0.35 + 0.5 * fade));
                }
            }
            if (a * 60.0).fract() < 0.5 {
                c = blend(c, t.accent, (line - (d - target_ring).abs()).clamp(0.0, 1.0));
            }
            let across = ((a - hand + 0.5).rem_euclid(1.0) - 0.5).abs() * std::f32::consts::TAU * d;
            c = blend(c, t.text, (line - across).clamp(0.0, 1.0) * 0.85);
            f.mix(x0 + x, y0 + y, C(0xff00_0000 | c), edge);
        }
    }
    for (lu, colour) in [(0.0, t.accent), (-12.0, t.dim), (-24.0, t.dim)] {
        let y = RADAR_C.1 - (RADAR_BELOW + lu) / RADAR_SPAN * RADAR_R + 7.0;
        f.text(glyphs, (RADAR_C.0 + 4.0) * k, y * k, &format!("{:.0}", target + lu), 8.0 * k, colour, Align::Left);
    }
}

/// The loudness bar's numbers: output large, input small, a deviation bar along the bottom.
fn paint_loud_bar(ui: &Ui, f: &mut Target, glyphs: &mut GlyphCache, t: &Theme, k: f32) {
    let (l, target) = (ui.readout, ui.target());
    let heard = |v: f32| v > LOUD_FLOOR;
    let lufs = |v: f32| if heard(v) { (format!("{v:.1}"), loud_colour(t, v - target)) } else { ("--".to_string(), t.dim) };
    let plain = |v: Option<f32>, signed: bool| match v {
        Some(v) if signed => (format!("{v:+.1}"), t.accent),
        Some(v) => (format!("{v:.1}"), t.text),
        None => ("--".to_string(), t.dim),
    };
    let peak = ui.output[0].hold.max(ui.output[1].hold);
    let out = [
        lufs(l.momentary_out),
        lufs(l.short_out),
        lufs(l.integrated),
        plain(heard(l.integrated).then_some(l.range), false),
        plain((heard(l.short_in) && heard(l.short_out)).then(|| l.short_out - l.short_in), true),
        plain((heard(l.short_out) && peak > FLOOR_DB).then(|| peak - l.short_out), false),
    ];
    let input = [Some(l.momentary_in), Some(l.short_in), Some(l.integrated_in), None, None, None];
    for (i, ((label, colour), inp)) in out.iter().zip(input).enumerate() {
        let x = LOUD_CELL_X + i as f32 * LOUD_CELL_PITCH;
        let y = LOUD_BAR.1;
        f.text(glyphs, (x + 8.0) * k, (y + 31.0) * k, label, 18.0 * k, *colour, Align::Left);
        if let Some(v) = inp.filter(|v| heard(*v)) {
            let s = format!("in {v:.1}");
            // Right-aligned under the unit, clear of the output value.
            let right = x + LOUD_CELL_PITCH - 14.0;
            let left = (right - openplugin::dri::goodlookindri_shell::text::width(&s, 10.0)).max(x + 8.0 + openplugin::dri::goodlookindri_shell::text::width(label, 18.0) + 6.0);
            f.text(glyphs, left * k, (y + 33.0) * k, &s, 10.0 * k, t.dim, Align::Left);
        }
        let level = [l.momentary_out, l.short_out, l.integrated].get(i).copied().filter(|v| heard(*v));
        if let Some(v) = level {
            let w = LOUD_CELL_PITCH - 20.0;
            let bar = w * loud_norm(v, target);
            if bar > 1.0 {
                f.round(((x + 8.0) * k, (y + 46.0) * k, bar * k, 3.0 * k), 1.5 * k, 0.0, *colour, 1.0);
            }
        }
    }
}

/// The square of device pixels the radar is painted in: `(x, y, side)`.
pub fn radar_box(k: f32, c: (f32, f32), radius: f32) -> (usize, usize, usize) {
    let r = radius + 1.0;
    (((c.0 - r) * k).floor() as usize, ((c.1 - r) * k).floor() as usize, (2.0 * r * k).ceil() as usize)
}

/// Per pixel of `radar_box`: the angle as a fraction of a turn and the distance from the centre.
pub fn radar_lut(k: f32, c: (f32, f32), radius: f32) -> Vec<(f32, f32)> {
    let (x0, y0, side) = radar_box(k, c, radius);
    let mut lut = Vec::with_capacity(side * side);
    for y in 0..side {
        for x in 0..side {
            let (dx, dy) = ((x0 + x) as f32 + 0.5 - c.0 * k, (y0 + y) as f32 + 0.5 - c.1 * k);
            lut.push(((dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0), dx.hypot(dy)));
        }
    }
    lut
}

/// One level bar of lit and unlit segments. Lit segments above `accent_from` are accent.
fn meter_bar(f: &mut Target, t: &Theme, k: f32, r: R, m: Meter, accent_from: f32) {
    let step = METER_SPAN_DB / METER_SEGMENTS as f32;
    segments(f, k, r, |i| {
        let low = METER_TOP_DB - (i + 1) as f32 * step;
        let lit = m.level >= low + step * 0.5 || (m.hold > FLOOR_DB && m.hold >= low && m.hold < low + step);
        match lit {
            false => t.seg_off,
            true if low >= 0.0 => t.over,
            true if low + step > accent_from => t.accent,
            true => t.seg_on,
        }
    });
}

/// The gain reduction bar: lit from the top down.
fn reduction_bar(f: &mut Target, t: &Theme, k: f32, r: R, m: Meter) {
    let step = GR_METER_SPAN_DB / METER_SEGMENTS as f32;
    segments(f, k, r, |i| {
        let depth = i as f32 * step;
        let lit = m.level > depth + 0.05 || (m.hold > 0.05 && m.hold > depth && m.hold <= depth + step);
        if lit { t.accent } else { t.seg_off }
    });
}

fn segments(f: &mut Target, k: f32, r: R, colour: impl Fn(usize) -> C) {
    let (x0, x1) = ((r.0 * k).round() as usize, ((r.0 + r.2) * k).round() as usize);
    let top = (r.1 * k).round();
    let pitch = r.3 * k / METER_SEGMENTS as f32;
    for i in 0..METER_SEGMENTS {
        let y0 = (top + i as f32 * pitch).round() as usize;
        let y1 = (top + (i as f32 + 1.0) * pitch - k).round() as usize;
        f.fill(x0, y0, x1, y1.max(y0 + 1), rgb(colour(i)));
    }
}
