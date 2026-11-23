//! The composite test signal of the H job script in `measurements/scripts/` and its
//! `analyse` function, rebuilt.
#![allow(dead_code)]

use super::*;
use std::collections::HashMap;

pub const T_BL: [usize; 5] = [1, 5, 20, 100, 500];
const BL: [usize; 5] = T_BL;

pub struct Composite {
    pub l: Vec<f64>,
    pub r: Vec<f64>,
    pub sec: HashMap<String, (usize, usize)>,
}

/// The composite signal of the H job script (silence, sine steps, five bursts, a left-only
/// burst, an impulse, gated noise). The noise uses this crate's own random numbers.
pub fn composite() -> Composite {
    let mut c = Composite { l: Vec::new(), r: Vec::new(), sec: HashMap::new() };
    let tone = |n: usize, a: &dyn Fn(usize) -> f64| -> Vec<f64> { (0..n).map(|i| a(i) * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin()).collect() };
    let add = |c: &mut Composite, name: &str, l: Vec<f64>, r: Option<Vec<f64>>| {
        let s = c.l.len();
        c.sec.insert(name.to_string(), (s, s + l.len()));
        c.r.extend(r.unwrap_or_else(|| l.clone()));
        c.l.extend(l);
    };
    add(&mut c, "silence", vec![0.0; SR], None);
    let mut d = -36;
    while d <= 0 {
        add(&mut c, &format!("step{d}"), tone(SR / 2, &|_| lin(d as f64)), None);
        d += 2;
    }
    let burst = |ms: usize| tone(3 * SR, &|i| if i >= SR / 2 && i < SR / 2 + ms * SR / 1000 { lin(-2.0) } else { lin(-18.0) });
    for ms in BL {
        add(&mut c, &format!("burst{ms}"), burst(ms), None);
    }
    add(&mut c, "link", burst(100), Some(tone(3 * SR, &|_| lin(-18.0))));
    let mut imp = vec![0.0; SR];
    imp[SR / 2] = 0.25;
    add(&mut c, "impulse", imp, None);
    add(&mut c, "noise", gated_noise(1, 4), None);
    c
}

pub struct BurstRes {
    pub before: f64,
    pub in_burst: f64,
    pub end: f64,
    pub peak: f64,
    pub minus_1ms: f64,
    pub minus_5ms: f64,
    pub rec50: f64,
    pub rec90: f64,
    pub rec99: f64,
}

pub struct Res {
    pub statics: HashMap<i32, (f64, f64)>,
    pub burst: HashMap<usize, BurstRes>,
    pub link: (f64, f64, f64),
    pub impulse_offset: i64,
    pub impulse_peak: f64,
    pub noise_rms: f64,
    pub noise_in_rms: f64,
    pub noise_sample_peak: f64,
    pub noise_true_peak: f64,
    pub out: (Vec<f64>, Vec<f64>),
}

/// The H job script's `analyse`.
pub fn analyse(sig: &Composite, sets: &[(usize, f32)]) -> Res {
    let mut all = vec![(limiter_wall::P_STYLE, limiter_wall::STYLE_LEVELLER)];
    all.extend_from_slice(sets);
    analyse_with(sig, fresh(48000.0, &all))
}

/// Same, for a module that is already set up.
pub fn analyse_with(sig: &Composite, mut m: WallLimiter) -> Res {
    let (yl, yr) = run(&mut m, &sig.l, &sig.r);
    let mut statics = HashMap::new();
    let mut d = -36;
    while d <= 0 {
        let (s, e) = sig.sec[&format!("step{d}")];
        let seg = &yl[s + SR / 8..e - SR / 8];
        statics.insert(d, (db(rms(seg) * 2f64.sqrt()), db(sample_peak(seg))));
        d += 2;
    }
    let mut burst = HashMap::new();
    for ms in BL {
        let (s, e) = sig.sec[&format!("burst{ms}")];
        let g = gain_trace(&yl[s..e], &sig.l[s..e]);
        let on = SR / 2;
        let off = on + ms * SR / 1000;
        let n = e - s;
        let steady = median(&g[n - SR / 2..n - SR / 10]);
        let pre = median(&g[on - SR / 4..on - SR / 20]);
        let gin = if ms < 3 {
            g[on..off].iter().cloned().fold(f64::INFINITY, f64::min)
        } else {
            median(&g[on + (off - on) / 2..off - ((off - on) / 10).max(1)])
        };
        let rec = |f: f64| {
            let tgt = gin + f * (steady - gin);
            g[off + 24..].iter().position(|v| *v >= tgt).map(|h| h as f64 / SR as f64 * 1000.0).unwrap_or(f64::NAN)
        };
        burst.insert(
            ms,
            BurstRes {
                before: pre,
                in_burst: gin,
                end: steady,
                peak: db(sample_peak(&yl[s + on - SR / 100..s + off + SR / 100])),
                minus_1ms: g[on - 48],
                minus_5ms: g[on - 240],
                rec50: rec(0.5),
                rec90: rec(0.9),
                rec99: rec(0.99),
            },
        );
    }
    let (s, _) = sig.sec["link"];
    let on = SR / 2;
    let a = s + on + SR / 50..s + on + SR / 12;
    let b0 = s + SR / 8..s + SR / 3;
    let link = (
        db(rms(&yl[a.clone()])) - db(rms(&sig.l[a.clone()])),
        db(rms(&yr[a.clone()])) - db(rms(&sig.r[a])),
        db(rms(&yr[b0.clone()])) - db(rms(&sig.r[b0])),
    );
    let (s, e) = sig.sec["impulse"];
    let k = (s..e).max_by(|a, b| yl[*a].abs().partial_cmp(&yl[*b].abs()).unwrap()).unwrap();
    let (ns, ne) = sig.sec["noise"];
    Res {
        statics,
        burst,
        link,
        impulse_offset: k as i64 - (s + SR / 2) as i64,
        impulse_peak: yl[k],
        noise_rms: db(rms(&yl[ns..ne])),
        noise_in_rms: db(rms(&sig.l[ns..ne])),
        noise_sample_peak: db(sample_peak(&yl[ns..ne])),
        noise_true_peak: db(true_peak(&yl[ns..ns + 131072], 2000)),
        out: (yl, yr),
    }
}

