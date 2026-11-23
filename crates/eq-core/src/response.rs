//! Band design for all 48 types (OpenEQ `ResponseModel::designBandT`), stage evaluation, the 2x2
//! stereo placement matrix, and the Digital+ all-pass fit. Allocation-free.

use crate::cx::Cx;
use crate::design::{self, AnalogCascade, Biquad, Shape};
use crate::jfilt::{self, db_to_gain, Raw, UNITY};
use crate::types::*;
use crate::vintage::{self, VType};
use std::f64::consts::PI;

pub const MAX_STAGES: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct DesignedBand {
    pub ty: FilterType,
    pub frequency: f32,
    pub gain_db: f32,
    pub q: f32,
    pub slope_db: i32,
    pub stage_count: usize,
    /// Raw `[b0, b1, b2, a0, a1, a2]` per stage.
    pub raw: [Raw; MAX_STAGES],
}

impl Default for DesignedBand {
    fn default() -> Self {
        DesignedBand { ty: FilterType::Bell, frequency: 1000.0, gain_db: 0.0, q: 1.0, slope_db: 12, stage_count: 1, raw: [UNITY; MAX_STAGES] }
    }
}

/// Stage normalised to a0 = 1: `[b0, b1, b2, a1, a2]`.
pub type Norm = [f64; 5];

pub const NORM_UNITY: Norm = [1.0, 0.0, 0.0, 0.0, 0.0];

#[inline]
pub fn normalize(raw: &Raw) -> Norm {
    let inv = if raw[3].abs() > 1.0e-300 { 1.0 / raw[3] } else { 0.0 };
    [raw[0] * inv, raw[1] * inv, raw[2] * inv, raw[4] * inv, raw[5] * inv]
}

/// Complex response of a normalised stage at `f` Hz (f clamped below Nyquist).
#[inline]
pub fn stage_response(c: &Norm, f: f64, sr: f64) -> Cx {
    let f = f.clamp(0.0, sr * 0.5 * 0.999);
    let z1 = Cx::polar(1.0, -2.0 * PI * f / sr);
    let z2 = z1 * z1;
    let num = c[0] + z1 * c[1] + z2 * c[2];
    let den = 1.0 + z1 * c[3] + z2 * c[4];
    num / den
}

#[inline]
pub fn stage_magnitude(c: &Norm, f: f64, sr: f64) -> f64 {
    if sr <= 0.0 || !f.is_finite() {
        return 1.0;
    }
    let m = stage_response(c, f, sr).abs();
    if m.is_finite() {
        m
    } else {
        1.0
    }
}

#[inline]
pub fn stage_phase(c: &Norm, f: f64, sr: f64) -> f64 {
    if sr <= 0.0 || !f.is_finite() {
        return 0.0;
    }
    stage_response(c, f, sr).arg()
}

impl DesignedBand {
    pub fn magnitude(&self, f: f64, sr: f64) -> f64 {
        let mut m = 1.0;
        for r in &self.raw[..self.stage_count] {
            m *= stage_magnitude(&normalize(r), f, sr);
        }
        m
    }
    pub fn response(&self, f: f64, sr: f64) -> Cx {
        let mut h = Cx::ONE;
        for r in &self.raw[..self.stage_count] {
            h *= stage_response(&normalize(r), f, sr);
        }
        h
    }
}

fn biquad_raw(b: &Biquad) -> Raw {
    [b.b0, b.b1, b.b2, 1.0, b.a1, b.a2]
}

/// One stage of a fitted shape: kind, log2 offset to the reference frequency, stage Q, fraction
/// of the commanded gain.
#[derive(Clone, Copy)]
struct Fitted(u8, f64, f64, f64);

fn build_fitted(d: &mut DesignedBand, sr: f64, reference: f64, stages: &[Fitted], gain_db: f32) {
    let mut n = 0;
    for st in stages.iter().take(MAX_STAGES) {
        let f = clamp_frequency((reference * 2f64.powf(st.1)) as f32, sr) as f64;
        let lin = db_to_gain((st.3 * gain_db as f64) as f32 as f64);
        let q = st.2 as f32 as f64;
        d.raw[n] = match st.0 {
            b'L' => jfilt::low_shelf(sr, f, q, lin),
            b'H' => jfilt::high_shelf(sr, f, q, lin),
            b'P' => jfilt::high_pass(sr, f, q),
            b'Q' => jfilt::low_pass(sr, f, q),
            _ => jfilt::peak(sr, f, q, lin),
        };
        n += 1;
    }
    d.stage_count = n;
}

