//! Every parameter at min / max / default: finite, within the ceiling (+ the true-peak
//! tolerance) on noise. Block-size independence. Mid-Side on mono input. Gain match, view
//! and meters.

mod common;
use common::*;
use limiter_smart::{idx, view, SmartLimiter};
use olm_api::{Kind, Module};

#[test]
fn parameter_list_matches_the_spec() {
    let m = SmartLimiter::default();
    let p = m.params();
    let ids: Vec<&str> = p.iter().map(|d| d.id).collect();
    let want = [
        "engine", "drive", "ceiling", "release", "lookahead", "knee", "channelMode", "transientLink", "releaseLink", "truePeak", "autoRelease",
        "gainMatch", "delta", "bodyShare", "clipHeadroom", "clipKnee", "bandFocus", "crossoverMode", "slowMode", "crestHeadroom", "slowWindow",
        "bassHoldMax", "pauseHold", "joinWidth", "oversampling", "oversamplingOffline", "kWeighting", "split1", "split2", "split3",
    ];
    assert_eq!(ids, want);
    for d in p {
        println!("{:20} {:>8} .. {:>8} default {:>8} {:?}{}", d.id, d.min, d.max, d.default, d.kind, if d.log { " log" } else { "" });
        assert!(d.min <= d.default && d.default <= d.max);
        if let Kind::Choice(l) = d.kind {
            assert_eq!(l.len() as u32, d.steps);
        }
    }
    assert_eq!(p.len(), 30);
}

#[test]
fn every_parameter_at_min_max_default_is_finite_and_within_the_ceiling() {
    let n = 24000;
    let l = noise(n, 0.5, 41);
    let r = noise(n, 0.5, 42);
    let probe = SmartLimiter::default();
    let params = probe.params();
    let mut worst_sample = f64::NEG_INFINITY;
    let mut worst_tp = f64::NEG_INFINITY;
    let mut runs = 0;
    for i in 0..params.len() {
        let d = params[i];
        for v in [d.min, d.max, d.default] {
            for e in [0usize, 4] {
                if i == idx::ENGINE && e == 4 {
                    continue;
                }
                let mut sets = vec![engine(e), (idx::DRIVE, 12.0), (i, v)];
                if i == idx::ENGINE {
                    sets.remove(0);
                }
                let mut m = make(SR, &sets);
                let (yl, yr) = run(&mut m, &l, &r);
                assert!(yl.iter().chain(yr.iter()).all(|s| s.is_finite()), "{} = {v}: non-finite", d.id);
                runs += 1;
                if i == idx::DELTA && v > 0.5 {
                    continue; // delta outputs the removed signal
                }
                let c = m.param(idx::CEILING) as f64;
                let (a, b) = (&yl[4800..], &yr[4800..]);
                let sp = db(peak(a).max(peak(b))) - c;
                let tp = db(tp_sinc16(a).max(tp_sinc16(b))) - c;
                worst_sample = worst_sample.max(sp);
                worst_tp = worst_tp.max(tp);
                if sp > 0.001 || tp > 0.3 {
                    // reported: full-band white noise near Nyquist (see ceiling.rs)
                    println!("  {} = {v} (engine {}): sample {sp:+.4} dB, 16x {tp:+.3} dB re ceiling", d.id, ENGINE_NAMES[e]);
                }
            }
        }
    }
    println!("{runs} runs (each parameter at min, max and default, with Pure and Multiband, 12 dB drive on noise): worst sample peak {worst_sample:+.4} dB, worst 16x true peak {worst_tp:+.3} dB re ceiling");
    assert!(worst_sample <= 0.001);
    // Full-band white noise under heavy limiting: the reader cannot reconstruct what lies
    // above 22.5 kHz, so the 1x engines pass up to +0.43 dB (measured). Not a target.
    assert!(worst_tp <= 0.45);
}

#[test]
fn block_size_does_not_change_the_output() {
    let n = 30000;
    let mut l = noise(n, 0.4, 51);
    let r = noise(n, 0.4, 52);
    for k in (0..n).step_by(7000) {
        for j in 0..40 {
            l[k + j] += 1.5;
        }
    }
    for e in 0..6 {
        for ms in [0.0f32, 1.0] {
            let mut outs = Vec::new();
            for block in [1usize, 7, 64, 333, 4096] {
                let mut m = make(SR, &[engine(e), (idx::DRIVE, 9.0), (idx::CHANNEL_MODE, ms)]);
                outs.push(run_block(&mut m, &l, &r, block));
            }
            let mut worst = 0.0f64;
            for o in outs.iter().skip(1) {
                for i in 0..n {
                    worst = worst.max((o.0[i] - outs[0].0[i]).abs()).max((o.1[i] - outs[0].1[i]).abs());
                }
            }
            println!("{:9} {}: block sizes 1, 7, 64, 333, 4096: largest difference {:e}", ENGINE_NAMES[e], if ms > 0.5 { "M/S" } else { "L/R" }, worst);
            assert_eq!(worst, 0.0);
        }
    }
}

