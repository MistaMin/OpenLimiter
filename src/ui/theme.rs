//! Colours and the remembered choices (theme, loudness target).

use std::sync::atomic::{AtomicU8, Ordering};

use openplugin::dri::goodlookindri::color::Color as C;

use super::raster::{blend, rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    /// White with red.
    Light = 0,
    /// Black with blue.
    Dark = 1,
    /// Charcoal with teal.
    Graphite = 2,
    /// Warm paper with indigo.
    Paper = 3,
}

impl ThemeId {
    pub const ALL: [ThemeId; 4] = [ThemeId::Light, ThemeId::Dark, ThemeId::Graphite, ThemeId::Paper];

    pub const fn name(self) -> &'static str {
        match self {
            ThemeId::Light => "light",
            ThemeId::Dark => "dark",
            ThemeId::Graphite => "graphite",
            ThemeId::Paper => "paper",
        }
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % Self::ALL.len()]
    }

    /// The next theme, for the header button.
    pub fn next(self) -> Self {
        Self::from_index(self as usize + 1)
    }

    /// Light text on a dark ground.
    pub const fn is_dark(self) -> bool {
        matches!(self, ThemeId::Dark | ThemeId::Graphite)
    }
}

pub struct Theme {
    pub bg: C,
    pub panel: C,
    pub inset: C,
    pub border: C,
    pub text: C,
    pub dim: C,
    pub accent: C,
    pub on_accent: C,
    pub ok: C,
    /// Loudness well under the target, a little over it, and far over it.
    pub cool: C,
    pub warm: C,
    pub hot: C,
    pub plot_bg: C,
    pub grid: C,
    pub grid_strong: C,
    pub in_fill: C,
    pub in_fill_alpha: f32,
    pub in_line: C,
    pub out_fill: C,
    pub out_fill_alpha: f32,
    pub out_line: C,
    pub seg_off: C,
    pub seg_on: C,
    pub knob_top: C,
    pub knob_bottom: C,
    pub shadow: C,
    /// Warnings: an output that can pass the ceiling, a clip, an over.
    pub over: C,
    /// Band colours, cycled for more than eight bands.
    pub bands: [C; 8],
}

/// White with red.
const LIGHT: Theme = Theme {
    bg: C(0xffefefec),
    panel: C(0xffffffff),
    inset: C(0xfff5f5f3),
    border: C(0xffdededa),
    text: C(0xff17191c),
    dim: C(0xff686d75),
    accent: C(0xffe3262d),
    on_accent: C(0xffffffff),
    ok: C(0xff1f9d57),
    cool: C(0xff6f8299),
    warm: C(0xffc47a0a),
    hot: C(0xffe3262d),
    plot_bg: C(0xfffbfbfa),
    grid: C(0xffececea),
    grid_strong: C(0xffd4d4d0),
    in_fill: C(0xffc6c9cf),
    in_fill_alpha: 0.55,
    in_line: C(0xffa3a7ae),
    out_fill: C(0xff23262b),
    out_fill_alpha: 0.78,
    out_line: C(0xff0f1113),
    seg_off: C(0xffe7e7e4),
    seg_on: C(0xff23262b),
    knob_top: C(0xffffffff),
    knob_bottom: C(0xffeaeae7),
    shadow: C(0x38000000),
    over: C(0xffe3262d),
    bands: [C(0xffd9480f), C(0xffb88a00), C(0xff4f9d1e), C(0xff0f9d8a), C(0xff1c7ed6), C(0xff6741d9), C(0xffc2255c), C(0xff5c6470)],
};

