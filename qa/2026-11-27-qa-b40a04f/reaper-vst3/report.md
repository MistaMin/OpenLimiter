# Plugin QA: VST3: OpenLimiter

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -3.5 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1200x913, 4.1% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (4.139%) |
| 1 | Gain R |  | ok (-1.8 dB) | ok (3.935%) |
| 2 | Threshold |  | ok (-4.2 dB) | ok (5.841%) |
| 3 | Soft Clip |  | ok (-14.8 dB) | ok (6.050%) |
| 4 | Release A | random state 1 | CONDITIONAL (-12.0 dB) | ok (6.016%) |
| 5 | Release M | random state 1 | CONDITIONAL (-3.8 dB) | ok (5.606%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-12.0 dB) | ok (5.707%) |
| 7 | Output |  | ok (-3.5 dB) | ok (6.690%) |
| 8 | Delay L |  | ok (-1.9 dB) | ok (5.476%) |
| 9 | Delay R |  | ok (-0.9 dB) | ok (4.680%) |
| 10 | Delay Fine L |  | ok (-1.6 dB) | ok (5.407%) |
| 11 | Delay Fine R |  | ok (-0.4 dB) | ok (4.649%) |
| 12 | Phase Inv L |  | ok (2.5 dB) | ok (5.282%) |
| 13 | Phase Inv R |  | ok (1.3 dB) | ok (5.704%) |
| 14 | Delay Unit | random state 2 | CONDITIONAL (1.3 dB) | ok (5.694%) |
| 15 | Upsample |  | ok (-1.8 dB) | ok (5.396%) |
| 16 | Link |  | ok (-1.7 dB) | ok (5.885%) |
| 17 | Headroom | random state 2 | CONDITIONAL (4.5 dB) | ok (5.349%) |
| 18 | Dither |  | ok (-6.0 dB) | ok (4.907%) |
| 19 | No Latency | random state 2 | CONDITIONAL (-3.0 dB) | ok (6.034%) |
| 20 | Classic Limiter On | random state 1 | CONDITIONAL (-7.3 dB) | ok (5.650%) |
| 21 | Classic Limiter Position | random state 2 | CONDITIONAL (1.8 dB) | ok (5.503%) |
| 22 | Punch & Energy Curve | random state 2 | CONDITIONAL (5.1 dB) | ok (5.719%) |
| 23 | Punch & Energy Threshold | random state 1 | CONDITIONAL (-12.0 dB) | ok (5.463%) |
