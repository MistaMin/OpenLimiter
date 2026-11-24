//! Fixtures for the optimisation gate: renders every oversampling factor and filter, two settings
//! (plain wet; mix 60 % with harmonics and Auto Gain, which runs the dry and reference alignment),
//! block sizes 1, 17, 64, 129 and 512, through one programme-like fixture (silence, a quiet sine,
//! a full-scale sine, bursts, a sweep; left and right differ). Writes raw f32 to `$NULL_DIR`.
//! `NULL_DIR=<dir> cargo test --release --test null_render -- --ignored`

use comp_harmonic::HarmonicCompressor;
use olm_api::Module;

fn fixture(sr: f32) -> (Vec<f32>, Vec<f32>) {
    let n = (sr * 3.0) as usize;
    let mut l = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];
    for i in (sr * 0.2) as usize..n {
        let t = i as f32 / sr;
        let x = match t {
            t if t < 0.8 => 0.01 * (t * 997.0 * std::f32::consts::TAU).sin(),
            t if t < 1.4 => (t * 220.0 * std::f32::consts::TAU).sin(),
            t if t < 2.2 => (if (t * 8.0).fract() < 0.2 { 0.9 } else { 0.05 }) * (t * 3100.0 * std::f32::consts::TAU).sin(),
            t => 0.7 * (std::f32::consts::TAU * 40.0 * ((t - 2.2) * 6.0).exp() / 6.0).sin(),
        };
        l[i] = x;
        r[i] = 0.8 * x + 0.1 * (t * 61.0 * std::f32::consts::TAU).sin();
    }
    (l, r)
}

#[test]
#[ignore]
fn null_render() {
    let Ok(dir) = std::env::var("NULL_DIR") else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let sr = 48_000.0;
    let (fl, fr) = fixture(sr);
    for os in 0..4 {
        for filter in 0..2 {
            for (name, settings) in [("wet", &[][..]), ("mix", &[("mix", 60.0), ("harmonic2", 30.0), ("harmonic3", 20.0), ("autoGain", 1.0)][..])] {
                for block in [1usize, 17, 64, 129, 512] {
                    let mut m = HarmonicCompressor::default();
                    m.prepare(sr, 512);
                    let params = m.params();
                    let idx = |id: &str| params.iter().position(|p| p.id == id).unwrap();
                    let (o, f) = (idx("oversampleMode"), idx("oversampleFilter"));
                    m.set_param(o, os as f32);
                    m.set_param(f, filter as f32);
                    for (id, v) in settings {
                        let i = idx(id);
                        m.set_param(i, *v);
                    }
                    let (mut l, mut r) = (fl.clone(), fr.clone());
                    for start in (0..l.len()).step_by(block) {
                        let end = (start + block).min(l.len());
                        m.process(&mut l[start..end], &mut r[start..end]);
                    }
                    let mut bytes = Vec::with_capacity(l.len() * 8);
                    for (a, b) in l.iter().zip(&r) {
                        bytes.extend_from_slice(&a.to_le_bytes());
                        bytes.extend_from_slice(&b.to_le_bytes());
                    }
                    std::fs::write(format!("{dir}/os{os}-f{filter}-{name}-b{block}.f32"), bytes).unwrap();
                }
            }
        }
    }
}
