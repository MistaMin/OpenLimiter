//! Ports of the DSP-relevant checks in DualSone's `tests/PluginTests.cpp`, run through the
//! `Module` trait (the processor-level behaviour: settings handed over once per block).

use comp_sone::{idx, DualSone};
use olm_api::Module;

struct Lcg(u64);
impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
}

#[test]
fn hostile_block_sizes_bit_identical() {
    let render = |sizes: &[usize]| {
        let mut m = DualSone::default();
        m.prepare(48000.0, 4096);
        m.set_param(idx::THRESHOLD, -30.0);
        let mut rng = Lcg(21);
        let total = 48000 * 3;
        let mut out = Vec::with_capacity(total);
        let (mut l, mut r) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
        let (mut done, mut k) = (0, 0);
        while done < total {
            let n = sizes[k % sizes.len()].min(total - done);
            k += 1;
            for i in 0..n {
                let burst = if (done + i) % 12000 < 1500 { 0.9 } else { 0.1 };
                let v = (rng.next_f32() * 2.0 - 1.0) * burst;
                l[i] = v;
                r[i] = -v;
            }
            m.process(&mut l[..n], &mut r[..n]);
            out.extend_from_slice(&l[..n]);
            done += n;
        }
        out
    };
    let fixed = render(&[512]);
    let hostile = render(&[0, 1, 7, 4096, 3, 2049, 0, 511, 2048, 1]);
    let differ = fixed.iter().zip(&hostile).filter(|(a, b)| a != b).count();
    println!("PASS  hostile block sizes (0, 1, odd, 4096) give bit-identical output  -- {differ} samples differ");
    assert!(fixed.len() == hostile.len() && differ == 0);
}

#[test]
fn random_parameter_states_finite_and_audible() {
    let mut bad = Vec::new();
    for seed in 0..40u64 {
        let mut m = DualSone::default();
        m.prepare(48000.0, 512);
        let mut noise = Lcg(1000 + seed);
        let mut values = Lcg(seed * 7919 + 3);
        let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        let mut block = |m: &mut DualSone, i: usize, l: &mut [f32], r: &mut [f32]| {
            let level = if (i / 40) % 2 == 0 { 0.3 } else { 0.05 };
            for n in 0..512 {
                let k = if n % 97 == 0 { 3.0 } else { 1.0 };
                l[n] = (noise.next_f32() * 2.0 - 1.0) * level * k;
                r[n] = (noise.next_f32() * 2.0 - 1.0) * level * k;
            }
            m.process(l, r);
        };
        for i in 0..200 {
            block(&mut m, i, &mut l, &mut r);
        }
        let params = m.params();
        for (i, p) in params.iter().enumerate() {
            if !matches!(p.id, "mix" | "output" | "delta") {
                m.set_param(i, p.min + values.next_f32() * (p.max - p.min));
            }
        }
        let (mut finite, mut peak) = (true, 0.0f32);
        for i in 200..950 {
            block(&mut m, i, &mut l, &mut r);
            finite &= l.iter().chain(r.iter()).all(|v| v.is_finite());
            if i > 300 {
                peak = l.iter().fold(peak, |a, v| a.max(v.abs()));
            }
        }
        if !finite || peak < 1.0e-3 {
            bad.push(format!("seed {seed}: finite {finite}, peak {peak}"));
        }
    }
    println!("PASS  40 random parameter states set mid-stream: output stays finite and audible  -- {bad:?}");
    assert!(bad.is_empty());
}
