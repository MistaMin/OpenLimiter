//! Character types with closed-form analogue models (OpenEQ `VintageFilters`). Each design is a
//! few s-domain sections, digitised by `design::digitise` like the generic types.

use crate::design::{cascade_from_rational, AnalogCascade, AnalogSection};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VType {
    BlueLow,
    BlueHigh,
    GHighPass,
    ELowPass,
    GLowPass,
    VtLowPass,
    BritNHighPass,
    VtLowShelf,
    ToneLow,
    ToneHigh,
}

fn interpolate(table: &[[f64; 2]], x: f64) -> f64 {
    if x <= table[0][0] {
        return table[0][1];
    }
    for i in 1..table.len() {
        if x <= table[i][0] {
            let t = (x - table[i - 1][0]) / (table[i][0] - table[i - 1][0]);
            return table[i - 1][1] + (table[i][1] - table[i - 1][1]) * t;
        }
    }
    table[table.len() - 1][1]
}

fn second_order_high_pass(omega: f64, q: f64) -> AnalogSection {
    AnalogSection { b2: 1.0, b1: 0.0, b0: 0.0, a2: 1.0, a1: omega / q, a0: omega * omega }
}

fn second_order_low_pass(omega: f64, q: f64) -> AnalogSection {
    AnalogSection { b2: 0.0, b1: 0.0, b0: omega * omega, a2: 1.0, a1: omega / q, a0: omega * omega }
}

/// Ascending polynomial up to degree 4.
#[derive(Clone, Copy)]
struct Poly {
    c: [f64; 5],
    degree: usize,
}

fn poly(c0: f64, c1: f64) -> Poly {
    Poly { c: [c0, c1, 0.0, 0.0, 0.0], degree: 1 }
}

fn add(a: &Poly, b: &Poly, kb: f64) -> Poly {
    let mut r = Poly { c: [0.0; 5], degree: a.degree.max(b.degree) };
    for i in 0..=r.degree {
        r.c[i] = a.c[i] + kb * b.c[i];
    }
    r
}

fn scale(a: &Poly, k: f64) -> Poly {
    let mut r = *a;
    for i in 0..=r.degree {
        r.c[i] *= k;
    }
    r
}

fn mul(a: &Poly, b: &Poly) -> Poly {
    let mut r = Poly { c: [0.0; 5], degree: (a.degree + b.degree).min(4) };
    for i in 0..=a.degree {
        for j in 0..=b.degree {
            if i + j <= 4 {
                r.c[i + j] += a.c[i] * b.c[j];
            }
        }
    }
    r
}

/// P-type passive low section, fitted, capacitors normalised to x = s / (2 pi f).
fn passive_tube_low_shelf(fc: f64, gain_db: f64, slope: f64, out: &mut AnalogCascade) -> bool {
    const K: f64 = 2.868180903e-05;
    const RBH: f64 = 89759.11354;
    const E: f64 = 10809.97829;
    const R2: f64 = 20311.64032;
    const RATIO: f64 = 37.13187915;
    const R3: f64 = 1.0e4;
    let c2 = 2.0 * PI * K;
    let c1 = c2 / RATIO;
    let nb0 = R2 + R3;
    let np0 = E * nb0;
    let dp0 = E + nb0;
    let reference = (np0 / dp0) / (RBH + np0 / dp0) * R3 / nb0;
    let target = 10f64.powf(gain_db.clamp(0.0, 24.0) / 20.0) * reference;
    let rb = ((target * (RBH * dp0 + np0) - E * R3) / (dp0 * (1.0 - target))).max(0.0);

    const SLOPES: [f64; 22] = [0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 0.98, 1.0];
    const RCS: [f64; 22] = [
        0.0, 0.002754199767, 199.6430356, 2240.68939, 4434.362754, 6862.870713, 9625.484724, 12781.52008, 16438.37839,
        20729.14377, 25848.70503, 32020.00134, 39660.59441, 49343.22214, 62013.69234, 79361.85404, 104323.0239, 143714.9279,
        214878.0619, 382224.0393, 665214.3958, 1249680.321,
    ];
    let sn = slope.clamp(0.0, 1.0);
    let mut rc = 0.0;
    for i in 1..SLOPES.len() {
        if sn <= SLOPES[i] {
            let t = (sn - SLOPES[i - 1]) / (SLOPES[i] - SLOPES[i - 1]);
            rc = if SLOPES[i - 1] >= 0.1 {
                RCS[i - 1] * (RCS[i] / RCS[i - 1]).powf(t)
            } else {
                RCS[i - 1] + (RCS[i] - RCS[i - 1]) * t
            };
            break;
        }
    }
    let nh = poly(rc, 0.0);
    let dh = poly(1.0, rc * c1);
    let nj = poly(rb, 0.0);
    let dj = poly(1.0, rb * c2);
    let nb = add(&nh, &dh, R2 + R3);
    let np = scale(&nb, E);
    let dp = add(&scale(&dh, E), &nb, 1.0);
    let num = scale(&add(&mul(&nj, &dp), &mul(&dj, &dh), E * R3), 1.0 / reference);
    let den = add(&add(&scale(&mul(&dp, &dj), RBH), &mul(&np, &dj), 1.0), &mul(&nj, &dp), 1.0);
    cascade_from_rational(&num.c, num.degree, &den.c, den.degree, 2.0 * PI * fc, out)
}

