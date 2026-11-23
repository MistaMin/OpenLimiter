//! Channel sets, mid/side, external key, key link, latency, block independence, meters.
mod common;
use common::*;
use comp_dual::CompDual;
use olm_api::Module;

/// Level change of a 1 kHz tone at -12 dBFS (left) and a 1.5 kHz tone at -18 dBFS (right).
fn lr_change(settings: &[(&str, f32)]) -> (f64, f64) {
    let n = 3 * SR;
    let l = sine(1000.0, lin(-12.0), 0.0, n);
    let r = sine(1500.0, lin(-18.0), 0.7, n);
    let (yl, yr) = render2(&mut with(settings), &l, &r, None);
    (lvl(&yl, &l, 1.5, 2.5), lvl(&yr, &r, 1.5, 2.5))
}

#[test]
fn channel_sets() {
    // measurements/srcB_features/channel_sets.json (spec section 17). The stored stimulus (a stereo
    // transient file) is not reproduced; what is checked is which channel each set drives.
    let mut t = Table::new("Channel sets (channel_sets.json, structure)");
    let (l, r) = lr_change(&[("threshold1", -30.0)]);
    t.truth("ganged, set 1 engaged, set 1 selected: both reduced, same gain", l < -5.0 && (l - r).abs() < 0.01, &format!("L {l:.2} R {r:.2} dB (stored -10.2 / -9.5)"));
    let (l, r) = lr_change(&[("threshold1", -30.0), ("gangedSet", 1.0)]);
    t.truth("ganged, set 1 engaged, set 2 selected: untouched", l.abs() < 0.01 && r.abs() < 0.01, &format!("L {l:.2} R {r:.2} dB (stored 0.0 / 0.0)"));
    let (l, r) = lr_change(&[("threshold2", -30.0)]);
    t.truth("ganged, set 2 engaged, set 1 selected: untouched", l.abs() < 0.01 && r.abs() < 0.01, &format!("L {l:.2} R {r:.2} dB (stored 0.0 / 0.0)"));
    let (l, r) = lr_change(&[("threshold2", -30.0), ("gangedSet", 1.0)]);
    t.truth("ganged, set 2 engaged, set 2 selected: both reduced", l < -5.0 && (l - r).abs() < 0.01, &format!("L {l:.2} R {r:.2} dB (stored -10.2 / -9.5)"));
    let (l, r) = lr_change(&[("channelMode", 1.0), ("threshold1", -30.0)]);
    t.truth("per channel, set 1 engaged: left only", l < -5.0 && r.abs() < 0.01, &format!("L {l:.2} R {r:.2} dB (stored -10.2 / 0.0)"));
    let (l, r) = lr_change(&[("channelMode", 1.0), ("threshold2", -30.0)]);
    t.truth("per channel, set 2 engaged: right only", l.abs() < 0.01 && r < -5.0, &format!("L {l:.2} R {r:.2} dB (stored 0.0 / -9.5)"));
    // per channel, a steady tone at -12 dBFS with threshold -30: the static table value
    let (l, _) = lr_change(&[("channelMode", 1.0), ("threshold1", -30.0)]);
    t.level("per channel, left 1 kHz at -12 dBFS (static table -9.35)", l, -9.35, 0.2);
    t.finish();
}

#[test]
fn mid_side() {
    // measurements/srcB_features/equivalence_and_ms.json (spec section 20): set 1 drives mid, set 2 side.
    let n = 3 * SR;
    let l = sine(1000.0, lin(-12.0), 0.0, n);
    let r = sine(1500.0, lin(-18.0), 0.7, n);
    let ms = |a: &[f32], b: &[f32]| -> (Vec<f32>, Vec<f32>) {
        (a.iter().zip(b).map(|(x, y)| 0.5 * (x + y)).collect(), a.iter().zip(b).map(|(x, y)| 0.5 * (x - y)).collect())
    };
    let (m0, s0) = ms(&l, &r);
    let change = |settings: &[(&str, f32)]| {
        let (yl, yr) = render2(&mut with(settings), &l, &r, None);
        let (m, s) = ms(&yl, &yr);
        (lvl(&m, &m0, 1.5, 2.5), lvl(&s, &s0, 1.5, 2.5))
    };
    let mut t = Table::new("Mid / side (equivalence_and_ms.json, structure)");
    let (m, s) = change(&[("channelMode", 2.0), ("threshold1", -42.0)]);
    t.truth("set 1 engaged: mid reduced, side untouched", m < -5.0 && s.abs() < 0.01, &format!("mid {m:.2} side {s:.2} dB (stored -14.07 / 0.00)"));
    let (m, s) = change(&[("channelMode", 2.0), ("threshold2", -42.0)]);
    t.truth("set 2 engaged: side reduced, mid untouched", s < -5.0 && m.abs() < 0.01, &format!("mid {m:.2} side {s:.2} dB (stored 0.00 / -14.07)"));
    let (m, s) = change(&[("channelMode", 2.0)]);
    t.level("no reduction: mid", m, 0.0, 0.001);
    t.level("no reduction: side", s, 0.0, 0.001);
    t.finish();
}

