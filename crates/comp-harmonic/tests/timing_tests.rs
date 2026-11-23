//! Attack, release, programme-dependent and automatic timing per mode.

mod common;
use common::*;
use comp_harmonic::{idx, Mode};

/// Gain (dB) per sample of the module for a DC step: `lo` dBFS for `pre` s, then `hi` for `post` s.
fn gain_trace(mode: Mode, settings: &[(usize, f32)], levels: &[(f32, f32)]) -> Vec<f64> {
    let mut s = with(&neutral(), settings);
    s.push((idx::MODE, mode as usize as f32));
    let mut m = module(SR, 512, &s);
    let mut x = Vec::new();
    for &(db_level, secs) in levels {
        x.extend(std::iter::repeat_n(10.0f32.powf(db_level / 20.0), (SR * secs) as usize));
    }
    let (y, _) = run(&mut m, &x, &x, 512);
    y.iter().zip(&x).map(|(&o, &i)| db(o as f64) - db(i as f64)).collect()
}

/// Time (ms) after `from` until the gain has moved 63.2 % of the way to its final value.
fn time63(g: &[f64], from: usize) -> f64 {
    let start = g[from.saturating_sub(1)];
    let end = g[g.len() - 1];
    let goal = start + (end - start) * (1.0 - (-1.0f64).exp());
    let i = (from..g.len()).find(|&i| if end < start { g[i] <= goal } else { g[i] >= goal }).unwrap();
    (i - from) as f64 * 1000.0 / SR as f64
}

#[test]
fn class_a_peak_attack_and_release_follow_the_controls() {
    let base = [(idx::THRESHOLD, -20.0), (idx::RATIO, 100.0), (idx::KNEE, 0.0)];
    for (att, rel) in [(20.0, 100.0), (60.0, 400.0)] {
        let s = with(&base, &[(idx::ATTACK, att), (idx::RELEASE, rel)]);
        let g = gain_trace(Mode::ClassAPeak, &s, &[(-40.0, 0.2), (0.0, 1.5)]);
        let a = time63(&g, (0.2 * SR) as usize);
        let g = gain_trace(Mode::ClassAPeak, &s, &[(0.0, 1.5), (-40.0, 3.0)]);
        let r = time63(&g, (1.5 * SR) as usize);
        println!("Class-A Peak attack {att} ms -> {a:.2} ms, release {rel} ms -> {r:.2} ms");
        // The onset relief (a 1.5 ms tracker) briefly eases the attack; release is a pure one-pole in dB.
        assert!((a - att as f64).abs() < 0.12 * att as f64, "attack {a}");
        assert!((r - rel as f64).abs() < 0.01 * rel as f64, "release {r}");
    }
}

#[test]
fn auto_release_and_attack_adapt_to_sustained_reduction() {
    let base = [(idx::THRESHOLD, -30.0), (idx::RATIO, 100.0), (idx::KNEE, 0.0), (idx::RELEASE, 100.0), (idx::ATTACK, 20.0)];
    for mode in [Mode::ClassAPeak, Mode::HybridFfFb] {
        let manual = gain_trace(mode, &base, &[(0.0, 4.0), (-50.0, 3.0)]);
        let auto = gain_trace(mode, &with(&base, &[(idx::AUTO_RELEASE, 1.0)]), &[(0.0, 4.0), (-50.0, 3.0)]);
        let rm = time63(&manual, (4.0 * SR) as usize);
        let ra = time63(&auto, (4.0 * SR) as usize);
        println!("{}: release after 4 s of 30 dB reduction: manual {rm:.1} ms, auto {ra:.1} ms", mode.label());
        // Deep sustained reduction takes the adaptive factor to 1: release x2 (x2.5 hybrid).
        assert!(ra > 1.6 * rm, "{ra} vs {rm}");
        let am = time63(&gain_trace(mode, &base, &[(-50.0, 0.5), (0.0, 1.0)]), (0.5 * SR) as usize);
        let aa = time63(&gain_trace(mode, &with(&base, &[(idx::AUTO_ATTACK, 1.0)]), &[(-50.0, 0.5), (0.0, 1.0)]), (0.5 * SR) as usize);
        println!("{}: attack from rest: manual {am:.2} ms, auto {aa:.2} ms", mode.label());
        // From rest the adaptive factor is 0: attack x0.3.
        assert!(aa < 0.5 * am, "{aa} vs {am}");
    }
}

