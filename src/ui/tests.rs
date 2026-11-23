use std::collections::BTreeSet;

use super::*;

/// A burst the chain would be working on, and loudness swelling around the target.
pub(crate) fn feed(shared: &Shared, entries: usize) {
    for i in 0..entries {
        let phase = i as f32 / 40.0;
        let input = 0.25 + 0.9 * (phase.sin().abs()).powf(3.0);
        let output = input.min(0.5);
        shared.push(Entry { input, output, gain: output / input, ms: [output * output * 0.5; 2] });
    }
    shared.raise_peaks(0, 1.1, 0.5);
    shared.raise_peaks(1, 0.8, 0.5);
    shared.set_timing(39, 48_000.0);
    shared.raise_slot_gr(0, 4.5);
    shared.set_slot_latency(0, 39);
    for i in 0..entries.div_ceil(10) {
        let swell = (shared.loud_head() + i) as f32 / 55.0;
        let out = -17.0 + 6.0 * swell.sin() + 1.5 * (swell * 7.3).sin();
        shared.push_loud(Loud { momentary_in: out - 5.0, momentary_out: out, short_in: out - 5.5, short_out: out - 0.5, integrated: -15.2, integrated_in: -20.0, range: 6.4, true_peak: -0.3 });
    }
}

pub(crate) fn editor(theme_id: ThemeId) -> (OpenLimiterEditor, Arc<Shared>) {
    let shared = Arc::new(Shared::default());
    let mut e = OpenLimiterEditor::with_theme(EditorHost::detached(), theme_id);
    e.bind(Some(shared.clone()));
    (e, shared)
}

pub(crate) fn click(e: &mut OpenLimiterEditor, r: R) {
    let (x, y) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5);
    e.mouse_down(x, y, 1);
    e.mouse_up(x, y);
}

fn slot_of(key: &str) -> usize {
    LAYOUT.slots.iter().position(|s| s.key == key).expect("slot")
}

/// Every page, every view, with and without MORE: the controls reach every parameter.
#[test]
fn every_parameter_can_be_reached() {
    let (mut e, _) = editor(ThemeId::Light);
    let mut seen = BTreeSet::new();
    for selected in 0..=dither_page() {
        e.open_page(selected);
        for more in [false, true] {
            if more && !e.ui.page.has_more() {
                continue;
            }
            e.ui.more = more;
            for state in 0..e.ui.page.states() {
                e.ui.page.set_state(state);
                e.rebuild();
                seen.extend(e.ui.controls.iter().map(|c| c.param));
            }
        }
        e.ui.more = false;
    }
    e.set_setup(true);
    seen.extend(e.ui.controls.iter().map(|c| c.param));
    // Positions and the rack's own mask are set in the rack; the first limiter's dither is
    // replaced by the pinned dither stage.
    seen.extend(LAYOUT.slots.iter().map(|s| s.position));
    seen.extend(LAYOUT.slots.iter().map(|s| s.on));
    seen.insert(LAYOUT.rack_shown);
    seen.insert(P_DITHER);
    let missing: Vec<&str> = (0..INFO.params.len()).filter(|i| !seen.contains(i)).map(|i| INFO.params[i].id).collect();
    assert!(missing.is_empty(), "no control for {} parameters: {missing:?}", missing.len());
}

#[test]
fn every_page_shows_its_whole_description() {
    let (mut e, _) = editor(ThemeId::Light);
    let mut short = Vec::new();
    for selected in 0..=dither_page() {
        e.open_page(selected);
        for tall in [false, true] {
            e.set_tall(tall);
            let key = e.ui.slot().map_or("dither", |s| LAYOUT.slots[s].key);
            let full = if e.ui.slot().is_none() { "Word-length reduction and dither, always last" } else { module_names(key).1 };
            let shown = chrome::header_line(&e.ui).unwrap_or_default();
            if shown != full {
                short.push(format!("{} (tall {tall}): \"{shown}\" of \"{full}\"", slot_label(e.ui.slot())));
            }
        }
    }
    assert!(short.is_empty(), "{}", short.join("\n"));
}