struct Network {
    r2: f64,
    r3: f64,
    pb: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    c4: f64,
    pt: f64,
    r4: f64,
    fixed: f64,
    offset_db: f64,
    taper: [f64; 6],
}

const BAX_LOW: Network = Network {
    r2: 0.06363503222,
    r3: 3.049295449e-06,
    pb: 5.851861887,
    c1: 0.0001278414474,
    c2: 0.001289134074,
    c3: 3.010115087e-05,
    c4: 0.0003084422416,
    pt: 1.841473552,
    r4: 6.298187018,
    fixed: 0.077251663,
    offset_db: 21.56891902,
    taper: [6.08447711e-07, 5.35626624e-06, 4.30954335e-06, 2.36631296e-04, 1.57704898e-01, -2.23716026e+00],
};

const BAX_HIGH: Network = Network {
    r2: 0.8004385911,
    r3: 214.7829779,
    pb: 0.02520901593,
    c1: 0.0028198178,
    c2: 0.002304485375,
    c3: 1.123506018e-05,
    c4: 0.0001100221695,
    pt: 117.3882375,
    r4: 46.45535317,
    fixed: 0.4304696906,
    offset_db: 22.05814639,
    taper: [-1.16778970e-07, -1.33443465e-06, 1.52537899e-04, 2.82818553e-03, 1.52027060e-01, -2.45461350e+00],
};

/// Passive bass/treble tone network, fitted, capacitors normalised to x = s / (2 pi f).
fn tone_network(bass: bool, fc: f64, gain_db: f64, out: &mut AnalogCascade) -> bool {
    let n = if bass { &BAX_LOW } else { &BAX_HIGH };
    let g = gain_db.clamp(-18.0, 18.0);
    let mut logit = 0.0;
    for t in n.taper {
        logit = logit * g + t;
    }
    let pot = 1.0 / (1.0 + (-logit).exp());
    let (x, y) = if bass { (pot, n.fixed) } else { (n.fixed, pot) };
    let w = 2.0 * PI * 100.0;
    let (c1, c2, c3, c4, r1) = (n.c1 * w, n.c2 * w, n.c3 * w, n.c4 * w, 1.0);
    let ra = (n.pb * (1.0 - x)).max(1.0e-9);
    let rb = (n.pb * x).max(1.0e-9);
    let rc = (n.pt * (1.0 - y)).max(1.0e-9);
    let rd = (n.pt * y).max(1.0e-9);
    let n1 = poly(1.0, c1 * ra);
    let d1 = poly(r1 + ra, r1 * c1 * ra);
    let n2 = poly(1.0, c2 * rb);
    let d2 = poly(n.r2 + rb, n.r2 * c2 * rb);
    let n3 = poly(0.0, c3);
    let d3 = poly(1.0, c3 * rc);
    let n4 = poly(0.0, c4);
    let d4 = poly(1.0, c4 * rd);
    let g3 = 1.0 / n.r3;
    let g4 = 1.0 / n.r4;
    let p = add(&add(&mul(&n1, &d2), &mul(&n2, &d1), 1.0), &mul(&d1, &d2), g3);
    let q = add(&add(&scale(&mul(&d3, &d4), g3 + g4), &mul(&n3, &d4), 1.0), &mul(&n4, &d3), 1.0);
    let d1234 = mul(&mul(&d1, &d2), &mul(&d3, &d4));
    let num = scale(&add(&mul(&mul(&p, &n3), &d4), &mul(&mul(&n1, &d2), &mul(&d3, &d4)), g3), 10f64.powf(n.offset_db / 20.0));
    let den = add(&mul(&p, &q), &d1234, -g3 * g3);
    cascade_from_rational(&num.c, num.degree, &den.c, den.degree, 2.0 * PI * fc, out)
}