/// Black with blue.
const DARK: Theme = Theme {
    bg: C(0xff090a0c),
    panel: C(0xff131518),
    inset: C(0xff0c0d0f),
    border: C(0xff25292f),
    text: C(0xffedeff2),
    dim: C(0xff7d8590),
    accent: C(0xff2e8bff),
    on_accent: C(0xff07111d),
    ok: C(0xff35c979),
    cool: C(0xff2e8bff),
    warm: C(0xffffb224),
    hot: C(0xffff4d4d),
    plot_bg: C(0xff0b0c0e),
    grid: C(0xff181b1f),
    grid_strong: C(0xff2b3037),
    in_fill: C(0xff3a414c),
    in_fill_alpha: 0.6,
    in_line: C(0xff5d6673),
    out_fill: C(0xff8ea3be),
    out_fill_alpha: 0.3,
    out_line: C(0xffe3ebf5),
    seg_off: C(0xff1b1e23),
    seg_on: C(0xffd8dee6),
    knob_top: C(0xff262a31),
    knob_bottom: C(0xff16191d),
    shadow: C(0x90000000),
    over: C(0xffff4d4d),
    bands: [C(0xffff8a4c), C(0xfff2c94c), C(0xff8ed05a), C(0xff3fd0bd), C(0xff5ab0ff), C(0xff9d86ff), C(0xffff6fa3), C(0xff9aa4b2)],
};

/// Charcoal with teal: a softer dark for long sessions.
const GRAPHITE: Theme = Theme {
    bg: C(0xff1c1e22),
    panel: C(0xff25282d),
    inset: C(0xff1a1c20),
    border: C(0xff363a41),
    text: C(0xffe4e7ea),
    dim: C(0xff8b929c),
    accent: C(0xff22b89a),
    on_accent: C(0xff0e1412),
    ok: C(0xff5fcf7a),
    cool: C(0xff6f9fd8),
    warm: C(0xffe9a23b),
    hot: C(0xffff5c57),
    plot_bg: C(0xff17191c),
    grid: C(0xff23262b),
    grid_strong: C(0xff3a3f47),
    in_fill: C(0xff454b55),
    in_fill_alpha: 0.6,
    in_line: C(0xff6b7380),
    out_fill: C(0xff9fb1bf),
    out_fill_alpha: 0.28,
    out_line: C(0xffe8eef2),
    seg_off: C(0xff23262b),
    seg_on: C(0xffcfd5dc),
    knob_top: C(0xff33373e),
    knob_bottom: C(0xff22252a),
    shadow: C(0x80000000),
    over: C(0xffff5c57),
    bands: [C(0xffff8f5a), C(0xffe9c46a), C(0xff94d26b), C(0xff4fd1c5), C(0xff6aaeff), C(0xffa48bff), C(0xffff7aa8), C(0xffa1a9b4)],
};

/// Warm paper with indigo: a soft light theme.
const PAPER: Theme = Theme {
    bg: C(0xffece6da),
    panel: C(0xfff8f4ec),
    inset: C(0xfff1ece1),
    border: C(0xffd9d1c2),
    text: C(0xff26231f),
    dim: C(0xff6a6357),
    accent: C(0xff3b4cc0),
    on_accent: C(0xffffffff),
    ok: C(0xff2f8f55),
    cool: C(0xff6b7f99),
    warm: C(0xffc27512),
    hot: C(0xffcf3a30),
    plot_bg: C(0xfffbf8f2),
    grid: C(0xffebe5d9),
    grid_strong: C(0xffd3cab9),
    in_fill: C(0xffcdc6b8),
    in_fill_alpha: 0.55,
    in_line: C(0xffa89f8f),
    out_fill: C(0xff2e2a25),
    out_fill_alpha: 0.72,
    out_line: C(0xff1a1815),
    seg_off: C(0xffe6dfd2),
    seg_on: C(0xff2e2a25),
    knob_top: C(0xfffdfbf7),
    knob_bottom: C(0xffe9e3d7),
    shadow: C(0x33000000),
    over: C(0xffcf3a30),
    bands: [C(0xffc8501c), C(0xffa07c00), C(0xff4a8f22), C(0xff13897c), C(0xff2f6fc0), C(0xff6a4bd0), C(0xffb52a62), C(0xff5f6670)],
};

