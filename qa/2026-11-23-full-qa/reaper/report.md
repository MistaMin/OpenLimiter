# Plugin QA: VST3: OpenLimiter (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenLimiter (OpenGrid), 1983 parameters |
| finite output | PASS | peak -3.8 dBFS |
| not silent | PASS | rms -14.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x913, 0.0% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 1898 | Floor Lifter Threshold | Floor Lifter On=1 | ok (-3.9 dB) | ok (6.415%) |
| 1899 | Floor Lifter Ratio | Floor Lifter On=1random state 5 | CONDITIONAL (-49.4 dB) | ok (5.773%) |
| 1900 | Floor Lifter Range | Floor Lifter On=1random state 5 | CONDITIONAL (-49.4 dB) | ok (6.523%) |
| 1901 | Floor Lifter Knee | Floor Lifter On=1 | ok (-6.0 dB) | ok (6.585%) |
| 1902 | Floor Lifter Floor | Floor Lifter On=1random state 3 | CONDITIONAL (-7.6 dB) | ok (6.365%) |
| 1903 | Floor Lifter Attack | Floor Lifter On=1 | ok (-5.4 dB) | ok (6.505%) |
| 1904 | Floor Lifter Hold | Floor Lifter On=1random state 3 | CONDITIONAL (-3.8 dB) | ok (6.446%) |
| 1905 | Floor Lifter Release | Floor Lifter On=1random state 4 | CONDITIONAL (-2.5 dB) | ok (6.524%) |
| 1906 | Floor Lifter Detector | Floor Lifter On=1random state 3 | CONDITIONAL (-7.6 dB) | ok (6.596%) |
| 1907 | Floor Lifter RMS Window | Floor Lifter On=1random state 5 | CONDITIONAL (-54.6 dB) | ok (6.671%) |
| 1908 | Floor Lifter Look-Ahead | Floor Lifter On=1 | ok (-4.3 dB) | ok (6.740%) |
| 1909 | Floor Lifter Stereo Link | Floor Lifter On=1random state 5 | CONDITIONAL (-69.5 dB) | ok (6.651%) |
| 1910 | Floor Lifter Key Source | Floor Lifter On=1random state 3 | CONDITIONAL (-88.4 dB) | ok (6.443%) |
| 1911 | Floor Lifter Key High-Pass | Floor Lifter On=1 | ok (-14.9 dB) | ok (6.595%) |
| 1912 | Floor Lifter Key Low-Pass | Floor Lifter On=1 | ok (-17.1 dB) | ok (6.456%) |
| 1913 | Floor Lifter Mix | Floor Lifter On=1random state 5 | CONDITIONAL (-49.4 dB) | ok (6.701%) |
| 1914 | Floor Lifter On | random state 1 | CONDITIONAL (-78.1 dB) | ok (6.416%) |
| 1915 | Floor Lifter Position |  | excused (-240.0 dB) | ok (5.875%) |