#[test]
fn numbers_fit_three_digits() {
    for (from, to) in [("3510.0 ms", "3.51 s"), ("17.74 kHz", "17.7 kHz"), ("-144.0 dB", "-144 dB"), ("1038.0 ms", "1.04 s"), ("220.4 Hz", "220 Hz"), ("2250.0 Hz", "2.25 kHz"), ("1:5.0 UP", "1:5.0 UP"), ("∞:1", "∞:1"), ("+0.3 dB", "+0.3 dB"), ("100.00", "100")] {
        assert_eq!(kit::three_digits(from), to);
    }
    // Every number control on every page, at its ends, its middle and a step past it.
    let (mut e, _) = editor(ThemeId::Light);
    let digits = |t: &str| t.split(|c: char| !c.is_ascii_digit() && c != '.').map(|n| n.bytes().filter(u8::is_ascii_digit).count()).max().unwrap_or(0);
    for selected in 0..=dither_page() {
        e.open_page(selected);
        for more in [false, true] {
            if more && !e.ui.page.has_more() {
                continue;
            }
            e.ui.more = more;
            for state in 0..e.ui.page.states() {
                e.ui.page.set_state(state);
                e.rebuild();
                for cell in e.ui.cells.iter() {
                    let p = &INFO.params[cell.param];
                    for v in [p.min, p.min + (p.max - p.min) * 0.5, p.min + (p.max - p.min) * 0.501, p.max] {
                        let t = cell.text(p.clamp(v));
                        assert!(digits(&t) <= 3, "{}: {t}", p.id);
                    }
                }
            }
        }
        e.ui.more = false;
    }
}

/// The text every number control shows at its minimum, middle and maximum (and one step
/// above the middle), for the QA display rules: `build/value-texts.tsv`. Run with `--ignored`.
#[test]
#[ignore]
fn dump_value_texts() {
    let (mut e, _) = editor(ThemeId::Light);
    let mut seen = BTreeSet::new();
    let mut rows = vec!["page\tid\tname\tkind\tunit\tsteps\tmin\tmid\tmid+step\tmax".to_string()];
    let mut dump = |e: &OpenLimiterEditor, page: &str, rows: &mut Vec<String>| {
        for cell in e.ui.cells.iter() {
            if matches!(cell.kind, Kind::Pill | Kind::Segmented | Kind::Dropdown) || !seen.insert(cell.param) {
                continue;
            }
            let p = &INFO.params[cell.param];
            let mid = p.clamp(p.min + (p.max - p.min) * 0.5);
            let step = if p.steps > 1 { (p.max - p.min) / (p.steps - 1) as f32 } else { (p.max - p.min) / 1000.0 };
            rows.push(format!("{page}\t{}\t{}\t{:?}\t{:?}\t{}\t{}\t{}\t{}\t{}", p.id, p.name, cell.kind, p.unit, p.steps, cell.text(p.min), cell.text(mid), cell.text(p.clamp(mid + step)), cell.text(p.max)));
        }
    };
    for selected in 0..=dither_page() {
        e.open_page(selected);
        for more in [false, true] {
            if more && !e.ui.page.has_more() {
                continue;
            }
            e.ui.more = more;
            for state in 0..e.ui.page.states() {
                e.ui.page.set_state(state);
                e.rebuild();
                dump(&e, slot_label(e.ui.slot()), &mut rows);
            }
        }
        e.ui.more = false;
    }
    e.set_setup(true);
    dump(&e, "Setup", &mut rows);
    std::fs::create_dir_all("build").unwrap();
    std::fs::write("build/value-texts.tsv", rows.join("\n") + "\n").unwrap();
}

