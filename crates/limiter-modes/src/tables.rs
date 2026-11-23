//! Measured tables for the thirteen limiter styles, and the laws that turn a style and a
//! "character" value into envelope-follower settings.
//!
//! The spec gives tables, not algorithms. Every number that comes from the spec is cited
//! with its file and section. The way the numbers are turned into follower settings
//! (look-ahead ramp, fast and slow reduction shares, time constants) is this crate's own
//! design; see `REPORT.md` for the list of free choices.

/// Number of styles: ten maximizer modes followed by three character types.
pub const STYLE_COUNT: usize = 13;

/// Index of the first character type in the style list.
pub const FIRST_TYPE: usize = 10;

/// Own names for the ten modes (SPEC_LIMITERS.md sections 21 and 30, in table order) and
/// the three character types (params table of source E, in the order Analog-like,
/// valve-like, modern-like).
pub const STYLE_LABELS: [&str; STYLE_COUNT] = [
    "Clear", "Dense", "Surge", "Even", "Edge", "Slam", "Legacy", "Open", "Snap", "Quick", "Iron", "Glow",
    "Glass",
];

/// How the slow share of the reduction recovers.
#[derive(Clone, Copy)]
pub enum Family {
    /// Exponential in dB; the time constant grows with character.
    Tail,
    /// Held, then a short exponential; the hold grows with character.
    HoldDrop,
    /// No slow share at any character.
    Instant,
    /// Character type: all values come from five-point anchor tables.
    Anchored(&'static Anchors),
}

/// Values at character 0, 2, 5, 9 and 10 (the settings measured for source E).
pub struct Anchors {
    pub look_ms: [f32; 5],
    pub slow_share: [f32; 5],
    pub slow_tau_ms: [f32; 5],
    pub fast_tau_ms: [f32; 5],
    /// How far above the hard ceiling the envelope lets a steady tone, in dB.
    pub over_db: [f32; 5],
}

pub struct Style {
    /// Latency in samples at 48 kHz. Modes: SPEC_LIMITERS.md section 21
    /// (measurements/latency_srcD_modes.json). Types: 784, params table of source E.
    pub latency_48k: u32,
    /// Extra latency with the true-peak ceiling on. Modes: section 41.1 (534, and 102 for
    /// the low-latency mode). Types: 17 (784 -> 801).
    pub true_peak_extra_48k: u32,
    /// Gain added on top of the drive, in dB (source E: +0.35, +0.35, +0.12).
    pub offset_db: f32,
    /// The sample-peak limit relative to the ceiling control, in dB (source E: -0.01).
    pub limit_db: f32,
    pub family: Family,
    /// Look-ahead ramp length at character 9, in ms, fitted to the attack column of
    /// section 30 (measurements/srcD_presets_shots/mode_timing_char09.json).
    pub look9_ms: f32,
    /// Share of the reduction that recovers slowly at character 9.
    pub slow9: f32,
    /// Time constant of the slow share at character 9, in ms.
    pub tau9_ms: f32,
    /// Hold before the slow share starts to recover at character 9, in ms.
    pub hold9_ms: f32,
    /// The recovery is not smoothed (the ramp shapes the attack only).
    pub sharp_release: bool,
}

const fn mode(latency: u32, tp: u32, family: Family, look9: f32, slow9: f32, tau9: f32, hold9: f32) -> Style {
    Style {
        latency_48k: latency,
        true_peak_extra_48k: tp,
        offset_db: 0.0,
        limit_db: 0.0,
        family,
        look9_ms: look9,
        slow9,
        tau9_ms: tau9,
        hold9_ms: hold9,
        sharp_release: false,
    }
}

const fn kind(offset_db: f32, anchors: &'static Anchors) -> Style {
    Style {
        latency_48k: 784,
        true_peak_extra_48k: 17,
        offset_db,
        limit_db: -0.01,
        family: Family::Anchored(anchors),
        look9_ms: 0.0,
        slow9: 0.0,
        tau9_ms: 0.0,
        hold9_ms: 0.0,
        sharp_release: false,
    }
}

// Source E timing table (params table of source E, section "Timing"; data file
// measurements/srcE/all_jobs.json) and its six curves (spec/curves, the three-type
// files). The slow share and time constants below were derived from those by hand:
// release 63 % and 90 % times give the time constant and the share that recovers at once.
static IRON: Anchors = Anchors {
    look_ms: [0.02, 0.02, 0.02, 0.02, 0.02],
    slow_share: [0.69, 0.69, 0.69, 0.69, 0.69],
    slow_tau_ms: [405.0, 735.0, 1240.0, 1900.0, 2060.0],
    fast_tau_ms: [0.0; 5],
    over_db: [0.0; 5],
};
static GLOW: Anchors = Anchors {
    look_ms: [0.02, 0.62, 1.65, 4.2, 4.6],
    slow_share: [0.67, 0.67, 0.68, 0.69, 0.69],
    slow_tau_ms: [320.0, 1060.0, 2235.0, 4440.0, 5100.0],
    fast_tau_ms: [0.0, 0.0, 70.0, 105.0, 120.0],
    over_db: [0.0; 5],
};
static GLASS: Anchors = Anchors {
    look_ms: [0.05, 0.07, 0.13, 0.28, 0.32],
    slow_share: [0.0, 0.22, 0.21, 0.20, 0.20],
    slow_tau_ms: [60.0, 225.0, 470.0, 800.0, 880.0],
    fast_tau_ms: [0.0; 5],
    over_db: [0.08, 0.08, 0.046, 0.0, 0.0],
};

/// Latency: section 21. True-peak extra: section 41.1. Attack and release at character 9:
/// section 30 (repeated at 96 kHz in 39.1).
pub static STYLES: [Style; STYLE_COUNT] = [
    // 810 samples; attack -1.88 ms; release 104.6 / 267.8 ms.
    mode(810, 534, Family::Tail, 5.85, 0.845, 124.7, 0.0),
    // 3282; +0.22 ms; 104.6 / 267.8 ms.
    mode(3282, 534, Family::Tail, 0.88, 0.856, 124.7, 0.0),
    // 7465; +0.37 ms; 21.6 / 24.7 ms.
    mode(7465, 534, Family::HoldDrop, 0.53, 1.0, 2.37, 19.95),
    // 7465; +0.37 ms; 21.0 / 24.6 ms.
    mode(7465, 534, Family::HoldDrop, 0.53, 1.0, 2.71, 19.05),
    // 7465; +0.37 ms; 20.8 / 24.5 ms.
    mode(7465, 534, Family::HoldDrop, 0.53, 1.0, 2.78, 18.8),
    // 7465; +0.48 ms; release below the method floor / 0.78 ms.
    mode(7465, 534, Family::Instant, 0.28, 0.0, 1.0, 0.0),
    // 1833; -1.86 ms; 91.7 / 253.8 ms.
    mode(1833, 534, Family::Tail, 5.85, 0.765, 123.9, 0.0),
    // 5329; -0.59 ms; 58.7 / 196.6 ms.
    mode(5329, 534, Family::Tail, 2.7, 0.646, 105.4, 0.0),
    // 9385; -1.03 ms; release below the method floor.
    Style { sharp_release: true, ..mode(9385, 534, Family::Instant, 4.1, 0.0, 1.0, 0.0) },
    // 378; same timing as the first mode; true peak adds 102.
    mode(378, 102, Family::Tail, 5.85, 0.845, 124.7, 0.0),
    kind(0.35, &IRON),
    kind(0.35, &GLOW),
    kind(0.12, &GLASS),
];

/// Follower settings for one style at one character value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    /// Length of the look-ahead ramp (the gain smoother), ms.
    pub look_ms: f32,
    /// Extra peak-hold after the look-ahead window, ms.
    pub hold_ms: f32,
    /// Share of the reduction that recovers slowly, 0..1.
    pub slow_share: f32,
    pub slow_tau_ms: f32,
    /// Hold before the slow share starts to recover, ms.
    pub slow_hold_ms: f32,
    /// Time constant of the remaining share; 0 means at once.
    pub fast_tau_ms: f32,
    pub over_db: f32,
    pub sharp_release: bool,
    /// The ramp ends this long before the peak reaches the output, ms.
    pub early_ms: f32,
}