#[test]
fn circuit_modes_follow_attack_and_release() {
    for mode in [Mode::DiodeBridge, Mode::BusVca, Mode::FastFet, Mode::HybridFfFb] {
        let base = [(idx::THRESHOLD, -30.0), (idx::RATIO, 20.0), (idx::KNEE, 0.0)];
        let fast = with(&base, &[(idx::ATTACK, 1.0), (idx::RELEASE, 50.0)]);
        let slow = with(&base, &[(idx::ATTACK, 30.0), (idx::RELEASE, 800.0)]);
        let a1 = time63(&gain_trace(mode, &fast, &[(-60.0, 0.2), (0.0, 2.0)]), (0.2 * SR) as usize);
        let a2 = time63(&gain_trace(mode, &slow, &[(-60.0, 0.2), (0.0, 2.0)]), (0.2 * SR) as usize);
        let r1 = time63(&gain_trace(mode, &fast, &[(0.0, 2.0), (-60.0, 4.0)]), (2.0 * SR) as usize);
        let r2 = time63(&gain_trace(mode, &slow, &[(0.0, 2.0), (-60.0, 4.0)]), (2.0 * SR) as usize);
        println!("{}: attack 1 / 30 ms -> {a1:.2} / {a2:.2} ms; release 50 / 800 ms -> {r1:.1} / {r2:.1} ms", mode.label());
        assert!(a2 > 4.0 * a1, "attack");
        assert!(r2 > 4.0 * r1, "release");
        assert!(r1 > a1, "release slower than attack");
    }
    // Auto attack in the circuit modes uses each circuit's own attack (bridge 2 ms, bus 3 ms, FET 0.2 ms).
    let base = [(idx::THRESHOLD, -30.0), (idx::RATIO, 20.0), (idx::ATTACK, 100.0), (idx::AUTO_ATTACK, 1.0)];
    for mode in [Mode::DiodeBridge, Mode::BusVca, Mode::FastFet] {
        let a = time63(&gain_trace(mode, &base, &[(-60.0, 0.2), (0.0, 1.0)]), (0.2 * SR) as usize);
        assert!(a < 10.0, "{} auto attack {a}", mode.label());
    }
}

#[test]
fn diode_bridge_auto_release_holds_after_long_reduction() {
    let base = [(idx::THRESHOLD, -30.0), (idx::RATIO, 20.0), (idx::RELEASE, 50.0)];
    let manual = gain_trace(Mode::DiodeBridge, &base, &[(0.0, 3.0), (-60.0, 6.0)]);
    let auto = gain_trace(Mode::DiodeBridge, &with(&base, &[(idx::AUTO_RELEASE, 1.0)]), &[(0.0, 3.0), (-60.0, 6.0)]);
    let i = (3.0 * SR) as usize;
    let rm = time63(&manual, i);
    let ra = time63(&auto, i);
    println!("Diode Bridge release after 3 s: manual {rm:.1} ms, auto (recovery memory) {ra:.1} ms");
    assert!(ra > 1.5 * rm);
}

#[test]
fn optical_ignores_the_time_controls_and_releases_by_history() {
    let x = |a: f32, r: f32| gain_trace(Mode::Optical, &[(idx::THRESHOLD, -30.0), (idx::ATTACK, a), (idx::RELEASE, r)], &[(-6.0, 0.3), (-60.0, 1.0)]);
    assert!(x(1.0, 10.0) == x(200.0, 2000.0));
    // Programme dependence: the three cells (attack 117/117/1000 ms, release 13/92/696 ms) and the
    // light-to-cell feedback make the recovery depend on the history, not on a time constant. (The
    // onset overshoots: 0.1 s into a hold the reduction is deeper than its steady state.)
    let short = gain_trace(Mode::Optical, &[(idx::THRESHOLD, -30.0)], &[(-6.0, 0.1), (-60.0, 6.0)]);
    let long = gain_trace(Mode::Optical, &[(idx::THRESHOLD, -30.0)], &[(-6.0, 5.0), (-60.0, 6.0)]);
    let until = |g: &[f64], from: usize, db: f64| (from..g.len()).find(|&i| g[i] > db).unwrap() as f64 - from as f64;
    let (fs, fl) = ((0.1 * SR) as usize, (5.0 * SR) as usize);
    let ms = 1000.0 / SR as f64;
    let (rs, rl) = (until(&short, fs, -0.5) * ms, until(&long, fl, -0.5) * ms);
    println!(
        "Optical: reduction at release {:.1} / {:.1} dB (0.1 s / 5 s hold); to -10 dB {:.0} / {:.0} ms; to -0.5 dB {rs:.0} / {rl:.0} ms",
        short[fs - 1], long[fl - 1], until(&short, fs, -10.0) * ms, until(&long, fl, -10.0) * ms
    );
    assert!((rs - rl).abs() > 0.2 * rs.max(rl));
    assert!(short[fs - 1] < long[fl - 1] - 6.0);
}

#[test]
fn rms_vca_releases_at_its_log_cap_rate() {
    // Detector level falls at 126 dB/s; the gain recovers at slope * 3/2.98 times that.
    let g = gain_trace(Mode::RmsVca, &[(idx::THRESHOLD, -60.0), (idx::RATIO, 4.0)], &[(0.0, 1.0), (-200.0, 0.4)]);
    // input is 1e-10 after the step, so the gain is in dB of (out / in) up to the floor; measure 0.05..0.25 s.
    let i0 = (1.05 * SR) as usize;
    let i1 = (1.25 * SR) as usize;
    let rate = (g[i1] - g[i0]) / 0.2;
    let expect = 126.0 * 0.75 * 3.0 / 2.98;
    println!("RMS VCA gain recovery {rate:.2} dB/s, expected {expect:.2}");
    assert!((rate - expect).abs() < 0.01 * expect);
    // Attack and release controls are not read.
    let a = gain_trace(Mode::RmsVca, &[(idx::ATTACK, 1.0)], &[(0.0, 0.2)]);
    let b = gain_trace(Mode::RmsVca, &[(idx::ATTACK, 300.0), (idx::RELEASE, 5.0)], &[(0.0, 0.2)]);
    assert!(a == b);
}