/// Key: bursts of a tone, 100 ms long every 0.5 s starting at 0.2 s, at -3 dBFS, 5 ms fades.
fn key_bursts(freq: f64, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f64 / SRF;
            let tb = (t - 0.2).rem_euclid(0.5);
            let e = if t < 0.2 || tb >= 0.1 { 0.0 } else { (tb / 0.005).min(1.0).min((0.1 - tb) / 0.005) };
            (lin(-3.0) * e * (PI2 * freq * t).sin()) as f32
        })
        .collect()
}

/// Gain of the 1 kHz main tone late in the key bursts and just before the next burst.
fn key_gains(y: &[f32], x: &[f32]) -> (f64, f64) {
    let (mut during, mut between) = (0.0, 0.0);
    for k in 2..6 {
        let b0 = 0.2 + 0.5 * k as f64;
        during += lvl(y, x, b0 + 0.06, b0 + 0.1) / 4.0;
        between += lvl(y, x, b0 + 0.44, b0 + 0.5) / 4.0;
    }
    (during, between)
}

#[test]
fn external_key() {
    // measurements/sidechain_external.json (spec section 25): main = steady 1 kHz at -12 dBFS,
    // key = 400 Hz bursts (100 ms at -3 dBFS every 0.5 s), threshold -30.
    let n = 4 * SR;
    let x = sine(1000.0, lin(-12.0), 0.0, n);
    let key = key_bursts(400.0, n);
    let silent = vec![0.0f32; n];
    let mut t = Table::new("External key (sidechain_external.json)");
    let (y, _) = render2(&mut with(&[("threshold1", -30.0)]), &x, &x, Some((&key, &key)));
    let (d, b) = key_gains(&y, &x);
    t.level("internal, key routed but unused: during key bursts", d, -9.35, 0.2);
    t.level("internal, key routed but unused: between bursts", b, -9.35, 0.2);
    let (y, _) = render2(&mut with(&[("threshold1", -30.0), ("keySource", 1.0)]), &x, &x, Some((&key, &key)));
    let (d, b) = key_gains(&y, &x);
    // The stored windows are not documented; "during" is read late in the burst, "between" just
    // before the next burst. 0.5 dB.
    t.level("external, key routed: during key bursts", d, -12.39, 0.5);
    t.level("external, key routed: between bursts", b, -0.37, 0.5);
    let leak = db(tone_amp(&y, 400.0, SR, 3 * SR)) - (-12.0);
    t.truth("external: the key is not mixed into the output", leak < -60.0, &format!("{leak:.1} dB relative to the main tone (stored -73.4)"));
    let (y, _) = render2(&mut with(&[("threshold1", -30.0), ("keySource", 1.0)]), &x, &x, Some((&silent, &silent)));
    let (d, b) = key_gains(&y, &x);
    t.level("external, silent key: during", d, 0.0, 0.01);
    t.level("external, silent key: between", b, 0.0, 0.01);
    // `process` without a key while External is selected behaves like a silent key.
    let y = render(&mut with(&[("threshold1", -30.0), ("keySource", 1.0)]), &x);
    t.level("external, process() without a key", lvl(&y, &x, 1.0, 3.0), 0.0, 0.01);
    t.finish();
}

