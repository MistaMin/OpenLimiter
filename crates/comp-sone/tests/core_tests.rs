//! Port of the DSP checks in DualSone's `tests/CoreTests.cpp` (the meter-ballistics and readout
//! checks are left out: those belong to the editor's meters, which this module does not include).
//! Each test prints the figure the original prints, so `cargo test -- --nocapture` gives the numbers.
//! The noise is this file's own Gaussian generator (not std::mt19937), so the figures are close to,
//! not equal to, the original's `dsp/evidence/core_tests.txt`; the pass criteria are the original's.

use comp_sone::engine::{gain_computer, Core, KWeighting, Mode, Settings, Trace};
use comp_sone::{idx, DualSone};
use olm_api::{Kind, Module};

const FS: f64 = 48000.0;
const PI: f64 = std::f64::consts::PI;
const ALL_MODES: [Mode; 4] = [Mode::Parallel, Mode::Series, Mode::FastLed, Mode::Mix];

fn report(ok: bool, name: &str, detail: String) {
    println!("{}  {}  -- {}", if ok { "PASS" } else { "FAIL" }, name, detail);
    assert!(ok, "{name}: {detail}");
}

#[derive(Clone)]
struct Signal {
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Signal {
    fn new(n: usize) -> Self {
        Self { l: vec![0.0; n], r: vec![0.0; n] }
    }
    fn len(&self) -> usize {
        self.l.len()
    }
}

#[derive(Default)]
struct Run {
    l: Vec<f32>,
    r: Vec<f32>,
    g_s: Vec<f32>,
    g_f: Vec<f32>,
    l_slow: Vec<f32>,
    l_fast: Vec<f32>,
    reference: Vec<f32>,
    t_f: Vec<f32>,
    latency: usize,
}

fn quiet() -> Settings {
    Settings { slow_on: false, fast_on: false, ..Settings::default() }
}

fn run_with(input: &Signal, s: &Settings, block: usize, mut before: impl FnMut(&mut Core, usize)) -> Run {
    let mut core = Core::new();
    core.set_settings(s);
    core.prepare(FS);
    let n = input.len();
    let mut r = Run {
        l: input.l.clone(),
        r: input.r.clone(),
        g_s: vec![0.0; n],
        g_f: vec![0.0; n],
        l_slow: vec![0.0; n],
        l_fast: vec![0.0; n],
        reference: vec![0.0; n],
        t_f: vec![0.0; n],
        latency: core.latency_samples() as usize,
    };
    let mut start = 0;
    while start < n {
        before(&mut core, start);
        let end = (start + block).min(n);
        let mut t = Trace {
            slow_gain_db: Some(&mut r.g_s[start..end]),
            fast_gain_db: Some(&mut r.g_f[start..end]),
            slow_loudness: Some(&mut r.l_slow[start..end]),
            fast_loudness: Some(&mut r.l_fast[start..end]),
            reference: Some(&mut r.reference[start..end]),
            fast_target_db: Some(&mut r.t_f[start..end]),
            slow_engine_loudness: None,
        };
        core.process(&mut r.l[start..end], &mut r.r[start..end], Some(&mut t));
        start = end;
    }
    r
}

fn run(input: &Signal, s: &Settings) -> Run {
    run_with(input, s, 256, |_, _| {})
}

fn sine(n: usize, freq: f64, dbfs: f64) -> Signal {
    let mut s = Signal::new(n);
    let a = 10f64.powf(dbfs / 20.0);
    for i in 0..n {
        let v = (a * (2.0 * PI * freq * i as f64 / FS).sin()) as f32;
        s.l[i] = v;
        s.r[i] = v;
    }
    s
}

struct Rng(u64);
impl Rng {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let (u, v) = (self.uniform(), self.uniform());
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
}

fn noise(n: usize, dbfs: f64, seed: u64) -> Signal {
    let mut s = Signal::new(n);
    let mut rng = Rng(0x2545F4914F6CDD1D ^ seed.wrapping_mul(0x9E3779B97F4A7C15));
    let sd = 10f64.powf(dbfs / 20.0);
    for i in 0..n {
        s.l[i] = (rng.gauss() * sd) as f32;
        s.r[i] = (rng.gauss() * sd) as f32;
    }
    s
}

fn add_burst(s: &mut Signal, start: usize, len: usize, freq: f64, dbfs: f64) {
    let a = 10f64.powf(dbfs / 20.0);
    for i in start..(start + len).min(s.len()) {
        let v = (a * (2.0 * PI * freq * (i - start) as f64 / FS).sin()) as f32;
        s.l[i] += v;
        s.r[i] += v;
    }
}

fn scale(s: &mut Signal, from: usize, to: usize, k: f32) {
    for i in from..to.min(s.len()) {
        s.l[i] *= k;
        s.r[i] *= k;
    }
}

fn mean(v: &[f32], a: usize, b: usize) -> f64 {
    v[a..b].iter().map(|&x| x as f64).sum::<f64>() / (b - a) as f64
}

#[derive(Default, Clone, Copy)]
struct Dip {
    first: usize,
    last: usize,
    min_at: usize,
    depth: f32,
}

fn find_dip_below(g: &[f32], below: f32) -> Dip {
    let mut d = Dip::default();
    let mut found = false;
    for (i, &v) in g.iter().enumerate() {
        if v < below {
            if !found {
                d.first = i;
            }
            d.last = i;
            found = true;
        }
        if v < d.depth {
            d.depth = v;
            d.min_at = i;
        }
    }
    d
}

fn find_dip(g: &[f32]) -> Dip {
    find_dip_below(g, -0.01)
}

fn burst_settings(attack_ms: f32, release_ms: f32, lookahead: f32, k_weighted: bool) -> Settings {
    Settings {
        k_weighted,
        fast_on: true,
        threshold_db: -60.0,
        fast_crest_db: 6.0,
        fast_ratio: 8.0,
        fast_attack_ms: attack_ms,
        fast_release_ms: release_ms,
        lookahead,
        ..quiet()
    }
}

const BURST_AT: usize = 20000;

fn burst_signal() -> Signal {
    let mut s = sine(48000, 220.0, -50.0);
    add_burst(&mut s, BURST_AT, 96, 1000.0, -6.0);
    s
}

fn lround(x: f64) -> i64 {
    x.round() as i64
}

// THD+N of the output's last second (least squares on sin, cos, u sin, u cos).
fn thd_n(y: &[f32], f: f64) -> f64 {
    let b = y.len();
    let a = b - FS as usize;
    let mut m = [[0.0f64; 4]; 4];
    let mut v = [0.0f64; 4];
    let basis = |i: usize| {
        let ph = 2.0 * PI * f * i as f64 / FS;
        let u = 2.0 * (i - a) as f64 / (b - a) as f64 - 1.0;
        let (s, c) = (ph.sin(), ph.cos());
        [s, c, u * s, u * c]
    };
    for i in a..b {
        let e = basis(i);
        for r in 0..4 {
            v[r] += e[r] * y[i] as f64;
            for c in 0..4 {
                m[r][c] += e[r] * e[c];
            }
        }
    }
    for c in 0..4 {
        for r in c + 1..4 {
            let k = m[r][c] / m[c][c];
            for q in c..4 {
                m[r][q] -= k * m[c][q];
            }
            v[r] -= k * v[c];
        }
    }
    let mut x = [0.0f64; 4];
    for r in (0..4).rev() {
        let mut t = v[r];
        for q in r + 1..4 {
            t -= m[r][q] * x[q];
        }
        x[r] = t / m[r][r];
    }
    let (mut res, mut fund) = (0.0, 0.0);
    for i in a..b {
        let e = basis(i);
        let fit = x[0] * e[0] + x[1] * e[1] + x[2] * e[2] + x[3] * e[3];
        res += (y[i] as f64 - fit) * (y[i] as f64 - fit);
        fund += fit * fit;
    }
    10.0 * (res / fund + 1e-30).log10()
}

// BS.1770-4 integrated loudness of samples [a, b), independent of the core.
fn integrated_lufs(l: &[f32], r: &[f32], a: usize, b: usize) -> f64 {
    let mut kl = KWeighting::default();
    kl.prepare(FS);
    let mut kr = kl;
    let e: Vec<f64> = (a..b)
        .map(|i| {
            let x = kl.process(l[i] as f64);
            let y = kr.process(r[i] as f64);
            x * x + y * y
        })
        .collect();
    let block = (0.4 * FS) as usize;
    let hop = block / 4;
    let mut blocks = Vec::new();
    let mut s = (0.5 * FS) as usize;
    while s + block <= e.len() {
        blocks.push(e[s..s + block].iter().sum::<f64>() / block as f64);
        s += hop;
    }
    let lufs = |ms: f64| -0.691 + 10.0 * (ms + 1e-30).log10();
    let gated = |gate: f64| {
        let sel: Vec<f64> = blocks.iter().copied().filter(|&m| lufs(m) > gate).collect();
        if sel.is_empty() { 0.0 } else { sel.iter().sum::<f64>() / sel.len() as f64 }
    };
    lufs(gated(lufs(gated(-70.0)) - 10.0))
}

// ---------------------------------------------------------------- detector

#[test]
fn k_weighting_matches_bs1770_coefficients() {
    let mut k = KWeighting::default();
    k.prepare(48000.0);
    let ok = (k.shelf.b0 - 1.53512485958697).abs() < 1e-9
        && (k.shelf.b1 + 2.69169618940638).abs() < 1e-9
        && (k.shelf.b2 - 1.19839281085285).abs() < 1e-9
        && (k.shelf.a1 + 1.69065929318241).abs() < 1e-9
        && (k.shelf.a2 - 0.73248077421585).abs() < 1e-9
        && (k.high_pass.a1 + 1.99004745483398).abs() < 1e-9
        && (k.high_pass.a2 - 0.99007225036621).abs() < 1e-9;
    report(ok, "K-weighting matches BS.1770 coefficients at 48 kHz", String::new());
}

#[test]
fn ebu_3341_reads_minus_23() {
    let input = sine((FS * 4.0) as usize, 1000.0, -23.0);
    let mut s = quiet();
    let m = run(&input, &s);
    s.slow_window_seconds = 3.0;
    let st = run(&input, &s);
    let (lm, ls) = (*m.l_slow.last().unwrap(), *st.l_slow.last().unwrap());
    report(
        (lm + 23.0).abs() < 0.1 && (ls + 23.0).abs() < 0.1,
        "EBU 3341 -23 dBFS 1 kHz stereo reads -23.0 LUFS",
        format!("momentary {lm:.3}, short-term {ls:.3}"),
    );
}

// ---------------------------------------------------------------- audio path

#[test]
fn impulse_unchanged_delayed_by_latency() {
    let mut input = Signal::new(4096);
    input.l[100] = 1.0;
    input.r[100] = -0.5;
    let r = run(&input, &quiet());
    let mut exact = true;
    for i in 0..input.len() {
        let (el, er) = if i == 100 + r.latency { (1.0, -0.5) } else { (0.0, 0.0) };
        exact &= r.l[i] == el && r.r[i] == er;
    }
    let expect = (lround(0.030 * FS) | 1) + (lround(0.005 * FS) | 1) - 2 + lround(0.010 * FS);
    report(
        exact && r.latency as i64 == expect && r.latency == 2160,
        "impulse comes out unchanged, delayed by the reported latency",
        format!("latency {} samples ({:.2} ms)", r.latency, 1000.0 * r.latency as f64 / FS),
    );
}

#[test]
fn below_threshold_bit_exact_every_mode() {
    let input = noise(FS as usize * 2, -60.0, 1);
    let mut max_err = 0.0f64;
    for mode in ALL_MODES {
        let s = Settings { mode, threshold_db: -10.0, fast_crest_db: 24.0, ..Settings::default() };
        let r = run(&input, &s);
        for i in r.latency..input.len() {
            max_err = max_err.max((r.l[i] - input.l[i - r.latency]).abs() as f64).max((r.r[i] - input.r[i - r.latency]).abs() as f64);
        }
    }
    report(max_err == 0.0, "every mode, below threshold: output is a bit-exact delayed input", format!("max error {max_err:.3e}"));
}

#[test]
fn mix_zero_is_exact_dry() {
    let input = noise(FS as usize * 2, -6.0, 1);
    let s = Settings { threshold_db: -40.0, fast_crest_db: 0.0, mix: 0.0, ..Settings::default() };
    let r = run(&input, &s);
    let mut max_err = 0.0f64;
    for i in r.latency..input.len() {
        max_err = max_err.max((r.l[i] - input.l[i - r.latency]).abs() as f64);
    }
    let d = find_dip(&r.g_s).depth;
    report(
        max_err == 0.0 && d < -3.0,
        "mix 0 while compressing: output is the exact delayed dry signal",
        format!("max error {max_err:.3e}, slow GR {d:.1} dB"),
    );
}

#[test]
fn block_size_does_not_matter() {
    let mut input = noise(FS as usize * 3, -12.0, 1);
    add_burst(&mut input, 70000, 500, 2000.0, -1.0);
    let s = Settings::default();
    let a = run_with(&input, &s, 64, |_, _| {});
    let b = run_with(&input, &s, 1024, |_, _| {});
    report(a.l == b.l && a.r == b.r, "output identical with 64- and 1024-sample blocks (engine driven directly)", String::new());
}

// ---------------------------------------------------------------- fast engine envelope

#[test]
fn fast_envelope_symmetric_and_centred() {
    let burst = burst_signal();
    let centre_expected = BURST_AT as f64 + 47.5;
    let r = run(&burst, &burst_settings(10.0, 0.0, 1.0, false));
    let d = find_dip(&r.g_f);
    let centre = 0.5 * (d.first + d.last) as f64;
    let mut asym = 0.0f64;
    for k in 0..=(d.last - d.first) / 2 {
        asym = asym.max((r.g_f[d.first + k] - r.g_f[d.last - k]).abs() as f64);
    }
    report(asym < 0.01, "fast envelope is time-symmetric (release 0)", format!("max asymmetry {asym:.4} dB, depth {:.1} dB", d.depth));
    report(
        (centre - (centre_expected + r.latency as f64)).abs() <= 1.0,
        "fast envelope is centred on the delayed burst",
        format!("dip centre {centre:.1}, delayed burst centre {:.1}", centre_expected + r.latency as f64),
    );
}

#[test]
fn full_lookahead_never_late() {
    let burst = burst_signal();
    let w = (lround(0.005 * FS) | 1) as usize;
    let mut ok = true;
    let mut worst = -1e9f64;
    for mode in ALL_MODES {
        for release in [0.0, 100.0] {
            for attack in [0.5, 10.0, 30.0] {
                let mut st = burst_settings(attack, release, 1.0, false);
                st.mode = mode;
                let r = run(&burst, &st);
                let mut k = w;
                while k + r.latency < r.g_f.len() {
                    if r.t_f[k] <= -1e-3 {
                        for m in (k + 1 - w)..=k {
                            let late = (r.g_f[m + r.latency] - r.t_f[k]) as f64;
                            worst = worst.max(late);
                            ok &= late <= 1e-4;
                        }
                    }
                    k += 1;
                }
            }
        }
    }
    report(ok, "lookahead 100%, every mode: every sample of a loud window gets its gain reduction in time", format!("worst gain above target {worst:.2e} dB"));
}

#[test]
fn fast_attack_sets_ramp() {
    let burst = burst_signal();
    let ramp_of = |attack: f32| {
        let r = run(&burst, &burst_settings(attack, 0.0, 1.0, false));
        let d = find_dip(&r.g_f);
        let mut full = d.first;
        while r.g_f[full] > d.depth + 0.01 {
            full += 1;
        }
        ((full - d.first) as f64 / FS * 1000.0, r.latency)
    };
    let (a, b) = (ramp_of(2.0), ramp_of(20.0));
    report(
        ((b.0 - a.0) - 18.0).abs() < 1.8 && a.1 == b.1,
        "Fast Attack sets the ramp time, latency unchanged",
        format!("ramp {:.2} ms at 2 ms, {:.2} ms at 20 ms, latency {}", a.0, b.0, a.1),
    );
}

#[test]
fn lookahead_zero_lets_transient_through() {
    let burst = burst_signal();
    let a = run(&burst, &burst_settings(10.0, 0.0, 1.0, false));
    let b = run(&burst, &burst_settings(10.0, 0.0, 0.0, false));
    let shift = find_dip(&b.g_f).first as f64 - find_dip(&a.g_f).first as f64;
    let h = ((lround(0.010 * FS) | 1) + (lround(0.005 * FS) | 1) - 2 + lround(0.010 * FS)) as f64;
    let at_onset = b.g_f[BURST_AT + b.latency];
    report(
        (shift - h).abs() <= 1.0 && at_onset > -0.05,
        "Lookahead 0% lets the transient through before the gain falls",
        format!("shift {shift:.0} samples (expected {h:.0}), gain at burst onset {at_onset:.3} dB"),
    );
}

#[test]
fn fast_release_sets_recovery() {
    let burst = burst_signal();
    let recovery = |release: f32| {
        let r = run(&burst, &burst_settings(2.0, release, 1.0, false));
        let d = find_dip(&r.g_f);
        let mut i = d.min_at;
        while i < r.g_f.len() && r.g_f[i] < d.depth * 0.1 {
            i += 1;
        }
        (i - d.min_at) as f64 / FS * 1000.0
    };
    let (a, b) = (recovery(20.0), recovery(200.0));
    report(b > 5.0 * a, "Fast Release sets the recovery time", format!("90% recovery {a:.1} ms at 20 ms, {b:.1} ms at 200 ms"));
}

#[test]
fn attack_lookahead_automation_does_not_jump() {
    let mut input = noise(FS as usize * 2, -10.0, 1);
    for k in 0..40 {
        add_burst(&mut input, 2000 + k * 2300, 200, 1500.0, -1.0);
    }
    let s = burst_settings(2.0, 50.0, 1.0, false);
    let r = run_with(&input, &s, 256, |core, start| {
        let mut t = s;
        t.fast_attack_ms = if (start / 256) % 2 == 0 { 2.0 } else { 30.0 };
        t.lookahead = if (start / 512) % 2 == 0 { 1.0 } else { 0.0 };
        core.set_settings(&t);
    });
    let steady = run(&input, &s);
    let (mut max_step, mut steady_step) = (0.0f64, 0.0f64);
    for i in 1..r.g_f.len() {
        max_step = max_step.max((r.g_f[i] - r.g_f[i - 1]).abs() as f64);
        steady_step = steady_step.max((steady.g_f[i] - steady.g_f[i - 1]).abs() as f64);
    }
    report(
        max_step < 2.0 * steady_step + 0.05,
        "attack/lookahead automation does not jump the gain",
        format!("max step {max_step:.3} dB/sample (static settings {steady_step:.3})"),
    );
}

// ---------------------------------------------------------------- slow engine

fn level_step(seconds: usize) -> Signal {
    let mut input = noise(FS as usize * seconds, -40.0, 1);
    let n = input.len();
    scale(&mut input, FS as usize * 2, n, 30.0);
    input
}

fn half_time(r: &Run) -> f64 {
    let fin = *r.g_s.last().unwrap();
    let mut i = FS as usize * 2;
    while r.g_s[i] > 0.5 * fin {
        i += 1;
    }
    (i - FS as usize * 2) as f64 / FS * 1000.0
}

#[test]
fn slow_windows_order() {
    let input = level_step(10);
    let reach = |w: f32| {
        let s = Settings { slow_on: true, threshold_db: -30.0, slow_attack_ms: 50.0, slow_window_seconds: w, ..quiet() };
        half_time(&run(&input, &s))
    };
    let (a, b, c) = (reach(0.2), reach(0.4), reach(3.0));
    report(
        a < b - 10.0 && b < c - 50.0,
        "slow windows: 200 ms reacts before 400 ms, 400 ms before 3 s",
        format!("50% of the GR after {a:.0} / {b:.0} / {c:.0} ms"),
    );
}

#[test]
fn slow_attack_sets_pull_down() {
    let input = level_step(12);
    let ht = |attack: f32| {
        let s = Settings { slow_on: true, threshold_db: -30.0, slow_attack_ms: attack, ..quiet() };
        let r = run(&input, &s);
        (half_time(&r), *r.g_s.last().unwrap())
    };
    let (a, b) = (ht(100.0), ht(2000.0));
    report(
        b.0 > 3.0 * a.0 && a.1 < -3.0,
        "Slow Attack sets how fast the leveller pulls down",
        format!("50% at {:.0} ms (attack 100 ms), {:.0} ms (attack 2 s); GR {:.1} dB", a.0, b.0, a.1),
    );
}

#[test]
fn leveller_holds_through_pause() {
    let mut input = noise(FS as usize * 16, -6.0, 1);
    let n = input.len();
    scale(&mut input, FS as usize * 6, FS as usize * 10, 0.0);
    scale(&mut input, FS as usize * 10, n, 0.1);
    let s = Settings { slow_on: true, threshold_db: -30.0, slow_release_ms: 500.0, ..quiet() };
    let r = run(&input, &s);
    let (before, in_pause, after) = (r.g_s[FS as usize * 6 - 1], r.g_s[FS as usize * 10 - 1], *r.g_s.last().unwrap());
    report(
        (in_pause - before).abs() < 0.1 && after > before + 5.0,
        "leveller holds through a pause, then releases",
        format!("GR {before:.2} before, {in_pause:.2} end of pause, {after:.2} after"),
    );
}

// ---------------------------------------------------------------- engine independence

#[test]
fn level_step_moves_only_slow() {
    let mut input = noise(FS as usize * 16, -24.0, 1);
    let n = input.len();
    scale(&mut input, FS as usize * 8, n, 2.0);
    let s = Settings { threshold_db: -30.0, fast_crest_db: 6.0, ..Settings::default() };
    let r = run(&input, &s);
    let f = FS as usize;
    let (sa, sb) = (mean(&r.g_s, f * 6, f * 8), mean(&r.g_s, f * 14, f * 16));
    let (fa, fb) = (mean(&r.g_f, f * 6, f * 8), mean(&r.g_f, f * 14, f * 16));
    report(
        (sb - sa).abs() > 2.0 && (fb - fa).abs() < 0.25,
        "level step moves only the slow engine",
        format!("slow {:.2} dB, fast {:.2} dB", sb - sa, fb - fa),
    );
}

#[test]
fn crest_change_moves_only_fast() {
    let f = FS as usize;
    let steady = sine(f * 8, 1000.0, -20.0);
    let mut clicks = Signal::new(f * 8);
    let mut i = 0;
    while i < clicks.len() {
        add_burst(&mut clicks, i, 48, 1000.0, 0.0);
        i += 4800;
    }
    let q = quiet();
    let ls = *run(&steady, &q).l_slow.last().unwrap();
    let lc = *run(&clicks, &q).l_slow.last().unwrap();
    let up = 10f32.powf((ls - lc) / 20.0);
    let n = clicks.len();
    scale(&mut clicks, 0, n, up);
    let t = Settings { threshold_db: -30.0, fast_crest_db: 6.0, ..Settings::default() };
    let (a, b) = (run(&steady, &t), run(&clicks, &t));
    let slow_diff = mean(&b.g_s, f * 4, f * 8) - mean(&a.g_s, f * 4, f * 8);
    let (fa, fb) = (find_dip(&a.g_f[f * 2..]).depth, find_dip(&b.g_f[f * 2..]).depth);
    report(
        slow_diff.abs() < 1.0 && fb < -3.0 && fa > -0.5,
        "crest change at equal loudness moves only the fast engine",
        format!("slow diff {slow_diff:.2} dB, fast GR steady {fa:.2} / clicks {fb:.2} dB"),
    );
}

#[test]
fn k_weighted_envelope_offset_small() {
    let burst = burst_signal();
    let r = run(&burst, &burst_settings(10.0, 0.0, 1.0, true));
    let d = find_dip(&r.g_f);
    let half = find_dip_below(&r.g_f, 0.5 * d.depth);
    let offset = (0.5 * (half.first + half.last) as f64 - (BURST_AT as f64 + 47.5 + r.latency as f64)) / FS * 1000.0;
    report(
        offset.abs() < 0.5 && d.depth < -3.0,
        "K-weighted detector keeps the envelope within 0.5 ms of the burst",
        format!("offset {offset:.3} ms, depth {:.1} dB", d.depth),
    );
}

#[test]
fn entry_after_silence_not_crushed() {
    let f = FS as usize;
    let mut input = Signal::new(f * 3);
    let tone = sine(f * 2, 440.0, -14.0);
    input.l[f..].copy_from_slice(&tone.l);
    input.r[f..].copy_from_slice(&tone.r);
    let s = Settings { slow_on: false, ..Settings::default() };
    let d = find_dip(&run(&input, &s).g_f).depth;
    report(d > -1.0, "entry after silence is not over-compressed by the fast engine", format!("worst fast GR {d:.2} dB"));
}

#[test]
fn threshold_floors_fast_engine() {
    let f = FS as usize;
    let mut input = noise(f * 3, -60.0, 1);
    add_burst(&mut input, f * 2, 240, 1000.0, -30.0);
    let mut s = Settings { slow_on: false, ..Settings::default() };
    let above = find_dip(&run(&input, &s).g_f).depth;
    s.threshold_db = -50.0;
    let below = find_dip(&run(&input, &s).g_f).depth;
    report(
        above > -0.5 && below < -6.0,
        "transients under the threshold are not compressed, however quiet the passage",
        format!("fast GR {above:.2} dB (threshold -18), {below:.2} dB (threshold -50)"),
    );
}

#[test]
fn sparse_hits_not_squashed_harder() {
    let f = FS as usize;
    let hits = |every: f64| {
        let mut input = noise(f * 12, -40.0, 1);
        let mut t = 1.0;
        while t < 12.0 {
            add_burst(&mut input, (t * FS) as usize, 480, 80.0, -3.0);
            t += every;
        }
        let s = Settings { slow_on: false, threshold_db: -40.0, ..Settings::default() };
        find_dip(&run(&input, &s).g_f[f * 6..]).depth
    };
    let (dense, sparse) = (hits(0.4), hits(1.5));
    report(
        (sparse - dense).abs() < 1.5,
        "sparse hits are not squashed harder than dense ones",
        format!("fast GR every 0.4 s {dense:.2} dB, every 1.5 s {sparse:.2} dB"),
    );
}

// ---------------------------------------------------------------- modes

#[test]
fn series_slow_depends_on_fast() {
    let f = FS as usize;
    let mut input = noise(f * 12, -24.0, 1);
    let mut t = 0;
    while t < input.len() {
        add_burst(&mut input, t, 960, 120.0, -2.0);
        t += 4800;
    }
    let at = |mode: Mode, fast_on: bool| {
        let s = Settings { mode, fast_on, threshold_db: -30.0, fast_crest_db: 2.0, fast_ratio: 6.0, ..Settings::default() };
        let r = run(&input, &s);
        (mean(&r.g_s, f * 8, input.len()), mean(&r.g_f, f * 8, input.len()))
    };
    let (par, ser) = (at(Mode::Parallel, true), at(Mode::Series, true));
    let (par_off, ser_off) = (at(Mode::Parallel, false), at(Mode::Series, false));
    report(
        ser.0 > par.0 + 0.5 && (ser.1 - par.1).abs() < 0.01 && (ser_off.0 - par_off.0).abs() < 0.1,
        "Series: the slow engine depends on the fast engine's work",
        format!("slow GR parallel {:.2}, series {:.2} dB; fast off: {:.2} vs {:.2} dB", par.0, ser.0, par_off.0, ser_off.0),
    );
}

#[test]
fn fast_led_and_mix() {
    let f = FS as usize;
    let input = noise(f * 12, -12.0, 1);
    let at = |mode: Mode| {
        let s = Settings { mode, threshold_db: -30.0, ..Settings::default() };
        let r = run(&input, &s);
        (mean(&r.g_s, f * 8, input.len()), mean(&r.g_f, f * 8, input.len()))
    };
    let (par, led, mix) = (at(Mode::Parallel), at(Mode::FastLed), at(Mode::Mix));
    let share_led = led.1 / (led.0 + led.1);
    let share_par = par.1 / (par.0 + par.1);
    report(
        share_led > 0.5 && share_par < 0.05,
        "Fast-led: the fast engine does most of the work",
        format!("fast share parallel {:.0}%, fast-led {:.0}%", 100.0 * share_par, 100.0 * share_led),
    );
    let between = |v: f64, a: f64, b: f64| v > a.min(b) && v < a.max(b);
    report(
        between(mix.0, par.0, led.0) && between(mix.1, par.1, led.1),
        "Mix sits between Parallel and Fast-led for both engines",
        format!("slow {:.2} < {:.2} < {:.2} dB; fast {:.2} / {:.2} / {:.2} dB", led.0, mix.0, par.0, led.1, mix.1, par.1),
    );
}

#[test]
fn mode_switching_glides() {
    let mut input = noise(FS as usize * 6, -12.0, 1);
    let mut t = 0;
    while t < input.len() {
        add_burst(&mut input, t, 300, 900.0, -1.0);
        t += 3000;
    }
    let mut s = Settings { threshold_db: -30.0, ..Settings::default() };
    let step_of = |r: &Run| (1..r.l.len()).map(|i| ((r.g_s[i] + r.g_f[i]) - (r.g_s[i - 1] + r.g_f[i - 1])).abs() as f64).fold(0.0, f64::max);
    let mut static_step = 0.0f64;
    for mode in ALL_MODES {
        s.mode = mode;
        static_step = static_step.max(step_of(&run(&input, &s)));
    }
    let r = run_with(&input, &s, 256, |core, start| {
        let mut t = s;
        t.mode = ALL_MODES[(start / 2048) % 4];
        core.set_settings(&t);
    });
    let step = step_of(&r);
    report(
        step < 1.5 * static_step + 0.01,
        "switching modes while compressing does not jump the gain",
        format!("max step {step:.3} dB/sample (static modes {static_step:.3})"),
    );
}

#[test]
fn mode_crossfade_is_50_ms() {
    // A one-pole of 50 ms: after a switch, the old lane's weight is e^-1 of the way after 50 ms.
    let input = noise(FS as usize * 6, -12.0, 3);
    let base = Settings { threshold_db: -30.0, ..Settings::default() };
    let switch_at = FS as usize * 4;
    let a = run(&input, &base);
    let b = run(&input, &Settings { mode: Mode::FastLed, ..base });
    let r = run_with(&input, &base, 256, |core, start| {
        if start == switch_at {
            core.set_settings(&Settings { mode: Mode::FastLed, ..base });
        }
    });
    let frac = |i: usize| {
        let (ga, gb, g) = (a.g_s[i] + a.g_f[i], b.g_s[i] + b.g_f[i], r.g_s[i] + r.g_f[i]);
        ((g - gb) / (ga - gb)) as f64
    };
    let at50 = frac(switch_at + (0.05 * FS) as usize - 1);
    report(
        (at50 - (-1.0f64).exp()).abs() < 0.05,
        "mode switch is a 50 ms crossfade between lanes",
        format!("old-mode share 50 ms after the switch {at50:.3} (e^-1 = 0.368)"),
    );
}

// ---------------------------------------------------------------- artifacts

#[test]
fn steady_tones_undistorted() {
    let mut worst = -300.0f64;
    let mut at = String::new();
    for mode in ALL_MODES {
        for k in [true, false] {
            for attack in [0.5f32, 10.0] {
                for f in [20.0, 30.0, 50.0, 100.0, 1000.0, 5000.0, 15000.0] {
                    let input = sine(FS as usize * 12, f, -6.0);
                    let s = Settings {
                        mode,
                        k_weighted: k,
                        threshold_db: -30.0,
                        fast_crest_db: if matches!(mode, Mode::Parallel | Mode::Series) { 0.0 } else { 6.0 },
                        fast_ratio: 4.0,
                        fast_attack_ms: attack,
                        fast_release_ms: if attack < 1.0 { 5.0 } else { 100.0 },
                        ..Settings::default()
                    };
                    let d = thd_n(&run(&input, &s).l, f);
                    if d > worst {
                        worst = d;
                        at = format!("{f:.0} Hz, attack {attack:.1} ms, {mode:?}, {}", if k { "K" } else { "RMS" });
                    }
                }
            }
        }
    }
    report(
        worst < -100.0,
        "steady tones under compression: no distortion, 20 Hz - 15 kHz, all modes and settings",
        format!("worst THD+N {worst:.1} dB ({at})"),
    );
}

#[test]
fn hit_during_release_joins_smoothly() {
    let hf_share = |g_db: &[f32]| {
        let taps = 2001usize;
        let half = taps / 2;
        let mut h = vec![0.0f64; taps];
        let mut sum = 0.0;
        for i in 0..taps {
            let x = i as f64 - half as f64;
            let w = 0.42 - 0.5 * (2.0 * PI * i as f64 / (taps - 1) as f64).cos() + 0.08 * (4.0 * PI * i as f64 / (taps - 1) as f64).cos();
            h[i] = w * if x == 0.0 { 2.0 * 200.0 / FS } else { (2.0 * PI * 200.0 / FS * x).sin() / (PI * x) };
            sum += h[i];
        }
        let lin: Vec<f64> = g_db.iter().map(|&g| 10f64.powf(g as f64 / 20.0)).collect();
        let (a, b) = (FS as usize * 2, FS as usize * 4);
        let (mut high, mut total) = (0.0, 0.0);
        for n in a..b {
            let mut lp = 0.0;
            for i in 0..taps {
                lp += h[i] / sum * lin[n + i - half];
            }
            let g = lin[n];
            high += (g - lp) * (g - lp);
            total += g * g;
        }
        10.0 * (high / total).log10()
    };
    let mut input = Signal::new(FS as usize * 5);
    for i in 0..input.len() {
        let ph = i % 12000;
        let env = 0.1 + if ph < 1440 { 0.694 * 0.5 * (1.0 - (2.0 * PI * ph as f64 / 1440.0).cos()) } else { 0.0 };
        let v = (env * (2.0 * PI * 1000.0 * i as f64 / FS).sin()) as f32;
        input.l[i] = v;
        input.r[i] = v;
    }
    let with_release = |rel: f32| {
        let s = Settings {
            mode: Mode::FastLed,
            slow_on: false,
            threshold_db: -30.0,
            fast_crest_db: 0.0,
            fast_ratio: 6.0,
            fast_attack_ms: 2.0,
            fast_release_ms: rel,
            ..Settings::default()
        };
        hf_share(&run(&input, &s).g_f)
    };
    let (short, long) = (with_release(20.0), with_release(100.0));
    report(
        long < short + 3.0,
        "a hit during a release joins it smoothly (no corner splatter)",
        format!("gain energy above 200 Hz: release 20 ms {short:.1} dB, 100 ms {long:.1} dB"),
    );
}

#[test]
fn leveller_reaches_target_at_192k() {
    let rate = 192000.0;
    let mut worst = 0.0f64;
    for phase in 0..2 {
        let n = (rate * 36.0) as usize;
        let mut rng = Rng(1);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        for i in 0..n {
            let k = if phase == 1 && i < (rate * 4.0) as usize { 4.0 } else { 1.0 };
            l[i] = (rng.gauss() * 0.1 * k) as f32;
            r[i] = (rng.gauss() * 0.1 * k) as f32;
        }
        let s = Settings {
            fast_on: false,
            threshold_db: -40.0,
            slow_ratio: 4.0,
            slow_attack_ms: 2000.0,
            slow_release_ms: 4000.0,
            slow_window_seconds: 3.0,
            ..Settings::default()
        };
        let mut core = Core::new();
        core.set_settings(&s);
        core.prepare(rate);
        let (mut gs, mut ls) = (vec![0.0f32; n], vec![0.0f32; n]);
        let mut st = 0;
        while st < n {
            let e = (st + 512).min(n);
            let mut t = Trace { slow_gain_db: Some(&mut gs[st..e]), slow_loudness: Some(&mut ls[st..e]), ..Default::default() };
            core.process(&mut l[st..e], &mut r[st..e], Some(&mut t));
            st = e;
        }
        let mut e = 0.0f64;
        for i in n - rate as usize..n {
            e += (gs[i] - gain_computer(ls[i], s.threshold_db, s.slow_ratio, Core::SLOW_KNEE_DB)) as f64;
        }
        worst = worst.max((e / rate).abs());
    }
    report(worst < 0.03, "192 kHz, 2 s attack / 4 s release: the leveller reaches its target", format!("worst error {worst:.3} dB"));
}

#[test]
fn nan_inf_does_not_poison() {
    let mut input = noise(FS as usize * 4, -12.0, 1);
    input.l[48000] = f32::NAN;
    input.r[48001] = f32::INFINITY;
    let s = Settings { threshold_db: -30.0, ..Settings::default() };
    let r = run(&input, &s);
    let reference = run(&noise(FS as usize * 4, -12.0, 1), &s);
    let mut finite = true;
    for i in 48002 + r.latency..r.l.len() {
        finite &= r.l[i].is_finite() && r.r[i].is_finite() && r.g_s[i].is_finite() && r.g_f[i].is_finite();
    }
    let f = FS as usize;
    let diff = (mean(&r.g_s, f * 3, r.g_s.len()) - mean(&reference.g_s, f * 3, reference.g_s.len())).abs();
    report(
        finite && diff < 0.5,
        "a NaN/inf input sample does not poison the compressor",
        format!("output finite after it passes; slow GR differs from clean run by {diff:.2} dB"),
    );
}

#[test]
fn stereo_link_symmetric() {
    let mut input = noise(FS as usize * 2, -40.0, 1);
    for i in 30000..30200 {
        input.l[i] += 0.8 * (2.0 * PI * 1000.0 * i as f64 / FS).sin() as f32;
    }
    let swapped = Signal { l: input.r.clone(), r: input.l.clone() };
    let s = Settings::default();
    let (a, b) = (run(&input, &s), run(&swapped, &s));
    report(a.g_f == b.g_f && a.g_s == b.g_s && a.l == b.r && a.r == b.l, "stereo link is symmetric under a channel swap", String::new());
}

// ---------------------------------------------------------------- autogain and delta

fn compressing(mode: Mode) -> Settings {
    Settings { mode, threshold_db: -30.0, slow_ratio: 4.0, fast_crest_db: 3.0, ..Settings::default() }
}

fn sections(seconds: usize) -> Signal {
    let mut s = noise(FS as usize * seconds, -12.0, 7);
    for i in 0..s.len() {
        if (i / (4 * FS as usize)) % 2 == 1 {
            s.l[i] *= 0.3;
            s.r[i] *= 0.3;
        }
    }
    s
}

#[test]
fn autogain_matches_integrated_loudness() {
    let steady = noise(FS as usize * 20, -12.0, 7);
    let (from, to) = (6 * FS as usize, steady.len());
    let (mut worst_lu, mut worst_off) = (0.0f64, 0.0f64);
    let mut per = String::new();
    for m in ALL_MODES {
        let mut s = compressing(m);
        let off = run(&steady, &s);
        s.auto_gain = true;
        let on = run(&steady, &s);
        let lat = on.latency;
        let lin = integrated_lufs(&steady.l, &steady.r, from - lat, to - lat);
        let d = integrated_lufs(&on.l, &on.r, from, to) - lin;
        worst_lu = worst_lu.max(d.abs());
        worst_off = worst_off.max(lin - integrated_lufs(&off.l, &off.r, from, to));
        per += &format!("{m:?} {d:+.2} ");
    }
    report(
        worst_lu < 0.5 && worst_off > 10.0,
        "autogain: steady compressed output is as loud as the input (BS.1770 integrated), every mode",
        format!("{per}LU (without autogain up to {worst_off:.1} dB quieter)"),
    );
}

#[test]
fn autogain_keeps_section_contrast() {
    let input = sections(32);
    let f = FS as usize;
    let (from, to) = (8 * f, input.len());
    let (mut worst_c, mut worst_lu) = (0.0f64, 0.0f64);
    for m in ALL_MODES {
        let mut s = compressing(m);
        let off = run(&input, &s);
        s.auto_gain = true;
        let on = run(&input, &s);
        let lat = on.latency;
        let contrast = |r: &Run| {
            integrated_lufs(&r.l, &r.r, 17 * f + lat, 20 * f + lat) - integrated_lufs(&r.l, &r.r, 21 * f + lat, 24 * f + lat)
        };
        worst_c = worst_c.max((contrast(&on) - contrast(&off)).abs());
        worst_lu = worst_lu.max((integrated_lufs(&on.l, &on.r, from, to) - integrated_lufs(&input.l, &input.r, from - lat, to - lat)).abs());
    }
    report(
        worst_c < 1.0 && worst_lu < 1.0,
        "autogain on 4 s loud/quiet sections: keeps the leveller's contrast, matches integrated loudness",
        format!("contrast changed by at most {worst_c:.2} dB, integrated within {worst_lu:.2} LU"),
    );
}

#[test]
fn autogain_with_input_gain() {
    let input = sections(32);
    let f = FS as usize;
    let (from, to) = (8 * f, input.len());
    let s = Settings { auto_gain: true, input_db: 12.0, ..compressing(Mode::Parallel) };
    let mut core = Core::new();
    core.set_settings(&s);
    core.prepare(FS);
    let (mut l, mut r) = (input.l.clone(), input.r.clone());
    let mut st = 0;
    while st < l.len() {
        let e = (st + 256).min(l.len());
        core.apply_input_gain(&mut l[st..e], &mut r[st..e]);
        core.process(&mut l[st..e], &mut r[st..e], None);
        st = e;
    }
    let lat = core.latency_samples() as usize;
    let d = integrated_lufs(&l, &r, from, to) - integrated_lufs(&input.l, &input.r, from - lat, to - lat);
    report(d.abs() < 0.5, "autogain with +12 dB Input: output matches the loudness before Input", format!("{d:.2} LU"));
}

#[test]
fn autogain_nothing_compressed_bit_exact() {
    let s = Settings { auto_gain: true, ..quiet() };
    let input = noise(FS as usize * 3, -20.0, 3);
    let r = run(&input, &s);
    let mut max_err = 0.0f64;
    for i in r.latency..input.len() {
        max_err = max_err.max((r.l[i] - input.l[i - r.latency]).abs() as f64);
    }
    report(max_err == 0.0, "autogain on, nothing compressed: output is still a bit-exact delayed input", format!("max error {max_err:.3e}"));
}

#[test]
fn autogain_holds_through_pause() {
    let s = Settings { auto_gain: true, ..compressing(Mode::Parallel) };
    let mut input = noise(FS as usize * 20, -12.0, 9);
    scale(&mut input, 10 * FS as usize, 15 * FS as usize, 0.0);
    let mut core = Core::new();
    core.set_settings(&s);
    core.prepare(FS);
    let mut ag = Vec::new();
    let mut st = 0;
    while st < input.len() {
        let e = (st + 480).min(input.len());
        core.process(&mut input.l[st..e], &mut input.r[st..e], None);
        ag.push(core.meter_auto_gain_db);
        st = e;
    }
    let at = |t: f64| ag[(t * FS / 480.0) as usize];
    let (before, gap, after) = (at(9.9), at(14.9), at(16.0));
    report(
        before > 3.0 && (gap - before).abs() < 0.1 && (after - before).abs() < 0.5,
        "autogain holds through a 5 s pause (no swell in the silence)",
        format!("{before:.2} dB before, {gap:.2} in the gap, {after:.2} after"),
    );
}

#[test]
fn autogain_follows_threshold_change() {
    let s = Settings { auto_gain: true, ..compressing(Mode::Parallel) };
    let steady = noise(FS as usize * 30, -12.0, 11);
    let (mut before, mut after) = (0.0f32, 0.0f32);
    let r = run_with(&steady, &s, 480, |core, start| {
        if start == (20.0 * FS) as usize {
            before = core.meter_auto_gain_db;
            core.set_settings(&Settings { threshold_db: -20.0, ..s });
        }
        if start == (26.0 * FS) as usize {
            after = core.meter_auto_gain_db;
        }
    });
    let reduction = |t: f64| {
        let (a0, a1) = (((t - 0.5) * FS) as usize, (t * FS) as usize);
        -(a0..a1).map(|i| (r.g_s[i] + r.g_f[i]) as f64).sum::<f64>() / (a1 - a0) as f64
    };
    let (was, now) = (reduction(20.0), reduction(26.0));
    report(
        (before as f64 - was).abs() < 1.0 && was - now > 2.0 && (after as f64 - now).abs() < 1.0,
        "autogain follows a threshold change: within 1 dB of the reduction 6 s later",
        format!("reduction {was:.2} -> {now:.2} dB, make-up {before:.2} -> {after:.2} dB"),
    );
}

#[test]
fn delta_nulls_and_trims() {
    let input = sections(32);
    let mut s = Settings { mix: 0.7, ..compressing(Mode::Parallel) };
    let wet = run(&input, &s);
    s.delta = true;
    let delta = run(&input, &s);
    s.output_db = 6.0;
    s.auto_gain = true;
    let trimmed = run(&input, &s);
    let lat = wet.latency;
    let (mut null_err, mut trim_err, mut peak) = (0.0f64, 0.0f64, 0.0f64);
    for i in lat + FS as usize..input.len() {
        null_err = null_err.max((wet.l[i] + delta.l[i] - input.l[i - lat]).abs() as f64);
        trim_err = trim_err.max((trimmed.l[i] - delta.l[i]).abs() as f64);
        peak = peak.max(delta.l[i].abs() as f64);
    }
    let silent = run(&noise(FS as usize, -20.0, 5), &Settings { delta: true, ..quiet() });
    let silent_peak = silent.l.iter().fold(0.0f64, |m, &v| m.max(v.abs() as f64));
    report(
        null_err < 1.0e-6 && peak > 0.05 && trim_err < 1.0e-6 && silent_peak == 0.0,
        "delta: compressed + delta = delayed input; silent when nothing is compressed; Output and Autogain stay out",
        format!("null {null_err:.2e}, peak {peak:.3}, trim leak {trim_err:.2e}"),
    );

    let d = Settings { input_db: 10.0, delta: true, ..compressing(Mode::Series) };
    // Input gain is applied by the caller (as the plugin does) — here, as in the original test, it is not,
    // so the two runs see the same audio and differ only in Delta's trim.
    let driven = run(&input, &d);
    let driven_ag = run(&input, &Settings { auto_gain: true, ..d });
    let back = 10f32.powf(-10.0 / 20.0);
    let (mut err, mut dpeak) = (0.0f64, 0.0f64);
    for i in lat + FS as usize..input.len() {
        err = err.max((driven_ag.l[i] - driven.l[i] * back).abs() as f64);
        dpeak = dpeak.max(driven.l[i].abs() as f64);
    }
    report(
        err < 1.0e-6 && dpeak > 0.05,
        "delta with Autogain gives back the Input gain",
        format!("error {err:.2e} against delta x -10 dB (peak {dpeak:.3})"),
    );
}

#[test]
fn autogain_delta_toggles_glide() {
    let s = compressing(Mode::Parallel);
    let tone = sine(FS as usize * 6, 100.0, -12.0);
    let f = FS as usize;
    let r = run_with(&tone, &s, 256, |core, start| {
        let t = Settings { auto_gain: start >= 2 * f, delta: start >= 4 * f && start < 5 * f, ..s };
        core.set_settings(&t);
    });
    let max_step = (f..r.l.len()).map(|i| (r.l[i] - r.l[i - 1]).abs()).fold(0.0f32, f32::max);
    report(max_step < 0.012, "switching Autogain and Delta mid-stream glides without a click", format!("largest sample step {max_step:.4}"));
}

// ---------------------------------------------------------------- the module (not in the original tests)

#[test]
fn module_parameters_match_params_h() {
    let m = DualSone::default();
    let p = m.params();
    let expect: [(&str, &str, f32, f32, f32); 19] = [
        ("detector", "Detector", 0.0, 1.0, 0.0),
        ("mode", "Mode", 0.0, 3.0, 0.0),
        ("threshold", "Threshold", -40.0, 0.0, -18.0),
        ("mix", "Mix", 0.0, 100.0, 100.0),
        ("output", "Output", -24.0, 24.0, 0.0),
        ("slowOn", "Slow On", 0.0, 1.0, 1.0),
        ("slowRatio", "Slow Ratio", 1.0, 10.0, 2.0),
        ("slowAttack", "Slow Attack", 50.0, 5000.0, 800.0),
        ("slowRelease", "Slow Release", 100.0, 10000.0, 2500.0),
        ("slowWindow", "Slow Window", 0.0, 2.0, 1.0),
        ("fastOn", "Fast On", 0.0, 1.0, 1.0),
        ("fastCrest", "Fast Crest", 0.0, 24.0, 6.0),
        ("fastRatio", "Fast Ratio", 1.0, 20.0, 3.0),
        ("fastAttack", "Fast Attack", 0.5, 30.0, 10.0),
        ("fastRelease", "Fast Release", 5.0, 1000.0, 100.0),
        ("fastLookahead", "Fast Lookahead", 0.0, 100.0, 100.0),
        ("input", "Input", -24.0, 24.0, 0.0),
        ("autoGain", "Autogain", 0.0, 1.0, 0.0),
        ("delta", "Delta", 0.0, 1.0, 0.0),
    ];
    assert_eq!(p.len(), expect.len());
    for (d, e) in p.iter().zip(expect.iter()) {
        assert_eq!((d.id, d.name, d.min, d.max, d.default), *e);
        assert_eq!(m.param(DualSone::index_of(d.id).unwrap()), d.default);
    }
    assert!(matches!(p[idx::MODE].kind, Kind::Choice(l) if l == ["Parallel", "Series", "Fast-led", "Mix"]));
    assert!(matches!(p[idx::SLOW_WINDOW].kind, Kind::Choice(l) if l == ["200 ms", "400 ms", "3 s"]));
    // Defaults give the engine's default settings.
    let v: [f32; idx::COUNT] = std::array::from_fn(|i| p[i].default);
    assert_eq!(comp_sone::to_settings(&v), Settings::default());
    println!("PASS  module parameters: ids, names, ranges and defaults as Params.h");
}

#[test]
fn module_latency_and_lifecycle() {
    let mut m = DualSone::default();
    let (mut l, mut r) = (vec![0.5f32; 64], vec![0.5f32; 64]);
    m.process(&mut l, &mut r); // before prepare: untouched, no panic
    assert_eq!(l[0], 0.5);
    for (fs, lat) in [(44100.0, 1983usize), (48000.0, 2160), (96000.0, 4320), (192000.0, 8640)] {
        m.prepare(fs, 512);
        assert_eq!(m.latency(), lat, "latency at {fs}");
    }
    m.prepare(48000.0, 512);
    // set_param: clamped, snapped where the original snaps, unknown index and NaN ignored.
    m.set_param(idx::THRESHOLD, -99.0);
    assert_eq!(m.param(idx::THRESHOLD), -40.0);
    m.set_param(idx::THRESHOLD, -20.4);
    assert_eq!(m.param(idx::THRESHOLD), -20.0);
    m.set_param(idx::MIX, 37.5);
    assert_eq!(m.param(idx::MIX), 37.5);
    m.set_param(idx::FAST_ATTACK, 0.1);
    assert_eq!(m.param(idx::FAST_ATTACK), 0.5);
    m.set_param(idx::MODE, 2.6);
    assert_eq!(m.param(idx::MODE), 3.0);
    m.set_param(999, 1.0);
    m.set_param(idx::OUTPUT, f32::NAN);
    assert_eq!(m.param(idx::OUTPUT), 0.0);
    assert_eq!(m.latency(), 2160, "latency does not depend on parameters");
    println!("PASS  module: latency per rate, process before prepare, set_param clamping");
}

#[test]
fn module_impulse_and_meters() {
    let mut m = DualSone::default();
    m.prepare(48000.0, 512);
    m.set_param(idx::SLOW_ON, 0.0);
    m.set_param(idx::FAST_ON, 0.0);
    let n = 4096;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    l[100] = 1.0;
    r[100] = -0.5;
    for k in 0..n / 512 {
        m.process(&mut l[k * 512..(k + 1) * 512], &mut r[k * 512..(k + 1) * 512]);
    }
    assert_eq!(l[100 + 2160], 1.0);
    assert_eq!(r[100 + 2160], -0.5);
    assert_eq!(l.iter().filter(|&&v| v != 0.0).count(), 1);

    // Loud noise: both engines reduce; total reduction covers each, and is reported positive.
    let mut m = DualSone::default();
    m.prepare(48000.0, 480);
    m.set_param(idx::THRESHOLD, -30.0);
    m.set_param(idx::FAST_CREST, 0.0);
    let mut sig = noise(48000 * 6, -12.0, 2);
    let mut t = 0;
    while t < sig.len() {
        add_burst(&mut sig, t, 240, 1000.0, -1.0);
        t += 12000;
    }
    let (mut worst_total, mut worst_slow, mut worst_fast) = (0.0f32, 0.0f32, 0.0f32);
    let mut ok = true;
    for k in 0..sig.len() / 480 {
        let (a, b) = (k * 480, (k + 1) * 480);
        m.process(&mut sig.l[a..b], &mut sig.r[a..b]);
        let mt = m.meters();
        ok &= mt.reduction_db >= 0.0 && mt.band_reduction_db[0] >= 0.0 && mt.band_reduction_db[1] >= 0.0;
        ok &= mt.reduction_db + 1e-4 >= mt.band_reduction_db[0].max(mt.band_reduction_db[1]);
        ok &= mt.band_reduction_db[2..].iter().all(|&v| v == 0.0);
        worst_total = worst_total.max(mt.reduction_db);
        worst_slow = worst_slow.max(mt.band_reduction_db[0]);
        worst_fast = worst_fast.max(mt.band_reduction_db[1]);
    }
    report(
        ok && worst_slow > 3.0 && worst_fast > 1.0,
        "module meters: total, slow and fast reduction, positive dB",
        format!("deepest total {worst_total:.1}, slow {worst_slow:.1}, fast {worst_fast:.1} dB"),
    );
}
