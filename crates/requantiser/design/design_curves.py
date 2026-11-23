"""Designs the noise-shaping curves of the requantiser and writes ../src/curves.rs.

Each curve is a minimum-phase FIR noise transfer function NTF(z) = 1 + c1 z^-1 + ... + cN z^-N applied to the total
requantisation error (rounding plus dither) by error feedback. Method:
  1. A target log-power profile is a step per band (levels L_b in dB), smoothed over 300 Hz.
  2. The minimum-phase FIR for that profile is built by the folded-cepstrum method and truncated to N taps,
     then scaled so its first tap is 1 (this enforces the mean-log-gain-is-zero law of error feedback).
  3. Levenberg-Marquardt moves the levels L_b until the band means of |NTF|^2 of the truncated FIR equal the
     stored band level divided by the level flat TPDF dither of the same amount gives in that band.
  The lowest band (0-2 kHz for sources A and D, 0-4 kHz for source H) was not measurable and is left free (tiny
  weight); it takes whatever level the other bands leave it.

Run from the crate folder:  python3 design/design_curves.py      (numpy only). Designed at 48 kHz.
"""
import numpy as np, os

FS = 48000.0
K = 1 << 14
FG = np.arange(K // 2 + 1) * FS / K

def flat_total_db(amount, bits=16):
    q = 2.0 ** -(bits - 1)
    return 10 * np.log10(q * q * (1 / 12 + amount * amount / 6))

def band_db(total_db, lo, hi):
    return total_db + 10 * np.log10((hi - lo) / (FS / 2))

def minphase(logpow_db, n):
    c = np.fft.irfft(logpow_db * np.log(10) / 20, K)
    fold = np.zeros(K); fold[0] = c[0]; fold[1:K // 2] = 2 * c[1:K // 2]; fold[K // 2] = c[K // 2]
    h = np.fft.irfft(np.exp(np.fft.rfft(fold)), K)[:n]
    return h / h[0]

def bandmeans(h, bands):
    P = np.abs(np.fft.rfft(h, K)) ** 2
    return np.array([10 * np.log10(np.mean(P[(FG >= lo) & (FG < hi)])) for lo, hi in bands])

def profile(L, bands, sm=300.0):
    p = np.zeros_like(FG)
    for b, (lo, hi) in enumerate(bands): p[(FG >= lo) & (FG <= hi)] = L[b]
    m = max(1, int(sm / (FS / K))); ker = np.ones(m) / m
    return np.convolve(np.pad(p, m, mode="edge"), ker, "same")[m:-m]

def fit(bands, tg, n, free=0, wfree=1e-4):
    tg = np.array(tg, float)
    def res(L):
        h = minphase(profile(L, bands), n); got = bandmeans(h, bands)
        r = got - tg; r[free] *= wfree; return r, h, got
    L = tg.copy(); r, h, got = res(L); cost = r @ r; mu = 1e-2
    for it in range(150):
        J = np.zeros((len(r), len(L)))
        for j in range(len(L)):
            d = np.zeros(len(L)); d[j] = 1e-3; J[:, j] = (res(L + d)[0] - r) / 1e-3
        A = J.T @ J; step = np.linalg.solve(A + mu * np.diag(np.diag(A) + 1e-9), -J.T @ r)
        r2, h2, got2 = res(L + step); c2 = r2 @ r2
        if np.isfinite(c2) and c2 < cost: L, r, h, got, cost = L + step, r2, h2, got2, c2; mu = max(mu / 3, 1e-7)
        else: mu *= 5
        if cost < 1e-10 or mu > 1e9: break
    return h[1:], got

A_BANDS = [(0, 2000), (2000, 4000), (4000, 12000), (12000, 16000), (16000, 20000), (20000, 24000)]
H_BANDS = [(0, 4000), (4000, 12000), (12000, 16000), (16000, 20000), (20000, 24000)]

curves = []   # (rust name, label, source note, bands, targets, weights)

# Source A shaped types at 16 bits, absolute band levels (dBFS) for 2-4, 4-12, 12-16, 16-20, 20-24 kHz.
# Data: measurements/srcA_recheck/dither_float.json (SPEC_LIMITERS.md 56.4). Flat amount that matches its TPDF: 1.09.
A_AMT = 1.09
A_TABLE = {
    "tilt1": {50: [-112.7, -108.5, -99.4, -93.8, -92.3], 100: [-130.3, -102.3, -92.7, -88.5, -87.5]},
    "tilt2": {50: [-112.2, -107.6, -100.5, -95.4, -93.1], 100: [-130.2, -101.5, -94.5, -90.4, -88.3]},
    "top":   {50: [-111.4, -104.7, -108.4, -95.9, -90.1], 100: [-124.0, -103.2, -102.9, -89.9, -84.8]},
    "soft":  {50: [-111.4, -105.2, -101.6, -98.0, -96.3], 100: [-122.4, -102.7, -97.4, -93.9, -92.4]},
}
for key, label in (("tilt1", "Tilt 1"), ("tilt2", "Tilt 2"), ("top", "Top Octave"), ("soft", "Tilt Soft")):
    for depth in (50, 100):
        meas = A_TABLE[key][depth]
        tot = flat_total_db(A_AMT)
        tg = [m - band_db(tot, lo, hi) for m, (lo, hi) in zip(meas, A_BANDS[1:])]
        # 0-2 kHz was not measurable (tone skirt): free band
        curves.append((f"{key.upper()}_{depth}", label, depth, A_BANDS, [tg[0]] + tg, 128, 1e-4))

# Source D shaping strengths at 16 bits, dither amount Medium (modelled as 0.85 x full TPDF).
# Data: measurements/srcD_dither.json, rows "16 | Medium | shaping X" (SPEC_LIMITERS.md 68.3, 68.4).
D_AMT = 0.85
D_TABLE = [
    ("RISE1", "Rise 1", [-110.7, -105.2, -105.0, -101.4, -101.3]),   # Lowest
    ("RISE2", "Rise 2", [-113.3, -107.8, -105.7, -98.1, -98.1]),     # Low
    ("RISE3", "Rise 3", [-114.6, -109.5, -106.1, -95.8, -95.6]),     # Medium
    ("RISE4", "Rise 4", [-116.2, -110.9, -107.1, -93.5, -93.0]),     # High
    ("RISE5", "Rise 5", [-120.0, -114.8, -111.3, -86.6, -85.0]),     # Higher
    ("RISE6", "Rise 6", [-123.4, -118.1, -115.1, -80.3, -77.3]),     # Max
]
for name, label, meas in D_TABLE:
    tot = flat_total_db(D_AMT)
    tg = [m - band_db(tot, lo, hi) for m, (lo, hi) in zip(meas, A_BANDS[1:])]
    free, wf = tg[0], 1e-4
    if name == "RISE6":
        # Evidence for the 0-2 kHz band at Max: total noise on digital silence -73.75 dBFS at amount Strong
        # (SPEC_LIMITERS.md 68.5, measurements/srcD_dither_switches3.json). Strong to Medium is -0.89 dB in this
        # model, so Medium/Max totals about -74.64 dBFS against -75.5 dBFS in 2-24 kHz: 0-2 kHz holds about
        # -82.1 dBFS, i.e. +26.3 dB over flat dither of amount 0.85 in that band. Weight 0.1: a full-weight
        # fit spills the 0-2 kHz hump into 2-4 kHz with 256 taps; 0.1 keeps the measured bands exact.
        free, wf = 26.3, 0.1
    curves.append((name, label, 100, A_BANDS, [free] + tg, 256 if name == "RISE6" else 128, wf))

# Source H curves at 16 bits, full TPDF: silence totals and band shares (dB re total) of the tone residual.
# Data: measurements/srcH/results.json, "dither | quant 4 requant 1 shape 1..3" (params/srcH.md section 6).
# The 0-4 kHz share is dominated by an unexplained -99 dBFS residual, so 0-4 kHz is a free choice (low weight,
# aimed at the 4-12 kHz gain).
H_TABLE = [
    ("VALLEY1", "Valley 1", -90.2, [-9.0, -18.6, -7.5, -4.5, -4.9]),
    ("VALLEY2", "Valley 2", -84.4, [-14.6, -27.3, -12.2, -3.6, -3.3]),
    ("VALLEY3", "Valley 3", -78.5, [-20.5, -36.0, -21.8, -3.2, -2.9]),
]
for name, label, total, shares in H_TABLE:
    tot = flat_total_db(1.0)
    tg = [total + s - band_db(tot, lo, hi) for s, (lo, hi) in zip(shares, H_BANDS)]
    curves.append((name, label, 100, H_BANDS, tg, 128, 1e-4))

out = []
report = []
for name, label, depth, bands, tg, n, wf in curves:
    c, got = fit(bands, tg, n, wfree=wf)
    h = np.concatenate([[1.0], c]); peak = 20 * np.log10(np.abs(np.fft.rfft(h, K)).max())
    err = [g - t for g, t in zip(got, tg)]
    report.append(f"{name:10s} taps {n:3d}  free band {got[0]:+6.1f} dB  measured-band error dB " + " ".join(f"{e:+.2f}" for e in err[1:])
                  + f"  |NTF| peak {peak:.1f} dB")
    out.append((name, c))
print("\n".join(report))

here = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(here, "..", "src", "curves.rs"), "w") as f:
    f.write("// Generated by design/design_curves.py; do not edit by hand.\n")
    f.write("// Taps c1..cN of each noise transfer function NTF = 1 + sum c_k z^-k, fitted at 48 kHz.\n\n")
    for name, c in out:
        f.write(f"pub static {name}: [f64; {len(c)}] = [\n")
        for i in range(0, len(c), 4):
            f.write("    " + " ".join(f"{v:.17e}," for v in c[i:i + 4]) + "\n")
        f.write("];\n")
    f.write("\n// Design fit, for the record (free band = level of the unmeasured lowest band re flat):\n")
    for line in report:
        f.write("// " + line + "\n")
