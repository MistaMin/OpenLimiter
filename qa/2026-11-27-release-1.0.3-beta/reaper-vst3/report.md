# Plugin QA: VST3: OpenLimiter

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -3.5 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1200x913, 0.3% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (5.470%) |
| 1 | Gain R |  | ok (-1.8 dB) | ok (5.000%) |
| 2 | Threshold |  | ok (-4.2 dB) | ok (6.872%) |
| 3 | Soft Clip |  | ok (-14.8 dB) | ok (6.303%) |
| 4 | Release A | random state 1 | CONDITIONAL (-8.4 dB) | ok (6.580%) |
| 5 | Release M | random state 1 | CONDITIONAL (-8.4 dB) | ok (6.654%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-8.4 dB) | ok (7.825%) |
| 7 | Output |  | ok (-3.5 dB) | ok (8.866%) |
| 8 | Delay L |  | ok (-0.9 dB) | ok (7.795%) |
| 9 | Delay R |  | ok (-0.9 dB) | ok (8.118%) |
| 10 | Delay Fine L |  | ok (-0.4 dB) | ok (8.338%) |
| 11 | Delay Fine R |  | ok (-0.4 dB) | ok (8.505%) |
| 12 | Phase Inv L |  | ok (2.5 dB) | ok (8.733%) |
| 13 | Phase Inv R |  | ok (1.3 dB) | ok (8.997%) |
| 14 | Delay Unit | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.122%) |
| 15 | Upsample | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.197%) |
| 16 | Link | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.337%) |
| 17 | Headroom | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.295%) |
| 18 | Dither |  | ok (-38.7 dB) | ok (9.805%) |
| 19 | No Latency |  | ok (-1.7 dB) | ok (9.490%) |
| 20 | Classic Limiter On |  | ok (-1.7 dB) | ok (9.282%) |
| 21 | Classic Limiter Position | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.015%) |
| 22 | Punch & Energy Curve | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.432%) |
| 23 | Punch & Energy Threshold | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.995%) |
