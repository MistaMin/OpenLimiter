# Plugin QA: CLAP: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | CLAP: OpenLimiter (OpenGrid), 1984 parameters |
| finite output | PASS | peak -3.8 dBFS |
| not silent | PASS | rms -16.7 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x913, 0.4% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (8.126%) |
| 1 | Gain R |  | ok (-1.9 dB) | ok (7.657%) |
| 2 | Threshold |  | ok (-4.5 dB) | ok (5.896%) |
| 3 | Soft Clip |  | ok (-15.6 dB) | ok (5.807%) |
| 4 | Release A | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.921%) |
| 5 | Release M | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.671%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.613%) |
| 7 | Output |  | ok (-3.8 dB) | ok (5.501%) |
| 8 | Delay L |  | ok (-0.7 dB) | ok (6.109%) |
| 9 | Delay R |  | ok (-0.7 dB) | ok (6.142%) |
| 10 | Delay Fine L |  | ok (-0.6 dB) | ok (5.456%) |
| 11 | Delay Fine R |  | ok (-0.6 dB) | ok (5.607%) |
| 12 | Phase Inv L |  | ok (2.3 dB) | ok (5.607%) |
| 13 | Phase Inv R |  | ok (1.0 dB) | ok (5.578%) |
| 14 | Delay Unit | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.516%) |
| 15 | Upsample | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.526%) |
| 16 | Link | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.494%) |
| 17 | Headroom | random state 1 | CONDITIONAL (-4.6 dB) | ok (5.558%) |
| 18 | Dither |  | ok (-38.7 dB) | ok (5.537%) |
| 19 | No Latency |  | ok (-0.2 dB) | ok (5.909%) |
