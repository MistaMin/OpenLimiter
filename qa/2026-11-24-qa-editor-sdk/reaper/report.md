# Plugin QA: VST3: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -3.5 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1200x913, 0.6% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (6.090%) |
| 1 | Gain R |  | ok (-1.8 dB) | ok (5.680%) |
| 2 | Threshold |  | ok (-4.2 dB) | ok (7.453%) |
| 3 | Soft Clip |  | ok (-14.8 dB) | ok (6.906%) |
| 4 | Release A | random state 1 | CONDITIONAL (-8.4 dB) | ok (7.222%) |
| 5 | Release M | random state 1 | CONDITIONAL (-8.4 dB) | ok (6.598%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-8.4 dB) | ok (7.560%) |
| 7 | Output |  | ok (-3.5 dB) | ok (8.845%) |
| 8 | Delay L |  | ok (-0.9 dB) | ok (7.574%) |
| 9 | Delay R |  | ok (-0.9 dB) | ok (7.807%) |
| 10 | Delay Fine L |  | ok (-0.4 dB) | ok (7.928%) |
| 11 | Delay Fine R |  | ok (-0.4 dB) | ok (8.244%) |
| 12 | Phase Inv L |  | ok (2.5 dB) | ok (8.305%) |
| 13 | Phase Inv R |  | ok (1.3 dB) | ok (8.482%) |
| 14 | Delay Unit | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.591%) |
| 15 | Upsample | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.675%) |
| 16 | Link | random state 1 | CONDITIONAL (-8.4 dB) | ok (8.948%) |
| 17 | Headroom | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.035%) |
| 18 | Dither |  | ok (-38.7 dB) | ok (9.256%) |
| 19 | No Latency |  | ok (-2.5 dB) | ok (9.681%) |
| 20 | Classic Limiter On |  | ok (-1.7 dB) | ok (9.647%) |
| 21 | Classic Limiter Position | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.645%) |
| 22 | Punch & Energy Curve | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.823%) |
| 23 | Punch & Energy Threshold | random state 1 | CONDITIONAL (-8.4 dB) | ok (9.996%) |
