//! Generic bell, shelf, notch, band-pass and cut designs (OpenEQ `FilterDesign`).
//!
//! The slope sets the filter order, fractional values included. Bells and notches span
//! prototype order 1..8 (total 2..16), shelves 1..16, cuts and band-pass 0..16. Bells use
//! Butterworth parametric (G * prod B(W/r) / B(rW)) designs, shelves the same on a low/high-pass
//! variable, notch and band-pass use W = Q (s/w0 + w0/s). Fractional orders interpolate the
//! section coefficients of the two neighbouring integer orders. Cuts between order 1 and 2 use a
//! fitted table; below order 1 an Oustaloup approximation of a fractional first-order slope.
//!
//! Digitisation: impulse-invariant poles, numerator matched to the analogue magnitude at DC, the
//! section frequency and Nyquist. Everything here works on the stack and never allocates.

use crate::cx::Cx;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Bell,
    LowShelf,
    HighShelf,
    Notch,
    BandPass,
    LowPass,
    HighPass,
}

/// Digital biquad with a0 = 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }
}

/// One s-domain section: (b2 s^2 + b1 s + b0) / (a2 s^2 + a1 s + a0), s in rad/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnalogSection {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Default for AnalogSection {
    fn default() -> Self {
        AnalogSection { b0: 1.0, b1: 0.0, b2: 0.0, a0: 1.0, a1: 0.0, a2: 0.0 }
    }
}

pub const MAX_ANALOG_SECTIONS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct AnalogCascade {
    pub sections: [AnalogSection; MAX_ANALOG_SECTIONS],
    pub count: usize,
    pub gain: f64,
}

impl Default for AnalogCascade {
    fn default() -> Self {
        AnalogCascade { sections: [AnalogSection::default(); MAX_ANALOG_SECTIONS], count: 0, gain: 1.0 }
    }
}

/// Cut and band-pass order: 0 at slope 0, 2 at 0.5, 8 at 0.75, 16 at 1.
pub fn cut_order(s: f64) -> f64 {
    let s = s.clamp(0.0, 1.0);
    if s <= 0.5 {
        4.0 * s
    } else if s <= 0.75 {
        2.0 + 24.0 * (s - 0.5)
    } else {
        8.0 + 32.0 * (s - 0.75)
    }
}

/// Bell and notch prototype order (half the total order): 1 at slope 0, 2 at 0.5, 4 at 0.75, 8 at 1.
pub fn bell_order(s: f64) -> f64 {
    let s = s.clamp(0.0, 1.0);
    let label = if s <= 0.5 {
        12.0 + 24.0 * s
    } else if s <= 0.75 {
        24.0 + 96.0 * (s - 0.5)
    } else {
        48.0 + 192.0 * (s - 0.75)
    };
    label / 12.0
}

/// Shelf order: 1 at slope 0, 4 at 0.5, 8 at 0.75, 16 at 1.
pub fn shelf_order(s: f64) -> f64 {
    let s = s.clamp(0.0, 1.0);
    if s <= 0.5 {
        1.0 + 6.0 * s
    } else if s <= 0.75 {
        4.0 + 16.0 * (s - 0.5)
    } else {
        8.0 + 32.0 * (s - 0.75)
    }
}

/// Normalised slope that gives a cut of `db_per_oct` (inverse of the cut order map, order = dB / 6).
pub fn slope_norm_for_cut_db(db_per_oct: f32) -> f32 {
    let order = (db_per_oct / 6.0).clamp(0.0, 16.0);
    if order <= 2.0 {
        order / 4.0
    } else if order <= 8.0 {
        0.5 + (order - 2.0) / 24.0
    } else {
        0.75 + (order - 8.0) / 32.0
    }
}

/// Prototype order for a shape and normalised slope (bells and notches: the half order).
pub fn prototype_order(shape: Shape, slope: f64) -> f64 {
    match shape {
        Shape::Bell | Shape::Notch => bell_order(slope),
        Shape::LowShelf | Shape::HighShelf => shelf_order(slope),
        Shape::BandPass => cut_order(slope) / 2.0,
        Shape::LowPass | Shape::HighPass => cut_order(slope),
    }
}

