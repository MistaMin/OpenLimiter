//! Fitting aid, not a proof: grid search of the auto-release constants against the
//! burst-length table (G section 5). Build with `--features tuning`, run with `--ignored`.
#![cfg(feature = "tuning")]

mod common;
use common::*;
use limiter_wall::*;
use std::f64::consts::PI;

fn short_bursts() -> (Vec<f64>, [usize; 3], [usize; 3]) {
    let n = 10 * SR;
    let (lo, hi) = (lin(-30.0), lin(-6.0));
    let mut a = vec![lo; n];
    let starts = [SR, 4 * SR, 7 * SR];
    let lens = [5usize, 20, 100];
    let mut ends = [0; 3];
    for k in 0..3 {
        let (i0, i1) = (starts[k], starts[k] + lens[k] * SR / 1000);
        ends[k] = i1;
        for v in a[i0..i1].iter_mut() {
            *v = hi;
        }
        for j in 0..48 {
            let r = 0.5 - 0.5 * (PI * j as f64 / 48.0).cos();
            a[i0 + j] = lo + (hi - lo) * r;
            a[i1 - 48 + j] = lo + (hi - lo) * (0.5 - 0.5 * (PI * (47 - j) as f64 / 48.0).cos());
        }
    }
    let x = (0..n).map(|i| (a[i] * (2.0 * PI * 1000.0 * i as f64 / SR as f64).sin() * 32767.0).trunc() / 32768.0).collect();
    (x, starts, ends)
}

fn eval(t: Tuning, profile: f32, x: &[f64], starts: &[usize; 3], ends: &[usize; 3], es: &[f64]) -> [(f64, f64, f64); 3] {
    let mut m = fresh(48000.0, &[(P_DRIVE, 12.0), (P_AUTO_RELEASE, 1.0), (P_PROFILE, profile)]);
    m.set_tuning(t);
    let y = run_mono(&mut m, x);
    let ey = envelope(&y);
    let g: Vec<f64> = ey.iter().zip(es.iter()).map(|(a, b)| 20.0 * (a + 1e-12).log10() - 20.0 * (b + 1e-12).log10()).collect();
    let mut out = [(0.0, 0.0, 0.0); 3];
    for k in 0..3 {
        let pre = median(&g[starts[k] - 9600..starts[k] - 480]);
        let j = ends[k] + 144;
        let g0 = g[j..j + 960].iter().cloned().fold(f64::INFINITY, f64::min);
        let seg = &g[j..(j + 2 * SR + SR / 2).min(g.len())];
        let rec = |fr: f64| seg.iter().position(|v| *v >= g0 + fr * (pre - g0)).map(|i| (i as f64 + 144.0) / 48.0).unwrap_or(9999.0);
        out[k] = (g0, rec(0.5), rec(0.9));
    }
    out
}

#[test]
#[ignore]
fn fit_contour_auto() {
    let (x, starts, ends) = short_bursts();
    let es = envelope(&x);
    let want: [[(f64, f64, f64); 3]; 4] = [
        [(11.49, 3.8, 4.9), (7.12, 412.9, 1301.9), (5.67, 602.8, 1509.9)],
        [(11.84, 3.0, 3.0), (6.24, 487.4, 1387.4), (5.67, 602.6, 1509.9)],
        [(7.51, 6.5, 15.9), (5.96, 462.4, 1365.9), (5.67, 602.9, 1510.4)],
        [(11.87, 3.0, 3.0), (6.51, 235.8, 683.9), (5.67, 301.4, 754.9)],
    ];
    for p in 0..4 {
        let mut best = (f64::INFINITY, Tuning::DEFAULT, [(0.0, 0.0, 0.0); 3]);
        let fasts: &[f64] = if p == 2 { &[3.0, 3.4, 3.8, 4.2] } else if p == 0 { &[0.6, 0.8, 1.0] } else { &[0.3, 0.5] };
        for &fast in fasts {
            for q in [3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0] {
                for co in [1.0, 2.0, 3.0, 4.0, 5.0, 6.5, 8.0, 10.0, 13.0] {
                    for ci in [4.0, 6.0, 8.0, 10.0, 12.0, 15.0, 18.0, 22.0, 26.0, 32.0] {
                        let mut t = Tuning::DEFAULT;
                        t.contour_fast_ms[p] = fast;
                        t.contour_qualify_ms[p] = q;
                        t.contour_charge_out_ms[p] = co;
                        t.contour_charge_in_ms[p] = ci;
                        let got = eval(t, p as f32, &x, &starts, &ends, &es);
                        let mut e = 0.0;
                        for k in 0..3 {
                            e += ((got[k].0 - want[p][k].0) / 0.1).powi(2);
                            for (a, b) in [(got[k].1, want[p][k].1), (got[k].2, want[p][k].2)] {
                                let d = (a - b).abs();
                                e += (d / (0.1 * b).max(0.3)).powi(2);
                            }
                        }
                        if e < best.0 {
                            best = (e, t, got);
                        }
                    }
                }
            }
        }
        let t = best.1;
        println!(
            "profile {p}: err {:.2} fast {} qualify {} charge_out {} charge_in {} -> {:?} want {:?}",
            best.0, t.contour_fast_ms[p], t.contour_qualify_ms[p], t.contour_charge_out_ms[p], t.contour_charge_in_ms[p], best.2, want[p]
        );
    }
}

