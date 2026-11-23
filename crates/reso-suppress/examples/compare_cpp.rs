//! Runs the same signal through this engine and through OpenEQ's (a harness binary given by
//! RESO_CPP), and prints the largest sample difference. Developer tool; not a test.
use reso_suppress::engine::{Engine, Settings};
use std::io::Write;
use std::process::{Command, Stdio};
fn main() {
    let n = 48000 * 2;
    let mut s = 12345u64;
    let mut x: Vec<f32> = (0..n).map(|_| { s ^= s << 13; s ^= s >> 7; s ^= s << 17; ((s >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * 0.1 }).collect();
    // resonance at 3 kHz
    let (f, g, q, sr) = (3000.0f64, 12.0f64, 8.0f64, 48000.0f64);
    let a = 10f64.powf(g / 40.0); let w = 2.0 * std::f64::consts::PI * f / sr; let al = w.sin() / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = (1.0 + al * a, -2.0 * w.cos(), 1.0 - al * a, 1.0 + al / a, -2.0 * w.cos(), 1.0 - al / a);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for v in x.iter_mut() { let y = (b0 * *v as f64 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0; x2 = x1; x1 = *v as f64; y2 = y1; y1 = y; *v = y as f32; }
    let depth: f32 = 0.8;
    let mut e = Engine::new(sr);
    let st = Settings { depth, ..Settings::default() };
    let (mut l, mut r) = (x.clone(), x.clone());
    for p in (0..n).step_by(512) { let end = (p + 512).min(n); e.process(&mut l[p..end], &mut r[p..end], &st, None); }
    let mut child = Command::new(std::env::var("RESO_CPP").unwrap()).arg(n.to_string()).arg(depth.to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let bytes: Vec<u8> = x.iter().flat_map(|v| v.to_le_bytes()).collect();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let out = child.wait_with_output().unwrap().stdout;
    let c: Vec<f32> = out.chunks(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let worst = l.iter().zip(&c).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    let peak = c.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!("max abs difference {worst:e} (signal peak {peak})");
}
