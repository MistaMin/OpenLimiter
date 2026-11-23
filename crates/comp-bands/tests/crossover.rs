//! Crossover: flat recombination and band shapes (source F table file, section 5).
//! Signal and analysis: the dense multitone and the complex single-bin responses of the
//! source F signal and round 2 scripts in `measurements/scripts/`.
mod common;
use common::*;
use comp_bands::*;
use olm_api::Module;

fn responses(m: &mut BandCompressor) -> (Vec<f64>, Vec<(f64, f64)>) {
    let freqs = multitone_freqs();
    let s = multitone(&freqs);
    let y = render_mono(m, &s);
    let h = tone_response(&y, &s, &freqs);
    (freqs, h)
}

fn solo_band(b: usize) -> BandCompressor {
    let mut m = module();
    nothing_compresses(&mut m);
    set_band(&mut m, b, B_SOLO, 1.0);
    m
}

/// Section 5: "compressor on, nothing compressing, all bands at makeup 0 dB: 0.0 dB (+/-0.05)
/// from 20 Hz to 16 kHz". Reference: `recon_from_parts_db` in the source F data folder,
/// `xover_bands.json` (0.0 at every tone). Required here: within 0.05 dB from 20 Hz to 20 kHz.
#[test]
fn bands_sum_flat() {
    for mode in [MODE_DOWNWARD, MODE_PARALLEL, MODE_PARALLEL_UNLINKED] {
        let mut m = module();
        nothing_compresses(&mut m);
        m.set_param(P_MODE, mode);
        let (freqs, h) = responses(&mut m);
        let worst = h.iter().map(|v| mag_db(*v).abs()).fold(0.0, f64::max);
        println!("flat sum, mode {mode}: worst deviation {worst:.5} dB over {} tones", freqs.len());
        assert!(worst < 0.05, "mode {mode}: {worst} dB");
    }
}

/// The recombination is exact in the time domain as well: the output is the delayed input.
#[test]
fn neutral_is_a_pure_delay() {
    let mut m = module();
    nothing_compresses(&mut m);
    let s = pink(7);
    let y = render_mono(&mut m, &s[..96000]);
    let err: Vec<f32> = y.iter().zip(s.iter()).map(|(a, b)| a - b).collect();
    let rel = db(rms(&err)) - db(rms(&s[..96000]));
    println!("neutral residual {rel:.1} dB relative to the signal");
    assert!(rel < -120.0);
}

/// Section 5 table "Level at 20 Hz / 107 Hz / 431 Hz / 996 Hz / 4 kHz / 16.2 kHz" (data:
/// `xover_bands.json`, `bands_db`). Tolerance 0.15 dB (the table is rounded to 0.1 dB); 1.5 dB
/// for entries below -40 dB, which the table gives without decimals.
#[test]
fn band_shapes_match_the_table() {
    let at = [20.0, 107.0, 431.0, 996.0, 4000.0, 16200.0];
    let table: [[f64; 6]; 5] = [
        [-0.1, -2.4, -11.1, -18.1, -30.4, -46.8],
        [-16.0, -3.9, -2.8, -7.0, -18.4, -34.8],
        [-44.0, -17.4, -4.2, -2.0, -7.2, -22.7],
        [-84.0, -42.9, -17.6, -8.2, -1.9, -10.3],
        [-200.0, -81.0, -44.0, -27.1, -8.5, -0.5],
    ];
    // Peak level, -3 dB span and -6 dB span from the same table (Hz; 0 = open end).
    let peaks = [-0.1, -1.9, -2.0, -1.9, -0.1];
    let span3 = [(0.0, 123.0), (93.0, 702.0), (402.0, 2835.0), (1620.0, 9950.0), (8660.0, 0.0)];
    let span6 = [(0.0, 215.0), (57.0, 1067.0), (265.0, 4309.0), (1070.0, 13200.0), (5310.0, 0.0)];
    for b in 0..BANDS {
        let mut m = solo_band(b);
        let (freqs, h) = responses(&mut m);
        let dbs: Vec<f64> = h.iter().map(|v| mag_db(*v)).collect();
        let mut line = String::new();
        for (k, f) in at.iter().enumerate() {
            let got = dbs[nearest(&freqs, *f)];
            let want = table[b][k];
            line += &format!(" {got:.2}/{want}");
            if want <= -100.0 {
                assert!(got < -100.0, "band {b} at {f} Hz: {got}");
            } else {
                let tol = if want < -40.0 { 1.5 } else { 0.15 };
                assert!((got - want).abs() <= tol, "band {b} at {f} Hz: {got} against {want}");
            }
        }
        let pk = dbs.iter().cloned().fold(f64::MIN, f64::max);
        let inside = |drop: f64| -> (f64, f64) {
            let idx: Vec<usize> = (0..dbs.len()).filter(|i| dbs[*i] >= pk - drop).collect();
            (freqs[idx[0]], freqs[*idx.last().unwrap()])
        };
        let (a3, b3) = inside(3.0);
        let (a6, b6) = inside(6.0);
        println!("band {b}: got/table{line}; peak {pk:.2} (table {}), -3 dB {a3:.0}..{b3:.0}, -6 dB {a6:.0}..{b6:.0}", peaks[b]);
        assert!((pk - peaks[b]).abs() < 0.15);
        // Span edges fall on tones of the grid; allow one grid step (7.2 %).
        let close = |got: f64, want: f64| want == 0.0 || (got / want).ln().abs() < 0.075;
        assert!(close(a3, span3[b].0) && close(b3, span3[b].1), "band {b} -3 dB span {a3}..{b3}");
        assert!(close(a6, span6[b].0) && close(b6, span6[b].1), "band {b} -6 dB span {a6}..{b6}");
    }
}