/// The manual's parameter reference, page by page, as the editor shows each control: range and
/// default in the control's own text. Controls repeated per band or per set are listed once, with
/// the number written as `n`. Writes `docs/PARAMETERS.md`. Run with `--ignored`.
#[test]
#[ignore]
fn dump_manual_params() {
    let (mut e, _) = editor(ThemeId::Light);
    let mut seen = BTreeSet::new();
    let mut out = String::from(
        "# OpenLimiter parameter reference\n\nGenerated from the plugin (`cargo test --release dump_manual_params -- --ignored`). \
Every control, page by page, with its range and default as the editor shows them. A control that repeats per band, set or \
channel is listed once, with its number written as `n`.\n",
    );
    let norm = |s: &str| {
        let mut o = String::new();
        let mut digit = false;
        let mut bracket = false;
        for c in s.chars() {
            bracket |= c == '(';
            if c.is_ascii_digit() && !bracket {
                if !digit {
                    o.push('n');
                }
                digit = true;
            } else {
                digit = false;
                o.push(c);
            }
        }
        o
    };
    // The top bar and the input section are on every page: listed once, first.
    let l = &*LAYOUT;
    let shell = [l.input_gain[0], l.input_gain[1], l.input_link, l.autogain, l.delta];
    out.push_str("\n## Top bar and input\n\n| Control | Range or choices | Default |\n|---|---|---|\n");
    for &i in &shell {
        let p = &INFO.params[i];
        let cell = e.ui.cells.iter().find(|c| c.param == i).expect("shell control on screen");
        let (range, default) = if p.unit == openplugin::prelude::Unit::Toggle {
            ("Off, On".to_string(), if p.default >= 0.5 { "On" } else { "Off" }.to_string())
        } else {
            (format!("{} to {}", cell.text(p.min), cell.text(p.max)), cell.text(p.default))
        };
        out.push_str(&format!("| {} | {} | {} |\n", p.name, range, default));
    }
    let mut dump = |e: &OpenLimiterEditor, page: &str, out: &mut String| {
        for cell in e.ui.cells.iter() {
            if shell.contains(&cell.param) {
                continue;
            }
            let p = &INFO.params[cell.param];
            let name = norm(p.name);
            if !seen.insert((page.to_string(), name.clone())) {
                continue;
            }
            if !seen.contains(&(page.to_string(), String::from("\u{0}"))) {
                seen.insert((page.to_string(), String::from("\u{0}")));
                out.push_str(&format!("\n## {page}\n\n| Control | Range or choices | Default |\n|---|---|---|\n"));
            }
            let (range, default) = match cell.kind {
                Kind::Pill => ("Off, On".to_string(), if p.default >= 0.5 { "On" } else { "Off" }.to_string()),
                _ if !p.choices.is_empty() => (p.choices.join(", "), p.choices.get(p.default.round() as usize).copied().unwrap_or("").to_string()),
                _ => (format!("{} to {}", cell.text(p.min), cell.text(p.max)), cell.text(p.default)),
            };
            out.push_str(&format!("| {} | {} | {} |\n", name.replace('|', "/"), range.replace('|', "/"), default.replace('|', "/")));
        }
    };
    for selected in 0..=dither_page() {
        e.open_page(selected);
        for more in [false, true] {
            if more && !e.ui.page.has_more() {
                continue;
            }
            e.ui.more = more;
            for state in 0..e.ui.page.states() {
                e.ui.page.set_state(state);
                e.rebuild();
                dump(&e, slot_label(e.ui.slot()), &mut out);
            }
        }
        e.ui.more = false;
    }
    e.set_setup(true);
    dump(&e, "Setup", &mut out);
    std::fs::create_dir_all("docs").unwrap();
    std::fs::write("docs/PARAMETERS.md", out).unwrap();
}

#[test]
fn cells_stay_inside_their_zone_and_the_window() {
    let (mut e, _) = editor(ThemeId::Light);
    for selected in 0..=dither_page() {
        for tall in [false, true] {
            e.open_page(selected);
            e.set_tall(tall);
            for more in [false, true] {
                if more && !e.ui.page.has_more() {
                    continue;
                }
                e.ui.more = more;
                for state in 0..e.ui.page.states() {
                    e.ui.page.set_state(state);
                    e.rebuild();
                    let z = e.ui.zones();
                    for cell in e.ui.page_cells() {
                        let r = cell.rect;
                        let label = format!("{} / {}: {} ({})", slot_label(e.ui.slot()), state, cell.label, INFO.params[cell.param].id);
                        assert!(r.0 >= z.page.0 - 0.01 && r.1 >= z.page.1 - 0.01 && r.0 + r.2 <= z.page.0 + z.page.2 + 0.01 && r.1 + r.3 <= z.page.1 + z.page.3 + 0.01, "{label}: {r:?} outside the page {:?}", z.page);
                    }
                }
            }
            e.ui.more = false;
        }
    }
}

#[test]
fn renders_every_page_in_both_themes_and_modes() {
    for id in ThemeId::ALL {
        let (mut e, shared) = editor(id);
        feed(&shared, 500);
        for selected in 0..=dither_page() {
            e.open_page(selected);
            for tall in [false, true] {
                e.set_tall(tall);
                e.tick(1.0 / 30.0);
                let f = e.render(1.0);
                assert_eq!((f.width, f.height), (WIDTH as usize, HEIGHT as usize));
            }
        }
    }
}

