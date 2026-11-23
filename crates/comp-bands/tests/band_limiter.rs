//! The per-band peak limiter. This stage is the project's own addition (requested by the
//! project owner); nothing here is compared with a measurement.
mod common;
use common::*;
use comp_bands::*;
use olm_api::Module;

fn neutral() -> BandCompressor {
    let mut m = module();
    nothing_compresses(&mut m);
    m
}

#[test]
fn off_by_default_and_adds_latency_only_when_on() {
    let mut m = module();
    assert_eq!(m.param(P_BAND_LIMITER), 0.0);
    assert_eq!(m.params()[P_BAND_LIMITER].default, 0.0);
    let base = m.latency();
    assert_eq!(base, 720); // 15 ms at 48 kHz
    m.set_param(P_BAND_LIMITER, 1.0);
    assert_eq!(m.latency(), base + 71); // 1.5 ms window, less one sample
    m.set_param(P_BAND_LIMITER, 0.0);
    assert_eq!(m.latency(), base);
}

/// Each band, soloed, never exceeds its ceiling; a band below its ceiling is not touched.
#[test]
fn each_band_is_held_at_its_ceiling() {
    let x: Vec<f32> = pink(11)[..3 * SRU].iter().map(|v| v * 8.0).collect(); // about -2 dBFS RMS, clipping peaks
    for b in 0..BANDS {
        let unlimited = {
            let mut m = neutral();
            set_band(&mut m, b, B_SOLO, 1.0);
            render_mono(&mut m, &x)
        };
        let band_peak = db(peak(&unlimited));
        let ceiling = (band_peak - 9.0).max(-24.0) as f32;
        let mut m = neutral();
        set_band(&mut m, b, B_SOLO, 1.0);
        set_band(&mut m, b, B_CEILING, ceiling);
        m.set_param(P_BAND_LIMITER, 1.0);
        let y = render_mono(&mut m, &x);
        let got = db(peak(&y));
        let meter = m.meters().band_reduction_db[b];
        println!("band {b}: unlimited peak {band_peak:.2} dBFS, ceiling {ceiling:.2}, limited peak {got:.2}, RMS change {:.2} dB", db(rms(&y)) - db(rms(&unlimited)));
        assert!(got <= ceiling as f64 + 1e-4, "band {b} over its ceiling");
        assert!(got > ceiling as f64 - 1.0, "band {b} far under its ceiling");
        assert!(meter >= 0.0);
    }
}

/// With the ceilings out of reach the limiter only delays: same signal, 71 samples later.
#[test]
fn transparent_below_the_ceiling() {
    let x = pink(5);
    let x = &x[..2 * SRU];
    let mut a = neutral();
    let ya = render_mono(&mut a, x);
    let mut b = neutral();
    b.set_param(P_BAND_LIMITER, 1.0);
    for k in 0..BANDS {
        set_band(&mut b, k, B_CEILING, 0.0);
    }
    let yb = render_mono(&mut b, x);
    let err: Vec<f32> = ya.iter().zip(yb.iter()).map(|(p, q)| p - q).collect();
    println!("limiter on, nothing limiting: residual {:.1} dB", db(rms(&err)) - db(rms(x)));
    assert!(db(rms(&err)) - db(rms(x)) < -110.0);
    assert_eq!(b.meters().reduction_db, 0.0);
}

/// A steady tone over the ceiling settles exactly at the ceiling, and the gain comes back with
/// the shared release time after the tone drops.
#[test]
fn tone_settles_at_the_ceiling_and_releases() {
    let n = 2 * SRU;
    let x: Vec<f32> = (0..n)
        .map(|i| {
            let a = if i < SRU { lin(-6.0) } else { lin(-30.0) };
            (a * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / SR as f64).sin()) as f32
        })
        .collect();
    let mut m = neutral();
    set_band(&mut m, 2, B_SOLO, 1.0);
    set_band(&mut m, 2, B_CEILING, -20.0);
    m.set_param(P_BAND_LIMITER, 1.0);
    m.set_param(P_BAND_LIMITER_RELEASE, 50.0);
    let y = render_mono(&mut m, &x);
    let held = db(peak(&y[SRU / 2..SRU - 480]));
    println!("mid band of a -6 dBFS 1 kHz tone (-8 dBFS in the band), ceiling -20: peak {held:.3} dBFS");
    assert!((held + 20.0).abs() < 0.05);
    // After the drop the band is at -32 dBFS; the gain is back within 0.5 dB after 4 release times.
    let late = db(rms(&y[SRU + 9600..SRU + 14400])) + 3.01;
    let settled = db(rms(&y[2 * SRU - 4800..])) + 3.01;
    println!("after the drop: {late:.2} dBFS peak at 200 to 300 ms, {settled:.2} at the end");
    assert!((late - settled).abs() < 0.5);
    assert!((settled + 32.0).abs() < 0.1);
}
