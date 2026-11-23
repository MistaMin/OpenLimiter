//! Band split: only the selected band is reduced, band solo, make-up on the band.
mod common;
use common::*;

const LOW: f32 = 0.0;
const MID: f32 = 1.0;
const HIGH: f32 = 2.0;
const FULL: f32 = 3.0;

/// The 24 tones of `measurements/scripts/srcB_monitor2.py` (30 Hz to 16 kHz, geometric), with the
/// phases numpy's `default_rng(3).uniform(0, 6.28)` gives, normalised to a -6 dBFS peak; 4 s.
const TONES: [f64; 24] = [
    30.0, 39.0, 52.0, 68.0, 89.0, 117.0, 154.0, 203.0, 266.0, 350.0, 460.0, 604.0, 794.0, 1043.0, 1371.0, 1801.0, 2367.0, 3110.0, 4086.0, 5369.0,
    7054.0, 9268.0, 12177.0, 16000.0,
];
const PHASES: [f64; 24] = [
    0.537877, 1.48717, 5.032004, 3.655978, 0.591128, 2.720037, 3.008442, 1.00316, 4.613145, 0.71386, 2.456913, 3.245128, 2.704344, 3.685095,
    4.633621, 6.005358, 1.784783, 4.072876, 4.372236, 1.838286, 0.009358, 6.113331, 1.87396, 1.971832,
];

fn multitone() -> Vec<f32> {
    let n = 4 * SR;
    let mut x = vec![0.0f64; n];
    for (f, ph) in TONES.iter().zip(PHASES.iter()) {
        for (i, v) in x.iter_mut().enumerate() {
            *v += (PI2 * f * i as f64 / SRF + ph).sin();
        }
    }
    let pk = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    x.iter().map(|v| (v * lin(-6.0) / pk) as f32).collect()
}

/// Level of every tone between 1 and 3 s, dBFS (`lev` in srcB_monitor2.py).
fn levels(y: &[f32]) -> Vec<f64> {
    TONES.iter().map(|f| db(tone_amp(y, *f, SR, 3 * SR))).collect()
}

#[test]
fn only_the_selected_band_is_reduced() {
    // measurements/srcB_features/filter_audio_path.json (spec section 32): 100 Hz and 5 kHz together,
    // each at -12 dBFS; threshold -30, other controls at default; gain of each tone.
    let n = 6 * SR;
    let a = sine(100.0, lin(-12.0), 0.0, n);
    let b = sine(5000.0, lin(-12.0), 0.0, n);
    let x: Vec<f32> = a.iter().zip(b.iter()).map(|(p, q)| p + q).collect();
    let rows = [("full band", FULL, -10.84, -10.83), ("low band", LOW, -9.29, 0.0), ("high band", HIGH, 0.0, -9.63), ("mid band", MID, 0.0, 0.0)];
    let mut t = Table::new("Band split, two tones (filter_audio_path.json)");
    for (name, ft, g100, g5k) in rows {
        for key_filter in [0.0f32, 1.0] {
            let y = render(&mut with(&[("threshold1", -30.0), ("filterType1", ft), ("keyFilter", key_filter)]), &x);
            let ga = db(tone_amp(&y, 100.0, 2 * SR, 5 * SR)) - (-12.0);
            let gb = db(tone_amp(&y, 5000.0, 2 * SR, 5 * SR)) - (-12.0);
            let kf = if key_filter > 0.5 { "key filter on" } else { "key filter off" };
            t.level(&format!("{name}, {kf}: 100 Hz"), ga, g100, 0.2);
            t.level(&format!("{name}, {kf}: 5 kHz"), gb, g5k, 0.2);
        }
    }
    t.finish();
}

/// Share of a tone that goes through the band, from its gain and the full reduction of the case:
/// tone gain = |1 - share x (1 - g_full)|.
fn share(tone_db: f64, full_db: f64) -> f64 {
    (1.0 - lin(tone_db)) / (1.0 - lin(full_db))
}