#[test]
fn an_idle_editor_draws_nothing_and_audio_redraws_only_live_parts() {
    let (mut e, shared) = editor(ThemeId::Light);
    // The page's tick may ask for a repaint; the shell itself must be idle.
    e.ui.page = Box::new(Quiet);
    e.rebuild();
    e.render(1.0);
    e.tick(1.0 / 30.0);
    e.render(1.0);
    e.tick(1.0 / 30.0);
    assert_eq!(e.damage(), Some(vec![]), "nothing changed");
    feed(&shared, 10);
    e.tick(1.0 / 30.0);
    let d = e.damage().expect("partial");
    assert!(d.iter().any(|r| r.touches(&dmg(PLOT))));
}

/// A page with nothing on it, for the idle test.
struct Quiet;
impl Page for Quiet {
    fn cells(&self, _: &Ctx) -> Vec<Cell> {
        Vec::new()
    }
}

#[test]
fn a_damaged_frame_equals_a_full_repaint() {
    for scale in [1.0, 2.0] {
        let (mut part, shared_a) = editor(ThemeId::Dark);
        let (mut whole, shared_b) = editor(ThemeId::Dark);
        part.render(scale);
        whole.render(scale);
        for step in 0..6 {
            for (e, shared) in [(&mut part, &shared_a), (&mut whole, &shared_b)] {
                feed(shared, 7);
                e.param_changed(P_THRESHOLD, -3.0 * step as f32);
                e.param_changed(P_GAIN_L, step as f32);
                e.param_changed(LAYOUT.slots[3].on, (step % 2) as f32);
                e.param_changed(LAYOUT.dither_base, (step % 3) as f32);
                e.param_changed(LAYOUT.input_gain[0], step as f32);
                e.tick(1.0 / 30.0);
            }
            whole.damage.add_all();
            whole.live_dirty = true;
            let expected = whole.render(scale).pixels.to_vec();
            let got = part.render(scale);
            let w = got.width;
            let bad: Vec<usize> = (0..got.pixels.len()).filter(|&i| got.pixels[i] != expected[i]).collect();
            let bbox = bad.iter().fold((usize::MAX, usize::MAX, 0, 0), |b, &i| (b.0.min(i % w), b.1.min(i / w), b.2.max(i % w), b.3.max(i / w)));
            assert!(bad.is_empty(), "scale {scale}, step {step}: {} pixels differ, device bbox {bbox:?}", bad.len());
        }
    }
}

#[test]
fn the_threshold_grip_follows_the_pointer_while_the_first_limiter_is_on() {
    let (mut e, _) = editor(ThemeId::Light);
    let grip = e.ui.cells.iter().find(|c| c.kind == Kind::Grip).expect("grip").rect;
    let x = grip.0 + grip.2 * 0.5;
    let y = |db: f32| PLOT.1 + live::plot_y(db, PLOT);
    e.mouse_down(x, y(0.0), 1);
    e.mouse_move(x, y(-12.0));
    e.mouse_up(x, y(-12.0));
    assert!((e.ui.value(P_THRESHOLD) + 12.0).abs() < 0.2, "{}", e.ui.value(P_THRESHOLD));
    e.param_changed(LAYOUT.slots[0].on, 0.0);
    assert!(!e.ui.cells.iter().any(|c| c.kind == Kind::Grip));
}

#[test]
fn the_rack_adds_opens_switches_reorders_and_removes_modules() {
    let (mut e, _) = editor(ThemeId::Light);
    let shown = |e: &OpenLimiterEditor| e.ui.rack().chips.iter().map(|(s, _)| *s).collect::<Vec<_>>();
    assert_eq!(shown(&e), vec![0], "only the limiter at first");
    // Add the band compressor from "+".
    let add = e.ui.rack().add.expect("+");
    click(&mut e, add);
    let PopFor::Add(slots) = e.ui.pop.as_ref().expect("list").what.clone() else { panic!("add list") };
    let bands = slot_of("bands");
    let item = slots.iter().position(|&s| s == bands).unwrap();
    e.choose(item);
    assert_eq!(shown(&e), vec![0, bands]);
    assert_eq!(e.ui.selected, bands, "the new module opens");
    assert!(e.ui.on(LAYOUT.slots[bands].on));
    // Its dot switches it off; it stays in the rack.
    let r = e.ui.rack().chips[1].1;
    click(&mut e, chip_dot(r));
    assert!(!e.ui.on(LAYOUT.slots[bands].on));
    assert_eq!(shown(&e), vec![0, bands]);
    // Drag it in front of the limiter.
    let (from, to) = ((r.0 + r.2 * 0.5, r.1 + 10.0), (e.ui.rack().chips[0].1 .0 + 2.0, r.1 + 10.0));
    e.mouse_down(from.0, from.1, 1);
    e.mouse_move(to.0, to.1);
    e.mouse_up(to.0, to.1);
    assert_eq!(shown(&e), vec![bands, 0]);
    // Clicking a chip opens its page.
    let limiter = e.ui.rack().chips[1].1;
    click(&mut e, (limiter.0 + 30.0, limiter.1 + 4.0, 4.0, 4.0));
    assert_eq!(e.ui.selected, 0);
    // Drag the band compressor down off the rack: it is removed.
    let r = e.ui.rack().chips[0].1;
    e.mouse_down(r.0 + r.2 * 0.5, r.1 + 10.0, 1);
    e.mouse_move(r.0 + r.2 * 0.5, r.1 + 120.0);
    e.mouse_up(r.0 + r.2 * 0.5, r.1 + 120.0);
    assert_eq!(shown(&e), vec![0]);
    assert!(!e.ui.on(LAYOUT.slots[bands].on));
}