fn impulse_invariant_low_pass(sr: f64, f: f32, q: f32) -> Raw {
    let omega = 2.0 * PI * f as f64;
    let zeta = 0.5 / q as f64;
    let period = 1.0 / sr;
    let r = (-zeta * omega * period).exp();
    let theta = omega * (1.0 - zeta * zeta).sqrt() * period;
    let a1 = -2.0 * r * theta.cos();
    let a2 = r * r;
    let b1 = 1.0 + a1 + a2;
    [0.0, b1, 0.0, 1.0, a1, a2]
}

fn impulse_low_pass_cascade_magnitude(sr: f64, f: f64, qs: &[f64], scale: f64) -> f64 {
    let mut m = 1.0;
    for &q in qs {
        let c = impulse_invariant_low_pass(sr, (f * scale) as f32, q as f32);
        let z = Cx::polar(1.0, -2.0 * PI * f / sr);
        let r = (z * c[1]) / (1.0 + z * c[4] + z * z * c[5]);
        m *= r.abs();
    }
    m
}

/// Designs one band. `slope_db` is either an encoded normalised slope or a legacy dB/oct value.
pub fn design_band(ty: FilterType, frequency: f32, gain_db: f32, q: f32, slope_db: i32, sr: f64) -> DesignedBand {
    let mut d = DesignedBand {
        ty,
        frequency: clamp_frequency(frequency, sr),
        gain_db,
        q: clamp_q(q),
        slope_db,
        stage_count: 1,
        raw: [UNITY; MAX_STAGES],
    };
    if sr <= 0.0 {
        return d;
    }
    let gain_db = if gain_db.is_finite() { gain_db } else { 0.0 };
    let f = d.frequency as f64;

    if is_encoded_slope(slope_db) && is_generic(ty) {
        let shape = match ty {
            FilterType::LowShelf => Shape::LowShelf,
            FilterType::HighShelf => Shape::HighShelf,
            FilterType::Notch => Shape::Notch,
            FilterType::BandPass => Shape::BandPass,
            FilterType::LowCut => Shape::HighPass,
            FilterType::HighCut => Shape::LowPass,
            _ => Shape::Bell,
        };
        let mut sec = [Biquad::default(); MAX_STAGES];
        let count = design::design(shape, sr, f, gain_db as f64, d.q as f64, decode_slope_norm(slope_db), &mut sec);
        if count == 0 {
            d.raw[0] = UNITY;
            d.stage_count = 1;
            return d;
        }
        for i in 0..count {
            d.raw[i] = biquad_raw(&sec[i]);
        }
        d.stage_count = count;
        return d;
    }

    let vt = match ty {
        FilterType::BlueLow => Some(VType::BlueLow),
        FilterType::BlueHigh => Some(VType::BlueHigh),
        FilterType::GHighPass => Some(VType::GHighPass),
        FilterType::ELowPass => Some(VType::ELowPass),
        FilterType::GLowPass => Some(VType::GLowPass),
        FilterType::VtLowPass => Some(VType::VtLowPass),
        FilterType::BritNHighPass => Some(VType::BritNHighPass),
        FilterType::VtLowShelf => Some(VType::VtLowShelf),
        FilterType::ToneLow => Some(VType::ToneLow),
        FilterType::ToneHigh => Some(VType::ToneHigh),
        _ => None,
    };
    if let Some(vt) = vt {
        let mut cascade = AnalogCascade::default();
        let vslope = if is_encoded_slope(slope_db) { decode_slope_norm(slope_db) } else { 0.0 };
        if !vintage::design_analog(vt, f, gain_db as f64, d.q as f64, &mut cascade, vslope) {
            d.raw[0] = UNITY;
            d.stage_count = 1;
            return d;
        }
        let mut sec = [Biquad::default(); MAX_STAGES];
        let count = design::digitise(&cascade, sr, &mut sec);
        for i in 0..count {
            d.raw[i] = biquad_raw(&sec[i]);
        }
        d.stage_count = count.max(1);
        return d;
    }

    if ty == FilterType::VtAtten {
        // Reduce-only high shelf whose knee, Q and gain follow the cut depth.
        let u = (-gain_db).max(0.0);
        let mut s = 0.0f32;
        if u > 0.0 && u <= 12.0 {
            s = -1.048316765 * u + 0.006819794 * u * u - 0.001110705 * u * u * u;
        } else if u > 12.0 {
            let s12 = -13.517f32;
            s = s12 + (u - 12.0) * (-18.246 - s12) / 3.0;
        }
        s = s.clamp(-18.246, 0.0);
        let qs = (0.490742950 + 0.005047484 * u - 0.000944086 * u * u + 0.000026823 * u * u * u).clamp(0.02, 12.0);
        let ck = 934.552396 - 102.560885 * u + 12.617993 * u * u - 0.330062 * u * u * u;
        let knee = ck * (d.frequency / 1033.0).max(1.0);
        let mut written = 0;
        if s.abs() >= 1.0e-3 {
            let fk = clamp_frequency(knee, sr) as f64;
            d.raw[0] = jfilt::high_shelf(sr, fk, qs as f64, db_to_gain(s as f64));
            written = 1;
        }
        if written == 0 {
            d.raw[0] = UNITY;
            written = 1;
        }
        d.stage_count = written;
        d.q = clamp_q(0.5);
        return d;
    }

    {
        use FilterType::*;
        const EQ250_LOW: [Fitted; 3] = [Fitted(b'L', 2.9218, 1.0351, -0.0440), Fitted(b'B', -0.0170, 0.2005, 1.0068), Fitted(b'H', 1.7977, 1.0938, 0.0284)];
        const EQ250_HIGH: [Fitted; 3] = [Fitted(b'L', 3.4258, 1.1647, 0.0110), Fitted(b'B', -0.0809, 0.2030, 0.8171), Fitted(b'H', 1.1638, 0.7193, 0.0455)];
        const E_HIGH_BELL: [Fitted; 3] = [Fitted(b'B', 1.9916, 0.0520, 0.1404), Fitted(b'B', -0.0223, 0.2982, 0.8721), Fitted(b'H', 2.3232, 0.7071, 0.0748)];
        const E_HIGH_SHELF: [Fitted; 3] = [Fitted(b'L', -0.1149, 1.1364, -0.0081), Fitted(b'B', 2.2222, 0.2845, 0.1236), Fitted(b'H', -0.6572, 0.4902, 0.9835)];
        const E_LOW_BELL: [Fitted; 1] = [Fitted(b'B', -0.0690, 0.2956, 1.0020)];
        const BRITN_LOW: [Fitted; 3] = [Fitted(b'L', 1.4000, 0.6381, 0.8131), Fitted(b'B', -3.7966, 0.1135, 0.1921), Fitted(b'H', 0.0925, 1.6072, -0.0482)];
        const BRITN_HIGH: [Fitted; 3] = [Fitted(b'L', -0.0379, 1.1411, -0.1100), Fitted(b'B', 0.2354, 0.6795, -0.1600), Fitted(b'H', -1.0403, 0.6086, 1.0216)];
        const FLAT_TOP: [Fitted; 2] = [Fitted(b'B', 0.2183, 1.1289, 0.2475), Fitted(b'B', -0.0693, 1.0216, 0.7841)];
        const G_LMF: [Fitted; 1] = [Fitted(b'B', 0.0053, 0.3474, 1.0004)];
        const E_HIGH_PASS: [Fitted; 2] = [Fitted(b'P', 0.1872, 0.7538, 0.0), Fitted(b'P', -6.9283, 0.7372, 0.0)];
        const VT_HIGH_PASS: [Fitted; 2] = [Fitted(b'P', -0.0523, 0.6828, 0.0), Fitted(b'P', -6.6508, 0.7073, 0.0)];
        const G_LOW_SHELF: [Fitted; 3] = [Fitted(b'L', 0.7375, 0.4724, 0.7671), Fitted(b'B', -3.1104, 0.0778, 0.2419), Fitted(b'H', -3.7309, 1.1020, -0.0036)];
        const G_HIGH_SHELF: [Fitted; 3] = [Fitted(b'L', -0.3110, 1.1597, -0.0508), Fitted(b'B', -0.4015, 0.5501, -0.1713), Fitted(b'H', -1.2205, 0.5862, 0.9925)];
        let fit: Option<(f64, &[Fitted], f32)> = match ty {
            Eq250LowShelf => Some((1000.0f64.min(f), &EQ250_LOW, 0.2)),
            Eq250HighShelf => Some((1000.0, &EQ250_HIGH, 0.2)),
            EHighBell => Some((f, &E_HIGH_BELL, 0.2)),
            EHighShelf => Some((f, &E_HIGH_SHELF, 0.2)),
            ELowBell => Some((f, &E_LOW_BELL, 0.3)),
            BritNLowShelf => Some((f, &BRITN_LOW, 0.6)),
            BritNHighShelf => Some((f, &BRITN_HIGH, 0.6)),
            FlatTop => Some((f, &FLAT_TOP, 1.1)),
            GLmf => Some((f, &G_LMF, 0.35)),
            GLowShelf => Some((f, &G_LOW_SHELF, 0.5)),
            EHighPass => Some((f, &E_HIGH_PASS, 0.74)),
            VtHighPass => Some((f, &VT_HIGH_PASS, 0.7)),
            GHighShelf => Some((f, &G_HIGH_SHELF, 0.6)),
            _ => None,
        };
        if let Some((reference, stages, q_shown)) = fit {
            build_fitted(&mut d, sr, reference, stages, gain_db);
            d.q = clamp_q(q_shown);
            return d;
        }
    }

    let traits = type_traits(ty);
    if traits.base != ty {
        let mut gain = gain_db;
        if traits.gain_limit > 0 {
            gain = gain.max(0.0);
        } else if traits.gain_limit < 0 {
            gain = gain.min(0.0);
        }
        let q_used = if traits.fixed_q > 0.0 { traits.fixed_q } else { q * traits.q_scale };
        let mut shape = design_band(traits.base, frequency, gain, q_used, slope_db, sr);
        shape.ty = ty;
        shape.gain_db = gain;
        shape.q = clamp_q(q_used);
        if traits.mirror {
            for r in shape.raw[..shape.stage_count].iter_mut() {
                r[1] = -r[1];
                r[4] = -r[4];
            }
        }
        if traits.mid_sink && shape.stage_count < MAX_STAGES {
            let fm = clamp_frequency(shape.frequency * 3.0, sr) as f64;
            shape.raw[shape.stage_count] = jfilt::peak(sr, fm, 0.7f32 as f64, db_to_gain((-0.25 * gain) as f64));
            shape.stage_count += 1;
        }
        if traits.high_loss && shape.stage_count < MAX_STAGES {
            let fh = clamp_frequency(4000.0, sr) as f64;
            shape.raw[shape.stage_count] = jfilt::high_shelf(sr, fh, 0.7f32 as f64, db_to_gain((-0.2 * gain.max(0.0)) as f64));
            shape.stage_count += 1;
        }
        return shape;
    }

    let lin = db_to_gain(d.gain_db as f64);
    let qd = d.q as f64;
    match ty {
        FilterType::Bell => d.raw[0] = jfilt::peak(sr, f, qd, lin),
        FilterType::LowShelf => d.raw[0] = jfilt::low_shelf(sr, f, qd, lin),
        FilterType::HighShelf => d.raw[0] = jfilt::high_shelf(sr, f, qd, lin),
        FilterType::TiltShelf => {
            // A low shelf at minus the gain cascaded with a high shelf at plus the gain, one pivot.
            let full = db_to_gain(d.gain_db as f64) as f32 as f64;
            let inv = db_to_gain(-d.gain_db as f64) as f32 as f64;
            d.stage_count = 2;
            d.raw[0] = jfilt::low_shelf(sr, f, qd, inv);
            d.raw[1] = jfilt::high_shelf(sr, f, qd, full);
        }
        FilterType::Notch => d.raw[0] = jfilt::notch(sr, f, qd),
        FilterType::BandPass => d.raw[0] = jfilt::band_pass(sr, f, qd),
        FilterType::AllPass => d.raw[0] = jfilt::all_pass(sr, f, qd),
        FilterType::SoftBell => {
            let q_eff = clamp_q(d.q * (1.0 + 0.15 * d.gain_db.abs() / 12.0));
            d.q = q_eff;
            d.raw[0] = jfilt::peak(sr, f, q_eff as f64, lin);
        }
        FilterType::FlatTilt => {
            // Straight line in dB against log frequency, about gain / 6 dB per octave, pivoting at
            // the frequency: 15 fixed shelves scaled by gain / 6, plus a flat offset.
            const GAINS_AT_6: [f32; 15] = [
                -1.69327, -1.83087, -5.98842, -5.32816, 2.00866, 6.91748, 0.42315, 8.65194, 7.76851, 3.25589, -5.18791, -0.01847,
                -3.70616, -3.70616, -3.70616,
            ];
            let shelf_q = 0.15f32;
            let scale = d.gain_db / 6.0;
            for (k, g) in GAINS_AT_6.iter().enumerate() {
                let pivot = clamp_frequency(1000.0 * 2f32.powf(k as f32 - 7.0), sr) as f64;
                d.raw[k] = jfilt::high_shelf(sr, pivot, shelf_q as f64, db_to_gain((g * scale) as f64));
            }
            let offset_db = scale * (1000.0 / d.frequency).log2();
            d.raw[15] = [db_to_gain(offset_db as f64), 0.0, 0.0, 1.0, 0.0, 0.0];
            d.stage_count = 16;
            d.q = shelf_q;
        }
        FilterType::NarrowResonance => {
            let q_eff = clamp_q(d.q * 4.0);
            d.q = q_eff;
            d.raw[0] = jfilt::peak(sr, f, q_eff as f64, lin);
        }
        FilterType::LowCut | FilterType::HighCut | FilterType::SteepLowCut | FilterType::SteepHighCut => {
            let high_pass = matches!(ty, FilterType::LowCut | FilterType::SteepLowCut);
            if is_steep(ty) {
                // Fitted steep cut: 15 bilinear sections whose frequency ratios and Qs reproduce
                // the measured knee and roll-off. The high pass mirrors the low pass (fc / ratio).
                const RATIOS: [f64; 15] = [1.0287, 0.9848, 0.5000, 1.7615, 0.8422, 1.8274, 3.2683, 1.5256, 2.4631, 2.3359, 0.6815, 1.7172, 2.3688, 1.7555, 2.3451];
                const QS: [f64; 15] = [21.3706, 13.7389, 1.0297, 0.3525, 6.0471, 0.3053, 5.1587, 0.3295, 0.5592, 0.3681, 2.2832, 0.3298, 0.3889, 0.3759, 0.2564];
                d.stage_count = 15;
                for i in 0..15 {
                    let sf = clamp_frequency((f * if high_pass { 1.0 / RATIOS[i] } else { RATIOS[i] }) as f32, sr) as f64;
                    let sq = QS[i] as f32 as f64;
                    d.raw[i] = if high_pass { jfilt::high_pass(sr, sf, sq) } else { jfilt::low_pass(sr, sf, sq) };
                }
                return d;
            }
            let poles = ((slope_db as f32 / 6.0).round() as i32).clamp(1, (MAX_STAGES * 2) as i32) as usize;
            if poles <= 1 {
                d.raw[0] = if high_pass { jfilt::first_order_high_pass(sr, f) } else { jfilt::first_order_low_pass(sr, f) };
                return d;
            }
            let sections = (poles / 2).min(MAX_STAGES);
            let mut qs = [0.0f64; MAX_STAGES];
            for k in 1..=sections {
                let theta = PI * (2.0 * k as f64 - 1.0) / (2.0 * poles as f64);
                qs[k - 1] = 1.0 / (2.0 * theta.cos());
            }
            d.stage_count = sections;
            let mut scale = 1.0;
            if !high_pass {
                let target = std::f64::consts::FRAC_1_SQRT_2;
                let (mut lo, mut hi) = (0.9, 1.1);
                for _ in 0..40 {
                    let mid = 0.5 * (lo + hi);
                    let v = impulse_low_pass_cascade_magnitude(sr, f, &qs[..sections], mid);
                    if v > target {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                scale = 0.5 * (lo + hi);
            }
            for i in 0..sections {
                let sq = qs[i] as f32;
                d.raw[i] = if high_pass {
                    jfilt::high_pass(sr, f, sq as f64)
                } else {
                    impulse_invariant_low_pass(sr, d.frequency * scale as f32, sq)
                };
            }
        }
        _ => {}
    }
    d
}

/// Wet weights for a pan position (-1..1): a full pan to one side removes the band from the
/// other. Centre is 1 and 1.
#[inline]
pub fn pan_weights(pan: f32) -> (f64, f64) {
    let p = (pan as f64).clamp(-1.0, 1.0);
    ((1.0 - p).min(1.0), (1.0 + p).min(1.0))
}

/// Real 2x2 gain at one frequency: [[ll, lr], [rl, rr]] (output row, input column).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub ll: f64,
    pub lr: f64,
    pub rl: f64,
    pub rr: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix { ll: 1.0, lr: 0.0, rl: 0.0, rr: 1.0 }
    }
}

