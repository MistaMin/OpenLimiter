#!/usr/bin/env python3
"""Writes tests/reference.rs from the stored multitone EQ responses of source F.

Reads only data files: measurements/srcF/round1_all.json and
measurements/srcF/round5/jobs_s0.json (field resp_L_db, 100 tones). The tone grid is the one
built by measurements/scripts/srcF_signals.py (100 log-spaced tones, 20 Hz to 20 kHz, on a
0.25 Hz grid). Run: python3 -I tools/make_reference.py  (from the crate folder).

Set values written with "=x" in the job scripts were found by bisection on the display text,
which lands on the lower edge of the display step (measurements/scripts/srcF_lib.py, set()):
"=6" dB -> 5.95 dB, "=-6" -> -6.05, "=80" Hz -> 79.5 Hz, "=5000" -> 4995 Hz, and so on. Those
edge values are used below. Values set as normalised numbers use the displayed value.
"""
import json, os, sys
import numpy as np

M = os.environ.get("SRCF_DIR") or sys.exit("set SRCF_DIR to the srcF measurement folder")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "data", "reference.rs")

def band(n, typ, freq=None, gain=None, width=None, depth=None, sfx=""):
    p = [(f"b{n}Type{sfx}", typ)]
    if freq is not None: p.append((f"b{n}Freq{sfx}", freq))
    if gain is not None: p.append((f"b{n}Gain{sfx}", gain))
    if width is not None: p.append((f"b{n}Width{sfx}", width))
    if depth is not None: p.append((f"b{n}Depth{sfx}", depth))
    return p