#[test]
fn a_dropdown_lists_its_choices_and_sets_one() {
    let (mut e, _) = editor(ThemeId::Light);
    e.open_page(dither_page());
    let cell = e.ui.page_cells().iter().find(|c| c.kind == Kind::Dropdown).cloned().expect("a dropdown on the dither page");
    click(&mut e, cell.rect);
    let pop = e.ui.pop.as_ref().expect("open");
    assert_eq!(pop.list.items.len(), kit::choices(&cell).len());
    let r = pop.list.rect;
    let row = (r.0 + 10.0, r.1 + 4.0 + 2.0 * Popover::ROW, 4.0, 4.0);
    let want = pop.list.item_at(row.0 + 2.0, row.1 + 2.0).unwrap();
    click(&mut e, row);
    assert!(e.ui.pop.is_none());
    assert_eq!(e.ui.values[cell.param], INFO.params[cell.param].min + want as f32);
}

#[test]
fn link_gangs_the_input_gains() {
    let (mut e, _) = editor(ThemeId::Light);
    let [l, r] = LAYOUT.input_gain;
    let fader = e.ui.cells.iter().find(|c| c.param == l && c.kind == Kind::FaderV).unwrap().rect;
    let x = fader.0 + fader.2 * 0.5;
    let y0 = fader.1 + fader.3 * 0.5;
    e.mouse_down(x, y0, 1);
    e.mouse_move(x, y0 - 25.0);
    e.mouse_up(x, y0 - 25.0);
    assert!(e.ui.value(l) > 1.0, "{}", e.ui.value(l));
    assert!((e.ui.value(l) - e.ui.value(r)).abs() < 1e-4, "linked");
}

#[test]
fn the_setup_sheet_holds_the_alignment() {
    let (mut e, _) = editor(ThemeId::Light);
    assert!(!e.ui.cells.iter().any(|c| c.param == P_DELAY_L));
    click(&mut e, SETUP_BUTTON);
    assert!(e.ui.cells.iter().any(|c| c.param == P_DELAY_L));
    click(&mut e, SETUP_DONE);
    assert!(!e.ui.setup_open);
}

#[test]
fn loudness_reaches_the_bar_and_reset_asks_the_engine() {
    let (mut e, shared) = editor(ThemeId::Light);
    feed(&shared, 100);
    e.tick(1.0 / 30.0);
    assert_eq!(e.ui.loud.integrated, -15.2);
    assert_eq!(e.ui.loud.integrated_in, -20.0);
    click(&mut e, RESET_CHIP);
    assert!(shared.take_loud_reset());
}

#[test]
fn a_new_editor_starts_from_the_engine_values() {
    let shared = Arc::new(Shared::default());
    shared.set_value(P_THRESHOLD, -7.0);
    shared.set_value(LAYOUT.slots[slot_of("wall")].on, 1.0);
    let mut e = OpenLimiterEditor::with_theme(EditorHost::detached(), ThemeId::Light);
    e.bind(Some(shared));
    assert_eq!(e.ui.value(P_THRESHOLD), -7.0);
    assert!(e.ui.rack().chips.iter().any(|(s, _)| *s == slot_of("wall")), "a module that is on is in the rack");
}