#[test]
#[ignore]
fn fit_leveller_auto() {
    use common::composite::*;
    let sig = composite();
    type Row = [(f64, f64, f64, f64); 5];
    let want: [(f32, Row); 3] = [
        (-12.0, [(0.06, 0.79, 439.54, 10.59), (0.0, 0.73, 0.83, 12.0), (0.0, 0.83, 511.73, 12.0), (15.81, 290.62, 777.92, 12.0), (110.77, 396.73, 878.88, 12.0)]),
        (-6.0, [(0.04, 0.83, 541.42, 5.09), (0.0, 0.73, 0.83, 6.0), (0.0, 0.83, 1.0, 6.0), (16.0, 347.77, 841.88, 6.0), (124.88, 453.79, 939.98, 6.0)]),
        (-18.0, [(0.02, 0.77, 375.12, 16.33), (0.0, 0.71, 0.85, 18.0), (0.0, 0.81, 455.88, 18.0), (15.81, 240.88, 722.0, 18.0), (108.9, 346.92, 823.92, 18.0)]),
    ];
    let grid = |name: &str, d: f64| -> Vec<f64> {
        std::env::var(name).ok().map(|s| s.split(',').map(|v| v.parse().unwrap()).collect()).unwrap_or(vec![d])
    };
    let mut best = (f64::INFINITY, Tuning::DEFAULT, 0);
    for q in grid("FQ", 12.0) {
        for ch in grid("FC", 100.0) {
            for sl in grid("FS", 192.0) {
                for pw in grid("FP", 0.55) {
                  for sm in grid("FM", 10.0) {
                  for co in grid("FO", 60.0) {
                    let mut t = Tuning::DEFAULT;
                    t.leveller_qualify_ms = q;
                    t.leveller_charge_ms = ch;
                    t.leveller_slow_ms = sl;
                    t.leveller_power = pw;
                    t.leveller_smooth_ms = sm;
                    t.leveller_charge_out_ms = co;
                    let (mut e, mut fails) = (0.0, 0);
                    for (thr, w) in want.iter() {
                        let mut m = fresh(48000.0, &[(P_STYLE, STYLE_LEVELLER), (P_THRESHOLD, *thr), (P_CEILING, 0.0)]);
                        m.set_tuning(t);
                        let r = analyse_with(&sig, m);
                        for (k, len) in T_BL.iter().enumerate() {
                            let b = &r.burst[len];
                            let d0 = (b.before - w[k].3).abs() / 0.1;
                            e += d0 * d0;
                            if d0 > 1.0 {
                                fails += 1;
                            }
                            for (a, s) in [(b.rec50, w[k].0), (b.rec90, w[k].1), (b.rec99, w[k].2)] {
                                let a = if a.is_finite() { a } else { 5000.0 };
                                let d = ((a - s).abs() / (0.1 * s).max(0.3)).min(8.0);
                                e += d * d;
                                if d > 1.0 {
                                    fails += 1;
                                }
                            }
                        }
                    }
                    println!("q {q} charge {ch} out {co} slow {sl} power {pw} smooth {sm}: err {e:.1} fails {fails}");
                    if e < best.0 {
                        best = (e, t, fails);
                    }
                  }
                  }
                }
            }
        }
    }
    let t = best.1;
    println!("BEST err {:.1} fails {} q {} charge {} slow {} power {} smooth {}", best.0, best.2, t.leveller_qualify_ms, t.leveller_charge_ms, t.leveller_slow_ms, t.leveller_power, t.leveller_smooth_ms);
}
