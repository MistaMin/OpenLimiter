//! Filter types (48, OpenEQ index order), stereo placements and the type rules.

/// The 48 filter types in OpenEQ's stored index order (append only).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    Bell = 0,
    LowShelf,
    HighShelf,
    LowCut,
    HighCut,
    Notch,
    BandPass,
    TiltShelf,
    AllPass,
    SoftBell,
    FlatTilt,
    FlatTop,
    NarrowResonance,
    SteepLowCut,
    SteepHighCut,
    Sword,
    AllPassHigh,
    EHighPass,
    ELowPass,
    ELowShelf,
    EHighShelf,
    ELmf,
    EHmf,
    ELowBell,
    EHighBell,
    GLmf,
    BritNHighPass,
    BritNLowShelf,
    BritNBell,
    BritNHighShelf,
    Eq250Bell,
    Eq250LowShelf,
    Eq250HighShelf,
    VtHighPass,
    VtLowPass,
    VtLowShelf,
    VtBoost,
    VtAtten,
    VtLmf,
    VtHmf,
    ToneLow,
    ToneHigh,
    BlueLow,
    GLowShelf,
    GHighShelf,
    BlueHigh,
    GHighPass,
    GLowPass,
}

pub const FILTER_TYPE_COUNT: usize = 48;

/// Labels in index order. OpenEQ's names, except the two steep cuts.
pub const FILTER_TYPE_NAMES: [&str; FILTER_TYPE_COUNT] = [
    "Bell", "Low Shelf", "High Shelf", "High Pass", "Low Pass", "Notch", "Band Pass", "Tilt Shelf",
    "Allpass L", "Soft Bell", "Flat Tilt", "Flat Top", "Narrow Resonance",
    "Steep HP", "Steep LP", "Sword", "Allpass H",
    "E HP", "E LP", "E LS", "E HS", "E LMF", "E HMF", "E Low Bell", "E High Bell",
    "G LMF",
    "BritN HP", "BritN LS", "BritN Bell", "BritN HS",
    "250 Bell", "250 LS", "250 HS",
    "VT HP", "VT LP", "VT LS", "VT Boost", "VT Atten", "VT LMF", "VT HMF",
    "Tone Low", "Tone High",
    "Blue Low", "G LS", "G HS", "Blue High", "G HP", "G LP",
];

const ALL: [FilterType; FILTER_TYPE_COUNT] = {
    use FilterType::*;
    [
        Bell, LowShelf, HighShelf, LowCut, HighCut, Notch, BandPass, TiltShelf, AllPass, SoftBell, FlatTilt, FlatTop,
        NarrowResonance, SteepLowCut, SteepHighCut, Sword, AllPassHigh, EHighPass, ELowPass, ELowShelf, EHighShelf, ELmf,
        EHmf, ELowBell, EHighBell, GLmf, BritNHighPass, BritNLowShelf, BritNBell, BritNHighShelf, Eq250Bell, Eq250LowShelf,
        Eq250HighShelf, VtHighPass, VtLowPass, VtLowShelf, VtBoost, VtAtten, VtLmf, VtHmf, ToneLow, ToneHigh, BlueLow,
        GLowShelf, GHighShelf, BlueHigh, GHighPass, GLowPass,
    ]
};

