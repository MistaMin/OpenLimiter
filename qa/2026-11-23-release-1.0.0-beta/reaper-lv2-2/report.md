# Plugin QA: LV2: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | LV2: OpenLimiter (OpenGrid) (4->2ch), 1985 parameters |
| finite output | PASS | peak -3.8 dBFS |
| not silent | PASS | rms -16.7 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x913, 0.0% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | ok (-1.9 dB) | ok (0.114%) |
| 1 | Gain R |  | ok (-1.9 dB) | ok (0.049%) |
| 2 | Threshold |  | ok (-4.5 dB) | ok (0.714%) |
| 3 | Soft Clip |  | ok (-15.6 dB) | ok (0.106%) |
| 4 | Release A | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.111%) |
| 5 | Release M | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.034%) |
| 6 | Auto Release | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.277%) |
| 7 | Output |  | ok (-3.8 dB) | ok (0.077%) |
| 8 | Delay L |  | ok (-2.0 dB) | ok (0.087%) |
| 9 | Delay R |  | ok (-0.7 dB) | ok (0.087%) |
| 10 | Delay Fine L |  | ok (-2.5 dB) | ok (0.087%) |
| 11 | Delay Fine R |  | ok (-0.6 dB) | ok (0.087%) |
| 12 | Phase Inv L |  | ok (2.3 dB) | ok (0.087%) |
| 13 | Phase Inv R |  | ok (1.0 dB) | ok (0.087%) |
| 14 | Delay Unit | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.034%) |
| 15 | Upsample | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.203%) |
| 16 | Link | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.203%) |
| 17 | Headroom | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.087%) |
| 18 | Dither |  | ok (-38.7 dB) | ok (0.034%) |
| 19 | No Latency | random state 1 | CONDITIONAL (-3.1 dB) | ok (0.187%) |