BELL, REJ, LS3, LS6, LS12, HS3, HS6, HS12, HP, LP = range(10)
R1, R5 = "round1_all.json", "round5/jobs_s0.json"
# (file, key, own label, settings, tolerance dB)
CASES = [
    (R1, "eq | all bands default (0 dB)", "flat defaults", [], 0.01),
    (R1, "eq | LO1 shelf 3 dB/oct +12", "low shelf 3 dB/oct 50 Hz +12", band(1, LS3, 50, 12), 0.10),
    (R5, "eq | LO1 shelf 3 dB/oct 200 Hz +12", "low shelf 3 dB/oct 199.5 Hz +12", band(1, LS3, 199.5, 12), 0.10),
    (R5, "eq | LO1 shelf 3 dB/oct -12", "low shelf 3 dB/oct 50 Hz -12", band(1, LS3, 50, -12), 0.10),
    (R1, "eq | LO1 shelf 6 dB/oct +12", "low shelf 6 dB/oct 50 Hz +12", band(1, LS6, 50, 12), 0.10),
    (R1, "eq | LO1 shelf 12 dB/oct +12", "low shelf 12 dB/oct 50 Hz +12", band(1, LS12, 50, 12), 0.10),
    (R1, "eq | HI2 shelf 3 dB/oct +12", "high shelf 3 dB/oct 14 kHz +12", band(6, HS3, 14000, 12), 0.60),
    (R1, "eq | HI2 shelf 12 dB/oct +12", "high shelf 12 dB/oct 14 kHz +12", band(6, HS12, 14000, 12), 0.10),
    (R5, "eq | HI2 shelf 12 dB/oct 5 kHz +12", "high shelf 12 dB/oct 4995 Hz +12", band(6, HS12, 4995, 12), 0.10),
    (R1, "eq | LO1 Cut Butterworth", "high-pass 50 Hz", band(1, HP, 50), 0.10),
    (R1, "eq | LO1 Cut Butterworth, gain +12", "high-pass 50 Hz, gain +12 ignored", band(1, HP, 50, 12), 0.10),
    (R1, "eq | HI2 Cut Butterworth", "low-pass 14 kHz", band(6, LP, 14000), 0.10),
    (R5, "eq | HI2 Cut 5 kHz", "low-pass 4995 Hz", band(6, LP, 4995), 0.10),
    (R1, "eq | LO1 Notch 0.25 oct, notch gain -100", "reject 50 Hz 0.25 oct -100", band(1, REJ, 50, None, 0.25, -100), 0.10),
    (R1, "eq | LO1 Notch 0.25 oct, notch gain -20", "reject 50 Hz 0.25 oct -20.05", band(1, REJ, 50, None, 0.25, -20.05), 0.10),
    (R1, "eq | LO1 Notch 0.25 oct, gain +12 (notch gain 0)", "reject depth 0, gain +12 ignored", band(1, REJ, 50, 12, 0.25, 0), 0.01),
    (R1, "eq | MID1 Notch 0.80 oct, notch gain -100", "reject 500 Hz 0.80 oct -100", band(3, REJ, 500, None, 0.8, -100), 0.10),
    (R5, "eq | LO2 Notch 0.16 oct -100", "reject 150 Hz 0.16 oct -100", band(2, REJ, 150, None, 0.16, -100), 0.10),
    (R5, "eq | HI1 Notch 0.16 oct -100", "reject 6 kHz 0.16 oct -100", band(5, REJ, 6000, None, 0.16, -100), 0.15),
    (R1, "eq | MID1 ParEQ 2.00 oct +12", "bell 500 Hz 2 oct +12", band(3, BELL, 500, 12, 2.0), 0.10),
    (R1, "eq | MID1 ParEQ 2.00 oct -12", "bell 500 Hz 2 oct -12", band(3, BELL, 500, -12, 2.0), 0.10),
    (R1, "eq | MID1 ParEQ 0.10 oct +12", "bell 500 Hz 0.10 oct +12", band(3, BELL, 500, 12, 0.1), 0.10),
    (R1, "eq | MID1 ParEQ 0.80 oct +12", "bell 500 Hz 0.80 oct +12", band(3, BELL, 500, 12, 0.8), 0.10),
    (R1, "eq | MID1 ParEQ 4.00 oct +12", "bell 500 Hz 4.00 oct +12", band(3, BELL, 500, 12, 4.0), 0.10),
    (R1, "eq | MID1 ParEQ 2.00 oct +12, enable off", "bell +12 with band off", band(3, BELL, 500, 12, 2.0) + [("b3On", 0)], 0.01),
    (R1, "eq | LO1 ParEQ 1.00 oct +12", "bell 50 Hz 1.00 oct +12", band(1, BELL, 50, 12, 1.0), 0.10),
    (R5, "eq | LO1 ParEQ 0.20 oct +12", "bell 50 Hz 0.20 oct +12", band(1, BELL, 50, 12, 0.2), 0.10),
    (R1, "eq | LO2 ParEQ 2.00 oct +12", "bell 150 Hz 2 oct +12", band(2, BELL, 150, 12, 2.0), 0.10),
    (R5, "eq | LO2 +6 (150 Hz 2 oct)", "bell 150 Hz 2 oct +5.95", band(2, BELL, 150, 5.95, 2.0), 0.10),
    (R5, "eq | LO2 80 Hz +12", "bell 79.5 Hz 2 oct +12", band(2, BELL, 79.5, 12, 2.0), 0.10),
    (R5, "eq | LO2 0.63 oct +12", "bell 150 Hz 0.63 oct +12", band(2, BELL, 150, 12, 0.63), 0.10),
    (R1, "eq | MID2 ParEQ 2.00 oct +12", "bell 2 kHz 2 oct +12", band(4, BELL, 2000, 12, 2.0), 0.10),
    (R5, "eq | MID2 -6 (2 kHz 2 oct)", "bell 2 kHz 2 oct -6.05", band(4, BELL, 2000, -6.05, 2.0), 0.10),
    (R5, "eq | MID2 5 kHz +12", "bell 4995 Hz 2 oct +12", band(4, BELL, 4995, 12, 2.0), 0.10),
    (R5, "eq | MID2 0.10 oct +12", "bell 2 kHz 0.10 oct +12", band(4, BELL, 2000, 12, 0.1), 0.10),
    (R5, "eq | MID2 4.00 oct +12", "bell 2 kHz 4.00 oct +12", band(4, BELL, 2000, 12, 4.0), 0.10),
    (R5, "eq | MID1 2 kHz +12", "bell 1995 Hz 2 oct +12 (band 3)", band(3, BELL, 1995, 12, 2.0), 0.10),
    (R1, "eq | HI1 ParEQ 2.00 oct +12", "bell 6 kHz 2 oct +12", band(5, BELL, 6000, 12, 2.0), 0.15),
    (R5, "eq | HI1 +6 (6 kHz 2 oct)", "bell 6 kHz 2 oct +5.95", band(5, BELL, 6000, 5.95, 2.0), 0.15),
    (R5, "eq | HI1 10 kHz +12", "bell 9995 Hz 2 oct +12", band(5, BELL, 9995, 12, 2.0), 0.40),
    (R5, "eq | HI1 0.63 oct +12", "bell 6 kHz 0.63 oct +12", band(5, BELL, 6000, 12, 0.63), 0.10),
    (R1, "eq | MID1 +12, EQ unlinked (R/S MID1 0 dB)", "unlinked, set 1 bell +12 on L", band(3, BELL, 500, 12, 2.0) + [("linkSets", 0)], 0.10),
    (R1, "eq | MID1 R/S +12, EQ unlinked", "unlinked, set 2 bell +12, L flat", band(3, BELL, 500, 12, 2.0, sfx="Ch2") + [("linkSets", 0)], 0.01),
    (R1, "eq | MID1 R/S +12, EQ linked", "linked, set 2 ignored, L flat", band(3, BELL, 500, 12, 2.0, sfx="Ch2"), 0.01),
    (R1, "lo cut | value 0.1", "low cut 2 Hz", [("lowCutOn", 1), ("lowCutFreq", 2)], 0.10),
    (R1, "lo cut | value 0.5", "low cut 16 Hz", [("lowCutOn", 1), ("lowCutFreq", 16)], 0.10),
    (R1, "lo cut | value 0.75", "low cut 56 Hz", [("lowCutOn", 1), ("lowCutFreq", 56)], 0.10),
    (R1, "lo cut | value 1.0", "low cut 200 Hz", [("lowCutOn", 1), ("lowCutFreq", 200)], 0.10),
    (R5, "lo cut | 10 Hz", "low cut 9.5 Hz", [("lowCutOn", 1), ("lowCutFreq", 9.5)], 0.10),
    (R5, "lo cut | 30 Hz", "low cut 29.5 Hz", [("lowCutOn", 1), ("lowCutFreq", 29.5)], 0.10),
    (R5, "lo cut | 100 Hz", "low cut 99.5 Hz", [("lowCutOn", 1), ("lowCutFreq", 99.5)], 0.10),
    (R5, "lo cut | 150 Hz", "low cut 149.5 Hz", [("lowCutOn", 1), ("lowCutFreq", 149.5)], 0.10),
]