#[test]
fn key_link_and_key_filter() {
    // measurements/srcB_features/sidechain_link_filter_safety.json, "sidechain" (spec section 28):
    // external key, per channel, set 1 threshold -30, set 2 at default; 80 Hz bursts on the left key only.
    let n = 4 * SR;
    let x = sine(1000.0, lin(-12.0), 0.0, n);
    let key = key_bursts(80.0, n);
    let silent = vec![0.0f32; n];
    let base = [("channelMode", 1.0), ("keySource", 1.0), ("threshold1", -30.0)];
    let run = |extra: &[(&str, f32)]| {
        let mut s = base.to_vec();
        s.extend_from_slice(extra);
        let (yl, yr) = render2(&mut with(&s), &x, &x, Some((&key, &silent)));
        (key_gains(&yl, &x).0, key_gains(&yr, &x).0)
    };
    let mut t = Table::new("Key link and key filter (sidechain_link_filter_safety.json)");
    // 0.6 dB on the engaged cells: burst level and reading window of the stored run are not documented.
    let (l, r) = run(&[]);
    t.level("link off: left during left key", l, -12.78, 0.6);
    t.level("link off: right during left key", r, -0.04, 0.1);
    let (l, r) = run(&[("keyLink", 1.0)]);
    t.level("link on: left during left key", l, -12.78, 0.6);
    t.level("link on: right during left key", r, -12.78, 0.6);
    t.level("link on: left and right get the same gain", l - r, 0.0, 0.01);
    let (l, r) = run(&[("keyFilter", 1.0)]);
    t.level("key filter on, full band: left", l, -12.78, 0.6);
    t.level("key filter on, full band: right", r, -0.04, 0.1);
    let (l, _) = run(&[("keyFilter", 1.0), ("filterType1", 2.0)]);
    t.level("key filter on, high band: the 80 Hz key no longer triggers", l, 0.0, 0.05);
    let (l, _) = run(&[("filterType1", 2.0)]);
    t.level("key filter off, high band: the key triggers", l, -12.78, 0.6);
    t.finish();
}

#[test]
fn reported_latency_is_exact() {
    // spec section 18: after compensating the reported latency the residual lag is 0 samples.
    // Here: with the gain computer neutral, the compensated output equals the input.
    let x: Vec<f32> = {
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        (0..SR).map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            ((s >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 0.5
        }).collect()
    };
    let neutral = [("ratio1", 1.0f32), ("softKnee1", 0.0)];
    let cases: [(&str, Vec<(&str, f32)>, f64); 6] = [
        ("default", vec![], -140.0),
        ("limiter off", vec![("limiterOn", 0.0)], -140.0),
        ("look-ahead 10 ms", vec![("lookahead", 10.0)], -140.0),
        ("look-ahead 63 ms", vec![("lookahead", 63.0)], -140.0),
        ("low band split", vec![("filterType1", 0.0)], -120.0),
        ("mid band split, look-ahead 1.6 ms, mid/side", vec![("filterType1", 1.0), ("lookahead", 1.6), ("channelMode", 2.0)], -120.0),
    ];
    let mut t = Table::new("Latency (spec 18)");
    for (name, extra, floor) in cases.iter() {
        let mut s = neutral.to_vec();
        s.extend_from_slice(extra);
        let mut m = with(&s);
        let lat = m.latency();
        let y = render(&mut m, &x);
        let res: Vec<f32> = y.iter().zip(x.iter()).map(|(a, b)| a - b).collect();
        let rel = db(rms(&res)) - db(rms(&x));
        t.truth(&format!("{name}: {lat} samples reported, residual after compensation"), rel < *floor, &format!("{rel:.1} dB"));
    }
    // the figures themselves, at 48 kHz
    t.level("latency, default (8 detector + 79 limiter)", with(&[]).latency() as f64, 87.0, 0.0);
    t.level("latency, limiter off", with(&[("limiterOn", 0.0)]).latency() as f64, 8.0, 0.0);
    t.level("latency, band split on, limiter on", with(&[("filterType1", 0.0)]).latency() as f64, 87.0 + 1261.0, 0.0);
    t.finish();
}

