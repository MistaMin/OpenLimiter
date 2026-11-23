//! 2x polyphase IIR half-band up/down sampler (two chains of first-order all-passes at the low
//! rate; coefficients from the elliptic half-band design). Stands in for the polyphase IIR
//! oversampler the original used; the latency (group delay at low frequencies) is measured in
//! `new` and is fractional, like the original's.

use std::f64::consts::PI;

pub const MAX_COEFS: usize = 16;

fn transition_param(transition: f64) -> (f64, f64) {
    let mut k = ((1.0 - transition * 2.0) * PI / 4.0).tan();
    k *= k;
    let kksqrt = (1.0 - k * k).powf(0.25);
    let e = 0.5 * (1.0 - kksqrt) / (1.0 + kksqrt);
    let e2 = e * e;
    let e4 = e2 * e2;
    let q = e * (1.0 + e4 * (2.0 + e4 * (15.0 + 150.0 * e4)));
    (k, q)
}

fn order_for(attenuation: f64, q: f64) -> usize {
    let attn_p2 = 10f64.powf(-attenuation / 10.0);
    let a = attn_p2 / (1.0 - attn_p2);
    let mut order = ((a * a / 16.0).ln() / q.ln()).ceil() as usize;
    if order % 2 == 0 {
        order += 1;
    }
    if order == 1 {
        order = 3;
    }
    order
}

fn acc_num(q: f64, order: usize, c: usize) -> f64 {
    let (mut i, mut j, mut acc) = (0i32, 1.0f64, 0.0f64);
    loop {
        let mut q_ii1 = q.powi(i * (i + 1));
        q_ii1 *= (((i * 2 + 1) as usize * c) as f64 * PI / order as f64).sin() * j;
        acc += q_ii1;
        j = -j;
        i += 1;
        if q_ii1.abs() <= 1.0e-100 || i > 100 {
            break;
        }
    }
    acc
}

fn acc_den(q: f64, order: usize, c: usize) -> f64 {
    let (mut i, mut j, mut acc) = (1i32, -1.0f64, 0.0f64);
    loop {
        let mut q_i2 = q.powi(i * i);
        q_i2 *= ((i as usize * 2 * c) as f64 * PI / order as f64).cos() * j;
        acc += q_i2;
        j = -j;
        i += 1;
        if q_i2.abs() <= 1.0e-100 || i > 100 {
            break;
        }
    }
    acc
}

/// Half-band all-pass coefficients for a transition width (relative to the high rate) and a
/// stop-band attenuation in dB.
pub fn design_coefs(transition: f64, attenuation: f64, out: &mut [f64; MAX_COEFS]) -> usize {
    let (k, q) = transition_param(transition);
    let order = order_for(attenuation, q);
    let count = ((order - 1) / 2).min(MAX_COEFS);
    for (idx, o) in out.iter_mut().enumerate().take(count) {
        let c = idx + 1;
        let num = acc_num(q, order, c) * q.powf(0.25);
        let den = acc_den(q, order, c) + 0.5;
        let ww = num / den;
        let wwsq = ww * ww;
        let x = ((1.0 - wwsq * k) * (1.0 - wwsq / k)).sqrt() / (1.0 + wwsq);
        *o = (1.0 - x) / (1.0 + x);
    }
    count
}

#[derive(Clone, Copy)]
struct Chains {
    x: [f64; MAX_COEFS],
    y: [f64; MAX_COEFS],
}

impl Default for Chains {
    fn default() -> Self {
        Chains { x: [0.0; MAX_COEFS], y: [0.0; MAX_COEFS] }
    }
}

/// Polyphase half-band, one channel.
#[derive(Clone, Copy)]
pub struct HalfBand {
    coefs: [f64; MAX_COEFS],
    count: usize,
    up: Chains,
    down: Chains,
}

impl HalfBand {
    pub fn new() -> Self {
        let mut coefs = [0.0; MAX_COEFS];
        let count = design_coefs(0.04, 90.0, &mut coefs);
        HalfBand { coefs, count, up: Chains::default(), down: Chains::default() }
    }

    pub fn reset(&mut self) {
        self.up = Chains::default();
        self.down = Chains::default();
    }

    #[inline]
    fn chain(coefs: &[f64; MAX_COEFS], count: usize, st: &mut Chains, first: usize, input: f64) -> f64 {
        let mut v = input;
        let mut i = first;
        while i < count {
            // y[n] = a (x[n] - y[n-1]) + x[n-1]
            let y = coefs[i] * (v - st.y[i]) + st.x[i];
            st.x[i] = v;
            st.y[i] = y;
            v = y;
            i += 2;
        }
        v
    }

    /// One low-rate sample in, two high-rate samples out.
    #[inline]
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        let a = Self::chain(&self.coefs, self.count, &mut self.up, 0, x as f64);
        let b = Self::chain(&self.coefs, self.count, &mut self.up, 1, x as f64);
        [a as f32, b as f32]
    }

    /// Two high-rate samples in, one low-rate sample out.
    #[inline]
    pub fn down(&mut self, x: [f32; 2]) -> f32 {
        let a = Self::chain(&self.coefs, self.count, &mut self.down, 0, x[1] as f64);
        let b = Self::chain(&self.coefs, self.count, &mut self.down, 1, x[0] as f64);
        (0.5 * (a + b)) as f32
    }
}

impl Default for HalfBand {
    fn default() -> Self {
        Self::new()
    }
}

/// Group delay of an up + down pass at low frequency, in low-rate samples (measured once).
pub fn round_trip_latency() -> f64 {
    let mut h = HalfBand::new();
    let n = 512;
    let (mut sum, mut moment) = (0.0f64, 0.0f64);
    // Group delay at a low frequency from the phase slope of the response.
    let f = 0.005; // cycles per low-rate sample
    let (mut re0, mut im0, mut re1, mut im1) = (0.0, 0.0, 0.0, 0.0);
    let df = 0.001;
    for i in 0..n {
        let u = h.up(if i == 0 { 1.0 } else { 0.0 });
        let y = h.down(u) as f64;
        sum += y;
        moment += y * i as f64;
        let w0 = 2.0 * PI * f * i as f64;
        let w1 = 2.0 * PI * (f + df) * i as f64;
        re0 += y * w0.cos();
        im0 -= y * w0.sin();
        re1 += y * w1.cos();
        im1 -= y * w1.sin();
    }
    let p0 = im0.atan2(re0);
    let mut p1 = im1.atan2(re1);
    while p1 - p0 > PI {
        p1 -= 2.0 * PI;
    }
    while p1 - p0 < -PI {
        p1 += 2.0 * PI;
    }
    let gd = -(p1 - p0) / (2.0 * PI * df);
    if gd.is_finite() && gd > 0.0 {
        gd
    } else if sum.abs() > 1e-12 {
        moment / sum
    } else {
        0.0
    }
}

/// Group delay of the up-sampler alone, in high-rate samples, and of the down-sampler alone.
/// Used to align a 2x processing stage that sits between them.
pub fn up_down_latency_high_rate() -> f64 {
    2.0 * round_trip_latency()
}