impl FilterType {
    pub fn from_index(i: usize) -> FilterType {
        ALL[i.min(FILTER_TYPE_COUNT - 1)]
    }
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        FILTER_TYPE_NAMES[self as usize]
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StereoMode {
    Stereo = 0,
    Mid,
    Side,
    Left,
    Right,
    /// Both mid and side pass through the band, weighted by the pan.
    MidSide,
}

pub const PLACEMENT_NAMES: [&str; 6] = ["Stereo", "Mid", "Side", "Left", "Right", "Mid Side"];

impl StereoMode {
    pub fn from_index(i: usize) -> StereoMode {
        match i {
            1 => StereoMode::Mid,
            2 => StereoMode::Side,
            3 => StereoMode::Left,
            4 => StereoMode::Right,
            5 => StereoMode::MidSide,
            _ => StereoMode::Stereo,
        }
    }
}

/// The slope reaches the designs through one integer: values from `ENCODED_SLOPE_BASE` up carry
/// the normalised slope (0..1, 1e-4 steps); smaller values are plain dB per octave (legacy).
pub const ENCODED_SLOPE_BASE: i32 = 1000;
pub fn encode_slope_norm(s: f32) -> i32 {
    let s = s.clamp(0.0, 1.0);
    ENCODED_SLOPE_BASE + (s * 10000.0 + 0.5) as i32
}
pub fn is_encoded_slope(slope_db: i32) -> bool {
    slope_db >= ENCODED_SLOPE_BASE
}
pub fn decode_slope_norm(slope_db: i32) -> f64 {
    (slope_db - ENCODED_SLOPE_BASE) as f64 / 10000.0
}
/// The steep cuts are a fixed fitted cascade; this is the slope reported for them.
pub const STEEP_SLOPE_DB: i32 = 180;

/// How a family type is built from a base shape.
#[derive(Clone, Copy, Debug)]
pub struct TypeTraits {
    pub base: FilterType,
    /// 0 both ways, +1 raise only, -1 reduce only.
    pub gain_limit: i32,
    pub q_scale: f32,
    /// Above zero replaces the Q control.
    pub fixed_q: f32,
    /// All-pass with the phase transition moved to the top.
    pub mirror: bool,
    pub mid_sink: bool,
    pub high_loss: bool,
}

pub fn type_traits(t: FilterType) -> TypeTraits {
    use FilterType::*;
    let mut r = TypeTraits { base: t, gain_limit: 0, q_scale: 1.0, fixed_q: 0.0, mirror: false, mid_sink: false, high_loss: false };
    match t {
        Sword => {
            r.base = Bell;
            r.q_scale = 2.0;
        }
        AllPassHigh => {
            r.base = AllPass;
            r.mirror = true;
        }
        EHighPass | BritNHighPass | VtHighPass | GHighPass => r.base = LowCut,
        ELowPass | VtLowPass | GLowPass => r.base = HighCut,
        ELowShelf => {
            r.base = LowShelf;
            r.q_scale = 0.8;
        }
        EHighShelf => {
            r.base = HighShelf;
            r.q_scale = 0.8;
        }
        ELmf => {
            r.base = Bell;
            r.q_scale = 0.9;
        }
        EHmf => {
            r.base = Bell;
            r.q_scale = 1.1;
        }
        ELowBell | EHighBell => {
            r.base = Bell;
            r.fixed_q = 0.7;
        }
        GLmf => {
            r.base = Bell;
            r.q_scale = 1.2;
        }
        BritNLowShelf => r.base = LowShelf,
        BritNBell => r.base = Bell,
        BritNHighShelf => r.base = HighShelf,
        Eq250Bell => {
            r.base = Bell;
            r.q_scale = 0.8;
        }
        Eq250LowShelf => {
            r.base = LowShelf;
            r.fixed_q = 0.5;
        }
        Eq250HighShelf => {
            r.base = HighShelf;
            r.fixed_q = 0.5;
        }
        VtLowShelf => {
            r.base = LowShelf;
            r.mid_sink = true;
        }
        VtBoost => {
            r.base = Bell;
            r.gain_limit = 1;
        }
        VtAtten => {
            r.base = HighShelf;
            r.gain_limit = -1;
        }
        VtLmf => {
            r.base = Bell;
            r.q_scale = 0.8;
        }
        VtHmf => {
            r.base = Bell;
            r.q_scale = 1.2;
        }
        ToneLow => {
            r.base = LowShelf;
            r.high_loss = true;
        }
        ToneHigh => r.base = HighShelf,
        BlueLow => {
            r.base = LowShelf;
            r.gain_limit = 1;
        }
        BlueHigh => {
            r.base = HighShelf;
            r.gain_limit = 1;
        }
        GLowShelf => r.base = LowShelf,
        GHighShelf => r.base = HighShelf,
        _ => {}
    }
    r
}

pub fn shape_of(t: FilterType) -> FilterType {
    type_traits(t).base
}
pub fn is_steep(t: FilterType) -> bool {
    matches!(t, FilterType::SteepLowCut | FilterType::SteepHighCut)
}
pub fn is_all_pass(t: FilterType) -> bool {
    shape_of(t) == FilterType::AllPass
}
pub fn uses_gain(t: FilterType) -> bool {
    use FilterType::*;
    matches!(shape_of(t), Bell | LowShelf | HighShelf | TiltShelf | SoftBell | FlatTilt | FlatTop | NarrowResonance)
}
pub fn is_cut(t: FilterType) -> bool {
    matches!(shape_of(t), FilterType::LowCut | FilterType::HighCut) || is_steep(t)
}
/// Generic types designed with the continuous slope (filter order).
pub fn is_generic(t: FilterType) -> bool {
    use FilterType::*;
    matches!(t, Bell | LowShelf | HighShelf | Notch | BandPass | LowCut | HighCut)
}
/// Types whose slope parameter is the continuous slope. For VT LS it is the attenuation.
pub fn uses_continuous_slope(t: FilterType) -> bool {
    is_generic(t) || t == FilterType::VtLowShelf
}
pub fn uses_slope(t: FilterType) -> bool {
    uses_continuous_slope(t)
}
pub fn uses_q(t: FilterType) -> bool {
    use FilterType::*;
    if matches!(t, LowShelf | HighShelf) {
        return false;
    }
    if matches!(t, LowCut | HighCut) {
        return true;
    }
    if matches!(t, VtLowShelf | ToneLow | ToneHigh | BlueHigh) {
        return false;
    }
    !is_cut(t) && shape_of(t) != FlatTilt && type_traits(t).fixed_q <= 0.0
}
/// Dynamic gain moves the gain parameter; types without one cannot be dynamic.
pub fn supports_dynamics(t: FilterType) -> bool {
    uses_gain(t)
}
/// The kernel modes are magnitude designs; all-pass phase cannot be held.
pub fn supported_in_linear_phase(t: FilterType) -> bool {
    !is_all_pass(t)
}

/// Design frequency: at most 20 kHz and 0.49 of the rate, at least 10 Hz.
pub fn clamp_frequency(f: f32, sr: f64) -> f32 {
    let upper = 20000.0f64.min(sr * 0.49) as f32;
    let lower = 10.0f32;
    if !(upper > lower) {
        return lower;
    }
    if f.is_nan() {
        return lower;
    }
    f.clamp(lower, upper)
}

pub fn clamp_q(q: f32) -> f32 {
    if q.is_nan() {
        return 1.0;
    }
    q.clamp(0.025, 40.0)
}
