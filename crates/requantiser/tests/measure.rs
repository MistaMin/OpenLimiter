//! Renders the measurement signals through the module and compares with the stored tables.
//! Tolerance (HANDOFF_NEXT_TOOL.md 3.3): 1 dB on total noise, 2 dB per band. Run with `-- --nocapture` to see
//! the tables (copied into REPORT.md).

mod common;
use common::*;
use olm_api::Module;
use requantiser::*;

const TOT: f64 = 1.0;
const BAND: f64 = 2.0;
const INFO: f64 = 0.0;

// Word indices
const W_OFF: usize = 0;
const W24: usize = 1;
const W22: usize = 2;
const W20: usize = 3;
const W18: usize = 4;
const W16: usize = 5;
const W12: usize = 6;
const W8: usize = 7;
// Shaping indices
const S_OFF: usize = 0;
const TILT1: usize = 1;
const TILT2: usize = 2;
const TOP_OCT: usize = 3;
const TILT_SOFT: usize = 4;
const RISE1: usize = 5;
const RISE2: usize = 6;
const RISE3: usize = 7;
const RISE4: usize = 8;
const RISE5: usize = 9;
const RISE6: usize = 10;
const VALLEY1: usize = 11;
const VALLEY2: usize = 12;
const VALLEY3: usize = 13;

// Dither amounts (%) chosen to match each source's flat level (see REPORT.md, free choices)
const AMT_A: f32 = 109.0;
const AMT_FULL: f32 = 100.0;
const AMT_D_MEDIUM: f32 = 85.0;
const AMT_D_LOW: f32 = 57.0;
const AMT_GATED: f32 = 64.0;