#[test]
fn housekeeping() {
    let mut t = Table::new("Contract");
    // Not prepared: process leaves the audio alone and does not panic.
    let mut m = CompDual::default();
    let (mut a, mut b) = (vec![0.25f32; 64], vec![-0.25f32; 64]);
    m.process(&mut a, &mut b);
    t.truth("process before prepare is a no-op", a.iter().all(|v| *v == 0.25), "");
    // Unknown index, NaN and out-of-range values.
    let mut m = fresh();
    m.set_param(10_000, 1.0);
    m.set_param(0, f32::NAN);
    m.set_param(0, 1.0e9);
    t.truth("out-of-range values are clamped", m.param(0) == Some(12.0), "inputGain 1e9 -> 12");
    // Every parameter at both ends, with audio running: finite output, no panic.
    let x = sine(997.0, 0.5, 0.0, 2048);
    let mut ok = true;
    for i in 0..m.params().len() {
        for end in 0..2 {
            let mut m = fresh();
            let p = m.params()[i];
            m.set_param(i, if end == 0 { p.min } else { p.max });
            let (mut l, mut r) = (x.clone(), x.clone());
            m.process(&mut l, &mut r);
            m.process_keyed(&mut l, &mut r, &x[..100], &x);
            ok &= l.iter().chain(r.iter()).all(|v| v.is_finite());
        }
    }
    t.truth("every parameter at both ends gives finite output", ok, &format!("{} parameters", m.params().len()));
    // Non-finite input does not poison the state.
    let mut m = with(&[("threshold1", -30.0)]);
    let (mut l, mut r) = (vec![f32::NAN; 32], vec![f32::INFINITY; 32]);
    m.process(&mut l, &mut r);
    let (mut l, mut r) = (x.clone(), x.clone());
    m.process(&mut l, &mut r);
    t.truth("NaN / inf input is treated as silence", l.iter().all(|v| v.is_finite()), "");
    // Block size does not change the result; reset restores the start state.
    let x = sine(1000.0, 0.4, 0.0, 6000);
    let mut m = with(&[("threshold1", -30.0), ("filterType1", 1.0), ("lookahead", 2.0)]);
    let (mut a1, mut a2) = (x.clone(), x.clone());
    m.process(&mut a1, &mut a2);
    m.reset();
    let (mut b1, mut b2) = (x.clone(), x.clone());
    for (c1, c2) in b1.chunks_mut(7).zip(b2.chunks_mut(7)) {
        m.process(c1, c2);
    }
    t.truth("block size 6000 against 7: identical", a1 == b1, "");
    // Meters: a steady tone 18 dB over a hard 2:1 knee (detector 2.1 dB under the peak) reads about 8 dB.
    let x = sine(1000.0, lin(-12.0), 0.0, SR);
    let mut m = with(&[("threshold1", -30.0), ("softKnee1", 0.0), ("channelMode", 1.0)]);
    let (mut l, mut r) = (x.clone(), x.clone());
    m.process(&mut l, &mut r);
    let mt = m.meters();
    t.level("meter: reduction of the engaged set", mt.band_reduction_db[0] as f64, 8.0, 0.3);
    t.level("meter: the other set", mt.band_reduction_db[1] as f64, 0.0, 0.0);
    t.level("meter: overall", mt.reduction_db as f64, mt.band_reduction_db[0] as f64, 0.0);
    // Cost of the heaviest setting (two different low bands at 41 Hz, RMS, look-ahead, limiter).
    let mut m = with(&[("channelMode", 1.0), ("filterType1", 0.0), ("centerFreq1", 41.0), ("filterType2", 1.0), ("centerFreq2", 60.0), ("lookahead", 10.0), ("keyFilter", 1.0), ("keySource", 1.0)]);
    let secs = 5;
    let x = sine(220.0, 0.3, 0.0, secs * SR);
    let (mut l, mut r) = (x.clone(), x.clone());
    let t0 = std::time::Instant::now();
    for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        let k = vec![0.1f32; a.len()];
        m.process_keyed(a, b, &k, &k);
    }
    let rt = secs as f64 / t0.elapsed().as_secs_f64();
    t.truth("heaviest setting runs faster than real time at 48 kHz", rt > 2.0, &format!("{rt:.0} x real time on this machine"));
    // 96 kHz spot check: same static gain as at 48 kHz (-9.35 dB at -12 dBFS, threshold -30).
    let mut m = CompDual::default();
    m.prepare(96_000.0, 512);
    m.set_param(CompDual::param_index("threshold1").unwrap(), -30.0);
    m.reset();
    let x96: Vec<f32> = (0..2 * 96_000).map(|i| (lin(-12.0) * (PI2 * 1000.0 * i as f64 / 96_000.0).sin()) as f32).collect();
    let (mut l, mut r) = (x96.clone(), x96.clone());
    m.process(&mut l, &mut r);
    let g = db(rms(&l[96_000..180_000])) - db(rms(&x96[96_000..180_000]));
    t.level("96 kHz: gain at -12 dBFS, threshold -30", g, -9.35, 0.2);
    t.finish();
}
