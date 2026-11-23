//! Every type against OpenEQ 1.0.1's own design code (tests/data/ref_designs.txt, built by
//! tools/design_reference.cpp from the original headers).

use eq_core::response::design_band;
use eq_core::FilterType;

const FREQS: [f64; 16] = [20.0, 50.0, 100.0, 200.0, 300.0, 500.0, 700.0, 1000.0, 1500.0, 2000.0, 3000.0, 5000.0, 8000.0, 12000.0, 16000.0, 20000.0];

#[test]
fn all_types_match_the_original_designs() {
    let data = include_str!("data/ref_designs.txt");
    let mut worst = 0.0f64;
    let mut worst_case = String::new();
    let mut per_type = [0.0f64; 48];
    let mut cases = 0;
    for line in data.lines() {
        let (head, vals) = line.split_once('|').unwrap();
        let h: Vec<&str> = head.split_whitespace().collect();
        let ty: usize = h[0].parse().unwrap();
        let f: f32 = h[1].parse().unwrap();
        let g: f32 = h[2].parse().unwrap();
        let q: f32 = h[3].parse().unwrap();
        let slope: i32 = h[4].parse().unwrap();
        let sr: f64 = h[5].parse().unwrap();
        let stages: usize = h[6].parse().unwrap();
        let d = design_band(FilterType::from_index(ty), f, g, q, slope, sr);
        assert_eq!(d.stage_count, stages, "stage count type {ty} {line}");
        for (k, v) in vals.split_whitespace().enumerate() {
            let want: f64 = v.parse().unwrap();
            if want < -120.0 {
                continue; // below the measurement floor of the baselines
            }
            let got = 20.0 * d.magnitude(FREQS[k], sr).max(1e-15).log10();
            let e = (got - want).abs();
            per_type[ty] = per_type[ty].max(e);
            if e > worst {
                worst = e;
                worst_case = format!("type {ty} f {f} g {g} q {q} slope {slope} sr {sr} at {} Hz: {got} vs {want}", FREQS[k]);
            }
        }
        cases += 1;
    }
    for (t, e) in per_type.iter().enumerate() {
        println!("{:>2} {:<16} worst {:.2e} dB", t, FilterType::from_index(t).name(), e);
    }
    println!("{cases} cases, worst {worst:.3e} dB: {worst_case}");
    assert!(worst < 1e-3, "{worst_case}");
}