#[test]
fn mid_side_on_mono_input_is_mono_compatible() {
    let n = 24000;
    let x = noise(n, 0.5, 61);
    for e in 0..6 {
        for link in [100.0f32, 0.0] {
            let mut ms = make(SR, &[engine(e), (idx::DRIVE, 9.0), (idx::CHANNEL_MODE, 1.0), (idx::TRANSIENT_LINK, link), (idx::RELEASE_LINK, link)]);
            let mut st = make(SR, &[engine(e), (idx::DRIVE, 9.0), (idx::CHANNEL_MODE, 0.0), (idx::TRANSIENT_LINK, link), (idx::RELEASE_LINK, link)]);
            let (ml, mr) = run(&mut ms, &x, &x);
            let (sl, _) = run(&mut st, &x, &x);
            let lr = ml.iter().zip(mr.iter()).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
            let vs = ml.iter().zip(sl.iter()).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
            println!("{:9} link {link:3}%: Mid-Side on mono input: |L - R| max {lr:e}, against Stereo max {vs:.2e} ({:.1} dB re signal)", ENGINE_NAMES[e], db(vs / 0.5));
            assert_eq!(lr, 0.0, "mono in must stay mono");
            assert!(vs < 1e-5);
        }
    }
}

#[test]
fn gain_match_turns_the_drive_back_down_and_never_up() {
    let n = 48000 * 8;
    let x = noise(n, 0.1, 71);
    let y0 = x.clone();
    for drive in [0.0f32, 6.0, 12.0] {
        let mut m = make(SR, &[(idx::DRIVE, drive), (idx::GAIN_MATCH, 1.0)]);
        let (yl, _) = run(&mut m, &x, &y0);
        let rms_in = (x[n / 2..].iter().map(|v| v * v).sum::<f64>() / (n / 2) as f64).sqrt();
        let rms_out = (yl[n / 2..].iter().map(|v| v * v).sum::<f64>() / (n / 2) as f64).sqrt();
        let mut v = [0f32; view::LEN];
        m.view(&mut v);
        println!(
            "gain match, drive {drive:4} dB: output {:+.2} dB re input RMS, applied {:.2} dB (loudness in {:.2}, out {:.2} LUFS)",
            db(rms_out / rms_in),
            v[view::MATCH_GAIN],
            v[view::LOUDNESS_IN],
            v[view::LOUDNESS_OUT]
        );
        assert!(v[view::MATCH_GAIN] <= 0.0);
        assert!(db(rms_out / rms_in).abs() < 1.0);
    }
}

#[test]
fn view_and_meters_report_the_state() {
    for e in 0..6 {
        let mut m = make(SR, &[engine(e), (idx::DRIVE, 12.0), (idx::OVERSAMPLING, 1.0)]);
        let x = noise(24000, 0.3, 81);
        let _ = run(&mut m, &x, &x);
        let mut v = [0f32; view::LEN];
        assert_eq!(m.view(&mut v), view::LEN);
        let me = m.meters();
        println!(
            "{:9} view: engine {} OS {}x total {:.2} slow {:.2} fast {:.2} clip {:.2} trim {:.3} bands [{:.2} {:.2} {:.2} {:.2}] catcher {:.2} crest {:.1} dB release {:.0} ms ch {:.2}/{:.2} in {:.1} out {:.2} TP {:.2}; meter {:.2} dB",
            ENGINE_NAMES[e],
            v[view::ENGINE],
            v[view::OS_FACTOR],
            v[view::TOTAL_GAIN],
            v[view::SLOW_GAIN],
            v[view::FAST_GAIN],
            v[view::CLIP_GAIN],
            v[view::TRIM_GAIN],
            v[view::BAND_GAIN_0],
            v[view::BAND_GAIN_1],
            v[view::BAND_GAIN_2],
            v[view::BAND_GAIN_3],
            v[view::CATCHER_GAIN],
            v[view::CREST],
            v[view::RELEASE_MS],
            v[view::CH0_GAIN],
            v[view::CH1_GAIN],
            v[view::IN_PEAK_L],
            v[view::OUT_PEAK_L],
            v[view::TRUE_PEAK],
            me.reduction_db
        );
        assert_eq!(v[view::ENGINE] as usize, e);
        assert!(v[view::TOTAL_GAIN] < -1.0 && me.reduction_db > 1.0);
        assert!(v[view::OUT_PEAK_L] <= -0.2 + 0.001);
        assert_eq!(m.lift_db(), 0.0);
        if matches!(e, 0 | 2 | 3) {
            assert_eq!(v[view::OS_FACTOR], 8.0);
        }
    }
    // offline: the oversampling follows oversamplingOffline (16x by default), latency fixed
    let mut m = make(SR, &[]);
    let lat = m.latency();
    m.set_offline(true);
    let x = noise(48000, 0.3, 82);
    let _ = run(&mut m, &x, &x);
    let mut v = [0f32; view::LEN];
    m.view(&mut v);
    println!("offline: OS {}x, latency {} (online {})", v[view::OS_FACTOR], m.latency(), lat);
    assert_eq!(v[view::OS_FACTOR], 16.0);
    assert_eq!(m.latency(), lat);
    // the 768 kHz cap
    for (sr, want) in [(44100.0, 16.0), (96000.0, 8.0), (192000.0, 4.0)] {
        let mut m = make(sr, &[(idx::OVERSAMPLING, 2.0)]);
        let x = noise(4800, 0.3, 83);
        let _ = run(&mut m, &x, &x);
        m.view(&mut v);
        println!("16x requested at {sr} Hz: effective {}x", v[view::OS_FACTOR]);
        assert_eq!(v[view::OS_FACTOR], want);
    }
}