#[derive(Clone, Copy, Default)]
struct ProtoSection {
    n: [f64; 3],
    d: [f64; 3],
}

const UNIT_SECTION: ProtoSection = ProtoSection { n: [1.0, 0.0, 0.0], d: [1.0, 0.0, 0.0] };

#[derive(Clone, Copy, PartialEq)]
enum Variable {
    BandPass,
    LowPass,
    HighPass,
}

struct Proto {
    sections: [ProtoSection; 10],
    count: usize,
    gain: f64,
    variable: Variable,
}

fn butterworth_sections(n: usize, out: &mut [ProtoSection; 10], as_den: bool) -> usize {
    let mut count = 0;
    let mut c1s = [0.0f64; 8];
    let half = n / 2;
    for k in 1..=half {
        c1s[k - 1] = 2.0 * (PI * (2.0 * k as f64 - 1.0) / (2.0 * n as f64)).sin();
    }
    c1s[..half].sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    for &c1 in c1s.iter().take(half) {
        let mut s = UNIT_SECTION;
        let c = if as_den { &mut s.d } else { &mut s.n };
        *c = [1.0, c1, 1.0];
        out[count] = s;
        count += 1;
    }
    if n % 2 == 1 {
        let mut s = UNIT_SECTION;
        let c = if as_den { &mut s.d } else { &mut s.n };
        *c = [1.0, 1.0, 0.0];
        out[count] = s;
        count += 1;
    }
    count
}

fn shelf_like_sections(n: usize, g: f64, out: &mut [ProtoSection; 10]) -> usize {
    let mut base = [UNIT_SECTION; 10];
    let count = butterworth_sections(n, &mut base, true);
    let r = g.powf(1.0 / (2.0 * n as f64));
    for i in 0..count {
        let d = base[i].d;
        let mut s = UNIT_SECTION;
        for k in 0..3 {
            s.n[k] = d[k] * (1.0 / r).powf(k as f64);
            s.d[k] = d[k] * r.powf(k as f64);
        }
        out[i] = s;
    }
    count
}

fn cut_sections(n: usize, high_pass: bool, out: &mut [ProtoSection; 10]) -> usize {
    let mut base = [UNIT_SECTION; 10];
    let count = butterworth_sections(n, &mut base, true);
    for i in 0..count {
        let mut s = UNIT_SECTION;
        s.d = base[i].d;
        let quad = base[i].d[2] != 0.0;
        s.n = [if high_pass { 0.0 } else { 1.0 }, 0.0, 0.0];
        if high_pass {
            if quad {
                s.n[2] = 1.0;
            } else {
                s.n[1] = 1.0;
            }
        }
        out[i] = s;
    }
    count
}

fn convolve_first_order(c: &mut [f64; 3], a0: f64, a1: f64) {
    let (c0, c1) = (c[0], c[1]);
    c[0] = c0 * a0;
    c[1] = c0 * a1 + c1 * a0;
    c[2] = c1 * a1;
}

fn fractional_sections(order: f64, make: &dyn Fn(usize, &mut [ProtoSection; 10]) -> usize, out: &mut [ProtoSection; 10]) -> usize {
    let n = (order + 1.0e-9).floor() as usize;
    let x = order - n as f64;
    let mut lo = [UNIT_SECTION; 10];
    let mut hi = [UNIT_SECTION; 10];
    let mut lo_count = make(n, &mut lo);
    if x < 1.0e-9 {
        out[..lo_count].copy_from_slice(&lo[..lo_count]);
        return lo_count;
    }
    let hi_count = make(n + 1, &mut hi);
    if n % 2 == 0 {
        if lo_count < 10 {
            lo[lo_count] = ProtoSection { n: [1.0, 1.0, 0.0], d: [1.0, 1.0, 0.0] };
            lo_count += 1;
        }
    } else if lo_count > 0 {
        let last = &mut lo[lo_count - 1];
        convolve_first_order(&mut last.n, 1.0, 1.0);
        convolve_first_order(&mut last.d, 1.0, 1.0);
    }
    let count = lo_count.min(hi_count);
    let blend = |a: f64, b: f64| {
        if a > 0.0 && b > 0.0 {
            a.powf(1.0 - x) * b.powf(x)
        } else {
            a * (1.0 - x) + b * x
        }
    };
    for i in 0..count {
        for k in 0..3 {
            out[i].n[k] = blend(lo[i].n[k], hi[i].n[k]);
            out[i].d[k] = blend(lo[i].d[k], hi[i].d[k]);
        }
    }
    count
}

