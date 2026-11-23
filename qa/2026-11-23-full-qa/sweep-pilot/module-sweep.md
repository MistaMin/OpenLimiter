# OpenLimiter host sweep, one module at a time

VST3: OpenLimiter (OpenGrid), 2 workers, 2 s renders, 0.4 min, 1985 host parameters

| Module | Result counts |
|---|---|
| shaper | CONDITIONAL 5, ok 4 |
| monitor | ok 2 |

## Not ok

| # | Parameter | Module | Result | Detail |
|---|---|---|---|---|
| 23 | shaper.threshold | shaper | CONDITIONAL | state 2, -6.4 dB |
| 25 | shaper.mix | shaper | CONDITIONAL | state 2, -5.6 dB |
| 26 | shaper.oversample | shaper | CONDITIONAL | state 2, -6.3 dB |
| 28 | shaper.upwardThreshold | shaper | CONDITIONAL | state 1, -14.0 dB |
| 30 | shaper.on | shaper | CONDITIONAL | state 1, -12.9 dB |