#[test]
fn ab_switches_between_two_settings() {
    let (mut e, _) = editor(ThemeId::Light);
    e.param_changed(P_THRESHOLD, -3.0);
    click(&mut e, AB);
    assert_eq!(e.ui.ab_on, 1);
    e.param_changed(P_THRESHOLD, -9.0);
    click(&mut e, AB);
    assert!((e.ui.value(P_THRESHOLD) + 3.0).abs() < 1e-3);
    click(&mut e, AB);
    assert!((e.ui.value(P_THRESHOLD) + 9.0).abs() < 1e-3);
}

#[test]
fn the_window_scales_and_keeps_its_shape() {
    let (mut e, _) = editor(ThemeId::Light);
    e.resize(1800, 100);
    assert_eq!(e.size(), (1800, 1290));
    let f = e.render(1.0);
    assert_eq!((f.width, f.height), (1800, 1290));
    // A click lands in design points.
    e.mouse_down(SETUP_BUTTON.0 * 1.5 + 10.0, SETUP_BUTTON.1 * 1.5 + 10.0, 1);
    assert!(e.ui.setup_open);
}

/// Writes `build/ui-<page>-<theme>.ppm` for every page. Run with `--ignored`.
#[test]
#[ignore]
fn dump_editor() {
    let only = std::env::var("PAGES").ok();
    std::fs::create_dir_all(format!("{}/build", env!("CARGO_MANIFEST_DIR"))).expect("build dir");
    for (id, name) in ThemeId::ALL.map(|t| (t, t.name())) {
        for selected in 0..=dither_page() {
            let key = LAYOUT.slots.get(selected).map_or("dither", |s| if s.key.is_empty() { "limiter" } else { s.key });
            if only.as_deref().is_some_and(|o| !o.split(',').any(|k| k == key)) {
                continue;
            }
            let (mut e, shared) = editor(id);
            e.set_target(3, false);
            feed(&shared, WINDOW);
            feed(&shared, 4000);
            e.param_changed(P_THRESHOLD, -6.0);
            e.param_changed(P_SOFT_CLIP, -3.0);
            for slot in [slot_of("bands"), slot_of("eq"), selected.min(dither_page() - 1)] {
                e.param_changed(LAYOUT.slots[slot].on, 1.0);
            }
            e.param_changed(LAYOUT.dither_base, 5.0);
            e.open_page(selected);
            for _ in 0..4 {
                feed(&shared, 7);
                shared.raise_slot_gr(selected.min(dither_page() - 1), 3.0);
                e.tick(1.0 / 30.0);
            }
            let f = e.render(2.0);
            let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
            for px in f.pixels {
                out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
            }
            std::fs::write(format!("{}/build/ui-{key}-{name}.ppm", env!("CARGO_MANIFEST_DIR")), out).expect("write");
        }
    }
}

fn write_ppm(e: &mut OpenLimiterEditor, scale: f32, path: &str) {
    let f = e.render(scale);
    let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
    for px in f.pixels {
        out.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
    }
    std::fs::write(path, out).expect("write");
}

/// The shell's own states: setup, lists, strip, key, MORE, at 1x and 2x. Writes
/// `build/shell/<state>-<theme>-<scale>x.ppm`. Run with `--ignored`.
#[test]
#[ignore]
fn dump_shell() {
    let dir = format!("{}/build/shell", env!("CARGO_MANIFEST_DIR"));
    std::fs::create_dir_all(&dir).expect("dir");
    let states = ["main", "strip", "setup", "presets", "add", "menu", "chip", "key", "nokey", "dither", "more"];
    for id in ThemeId::ALL {
        for state in states {
            let (mut e, shared) = editor(id);
            e.set_target(3, false);
            feed(&shared, WINDOW);
            e.param_changed(P_THRESHOLD, -6.0);
            e.param_changed(P_SOFT_CLIP, -3.0);
            for key in ["bands", "dual", "eq", "wall", "deq"] {
                e.param_changed(LAYOUT.slots[slot_of(key)].on, 1.0);
            }
            e.param_changed(LAYOUT.dither_base, 5.0);
            if state == "key" {
                shared.set_key_present(true);
                shared.raise_key(0.3);
            }
            match state {
                "strip" => {
                    e.open_page(slot_of("peq"));
                    e.set_tall(true);
                }
                "key" | "nokey" => {
                    // External key on the dual compressor and the dynamic EQ.
                    for key in ["deq", "dual"] {
                        e.open_page(slot_of(key));
                        if let Some(c) = e.ui.cells.iter().find(|c| c.label == "KEY").cloned() {
                            e.param_changed(c.param, 1.0);
                        }
                    }
                }
                "dither" => e.open_page(dither_page()),
                "more" => {
                    e.open_page(slot_of("wall"));
                    if e.ui.page.has_more() {
                        e.ui.more = true;
                        e.rebuild();
                    }
                }
                _ => e.open_page(0),
            }
            for _ in 0..4 {
                feed(&shared, 7);
                if state == "key" {
                    shared.set_key_present(true);
                    shared.raise_key(0.3);
                }
                shared.raise_slot_gr(e.ui.selected.min(dither_page() - 1), 3.0);
                e.tick(1.0 / 30.0);
            }
            match state {
                "setup" => e.set_setup(true),
                "presets" => click(&mut e, PRESET_BOX),
                "add" => {
                    if let Some(a) = e.ui.rack().add {
                        click(&mut e, a);
                    }
                }
                "menu" => {
                    let m = e.menu_button();
                    click(&mut e, m);
                }
                "chip" => {
                    let r = e.ui.rack().chips[1].1;
                    e.right_mouse_down(r.0 + 40.0, r.1 + 10.0);
                }
                _ => {}
            }
            for scale in [1.0, 2.0] {
                e.damage.add_all();
                write_ppm(&mut e, scale, &format!("{dir}/{state}-{}-{scale:.0}x.ppm", id.name()));
            }
        }
    }
}