fn cut_one_to_two(order: f64, high_pass: bool) -> ProtoSection {
    const TABLE: [[f64; 4]; 9] = [
        [1.00, 0.618, 1.618, 0.618],
        [1.05, 0.583, 1.596, 0.637],
        [1.20, 0.487, 1.543, 0.695],
        [1.40, 0.378, 1.495, 0.775],
        [1.50, 0.327, 1.476, 0.814],
        [1.60, 0.278, 1.460, 0.852],
        [1.80, 0.177, 1.434, 0.927],
        [1.95, 0.082, 1.419, 0.982],
        [2.00, 0.000, 1.41421356, 1.000],
    ];
    let rows = TABLE.len();
    let mut i = 0;
    while i < rows - 2 && order > TABLE[i + 1][0] {
        i += 1;
    }
    let t = ((order - TABLE[i][0]) / (TABLE[i + 1][0] - TABLE[i][0])).clamp(0.0, 1.0);
    let z = TABLE[i][1] + (TABLE[i + 1][1] - TABLE[i][1]) * t;
    let a = TABLE[i][2] + (TABLE[i + 1][2] - TABLE[i][2]) * t;
    let b = TABLE[i][3] + (TABLE[i + 1][3] - TABLE[i][3]) * t;
    if high_pass {
        ProtoSection { n: [0.0, z, 1.0], d: [b, a, 1.0] }
    } else {
        ProtoSection { n: [1.0, z, 0.0], d: [1.0, a, b] }
    }
}

fn fractional_first_order(n: f64, high_pass: bool, out: &mut [ProtoSection; 10]) -> usize {
    const PAIRS: usize = 4;
    let (low, high) = (1.0e-3f64, 1.0f64);
    for k in 1..=PAIRS {
        let kf = k as f64;
        let zero = low * (high / low).powf((kf - 0.5 - 0.5 * n) / PAIRS as f64);
        let pole = low * (high / low).powf((kf - 0.5 + 0.5 * n) / PAIRS as f64);
        out[k - 1] = if high_pass {
            ProtoSection { n: [zero, 1.0, 0.0], d: [pole, 1.0, 0.0] }
        } else {
            ProtoSection { n: [1.0, zero, 0.0], d: [1.0, pole, 0.0] }
        };
    }
    PAIRS
}

