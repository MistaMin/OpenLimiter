# Plugin QA: CLAP: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | CLAP: OpenLimiter (OpenGrid), 1984 parameters |
| finite output | PASS | peak -3.8 dBFS |
| not silent | PASS | rms -16.7 dBFS |
| deterministic render | FAIL | repeat differs by -6.0 dB |
| editor opens | PASS | 1080x913, 0.4% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Gain L |  | skipped (render not deterministic)  | ok (0.012%) |
| 1 | Gain R |  | skipped (render not deterministic)  | ok (0.024%) |
| 2 | Threshold |  | skipped (render not deterministic)  | ok (0.024%) |
| 3 | Soft Clip |  | skipped (render not deterministic)  | ok (0.024%) |
| 4 | Release A |  | skipped (render not deterministic)  | ok (0.024%) |
| 5 | Release M |  | skipped (render not deterministic)  | ok (0.024%) |
| 6 | Auto Release |  | skipped (render not deterministic)  | ok (0.024%) |
| 7 | Output |  | skipped (render not deterministic)  | ok (0.024%) |
| 8 | Delay L |  | skipped (render not deterministic)  | ok (0.024%) |
| 9 | Delay R |  | skipped (render not deterministic)  | ok (0.024%) |
| 10 | Delay Fine L |  | skipped (render not deterministic)  | ok (0.024%) |
| 11 | Delay Fine R |  | skipped (render not deterministic)  | ok (0.024%) |
| 12 | Phase Inv L |  | skipped (render not deterministic)  | ok (0.024%) |
| 13 | Phase Inv R |  | skipped (render not deterministic)  | ok (0.024%) |
| 14 | Delay Unit |  | skipped (render not deterministic)  | ok (0.024%) |
| 15 | Upsample |  | skipped (render not deterministic)  | ok (0.024%) |
| 16 | Link |  | skipped (render not deterministic)  | ok (0.024%) |
| 17 | Headroom |  | skipped (render not deterministic)  | ok (0.024%) |
| 18 | Dither |  | skipped (render not deterministic)  | ok (0.024%) |
| 19 | No Latency |  | skipped (render not deterministic)  | ok (0.024%) |