#[test]
fn a_dimmed_cell_ignores_the_wheel_and_a_gui_range_bounds_edits() {
    let (mut e, _) = editor(ThemeId::Light);
    let dual = slot_of("dual");
    e.param_changed(LAYOUT.slots[dual].on, 1.0);
    e.open_page(dual);
    e.ui.more = true;
    e.rebuild();
    let cell = e.ui.cells.iter().find(|c| c.dim && INFO.params[c.param].id.ends_with("outputGain")).cloned().expect("post trim box");
    let before = e.ui.values[cell.param];
    let (x, y) = (cell.rect.0 + 4.0, cell.rect.1 + 4.0);
    for _ in 0..20 {
        e.wheel(x, y, 1.0);
    }
    assert_eq!(e.ui.values[cell.param], before, "dimmed: unchanged");
}


#[test]
fn every_module_fits_in_the_rack_at_once() {
    let (mut e, _) = editor(ThemeId::Light);
    for s in 0..LAYOUT.slots.len() {
        e.param_changed(LAYOUT.slots[s].on, 1.0);
    }
    let rack = e.ui.rack();
    assert_eq!(rack.chips.len(), LAYOUT.slots.len());
    assert!(rack.add.is_none());
    let last = rack.chips.last().unwrap().1;
    assert!(last.0 + last.2 <= rack.dither.0 - 4.0, "chips run into the dither: {last:?} vs {:?}", rack.dither);
    e.render(2.0);
}


#[test]
fn the_numbers_change_four_times_a_second_not_every_frame() {
    let (mut e, shared) = editor(ThemeId::Light);
    feed(&shared, 100);
    for _ in 0..600 {
        shared.push(Entry { input: 1.0, output: 1.0, gain: 1.0, ms: [0.5, 0.5] });
    }
    e.tick(1.0 / 30.0);
    assert_eq!(e.ui.readout.integrated, -15.2, "the first tick shows the numbers at once");
    let first = e.ui.rms.expect("a reading");
    // Quieter audio for a few frames: the meters move, the numbers wait for the period.
    for _ in 0..60 {
        shared.push(Entry { input: 1.0, output: 1.0, gain: 1.0, ms: [0.005, 0.005] });
    }
    for _ in 0..5 {
        e.tick(1.0 / 30.0);
    }
    assert_eq!(e.ui.rms, Some(first), "held under {READOUT_PERIOD} s");
    for _ in 0..3 {
        e.tick(1.0 / 30.0);
    }
    assert!(e.ui.rms.unwrap() < first - 0.5, "taken after the period");
}

