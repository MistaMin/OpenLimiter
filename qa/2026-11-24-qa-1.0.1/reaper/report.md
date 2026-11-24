# Plugin QA: VST3: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -1.4 dBFS |
| not silent | PASS | rms -6.8 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1200x913, 0.4% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (11.892%) |
| 1 | Gain R |  | ok (-1.9 dB) | ok (10.707%) |
| 2 | Threshold |  | ok (-2.0 dB) | ok (11.314%) |
| 3 | Soft Clip |  | ok (-9.1 dB) | ok (9.450%) |
| 4 | Release A | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.287%) |
| 5 | Release M | random state 2 | CONDITIONAL (-7.6 dB) | ok (8.912%) |
| 6 | Auto Release | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.494%) |
| 7 | Output |  | ok (-1.4 dB) | ok (10.470%) |
| 8 | Delay L |  | ok (-5.1 dB) | ok (9.041%) |
| 9 | Delay R |  | ok (-8.2 dB) | ok (9.108%) |
| 10 | Delay Fine L |  | ok (-7.6 dB) | ok (8.527%) |
| 11 | Delay Fine R |  | ok (-11.7 dB) | ok (8.982%) |
| 12 | Phase Inv L |  | ok (4.6 dB) | ok (9.124%) |
| 13 | Phase Inv R |  | ok (1.2 dB) | ok (9.012%) |
| 14 | Delay Unit | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.083%) |
| 15 | Upsample | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.228%) |
| 16 | Link | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.183%) |
| 17 | Headroom | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.107%) |
| 18 | Dither |  | ok (-38.7 dB) | ok (9.057%) |
| 19 | No Latency |  | ok (-12.1 dB) | ok (9.471%) |
| 20 | Classic Limiter On |  | ok (-7.6 dB) | ok (9.224%) |
| 21 | Classic Limiter Position | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.044%) |
| 22 | Punch & Energy Curve | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.054%) |
| 23 | Punch & Energy Threshold | random state 2 | CONDITIONAL (-7.6 dB) | ok (9.029%) |
