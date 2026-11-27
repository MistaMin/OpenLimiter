# Plugin QA: VST3: OpenLimiter

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -3.5 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1200x913, 10.0% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (3.815%) |
| 1 | Gain R |  | ok (-1.8 dB) | ok (3.911%) |
| 2 | Threshold |  | ok (-4.2 dB) | ok (3.422%) |
| 3 | Soft Clip |  | ok (-6.0 dB) | ok (2.634%) |
| 4 | Release A |  | ok (-6.0 dB) | ok (3.838%) |
| 5 | Release M |  | ok (-5.2 dB) | ok (3.405%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-5.3 dB) | ok (5.021%) |
| 7 | Output |  | ok (-3.5 dB) | ok (3.675%) |
| 8 | Delay L |  | ok (-1.9 dB) | ok (4.050%) |
| 9 | Delay R |  | ok (-0.9 dB) | ok (3.320%) |
| 10 | Delay Fine L |  | ok (-1.6 dB) | ok (4.124%) |
| 11 | Delay Fine R |  | ok (-0.4 dB) | ok (3.765%) |
| 12 | Phase Inv L |  | ok (2.5 dB) | ok (3.262%) |
| 13 | Phase Inv R |  | ok (1.3 dB) | ok (4.195%) |
| 14 | Delay Unit |  | ok (-3.2 dB) | ok (4.994%) |
| 15 | Upsample | random state 1 | CONDITIONAL (-2.3 dB) | ok (4.677%) |
| 16 | Link |  | ok (-6.0 dB) | ok (4.729%) |
| 17 | Headroom | random state 1 | CONDITIONAL (-10.4 dB) | ok (5.059%) |
| 18 | Dither |  | ok (-2.8 dB) | ok (3.967%) |
| 19 | No Latency | random state 1 | CONDITIONAL (-12.0 dB) | ok (4.778%) |
| 20 | Classic Limiter On | random state 1 | CONDITIONAL (-5.2 dB) | ok (5.220%) |
| 21 | Classic Limiter Position | random state 1 | CONDITIONAL (-2.3 dB) | ok (4.774%) |
| 22 | Punch & Energy Curve | random state 1 | CONDITIONAL (-5.4 dB) | ok (4.974%) |
| 23 | Punch & Energy Threshold | random state 1 | CONDITIONAL (-7.7 dB) | ok (2.816%) |