#[test]
fn band_shape() {
    // measurements/srcB_features/filter_shape.json (spec section 34): threshold -48, 1000:1, hard knee,
    // 24-tone signal; reduction per tone. The stored file's top-level names are shuffled; the rows
    // below are identified by the labels read back from the host, as the spec text does.
    // The phases of the stored stimulus are not known, so the amount of full reduction differs;
    // what is compared is the share of each tone that is in the band (0 = untouched, 1 = fully reduced).
    let z = -0.0;
    #[rustfmt::skip]
    let rows: [(&str, Vec<(&str, f32)>, f64, [f64; 24]); 7] = [
        ("low band 415 Hz", vec![("filterType1", LOW)], -24.0,
         [-24.1, -24.1, -24.0, -24.1, -24.0, -24.0, -24.0, -23.9, -24.0, -16.4, -0.9, z, z, z, z, z, z, z, z, z, z, z, z, z]),
        ("high band 415 Hz", vec![("filterType1", HIGH)], -26.5,
         [z, z, z, z, z, z, z, z, z, -0.8, -16.4, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5, -26.5]),
        ("mid band 415 Hz, 2 oct", vec![("filterType1", MID)], -20.5,
         [z, z, z, z, z, z, -0.3, -4.6, -19.0, -20.5, -20.5, -17.9, -5.9, -0.1, z, z, z, z, z, z, z, z, z, z]),
        ("low band 41 Hz", vec![("filterType1", LOW), ("centerFreq1", 41.0)], -28.6,
         [-6.8, -4.7, -2.3, -0.7, -0.1, z, z, z, z, z, z, z, z, z, z, z, z, z, z, z, z, z, z, z]),
        ("low band 17740 Hz", vec![("filterType1", LOW), ("centerFreq1", 17740.0)], -28.55,
         [-28.6, -28.6, -28.6, -28.6, -28.6, -28.6, -28.6, -28.5, -28.6, -28.6, -28.5, -28.6, -28.5, -28.6, -28.5, -28.5, -28.5, -28.5, -28.5, -28.5, -28.5, -28.5, -28.5, -13.3]),
        ("mid band 415 Hz, 4 1/6 oct", vec![("filterType1", MID), ("bandwidth1", 4.0 + 1.0 / 6.0)], -24.0,
         [0.0, z, -0.1, -0.8, -4.0, -13.9, -23.9, -24.0, -24.0, -24.0, -24.0, -24.0, -23.9, -22.0, -14.3, -4.8, -0.3, z, z, z, z, z, z, z]),
        ("mid band 415 Hz, 8 oct", vec![("filterType1", MID), ("bandwidth1", 8.0)], -26.8,
         [-3.0, -5.2, -9.8, -17.3, -25.4, -26.8, -26.8, -26.7, -26.8, -26.8, -26.8, -26.8, -26.8, -26.8, -26.8, -26.7, -26.7, -26.7, -24.2, -13.7, -3.3, -0.1, z, z]),
    ];
    let x = multitone();
    let src = levels(&x);
    let mut t = Table::new("Band shape, share of each tone in the band (filter_shape.json)");
    for (name, settings, full_want, want) in rows.iter() {
        let mut s = vec![("threshold1", -48.0), ("ratio1", 1000.0), ("softKnee1", 0.0)];
        s.extend_from_slice(settings);
        let y = render(&mut with(&s), &x);
        let got: Vec<f64> = levels(&y).iter().zip(src.iter()).map(|(a, b)| a - b).collect();
        let full_got = got.iter().cloned().fold(0.0f64, f64::min);
        if *name == "low band 41 Hz" {
            // Only the skirt of the band reaches the lowest tones here, so there is no fully
            // reduced tone to scale by: the five lowest tone gains are compared in dB instead.
            for k in 0..5 {
                t.level(&format!("{name}: gain of the {} Hz tone", TONES[k]), got[k], want[k], 1.0);
            }
            continue;
        }
        let mut worst = 0.0f64;
        let mut worst_f = 0.0;
        for k in 0..24 {
            let d = share(got[k], full_got) - share(want[k], *full_want);
            if d.abs() > worst.abs() {
                worst = d;
                worst_f = TONES[k];
            }
        }
        // 0.12 of the tone's amplitude: about 1 dB where a tone is half in the band.
        t.level(&format!("{name}: worst share error of 24 tones (at {worst_f} Hz)"), worst, 0.0, 0.12);
        t.truth(&format!("{name}: full reduction"), full_got < -15.0, &format!("{full_got:.1} dB here, {full_want} dB stored (different tone phases)"));
    }
    // Full band: every tone reduced equally (stored -28.7 / -28.6 on all 24).
    let y = render(&mut with(&[("threshold1", -48.0), ("ratio1", 1000.0), ("softKnee1", 0.0)]), &x);
    let got: Vec<f64> = levels(&y).iter().zip(src.iter()).map(|(a, b)| a - b).collect();
    let (lo, hi) = got.iter().fold((0.0f64, -200.0f64), |(l, h), v| (l.min(*v), h.max(*v)));
    t.level("full band: spread of the 24 tone gains", hi - lo, 0.0, 0.15);
    // Bandwidth has no effect on the low band.
    let a = render(&mut with(&[("threshold1", -48.0), ("ratio1", 1000.0), ("softKnee1", 0.0), ("filterType1", LOW), ("bandwidth1", 1.0 / 6.0)]), &x);
    let b = render(&mut with(&[("threshold1", -48.0), ("ratio1", 1000.0), ("softKnee1", 0.0), ("filterType1", LOW), ("bandwidth1", 8.0)]), &x);
    t.truth("bandwidth does nothing on the low band", a == b, "bit-identical");
    t.finish();
}

