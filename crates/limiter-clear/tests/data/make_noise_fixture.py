"""Writes the 4 s gated-noise section of the hot programme signal as 32-bit float, little endian.
The three lines that build `prog` are the signal definition from the measurement script
the measurement script that writes the composite hot test signal read by float_ceiling_check.py.
Usage: python3 make_noise_fixture.py   -> noise_prog_f32le.bin (192000 samples, 48 kHz)"""
import os
import numpy as np
SR = 48000
def lin(d): return 10**(d/20)
t = lambda n: np.arange(n)/SR
rng = np.random.default_rng(1); nz = rng.standard_normal(4*SR); env = 0.15 + 0.85*(np.sin(2*np.pi*2*t(4*SR)) > 0.6)
prog = nz*env; prog *= lin(-1)/np.max(np.abs(prog))
prog.astype("<f4").tofile(os.path.join(os.path.dirname(os.path.abspath(__file__)), "noise_prog_f32le.bin"))
