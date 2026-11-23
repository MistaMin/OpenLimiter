//! Sample-by-sample comparison against the original C++ engine.
//!
//!   c++ -std=c++17 -O2 -ffp-contract=off -I "$DUALSONE_DIR/Source" \
//!       harness/harness.cpp -o target/harness_cpp
//!   CARGO_TARGET_DIR=target cargo run --offline --release --example compare -- target/harness_cpp target/cmp
//!
//! Each case writes a stereo signal and a parameter script, runs the C++ harness on them, runs this
//! crate's `DualSone` module with the same script (set_param at the same block starts), and reports
//! the largest difference of the audio and of the slow and fast gain traces.

use comp_sone::engine::Trace;
use comp_sone::DualSone;
use olm_api::Module;
use std::f64::consts::PI;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let (u, v) = (self.next(), self.next());
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
}

fn db(x: f64) -> f64 {
    10f64.powf(x / 20.0)
}

type Sig = (Vec<f32>, Vec<f32>);

fn tone_steps(fs: f64) -> Sig {
    let n = (fs * 14.0) as usize;
    let levels = [-40.0, -12.0, -24.0, -3.0, -18.0, -200.0, -10.0];
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        let t = i as f64 / fs;
        let a = db(levels[((t / 2.0) as usize).min(levels.len() - 1)]);
        let f = if t >= 12.0 { 60.0 } else { 1000.0 };
        l[i] = (a * (2.0 * PI * f * t).sin()) as f32;
        r[i] = (a * (2.0 * PI * f * t + 0.3).sin()) as f32;
    }
    (l, r)
}

fn bursts(fs: f64) -> Sig {
    let n = (fs * 12.0) as usize;
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let bed = db(-40.0);
    let (mut l, mut r): (Vec<f32>, Vec<f32>) = (0..n).map(|_| ((rng.gauss() * bed) as f32, (rng.gauss() * bed) as f32)).unzip();
    let add = |start: f64, len: f64, f: f64, lev: f64, l: &mut Vec<f32>, r: &mut Vec<f32>| {
        let (s, k) = ((start * fs) as usize, (len * fs) as usize);
        for i in s..(s + k).min(n) {
            let v = (db(lev) * (2.0 * PI * f * (i - s) as f64 / fs).sin()) as f32;
            l[i] += v;
            r[i] += 0.7 * v;
        }
    };
    let mut t = 0.5;
    while t < 6.0 {
        add(t, 0.002, 1000.0, -6.0, &mut l, &mut r);
        t += 0.4;
    }
    let mut t = 6.0;
    while t < 12.0 {
        add(t, 0.010, 80.0, -3.0, &mut l, &mut r);
        add(t + 0.2, 0.005, 3000.0, -10.0, &mut l, &mut r);
        t += 1.5;
    }
    (l, r)
}

fn noise_levels(fs: f64) -> Sig {
    let n = (fs * 16.0) as usize;
    let mut rng = Rng(12345);
    let levels = [-24.0, -6.0, -200.0, -18.0, -30.0, -9.0, -15.0, -12.0];
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        let a = db(levels[((i as f64 / fs / 2.0) as usize).min(levels.len() - 1)]);
        l[i] = (rng.gauss() * a) as f32;
        r[i] = (rng.gauss() * a) as f32;
    }
    (l, r)
}

fn programme(fs: f64) -> Sig {
    // Noise bed with bursts, different in each channel, with a loud and a quiet section.
    let n = (fs * 12.0) as usize;
    let mut rng = Rng(777);
    let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        let t = i as f64 / fs;
        let sec = if ((t / 3.0) as usize) % 2 == 1 { db(-14.0) } else { 1.0 };
        let hit = if (i % (fs as usize / 4)) < (fs * 0.004) as usize { db(-4.0) * (2.0 * PI * 1500.0 * t).sin() } else { 0.0 };
        l[i] = ((rng.gauss() * db(-18.0) + hit) * sec) as f32;
        r[i] = ((rng.gauss() * db(-20.0) + 0.5 * hit + 0.2 * (2.0 * PI * 110.0 * t).sin()) * sec) as f32;
    }
    (l, r)
}