pub fn band_matrix(h: f64, mode: StereoMode, pan: f32) -> Matrix {
    let h = if h.is_finite() { h } else { 1.0 };
    let (wa, wb) = pan_weights(pan);
    match mode {
        StereoMode::MidSide => {
            let mid = wa * h + (1.0 - wa);
            let side = wb * h + (1.0 - wb);
            Matrix { ll: 0.5 * (mid + side), lr: 0.5 * (mid - side), rl: 0.5 * (mid - side), rr: 0.5 * (mid + side) }
        }
        StereoMode::Left => Matrix { ll: h, lr: 0.0, rl: 0.0, rr: 1.0 },
        StereoMode::Right => Matrix { ll: 1.0, lr: 0.0, rl: 0.0, rr: h },
        StereoMode::Mid => {
            let a = 0.5 * h + 0.5;
            let b = 0.5 * h - 0.5;
            Matrix { ll: a, lr: b, rl: b, rr: a }
        }
        StereoMode::Side => {
            let a = 0.5 + 0.5 * h;
            let b = 0.5 - 0.5 * h;
            Matrix { ll: a, lr: b, rl: b, rr: a }
        }
        StereoMode::Stereo => Matrix { ll: wa * h + (1.0 - wa), lr: 0.0, rl: 0.0, rr: wb * h + (1.0 - wb) },
    }
}