/// Longest look-ahead ramp any setting may ask for, ms.
pub const MAX_LOOK_MS: f32 = 7.0;
/// Longest extra peak-hold, ms.
pub const MAX_HOLD_MS: f32 = 0.6;
/// Longest lead of the ramp end over the peak, ms.
pub const MAX_EARLY_MS: f32 = 0.15;

const KNOTS: [f32; 5] = [0.0, 2.0, 5.0, 9.0, 10.0];

fn anchored(values: &[f32; 5], character: f32, geometric: bool) -> f32 {
    let mut k = 0;
    while k < 3 && character > KNOTS[k + 1] {
        k += 1;
    }
    let t = ((character - KNOTS[k]) / (KNOTS[k + 1] - KNOTS[k])).clamp(0.0, 1.0);
    let (a, b) = (values[k], values[k + 1]);
    if geometric && a > 0.0 && b > 0.0 {
        a * (b / a).powf(t)
    } else {
        a + (b - a) * t
    }
}

/// The law from style and character (0..10) to follower settings.
///
/// Modes: at character 2 every mode measured the same (section 30: attack 0.45 ms, release
/// below the method floor), so all modes share the character-2 ramp; the character-9
/// column sets the other end. The one mode with a third point (the first mode at
/// character 5, curve file `srcD_irc1__character_5.csv`) sets the shape in between.
pub fn timing(style: usize, character: f32) -> Timing {
    let s = &STYLES[if style < STYLE_COUNT { style } else { 0 }];
    let c = character.clamp(0.0, 10.0);
    if let Family::Anchored(a) = s.family {
        return Timing {
            look_ms: anchored(&a.look_ms, c, true).min(MAX_LOOK_MS),
            hold_ms: 0.55,
            slow_share: anchored(&a.slow_share, c, false),
            slow_tau_ms: anchored(&a.slow_tau_ms, c, false),
            slow_hold_ms: 0.0,
            fast_tau_ms: anchored(&a.fast_tau_ms, c, false),
            over_db: anchored(&a.over_db, c, false),
            sharp_release: false,
            early_ms: 0.08,
        };
    }
    const LOOK0: f32 = 0.06;
    const LOOK2: f32 = 0.35;
    let look_ms = if c < 2.0 {
        LOOK0 * (LOOK2 / LOOK0).powf(c / 2.0)
    } else {
        LOOK2 * (s.look9_ms / LOOK2).powf(((c - 2.0) / 7.0).powf(0.5))
    };
    let rise = (c / 9.0).powf(2.65);
    let (slow_share, slow_tau_ms, slow_hold_ms) = match s.family {
        Family::Tail => ((s.slow9 * rise).min(1.0), s.tau9_ms * (103.0 + 5.43 * (c - 5.0)) / 124.7, 0.0),
        Family::HoldDrop => ((s.slow9 * rise).min(1.0), s.tau9_ms, s.hold9_ms * c / 9.0),
        _ => (0.0, 1.0, 0.0),
    };
    Timing {
        look_ms: look_ms.min(MAX_LOOK_MS),
        hold_ms: 0.3 * (c / 2.0).min(1.0),
        slow_share,
        slow_tau_ms,
        slow_hold_ms,
        fast_tau_ms: 0.0,
        over_db: 0.0,
        sharp_release: s.sharp_release,
        early_ms: 0.12,
    }
}
