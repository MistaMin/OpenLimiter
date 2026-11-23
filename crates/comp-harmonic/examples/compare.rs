//! Compares the Rust port with the original C++ headers driven by `harness/harness.cpp`.
//!
//! Usage: compare <harness binary> <work dir>
//! Runs every case through both, sample by sample, and prints the largest differences.

use comp_harmonic::harmonics::Harmonics;
use comp_harmonic::modes::{CompParams, Mode, Modes};
use comp_harmonic::{idx, HarmonicCompressor};
use olm_api::Module;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

#[derive(Clone, Copy)]
enum Sig {
    ToneSteps,
    Bursts,
    Noise,
    Programme,
    Hot,
    Silence,
}

fn signal(kind: Sig, sr: f64, frames: usize, seed: u32) -> Vec<f32> {
    let mut rng = Rng(seed);
    let mut v = vec![0.0f32; frames * 2];
    let tau = 2.0 * std::f64::consts::PI;
    for i in 0..frames {
        let t = i as f64 / sr;
        let (a, b) = match kind {
            Sig::ToneSteps => {
                let step = (t / 0.25) as usize % 6;
                let level = [0.02, 0.1, 0.5, 0.9, 0.3, 0.005][step];
                let s = (tau * 997.0 * t).sin() * level;
                (s, (tau * 1499.0 * t).sin() * level * 0.7)
            }
            Sig::Bursts => {
                let on = (t * 4.0).fract() < 0.12;
                let env = if on { 0.95 } else { 0.03 };
                let s = (tau * 220.0 * t).sin() * env + rng.next() as f64 * env * 0.3;
                (s, s * 0.8 + (tau * 330.0 * t).sin() * 0.05)
            }
            Sig::Noise => {
                let seg = (t / 0.4) as usize % 5;
                let level = [0.3, 0.0, 0.8, 0.05, 0.0][seg];
                (rng.next() as f64 * level, rng.next() as f64 * level)
            }
            Sig::Programme => {
                let env = 0.5 + 0.45 * (tau * 1.7 * t).sin();
                let kick = if (t * 2.0).fract() < 0.05 { (tau * 60.0 * t).sin() * 0.9 } else { 0.0 };
                let s = env * (0.3 * (tau * 110.0 * t).sin() + 0.2 * (tau * 1760.0 * t).sin()) + kick + 0.05 * rng.next() as f64;
                let r = env * (0.3 * (tau * 165.0 * t).sin() + 0.15 * (tau * 2500.0 * t).sin()) + kick * 0.8 + 0.05 * rng.next() as f64;
                (s, r)
            }
            Sig::Hot => {
                let s = (tau * 3100.0 * t).sin() * 1.6 + 0.4 * rng.next() as f64;
                (s, (tau * 7300.0 * t).sin() * 1.2)
            }
            Sig::Silence => (0.0, 0.0),
        };
        v[2 * i] = a as f32;
        v[2 * i + 1] = b as f32;
    }
    v
}

fn write_f32(path: &Path, v: &[f32]) {
    let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    std::fs::write(path, bytes).unwrap();
}

