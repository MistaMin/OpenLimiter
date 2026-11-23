//! Spec 7.7 (Glide): two pulses closer than the lookahead are bridged flat (no gain
//! modulation between them); an isolated snare gets no gain reduction earlier than 2.5 ms
//! before its peak, whatever the lookahead.

mod common;
use common::*;
use limiter_smart::idx;

fn burst(x: &mut [f64], at: usize, len: usize, amp: f64, f: f64) {
    for k in 0..len {
        let w = (std::f64::consts::PI * k as f64 / len as f64).sin();
        x[at + k] += amp * w * (2.0 * std::f64::consts::PI * f * k as f64 / SR).sin();
    }
}

/// Index of the largest |x| in `range`.
fn argmax(x: &[f64], range: std::ops::Range<usize>) -> usize {
    let mut best = range.start;
    for i in range {
        if x[i].abs() > x[best].abs() {
            best = i;
        }
    }
    best
}

#[test]
fn double_pulse_valley_is_bridged_flat() {
    for (spacing_ms, la) in [(8.0, 15.0f32), (5.0, 10.0), (2.0, 3.0)] {
        let mut m = make(SR, &[engine(5), (idx::LOOKAHEAD, la), (idx::RELEASE, 20.0), (idx::AUTO_RELEASE, 0.0)]);
        let n = 24000;
        let mut x = vec![0.0; n];
        let at1 = 9600;
        let at2 = at1 + (spacing_ms * SR / 1000.0) as usize;
        burst(&mut x, at1, 48, 2.0, 2000.0);
        burst(&mut x, at2, 48, 2.0, 2000.0);
        let (y, _, g) = run_traced(&mut m, &x, &x);
        // from the deepest point of the first pulse to that of the second
        let p1 = (at1..at1 + 48).min_by(|a, b| g[*a].partial_cmp(&g[*b]).unwrap()).unwrap();
        let p2 = (at2..at2 + 48).min_by(|a, b| g[*a].partial_cmp(&g[*b]).unwrap()).unwrap();
        let seg = &g[p1..=p2];
        let span = seg.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - seg.iter().cloned().fold(f64::INFINITY, f64::min);
        // and against the straight bridge between the two deepest points (the hull)
        let bend = seg
            .iter()
            .enumerate()
            .map(|(k, v)| (v - (g[p1] + (g[p2] - g[p1]) * k as f64 / (p2 - p1) as f64)).abs())
            .fold(0.0, f64::max);
        // without bridging the gain would recover between the pulses: compare the release
        // the same pulse gets after the second one over the same distance
        let after = g[p2 + (p2 - p1)] - g[p2];
        println!(
            "Glide pulses {spacing_ms} ms apart, lookahead {la} ms: gain between the deepest points {:.4} -> {:.4} dB, modulation {:.5} dB, off the straight bridge {:.5} dB (the same distance after the second pulse recovers {:.2} dB); peak {:.3} dBFS",
            g[p1],
            g[p2],
            span,
            bend,
            after,
            db(peak(&y))
        );
        assert!(span < 0.01, "valley must be bridged flat");
        assert!(after > 0.1, "the release would otherwise recover");
    }
}

#[test]
fn isolated_snare_has_no_pre_suck_before_2_5_ms() {
    for la in [1.5f32, 5.0, 15.0] {
        let mut m = make(SR, &[engine(5), (idx::LOOKAHEAD, la)]);
        let n = 24000;
        let mut x = noise(n, 0.01, 5);
        let at = 12000;
        // snare-like: a fast 200 Hz thump with noise, peaking about 1 ms in
        for k in 0..2400 {
            let env = (-(k as f64) / 300.0).exp() * (1.0 - (-(k as f64) / 20.0).exp());
            x[at + k] += 3.0 * env * ((2.0 * std::f64::consts::PI * 200.0 * k as f64 / SR).sin() + 0.5 * noise(1, 1.0, 1000 + k as u64)[0]);
        }
        let (_, _, g) = run_traced(&mut m, &x, &x);
        let pk = argmax(&x, at..at + 2400);
        // the first input sample over the ceiling (one sample earlier for the reconstructed
        // interval before it): the earliest point that needs any reduction
        let need = (0..n).find(|&i| x[i].abs() > lin(-0.2)).unwrap_or(n).saturating_sub(1);
        let first = (0..n).find(|&i| g[i] < -1e-4).unwrap_or(n);
        let lead_ms = (need as f64 - first as f64) * 1000.0 / SR;
        println!(
            "Glide lookahead {la:4} ms: first gain reduction {:.3} ms before the first sample that needs it ({:.3} ms before the snare peak; deepest {:.2} dB)",
            lead_ms,
            (pk as f64 - first as f64) * 1000.0 / SR,
            g.iter().cloned().fold(0.0, f64::min)
        );
        assert!(lead_ms <= 2.5 + 1e-9, "pre-suck {lead_ms} ms");
        assert!(lead_ms > 0.0);
    }
}
