# Plugin QA: VST3: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -3.5 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1200x913, 7.5% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (5.257%) |
| 1 | Gain R |  | ok (-1.8 dB) | ok (4.890%) |
| 2 | Threshold |  | ok (-4.2 dB) | ok (6.488%) |
| 3 | Soft Clip |  | ok (-14.8 dB) | ok (6.129%) |
| 4 | Release A | random state 1 | CONDITIONAL (-8.4 dB) | ok (6.486%) |
| 5 | Release M | random state 1 | CONDITIONAL (-8.4 dB) | ok (6.080%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-8.4 dB) | ok (6.992%) |
| 7 | Output |  | ok (-3.5 dB) | ok (8.015%) |
| 8 | Delay L |  | ok (-0.9 dB) | ok (7.223%) |
| 9 | Delay R |  | ok (-0.9 dB) | ok (7.397%) |
| 10 | Delay Fine L |  | ok (-0.4 dB) | ok (7.530%) |
| 11 | Delay Fine R |  | ok (-0.4 dB) | ok (7.852%) |
| 12 | Phase Inv L |  | ok (2.5 dB) | ok (8.029%) |
| 13 | Phase Inv R |  | ok (1.3 dB) | ok (8.218%) |
| 14 | Delay Unit | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.394%) |
| 15 | Upsample | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.472%) |
| 16 | Link | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.630%) |
| 17 | Headroom | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.861%) |
| 18 | Dither |  | ok (-38.7 dB) | ok (8.905%) |
| 19 | No Latency |  | ok (-2.5 dB) | ok (9.264%) |
| 20 | Classic Limiter On |  | ok (-2.5 dB) | ok (9.229%) |
| 21 | Classic Limiter Position | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.432%) |
| 22 | Punch & Energy Curve | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.574%) |
| 23 | Punch & Energy Threshold | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.701%) |