#[test]
fn the_rms_meter_reads_the_output_on_the_chosen_scale_and_window() {
    let (mut e, shared) = editor(ThemeId::Light);
    assert!(!e.ui.rms_aes, "plain RMS by default");
    assert_eq!(RMS_WINDOWS[e.ui.rms_window], 2.0, "2 s by default");
    e.ui.rms_aes = true;
    e.ui.rms_window = 2; // 2 s
    // A full-scale sine has a mean square of 0.5: 0 dB on the AES scale, -3.01 dB plain.
    for _ in 0..600 {
        shared.push(Entry { input: 1.0, output: 1.0, gain: 1.0, ms: [0.5, 0.25] });
    }
    e.tick(1.0 / 30.0);
    let aes = e.ui.rms.expect("a reading");
    assert!(aes.abs() < 0.01, "AES {aes}");
    e.ui.rms_aes = false;
    let plain = e.ui.rms_now().unwrap();
    assert!((plain + 3.0103).abs() < 0.01, "plain {plain}");
    // A quieter last 0.3 s shows in the short window only.
    for _ in 0..60 {
        shared.push(Entry { input: 1.0, output: 1.0, gain: 1.0, ms: [0.005, 0.005] });
    }
    e.tick(1.0 / 30.0);
    e.ui.rms_window = 0;
    let short = e.ui.rms_now().unwrap();
    e.ui.rms_window = 2;
    let long = e.ui.rms_now().unwrap();
    assert!((short + 23.01).abs() < 0.05 && long > -6.0, "short {short}, long {long}");
    // The setup sheet switches both and remembers nothing else.
    click(&mut e, SETUP_BUTTON);
    click(&mut e, (SETUP_RMS_SCALE.0, SETUP_RMS_SCALE.1, 40.0, 24.0));
    assert!(e.ui.rms_aes);
    click(&mut e, (SETUP_RMS_SCALE.0 + SETUP_RMS_SCALE.2 - 40.0, SETUP_RMS_SCALE.1, 40.0, 24.0));
    assert!(!e.ui.rms_aes);
    let key = SETUP_RMS_WINDOW.2 / RMS_WINDOWS.len() as f32;
    click(&mut e, (SETUP_RMS_WINDOW.0 + 4.0 * key, SETUP_RMS_WINDOW.1, key, 24.0));
    assert_eq!(RMS_WINDOWS[e.ui.rms_window], 5.0);
}

/// Autogain and Delta on: `build/monitor-<theme>.ppm`. Run with `--ignored`.
#[test]
#[ignore]
fn dump_monitor() {
    std::fs::create_dir_all(format!("{}/build", env!("CARGO_MANIFEST_DIR"))).expect("build dir");
    for id in ThemeId::ALL {
        let (mut e, shared) = editor(id);
        feed(&shared, WINDOW);
        e.param_changed(LAYOUT.autogain, 1.0);
        e.param_changed(LAYOUT.delta, 1.0);
        shared.set_autogain_db(-4.24);
        e.tick(0.3);
        write_ppm(&mut e, 2.0, &format!("{}/build/monitor-{}.ppm", env!("CARGO_MANIFEST_DIR"), id.name()));
    }
}

#[test]
fn the_header_shows_the_gain_autogain_applies() {
    let (mut e, shared) = editor(ThemeId::Dark);
    e.param_changed(LAYOUT.autogain, 1.0);
    shared.set_autogain_db(-4.24);
    e.tick(0.3);
    assert_eq!(e.ui.autogain_db, -4.2);
    assert!(e.ui.cells.iter().any(|c| c.param == LAYOUT.autogain));
    assert!(e.ui.cells.iter().any(|c| c.param == LAYOUT.delta));
}

/// The host parameter layout as JSON for the host sweep: `build/layout.json`. Run with `--ignored`.
#[test]
#[ignore]
fn dump_layout() {
    let l = &*LAYOUT;
    let slots: Vec<String> = l
        .slots
        .iter()
        .map(|s| format!("{{\"key\":\"{}\",\"label\":\"{}\",\"base\":{},\"count\":{},\"on\":{},\"position\":{}}}", s.key, s.label, s.base, s.count, s.on, s.position))
        .collect();
    let names: Vec<String> = INFO.params.iter().map(|p| format!("\"{}\"", p.id)).collect();
    let json = format!(
        "{{\"slots\":[{}],\"dither_base\":{},\"dither_count\":{},\"input_gain\":[{},{}],\"input_link\":{},\"rack_shown\":{},\"autogain\":{},\"delta\":{},\"ids\":[{}]}}",
        slots.join(","), l.dither_base, l.dither_count, l.input_gain[0], l.input_gain[1], l.input_link, l.rack_shown, l.autogain, l.delta, names.join(",")
    );
    std::fs::write(format!("{}/build/layout.json", env!("CARGO_MANIFEST_DIR")), json).expect("write");
}