pub fn theme(id: ThemeId) -> &'static Theme {
    match id {
        ThemeId::Light => &LIGHT,
        ThemeId::Dark => &DARK,
        ThemeId::Graphite => &GRAPHITE,
        ThemeId::Paper => &PAPER,
    }
}

/// The colour of a loudness `lu` above the target.
pub fn loud_colour(t: &Theme, lu: f32) -> C {
    let stops = [(-18.0, t.cool), (-3.0, t.ok), (1.0, t.warm), (4.0, t.hot)];
    let mut prev = stops[0];
    for stop in stops {
        if lu <= stop.0 {
            let m = if stop.0 > prev.0 { (lu - prev.0) / (stop.0 - prev.0) } else { 0.0 };
            return C(0xff00_0000 | blend(rgb(prev.1), stop.1, m));
        }
        prev = stop;
    }
    prev.1
}

/// The theme and the loudness target are one choice for every instance, remembered between sessions.
static THEME: AtomicU8 = AtomicU8::new(u8::MAX);
static TARGET: AtomicU8 = AtomicU8::new(u8::MAX);

pub fn pref_file(name: &str) -> Option<std::path::PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::path::PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        std::path::Path::new(&std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| Some(std::path::Path::new(&std::env::var_os("HOME")?).join(".config")))?
    };
    Some(base.join("OpenGrid/OpenLimiter").join(name))
}

fn theme_file() -> Option<std::path::PathBuf> {
    pref_file("theme")
}

/// The index into `super::TARGETS`. Without a saved choice it is -14 LUFS.
pub fn load_target() -> usize {
    let known = TARGET.load(Ordering::Relaxed) as usize;
    if known < super::TARGETS.len() {
        return known;
    }
    let saved = pref_file("target").and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| s.trim().parse::<f32>().ok());
    let index = saved.and_then(|v| super::TARGETS.iter().position(|t| (t - v).abs() < 0.01)).unwrap_or(3);
    TARGET.store(index as u8, Ordering::Relaxed);
    index
}

pub fn store_target(index: usize) {
    TARGET.store(index as u8, Ordering::Relaxed);
    if cfg!(test) {
        return;
    }
    if let Some(path) = pref_file("target") {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("{}", super::TARGETS[index]));
    }
}

pub fn load_theme() -> ThemeId {
    let known = THEME.load(Ordering::Relaxed) as usize;
    if known < ThemeId::ALL.len() {
        return ThemeId::from_index(known);
    }
    let saved = theme_file().and_then(|p| std::fs::read_to_string(p).ok());
    let id = ThemeId::ALL.into_iter().find(|t| saved.as_deref().map(str::trim) == Some(t.name())).unwrap_or(ThemeId::Light);
    THEME.store(id as u8, Ordering::Relaxed);
    id
}

pub fn store_theme(id: ThemeId) {
    THEME.store(id as u8, Ordering::Relaxed);
    if cfg!(test) {
        return;
    }
    if let Some(path) = theme_file() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, id.name());
    }
}

impl Theme {
    /// The colour of band `n` (from 0), cycling through eight.
    pub fn band(&self, n: usize) -> C {
        self.bands[n % 8]
    }

    /// Text that reads on a solid `fill` (a badge, a lamp): near-black on a light fill,
    /// white on a dark one, whichever has the higher contrast.
    pub fn ink_on(&self, fill: C) -> C {
        let lin = |shift: u32| {
            let c = ((fill.0 >> shift) & 255) as f32 / 255.0;
            if c <= 0.039_28 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        let l = 0.2126 * lin(16) + 0.7152 * lin(8) + 0.0722 * lin(0);
        // Contrast against white is 1.05 / (l + 0.05), against #111 about (l + 0.05) / 0.056.
        if 1.05 / (l + 0.05) >= (l + 0.05) / 0.056 { C(0xffffffff) } else { C(0xff111111) }
    }
}
