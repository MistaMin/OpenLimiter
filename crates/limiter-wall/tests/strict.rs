//! The strict true-peak ceiling: this crate's own addition (brief section 3, stage 10 asks for
//! selectable sample peak or true peak). No stored measurement exists for it; the test is the
//! requirement itself: the reconstructed peak (ideal 8x band-limited reconstruction, the
//! method (a) of G section 6) never exceeds the ceiling.

mod common;
use common::*;
use limiter_wall::*;
use olm_api::Module;
use std::f64::consts::PI;

fn signals() -> Vec<(&'static str, Vec<f64>, Vec<f64>)> {
    let n = 3 * SR;
    let t = |i: usize| i as f64 / SR as f64;
    let mut v: Vec<(&'static str, Vec<f64>, Vec<f64>)> = Vec::new();
    let noise = gated_noise(3, 3);
    let noise_r = gated_noise(4, 3);
    v.push(("gated white noise", noise.clone(), noise_r));
    let mut rng = Noise(99);
    let steady: Vec<f64> = (0..n).map(|_| 0.25 * rng.gauss()).collect();
    v.push(("steady white noise", steady.clone(), steady.iter().map(|x| -x).collect()));
    // the same noise without anything above 22 kHz (brick-wall in the frequency domain)
    {
        let m = 262144;
        let mut rng = Noise(123);
        let mut re: Vec<f64> = (0..m).map(|_| 0.25 * rng.gauss()).collect();
        let mut im = vec![0.0; m];
        fft(&mut re, &mut im, false);
        let cut = (22000.0 / SR as f64 * m as f64) as usize;
        for k in cut..=m - cut {
            re[k] = 0.0;
            im[k] = 0.0;
        }
        fft(&mut re, &mut im, true);
        let bl: Vec<f64> = re[..n].iter().map(|v| v / m as f64).collect();
        v.push(("white noise band-limited to 22 kHz", bl.clone(), bl.iter().rev().cloned().collect()));
    }
    let isp: Vec<f64> = (0..n).map(|i| (2.0 * PI * 12000.0 * t(i) + PI / 4.0).sin()).collect();
    v.push(("12 kHz inter-sample tone", isp.clone(), isp));
    let hot: Vec<f64> = (0..n).map(|i| (2.0 * PI * 1000.0 * t(i)).sin()).collect();
    v.push(("1 kHz sine at 0 dBFS", hot.clone(), hot));
    let beat: Vec<f64> = (0..n).map(|i| 0.5 * (2.0 * PI * 21000.0 * t(i)).sin() + 0.5 * (2.0 * PI * 22500.0 * t(i) + 1.0).sin()).collect();
    v.push(("21 kHz + 22.5 kHz beat", beat.clone(), beat));
    let near: Vec<f64> = (0..n).map(|i| (2.0 * PI * 23000.0 * t(i) + 0.3).sin()).collect();
    v.push(("23 kHz tone", near.clone(), near));
    let square: Vec<f64> = (0..n).map(|i| if (2.0 * PI * 997.0 * t(i)).sin() >= 0.0 { 0.9 } else { -0.9 }).collect();
    v.push(("997 Hz square wave", square.clone(), square));
    let mut clicks = vec![0.0; n];
    let mut k = 1000;
    let mut sign = 1.0;
    while k < n - 10 {
        clicks[k] = sign;
        clicks[k + 1] = -sign * 0.9;
        sign = -sign;
        k += 3331;
    }
    v.push(("alternating clicks", clicks.clone(), clicks));
    let bursts = timing_signal(3.0, SR);
    v.push(("timing burst signal", bursts.clone(), bursts));
    let mut rng = Noise(5);
    let sweepy: Vec<f64> = (0..n)
        .map(|i| {
            let f = 200.0 + 23000.0 * (i as f64 / n as f64);
            0.7 * (2.0 * PI * (200.0 * t(i) + 0.5 * (f - 200.0) * t(i))).sin() + 0.2 * rng.gauss()
        })
        .collect();
    v.push(("sweep to 23 kHz plus noise", sweepy.clone(), sweepy.iter().rev().cloned().collect()));
    v
}

#[test]
fn reconstructed_peak_never_exceeds_the_ceiling() {
    let mut c = Check::new();
    let configs: Vec<(&str, Vec<(usize, f32)>, f64)> = vec![
        ("Contour, drive 12, auto", vec![(P_DRIVE, 12.0)], -0.2),
        ("Contour, drive 18, manual 20 ms", vec![(P_DRIVE, 18.0), (P_AUTO_RELEASE, 0.0), (P_RELEASE, 20.0)], -0.2),
        ("Contour, drive 18, manual 0.01 ms, ceiling -1", vec![(P_DRIVE, 18.0), (P_AUTO_RELEASE, 0.0), (P_RELEASE, 0.01), (P_CEILING, -1.0)], -1.0),
        ("Contour, drive 6, soft clip Narrow at -6", vec![(P_DRIVE, 6.0), (P_SOFT_CLIP, 1.0), (P_SOFT_CLIP_THRESHOLD, -6.0), (P_SOFT_CLIP_CURVE, 2.0)], -0.2),
        ("Leveller, threshold -12, ceiling 0, auto", vec![(P_STYLE, 1.0), (P_THRESHOLD, -12.0), (P_CEILING, 0.0)], 0.0),
        ("Leveller, threshold -18, ceiling -0.3, manual 0.01 ms", vec![(P_STYLE, 1.0), (P_THRESHOLD, -18.0), (P_CEILING, -0.3), (P_AUTO_RELEASE, 0.0), (P_RELEASE, 0.01)], -0.3),
        ("Leveller, threshold -24, ceiling -6, manual 100 ms", vec![(P_STYLE, 1.0), (P_THRESHOLD, -24.0), (P_CEILING, -6.0), (P_AUTO_RELEASE, 0.0), (P_RELEASE, 100.0)], -6.0),
    ];
    let sigs = signals();
    let (mut worst_over, mut worst_under) = (f64::NEG_INFINITY, 0.0f64);
    for (cname, sets, ceil) in configs.iter() {
        for (sname, l, r) in sigs.iter() {
            let mut loose = fresh(48000.0, sets);
            let (ll, lr) = run(&mut loose, l, r);
            let tp_loose = db(true_peak(&ll[..131072], 2000).max(true_peak(&lr[..131072], 2000)));
            let mut all = sets.clone();
            all.push((P_STRICT_TRUE_PEAK, 1.0));
            let mut m = fresh(48000.0, &all);
            let (yl, yr) = run(&mut m, l, r);
            let tp = db(true_peak(&yl[..131072], 2000).max(true_peak(&yr[..131072], 2000)));
            let sp = db(sample_peak(&yl).max(sample_peak(&yr)));
            worst_over = worst_over.max(tp - ceil);
            if tp_loose > *ceil {
                worst_under = worst_under.max(ceil - tp);
            }
            c.cmp(&format!("{cname} | {sname}: true peak over the ceiling (dB, <= 0 passes); without strict {:+.2} dB", tp_loose - ceil), (tp - ceil).max(0.0), 0.0, 0.0, 0.0, false);
            c.cmp(&format!("{cname} | {sname}: sample peak over the ceiling (dB)"), (sp - ceil).max(0.0), 0.0, 0.0, 0.0, false);
        }
    }
    c.note("largest true peak relative to the ceiling over all cases (dB)", worst_over, "must be <= 0");
    c.note("largest shortfall under the ceiling where the stage had to work (dB)", worst_under, "cost of the safety margin and gain ramp");
    c.finish("Strict true peak (own addition): 7 settings x 11 signals");
}

#[test]
fn strict_latency_and_unity() {
    let mut c = Check::new();
    for (name, style, base) in [("Contour", 0.0f32, 36.0), ("Leveller", 1.0, 64.0)] {
        let mut m = fresh(48000.0, &[(P_STYLE, style), (P_STRICT_TRUE_PEAK, 1.0), (P_CEILING, 0.0)]);
        let lat = m.latency();
        c.note(&format!("{name}: latency with strict on (samples)"), lat as f64, &format!("{base} without"));
        let mut l = vec![0.0f32; 4096];
        let mut r = vec![0.0f32; 4096];
        l[500] = 0.25;
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            m.process(a, b);
        }
        let k = (0..l.len()).max_by(|a, b| l[*a].abs().partial_cmp(&l[*b].abs()).unwrap()).unwrap();
        c.cmp(&format!("{name}: impulse arrives at the reported latency (samples)"), (k - 500) as f64, lat as f64, 0.0, 0.0, false);
        c.cmp(&format!("{name}: impulse value, unity below the limit"), l[k] as f64, 0.25, 0.0, 1e-7, false);
        // switching strict off restores the style's own latency
        m.set_param(P_STRICT_TRUE_PEAK, 0.0);
        c.cmp(&format!("{name}: latency after switching strict off (samples)"), m.latency() as f64, base, 0.0, 0.0, false);
    }
    c.finish("Strict true peak: latency and unity");
}