fn build_proto(shape: Shape, gain_db: f64, q: f64, slope: f64) -> Option<Proto> {
    let g = 10f64.powf(gain_db / 20.0);
    let mut p = Proto { sections: [UNIT_SECTION; 10], count: 0, gain: 1.0, variable: Variable::LowPass };
    match shape {
        Shape::Bell => {
            p.variable = Variable::BandPass;
            if gain_db.abs() < 1.0e-9 {
                return None;
            }
            p.count = fractional_sections(bell_order(slope), &|n, o| shelf_like_sections(n, g, o), &mut p.sections);
            p.gain = g;
        }
        Shape::LowShelf | Shape::HighShelf => {
            p.variable = if shape == Shape::LowShelf { Variable::LowPass } else { Variable::HighPass };
            if gain_db.abs() < 1.0e-9 {
                return None;
            }
            p.count = fractional_sections(shelf_order(slope), &|n, o| shelf_like_sections(n, g, o), &mut p.sections);
            p.gain = g;
        }
        Shape::Notch => {
            p.variable = Variable::BandPass;
            p.count = fractional_sections(bell_order(slope), &|n, o| cut_sections(n, true, o), &mut p.sections);
        }
        Shape::BandPass => {
            p.variable = Variable::BandPass;
            let m = cut_order(slope) / 2.0;
            if m < 1.0e-6 {
                return None;
            }
            if m < 1.0 {
                p.count = fractional_first_order(m, false, &mut p.sections);
            } else {
                p.count = fractional_sections(m, &|n, o| cut_sections(n, false, o), &mut p.sections);
            }
        }
        Shape::LowPass | Shape::HighPass => {
            let high_pass = shape == Shape::HighPass;
            p.variable = Variable::LowPass;
            let n = cut_order(slope);
            if n < 1.0e-6 {
                return None;
            }
            if n < 1.0 {
                p.count = fractional_first_order(n, high_pass, &mut p.sections);
            } else if n < 2.0 - 1.0e-9 {
                p.sections[0] = cut_one_to_two(n, high_pass);
                p.count = 1;
            } else if (n - 2.0).abs() < 1.0e-9 {
                // Order 2: the section Q is the band Q / sqrt(2), so Q = 1 is Butterworth.
                let qs = (q * std::f64::consts::FRAC_1_SQRT_2).max(0.025);
                let mut s = UNIT_SECTION;
                s.d = [1.0, 1.0 / qs, 1.0];
                s.n = if high_pass { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
                p.sections[0] = s;
                p.count = 1;
            } else {
                if high_pass {
                    p.variable = Variable::HighPass;
                }
                p.count = fractional_sections(n, &|k, o| cut_sections(k, false, o), &mut p.sections);
            }
        }
    }
    Some(p)
}

#[inline]
fn eval_poly(c: &[f64; 3], w: Cx) -> Cx {
    c[0] + w * (c[1] + w * c[2])
}

fn proto_variable(v: Variable, w0: f64, q: f64, omega: f64) -> Cx {
    let s = Cx::new(0.0, omega);
    match v {
        Variable::BandPass => (s / w0 + w0 / s) * q,
        Variable::LowPass => s / w0,
        Variable::HighPass => w0 / s,
    }
}

fn proto_magnitude(p: &Proto, w0: f64, q: f64, omega: f64) -> f64 {
    let w = proto_variable(p.variable, w0, q, omega);
    let mut h = Cx::real(p.gain);
    for s in &p.sections[..p.count] {
        h *= eval_poly(&s.n, w) / eval_poly(&s.d, w);
    }
    h.abs()
}

fn roots_of(c: &[f64; 3], out: &mut [Cx; 2]) -> usize {
    if c[2].abs() > 1.0e-300 {
        let disc = Cx::real(c[1] * c[1] - 4.0 * c[2] * c[0]).sqrt();
        out[0] = (Cx::real(-c[1]) + disc) / (2.0 * c[2]);
        out[1] = (Cx::real(-c[1]) - disc) / (2.0 * c[2]);
        return 2;
    }
    if c[1].abs() > 1.0e-300 {
        out[0] = Cx::real(-c[0] / c[1]);
        return 1;
    }
    0
}

fn map_root(v: Variable, w0: f64, q: f64, w: Cx, out: &mut [Cx; 2]) -> usize {
    match v {
        Variable::BandPass => {
            let half = w / (2.0 * q);
            let rad = (half * half - 1.0).sqrt();
            out[0] = (half + rad) * w0;
            out[1] = (half - rad) * w0;
            2
        }
        Variable::LowPass => {
            out[0] = w * w0;
            1
        }
        Variable::HighPass => {
            if w.abs() < 1.0e-300 {
                return 0;
            }
            out[0] = w0 / w;
            1
        }
    }
}

/// Groups roots into conjugate pairs and lone reals, each as a monic polynomial (c0, c1, c2).
pub fn group_roots(roots: &[Cx], groups: &mut [[f64; 3]], freq: &mut [f64]) -> usize {
    let count = roots.len();
    let mut used = [false; 48];
    let mut g = 0;
    for i in 0..count {
        if used[i] || g >= groups.len() {
            continue;
        }
        used[i] = true;
        let r = roots[i];
        let scale = r.abs().max(1.0);
        if r.im.abs() > 1.0e-9 * scale {
            let mut best = usize::MAX;
            let mut best_dist = 1.0e300;
            for j in i + 1..count {
                if !used[j] {
                    let d = (roots[j] - r.conj()).abs();
                    if d < best_dist {
                        best_dist = d;
                        best = j;
                    }
                }
            }
            if best != usize::MAX {
                used[best] = true;
            }
            groups[g] = [r.norm(), -2.0 * r.re, 1.0];
            freq[g] = r.abs();
            g += 1;
        } else {
            let mut other = usize::MAX;
            for j in i + 1..count {
                if !used[j] && roots[j].im.abs() <= 1.0e-9 * roots[j].abs().max(1.0) {
                    other = j;
                    break;
                }
            }
            if other != usize::MAX {
                used[other] = true;
                let (a, b) = (r.re, roots[other].re);
                groups[g] = [a * b, -(a + b), 1.0];
                freq[g] = (a * b).abs().sqrt();
            } else {
                groups[g] = [-r.re, 1.0, 0.0];
                freq[g] = r.re.abs();
            }
            g += 1;
        }
    }
    g
}

/// Analogue design: sections of monic s-domain polynomials plus an overall gain matched to the
/// prototype. Returns false when the band does nothing.
pub fn design_analog(shape: Shape, f0: f64, gain_db: f64, q: f64, slope: f64, out: &mut AnalogCascade) -> bool {
    out.count = 0;
    out.gain = 1.0;
    let p = match build_proto(shape, gain_db, q, slope) {
        Some(p) if p.count > 0 => p,
        _ => return false,
    };
    let w0 = 2.0 * PI * f0;
    let q = q.max(0.025);
    let mut zeros = [Cx::ZERO; 48];
    let mut poles = [Cx::ZERO; 48];
    let (mut nz, mut np, mut proto_zeros, mut proto_poles) = (0usize, 0usize, 0usize, 0usize);
    for s in &p.sections[..p.count] {
        let mut r = [Cx::ZERO; 2];
        let zc = roots_of(&s.n, &mut r);
        for &rk in r.iter().take(zc) {
            proto_zeros += 1;
            let mut m = [Cx::ZERO; 2];
            let mc = map_root(p.variable, w0, q, rk, &mut m);
            for &mj in m.iter().take(mc) {
                if nz < 48 {
                    zeros[nz] = mj;
                    nz += 1;
                }
            }
        }
        let pc = roots_of(&s.d, &mut r);
        for &rk in r.iter().take(pc) {
            proto_poles += 1;
            let mut m = [Cx::ZERO; 2];
            let mc = map_root(p.variable, w0, q, rk, &mut m);
            for &mj in m.iter().take(mc) {
                if np < 48 {
                    poles[np] = mj;
                    np += 1;
                }
            }
        }
    }
    if p.variable != Variable::LowPass {
        for _ in proto_zeros..proto_poles {
            if nz < 48 {
                zeros[nz] = Cx::ZERO;
                nz += 1;
            }
        }
    }
    let mut origin_zeros = 0;
    {
        let mut kept = 0;
        for i in 0..nz {
            if zeros[i].abs() < 1.0e-9 * w0.max(1.0) {
                origin_zeros += 1;
            } else {
                zeros[kept] = zeros[i];
                kept += 1;
            }
        }
        nz = kept;
    }
    let mut zg = [[0.0f64; 3]; 48];
    let mut pg = [[0.0f64; 3]; 48];
    let mut zf = [0.0f64; 48];
    let mut pf = [0.0f64; 48];
    let zg_count = group_roots(&zeros[..nz], &mut zg, &mut zf);
    let pg_count = group_roots(&poles[..np], &mut pg, &mut pf);

    let mut z_used = [false; 48];
    let mut count = 0;
    for i in 0..pg_count {
        if count >= MAX_ANALOG_SECTIONS {
            break;
        }
        let mut s = AnalogSection { a0: pg[i][0], a1: pg[i][1], a2: pg[i][2], ..Default::default() };
        let mut best = usize::MAX;
        let mut best_score = 1.0e300;
        for j in 0..zg_count {
            if z_used[j] {
                continue;
            }
            let same = (zg[j][2] != 0.0) == (pg[i][2] != 0.0);
            let degree_ok = same || pg[i][2] != 0.0;
            if !degree_ok {
                continue;
            }
            let score = ((zf[j] + 1.0e-9) / (pf[i] + 1.0e-9)).ln().abs() + if same { 0.0 } else { 100.0 };
            if score < best_score {
                best_score = score;
                best = j;
            }
        }
        if best != usize::MAX {
            z_used[best] = true;
            s.b0 = zg[best][0];
            s.b1 = zg[best][1];
            s.b2 = zg[best][2];
        } else {
            s.b0 = 1.0;
            s.b1 = 0.0;
            s.b2 = 0.0;
        }
        out.sections[count] = s;
        count += 1;
    }
    out.count = count;

    let mut pass = 0;
    while pass < 2 && origin_zeros > 0 {
        for sec in out.sections[..count].iter_mut() {
            if origin_zeros == 0 {
                break;
            }
            let num_degree = if sec.b2 != 0.0 { 2 } else if sec.b1 != 0.0 { 1 } else { 0 };
            let den_degree = if sec.a2 != 0.0 { 2 } else { 1 };
            if num_degree >= den_degree {
                continue;
            }
            sec.b2 = sec.b1;
            sec.b1 = sec.b0;
            sec.b0 = 0.0;
            origin_zeros -= 1;
        }
        pass += 1;
    }

    let (mut best_ref, mut best_mag) = (0.0, -1.0);
    for factor in [1.0, 0.5, 2.0, 0.25, 4.0, 0.1, 10.0] {
        let omega = w0 * factor;
        let mag = proto_magnitude(&p, w0, q, omega);
        if mag.is_finite() && mag > best_mag {
            best_mag = mag;
            best_ref = omega;
        }
    }
    let mut h = Cx::ONE;
    let s = Cx::new(0.0, best_ref);
    for sec in &out.sections[..count] {
        h *= (sec.b0 + s * (sec.b1 + s * sec.b2)) / (sec.a0 + s * (sec.a1 + s * sec.a2));
    }
    out.gain = if h.abs() > 0.0 { best_mag / h.abs() } else { 1.0 };
    true
}

/// Roots of c[0] + c[1] x + ... + c[degree] x^degree (degree up to 8), Durand-Kerner with Newton
/// polishing. Returns the root count.
pub fn polynomial_roots(c: &[f64], degree: usize, roots: &mut [Cx]) -> usize {
    let mut degree = degree.min(c.len().saturating_sub(1));
    let scale_max = c[..=degree].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    while degree > 0 && c[degree].abs() <= 1.0e-14 * scale_max {
        degree -= 1;
    }
    if degree == 0 || degree > 8 || roots.len() < degree {
        return 0;
    }
    let mut monic = [0.0f64; 9];
    for i in 0..=degree {
        monic[i] = c[i] / c[degree];
    }
    let eval = |x: Cx| {
        let mut v = Cx::ONE;
        for i in (0..degree).rev() {
            v = v * x + monic[i];
        }
        v
    };
    let eval_d = |x: Cx| {
        let mut v = Cx::real(degree as f64);
        for i in (1..degree).rev() {
            v = v * x + i as f64 * monic[i];
        }
        v
    };
    let mut radius = 0.0f64;
    for &m in monic.iter().take(degree) {
        radius = radius.max(m.abs());
    }
    radius = (1.0 + radius).min(1.0e12);
    let start = monic[0].abs().powf(1.0 / degree as f64).max(1.0e-12);
    for (i, r) in roots.iter_mut().enumerate().take(degree) {
        *r = Cx::polar(start.min(radius), 0.4 + 2.0 * PI * i as f64 / degree as f64);
    }
    for _ in 0..500 {
        let mut change = 0.0f64;
        for i in 0..degree {
            let mut den = Cx::ONE;
            for j in 0..degree {
                if j != i {
                    den *= roots[i] - roots[j];
                }
            }
            if den.abs() < 1.0e-300 {
                den = Cx::real(1.0e-300);
            }
            let step = eval(roots[i]) / den;
            roots[i] -= step;
            change = change.max(step.abs() / roots[i].abs().max(1.0e-300));
        }
        if change < 1.0e-15 {
            break;
        }
    }
    for r in roots.iter_mut().take(degree) {
        for _ in 0..4 {
            let d = eval_d(*r);
            if d.abs() < 1.0e-300 {
                break;
            }
            *r -= eval(*r) / d;
        }
    }
    for r in roots.iter_mut().take(degree) {
        if r.im.abs() < 1.0e-10 * r.abs().max(1.0) {
            *r = Cx::real(r.re);
        }
    }
    degree
}

/// Turns H(x) = num(x) / den(x), x = s / omega_scale, into an analogue cascade in s. Used by the
/// component models.
pub fn cascade_from_rational(num: &[f64], num_degree: usize, den: &[f64], den_degree: usize, omega_scale: f64, out: &mut AnalogCascade) -> bool {
    out.count = 0;
    out.gain = 1.0;
    let mut zeros = [Cx::ZERO; 8];
    let mut poles = [Cx::ZERO; 8];
    let nz = polynomial_roots(num, num_degree, &mut zeros);
    let np = polynomial_roots(den, den_degree, &mut poles);
    if np == 0 {
        return false;
    }
    for z in zeros.iter_mut().take(nz) {
        *z = *z * omega_scale;
    }
    for p in poles.iter_mut().take(np) {
        *p = *p * omega_scale;
    }
    let mut zg = [[0.0f64; 3]; 8];
    let mut pg = [[0.0f64; 3]; 8];
    let mut zf = [0.0f64; 8];
    let mut pf = [0.0f64; 8];
    let zg_count = group_roots(&zeros[..nz], &mut zg, &mut zf);
    let pg_count = group_roots(&poles[..np], &mut pg, &mut pf);
    let mut z_used = [false; 8];
    let mut count = 0;
    for i in 0..pg_count {
        if count >= MAX_ANALOG_SECTIONS {
            break;
        }
        let mut s = AnalogSection { a0: pg[i][0], a1: pg[i][1], a2: pg[i][2], ..Default::default() };
        let mut best = usize::MAX;
        let mut best_score = 1.0e300;
        for j in 0..zg_count {
            if z_used[j] || (zg[j][2] != 0.0 && pg[i][2] == 0.0) {
                continue;
            }
            let same = (zg[j][2] != 0.0) == (pg[i][2] != 0.0);
            let score = ((zf[j] + 1.0e-9) / (pf[i] + 1.0e-9)).ln().abs() + if same { 0.0 } else { 100.0 };
            if score < best_score {
                best_score = score;
                best = j;
            }
        }
        if best != usize::MAX {
            z_used[best] = true;
            s.b0 = zg[best][0];
            s.b1 = zg[best][1];
            s.b2 = zg[best][2];
        }
        out.sections[count] = s;
        count += 1;
    }
    for j in 0..zg_count {
        if z_used[j] || zg[j][2] != 0.0 {
            continue;
        }
        for sec in out.sections[..count].iter_mut() {
            let num_degree = if sec.b2 != 0.0 { 2 } else if sec.b1 != 0.0 { 1 } else { 0 };
            let den_degree = if sec.a2 != 0.0 { 2 } else { 1 };
            if num_degree < den_degree {
                let (b0, b1) = (sec.b0, sec.b1);
                sec.b2 = b1;
                sec.b1 = b0 + b1 * zg[j][0];
                sec.b0 = b0 * zg[j][0];
                z_used[j] = true;
                break;
            }
        }
    }
    out.count = count;
    let eval_asc = |c: &[f64], degree: usize, x: Cx| {
        let mut v = Cx::ZERO;
        for i in (0..=degree).rev() {
            v = v * x + c[i];
        }
        v
    };
    let target = eval_asc(num, num_degree, Cx::new(0.0, 1.0)) / eval_asc(den, den_degree, Cx::new(0.0, 1.0));
    let mut h = Cx::ONE;
    let s = Cx::new(0.0, omega_scale);
    for sec in &out.sections[..count] {
        h *= (sec.b0 + s * (sec.b1 + s * sec.b2)) / (sec.a0 + s * (sec.a1 + s * sec.a2));
    }
    out.gain = if h.abs() > 0.0 { target.abs() / h.abs() } else { 1.0 };
    true
}

/// Nyquist-matched digitisation of one analogue section.
pub fn match_section(a: &AnalogSection, sample_rate: f64) -> Biquad {
    let t = 1.0 / sample_rate;
    let analog_mag2 = |omega: f64| {
        let s = Cx::new(0.0, omega);
        ((a.b0 + s * (a.b1 + s * a.b2)) / (a.a0 + s * (a.a1 + s * a.a2))).norm()
    };
    let nyquist = PI * sample_rate;
    let mut d = Biquad::default();
    if a.a2.abs() < 1.0e-300 {
        let pole = -a.a0 / a.a1;
        let zp = (pole * t).exp();
        d.a1 = -zp;
        let h0 = analog_mag2(0.0).sqrt();
        let hn = analog_mag2(nyquist).sqrt();
        d.b0 = 0.5 * ((1.0 + d.a1) * h0 + (1.0 - d.a1) * hn);
        d.b1 = 0.5 * ((1.0 + d.a1) * h0 - (1.0 - d.a1) * hn);
        d.b2 = 0.0;
        d.a2 = 0.0;
        return d;
    }
    let disc = Cx::real(a.a1 * a.a1 - 4.0 * a.a2 * a.a0).sqrt();
    let p1 = (Cx::real(-a.a1) + disc) / (2.0 * a.a2);
    let p2 = (Cx::real(-a.a1) - disc) / (2.0 * a.a2);
    let z1 = (p1 * t).exp();
    let z2 = (p2 * t).exp();
    d.a1 = -(z1 + z2).re;
    d.a2 = (z1 * z2).re;

    if a.b0 == 0.0 && a.b1 == 0.0 && a.b2 != 0.0 {
        let pole_freq = (a.a0 / a.a2).abs().sqrt();
        let wp = (pole_freq * t).clamp(1.0e-4, 0.9 * PI);
        let zi = Cx::polar(1.0, -wp);
        let one_minus = Cx::ONE - zi;
        let digital_shape = ((one_minus * one_minus) / (1.0 + d.a1 * zi + d.a2 * zi * zi)).abs();
        let k = if digital_shape > 0.0 { analog_mag2(wp * sample_rate).sqrt() / digital_shape } else { 0.0 };
        d.b0 = k;
        d.b1 = -2.0 * k;
        d.b2 = k;
        return d;
    }
    let a0 = (1.0 + d.a1 + d.a2) * (1.0 + d.a1 + d.a2);
    let a1 = (1.0 - d.a1 + d.a2) * (1.0 - d.a1 + d.a2);
    let a2 = -4.0 * d.a2;
    let mut match_freq = (a.a0 / a.a2).abs().sqrt();
    let axis_zeros = a.b2 != 0.0 && a.b0 > 0.0 && a.b1.abs() < 1.0e-6 * (a.b0 * a.b2).sqrt();
    if axis_zeros {
        match_freq = (a.b0 / a.b2).sqrt();
    }
    let wm = (match_freq * t).clamp(1.0e-4, 0.9 * PI);
    let phi1 = (wm / 2.0).sin() * (wm / 2.0).sin();
    let phi0 = 1.0 - phi1;
    let phi2 = 4.0 * phi0 * phi1;
    let bb0 = a0 * analog_mag2(0.0);
    let bb1 = a1 * analog_mag2(nyquist);
    let bb2 = ((a0 * phi0 + a1 * phi1 + a2 * phi2) * analog_mag2(wm * sample_rate) - bb0 * phi0 - bb1 * phi1) / phi2;
    let r0 = bb0.max(0.0).sqrt();
    let r1 = bb1.max(0.0).sqrt();
    let w = 0.5 * (r0 + r1);
    let root = (w * w + bb2).max(0.0).sqrt();
    d.b0 = 0.5 * (w + root);
    d.b1 = 0.5 * (r0 - r1);
    d.b2 = if d.b0 != 0.0 { -bb2 / (4.0 * d.b0) } else { 0.0 };
    d
}

/// Digitises a cascade; the overall gain goes into the first section.
pub fn digitise(cascade: &AnalogCascade, sample_rate: f64, out: &mut [Biquad]) -> usize {
    let count = cascade.count.min(out.len());
    for i in 0..count {
        out[i] = match_section(&cascade.sections[i], sample_rate);
    }
    if count > 0 {
        out[0].b0 *= cascade.gain;
        out[0].b1 *= cascade.gain;
        out[0].b2 *= cascade.gain;
    }
    count
}

/// Full design: digital sections. Returns 0 when the band does nothing.
pub fn design(shape: Shape, sample_rate: f64, f0: f64, gain_db: f64, q: f64, slope: f64, out: &mut [Biquad]) -> usize {
    let mut cascade = AnalogCascade::default();
    if !design_analog(shape, f0, gain_db, q, slope, &mut cascade) {
        return 0;
    }
    digitise(&cascade, sample_rate, out)
}