/// newer x older.
pub fn multiply(n: Matrix, o: Matrix) -> Matrix {
    Matrix {
        ll: n.ll * o.ll + n.lr * o.rl,
        lr: n.ll * o.lr + n.lr * o.rr,
        rl: n.rl * o.ll + n.rr * o.rl,
        rr: n.rl * o.lr + n.rr * o.rr,
    }
}

/// Second-order all-pass for Digital+ phase compensation: grid-fits (radius, angle) so the section
/// plus the all-passes fitted so far deviates least from a straight phase line over
/// [low_ratio, high_ratio] x frequency.
pub fn digital_plus_all_pass(raw: &Raw, frequency: f32, sr: f64, low_ratio: f64, high_ratio: f64, existing: &[(f64, f64)]) -> (f64, f64) {
    // The original rounds the section to single precision before fitting.
    let rf: Raw = [raw[0] as f32 as f64, raw[1] as f32 as f64, raw[2] as f32 as f64, raw[3] as f32 as f64, raw[4] as f32 as f64, raw[5] as f32 as f64];
    let section = normalize(&rf);
    let ap_phase = |d1: f64, d2: f64, w: f64| {
        let z = Cx::polar(1.0, -w);
        ((d2 + z * d1 + z * z) / (1.0 + z * d1 + z * z * d2)).arg()
    };
    let mut best = (0.0, 0.0);
    let mut best_err = 1.0e300;
    for ri in 0..15 {
        let r = 0.1 + 0.06 * ri as f64;
        for ti in 0..9 {
            let theta = 0.15 + 0.0875 * ti as f64;
            let d1 = -2.0 * r * (theta * PI).cos();
            let d2 = r * r;
            let mut phases = [0.0f64; 5];
            let mut freqs = [0.0f64; 5];
            for k in 0..5 {
                let ratio = low_ratio + (high_ratio - low_ratio) * k as f64 / 4.0;
                let fk = (frequency as f64 * ratio).clamp(1.0, sr * 0.49);
                let w = 2.0 * PI * fk / sr;
                let mut ph = stage_phase(&section, fk, sr) + ap_phase(d1, d2, w);
                for &(e1, e2) in existing {
                    ph += ap_phase(e1, e2, w);
                }
                phases[k] = ph;
                freqs[k] = fk;
            }
            for k in 1..5 {
                while phases[k] - phases[k - 1] > PI {
                    phases[k] -= 2.0 * PI;
                }
                while phases[k] - phases[k - 1] < -PI {
                    phases[k] += 2.0 * PI;
                }
            }
            let (mut sf, mut sp, mut sff, mut sfp) = (0.0, 0.0, 0.0, 0.0);
            for k in 0..5 {
                sf += freqs[k];
                sp += phases[k];
                sff += freqs[k] * freqs[k];
                sfp += freqs[k] * phases[k];
            }
            let den = 5.0 * sff - sf * sf;
            let slope = if den > 1.0e-12 { (5.0 * sfp - sf * sp) / den } else { 0.0 };
            let icpt = (sp - slope * sf) / 5.0;
            let mut err = 0.0;
            for k in 0..5 {
                let dd = phases[k] - (slope * freqs[k] + icpt);
                err += dd * dd;
            }
            if err < best_err {
                best_err = err;
                best = (d1, d2);
            }
        }
    }
    best
}