/// Builds the analogue cascade for a character type. Returns false when the band does nothing.
pub fn design_analog(ty: VType, fc: f64, gain_db: f64, q: f64, out: &mut AnalogCascade, slope: f64) -> bool {
    out.count = 0;
    out.gain = 1.0;
    let wc = 2.0 * PI * fc;
    match ty {
        VType::BlueLow => {
            let q_table = [
                [0.3f64.ln(), 0.411],
                [0.5f64.ln(), 0.437],
                [0.7f64.ln(), 0.456],
                [1.0f64.ln(), 0.478],
                [1.4f64.ln(), 0.512],
                [2.0f64.ln(), 0.555],
                [3.0f64.ln(), 0.614],
                [5.0f64.ln(), 0.708],
                [10.0f64.ln(), 0.894],
                [20.0f64.ln(), 1.215],
            ];
            let qe = interpolate(&q_table, q.max(0.025).ln());
            let gain = gain_db.clamp(0.43, 11.16);
            let k = 10f64.powf(gain / 20.0) - 1.0;
            let w0 = 2.0 * PI * fc.min(1978.0);
            out.sections[0] = AnalogSection { b2: 1.0, b1: (1.0 + k) * w0 / qe, b0: w0 * w0, a2: 1.0, a1: w0 / qe, a0: w0 * w0 };
            out.count = 1;
            true
        }
        VType::BlueHigh => {
            const K_TABLE: [[f64; 2]; 12] = [
                [0.0, 0.0756],
                [1.0, 0.1562],
                [2.0, 0.328],
                [3.0, 0.5087],
                [4.0, 0.6972],
                [5.0, 0.8982],
                [6.0, 1.1103],
                [8.0, 1.577],
                [10.0, 2.083],
                [12.0, 2.643],
                [14.0, 3.241],
                [16.0, 3.86],
            ];
            let k = interpolate(&K_TABLE, gain_db.clamp(0.0, 16.0));
            let w0 = 2.0 * PI * 0.798 * fc.max(1000.0);
            out.sections[0] = AnalogSection { b2: 0.0, b1: 1.0 + k, b0: w0, a2: 0.0, a1: 1.0, a0: w0 };
            out.count = 1;
            true
        }
        VType::GHighPass => {
            out.sections[0] = second_order_high_pass(1.16371 * wc, 0.77346);
            out.count = 1;
            true
        }
        VType::ELowPass | VType::GLowPass | VType::VtLowPass => {
            let (w, qq) = match ty {
                VType::ELowPass => (1.06966, 0.71775),
                VType::GLowPass => (1.04863, 0.71774),
                _ => (1.40237, 0.53776),
            };
            out.sections[0] = second_order_low_pass(w * wc, qq);
            out.count = 1;
            true
        }
        VType::BritNHighPass => {
            out.sections[0] = second_order_high_pass(1.01993 * wc, 0.93633);
            out.sections[1] = AnalogSection { b2: 0.0, b1: 1.0, b0: 0.39788 * wc, a2: 0.0, a1: 1.0, a0: 0.96022 * wc };
            out.count = 2;
            true
        }
        VType::VtLowShelf => passive_tube_low_shelf(fc, gain_db, slope, out),
        VType::ToneLow => tone_network(true, fc, gain_db, out),
        VType::ToneHigh => tone_network(false, fc, gain_db, out),
    }
}