/// Where the strict stage stops being a guarantee, and what it costs. Reported, with the one
/// assertion that the near-Nyquist cases are at least improved.
#[test]
fn strict_limits_and_cost() {
    let mut c = Check::new();
    let n = 3 * SR;
    let base = vec![(P_STYLE, 1.0f32), (P_THRESHOLD, -12.0), (P_CEILING, 0.0)];
    let mut on = base.clone();
    on.push((P_STRICT_TRUE_PEAK, 1.0));
    for f in [23500.0, 23800.0, 23950.0] {
        let x: Vec<f64> = (0..n).map(|i| (2.0 * PI * f * i as f64 / SR as f64 + 0.3).sin()).collect();
        let a = run_mono(&mut fresh(48000.0, &base), &x);
        let b = run_mono(&mut fresh(48000.0, &on), &x);
        let (ta, tb) = (db(true_peak(&a[..131072], 2000)), db(true_peak(&b[..131072], 2000)));
        c.note(&format!("{f} Hz tone, 12 dB over: true peak with strict on (dBTP, ceiling 0)"), tb, &format!("{ta:+.2} without"));
        assert!(tb < ta);
    }
    // level cost on material the stage has to work on
    for (name, x) in [("gated white noise", gated_noise(3, 3)), ("timing burst signal", timing_signal(3.0, SR))] {
        let a = run_mono(&mut fresh(48000.0, &base), &x);
        let b = run_mono(&mut fresh(48000.0, &on), &x);
        c.note(&format!("{name}: rms change from switching strict on (dB)"), db(rms(&b)) - db(rms(&a)), "own addition, no stored value");
    }
    // processing speed, one core, 10 s of stereo at 48 kHz in blocks of 512
    for (name, sets) in [("Contour auto", vec![(P_DRIVE, 12.0f32)]), ("Leveller auto", base.clone()), ("Leveller auto + strict", on.clone())] {
        let x = gated_noise(8, 10);
        let mut m = fresh(48000.0, &sets);
        let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
        let mut r = l.clone();
        let t0 = std::time::Instant::now();
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            m.process(a, b);
        }
        c.note(&format!("{name}: times faster than real time"), 10.0 / t0.elapsed().as_secs_f64(), "this machine, release build");
    }
    c.finish("Strict true peak: limits and cost");
}