struct Case {
    name: String,
    fs: f64,
    block: usize,
    sig: Sig,
    sets: Vec<(usize, &'static str, f32)>,
}

struct Result {
    audio: f64,
    slow: f64,
    fast: f64,
    differing: usize,
    n: usize,
    lat_cpp: i64,
    lat_rs: usize,
}

fn run_case(c: &Case, harness: &Path, dir: &Path) -> Result {
    let n = c.sig.0.len();
    let tag = c.name.replace(|ch: char| !ch.is_ascii_alphanumeric(), "_");
    let (script, inp, out) = (dir.join(format!("{tag}.txt")), dir.join(format!("{tag}.in")), dir.join(format!("{tag}.out")));
    let mut s = format!("fs {}\nblock {}\nn {}\n", c.fs, c.block, n);
    for (at, id, v) in &c.sets {
        s += &format!("set {at} {id} {v:?}\n");
    }
    fs::write(&script, s).unwrap();
    let mut f = fs::File::create(&inp).unwrap();
    for v in c.sig.0.iter().chain(c.sig.1.iter()) {
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    drop(f);
    let o = Command::new(harness).arg(&script).arg(&inp).arg(&out).output().expect("run harness");
    assert!(o.status.success(), "harness failed: {:?}", o);
    let lat_cpp: i64 = String::from_utf8_lossy(&o.stdout).trim().trim_start_matches("latency ").parse().unwrap();
    let bytes = fs::read(&out).unwrap();
    let cpp: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();

    // Rust module, same script: sample-0 values before prepare, the rest at block starts.
    let mut m = DualSone::default();
    let mut next = 0;
    let apply = |m: &mut DualSone, next: &mut usize, start: usize| {
        while *next < c.sets.len() && c.sets[*next].0 <= start {
            let (_, id, v) = c.sets[*next];
            m.set_param(DualSone::index_of(id).expect("param id"), v);
            *next += 1;
        }
    };
    apply(&mut m, &mut next, 0);
    m.prepare(c.fs as f32, c.block);
    let (mut l, mut r) = c.sig.clone();
    let (mut gs, mut gf, mut le) = (vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]);
    let mut start = 0;
    while start < n {
        apply(&mut m, &mut next, start);
        let end = (start + c.block).min(n);
        let mut t = Trace { slow_gain_db: Some(&mut gs[start..end]), fast_gain_db: Some(&mut gf[start..end]), slow_engine_loudness: Some(&mut le[start..end]), ..Default::default() };
        m.process_traced(&mut l[start..end], &mut r[start..end], Some(&mut t));
        start = end;
    }

    let diff = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| if x == y || (x.is_nan() && y.is_nan()) { 0.0 } else { (x - y).abs() as f64 }).fold(0.0, f64::max);
    let mut differing = 0;
    let engine_loudness_differing = (0..n).filter(|&i| le[i] != cpp[4 * n + i]).count();
    if std::env::var_os("COMPARE_VERBOSE").is_some() {
        if let Some(i) = (0..n).find(|&i| le[i] != cpp[4 * n + i]) {
            println!("  first engine-loudness difference at {i}: {:e} vs {:e} ({engine_loudness_differing} samples)", le[i], cpp[4 * n + i]);
        }
    }
    for i in 0..n {
        if l[i] != cpp[i] || r[i] != cpp[n + i] || gs[i] != cpp[2 * n + i] || gf[i] != cpp[3 * n + i] {
            if std::env::var_os("COMPARE_VERBOSE").is_some() {
                println!("  sample {i}: slow {:e} vs {:e}, audio {:e} vs {:e}", gs[i], cpp[2 * n + i], l[i], cpp[i]);
            }
            differing += 1;
        }
    }
    Result {
        audio: diff(&l, &cpp[..n]).max(diff(&r, &cpp[n..2 * n])),
        slow: diff(&gs, &cpp[2 * n..3 * n]),
        fast: diff(&gf, &cpp[3 * n..4 * n]),
        differing,
        n,
        lat_cpp,
        lat_rs: m.latency(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let harness = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("target/harness_cpp"));
    let dir = PathBuf::from(args.get(2).map(String::as_str).unwrap_or("target/cmp"));
    fs::create_dir_all(&dir).unwrap();

    let heavy: Vec<(usize, &str, f32)> = vec![(0, "threshold", -30.0), (0, "fastCrest", 2.0), (0, "slowRatio", 4.0), (0, "fastRatio", 6.0)];
    let mut cases = Vec::new();
    let signals: [(&str, fn(f64) -> Sig); 4] = [("tone steps", tone_steps), ("bursts", bursts), ("noise levels", noise_levels), ("programme", programme)];
    for (sname, gen) in signals {
        let sig = gen(48000.0);
        for mode in 0..4 {
            let mname = ["parallel", "series", "fast-led", "mix"][mode];
            let mut sets = vec![(0, "mode", mode as f32)];
            cases.push(Case { name: format!("{sname} / {mname} / defaults"), fs: 48000.0, block: 256, sig: sig.clone(), sets: sets.clone() });
            sets.extend(heavy.iter().cloned());
            cases.push(Case { name: format!("{sname} / {mname} / heavy"), fs: 48000.0, block: 256, sig: sig.clone(), sets });
        }
    }

    // Automation: mode switches every 2048 samples (crossfades), attack/lookahead flips, glides.
    let sig = programme(48000.0);
    let mut sets = heavy.clone();
    let mut t = 0;
    let mut k = 0;
    while t < sig.0.len() {
        sets.push((t, "mode", (k % 4) as f32));
        sets.push((t, "fastAttack", if k % 2 == 0 { 2.0 } else { 30.0 }));
        sets.push((t, "fastLookahead", if (k / 2) % 2 == 0 { 100.0 } else { 0.0 }));
        t += 2048;
        k += 1;
    }
    cases.push(Case { name: "programme / automation: mode, attack, lookahead".into(), fs: 48000.0, block: 256, sig: sig.clone(), sets });

    // Input, Autogain, Delta, Mix, Output, flat detector, windows, engines on/off, all mid-stream.
    let fs48 = 48000.0f64;
    let at = |s: f64| (s * fs48) as usize;
    let mut sets = heavy.clone();
    sets.extend([
        (0, "input", 6.0),
        (0, "autoGain", 1.0),
        (0, "mix", 70.0),
        (at(2.0), "detector", 1.0),
        (at(3.0), "delta", 1.0),
        (at(4.0), "delta", 0.0),
        (at(4.5), "output", -3.0),
        (at(5.0), "slowWindow", 0.0),
        (at(6.0), "slowWindow", 2.0),
        (at(6.5), "fastOn", 0.0),
        (at(7.0), "fastOn", 1.0),
        (at(7.5), "slowOn", 0.0),
        (at(8.0), "slowOn", 1.0),
        (at(8.5), "threshold", -20.0),
        (at(9.0), "slowAttack", 50.0),
        (at(9.0), "slowRelease", 300.0),
        (at(9.5), "fastRelease", 5.0),
        (at(10.0), "input", -6.0),
        (at(10.5), "autoGain", 0.0),
        (at(11.0), "mode", 3.0),
    ]);
    cases.push(Case { name: "programme / automation: input, autogain, delta, mix, detector, window, on/off".into(), fs: 48000.0, block: 256, sig: sig.clone(), sets });

    // Other rates and an odd block size.
    for (rate, block) in [(44100.0, 37usize), (96000.0, 512), (192000.0, 1024)] {
        for mode in 0..4 {
            let mut sets = heavy.clone();
            sets.push((0, "mode", mode as f32));
            sets.push((0, "fastAttack", 0.5));
            cases.push(Case { name: format!("bursts / mode {mode} / {rate} Hz, block {block}"), fs: rate, block, sig: bursts(rate), sets: sets.clone() });
        }
        let mut sets = heavy.clone();
        sets.push((0, "mode", 1.0));
        cases.push(Case { name: format!("noise levels / series / {rate} Hz, block {block}"), fs: rate, block, sig: noise_levels(rate), sets });
    }

    let mut worst = (0.0f64, 0.0f64, 0.0f64);
    let mut total_diff = 0usize;
    let mut total_n = 0usize;
    println!("{:<80} {:>10} {:>10} {:>10} {:>9} {:>11}", "case", "audio", "slow dB", "fast dB", "differing", "latency");
    for c in &cases {
        let r = run_case(c, &harness, &dir);
        println!(
            "{:<80} {:>10.3e} {:>10.3e} {:>10.3e} {:>9} {:>5}/{:<5}",
            c.name, r.audio, r.slow, r.fast, r.differing, r.lat_cpp, r.lat_rs
        );
        assert_eq!(r.lat_cpp as usize, r.lat_rs, "latency differs");
        worst = (worst.0.max(r.audio), worst.1.max(r.slow), worst.2.max(r.fast));
        total_diff += r.differing;
        total_n += r.n;
    }
    println!(
        "\n{} cases, {} samples: largest difference audio {:.3e}, slow gain {:.3e} dB, fast gain {:.3e} dB; {} samples not bit-identical",
        cases.len(),
        total_n,
        worst.0,
        worst.1,
        worst.2,
        total_diff
    );
}