def main():
    fr = np.unique(np.round(np.geomspace(20, 20000, 100) * 4) / 4)
    data = {f: json.load(open(os.path.join(M, f))) for f in (R1, R5)}
    o = ["// Generated by tools/make_reference.py. Do not edit by hand.",
         "// Stored magnitude responses of source F's EQ and low cut, 48 kHz, left channel, dB,",
         "// at the 100 multitone tones (built as in measurements/scripts/srcF_signals.py).",
         "",
         "pub struct RefCase {",
         "    pub label: &'static str,",
         "    pub settings: &'static [(&'static str, f32)],",
         "    pub tol_db: f64,",
         "    pub db: [f64; 100],",
         "}",
         "",
         "pub const FREQS: [f64; 100] = [" + ", ".join(f"{f:.2f}" for f in fr) + "];",
         "",
         "pub const CASES: &[RefCase] = &["]
    for f, key, label, sets, tol in CASES:
        r = data[f][key]["resp_L_db"]
        vals = list(r.values()); assert len(vals) == 100
        o.append(f"    // measurements/srcF/{f}, key \"{key}\", field resp_L_db")
        s = ", ".join(f"(\"{i}\", {float(v):.4f})" for i, v in sets)
        o.append(f"    RefCase {{ label: \"{label}\", settings: &[{s}], tol_db: {tol}, db: [" + ", ".join(f"{v:.2f}" for v in vals) + "] },")
    o.append("];")
    open(OUT, "w").write("\n".join(o) + "\n")
    print("wrote", OUT, len(CASES), "cases")

if __name__ == "__main__":
    main()