fn read_f32(path: &Path) -> Vec<f32> {
    std::fs::read(path).unwrap().chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

struct Case {
    name: String,
    cmd: &'static str,
    sr: f64,
    block: usize,
    frames: usize,
    input: Vec<f32>,
    key: Option<Vec<f32>>,
    /// (block, index, requested value)
    sets: Vec<(usize, usize, f32)>,
}

#[derive(Default)]
struct Diff {
    max_audio: f32,
    differing: usize,
    total: usize,
    max_meter: f32,
}

/// Snapped plain values, as the module stores them, so both sides use identical floats.
fn snapped(sets: &[(usize, usize, f32)]) -> Vec<(usize, usize, f32)> {
    let mut m = HarmonicCompressor::default();
    let mut out = Vec::new();
    for &(b, i, v) in sets {
        m.set_param(i, v);
        out.push((b, i, m.param(i)));
    }
    out
}

fn run_cpp(harness: &Path, dir: &Path, c: &Case, sets: &[(usize, usize, f32)]) -> (Vec<f32>, Vec<f32>) {
    let inp = dir.join("in.f32");
    let key = dir.join("key.f32");
    let out = dir.join("out.f32");
    let meter = dir.join("meter.f32");
    write_f32(&inp, &c.input);
    if let Some(k) = &c.key {
        write_f32(&key, k);
    }
    let mut script = format!(
        "{} {} {} {} {} {} {} {} {} {}\n",
        c.cmd,
        c.sr,
        c.block,
        c.frames,
        c.key.is_some() as i32,
        inp.display(),
        key.display(),
        out.display(),
        meter.display(),
        sets.len()
    );
    for &(b, i, v) in sets {
        script.push_str(&format!("{} {} {:e}\n", b, i, v));
    }
    // Paths with spaces would break the token reader, so the work dir must not contain any.
    let mut child = Command::new(harness).stdin(Stdio::piped()).spawn().expect("harness");
    child.stdin.as_mut().unwrap().write_all(script.as_bytes()).unwrap();
    let st = child.wait().unwrap();
    assert!(st.success(), "harness failed for {}", c.name);
    (read_f32(&out), read_f32(&meter))
}

fn comp_params(v: &[f32; idx::COUNT]) -> CompParams {
    CompParams {
        threshold_db: v[idx::THRESHOLD],
        ratio: v[idx::RATIO],
        attack_ms: v[idx::ATTACK],
        release_ms: v[idx::RELEASE],
        knee_db: v[idx::KNEE],
        makeup_db: v[idx::MAKEUP],
        mix: v[idx::MIX] / 100.0,
        auto_release: v[idx::AUTO_RELEASE] > 0.5,
        auto_attack: v[idx::AUTO_ATTACK] > 0.5,
    }
}

fn run_rust(c: &Case, sets: &[(usize, usize, f32)]) -> (Vec<f32>, Vec<f32>) {
    let n_blocks = c.frames.div_ceil(c.block);
    let mut next = 0;
    let mut out: Vec<f32>;
    let mut meter = Vec::new();
    match c.cmd {
        "chain" => {
            let mut m = HarmonicCompressor::default();
            while next < sets.len() && sets[next].0 == 0 {
                m.set_param(sets[next].1, sets[next].2);
                next += 1;
            }
            m.prepare(c.sr as f32, c.block);
            out = c.input.clone();
            let mut l = vec![0.0f32; c.block];
            let mut r = vec![0.0f32; c.block];
            let mut kl = vec![0.0f32; c.block];
            let mut kr = vec![0.0f32; c.block];
            let mut vbuf = [0.0f32; comp_harmonic::view::LEN];
            for b in 0..n_blocks {
                while next < sets.len() && sets[next].0 <= b {
                    m.set_param(sets[next].1, sets[next].2);
                    next += 1;
                }
                let start = b * c.block;
                let n = c.block.min(c.frames - start);
                for s in 0..n {
                    l[s] = c.input[2 * (start + s)];
                    r[s] = c.input[2 * (start + s) + 1];
                    if let Some(k) = &c.key {
                        kl[s] = k[2 * (start + s)];
                        kr[s] = k[2 * (start + s) + 1];
                    }
                }
                if c.key.is_some() {
                    m.process_keyed(&mut l[..n], &mut r[..n], &kl[..n], &kr[..n]);
                } else {
                    m.process(&mut l[..n], &mut r[..n]);
                }
                for s in 0..n {
                    out[2 * (start + s)] = l[s];
                    out[2 * (start + s) + 1] = r[s];
                }
                m.view(&mut vbuf);
                meter.push(m.meters().reduction_db.max(0.0));
                meter.push(vbuf[comp_harmonic::view::GAIN_REDUCTION_DB]);
                meter.push(vbuf[comp_harmonic::view::AUTO_GAIN_DB]);
            }
        }
        "alg" => {
            let mut v = *HarmonicCompressor::default().values();
            let mut modes = Modes::default();
            modes.prepare(c.sr);
            modes.reset();
            out = vec![0.0; c.frames];
            for b in 0..n_blocks {
                while next < sets.len() && sets[next].0 <= b {
                    v[sets[next].1] = sets[next].2;
                    next += 1;
                }
                let mode = Mode::from_index(v[idx::MODE] as i32);
                modes.update(mode, &comp_params(&v));
                let start = b * c.block;
                let n = c.block.min(c.frames - start);
                for s in 0..n {
                    out[start + s] = modes.process(mode, c.input[start + s]);
                }
                meter.push(modes.last_gr_db(mode));
            }
        }
        "harm" => {
            let mut v = *HarmonicCompressor::default().values();
            let mut h = Harmonics::default();
            h.prepare(c.sr);
            out = vec![0.0; c.frames * 2];
            for b in 0..n_blocks {
                while next < sets.len() && sets[next].0 <= b {
                    v[sets[next].1] = sets[next].2;
                    next += 1;
                }
                let start = b * c.block;
                let n = c.block.min(c.frames - start);
                for s in 0..n {
                    for ch in 0..2 {
                        let i = 2 * (start + s) + ch;
                        out[i] = h.process(c.input[i], v[idx::H2] / 100.0, v[idx::H3] / 100.0, v[idx::H4] / 100.0, v[idx::H5] / 100.0, ch);
                    }
                }
            }
        }
        _ => unreachable!(),
    }
    (out, meter)
}

fn compare(a: &[f32], b: &[f32], ma: &[f32], mb: &[f32]) -> Diff {
    let mut d = Diff { total: a.len(), ..Diff::default() };
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        if x.to_bits() != y.to_bits() {
            d.differing += 1;
            let e = if x.is_finite() && y.is_finite() { (x - y).abs() } else { f32::INFINITY };
            d.max_audio = d.max_audio.max(e);
        }
    }
    for (x, y) in ma.iter().zip(mb) {
        if x.to_bits() != y.to_bits() {
            let e = if x.is_finite() && y.is_finite() { (x - y).abs() } else { f32::INFINITY };
            d.max_meter = d.max_meter.max(e);
        }
    }
    d
}