type Row = (&'static str, usize, usize, f32, f32, f64, [f64; 5]);

fn judge_rows(t: &mut Table, rows: &[Row], x: &[f64], seg: (usize, usize), info_if: impl Fn(&Row) -> bool) {
    for row in rows {
        let (name, word, shaping, depth, amount, tot, bands) = *row;
        let mut m = module(word, amount, shaping, depth);
        let y = render(&mut m, x);
        let (mt, mb) = noise_bands(&y[seg.0..seg.1]);
        let info = info_if(row);
        t.row(name, "noise 2-24 kHz", tot, mt, if info { INFO } else { TOT });
        let labels = ["2-4 kHz", "4-12 kHz", "12-16 kHz", "16-20 kHz", "20-24 kHz"];
        for i in 0..5 {
            t.row(name, labels[i], bands[i], mb[i], if info { INFO } else { BAND });
        }
    }
}

/// Source A grid: -36 dBFS 1 kHz tone, 2 s, 24-bit file, analysis y[SR/4 .. 2 SR - SR/4], 2-24 kHz.
/// Stored: measurements/srcA_recheck/dither_float.json (SPEC_LIMITERS.md 56.4). Dither amount 109 %.
#[test]
fn source_a_grid() {
    let rows: &[Row] = &[
        ("A 24-bit flat", W24, S_OFF, 100.0, AMT_A, -143.5, [-154.3, -148.1, -150.8, -151.3, -150.1]),
        ("A 24-bit Tilt 1 depth 0", W24, TILT1, 0.0, AMT_A, -143.5, [-154.3, -148.1, -150.8, -151.3, -150.1]),
        ("A 24-bit Tilt 1 50%", W24, TILT1, 50.0, AMT_A, -137.4, [-159.5, -153.6, -147.0, -142.0, -140.3]),
        ("A 24-bit Tilt 1 100%", W24, TILT1, 100.0, AMT_A, -132.3, [-165.2, -149.5, -140.8, -136.7, -135.6]),
        ("A 24-bit Tilt 2 50%", W24, TILT2, 50.0, AMT_A, -138.5, [-159.0, -153.2, -148.2, -143.5, -141.2]),
        ("A 24-bit Tilt 2 100%", W24, TILT2, 100.0, AMT_A, -133.7, [-165.1, -148.7, -142.6, -138.6, -136.5]),
        ("A 24-bit Top Octave 50%", W24, TOP_OCT, 50.0, AMT_A, -136.9, [-158.5, -151.3, -154.0, -144.0, -138.2]),
        ("A 24-bit Top Octave 100%", W24, TOP_OCT, 100.0, AMT_A, -131.7, [-164.5, -150.2, -150.3, -138.1, -133.0]),
        ("A 24-bit Tilt Soft 50%", W24, TILT_SOFT, 50.0, AMT_A, -140.8, [-158.4, -151.6, -149.1, -146.1, -144.2]),
        ("A 24-bit Tilt Soft 100%", W24, TILT_SOFT, 100.0, AMT_A, -137.1, [-164.2, -149.8, -145.3, -142.2, -140.4]),
        ("A word length off", W_OFF, S_OFF, 100.0, AMT_A, -151.0, [-165.4, -156.6, -157.4, -162.6, -155.0]),
        ("A 16-bit flat", W16, S_OFF, 100.0, AMT_A, -96.2, [-106.5, -100.6, -103.7, -103.6, -103.4]),
        ("A 16-bit Tilt 1 depth 0", W16, TILT1, 0.0, AMT_A, -96.2, [-106.5, -100.7, -103.8, -103.5, -103.4]),
        ("A 16-bit Tilt 1 50%", W16, TILT1, 50.0, AMT_A, -89.5, [-112.7, -108.5, -99.4, -93.8, -92.3]),
        ("A 16-bit Tilt 1 100%", W16, TILT1, 100.0, AMT_A, -84.2, [-130.3, -102.3, -92.7, -88.5, -87.5]),
        ("A 16-bit Tilt 2 depth 0", W16, TILT2, 0.0, AMT_A, -96.2, [-106.6, -100.7, -103.7, -103.5, -103.4]),
        ("A 16-bit Tilt 2 50%", W16, TILT2, 50.0, AMT_A, -90.5, [-112.2, -107.6, -100.5, -95.4, -93.1]),
        ("A 16-bit Tilt 2 100%", W16, TILT2, 100.0, AMT_A, -85.5, [-130.2, -101.5, -94.5, -90.4, -88.3]),
        ("A 16-bit Top Octave 50%", W16, TOP_OCT, 50.0, AMT_A, -88.9, [-111.4, -104.7, -108.4, -95.9, -90.1]),
        ("A 16-bit Top Octave 100%", W16, TOP_OCT, 100.0, AMT_A, -83.5, [-124.0, -103.2, -102.9, -89.9, -84.8]),
        ("A 16-bit Tilt Soft 50%", W16, TILT_SOFT, 50.0, AMT_A, -93.0, [-111.4, -105.2, -101.6, -98.0, -96.3]),
        ("A 16-bit Tilt Soft 100%", W16, TILT_SOFT, 100.0, AMT_A, -89.1, [-122.4, -102.7, -97.4, -93.9, -92.4]),
    ];
    let x = as_24bit_file(&tone(-36.0, 2 * SR));
    let mut t = Table::new("Source A grid");
    // word length Off: the stored row is the source's own processing residue, which this module does not have
    judge_rows(&mut t, rows, &x, (SR / 4, 2 * SR - SR / 4), |r| r.1 == W_OFF);
    t.finish();
}

/// Source D grid: -40 dBFS 1 kHz tone, analysis y[SR/2 .. 2 SR], 2-24 kHz.
/// Stored: measurements/srcD_dither.json (second run; SPEC_LIMITERS.md 68.3, 68.4).
/// Amounts Strong / Medium / Low / Off = 100 / 85 / 57 / 0 %; shaping Lowest..Max = Rise 1..6.
#[test]
fn source_d_grid() {
    let rows: &[Row] = &[
        ("D 24 Medium High", W24, RISE4, 100.0, AMT_D_MEDIUM, -137.9, [-163.7, -149.9, -154.3, -141.7, -141.1]),
        ("D 20 Medium High", W20, RISE4, 100.0, AMT_D_MEDIUM, -114.2, [-140.2, -134.9, -131.3, -117.6, -117.0]),
        ("D 16 Medium High", W16, RISE4, 100.0, AMT_D_MEDIUM, -90.1, [-116.0, -110.9, -107.0, -93.7, -92.9]),
        ("D 12 Medium High", W12, RISE4, 100.0, AMT_D_MEDIUM, -66.0, [-91.9, -86.8, -83.0, -69.4, -68.8]),
        ("D 8 Medium High", W8, RISE4, 100.0, AMT_D_MEDIUM, -41.9, [-67.9, -62.7, -59.1, -45.2, -44.8]),
        ("D 16 Strong High", W16, RISE4, 100.0, AMT_FULL, -89.2, [-115.5, -109.8, -106.0, -92.6, -92.1]),
        ("D 16 Medium High (b)", W16, RISE4, 100.0, AMT_D_MEDIUM, -90.1, [-116.3, -110.9, -107.3, -93.6, -92.9]),
        ("D 16 Low High", W16, RISE4, 100.0, AMT_D_LOW, -91.8, [-117.2, -112.4, -109.0, -95.2, -94.7]),
        ("D 16 amount Off High", W16, RISE4, 100.0, 0.0, -93.8, [-243.7, -114.5, -118.2, -96.2, -97.6]),
        ("D 16 Medium Max", W16, RISE6, 100.0, AMT_D_MEDIUM, -75.5, [-123.4, -118.1, -115.1, -80.3, -77.3]),
        ("D 16 Medium Higher", W16, RISE5, 100.0, AMT_D_MEDIUM, -82.7, [-120.0, -114.8, -111.3, -86.6, -85.0]),
        ("D 16 Medium High (c)", W16, RISE4, 100.0, AMT_D_MEDIUM, -90.1, [-116.2, -110.9, -107.1, -93.5, -93.0]),
        ("D 16 Medium Medium", W16, RISE3, 100.0, AMT_D_MEDIUM, -92.4, [-114.6, -109.5, -106.1, -95.8, -95.6]),
        ("D 16 Medium Low", W16, RISE2, 100.0, AMT_D_MEDIUM, -94.5, [-113.3, -107.8, -105.7, -98.1, -98.1]),
        ("D 16 Medium Lowest", W16, RISE1, 100.0, AMT_D_MEDIUM, -96.6, [-110.7, -105.2, -105.0, -101.4, -101.3]),
        ("D 16 Medium shaping Off", W16, S_OFF, 100.0, AMT_D_MEDIUM, -97.3, [-107.7, -101.7, -104.6, -104.7, -104.8]),
        ("D 16 Strong shaping Off", W16, S_OFF, 100.0, AMT_FULL, -96.7, [-107.0, -101.1, -104.2, -104.1, -104.2]),
        ("D 16 Low shaping Off", W16, S_OFF, 100.0, AMT_D_LOW, -99.8, [-110.4, -104.3, -107.3, -107.4, -106.9]),
        ("D 16 amount Off shaping Off", W16, S_OFF, 100.0, 0.0, -102.1, [-109.6, -108.8, -117.2, -111.8, -105.4]),
        ("D word length off (stored: bypassed)", W_OFF, S_OFF, 100.0, 0.0, -148.4, [-172.6, -150.5, -161.2, -159.9, -154.4]),
    ];
    // the stored bypass render (-148.4 dBFS, 25 distinct levels per 1000 samples) shows a 24-bit source file
    let x = as_24bit_file(&tone(-40.0, 12 * SR / 5));
    let mut t = Table::new("Source D grid");
    // amount Off leaves undithered, signal-locked error: level-dependent, not a noise figure (SPEC_LIMITERS.md 68.4)
    judge_rows(&mut t, rows, &x, (SR / 2, 2 * SR), |r| r.4 == 0.0);
    // sample peak of the tone (informational)
    for (name, word, shaping, amount, stored) in [
        ("D 12 Medium High", W12, RISE4, AMT_D_MEDIUM, -38.62),
        ("D 8 Medium High", W8, RISE4, AMT_D_MEDIUM, -28.16),
        ("D 16 Medium Max", W16, RISE6, AMT_D_MEDIUM, -39.45),
    ] {
        let mut m = module(word, amount, shaping, 100.0);
        let y = render(&mut m, &x);
        let pk = y[SR / 2..2 * SR].iter().fold(0f64, |a, v| a.max(v.abs()));
        t.row(name, "tone sample peak dBFS", stored, 20.0 * pk.log10(), INFO);
    }
    t.finish();
}

/// Source B: -40 dBFS tone (float file), analysis y[SR/2 .. 1.75 SR]. Flat dither only.
/// Stored: measurements/float_check/srcB_rms_dither.json (SPEC_LIMITERS.md 70.2).
#[test]
fn source_b_flat() {
    let rows: &[Row] = &[
        ("B off", W_OFF, S_OFF, 100.0, AMT_FULL, -186.8, [-191.3, -194.7, -193.0, -198.6, -194.1]),
        ("B 16 bits", W16, S_OFF, 100.0, AMT_FULL, -96.7, [-107.3, -101.0, -104.1, -104.0, -104.2]),
        ("B 20 bits", W20, S_OFF, 100.0, AMT_FULL, -120.8, [-131.4, -125.1, -128.1, -128.1, -128.2]),
        ("B 24 bits", W24, S_OFF, 100.0, AMT_FULL, -144.9, [-155.4, -149.3, -152.2, -152.2, -152.4]),
    ];
    let x: Vec<f64> = tone(-40.0, 11 * SR / 5).iter().map(|&v| v as f32 as f64).collect();
    let mut t = Table::new("Source B");
    judge_rows(&mut t, rows, &x, (SR / 2, 7 * SR / 4), |r| r.1 == W_OFF);
    t.finish();
}

/// Source H: 1 s of digital silence then a -36 dBFS 1 kHz tone (24-bit file). Silence RMS from y[SR/10 .. SR - SR/10];
/// residual = output minus the float tone over the tone step without its first and last SR/20; band shares of
/// the residual. Stored: measurements/srcH/results.json (keys starting "dither | quant").
/// First dither type = flat 100 %; second type = flat 64 % with "Silence Stays Silent"; none = no dither.
#[test]
fn source_h_grid() {
    // (word index 0..4 = 24, 22, 20, 18, 16 bits; dither kind 0 none, 1 first type, 2 second type; shaping 0..3;
    //  silence; residual; shares 0-4k .. 20-24k)
    #[rustfmt::skip]
    let stored: &[(usize, usize, usize, f64, f64, [f64; 5])] = &[
        (0,0,0,-240.0,-149.5,[-5.1,-7.1,-7.9,-13.0,-5.5]), (0,0,1,-240.0,-99.0,[-0.0,-56.5,-50.2,-50.4,-47.4]),
        (0,0,2,-240.0,-99.0,[-0.0,-56.5,-45.3,-44.5,-40.7]), (0,0,3,-240.0,-99.0,[-0.0,-57.8,-53.5,-34.4,-36.3]),
        (0,1,0,-144.5,-143.2,[-7.0,-5.3,-8.0,-8.4,-6.9]), (0,1,1,-138.4,-99.0,[-0.0,-54.6,-46.4,-43.7,-43.4]),
        (0,1,2,-132.5,-99.0,[-0.0,-55.9,-45.2,-36.9,-36.5]), (0,1,3,-126.6,-99.0,[-0.0,-56.6,-49.3,-30.8,-30.4]),
        (0,2,0,-240.0,-149.5,[-5.1,-7.1,-7.9,-13.0,-5.5]), (0,2,1,-240.0,-99.0,[-0.0,-56.9,-52.1,-50.1,-48.9]),
        (0,2,2,-240.0,-99.0,[-0.0,-56.0,-47.7,-46.4,-40.2]), (0,2,3,-136.3,-99.0,[-0.0,-57.0,-55.4,-35.6,-34.2]),
        (1,0,0,-240.0,-134.5,[-1.8,-9.4,-9.8,-11.9,-12.9]), (1,0,1,-240.0,-99.0,[-0.0,-53.7,-41.4,-42.8,-40.6]),
        (1,0,2,-240.0,-99.0,[-0.0,-56.9,-42.6,-29.9,-25.6]), (1,0,3,-240.0,-99.0,[-0.0,-52.3,-40.3,-22.8,-30.0]),
        (1,1,0,-132.5,-132.4,[-7.6,-4.9,-7.8,-7.9,-7.7]), (1,1,1,-126.4,-99.0,[-0.0,-45.1,-34.5,-31.5,-31.3]),
        (1,1,2,-120.5,-99.0,[-0.0,-48.1,-33.7,-25.1,-24.8]), (1,1,3,-114.5,-98.9,[-0.1,-50.6,-36.6,-19.1,-18.2]),
        (1,2,0,-240.0,-133.4,[-15.2,-9.2,-14.1,-12.3,-7.8]), (1,2,1,-240.0,-99.0,[-0.0,-51.8,-38.4,-39.2,-37.6]),
        (1,2,2,-240.0,-99.0,[-0.0,-53.1,-36.5,-25.8,-30.9]), (1,2,3,-124.2,-99.0,[-0.0,-53.1,-45.8,-27.9,-30.9]),
        (2,0,0,-240.0,-124.7,[-5.3,-6.3,-21.2,-6.9,-5.9]), (2,0,1,-240.0,-99.0,[-0.0,-40.9,-28.3,-32.3,-28.0]),
        (2,0,2,-240.0,-98.8,[-0.2,-41.6,-26.2,-17.6,-18.0]), (2,0,3,-240.0,-98.5,[-0.6,-45.9,-35.0,-14.9,-11.9]),
        (2,1,0,-120.4,-120.4,[-7.6,-4.7,-7.9,-7.9,-7.8]), (2,1,1,-114.3,-98.9,[-0.1,-33.6,-22.7,-19.6,-19.7]),
        (2,1,2,-108.4,-98.5,[-0.5,-37.0,-21.6,-13.2,-13.1]), (2,1,3,-102.5,-97.4,[-1.6,-41.2,-26.7,-8.5,-8.0]),
        (2,2,0,-240.0,-121.9,[-16.0,-9.3,-12.3,-8.0,-8.1]), (2,2,1,-240.0,-99.0,[-0.1,-37.9,-28.1,-25.3,-25.0]),
        (2,2,2,-240.0,-98.8,[-0.2,-43.9,-28.4,-18.2,-16.9]), (2,2,3,-112.2,-98.2,[-0.9,-45.8,-31.6,-9.8,-13.7]),
        (3,0,0,-240.0,-113.7,[-7.0,-5.1,-9.6,-4.8,-13.0]), (3,0,1,-240.0,-98.8,[-0.3,-26.3,-23.1,-16.9,-13.6]),
        (3,0,2,-240.0,-96.4,[-2.9,-29.8,-18.4,-7.3,-7.7]), (3,0,3,-240.0,-93.3,[-5.7,-38.4,-25.1,-4.7,-4.2]),
        (3,1,0,-108.3,-108.3,[-7.8,-4.9,-7.6,-8.0,-7.5]), (3,1,1,-102.3,-97.3,[-1.7,-23.2,-12.0,-9.2,-8.9]),
        (3,1,2,-96.4,-94.6,[-4.4,-28.8,-14.3,-5.2,-5.3]), (3,1,3,-90.5,-90.0,[-9.0,-36.4,-22.1,-3.9,-3.4]),
        (3,2,0,-240.0,-110.2,[-12.9,-9.8,-9.2,-7.4,-8.5]), (3,2,1,-240.0,-98.2,[-0.5,-26.0,-18.4,-12.2,-17.0]),
        (3,2,2,-240.0,-97.3,[-1.8,-29.8,-16.8,-7.5,-9.2]), (3,2,3,-240.0,-93.9,[-5.1,-39.7,-24.1,-5.0,-4.8]),
        (4,0,0,-240.0,-102.3,[-10.7,-3.9,-5.0,-7.5,-20.2]), (4,0,1,-240.0,-95.2,[-3.8,-23.1,-26.0,-2.4,-25.1]),
        (4,0,2,-240.0,-90.0,[-9.6,-28.9,-11.8,-5.0,-6.2]), (4,0,3,-240.0,-83.2,[-15.9,-34.9,-22.0,-3.0,-3.4]),
        (4,1,0,-96.3,-96.3,[-7.8,-4.8,-7.9,-7.7,-7.7]), (4,1,1,-90.2,-89.7,[-9.0,-18.6,-7.5,-4.5,-4.9]),
        (4,1,2,-84.4,-84.2,[-14.6,-27.3,-12.2,-3.6,-3.3]), (4,1,3,-78.5,-78.4,[-20.5,-36.0,-21.8,-3.2,-2.9]),
        (4,2,0,-240.0,-98.5,[-9.9,-8.2,-11.2,-6.5,-7.3]), (4,2,1,-240.0,-95.2,[-3.8,-23.1,-26.0,-2.4,-25.1]),
        (4,2,2,-240.0,-90.0,[-9.6,-28.9,-11.8,-5.0,-6.2]), (4,2,3,-88.1,-82.5,[-16.8,-35.1,-26.5,-10.3,-0.5]),
    ];
    let words = [W24, W22, W20, W18, W16];
    let shapes = [S_OFF, VALLEY1, VALLEY2, VALLEY3];
    let types = ["no dither", "flat", "gated"];
    let bits = [24, 22, 20, 18, 16];
    let shape_names = ["unshaped", "Valley 1", "Valley 2", "Valley 3"];

    let step = tone(-36.0, SR / 2);
    let mut x_float = vec![0.0; SR];
    x_float.extend_from_slice(&step);
    let x_file = as_24bit_file(&x_float);
    let (s0, s1) = (SR + SR / 20, SR + SR / 2 - SR / 20);

    let mut t = Table::new("Source H grid");
    for &(qi, kind, sh, sil, res, shares) in stored {
        let (amount, keep) = [(0.0, false), (AMT_FULL, false), (AMT_GATED, true)][kind];
        let mut m = module(words[qi], amount, shapes[sh], 100.0);
        m.set_param(P_KEEP_EXACT, if keep { 1.0 } else { 0.0 });
        let y = render(&mut m, &x_file);
        let ms = rms_db(&y[SR / 10..SR - SR / 10]);
        let z: Vec<f64> = (s0..s1).map(|i| y[i] - x_float[i]).collect();
        let mr = rms_db(&z);
        let msh = band_shares(&z);
        let name = format!("H {} bit {} {}", bits[qi], types[kind], shape_names[sh]);

        // Judged: silence level for every row except the unexplained noise on silence of the second type with the strongest curve;
        // residual for rows without shaping and with dither (no dither = signal-locked rounding error);
        // absolute 4-24 kHz bands for the first type shaped at 16 bits (elsewhere the unexplained -99 dBFS residual dominates).
        let sil_tol = if kind == 2 && sh == 3 && sil > -200.0 { INFO } else { TOT };
        t.row(&name, "silence RMS", sil, ms, sil_tol);
        let res_tol = if sh == 0 && kind != 0 { TOT } else { INFO };
        t.row(&name, "tone residual RMS", res, mr, res_tol);
        if sh > 0 && qi == 4 && kind == 1 {
            let labels = ["0-4 kHz", "4-12 kHz", "12-16 kHz", "16-20 kHz", "20-24 kHz"];
            for b in 0..5 {
                let tol = if b == 0 { INFO } else { BAND };
                t.row(&name, &format!("{} abs (residual + share)", labels[b]), res + shares[b], mr + msh[b], tol);
            }
        }
    }
    t.finish();
}

/// Word length Off with no dither (and also with dither set): bit-exact pass-through, NaN and infinities included.
#[test]
fn passthrough_is_bit_exact() {
    let mut seed = 12345u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        f32::from_bits((seed >> 32) as u32)
    };
    let orig_l: Vec<f32> = (0..20_000).map(|_| next()).chain([f32::NAN, f32::INFINITY, -0.0, 1e-40, 4.0]).collect();
    let orig_r: Vec<f32> = orig_l.iter().rev().copied().collect();
    for (amount, shaping) in [(0.0, S_OFF), (100.0, S_OFF), (100.0, RISE6)] {
        let mut m = module(W_OFF, amount, shaping, 100.0);
        m.set_param(P_AUTO_BLANK, 1.0);
        m.set_param(P_PEAK_LIMIT, 2.0);
        m.set_param(P_KEEP_EXACT, 1.0);
        let (mut l, mut r) = (orig_l.clone(), orig_r.clone());
        for (cl, cr) in l.chunks_mut(300).zip(r.chunks_mut(300)) {
            m.process(cl, cr);
        }
        assert!(l.iter().zip(&orig_l).all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(r.iter().zip(&orig_r).all(|(a, b)| a.to_bits() == b.to_bits()));
    }
    // 24 bits, no dither, no shaping, input already on the 24-bit grid: also exact
    let x = as_24bit_file(&tone(-6.0, SR));
    let mut m = module(W24, 0.0, S_OFF, 100.0);
    let y = render(&mut m, &x);
    assert!(y.iter().zip(&x).all(|(a, b)| a == b));
    println!("pass-through: bit-exact (word length Off, any dither, shaping and toggles; and 24-bit grid at 24 bits without dither)");
}

/// Auto-blank (SPEC_LIMITERS.md 68.4, 68.5; measurements/srcD_dither_switches3.json, srcD_dither_switches2.json):
/// 16 bits, Medium, High; tone then digital silence. Off: the silence carries the dither (-90.1 dBFS per half second).
/// On: the last 10 ms window with signal ends 700 ms after the audio stops, later output is digital silence.
#[test]
fn auto_blank() {
    let mut x = tone(-40.0, 2 * SR);
    x.extend(std::iter::repeat(0.0).take(8 * SR));
    let mut t = Table::new("Auto-blank (source D)");
    for on in [false, true] {
        let mut m = module(W16, AMT_D_MEDIUM, RISE4, 100.0);
        m.set_param(P_AUTO_BLANK, if on { 1.0 } else { 0.0 });
        let y = render(&mut m, &x);
        let tail = &y[2 * SR..];
        let name = if on { "16 Medium High, blank on" } else { "16 Medium High, blank off" };
        let stored_w: [f64; 3] = if on { [-90.6, -195.7, -195.7] } else { [-90.1, -90.0, -90.1] };
        for w in 0..3 {
            let a = SR / 4 + w * SR / 2;
            let tol = if on && w > 0 { INFO } else { TOT };
            t.row(name, &format!("silence window {} RMS", w + 1), stored_w[w], rms_db(&tail[a..a + SR / 2]), tol);
        }
        if on {
            let win = SR / 100;
            let last = (0..tail.len() / win).filter(|&k| tail[k * win..(k + 1) * win].iter().any(|v| *v != 0.0)).max();
            let end_ms = last.map(|k| ((k + 1) * 10) as f64).unwrap_or(0.0);
            t.row(name, "last 10 ms window with signal ends (ms)", 700.0, end_ms, 10.0);
            assert!(tail[SR..].iter().all(|v| *v == 0.0), "blanked output must be exact digital silence");
            assert!(m.is_blanked());
        }
    }
    t.finish();
}

/// Limit Peaks (SPEC_LIMITERS.md 68.5, 68.6; measurements/srcD_dither_switches3.json, _run2.json): dither noise
/// on 7 s of digital silence; noise RMS and the largest peak in quantisation steps, with the limit off and with
/// the limit set to the largest step count measured with it on.
#[test]
fn peak_limit_on_silence() {
    let x = vec![0.0; 7 * SR];
    // (name, word, amount, shaping, stored rms off, steps off, stored rms on, steps on)
    let rows = [
        ("16 Medium High", W16, AMT_D_MEDIUM, RISE4, -90.08, 4.0, -90.32, 2.0),
        ("16 Medium Medium", W16, AMT_D_MEDIUM, RISE3, -92.36, 3.0, -92.36, 2.0),
        ("16 Medium Higher", W16, AMT_D_MEDIUM, RISE5, -82.59, 10.0, -82.60, 8.0),
        ("24 Medium High", W24, AMT_D_MEDIUM, RISE4, -138.27, 4.0, -138.41, 3.0),
        ("16 Strong Max", W16, AMT_FULL, RISE6, -73.75, 28.0, -73.75, 28.0),
        ("16 Strong shaping Off", W16, AMT_FULL, S_OFF, -96.34, 1.0, -96.33, 1.0),
    ];
    let mut t = Table::new("Noise on silence and peak limit (source D)");
    for (name, word, amount, shaping, rms_off, st_off, rms_on, st_on) in rows {
        for (limit, rms, steps) in [(0.0, rms_off, st_off), (st_on, rms_on, st_on)] {
            let mut m = module(word, amount, shaping, 100.0);
            m.set_param(P_PEAK_LIMIT, limit);
            let y = render(&mut m, &x);
            let q = if word == W24 { 2f64.powi(-23) } else { 2f64.powi(-15) };
            let pk = y.iter().fold(0f64, |a, v| a.max(v.abs())) / q;
            let label = if limit > 0.0 { format!("{name}, limit {limit}") } else { format!("{name}, limit off") };
            t.row(&label, "noise RMS", rms, rms_db(&y), TOT);
            t.row(&label, "noise peak (steps)", steps as f64, pk.round(), INFO);
            if limit > 0.0 {
                assert!(pk <= limit as f64 + 1e-9, "{label}: peak {pk} steps over the limit");
            }
        }
    }
    t.finish();
}

/// DC filter (SPEC_LIMITERS.md 68.4; measurements/srcD_dither_switches2.json): a -40 dBFS tone with a DC offset of
/// 0.005 for 2 s, then silence. Output DC 0.5 to 1.75 s after the offset begins, and the RMS of the first three
/// half-second windows of silence (0.25 s after the audio stops), 16 / Medium / High.
#[test]
fn dc_filter() {
    let mut x: Vec<f64> = tone(-0.175, 2 * SR);
    x.extend(tone(-40.0, 2 * SR).iter().map(|v| v + 0.005));
    x.extend(std::iter::repeat(0.0).take(8 * SR));
    let mut t = Table::new("DC filter (source D)");
    for on in [false, true] {
        let mut m = module(W16, AMT_D_MEDIUM, RISE4, 100.0);
        m.set_param(P_DC_FILTER, if on { 1.0 } else { 0.0 });
        let y = render(&mut m, &x);
        let seg = &y[2 * SR + SR / 2..2 * SR + 7 * SR / 4];
        let dc = seg.iter().sum::<f64>() / seg.len() as f64;
        let name = if on { "16 Medium High + DC filter" } else { "16 Medium High" };
        let stored_dc = if on { 0.00011 } else { 0.005 };
        t.row(name, "output DC x 1e4", stored_dc * 1e4, dc * 1e4, 0.3);
        let stored_w: [f64; 3] = if on { [-62.2, -81.0, -89.8] } else { [-90.1, -90.0, -90.1] };
        for w in 0..3 {
            let a = 4 * SR + SR / 4 + w * SR / 2;
            t.row(name, &format!("silence window {} RMS", w + 1), stored_w[w], rms_db(&y[a..a + SR / 2]), TOT);
        }
    }
    t.finish();
}

/// Output may exceed full scale at low word lengths (SPEC_LIMITERS.md 68.4: 8 bits, Strong, Max on a 0.98 tone peaks
/// at +1.1 dBFS; measurements/srcD_dither_switches2.json). Informational, plus the 8-bit noise on silence.
#[test]
fn eight_bit_strong_max() {
    let mut x = tone(20.0 * 0.98f64.log10(), 2 * SR);
    x.extend(std::iter::repeat(0.0).take(4 * SR));
    let mut m = module(W8, AMT_FULL, RISE6, 100.0);
    let y = render(&mut m, &x);
    let pk = y[..2 * SR].iter().fold(0f64, |a, v| a.max(v.abs()));
    let over = y[..2 * SR].iter().filter(|v| v.abs() > 1.0).count();
    let mut t = Table::new("8 bits, Strong, Max (source D)");
    t.row("8 Strong Max, 0.98 tone", "sample peak dBFS", 1.143, 20.0 * pk.log10(), INFO);
    t.row("8 Strong Max, 0.98 tone", "samples above full scale", 3683.0, over as f64, INFO);
    t.row("8 Strong Max", "silence window RMS", -25.6, rms_db(&y[2 * SR + SR / 4..2 * SR + 3 * SR / 4]), TOT);
    t.finish();
}

/// Bit-activity readout (SPEC_LIMITERS.md 68.4): after 16-bit dither, bits 1 to 16 of the 24-bit word are in use
/// and bits 17 to 24 are not.
#[test]
fn bit_activity() {
    let x = tone(-20.0, SR);
    let mut m = module(W16, AMT_FULL, S_OFF, 100.0);
    let _ = render(&mut m, &x);
    assert_eq!(m.toggled_bits(), 0x00FF_FF00);
    assert_eq!(m.bits_in_use(), 16);
    m.clear_bit_activity();
    assert_eq!(m.toggled_bits(), 0);
    let mut m = module(W24, AMT_FULL, S_OFF, 100.0);
    let _ = render(&mut m, &x);
    assert_eq!(m.bits_in_use(), 24);
    println!("bit activity: 16-bit dither -> bits 1-16 toggle, 17-24 idle; 24-bit -> all 24");
}

/// Same start state in every new instance and after reset (sources A and B, SPEC_LIMITERS.md 65 and 70.2).
#[test]
fn deterministic_start() {
    let x = tone(-36.0, SR / 2);
    let mut a = module(W16, AMT_FULL, TILT1, 100.0);
    let mut b = module(W16, AMT_FULL, TILT1, 100.0);
    let ya = render(&mut a, &x);
    let yb = render(&mut b, &x);
    assert_eq!(ya, yb);
    a.reset();
    assert_eq!(render(&mut a, &x), ya);
    // the two channels carry different dither
    let mut c = module(W16, AMT_FULL, S_OFF, 100.0);
    let (l, r) = render2(&mut c, &x);
    assert!(l != r);
}

/// Parameter table sanity and clamping.
#[test]
fn params_and_clamping() {
    let m = requantiser::Requantiser::default();
    let p = m.params();
    assert_eq!(p.len(), 8);
    for d in p {
        assert!(d.min <= d.default && d.default <= d.max, "{}", d.id);
        println!("| {} | {} | {} .. {} | {} | {:?} |", p.iter().position(|x| x.id == d.id).unwrap(), d.id, d.min, d.max, d.default, d.kind);
    }
    let mut m = module(W16, 1e9, 99, -5.0);
    m.set_param(99, 1.0);
    m.set_param(P_WORD_LENGTH, f32::NAN);
    let mut l = vec![0.5f32; 64];
    let mut r = vec![-0.5f32; 64];
    m.process(&mut l, &mut r);
    assert!(l.iter().chain(&r).all(|v| v.is_finite()));
    assert_eq!(m.latency(), 0);
}