/// Section 5: "Magnitude-only band + Mid renders agree: e.g. Lo + Mid has a -12.1 dB notch at
/// 246 Hz". This needs the phases right, not only the magnitudes.
#[test]
fn two_bands_together_show_the_measured_notch() {
    let mut m = module();
    nothing_compresses(&mut m);
    set_band(&mut m, 0, B_SOLO, 1.0);
    set_band(&mut m, 2, B_SOLO, 1.0);
    let (freqs, h) = responses(&mut m);
    let got = mag_db(h[nearest(&freqs, 246.5)]);
    println!("low + mid at 246 Hz: {got:.2} dB (measured -12.1)");
    assert!((got + 12.1).abs() < 0.15);
}

/// Complex responses of two bands together with a split moved or switched off, against the
/// stored complex data (source F data folder, `round2/jobs_s*.json`, keys "xover cx | ...",
/// fields H_re / H_im). Tolerance 2e-4 on the complex value.
///
/// Note: the table file (section 5) says the low-mid band with its upper split at 1.06 kHz is
/// "-10 dB at 2.3 kHz" with a -6 dB span up to 1.14 kHz. That figure comes from subtracting the
/// default mid band from a render whose mid band had moved with the split; the complex data
/// themselves are matched here (display "1.06 kHz" = 1063.7 Hz).
#[test]
fn moved_and_switched_off_splits_match_the_complex_data() {
    let cases: [(&str, usize, usize, f32, [(f64, f64, f64); 7]); 3] = [
        ("low-mid + mid, split 2 at 1.06 kHz", 1, P_SPLIT_2, 1063.7, [
            (20.00, 0.024965, 0.156032), (106.75, 0.424333, 0.496076), (431.00, 0.994269, 0.244188), (995.50, 1.124393, -0.143133),
            (2299.50, 0.748728, -0.584689), (4018.50, 0.356313, -0.554857), (16222.50, 0.010223, -0.111571)]),
        ("low + mid, split 1 off", 0, P_SPLIT_1, SPLIT_LOW_OFF_HZ, [
            (20.00, 0.001993, 0.039902), (106.75, 0.054247, 0.201250), (431.00, 0.508862, 0.385469), (995.50, 0.800840, 0.003927),
            (2299.50, 0.512739, -0.384357), (4018.50, 0.237424, -0.365750), (16222.50, 0.006702, -0.072958)]),
        ("high + mid, split 4 off", 4, P_SPLIT_4, SPLIT_HIGH_OFF_HZ, [
            (20.00, -0.006175, 0.001307), (106.75, -0.076507, 0.111663), (431.00, 0.366298, 0.491678), (995.50, 0.787959, 0.102729),
            (2299.50, 0.531927, -0.355660), (4018.50, 0.248308, -0.358205), (16222.50, 0.007035, -0.072925)]),
    ];
    for (name, band, param, value, want) in cases {
        let mut m = module();
        nothing_compresses(&mut m);
        set_band(&mut m, band, B_SOLO, 1.0);
        set_band(&mut m, 2, B_SOLO, 1.0);
        m.set_param(param, value);
        let (freqs, h) = responses(&mut m);
        let mut worst = 0.0f64;
        for (f, re, im) in want {
            let got = h[nearest(&freqs, f)];
            worst = worst.max(((got.0 - re).powi(2) + (got.1 - im).powi(2)).sqrt());
        }
        println!("{name}: worst complex error {worst:.2e}");
        assert!(worst < 2.0e-4, "{name}: {worst}");
    }
}

/// Section 5, crossover "Off": with the lowest split off the low band carries nothing and the
/// lows pass through the next band; with the highest split off the high band is silent. The sum
/// stays flat whatever the splits are.
#[test]
fn switched_off_splits_empty_their_band_and_keep_the_sum_flat() {
    let mut m = solo_band(0);
    m.set_param(P_SPLIT_1, SPLIT_LOW_OFF_HZ);
    let (_, h) = responses(&mut m);
    assert!(h.iter().all(|v| mag_db(*v) < -150.0), "low band not empty");

    let mut m = solo_band(1);
    m.set_param(P_SPLIT_1, SPLIT_LOW_OFF_HZ);
    let (freqs, h) = responses(&mut m);
    let low = mag_db(h[nearest(&freqs, 20.0)]);
    println!("lowest split off: low-mid band at 20 Hz {low:.2} dB");
    assert!(low.abs() < 0.05);

    let mut m = solo_band(4);
    m.set_param(P_SPLIT_4, SPLIT_HIGH_OFF_HZ);
    let (_, h) = responses(&mut m);
    assert!(h.iter().all(|v| mag_db(*v) < -150.0), "high band not silent");

    // Any order of splits, some off: still flat.
    let mut m = module();
    nothing_compresses(&mut m);
    m.set_param(P_SPLIT_1, 3000.0);
    m.set_param(P_SPLIT_2, SPLIT_LOW_OFF_HZ);
    m.set_param(P_SPLIT_3, 150.0);
    m.set_param(P_SPLIT_4, SPLIT_HIGH_OFF_HZ);
    let (_, h) = responses(&mut m);
    assert!(h.iter().all(|v| mag_db(*v).abs() < 0.05));
}