/// The full parameter set at block 0 for a chain case (1x, everything else as given).
fn base_sets(over: &[(usize, f32)]) -> Vec<(usize, usize, f32)> {
    let m = HarmonicCompressor::default();
    let mut v: Vec<f32> = (0..idx::COUNT).map(|i| m.param(i)).collect();
    v[idx::OS_MODE] = 0.0;
    for &(i, x) in over {
        v[i] = x;
    }
    v.iter().enumerate().map(|(i, &x)| (0, i, x)).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let harness = PathBuf::from(&args[1]);
    let dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&dir).unwrap();

    let mut cases: Vec<Case> = Vec::new();
    let secs = 2.0;

    // Chain cases at 1x: every mode with default, heavy, auto-timed and extreme settings.
    let sigs = [(Sig::ToneSteps, "tones"), (Sig::Bursts, "bursts"), (Sig::Noise, "noise"), (Sig::Programme, "programme"), (Sig::Hot, "hot")];
    for mode in 0..7 {
        let variants: Vec<(&str, Vec<(usize, f32)>)> = vec![
            ("default", vec![]),
            ("heavy", vec![(idx::THRESHOLD, -40.0), (idx::RATIO, 20.0), (idx::ATTACK, 0.5), (idx::RELEASE, 40.0), (idx::MAKEUP, 12.0)]),
            ("auto", vec![(idx::THRESHOLD, -30.0), (idx::RATIO, 8.0), (idx::AUTO_ATTACK, 1.0), (idx::AUTO_RELEASE, 1.0)]),
            ("limit", vec![(idx::THRESHOLD, -24.0), (idx::RATIO, 100.0), (idx::KNEE, 24.0), (idx::ATTACK, 0.1), (idx::RELEASE, 3000.0)]),
            ("expand", vec![(idx::THRESHOLD, -20.0), (idx::RATIO, 0.5), (idx::KNEE, 0.0)]),
            ("negative", vec![(idx::THRESHOLD, -30.0), (idx::RATIO, -2.0), (idx::KNEE, 12.0)]),
            (
                "harmonics",
                vec![(idx::H2, 60.0), (idx::H3, 100.0), (idx::H4, 30.0), (idx::H5, 80.0), (idx::MAKEUP, 6.0), (idx::MIX, 70.0)],
            ),
            (
                "sidechain",
                vec![(idx::SC_ENABLED, 1.0), (idx::SC_HP_FREQ, 400.0), (idx::THRESHOLD, -35.0), (idx::RATIO, 6.0)],
            ),
            ("listen", vec![(idx::SC_ENABLED, 1.0), (idx::SC_LISTEN, 1.0), (idx::SC_HP_FREQ, 90.0)]),
            (
                "full",
                vec![
                    (idx::INPUT_GAIN, 9.0),
                    (idx::OUTPUT_GAIN, -6.5),
                    (idx::TRANSIENT_ENABLED, 1.0),
                    (idx::TRANSIENT_ATTACK, 80.0),
                    (idx::TRANSIENT_SUSTAIN, -60.0),
                    (idx::TRANSIENT_SPEED, 3.0),
                    (idx::AUTO_GAIN, 1.0),
                    (idx::H2, 25.0),
                    (idx::MIX, 50.0),
                ],
            ),
            ("delta", vec![(idx::DELTA, 1.0), (idx::THRESHOLD, -30.0), (idx::H3, 40.0)]),
            ("delta-auto", vec![(idx::DELTA, 1.0), (idx::AUTO_GAIN, 1.0), (idx::INPUT_GAIN, 6.0), (idx::THRESHOLD, -30.0)]),
        ];
        for (vi, (vname, over)) in variants.iter().enumerate() {
            let (sig, sname) = sigs[(mode + vi) % sigs.len()];
            let sr = 48000.0;
            let frames = (sr * secs) as usize;
            let mut o = over.clone();
            o.push((idx::MODE, mode as f32));
            cases.push(Case {
                name: format!("chain {} {} {}", Mode::LABELS[mode], vname, sname),
                cmd: "chain",
                sr,
                block: 512,
                frames,
                input: signal(sig, sr, frames, 7 + mode as u32 * 13 + vi as u32),
                key: None,
                sets: base_sets(&o),
            });
        }
    }

    // Automation: mode switches, toggles and sweeps mid-stream; other rates and block sizes.
    for &(sr, block) in &[(44100.0, 37usize), (48000.0, 512), (96000.0, 256), (192000.0, 1024)] {
        let frames = (sr * 3.0) as usize;
        let mut sets = base_sets(&[(idx::THRESHOLD, -28.0), (idx::H2, 20.0)]);
        let n_blocks = frames / block;
        for b in 1..n_blocks {
            if b % 23 == 0 {
                sets.push((b, idx::MODE, ((b / 23) % 7) as f32));
            }
            if b % 31 == 0 {
                sets.push((b, idx::AUTO_RELEASE, ((b / 31) % 2) as f32));
                sets.push((b, idx::ATTACK, [0.3, 5.0, 60.0][(b / 31) % 3]));
            }
            if b % 41 == 0 {
                sets.push((b, idx::DELTA, ((b / 41) % 2) as f32));
                sets.push((b, idx::AUTO_GAIN, ((b / 82) % 2) as f32));
                sets.push((b, idx::INPUT_GAIN, [0.0, 6.0, -3.0][(b / 41) % 3]));
            }
            if b % 53 == 0 {
                sets.push((b, idx::MIX, [100.0, 40.0, 0.0, 85.0][(b / 53) % 4]));
                sets.push((b, idx::SC_ENABLED, ((b / 53) % 2) as f32));
                sets.push((b, idx::TRANSIENT_ENABLED, ((b / 106) % 2) as f32));
                sets.push((b, idx::OUTPUT_GAIN, [0.0, 3.0][(b / 53) % 2]));
            }
            if b % 67 == 0 {
                sets.push((b, idx::SC_LISTEN, ((b / 67) % 2) as f32));
            }
        }
        sets.sort_by_key(|s| s.0);
        cases.push(Case {
            name: format!("chain automation {} Hz block {}", sr, block),
            cmd: "chain",
            sr,
            block,
            frames,
            input: signal(Sig::Programme, sr, frames, 99),
            key: None,
            sets,
        });
    }

    // External key: a loud key over a quiet programme, a silent key, and Internal with a key.
    for (kname, ksrc, keysig) in [("external", 1.0, Sig::Bursts), ("external-silent-key", 1.0, Sig::Silence), ("internal", 0.0, Sig::Bursts)] {
        for mode in [0usize, 2, 5, 6] {
            let sr = 48000.0;
            let frames = (sr * secs) as usize;
            let mut input = signal(Sig::Programme, sr, frames, 5);
            for x in &mut input {
                *x *= 0.25;
            }
            cases.push(Case {
                name: format!("chain key {} {}", kname, Mode::LABELS[mode]),
                cmd: "chain",
                sr,
                block: 480,
                frames,
                input,
                key: Some(signal(keysig, sr, frames, 77)),
                sets: base_sets(&[(idx::MODE, mode as f32), (idx::KEY_SOURCE, ksrc), (idx::THRESHOLD, -30.0), (idx::SC_ENABLED, 1.0)]),
            });
        }
    }

    // Each mode alone at oversampled rates (where the full chain uses a different resampler).
    for &sr in &[44100.0, 48000.0 * 2.0, 48000.0 * 4.0, 48000.0 * 8.0, 44100.0 * 8.0] {
        for mode in 0..7 {
            for auto in [0.0, 1.0] {
                let frames = (sr * 1.0) as usize;
                let st = signal(Sig::Bursts, sr, frames, 3 + mode as u32);
                let input: Vec<f32> = st.chunks(2).map(|p| p[0].abs().max(p[1].abs())).collect();
                let mut sets = vec![
                    (0, idx::MODE, mode as f32),
                    (0, idx::THRESHOLD, -26.0),
                    (0, idx::RATIO, 6.0),
                    (0, idx::ATTACK, 3.0),
                    (0, idx::RELEASE, 120.0),
                    (0, idx::KNEE, 8.0),
                    (0, idx::AUTO_ATTACK, auto),
                    (0, idx::AUTO_RELEASE, auto),
                ];
                sets.push((40, idx::RATIO, 12.0));
                sets.push((80, idx::THRESHOLD, -40.0));
                sets.push((120, idx::RATIO, 100.0));
                cases.push(Case {
                    name: format!("alg {} {} Hz auto {}", Mode::LABELS[mode], sr, auto),
                    cmd: "alg",
                    sr,
                    block: 512,
                    frames,
                    input,
                    key: None,
                    sets,
                });
            }
        }
    }

    // The harmonic stage alone, at the rates oversampling runs it at, with levels beyond the rail.
    for &sr in &[48000.0, 96000.0, 192000.0, 384000.0] {
        for (hname, h) in [("even", [80.0, 0.0, 50.0, 0.0]), ("odd", [0.0, 100.0, 0.0, 100.0]), ("all", [100.0, 100.0, 100.0, 100.0])] {
            let frames = (sr * 0.5) as usize;
            let sets = vec![(0, idx::H2, h[0]), (0, idx::H3, h[1]), (0, idx::H4, h[2]), (0, idx::H5, h[3]), (50, idx::H2, 0.0), (90, idx::H3, 0.0), (90, idx::H5, 0.0), (91, idx::H4, 0.0)];
            cases.push(Case {
                name: format!("harm {} {} Hz", hname, sr),
                cmd: "harm",
                sr,
                block: 512,
                frames,
                input: signal(Sig::Hot, sr, frames, 11),
                key: None,
                sets,
            });
        }
    }

    let mut worst = Diff::default();
    let mut total_samples = 0usize;
    let mut total_diff = 0usize;
    for c in &cases {
        let sets = snapped(&c.sets);
        let (ca, cm) = run_cpp(&harness, &dir, c, &sets);
        let (ra, rm) = run_rust(c, &sets);
        let d = compare(&ca, &ra, &cm, &rm);
        let deepest = if c.cmd == "chain" { cm.iter().step_by(3).fold(0.0f32, |a, &b| a.max(b)) } else { cm.iter().fold(0.0f32, |a, &b| a.max(-b)) };
        let peak = ca.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        println!(
            "{:<55} samples {:>9} differing {:>8} max |audio| {:.3e} max |meter| {:.3e} (deepest GR {:.1} dB, out peak {:.2})",
            c.name, d.total, d.differing, d.max_audio, d.max_meter, deepest, peak
        );
        total_samples += d.total;
        total_diff += d.differing;
        worst.max_audio = worst.max_audio.max(d.max_audio);
        worst.max_meter = worst.max_meter.max(d.max_meter);
    }
    println!(
        "\n{} cases, {} samples, {} not bit-identical, largest audio difference {:.3e}, largest meter difference {:.3e}",
        cases.len(),
        total_samples,
        total_diff,
        worst.max_audio,
        worst.max_meter
    );
}
