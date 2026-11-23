//! Prints the measured-against-stored tables of REPORT.md.
//! Run: CARGO_TARGET_DIR=target cargo test --offline --release --test report -- --ignored --nocapture
mod common;
use common::*;
use limiter_modes::*;
use olm_api::Module;

fn f(v: Option<f64>) -> String {
    match v {
        None => "none".into(),
        Some(x) if x == f64::NEG_INFINITY => "at start".into(),
        Some(x) => format!("{x:.2}"),
    }
}

#[test]
#[ignore]
fn print_report_tables() {
    let x = burst_signal();
    println!("## modes");
    for k in 0..10 {
        for c in [0.0f32, 2.0, 5.0, 9.0, 10.0] {
            let mut m = make(k, c, 12.0);
            let lat = m.latency();
            let t = timing_modes(&gain_curve(&run(&mut m, &x), &x));
            println!("| {} | {} | {} | {:.2} | {:.2} | {} | {} | {} |", tables::STYLE_LABELS[k], c, lat, t.pre, t.fin, f(t.attack), f(t.release63), f(t.release90));
        }
    }
    println!("## types");
    for k in 10..13 {
        for c in [0.0f32, 2.0, 5.0, 9.0, 10.0] {
            let mut m = make(k, c, 12.0);
            let t = timing_types(&gain_curve(&run(&mut m, &x), &x));
            println!("| {} | {} | {:.2} | {:.3} | {} | {} | {} |", tables::STYLE_LABELS[k], c, t.pre, t.fin, f(t.attack), f(t.release63), f(t.release90));
        }
    }
    println!("## static, types");
    let s = static_steps();
    for k in 10..13 {
        for (drive, ceiling) in [(10.0f32, 0.0f32), (10.0, -6.0), (20.0, 0.0)] {
            let mut m = make(k, 2.0, drive);
            m.set_param(P_CEILING, ceiling);
            let y = run(&mut m, &s);
            let g: Vec<String> = static_gains(&y, &s).iter().map(|v| format!("{v:.2}")).collect();
            println!("| {} | drive {drive}, ceiling {ceiling} | {} | peak {:.3} |", tables::STYLE_LABELS[k], g.join(", "), db(peak(&y[8 * SR + SR / 2..]) as f64));
        }
    }
    println!("## static, first mode, drive 8");
    for c in [0.0f32, 2.0, 9.0] {
        let mut m = make(0, c, 8.0);
        let y = run(&mut m, &s);
        let g: Vec<String> = static_gains(&y, &s).iter().map(|v| format!("{v:.2}")).collect();
        println!("| character {c} | {} |", g.join(", "));
    }
    println!("## inter-sample tone");
    for (k, drive, level) in [(0usize, 12.0f32, -6.0f64), (9, 12.0, -6.0), (10, 6.0, 0.0), (11, 6.0, 0.0), (12, 6.0, 0.0)] {
        let tone = sine(12_000.0, level, core::f64::consts::FRAC_PI_4, 2 * SR);
        let mut m = make(k, 2.0, drive);
        let mut row = format!("| {} |", tables::STYLE_LABELS[k]);
        for tp in [0.0, 1.0] {
            m.set_param(P_TRUE_PEAK, tp);
            m.reset();
            let y = run(&mut m, &tone);
            let mid = &y[SR..SR + (1 << 14)];
            row += &format!(" {:.2} dBFS / {:.2} dBTP |", db(peak(mid) as f64), db(true_peak(mid)));
        }
        println!("{row}");
    }
    println!("## programme (own signal), first mode and types, drive 12: true peak off / on, RMS");
    let (xl, xr) = programme(4);
    for k in [0usize, 5, 8, 10, 11, 12] {
        for c in [2.0f32, 9.0] {
            let mut m = make(k, c, 12.0);
            let mut row = format!("| {} | {c} |", tables::STYLE_LABELS[k]);
            for tp in [0.0, 1.0] {
                m.set_param(P_TRUE_PEAK, tp);
                m.reset();
                let (l, _) = run_stereo(&mut m, &xl, &xr);
                row += &format!(" peak {:.2} dBFS, {:.2} dBTP, RMS {:.2} |", db(peak(&l) as f64), db(true_peak(&l[SR..])), rms_db(&l[SR / 10..]));
            }
            println!("{row}");
        }
    }
}