#[test]
fn band_solo_and_makeup_on_the_band() {
    // measurements/float_check/srcB_monitor2.json (spec sections 67, 67.1): threshold -30, ratio 5.18:1,
    // hard knee, centre 415 Hz, 2 octaves; the exact stored stimulus (phases known).
    let x = multitone();
    let base = [("threshold1", -30.0), ("ratio1", 5.18), ("softKnee1", 0.0)];
    let run = |extra: &[(&str, f32)]| {
        let mut s = base.to_vec();
        s.extend_from_slice(extra);
        render(&mut with(&s), &x)
    };
    let mut t = Table::new("Band solo and make-up (srcB_monitor2.json)");
    // "level_off_dbfs": tone levels in the normal output: tones in the band, tones outside it
    for (name, ft, in_band, in_want, out_band, out_want) in
        [("low band", LOW, 3usize, -36.2, 15usize, -29.0), ("mid band", MID, 9, -33.5, 20, -29.0), ("high band", HIGH, 15, -38.3, 3, -29.0), ("full band", FULL, 9, -40.1, 20, -40.0)]
    {
        let off = levels(&run(&[("filterType1", ft)]));
        let on = levels(&run(&[("filterType1", ft), ("bandSolo", 1.0)]));
        // 0.5 dB: the level in the band depends on the detector's reading of a 9-to-24-tone sum
        t.level(&format!("{name}: tone in the band ({} Hz), normal output", TONES[in_band]), off[in_band], in_want, 0.5);
        t.level(&format!("{name}: tone outside ({} Hz), normal output", TONES[out_band]), off[out_band], out_want, 0.5);
        t.level(&format!("{name}: solo changes the tone in the band by"), on[in_band] - off[in_band], 0.0, 0.05);
        if ft != FULL {
            let ch = on[out_band] - off[out_band];
            t.truth(&format!("{name}: solo removes the tone outside"), ch < -55.0, &format!("{ch:.1} dB (stored -78 dB or lower)"));
        } else {
            let a = run(&[("filterType1", ft)]);
            let b = run(&[("filterType1", ft), ("bandSolo", 1.0)]);
            t.truth("full band: solo output is identical", a == b, "bit-identical (stored -300 dB difference)");
        }
    }
    // Per-tone change with solo across the band edges: stored "level_change_per_tone_db".
    for (name, ft, cells) in [
        ("low band", LOW, vec![(9usize, -1.8), (10, -25.8)]),
        ("mid band", MID, vec![(7, -9.6), (8, -0.3), (11, -0.5), (12, -7.6)]),
        ("high band", HIGH, vec![(9, -29.3), (10, -2.7)]),
    ] {
        let off = levels(&run(&[("filterType1", ft)]));
        let on = levels(&run(&[("filterType1", ft), ("bandSolo", 1.0)]));
        for (k, want) in cells {
            // 1.5 dB: these tones sit on the steep edge, where 1 dB is a few Hz of edge position
            t.level(&format!("{name}: solo change at {} Hz", TONES[k]), on[k] - off[k], want, 1.5);
        }
    }
    // Make-up acts on the band only (67.1): mid band, +7 dB.
    let lv = |y: &[f32]| db(rms(y));
    let solo0 = lv(&run(&[("filterType1", MID), ("bandSolo", 1.0)]));
    let solo7 = lv(&run(&[("filterType1", MID), ("bandSolo", 1.0), ("makeup1", 7.0)]));
    let out0 = lv(&run(&[("filterType1", MID)]));
    let out7 = lv(&run(&[("filterType1", MID), ("makeup1", 7.0)]));
    let solo_og = lv(&run(&[("filterType1", MID), ("bandSolo", 1.0), ("outputGain", 1.5)]));
    t.level("mid band: solo RMS (stored -30.2 dBFS)", solo0, -30.2, 0.5);
    t.level("mid band: normal RMS (stored -19.06 dBFS)", out0, -19.06, 0.3);
    t.level("make-up +7: solo band rises by", solo7 - solo0, 7.0, 0.05);
    t.level("make-up +7: normal output rises by", out7 - out0, 1.31, 0.2);
    t.level("output gain +1.5: solo band rises by", solo_og - solo0, 1.5, 0.05);
    t.finish();
}
