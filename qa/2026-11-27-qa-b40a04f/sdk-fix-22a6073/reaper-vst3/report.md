# Plugin QA: VST3: OpenLimiter

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1985 parameters |
| finite output | PASS | peak -3.5 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | FAIL | repeat differs by -1.8 dB |
| editor opens | PASS | 1200x913, 4.1% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | skipped (render not deterministic)  | ok (1.301%) |
| 1 | Gain R |  | skipped (render not deterministic)  | ok (0.128%) |
| 2 | Threshold |  | skipped (render not deterministic)  | ok (1.130%) |
| 3 | Soft Clip |  | skipped (render not deterministic)  | ok (0.483%) |
| 4 | Release A |  | skipped (render not deterministic)  | ok (0.494%) |
| 5 | Release M |  | skipped (render not deterministic)  | ok (0.172%) |
| 6 | Auto Release |  | skipped (render not deterministic)  | ok (1.060%) |
| 7 | Output |  | skipped (render not deterministic)  | ok (2.244%) |
| 8 | Delay L |  | skipped (render not deterministic)  | ok (0.870%) |
| 9 | Delay R |  | skipped (render not deterministic)  | ok (0.872%) |
| 10 | Delay Fine L |  | skipped (render not deterministic)  | ok (0.847%) |
| 11 | Delay Fine R |  | skipped (render not deterministic)  | ok (0.847%) |
| 12 | Phase Inv L |  | skipped (render not deterministic)  | ok (0.822%) |
| 13 | Phase Inv R |  | skipped (render not deterministic)  | ok (0.813%) |
| 14 | Delay Unit |  | skipped (render not deterministic)  | ok (0.752%) |
| 15 | Upsample |  | skipped (render not deterministic)  | ok (0.775%) |
| 16 | Link |  | skipped (render not deterministic)  | ok (0.772%) |
| 17 | Headroom |  | skipped (render not deterministic)  | ok (0.824%) |
| 18 | Dither |  | skipped (render not deterministic)  | ok (1.116%) |
| 19 | No Latency |  | skipped (render not deterministic)  | ok (1.305%) |
| 20 | Classic Limiter On |  | skipped (render not deterministic)  | ok (0.957%) |
| 21 | Classic Limiter Position |  | skipped (render not deterministic)  | ok (1.004%) |
| 22 | Punch & Energy Curve |  | skipped (render not deterministic)  | ok (0.788%) |
| 23 | Punch & Energy Threshold |  | skipped (render not deterministic)  | ok (0.787%) |
