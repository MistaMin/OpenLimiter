# OpenLimiter host sweep, one module at a time

VST3: OpenLimiter (OpenGrid), 4 workers, 2 s renders, 28.6 min, 1985 host parameters

| Module | Result counts |
|---|---|
| classic | CONDITIONAL 7, DEAD 1, non-audio 1, ok 12 |
| shaper | CONDITIONAL 5, ok 4 |
| modes | CONDITIONAL 5, DEAD 8, ok 5 |
| eq | CONDITIONAL 42, DEAD 23, ok 13 |
| sone | CONDITIONAL 6, ok 14 |
| clear | CONDITIONAL 3, ok 8 |
| dual | CONDITIONAL 39, ok 7 |
| wall | CONDITIONAL 7, ok 5 |
| bands | CONDITIONAL 10, DEAD 5, ok 45 |
| peq | CONDITIONAL 411, DEAD 14, ok 15 |
| deq | CONDITIONAL 412, DEAD 669, ok 10 |
| sup | CONDITIONAL 23, DEAD 1, ok 3 |
| hcomp | ok 29 |
| decomp | DEAD 1, ok 21 |
| deexp | CONDITIONAL 11, DEAD 1, ok 5 |
| harm | CONDITIONAL 1, ok 17 |
| smart | CONDITIONAL 5, ok 26 |
| requantise | CONDITIONAL 6, ok 2 |
| input | non-audio 2, ok 2 |
| monitor | ok 2 |

## Not ok

| # | Parameter | Module | Result | Detail |
|---|---|---|---|---|
| 4 | releaseA | classic | CONDITIONAL | state 1, -86.8 dB |
| 5 | releaseM | classic | CONDITIONAL | state 2, -72.2 dB |
| 6 | autoRelease | classic | CONDITIONAL | state 1, -90.3 dB |
| 14 | delayUnit | classic | DEAD |  |
| 15 | upsample | classic | CONDITIONAL | state 1, -90.3 dB |
| 16 | link | classic | CONDITIONAL | state 1, -76.3 dB |
| 17 | headroom | classic | non-audio | kept so old sessions load; the setup sheet says it has no effect |
| 19 | noLatency | classic | CONDITIONAL | state 1, -77.2 dB |
| 20 | chain0.on | classic | CONDITIONAL | state 1, -3.8 dB |
| 23 | shaper.threshold | shaper | CONDITIONAL | state 2, -6.4 dB |
| 25 | shaper.mix | shaper | CONDITIONAL | state 2, -5.6 dB |
| 26 | shaper.oversample | shaper | CONDITIONAL | state 2, -6.3 dB |
| 28 | shaper.upwardThreshold | shaper | CONDITIONAL | state 1, -14.0 dB |
| 30 | shaper.on | shaper | CONDITIONAL | state 1, -12.9 dB |
| 33 | modes.character | modes | DEAD |  |
| 34 | modes.learnTarget | modes | DEAD |  |
| 35 | modes.learnLoudness | modes | DEAD |  |
| 38 | modes.truePeak | modes | DEAD |  |
| 39 | modes.stereoLink | modes | DEAD |  |
| 40 | modes.inputLinked | modes | CONDITIONAL | state 4, -72.5 dB |
| 42 | modes.inputGainLeft | modes | DEAD |  |
| 43 | modes.inputGainRight | modes | DEAD |  |
| 44 | modes.outputLinked | modes | CONDITIONAL | state 4, -68.9 dB |
| 46 | modes.outputGainLeft | modes | CONDITIONAL | state 4, -14.1 dB |
| 47 | modes.outputGainRight | modes | CONDITIONAL | state 4, -15.4 dB |
| 48 | modes.unityGain | modes | DEAD |  |
| 49 | modes.on | modes | CONDITIONAL | state 1, -3.8 dB |
| 51 | eq.eqOn | eq | CONDITIONAL | state 1, -3.1 dB |
| 52 | eq.matrix | eq | CONDITIONAL | state 1, -7.9 dB |
| 53 | eq.linkSets | eq | CONDITIONAL | state 1, -10.5 dB |
| 55 | eq.lowCutFreq | eq | CONDITIONAL | state 1, -23.3 dB |
| 56 | eq.b1On | eq | CONDITIONAL | state 1, -34.3 dB |
| 58 | eq.b1Freq | eq | CONDITIONAL | state 1, -30.7 dB |
| 60 | eq.b1Width | eq | DEAD |  |
| 61 | eq.b1Depth | eq | DEAD |  |
| 62 | eq.b2On | eq | CONDITIONAL | state 1, -42.7 dB |
| 64 | eq.b2Freq | eq | CONDITIONAL | state 2, -11.6 dB |
| 66 | eq.b2Width | eq | DEAD |  |
| 67 | eq.b2Depth | eq | DEAD |  |
| 68 | eq.b3On | eq | CONDITIONAL | state 1, -46.3 dB |
| 70 | eq.b3Freq | eq | CONDITIONAL | state 1, -12.7 dB |
| 72 | eq.b3Width | eq | CONDITIONAL | state 1, -12.1 dB |
| 73 | eq.b3Depth | eq | DEAD |  |
| 74 | eq.b4On | eq | CONDITIONAL | state 1, -10.5 dB |
| 76 | eq.b4Freq | eq | CONDITIONAL | state 1, -10.1 dB |
| 78 | eq.b4Width | eq | DEAD |  |
| 79 | eq.b4Depth | eq | DEAD |  |
| 80 | eq.b5On | eq | CONDITIONAL | state 1, -5.8 dB |
| 82 | eq.b5Freq | eq | CONDITIONAL | state 1, -3.4 dB |
| 84 | eq.b5Width | eq | CONDITIONAL | state 2, -24.0 dB |
| 85 | eq.b5Depth | eq | CONDITIONAL | state 2, -23.8 dB |
| 86 | eq.b6On | eq | CONDITIONAL | state 1, -37.5 dB |
| 88 | eq.b6Freq | eq | CONDITIONAL | state 2, -8.1 dB |
| 90 | eq.b6Width | eq | DEAD |  |
| 91 | eq.b6Depth | eq | DEAD |  |
| 92 | eq.b1OnCh2 | eq | CONDITIONAL | state 1, -16.8 dB |
| 93 | eq.b1TypeCh2 | eq | CONDITIONAL | state 1, -10.0 dB |
| 94 | eq.b1FreqCh2 | eq | CONDITIONAL | state 1, -16.7 dB |
| 95 | eq.b1GainCh2 | eq | CONDITIONAL | state 1, -2.3 dB |
| 96 | eq.b1WidthCh2 | eq | DEAD |  |
| 97 | eq.b1DepthCh2 | eq | DEAD |  |
| 98 | eq.b2OnCh2 | eq | CONDITIONAL | state 1, -28.0 dB |
| 99 | eq.b2TypeCh2 | eq | CONDITIONAL | state 4, -8.1 dB |
| 100 | eq.b2FreqCh2 | eq | CONDITIONAL | state 4, -11.5 dB |
| 101 | eq.b2GainCh2 | eq | DEAD |  |
| 102 | eq.b2WidthCh2 | eq | CONDITIONAL | state 5, -20.2 dB |
| 103 | eq.b2DepthCh2 | eq | CONDITIONAL | state 5, -26.0 dB |
| 104 | eq.b3OnCh2 | eq | CONDITIONAL | state 1, -25.7 dB |
| 105 | eq.b3TypeCh2 | eq | CONDITIONAL | state 4, -11.5 dB |
| 106 | eq.b3FreqCh2 | eq | CONDITIONAL | state 4, -18.9 dB |
| 107 | eq.b3GainCh2 | eq | CONDITIONAL | state 5, -2.8 dB |
| 108 | eq.b3WidthCh2 | eq | CONDITIONAL | state 4, -19.1 dB |
| 109 | eq.b3DepthCh2 | eq | CONDITIONAL | state 4, -19.2 dB |
| 110 | eq.b4OnCh2 | eq | CONDITIONAL | state 1, -19.9 dB |
| 111 | eq.b4TypeCh2 | eq | CONDITIONAL | state 1, -12.0 dB |
| 112 | eq.b4FreqCh2 | eq | CONDITIONAL | state 1, -10.0 dB |
| 113 | eq.b4GainCh2 | eq | DEAD |  |
| 114 | eq.b4WidthCh2 | eq | DEAD |  |
| 115 | eq.b4DepthCh2 | eq | DEAD |  |
| 116 | eq.b5OnCh2 | eq | CONDITIONAL | state 1, -18.5 dB |
| 117 | eq.b5TypeCh2 | eq | CONDITIONAL | state 4, -14.0 dB |
| 118 | eq.b5FreqCh2 | eq | CONDITIONAL | state 4, -11.5 dB |
| 119 | eq.b5GainCh2 | eq | DEAD |  |
| 120 | eq.b5WidthCh2 | eq | DEAD |  |
| 121 | eq.b5DepthCh2 | eq | DEAD |  |
| 122 | eq.b6OnCh2 | eq | CONDITIONAL | state 1, -12.0 dB |
| 123 | eq.b6TypeCh2 | eq | DEAD |  |
| 124 | eq.b6FreqCh2 | eq | DEAD |  |
| 125 | eq.b6GainCh2 | eq | DEAD |  |
| 126 | eq.b6WidthCh2 | eq | DEAD |  |
| 127 | eq.b6DepthCh2 | eq | DEAD |  |
| 128 | eq.on | eq | CONDITIONAL | state 1, -9.0 dB |
| 138 | sone.slowRelease | sone | CONDITIONAL | state 1, -64.3 dB |
| 140 | sone.fastOn | sone | CONDITIONAL | state 1, -26.8 dB |
| 142 | sone.fastRatio | sone | CONDITIONAL | state 4, -49.1 dB |
| 143 | sone.fastAttack | sone | CONDITIONAL | state 4, -81.9 dB |
| 144 | sone.fastRelease | sone | CONDITIONAL | state 4, -68.9 dB |
| 145 | sone.fastLookahead | sone | CONDITIONAL | state 4, -86.4 dB |
| 153 | clear.attack | clear | CONDITIONAL | state 1, -26.2 dB |
| 154 | clear.release | clear | CONDITIONAL | state 1, -26.2 dB |
| 158 | clear.slowHold | clear | CONDITIONAL | state 1, -29.3 dB |
| 166 | dual.limiterOn | dual | CONDITIONAL | state 1, -10.7 dB |
| 167 | dual.limiterType | dual | CONDITIONAL | state 1, -56.0 dB |
| 168 | dual.bandSolo | dual | CONDITIONAL | state 1, -81.5 dB |
| 169 | dual.channelMode | dual | CONDITIONAL | state 1, -37.6 dB |
| 170 | dual.gangedSet | dual | CONDITIONAL | state 2, -3.8 dB |
| 171 | dual.autoMakeup | dual | CONDITIONAL | state 1, -52.4 dB |
| 173 | dual.detector | dual | CONDITIONAL | state 1, -61.4 dB |
| 174 | dual.rmsTime | dual | CONDITIONAL | state 2, -3.8 dB |
| 175 | dual.keySource | dual | CONDITIONAL | state 1, -61.3 dB |
| 176 | dual.keyLink | dual | CONDITIONAL | state 1, -70.4 dB |
| 177 | dual.keyFilter | dual | CONDITIONAL | state 2, -3.8 dB |
| 178 | dual.parallelMode | dual | CONDITIONAL | state 2, -3.8 dB |
| 179 | dual.parallelMix | dual | CONDITIONAL | state 2, -3.8 dB |
| 180 | dual.filterType1 | dual | CONDITIONAL | state 1, -64.7 dB |
| 181 | dual.bandwidth1 | dual | CONDITIONAL | state 2, -3.8 dB |
| 182 | dual.centerFreq1 | dual | CONDITIONAL | state 2, -3.8 dB |
| 184 | dual.releaseDelay1 | dual | CONDITIONAL | state 1, -81.3 dB |
| 185 | dual.releaseFast1 | dual | CONDITIONAL | state 1, -86.3 dB |
| 186 | dual.releaseAverage1 | dual | CONDITIONAL | state 1, -93.5 dB |
| 187 | dual.releaseSlow1 | dual | CONDITIONAL | state 1, -83.5 dB |
| 188 | dual.autoRelease1 | dual | CONDITIONAL | state 1, -86.3 dB |
| 189 | dual.autoReleaseTime1 | dual | CONDITIONAL | state 2, -3.8 dB |
| 190 | dual.softKnee1 | dual | CONDITIONAL | state 1, -69.7 dB |
| 192 | dual.ratio1 | dual | CONDITIONAL | state 1, -61.9 dB |
| 194 | dual.filterType2 | dual | CONDITIONAL | state 1, -11.8 dB |
| 195 | dual.bandwidth2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 196 | dual.centerFreq2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 197 | dual.attack2 | dual | CONDITIONAL | state 1, -62.8 dB |
| 198 | dual.releaseDelay2 | dual | CONDITIONAL | state 1, -64.7 dB |
| 199 | dual.releaseFast2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 200 | dual.releaseAverage2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 201 | dual.releaseSlow2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 202 | dual.autoRelease2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 203 | dual.autoReleaseTime2 | dual | CONDITIONAL | state 2, -3.8 dB |
| 204 | dual.softKnee2 | dual | CONDITIONAL | state 1, -51.4 dB |
| 205 | dual.threshold2 | dual | CONDITIONAL | state 1, -45.3 dB |
| 206 | dual.ratio2 | dual | CONDITIONAL | state 1, -52.6 dB |
| 207 | dual.makeup2 | dual | CONDITIONAL | state 1, -42.8 dB |
| 208 | dual.on | dual | CONDITIONAL | state 1, -4.7 dB |
| 214 | wall.autoRelease | wall | CONDITIONAL | state 1, -30.4 dB |
| 215 | wall.release | wall | CONDITIONAL | state 1, -20.0 dB |
| 216 | wall.profile | wall | CONDITIONAL | state 4, -5.4 dB |
| 217 | wall.softClip | wall | CONDITIONAL | state 2, -63.4 dB |
| 218 | wall.softClipThreshold | wall | CONDITIONAL | state 2, -42.9 dB |
| 219 | wall.softClipCurve | wall | CONDITIONAL | state 2, -39.9 dB |
| 221 | wall.on | wall | CONDITIONAL | state 1, -6.8 dB |
| 227 | bands.wetGain | bands | CONDITIONAL | state 5, -46.4 dB |
| 228 | bands.floorGuardThreshold | bands | CONDITIONAL | state 1, -9.1 dB |
| 229 | bands.floorGuardRatio | bands | CONDITIONAL | state 4, -6.6 dB |
| 234 | bands.bandLimiter | bands | CONDITIONAL | state 1, -9.1 dB |
| 235 | bands.bandLimiterRelease | bands | CONDITIONAL | state 4, -23.2 dB |
| 241 | bands.lowLift | bands | DEAD |  |
| 244 | bands.lowCeiling | bands | CONDITIONAL | state 4, -13.0 dB |
| 250 | bands.lowMidLift | bands | CONDITIONAL | state 4, -8.9 dB |
| 253 | bands.lowMidCeiling | bands | DEAD |  |
| 259 | bands.midLift | bands | DEAD |  |
| 262 | bands.midCeiling | bands | CONDITIONAL | state 4, -19.9 dB |
| 268 | bands.highMidLift | bands | DEAD |  |
| 271 | bands.highMidCeiling | bands | CONDITIONAL | state 5, -16.1 dB |
| 277 | bands.highLift | bands | DEAD |  |
| 280 | bands.highCeiling | bands | CONDITIONAL | state 4, -15.9 dB |
| 284 | peq.b1On | peq | CONDITIONAL | state 1, 6.0 dB |
| 286 | peq.b1Freq | peq | CONDITIONAL | state 1, 1.5 dB |
| 288 | peq.b1Q | peq | CONDITIONAL | state 1, 1.5 dB |
| 289 | peq.b1Slope | peq | CONDITIONAL | state 1, 1.8 dB |
| 290 | peq.b1Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 291 | peq.b1Pan | peq | CONDITIONAL | state 1, 1.5 dB |
| 292 | peq.b1Solo | peq | CONDITIONAL | state 1, 5.9 dB |
| 293 | peq.b1Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 294 | peq.b1Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 295 | peq.b1SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 296 | peq.b1SplitTail | peq | CONDITIONAL | state 1, 3.6 dB |
| 297 | peq.b2On | peq | CONDITIONAL | state 1, 1.5 dB |
| 299 | peq.b2Freq | peq | CONDITIONAL | state 1, 1.5 dB |
| 301 | peq.b2Q | peq | CONDITIONAL | state 1, 1.5 dB |
| 302 | peq.b2Slope | peq | CONDITIONAL | state 1, 1.5 dB |
| 303 | peq.b2Placement | peq | CONDITIONAL | state 1, 1.5 dB |
| 304 | peq.b2Pan | peq | CONDITIONAL | state 1, 4.1 dB |
| 305 | peq.b2Solo | peq | CONDITIONAL | state 1, 1.5 dB |
| 306 | peq.b2Phase | peq | CONDITIONAL | state 1, 1.5 dB |
| 308 | peq.b2SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 309 | peq.b2SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 310 | peq.b3On | peq | CONDITIONAL | state 1, 1.5 dB |
| 312 | peq.b3Freq | peq | CONDITIONAL | state 1, 1.5 dB |
| 314 | peq.b3Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 315 | peq.b3Slope | peq | CONDITIONAL | state 1, 1.5 dB |
| 316 | peq.b3Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 317 | peq.b3Pan | peq | CONDITIONAL | state 1, 1.5 dB |
| 318 | peq.b3Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 319 | peq.b3Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 320 | peq.b3Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 321 | peq.b3SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 322 | peq.b3SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 323 | peq.b4On | peq | CONDITIONAL | state 1, 6.0 dB |
| 324 | peq.b4Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 325 | peq.b4Freq | peq | CONDITIONAL | state 1, 6.0 dB |
| 326 | peq.b4Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 327 | peq.b4Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 328 | peq.b4Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 329 | peq.b4Placement | peq | CONDITIONAL | state 1, 2.9 dB |
| 330 | peq.b4Pan | peq | CONDITIONAL | state 1, 3.9 dB |
| 331 | peq.b4Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 332 | peq.b4Phase | peq | CONDITIONAL | state 1, 5.3 dB |
| 333 | peq.b4Split | peq | CONDITIONAL | state 1, 4.1 dB |
| 334 | peq.b4SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 335 | peq.b4SplitTail | peq | CONDITIONAL | state 1, 4.0 dB |
| 336 | peq.b5On | peq | CONDITIONAL | state 1, 5.2 dB |
| 337 | peq.b5Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 338 | peq.b5Freq | peq | CONDITIONAL | state 1, 4.7 dB |
| 339 | peq.b5Gain | peq | CONDITIONAL | state 1, 4.3 dB |
| 340 | peq.b5Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 341 | peq.b5Slope | peq | CONDITIONAL | state 1, 5.6 dB |
| 342 | peq.b5Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 343 | peq.b5Pan | peq | CONDITIONAL | state 1, 2.3 dB |
| 344 | peq.b5Solo | peq | CONDITIONAL | state 1, 3.4 dB |
| 345 | peq.b5Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 346 | peq.b5Split | peq | CONDITIONAL | state 1, 3.9 dB |
| 347 | peq.b5SplitTrans | peq | CONDITIONAL | state 1, 3.5 dB |
| 348 | peq.b5SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 349 | peq.b6On | peq | CONDITIONAL | state 1, 6.0 dB |
| 350 | peq.b6Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 351 | peq.b6Freq | peq | CONDITIONAL | state 1, 5.5 dB |
| 352 | peq.b6Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 353 | peq.b6Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 354 | peq.b6Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 355 | peq.b6Placement | peq | CONDITIONAL | state 1, 3.7 dB |
| 356 | peq.b6Pan | peq | CONDITIONAL | state 1, 5.1 dB |
| 357 | peq.b6Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 358 | peq.b6Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 359 | peq.b6Split | peq | CONDITIONAL | state 1, 4.1 dB |
| 360 | peq.b6SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 361 | peq.b6SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 362 | peq.b7On | peq | CONDITIONAL | state 1, 6.0 dB |
| 363 | peq.b7Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 364 | peq.b7Freq | peq | CONDITIONAL | state 1, -5.4 dB |
| 365 | peq.b7Gain | peq | CONDITIONAL | state 1, -6.2 dB |
| 366 | peq.b7Q | peq | CONDITIONAL | state 1, -0.2 dB |
| 367 | peq.b7Slope | peq | CONDITIONAL | state 1, -4.1 dB |
| 368 | peq.b7Placement | peq | CONDITIONAL | state 1, -4.2 dB |
| 369 | peq.b7Pan | peq | CONDITIONAL | state 1, 0.8 dB |
| 370 | peq.b7Solo | peq | CONDITIONAL | state 1, -6.2 dB |
| 371 | peq.b7Phase | peq | CONDITIONAL | state 1, -5.9 dB |
| 372 | peq.b7Split | peq | CONDITIONAL | state 1, 0.8 dB |
| 373 | peq.b7SplitTrans | peq | CONDITIONAL | state 1, -6.2 dB |
| 374 | peq.b7SplitTail | peq | CONDITIONAL | state 1, -3.7 dB |
| 375 | peq.b8On | peq | CONDITIONAL | state 1, 5.8 dB |
| 376 | peq.b8Type | peq | CONDITIONAL | state 1, 1.7 dB |
| 377 | peq.b8Freq | peq | CONDITIONAL | state 1, -4.0 dB |
| 378 | peq.b8Gain | peq | CONDITIONAL | state 1, -6.2 dB |
| 379 | peq.b8Q | peq | CONDITIONAL | state 1, -6.2 dB |
| 380 | peq.b8Slope | peq | CONDITIONAL | state 1, -6.2 dB |
| 381 | peq.b8Placement | peq | CONDITIONAL | state 1, -6.2 dB |
| 382 | peq.b8Pan | peq | CONDITIONAL | state 1, 0.8 dB |
| 383 | peq.b8Solo | peq | CONDITIONAL | state 1, -6.1 dB |
| 384 | peq.b8Phase | peq | CONDITIONAL | state 1, -4.0 dB |
| 385 | peq.b8Split | peq | CONDITIONAL | state 1, -5.9 dB |
| 386 | peq.b8SplitTrans | peq | CONDITIONAL | state 1, -6.2 dB |
| 387 | peq.b8SplitTail | peq | CONDITIONAL | state 1, 0.8 dB |
| 388 | peq.b9On | peq | CONDITIONAL | state 1, 4.2 dB |
| 389 | peq.b9Type | peq | CONDITIONAL | state 1, 0.8 dB |
| 390 | peq.b9Freq | peq | CONDITIONAL | state 1, -3.7 dB |
| 391 | peq.b9Gain | peq | CONDITIONAL | state 1, 1.2 dB |
| 392 | peq.b9Q | peq | CONDITIONAL | state 1, -3.7 dB |
| 393 | peq.b9Slope | peq | CONDITIONAL | state 1, -4.0 dB |
| 394 | peq.b9Placement | peq | CONDITIONAL | state 1, -6.2 dB |
| 395 | peq.b9Pan | peq | CONDITIONAL | state 1, -6.2 dB |
| 396 | peq.b9Solo | peq | CONDITIONAL | state 1, -4.9 dB |
| 397 | peq.b9Phase | peq | CONDITIONAL | state 1, -0.2 dB |
| 398 | peq.b9Split | peq | CONDITIONAL | state 1, -3.0 dB |
| 399 | peq.b9SplitTrans | peq | CONDITIONAL | state 1, 5.9 dB |
| 400 | peq.b9SplitTail | peq | CONDITIONAL | state 1, 2.0 dB |
| 401 | peq.b10On | peq | CONDITIONAL | state 1, 3.4 dB |
| 402 | peq.b10Type | peq | CONDITIONAL | state 1, -0.5 dB |
| 403 | peq.b10Freq | peq | CONDITIONAL | state 1, 3.0 dB |
| 404 | peq.b10Gain | peq | CONDITIONAL | state 1, -4.4 dB |
| 405 | peq.b10Q | peq | CONDITIONAL | state 1, -9.6 dB |
| 406 | peq.b10Slope | peq | CONDITIONAL | state 1, -4.4 dB |
| 407 | peq.b10Placement | peq | CONDITIONAL | state 1, -4.6 dB |
| 408 | peq.b10Pan | peq | CONDITIONAL | state 1, -5.0 dB |
| 409 | peq.b10Solo | peq | CONDITIONAL | state 1, -1.9 dB |
| 410 | peq.b10Phase | peq | CONDITIONAL | state 2, -2.1 dB |
| 411 | peq.b10Split | peq | CONDITIONAL | state 2, -2.1 dB |
| 412 | peq.b10SplitTrans | peq | CONDITIONAL | state 2, -2.1 dB |
| 413 | peq.b10SplitTail | peq | CONDITIONAL | state 1, -7.8 dB |
| 414 | peq.b11On | peq | CONDITIONAL | state 2, -1.6 dB |
| 415 | peq.b11Type | peq | CONDITIONAL | state 1, -6.2 dB |
| 416 | peq.b11Freq | peq | CONDITIONAL | state 2, -1.8 dB |
| 417 | peq.b11Gain | peq | CONDITIONAL | state 2, -2.1 dB |
| 418 | peq.b11Q | peq | CONDITIONAL | state 1, -4.6 dB |
| 419 | peq.b11Slope | peq | CONDITIONAL | state 1, -6.7 dB |
| 420 | peq.b11Placement | peq | CONDITIONAL | state 1, -9.6 dB |
| 421 | peq.b11Pan | peq | CONDITIONAL | state 1, -5.8 dB |
| 422 | peq.b11Solo | peq | CONDITIONAL | state 1, -3.5 dB |
| 423 | peq.b11Phase | peq | CONDITIONAL | state 2, -2.1 dB |
| 424 | peq.b11Split | peq | CONDITIONAL | state 2, -2.1 dB |
| 425 | peq.b11SplitTrans | peq | CONDITIONAL | state 2, -2.1 dB |
| 426 | peq.b11SplitTail | peq | CONDITIONAL | state 2, -2.1 dB |
| 427 | peq.b12On | peq | CONDITIONAL | state 1, -7.2 dB |
| 428 | peq.b12Type | peq | CONDITIONAL | state 1, -3.6 dB |
| 429 | peq.b12Freq | peq | CONDITIONAL | state 1, -4.4 dB |
| 430 | peq.b12Gain | peq | CONDITIONAL | state 1, -4.4 dB |
| 431 | peq.b12Q | peq | CONDITIONAL | state 2, -2.1 dB |
| 432 | peq.b12Slope | peq | CONDITIONAL | state 1, -6.2 dB |
| 433 | peq.b12Placement | peq | CONDITIONAL | state 1, -9.6 dB |
| 434 | peq.b12Pan | peq | CONDITIONAL | state 1, -9.5 dB |
| 435 | peq.b12Solo | peq | CONDITIONAL | state 1, -5.0 dB |
| 436 | peq.b12Phase | peq | CONDITIONAL | state 1, -5.0 dB |
| 437 | peq.b12Split | peq | CONDITIONAL | state 1, -4.6 dB |
| 438 | peq.b12SplitTrans | peq | CONDITIONAL | state 1, -5.0 dB |
| 439 | peq.b12SplitTail | peq | CONDITIONAL | state 1, -5.0 dB |
| 440 | peq.b13On | peq | CONDITIONAL | state 1, -5.1 dB |
| 441 | peq.b13Type | peq | CONDITIONAL | state 1, -7.4 dB |
| 442 | peq.b13Freq | peq | CONDITIONAL | state 1, -6.8 dB |
| 443 | peq.b13Gain | peq | CONDITIONAL | state 1, 0.9 dB |
| 444 | peq.b13Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 445 | peq.b13Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 446 | peq.b13Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 447 | peq.b13Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 448 | peq.b13Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 449 | peq.b13Phase | peq | CONDITIONAL | state 1, 5.4 dB |
| 450 | peq.b13Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 451 | peq.b13SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 452 | peq.b13SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 453 | peq.b14On | peq | CONDITIONAL | state 1, 6.0 dB |
| 454 | peq.b14Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 455 | peq.b14Freq | peq | CONDITIONAL | state 1, 6.0 dB |
| 456 | peq.b14Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 457 | peq.b14Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 458 | peq.b14Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 459 | peq.b14Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 460 | peq.b14Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 461 | peq.b14Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 462 | peq.b14Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 463 | peq.b14Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 464 | peq.b14SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 465 | peq.b14SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 466 | peq.b15On | peq | CONDITIONAL | state 1, 6.0 dB |
| 467 | peq.b15Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 468 | peq.b15Freq | peq | CONDITIONAL | state 1, 5.4 dB |
| 469 | peq.b15Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 470 | peq.b15Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 471 | peq.b15Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 472 | peq.b15Placement | peq | CONDITIONAL | state 1, 5.4 dB |
| 473 | peq.b15Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 474 | peq.b15Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 475 | peq.b15Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 476 | peq.b15Split | peq | CONDITIONAL | state 1, 5.4 dB |
| 477 | peq.b15SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 478 | peq.b15SplitTail | peq | CONDITIONAL | state 1, 5.4 dB |
| 479 | peq.b16On | peq | CONDITIONAL | state 1, 5.4 dB |
| 480 | peq.b16Type | peq | CONDITIONAL | state 1, 5.4 dB |
| 481 | peq.b16Freq | peq | CONDITIONAL | state 1, 5.4 dB |
| 482 | peq.b16Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 483 | peq.b16Q | peq | CONDITIONAL | state 1, 5.4 dB |
| 484 | peq.b16Slope | peq | CONDITIONAL | state 1, -4.0 dB |
| 485 | peq.b16Placement | peq | CONDITIONAL | state 1, -4.4 dB |
| 486 | peq.b16Pan | peq | CONDITIONAL | state 1, -4.4 dB |
| 487 | peq.b16Solo | peq | CONDITIONAL | state 1, -3.5 dB |
| 488 | peq.b16Phase | peq | CONDITIONAL | state 1, -4.4 dB |
| 489 | peq.b16Split | peq | CONDITIONAL | state 1, -4.0 dB |
| 490 | peq.b16SplitTrans | peq | CONDITIONAL | state 1, -4.4 dB |
| 491 | peq.b16SplitTail | peq | CONDITIONAL | state 1, -4.0 dB |
| 492 | peq.b17On | peq | CONDITIONAL | state 1, -4.4 dB |
| 493 | peq.b17Type | peq | CONDITIONAL | state 1, -4.4 dB |
| 494 | peq.b17Freq | peq | CONDITIONAL | state 1, -4.4 dB |
| 495 | peq.b17Gain | peq | CONDITIONAL | state 1, -4.0 dB |
| 496 | peq.b17Q | peq | CONDITIONAL | state 1, -4.4 dB |
| 497 | peq.b17Slope | peq | CONDITIONAL | state 1, -4.2 dB |
| 498 | peq.b17Placement | peq | CONDITIONAL | state 1, -4.4 dB |
| 499 | peq.b17Pan | peq | CONDITIONAL | state 1, -4.4 dB |
| 500 | peq.b17Solo | peq | CONDITIONAL | state 1, -4.4 dB |
| 501 | peq.b17Phase | peq | CONDITIONAL | state 1, -4.4 dB |
| 502 | peq.b17Split | peq | CONDITIONAL | state 1, -4.3 dB |
| 503 | peq.b17SplitTrans | peq | CONDITIONAL | state 1, -4.4 dB |
| 504 | peq.b17SplitTail | peq | CONDITIONAL | state 1, -4.0 dB |
| 505 | peq.b18On | peq | CONDITIONAL | state 1, -1.0 dB |
| 506 | peq.b18Type | peq | CONDITIONAL | state 1, 5.9 dB |
| 507 | peq.b18Freq | peq | CONDITIONAL | state 1, -1.1 dB |
| 508 | peq.b18Gain | peq | CONDITIONAL | state 1, -4.0 dB |
| 509 | peq.b18Q | peq | CONDITIONAL | state 1, -4.0 dB |
| 510 | peq.b18Slope | peq | CONDITIONAL | state 1, -4.0 dB |
| 511 | peq.b18Placement | peq | CONDITIONAL | state 1, -0.5 dB |
| 512 | peq.b18Pan | peq | CONDITIONAL | state 1, -4.4 dB |
| 513 | peq.b18Solo | peq | CONDITIONAL | state 1, -1.0 dB |
| 514 | peq.b18Phase | peq | CONDITIONAL | state 1, -4.0 dB |
| 515 | peq.b18Split | peq | CONDITIONAL | state 1, -4.0 dB |
| 516 | peq.b18SplitTrans | peq | CONDITIONAL | state 1, -4.4 dB |
| 517 | peq.b18SplitTail | peq | CONDITIONAL | state 1, -4.2 dB |
| 518 | peq.b19On | peq | CONDITIONAL | state 1, -4.3 dB |
| 519 | peq.b19Type | peq | CONDITIONAL | state 1, -4.4 dB |
| 520 | peq.b19Freq | peq | CONDITIONAL | state 1, -4.4 dB |
| 521 | peq.b19Gain | peq | CONDITIONAL | state 1, -4.4 dB |
| 522 | peq.b19Q | peq | CONDITIONAL | state 1, -4.4 dB |
| 523 | peq.b19Slope | peq | CONDITIONAL | state 1, -4.4 dB |
| 524 | peq.b19Placement | peq | CONDITIONAL | state 2, -29.9 dB |
| 525 | peq.b19Pan | peq | DEAD |  |
| 526 | peq.b19Solo | peq | CONDITIONAL | state 2, -28.3 dB |
| 527 | peq.b19Phase | peq | DEAD |  |
| 528 | peq.b19Split | peq | CONDITIONAL | state 5, -16.1 dB |
| 529 | peq.b19SplitTrans | peq | DEAD |  |
| 530 | peq.b19SplitTail | peq | DEAD |  |
| 531 | peq.b20On | peq | CONDITIONAL | state 1, -0.7 dB |
| 532 | peq.b20Type | peq | CONDITIONAL | state 1, 3.3 dB |
| 533 | peq.b20Freq | peq | CONDITIONAL | state 1, 2.9 dB |
| 534 | peq.b20Gain | peq | CONDITIONAL | state 3, 3.5 dB |
| 535 | peq.b20Q | peq | CONDITIONAL | state 1, -0.9 dB |
| 536 | peq.b20Slope | peq | DEAD |  |
| 537 | peq.b20Placement | peq | CONDITIONAL | state 1, -1.9 dB |
| 538 | peq.b20Pan | peq | DEAD |  |
| 539 | peq.b20Solo | peq | CONDITIONAL | state 1, -0.7 dB |
| 540 | peq.b20Phase | peq | DEAD |  |
| 541 | peq.b20Split | peq | CONDITIONAL | state 3, 0.4 dB |
| 542 | peq.b20SplitTrans | peq | DEAD |  |
| 543 | peq.b20SplitTail | peq | DEAD |  |
| 544 | peq.b21On | peq | CONDITIONAL | state 2, -24.3 dB |
| 545 | peq.b21Type | peq | CONDITIONAL | state 2, -8.1 dB |
| 546 | peq.b21Freq | peq | CONDITIONAL | state 2, -17.7 dB |
| 547 | peq.b21Gain | peq | CONDITIONAL | state 2, 1.4 dB |
| 548 | peq.b21Q | peq | CONDITIONAL | state 2, -21.0 dB |
| 549 | peq.b21Slope | peq | DEAD |  |
| 550 | peq.b21Placement | peq | CONDITIONAL | state 2, -17.6 dB |
| 551 | peq.b21Pan | peq | DEAD |  |
| 552 | peq.b21Solo | peq | CONDITIONAL | state 1, 3.8 dB |
| 553 | peq.b21Phase | peq | DEAD |  |
| 554 | peq.b21Split | peq | CONDITIONAL | state 2, -16.0 dB |
| 555 | peq.b21SplitTrans | peq | CONDITIONAL | state 2, -14.1 dB |
| 556 | peq.b21SplitTail | peq | CONDITIONAL | state 2, -13.2 dB |
| 557 | peq.b22On | peq | CONDITIONAL | state 2, -4.6 dB |
| 558 | peq.b22Type | peq | CONDITIONAL | state 2, -1.8 dB |
| 559 | peq.b22Freq | peq | CONDITIONAL | state 2, -5.0 dB |
| 560 | peq.b22Gain | peq | CONDITIONAL | state 4, 3.0 dB |
| 561 | peq.b22Q | peq | DEAD |  |
| 562 | peq.b22Slope | peq | DEAD |  |
| 563 | peq.b22Placement | peq | CONDITIONAL | state 2, -10.6 dB |
| 564 | peq.b22Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 565 | peq.b22Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 566 | peq.b22Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 567 | peq.b22Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 568 | peq.b22SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 569 | peq.b22SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 570 | peq.b23On | peq | CONDITIONAL | state 1, 6.0 dB |
| 571 | peq.b23Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 572 | peq.b23Freq | peq | CONDITIONAL | state 1, 6.0 dB |
| 573 | peq.b23Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 574 | peq.b23Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 575 | peq.b23Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 576 | peq.b23Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 577 | peq.b23Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 578 | peq.b23Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 579 | peq.b23Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 580 | peq.b23Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 581 | peq.b23SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 582 | peq.b23SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 583 | peq.b24On | peq | CONDITIONAL | state 1, 6.0 dB |
| 584 | peq.b24Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 585 | peq.b24Freq | peq | CONDITIONAL | state 1, 6.0 dB |
| 586 | peq.b24Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 587 | peq.b24Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 588 | peq.b24Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 589 | peq.b24Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 590 | peq.b24Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 591 | peq.b24Solo | peq | CONDITIONAL | state 1, 6.0 dB |
| 592 | peq.b24Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 593 | peq.b24Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 594 | peq.b24SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 595 | peq.b24SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 596 | peq.b25On | peq | CONDITIONAL | state 1, 6.0 dB |
| 597 | peq.b25Type | peq | CONDITIONAL | state 1, 6.0 dB |
| 598 | peq.b25Freq | peq | CONDITIONAL | state 1, 6.0 dB |
| 599 | peq.b25Gain | peq | CONDITIONAL | state 1, 6.0 dB |
| 600 | peq.b25Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 601 | peq.b25Slope | peq | CONDITIONAL | state 1, 6.0 dB |
| 602 | peq.b25Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 603 | peq.b25Pan | peq | CONDITIONAL | state 1, 6.0 dB |
| 604 | peq.b25Solo | peq | CONDITIONAL | state 2, -2.5 dB |
| 605 | peq.b25Phase | peq | CONDITIONAL | state 2, -2.7 dB |
| 606 | peq.b25Split | peq | CONDITIONAL | state 2, -2.3 dB |
| 607 | peq.b25SplitTrans | peq | CONDITIONAL | state 2, -3.2 dB |
| 608 | peq.b25SplitTail | peq | CONDITIONAL | state 2, -4.9 dB |
| 609 | peq.b26On | peq | CONDITIONAL | state 1, 0.8 dB |
| 610 | peq.b26Type | peq | CONDITIONAL | state 2, -4.9 dB |
| 611 | peq.b26Freq | peq | CONDITIONAL | state 2, -4.9 dB |
| 612 | peq.b26Gain | peq | CONDITIONAL | state 2, -4.9 dB |
| 613 | peq.b26Q | peq | CONDITIONAL | state 2, -2.7 dB |
| 614 | peq.b26Slope | peq | CONDITIONAL | state 2, -2.3 dB |
| 615 | peq.b26Placement | peq | CONDITIONAL | state 2, -2.5 dB |
| 616 | peq.b26Pan | peq | CONDITIONAL | state 2, -4.9 dB |
| 617 | peq.b26Solo | peq | CONDITIONAL | state 2, -2.7 dB |
| 618 | peq.b26Phase | peq | CONDITIONAL | state 2, -4.9 dB |
| 619 | peq.b26Split | peq | CONDITIONAL | state 2, -2.5 dB |
| 620 | peq.b26SplitTrans | peq | CONDITIONAL | state 2, -2.7 dB |
| 621 | peq.b26SplitTail | peq | CONDITIONAL | state 2, -2.5 dB |
| 622 | peq.b27On | peq | CONDITIONAL | state 1, -6.7 dB |
| 623 | peq.b27Type | peq | CONDITIONAL | state 2, 3.2 dB |
| 624 | peq.b27Freq | peq | CONDITIONAL | state 2, -3.0 dB |
| 625 | peq.b27Gain | peq | CONDITIONAL | state 2, -2.7 dB |
| 626 | peq.b27Q | peq | CONDITIONAL | state 2, -3.4 dB |
| 627 | peq.b27Slope | peq | CONDITIONAL | state 2, -2.5 dB |
| 628 | peq.b27Placement | peq | CONDITIONAL | state 2, -2.5 dB |
| 629 | peq.b27Pan | peq | CONDITIONAL | state 2, -2.9 dB |
| 630 | peq.b27Solo | peq | CONDITIONAL | state 2, -2.6 dB |
| 631 | peq.b27Phase | peq | CONDITIONAL | state 2, -2.5 dB |
| 632 | peq.b27Split | peq | CONDITIONAL | state 2, -4.9 dB |
| 633 | peq.b27SplitTrans | peq | CONDITIONAL | state 2, -2.5 dB |
| 634 | peq.b27SplitTail | peq | CONDITIONAL | state 2, -2.5 dB |
| 635 | peq.b28On | peq | CONDITIONAL | state 2, -2.5 dB |
| 636 | peq.b28Type | peq | CONDITIONAL | state 2, -2.7 dB |
| 637 | peq.b28Freq | peq | CONDITIONAL | state 2, -4.9 dB |
| 638 | peq.b28Gain | peq | CONDITIONAL | state 2, -2.5 dB |
| 639 | peq.b28Q | peq | CONDITIONAL | state 2, -2.5 dB |
| 640 | peq.b28Slope | peq | CONDITIONAL | state 2, -2.5 dB |
| 641 | peq.b28Placement | peq | CONDITIONAL | state 2, -2.5 dB |
| 642 | peq.b28Pan | peq | CONDITIONAL | state 2, -2.7 dB |
| 643 | peq.b28Solo | peq | CONDITIONAL | state 1, -2.9 dB |
| 644 | peq.b28Phase | peq | CONDITIONAL | state 1, -4.3 dB |
| 645 | peq.b28Split | peq | CONDITIONAL | state 1, -4.3 dB |
| 646 | peq.b28SplitTrans | peq | CONDITIONAL | state 1, -4.7 dB |
| 647 | peq.b28SplitTail | peq | CONDITIONAL | state 1, -5.8 dB |
| 648 | peq.b29On | peq | CONDITIONAL | state 1, -4.3 dB |
| 649 | peq.b29Type | peq | CONDITIONAL | state 1, -5.0 dB |
| 650 | peq.b29Freq | peq | CONDITIONAL | state 1, -5.8 dB |
| 651 | peq.b29Gain | peq | CONDITIONAL | state 1, -5.0 dB |
| 652 | peq.b29Q | peq | CONDITIONAL | state 1, -5.8 dB |
| 653 | peq.b29Slope | peq | CONDITIONAL | state 1, -4.8 dB |
| 654 | peq.b29Placement | peq | CONDITIONAL | state 1, -5.8 dB |
| 655 | peq.b29Pan | peq | CONDITIONAL | state 1, -2.8 dB |
| 656 | peq.b29Solo | peq | CONDITIONAL | state 1, -5.8 dB |
| 657 | peq.b29Phase | peq | CONDITIONAL | state 1, -5.8 dB |
| 658 | peq.b29Split | peq | CONDITIONAL | state 1, -5.8 dB |
| 659 | peq.b29SplitTrans | peq | CONDITIONAL | state 1, -5.8 dB |
| 660 | peq.b29SplitTail | peq | CONDITIONAL | state 1, -5.8 dB |
| 661 | peq.b30On | peq | CONDITIONAL | state 1, -4.3 dB |
| 662 | peq.b30Type | peq | CONDITIONAL | state 1, -1.1 dB |
| 663 | peq.b30Freq | peq | CONDITIONAL | state 1, -5.8 dB |
| 664 | peq.b30Gain | peq | CONDITIONAL | state 1, -5.0 dB |
| 665 | peq.b30Q | peq | CONDITIONAL | state 1, -5.8 dB |
| 666 | peq.b30Slope | peq | CONDITIONAL | state 1, -5.8 dB |
| 667 | peq.b30Placement | peq | CONDITIONAL | state 1, -2.9 dB |
| 668 | peq.b30Pan | peq | CONDITIONAL | state 1, -5.8 dB |
| 669 | peq.b30Solo | peq | CONDITIONAL | state 1, -1.7 dB |
| 670 | peq.b30Phase | peq | CONDITIONAL | state 1, -5.8 dB |
| 671 | peq.b30Split | peq | CONDITIONAL | state 1, -5.8 dB |
| 672 | peq.b30SplitTrans | peq | CONDITIONAL | state 1, -5.3 dB |
| 673 | peq.b30SplitTail | peq | CONDITIONAL | state 1, -5.8 dB |
| 674 | peq.b31On | peq | CONDITIONAL | state 1, -5.8 dB |
| 675 | peq.b31Type | peq | CONDITIONAL | state 1, -4.7 dB |
| 676 | peq.b31Freq | peq | CONDITIONAL | state 1, -5.8 dB |
| 677 | peq.b31Gain | peq | CONDITIONAL | state 1, -5.8 dB |
| 678 | peq.b31Q | peq | CONDITIONAL | state 1, -5.8 dB |
| 679 | peq.b31Slope | peq | CONDITIONAL | state 1, -5.0 dB |
| 680 | peq.b31Placement | peq | CONDITIONAL | state 1, -5.8 dB |
| 681 | peq.b31Pan | peq | CONDITIONAL | state 1, -5.8 dB |
| 682 | peq.b31Solo | peq | CONDITIONAL | state 1, -5.0 dB |
| 683 | peq.b31Phase | peq | CONDITIONAL | state 1, -4.3 dB |
| 684 | peq.b31Split | peq | CONDITIONAL | state 1, 6.0 dB |
| 685 | peq.b31SplitTrans | peq | CONDITIONAL | state 1, 6.0 dB |
| 686 | peq.b31SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 687 | peq.b32On | peq | CONDITIONAL | state 1, 6.0 dB |
| 688 | peq.b32Type | peq | CONDITIONAL | state 1, 5.4 dB |
| 689 | peq.b32Freq | peq | CONDITIONAL | state 2, -4.4 dB |
| 690 | peq.b32Gain | peq | CONDITIONAL | state 1, 4.2 dB |
| 691 | peq.b32Q | peq | CONDITIONAL | state 1, 6.0 dB |
| 692 | peq.b32Slope | peq | CONDITIONAL | state 2, -4.4 dB |
| 693 | peq.b32Placement | peq | CONDITIONAL | state 1, 6.0 dB |
| 694 | peq.b32Pan | peq | CONDITIONAL | state 2, -10.7 dB |
| 695 | peq.b32Solo | peq | CONDITIONAL | state 1, 5.4 dB |
| 696 | peq.b32Phase | peq | CONDITIONAL | state 1, 6.0 dB |
| 697 | peq.b32Split | peq | CONDITIONAL | state 1, 5.4 dB |
| 698 | peq.b32SplitTrans | peq | CONDITIONAL | state 1, 3.8 dB |
| 699 | peq.b32SplitTail | peq | CONDITIONAL | state 1, 6.0 dB |
| 700 | peq.processingMode | peq | CONDITIONAL | state 1, 5.4 dB |
| 701 | peq.linearPhaseQuality | peq | CONDITIONAL | state 3, -19.8 dB |
| 702 | peq.windowShape | peq | CONDITIONAL | state 2, -10.7 dB |
| 703 | peq.windowParameter | peq | CONDITIONAL | state 1, 6.0 dB |
| 704 | peq.mixCrossover | peq | CONDITIONAL | state 1, 6.0 dB |
| 705 | peq.digitalPlus | peq | CONDITIONAL | state 2, -10.7 dB |
| 706 | peq.ultraPrecision | peq | CONDITIONAL | state 3, -11.8 dB |
| 710 | peq.gainScale | peq | CONDITIONAL | state 1, 6.0 dB |
| 716 | peq.autoGain | peq | CONDITIONAL | state 2, -10.7 dB |
| 717 | peq.bypass | peq | CONDITIONAL | state 1, 5.4 dB |
| 718 | peq.splitSource | peq | CONDITIONAL | state 1, 5.4 dB |
| 719 | peq.splitSeparation | peq | CONDITIONAL | state 2, -4.4 dB |
| 720 | peq.splitDecay | peq | CONDITIONAL | state 2, -10.7 dB |
| 721 | peq.splitSmoothing | peq | CONDITIONAL | state 1, 5.4 dB |
| 722 | peq.splitSolo | peq | CONDITIONAL | state 1, 6.0 dB |
| 723 | peq.on | peq | CONDITIONAL | state 1, -2.8 dB |
| 725 | deq.b1On | deq | CONDITIONAL | state 1, -7.0 dB |
| 727 | deq.b1Freq | deq | CONDITIONAL | state 1, -24.2 dB |
| 729 | deq.b1Q | deq | DEAD |  |
| 730 | deq.b1Slope | deq | CONDITIONAL | state 1, -15.7 dB |
| 731 | deq.b1Placement | deq | CONDITIONAL | state 1, -12.1 dB |
| 732 | deq.b1Pan | deq | DEAD |  |
| 733 | deq.b1Solo | deq | CONDITIONAL | state 2, 0.3 dB |
| 735 | deq.b1Thresh | deq | DEAD |  |
| 736 | deq.b1Range | deq | DEAD |  |
| 737 | deq.b1Attack | deq | DEAD |  |
| 738 | deq.b1Release | deq | DEAD |  |
| 739 | deq.b1Action | deq | DEAD |  |
| 740 | deq.b1Knee | deq | DEAD |  |
| 741 | deq.b1Source | deq | DEAD |  |
| 742 | deq.b1Relative | deq | DEAD |  |
| 743 | deq.b1Lookahead | deq | DEAD |  |
| 744 | deq.b1Transient | deq | DEAD |  |
| 745 | deq.b1Auto | deq | DEAD |  |
| 746 | deq.b1DetLink | deq | DEAD |  |
| 747 | deq.b1DetFreq | deq | DEAD |  |
| 748 | deq.b1DetQ | deq | DEAD |  |
| 749 | deq.b1BelowOn | deq | DEAD |  |
| 750 | deq.b1BelowRange | deq | DEAD |  |
| 751 | deq.b1BelowRatio | deq | DEAD |  |
| 752 | deq.b1AboveOn | deq | DEAD |  |
| 753 | deq.b1AboveRange | deq | DEAD |  |
| 754 | deq.b1AboveRatio | deq | DEAD |  |
| 755 | deq.b1RelFree | deq | DEAD |  |
| 756 | deq.b1RelFreq | deq | DEAD |  |
| 757 | deq.b1RelQ | deq | DEAD |  |
| 758 | deq.b1RelSource | deq | DEAD |  |
| 759 | deq.b2On | deq | CONDITIONAL | state 1, -11.7 dB |
| 761 | deq.b2Freq | deq | CONDITIONAL | state 1, -5.2 dB |
| 763 | deq.b2Q | deq | CONDITIONAL | state 1, 2.3 dB |
| 764 | deq.b2Slope | deq | CONDITIONAL | state 1, -5.8 dB |
| 765 | deq.b2Placement | deq | CONDITIONAL | state 1, -11.1 dB |
| 766 | deq.b2Pan | deq | DEAD |  |
| 767 | deq.b2Solo | deq | CONDITIONAL | state 2, 0.3 dB |
| 769 | deq.b2Thresh | deq | DEAD |  |
| 770 | deq.b2Range | deq | DEAD |  |
| 771 | deq.b2Attack | deq | DEAD |  |
| 772 | deq.b2Release | deq | DEAD |  |
| 773 | deq.b2Action | deq | DEAD |  |
| 774 | deq.b2Knee | deq | DEAD |  |
| 775 | deq.b2Source | deq | DEAD |  |
| 776 | deq.b2Relative | deq | DEAD |  |
| 777 | deq.b2Lookahead | deq | DEAD |  |
| 778 | deq.b2Transient | deq | DEAD |  |
| 779 | deq.b2Auto | deq | DEAD |  |
| 780 | deq.b2DetLink | deq | DEAD |  |
| 781 | deq.b2DetFreq | deq | DEAD |  |
| 782 | deq.b2DetQ | deq | DEAD |  |
| 783 | deq.b2BelowOn | deq | DEAD |  |
| 784 | deq.b2BelowRange | deq | DEAD |  |
| 785 | deq.b2BelowRatio | deq | DEAD |  |
| 786 | deq.b2AboveOn | deq | DEAD |  |
| 787 | deq.b2AboveRange | deq | DEAD |  |
| 788 | deq.b2AboveRatio | deq | DEAD |  |
| 789 | deq.b2RelFree | deq | DEAD |  |
| 790 | deq.b2RelFreq | deq | DEAD |  |
| 791 | deq.b2RelQ | deq | DEAD |  |
| 792 | deq.b2RelSource | deq | DEAD |  |
| 793 | deq.b3On | deq | CONDITIONAL | state 1, -8.0 dB |
| 795 | deq.b3Freq | deq | CONDITIONAL | state 1, -21.1 dB |
| 797 | deq.b3Q | deq | CONDITIONAL | state 2, -38.7 dB |
| 798 | deq.b3Slope | deq | DEAD |  |
| 799 | deq.b3Placement | deq | CONDITIONAL | state 1, -15.1 dB |
| 800 | deq.b3Pan | deq | CONDITIONAL | state 1, -9.7 dB |
| 801 | deq.b3Solo | deq | CONDITIONAL | state 3, -22.4 dB |
| 803 | deq.b3Thresh | deq | DEAD |  |
| 804 | deq.b3Range | deq | DEAD |  |
| 805 | deq.b3Attack | deq | CONDITIONAL | state 2, -13.6 dB |
| 806 | deq.b3Release | deq | CONDITIONAL | state 2, -9.0 dB |
| 807 | deq.b3Action | deq | CONDITIONAL | state 2, -12.0 dB |
| 808 | deq.b3Knee | deq | DEAD |  |
| 809 | deq.b3Source | deq | CONDITIONAL | state 2, -7.4 dB |
| 810 | deq.b3Relative | deq | CONDITIONAL | state 2, -0.5 dB |
| 811 | deq.b3Lookahead | deq | CONDITIONAL | state 2, -6.0 dB |
| 812 | deq.b3Transient | deq | CONDITIONAL | state 2, -13.0 dB |
| 813 | deq.b3Auto | deq | CONDITIONAL | state 2, -9.0 dB |
| 814 | deq.b3DetLink | deq | CONDITIONAL | state 2, -9.0 dB |
| 815 | deq.b3DetFreq | deq | CONDITIONAL | state 2, -9.0 dB |
| 816 | deq.b3DetQ | deq | CONDITIONAL | state 2, -7.0 dB |
| 817 | deq.b3BelowOn | deq | DEAD |  |
| 818 | deq.b3BelowRange | deq | DEAD |  |
| 819 | deq.b3BelowRatio | deq | DEAD |  |
| 820 | deq.b3AboveOn | deq | CONDITIONAL | state 2, -4.3 dB |
| 821 | deq.b3AboveRange | deq | CONDITIONAL | state 2, -4.5 dB |
| 822 | deq.b3AboveRatio | deq | CONDITIONAL | state 2, -0.5 dB |
| 823 | deq.b3RelFree | deq | DEAD |  |
| 824 | deq.b3RelFreq | deq | DEAD |  |
| 825 | deq.b3RelQ | deq | DEAD |  |
| 826 | deq.b3RelSource | deq | CONDITIONAL | state 2, -7.4 dB |
| 827 | deq.b4On | deq | CONDITIONAL | state 1, -0.1 dB |
| 828 | deq.b4Type | deq | DEAD |  |
| 829 | deq.b4Freq | deq | DEAD |  |
| 830 | deq.b4Gain | deq | DEAD |  |
| 831 | deq.b4Q | deq | DEAD |  |
| 832 | deq.b4Slope | deq | DEAD |  |
| 833 | deq.b4Placement | deq | DEAD |  |
| 834 | deq.b4Pan | deq | DEAD |  |
| 835 | deq.b4Solo | deq | CONDITIONAL | state 2, 3.7 dB |
| 836 | deq.b4Dyn | deq | DEAD |  |
| 837 | deq.b4Thresh | deq | DEAD |  |
| 838 | deq.b4Range | deq | DEAD |  |
| 839 | deq.b4Attack | deq | DEAD |  |
| 840 | deq.b4Release | deq | DEAD |  |
| 841 | deq.b4Action | deq | DEAD |  |
| 842 | deq.b4Knee | deq | DEAD |  |
| 843 | deq.b4Source | deq | DEAD |  |
| 844 | deq.b4Relative | deq | DEAD |  |
| 845 | deq.b4Lookahead | deq | DEAD |  |
| 846 | deq.b4Transient | deq | DEAD |  |
| 847 | deq.b4Auto | deq | DEAD |  |
| 848 | deq.b4DetLink | deq | DEAD |  |
| 849 | deq.b4DetFreq | deq | DEAD |  |
| 850 | deq.b4DetQ | deq | DEAD |  |
| 851 | deq.b4BelowOn | deq | DEAD |  |
| 852 | deq.b4BelowRange | deq | DEAD |  |
| 853 | deq.b4BelowRatio | deq | DEAD |  |
| 854 | deq.b4AboveOn | deq | DEAD |  |
| 855 | deq.b4AboveRange | deq | DEAD |  |
| 856 | deq.b4AboveRatio | deq | DEAD |  |
| 857 | deq.b4RelFree | deq | DEAD |  |
| 858 | deq.b4RelFreq | deq | DEAD |  |
| 859 | deq.b4RelQ | deq | DEAD |  |
| 860 | deq.b4RelSource | deq | DEAD |  |
| 861 | deq.b5On | deq | CONDITIONAL | state 2, 6.0 dB |
| 862 | deq.b5Type | deq | DEAD |  |
| 863 | deq.b5Freq | deq | DEAD |  |
| 864 | deq.b5Gain | deq | DEAD |  |
| 865 | deq.b5Q | deq | DEAD |  |
| 866 | deq.b5Slope | deq | DEAD |  |
| 867 | deq.b5Placement | deq | DEAD |  |
| 868 | deq.b5Pan | deq | DEAD |  |
| 869 | deq.b5Solo | deq | CONDITIONAL | state 1, -8.0 dB |
| 870 | deq.b5Dyn | deq | DEAD |  |
| 871 | deq.b5Thresh | deq | DEAD |  |
| 872 | deq.b5Range | deq | DEAD |  |
| 873 | deq.b5Attack | deq | DEAD |  |
| 874 | deq.b5Release | deq | DEAD |  |
| 875 | deq.b5Action | deq | DEAD |  |
| 876 | deq.b5Knee | deq | DEAD |  |
| 877 | deq.b5Source | deq | DEAD |  |
| 878 | deq.b5Relative | deq | DEAD |  |
| 879 | deq.b5Lookahead | deq | DEAD |  |
| 880 | deq.b5Transient | deq | DEAD |  |
| 881 | deq.b5Auto | deq | DEAD |  |
| 882 | deq.b5DetLink | deq | DEAD |  |
| 883 | deq.b5DetFreq | deq | DEAD |  |
| 884 | deq.b5DetQ | deq | DEAD |  |
| 885 | deq.b5BelowOn | deq | DEAD |  |
| 886 | deq.b5BelowRange | deq | DEAD |  |
| 887 | deq.b5BelowRatio | deq | DEAD |  |
| 888 | deq.b5AboveOn | deq | CONDITIONAL | state 1, -46.6 dB |
| 889 | deq.b5AboveRange | deq | DEAD |  |
| 890 | deq.b5AboveRatio | deq | DEAD |  |
| 891 | deq.b5RelFree | deq | DEAD |  |
| 892 | deq.b5RelFreq | deq | DEAD |  |
| 893 | deq.b5RelQ | deq | DEAD |  |
| 894 | deq.b5RelSource | deq | DEAD |  |
| 895 | deq.b6On | deq | CONDITIONAL | state 3, -7.8 dB |
| 896 | deq.b6Type | deq | CONDITIONAL | state 3, -3.2 dB |
| 897 | deq.b6Freq | deq | CONDITIONAL | state 3, -7.0 dB |
| 898 | deq.b6Gain | deq | CONDITIONAL | state 4, 4.1 dB |
| 899 | deq.b6Q | deq | CONDITIONAL | state 3, -6.1 dB |
| 900 | deq.b6Slope | deq | DEAD |  |
| 901 | deq.b6Placement | deq | CONDITIONAL | state 3, -5.5 dB |
| 902 | deq.b6Pan | deq | DEAD |  |
| 903 | deq.b6Solo | deq | CONDITIONAL | state 2, -18.8 dB |
| 904 | deq.b6Dyn | deq | CONDITIONAL | state 4, -5.0 dB |
| 905 | deq.b6Thresh | deq | DEAD |  |
| 906 | deq.b6Range | deq | DEAD |  |
| 907 | deq.b6Attack | deq | DEAD |  |
| 908 | deq.b6Release | deq | DEAD |  |
| 909 | deq.b6Action | deq | DEAD |  |
| 910 | deq.b6Knee | deq | DEAD |  |
| 911 | deq.b6Source | deq | CONDITIONAL | state 4, -37.7 dB |
| 912 | deq.b6Relative | deq | CONDITIONAL | state 4, -36.8 dB |
| 913 | deq.b6Lookahead | deq | CONDITIONAL | state 4, -5.0 dB |
| 914 | deq.b6Transient | deq | CONDITIONAL | state 4, -33.0 dB |
| 915 | deq.b6Auto | deq | CONDITIONAL | state 4, -36.8 dB |
| 916 | deq.b6DetLink | deq | CONDITIONAL | state 4, -34.3 dB |
| 917 | deq.b6DetFreq | deq | DEAD |  |
| 918 | deq.b6DetQ | deq | DEAD |  |
| 919 | deq.b6BelowOn | deq | CONDITIONAL | state 4, -21.3 dB |
| 920 | deq.b6BelowRange | deq | DEAD |  |
| 921 | deq.b6BelowRatio | deq | DEAD |  |
| 922 | deq.b6AboveOn | deq | CONDITIONAL | state 4, -24.9 dB |
| 923 | deq.b6AboveRange | deq | CONDITIONAL | state 4, -23.9 dB |
| 924 | deq.b6AboveRatio | deq | CONDITIONAL | state 4, -35.4 dB |
| 925 | deq.b6RelFree | deq | DEAD |  |
| 926 | deq.b6RelFreq | deq | DEAD |  |
| 927 | deq.b6RelQ | deq | DEAD |  |
| 928 | deq.b6RelSource | deq | DEAD |  |
| 929 | deq.b7On | deq | CONDITIONAL | state 1, -5.0 dB |
| 930 | deq.b7Type | deq | CONDITIONAL | state 1, -5.1 dB |
| 931 | deq.b7Freq | deq | CONDITIONAL | state 1, -7.9 dB |
| 932 | deq.b7Gain | deq | CONDITIONAL | state 1, 2.9 dB |
| 933 | deq.b7Q | deq | CONDITIONAL | state 1, -4.8 dB |
| 934 | deq.b7Slope | deq | DEAD |  |
| 935 | deq.b7Placement | deq | CONDITIONAL | state 1, -5.8 dB |
| 936 | deq.b7Pan | deq | DEAD |  |
| 937 | deq.b7Solo | deq | CONDITIONAL | state 4, -30.4 dB |
| 938 | deq.b7Dyn | deq | CONDITIONAL | state 1, -5.0 dB |
| 939 | deq.b7Thresh | deq | CONDITIONAL | state 1, -41.7 dB |
| 940 | deq.b7Range | deq | DEAD |  |
| 941 | deq.b7Attack | deq | CONDITIONAL | state 4, -51.2 dB |
| 942 | deq.b7Release | deq | CONDITIONAL | state 1, -44.2 dB |
| 943 | deq.b7Action | deq | DEAD |  |
| 944 | deq.b7Knee | deq | DEAD |  |
| 945 | deq.b7Source | deq | CONDITIONAL | state 1, -13.0 dB |
| 946 | deq.b7Relative | deq | CONDITIONAL | state 1, -23.9 dB |
| 947 | deq.b7Lookahead | deq | CONDITIONAL | state 1, -5.0 dB |
| 948 | deq.b7Transient | deq | CONDITIONAL | state 1, -46.5 dB |
| 949 | deq.b7Auto | deq | CONDITIONAL | state 4, -47.6 dB |
| 950 | deq.b7DetLink | deq | CONDITIONAL | state 1, -43.5 dB |
| 951 | deq.b7DetFreq | deq | CONDITIONAL | state 1, -46.4 dB |
| 952 | deq.b7DetQ | deq | CONDITIONAL | state 1, -13.0 dB |
| 953 | deq.b7BelowOn | deq | CONDITIONAL | state 4, -60.5 dB |
| 954 | deq.b7BelowRange | deq | CONDITIONAL | state 4, -52.8 dB |
| 955 | deq.b7BelowRatio | deq | CONDITIONAL | state 4, -60.5 dB |
| 956 | deq.b7AboveOn | deq | CONDITIONAL | state 1, -4.6 dB |
| 957 | deq.b7AboveRange | deq | CONDITIONAL | state 1, 3.0 dB |
| 958 | deq.b7AboveRatio | deq | CONDITIONAL | state 1, -13.0 dB |
| 959 | deq.b7RelFree | deq | DEAD |  |
| 960 | deq.b7RelFreq | deq | DEAD |  |
| 961 | deq.b7RelQ | deq | DEAD |  |
| 962 | deq.b7RelSource | deq | CONDITIONAL | state 1, -41.6 dB |
| 963 | deq.b8On | deq | CONDITIONAL | state 1, 1.2 dB |
| 964 | deq.b8Type | deq | CONDITIONAL | state 3, -1.8 dB |
| 965 | deq.b8Freq | deq | CONDITIONAL | state 5, -5.9 dB |
| 966 | deq.b8Gain | deq | DEAD |  |
| 967 | deq.b8Q | deq | DEAD |  |
| 968 | deq.b8Slope | deq | DEAD |  |
| 969 | deq.b8Placement | deq | CONDITIONAL | state 5, -9.8 dB |
| 970 | deq.b8Pan | deq | DEAD |  |
| 971 | deq.b8Solo | deq | DEAD |  |
| 972 | deq.b8Dyn | deq | DEAD |  |
| 973 | deq.b8Thresh | deq | DEAD |  |
| 974 | deq.b8Range | deq | DEAD |  |
| 975 | deq.b8Attack | deq | DEAD |  |
| 976 | deq.b8Release | deq | DEAD |  |
| 977 | deq.b8Action | deq | DEAD |  |
| 978 | deq.b8Knee | deq | DEAD |  |
| 979 | deq.b8Source | deq | DEAD |  |
| 980 | deq.b8Relative | deq | DEAD |  |
| 981 | deq.b8Lookahead | deq | DEAD |  |
| 982 | deq.b8Transient | deq | DEAD |  |
| 983 | deq.b8Auto | deq | DEAD |  |
| 984 | deq.b8DetLink | deq | DEAD |  |
| 985 | deq.b8DetFreq | deq | DEAD |  |
| 986 | deq.b8DetQ | deq | DEAD |  |
| 987 | deq.b8BelowOn | deq | DEAD |  |
| 988 | deq.b8BelowRange | deq | DEAD |  |
| 989 | deq.b8BelowRatio | deq | DEAD |  |
| 990 | deq.b8AboveOn | deq | DEAD |  |
| 991 | deq.b8AboveRange | deq | DEAD |  |
| 992 | deq.b8AboveRatio | deq | DEAD |  |
| 993 | deq.b8RelFree | deq | DEAD |  |
| 994 | deq.b8RelFreq | deq | DEAD |  |
| 995 | deq.b8RelQ | deq | DEAD |  |
| 996 | deq.b8RelSource | deq | DEAD |  |
| 997 | deq.b9On | deq | CONDITIONAL | state 1, 0.9 dB |
| 998 | deq.b9Type | deq | CONDITIONAL | state 1, 6.0 dB |
| 999 | deq.b9Freq | deq | CONDITIONAL | state 1, 1.3 dB |
| 1000 | deq.b9Gain | deq | CONDITIONAL | state 1, 3.4 dB |
| 1001 | deq.b9Q | deq | DEAD |  |
| 1002 | deq.b9Slope | deq | DEAD |  |
| 1003 | deq.b9Placement | deq | CONDITIONAL | state 1, 6.0 dB |
| 1004 | deq.b9Pan | deq | DEAD |  |
| 1005 | deq.b9Solo | deq | CONDITIONAL | state 1, -0.9 dB |
| 1006 | deq.b9Dyn | deq | CONDITIONAL | state 4, -5.0 dB |
| 1007 | deq.b9Thresh | deq | DEAD |  |
| 1008 | deq.b9Range | deq | DEAD |  |
| 1009 | deq.b9Attack | deq | DEAD |  |
| 1010 | deq.b9Release | deq | DEAD |  |
| 1011 | deq.b9Action | deq | DEAD |  |
| 1012 | deq.b9Knee | deq | DEAD |  |
| 1013 | deq.b9Source | deq | CONDITIONAL | state 4, -11.8 dB |
| 1014 | deq.b9Relative | deq | CONDITIONAL | state 4, -11.8 dB |
| 1015 | deq.b9Lookahead | deq | CONDITIONAL | state 4, -5.0 dB |
| 1016 | deq.b9Transient | deq | CONDITIONAL | state 4, -26.7 dB |
| 1017 | deq.b9Auto | deq | CONDITIONAL | state 4, -21.6 dB |
| 1018 | deq.b9DetLink | deq | CONDITIONAL | state 4, -11.8 dB |
| 1019 | deq.b9DetFreq | deq | DEAD |  |
| 1020 | deq.b9DetQ | deq | DEAD |  |
| 1021 | deq.b9BelowOn | deq | CONDITIONAL | state 4, -35.3 dB |
| 1022 | deq.b9BelowRange | deq | DEAD |  |
| 1023 | deq.b9BelowRatio | deq | DEAD |  |
| 1024 | deq.b9AboveOn | deq | CONDITIONAL | state 4, -3.2 dB |
| 1025 | deq.b9AboveRange | deq | CONDITIONAL | state 4, -3.8 dB |
| 1026 | deq.b9AboveRatio | deq | CONDITIONAL | state 4, -11.8 dB |
| 1027 | deq.b9RelFree | deq | CONDITIONAL | state 4, -9.7 dB |
| 1028 | deq.b9RelFreq | deq | DEAD |  |
| 1029 | deq.b9RelQ | deq | DEAD |  |
| 1030 | deq.b9RelSource | deq | CONDITIONAL | state 4, -9.7 dB |
| 1031 | deq.b10On | deq | CONDITIONAL | state 1, -0.2 dB |
| 1032 | deq.b10Type | deq | CONDITIONAL | state 1, 2.8 dB |
| 1033 | deq.b10Freq | deq | CONDITIONAL | state 1, 2.8 dB |
| 1034 | deq.b10Gain | deq | CONDITIONAL | state 1, 3.3 dB |
| 1035 | deq.b10Q | deq | CONDITIONAL | state 2, -15.2 dB |
| 1036 | deq.b10Slope | deq | CONDITIONAL | state 1, -0.5 dB |
| 1037 | deq.b10Placement | deq | CONDITIONAL | state 1, -0.5 dB |
| 1038 | deq.b10Pan | deq | DEAD |  |
| 1039 | deq.b10Solo | deq | CONDITIONAL | state 1, -0.2 dB |
| 1040 | deq.b10Dyn | deq | CONDITIONAL | state 1, -1.8 dB |
| 1041 | deq.b10Thresh | deq | DEAD |  |
| 1042 | deq.b10Range | deq | DEAD |  |
| 1043 | deq.b10Attack | deq | DEAD |  |
| 1044 | deq.b10Release | deq | DEAD |  |
| 1045 | deq.b10Action | deq | DEAD |  |
| 1046 | deq.b10Knee | deq | DEAD |  |
| 1047 | deq.b10Source | deq | DEAD |  |
| 1048 | deq.b10Relative | deq | DEAD |  |
| 1049 | deq.b10Lookahead | deq | DEAD |  |
| 1050 | deq.b10Transient | deq | DEAD |  |
| 1051 | deq.b10Auto | deq | DEAD |  |
| 1052 | deq.b10DetLink | deq | DEAD |  |
| 1053 | deq.b10DetFreq | deq | DEAD |  |
| 1054 | deq.b10DetQ | deq | DEAD |  |
| 1055 | deq.b10BelowOn | deq | DEAD |  |
| 1056 | deq.b10BelowRange | deq | DEAD |  |
| 1057 | deq.b10BelowRatio | deq | DEAD |  |
| 1058 | deq.b10AboveOn | deq | DEAD |  |
| 1059 | deq.b10AboveRange | deq | DEAD |  |
| 1060 | deq.b10AboveRatio | deq | DEAD |  |
| 1061 | deq.b10RelFree | deq | DEAD |  |
| 1062 | deq.b10RelFreq | deq | DEAD |  |
| 1063 | deq.b10RelQ | deq | DEAD |  |
| 1064 | deq.b10RelSource | deq | DEAD |  |
| 1065 | deq.b11On | deq | CONDITIONAL | state 1, 3.0 dB |
| 1066 | deq.b11Type | deq | CONDITIONAL | state 1, 6.0 dB |
| 1067 | deq.b11Freq | deq | CONDITIONAL | state 1, 6.0 dB |
| 1068 | deq.b11Gain | deq | CONDITIONAL | state 1, 1.7 dB |
| 1069 | deq.b11Q | deq | DEAD |  |
| 1070 | deq.b11Slope | deq | DEAD |  |
| 1071 | deq.b11Placement | deq | CONDITIONAL | state 1, 6.0 dB |
| 1072 | deq.b11Pan | deq | DEAD |  |
| 1073 | deq.b11Solo | deq | CONDITIONAL | state 1, -8.5 dB |
| 1074 | deq.b11Dyn | deq | CONDITIONAL | state 1, 2.2 dB |
| 1075 | deq.b11Thresh | deq | DEAD |  |
| 1076 | deq.b11Range | deq | DEAD |  |
| 1077 | deq.b11Attack | deq | DEAD |  |
| 1078 | deq.b11Release | deq | DEAD |  |
| 1079 | deq.b11Action | deq | DEAD |  |
| 1080 | deq.b11Knee | deq | DEAD |  |
| 1081 | deq.b11Source | deq | CONDITIONAL | state 1, 1.9 dB |
| 1082 | deq.b11Relative | deq | DEAD |  |
| 1083 | deq.b11Lookahead | deq | CONDITIONAL | state 1, 2.2 dB |
| 1084 | deq.b11Transient | deq | DEAD |  |
| 1085 | deq.b11Auto | deq | DEAD |  |
| 1086 | deq.b11DetLink | deq | DEAD |  |
| 1087 | deq.b11DetFreq | deq | DEAD |  |
| 1088 | deq.b11DetQ | deq | DEAD |  |
| 1089 | deq.b11BelowOn | deq | DEAD |  |
| 1090 | deq.b11BelowRange | deq | DEAD |  |
| 1091 | deq.b11BelowRatio | deq | DEAD |  |
| 1092 | deq.b11AboveOn | deq | DEAD |  |
| 1093 | deq.b11AboveRange | deq | DEAD |  |
| 1094 | deq.b11AboveRatio | deq | DEAD |  |
| 1095 | deq.b11RelFree | deq | DEAD |  |
| 1096 | deq.b11RelFreq | deq | DEAD |  |
| 1097 | deq.b11RelQ | deq | DEAD |  |
| 1098 | deq.b11RelSource | deq | DEAD |  |
| 1099 | deq.b12On | deq | CONDITIONAL | state 2, -11.9 dB |
| 1100 | deq.b12Type | deq | CONDITIONAL | state 3, -5.2 dB |
| 1101 | deq.b12Freq | deq | CONDITIONAL | state 3, -2.2 dB |
| 1102 | deq.b12Gain | deq | CONDITIONAL | state 5, 6.0 dB |
| 1103 | deq.b12Q | deq | CONDITIONAL | state 3, 2.3 dB |
| 1104 | deq.b12Slope | deq | CONDITIONAL | state 3, -6.1 dB |
| 1105 | deq.b12Placement | deq | CONDITIONAL | state 3, -15.0 dB |
| 1106 | deq.b12Pan | deq | CONDITIONAL | state 3, -5.5 dB |
| 1107 | deq.b12Solo | deq | CONDITIONAL | state 1, 3.4 dB |
| 1108 | deq.b12Dyn | deq | CONDITIONAL | state 5, -7.1 dB |
| 1109 | deq.b12Thresh | deq | DEAD |  |
| 1110 | deq.b12Range | deq | DEAD |  |
| 1111 | deq.b12Attack | deq | DEAD |  |
| 1112 | deq.b12Release | deq | DEAD |  |
| 1113 | deq.b12Action | deq | DEAD |  |
| 1114 | deq.b12Knee | deq | DEAD |  |
| 1115 | deq.b12Source | deq | DEAD |  |
| 1116 | deq.b12Relative | deq | DEAD |  |
| 1117 | deq.b12Lookahead | deq | DEAD |  |
| 1118 | deq.b12Transient | deq | DEAD |  |
| 1119 | deq.b12Auto | deq | DEAD |  |
| 1120 | deq.b12DetLink | deq | DEAD |  |
| 1121 | deq.b12DetFreq | deq | DEAD |  |
| 1122 | deq.b12DetQ | deq | DEAD |  |
| 1123 | deq.b12BelowOn | deq | DEAD |  |
| 1124 | deq.b12BelowRange | deq | DEAD |  |
| 1125 | deq.b12BelowRatio | deq | DEAD |  |
| 1126 | deq.b12AboveOn | deq | CONDITIONAL | state 3, -6.8 dB |
| 1127 | deq.b12AboveRange | deq | DEAD |  |
| 1128 | deq.b12AboveRatio | deq | DEAD |  |
| 1129 | deq.b12RelFree | deq | DEAD |  |
| 1130 | deq.b12RelFreq | deq | DEAD |  |
| 1131 | deq.b12RelQ | deq | DEAD |  |
| 1132 | deq.b12RelSource | deq | DEAD |  |
| 1133 | deq.b13On | deq | CONDITIONAL | state 1, -6.1 dB |
| 1134 | deq.b13Type | deq | CONDITIONAL | state 1, -4.2 dB |
| 1135 | deq.b13Freq | deq | CONDITIONAL | state 1, -6.1 dB |
| 1136 | deq.b13Gain | deq | DEAD |  |
| 1137 | deq.b13Q | deq | CONDITIONAL | state 1, 1.6 dB |
| 1138 | deq.b13Slope | deq | DEAD |  |
| 1139 | deq.b13Placement | deq | CONDITIONAL | state 1, -6.2 dB |
| 1140 | deq.b13Pan | deq | DEAD |  |
| 1141 | deq.b13Solo | deq | DEAD |  |
| 1142 | deq.b13Dyn | deq | DEAD |  |
| 1143 | deq.b13Thresh | deq | DEAD |  |
| 1144 | deq.b13Range | deq | DEAD |  |
| 1145 | deq.b13Attack | deq | DEAD |  |
| 1146 | deq.b13Release | deq | DEAD |  |
| 1147 | deq.b13Action | deq | CONDITIONAL | state 3, -4.8 dB |
| 1148 | deq.b13Knee | deq | DEAD |  |
| 1149 | deq.b13Source | deq | DEAD |  |
| 1150 | deq.b13Relative | deq | DEAD |  |
| 1151 | deq.b13Lookahead | deq | DEAD |  |
| 1152 | deq.b13Transient | deq | DEAD |  |
| 1153 | deq.b13Auto | deq | DEAD |  |
| 1154 | deq.b13DetLink | deq | DEAD |  |
| 1155 | deq.b13DetFreq | deq | DEAD |  |
| 1156 | deq.b13DetQ | deq | DEAD |  |
| 1157 | deq.b13BelowOn | deq | DEAD |  |
| 1158 | deq.b13BelowRange | deq | DEAD |  |
| 1159 | deq.b13BelowRatio | deq | DEAD |  |
| 1160 | deq.b13AboveOn | deq | DEAD |  |
| 1161 | deq.b13AboveRange | deq | DEAD |  |
| 1162 | deq.b13AboveRatio | deq | DEAD |  |
| 1163 | deq.b13RelFree | deq | DEAD |  |
| 1164 | deq.b13RelFreq | deq | DEAD |  |
| 1165 | deq.b13RelQ | deq | DEAD |  |
| 1166 | deq.b13RelSource | deq | CONDITIONAL | state 2, -22.1 dB |
| 1167 | deq.b14On | deq | CONDITIONAL | state 2, 1.5 dB |
| 1168 | deq.b14Type | deq | CONDITIONAL | state 4, -3.7 dB |
| 1169 | deq.b14Freq | deq | CONDITIONAL | state 4, -3.6 dB |
| 1170 | deq.b14Gain | deq | CONDITIONAL | state 4, 0.3 dB |
| 1171 | deq.b14Q | deq | DEAD |  |
| 1172 | deq.b14Slope | deq | DEAD |  |
| 1173 | deq.b14Placement | deq | CONDITIONAL | state 4, -10.0 dB |
| 1174 | deq.b14Pan | deq | DEAD |  |
| 1175 | deq.b14Solo | deq | CONDITIONAL | state 4, -9.9 dB |
| 1176 | deq.b14Dyn | deq | CONDITIONAL | state 4, 1.7 dB |
| 1177 | deq.b14Thresh | deq | DEAD |  |
| 1178 | deq.b14Range | deq | DEAD |  |
| 1179 | deq.b14Attack | deq | DEAD |  |
| 1180 | deq.b14Release | deq | DEAD |  |
| 1181 | deq.b14Action | deq | DEAD |  |
| 1182 | deq.b14Knee | deq | DEAD |  |
| 1183 | deq.b14Source | deq | DEAD |  |
| 1184 | deq.b14Relative | deq | DEAD |  |
| 1185 | deq.b14Lookahead | deq | DEAD |  |
| 1186 | deq.b14Transient | deq | DEAD |  |
| 1187 | deq.b14Auto | deq | DEAD |  |
| 1188 | deq.b14DetLink | deq | DEAD |  |
| 1189 | deq.b14DetFreq | deq | DEAD |  |
| 1190 | deq.b14DetQ | deq | DEAD |  |
| 1191 | deq.b14BelowOn | deq | DEAD |  |
| 1192 | deq.b14BelowRange | deq | DEAD |  |
| 1193 | deq.b14BelowRatio | deq | DEAD |  |
| 1194 | deq.b14AboveOn | deq | DEAD |  |
| 1195 | deq.b14AboveRange | deq | DEAD |  |
| 1196 | deq.b14AboveRatio | deq | DEAD |  |
| 1197 | deq.b14RelFree | deq | DEAD |  |
| 1198 | deq.b14RelFreq | deq | DEAD |  |
| 1199 | deq.b14RelQ | deq | DEAD |  |
| 1200 | deq.b14RelSource | deq | DEAD |  |
| 1201 | deq.b15On | deq | CONDITIONAL | state 1, -9.5 dB |
| 1202 | deq.b15Type | deq | CONDITIONAL | state 2, -7.1 dB |
| 1203 | deq.b15Freq | deq | CONDITIONAL | state 2, -37.9 dB |
| 1204 | deq.b15Gain | deq | CONDITIONAL | state 2, -5.2 dB |
| 1205 | deq.b15Q | deq | DEAD |  |
| 1206 | deq.b15Slope | deq | DEAD |  |
| 1207 | deq.b15Placement | deq | CONDITIONAL | state 4, -31.2 dB |
| 1208 | deq.b15Pan | deq | DEAD |  |
| 1209 | deq.b15Solo | deq | CONDITIONAL | state 2, 1.5 dB |
| 1210 | deq.b15Dyn | deq | CONDITIONAL | state 4, -5.0 dB |
| 1211 | deq.b15Thresh | deq | DEAD |  |
| 1212 | deq.b15Range | deq | DEAD |  |
| 1213 | deq.b15Attack | deq | DEAD |  |
| 1214 | deq.b15Release | deq | DEAD |  |
| 1215 | deq.b15Action | deq | DEAD |  |
| 1216 | deq.b15Knee | deq | DEAD |  |
| 1217 | deq.b15Source | deq | DEAD |  |
| 1218 | deq.b15Relative | deq | DEAD |  |
| 1219 | deq.b15Lookahead | deq | CONDITIONAL | state 4, -5.0 dB |
| 1220 | deq.b15Transient | deq | DEAD |  |
| 1221 | deq.b15Auto | deq | DEAD |  |
| 1222 | deq.b15DetLink | deq | DEAD |  |
| 1223 | deq.b15DetFreq | deq | DEAD |  |
| 1224 | deq.b15DetQ | deq | DEAD |  |
| 1225 | deq.b15BelowOn | deq | DEAD |  |
| 1226 | deq.b15BelowRange | deq | DEAD |  |
| 1227 | deq.b15BelowRatio | deq | DEAD |  |
| 1228 | deq.b15AboveOn | deq | DEAD |  |
| 1229 | deq.b15AboveRange | deq | DEAD |  |
| 1230 | deq.b15AboveRatio | deq | DEAD |  |
| 1231 | deq.b15RelFree | deq | DEAD |  |
| 1232 | deq.b15RelFreq | deq | DEAD |  |
| 1233 | deq.b15RelQ | deq | DEAD |  |
| 1234 | deq.b15RelSource | deq | DEAD |  |
| 1235 | deq.b16On | deq | CONDITIONAL | state 1, -20.9 dB |
| 1236 | deq.b16Type | deq | CONDITIONAL | state 3, 0.1 dB |
| 1237 | deq.b16Freq | deq | CONDITIONAL | state 3, -1.0 dB |
| 1238 | deq.b16Gain | deq | CONDITIONAL | state 3, 5.9 dB |
| 1239 | deq.b16Q | deq | DEAD |  |
| 1240 | deq.b16Slope | deq | DEAD |  |
| 1241 | deq.b16Placement | deq | CONDITIONAL | state 3, -3.9 dB |
| 1242 | deq.b16Pan | deq | DEAD |  |
| 1243 | deq.b16Solo | deq | CONDITIONAL | state 3, -1.9 dB |
| 1244 | deq.b16Dyn | deq | DEAD |  |
| 1245 | deq.b16Thresh | deq | DEAD |  |
| 1246 | deq.b16Range | deq | DEAD |  |
| 1247 | deq.b16Attack | deq | DEAD |  |
| 1248 | deq.b16Release | deq | DEAD |  |
| 1249 | deq.b16Action | deq | DEAD |  |
| 1250 | deq.b16Knee | deq | DEAD |  |
| 1251 | deq.b16Source | deq | DEAD |  |
| 1252 | deq.b16Relative | deq | DEAD |  |
| 1253 | deq.b16Lookahead | deq | CONDITIONAL | state 4, 6.0 dB |
| 1254 | deq.b16Transient | deq | CONDITIONAL | state 1, -12.1 dB |
| 1255 | deq.b16Auto | deq | CONDITIONAL | state 4, 6.0 dB |
| 1256 | deq.b16DetLink | deq | CONDITIONAL | state 4, 6.0 dB |
| 1257 | deq.b16DetFreq | deq | CONDITIONAL | state 4, 4.5 dB |
| 1258 | deq.b16DetQ | deq | DEAD |  |
| 1259 | deq.b16BelowOn | deq | DEAD |  |
| 1260 | deq.b16BelowRange | deq | DEAD |  |
| 1261 | deq.b16BelowRatio | deq | DEAD |  |
| 1262 | deq.b16AboveOn | deq | DEAD |  |
| 1263 | deq.b16AboveRange | deq | CONDITIONAL | state 4, 6.0 dB |
| 1264 | deq.b16AboveRatio | deq | DEAD |  |
| 1265 | deq.b16RelFree | deq | CONDITIONAL | state 4, 6.0 dB |
| 1266 | deq.b16RelFreq | deq | DEAD |  |
| 1267 | deq.b16RelQ | deq | CONDITIONAL | state 4, 1.9 dB |
| 1268 | deq.b16RelSource | deq | DEAD |  |
| 1269 | deq.b17On | deq | CONDITIONAL | state 1, -12.9 dB |
| 1270 | deq.b17Type | deq | CONDITIONAL | state 2, 3.4 dB |
| 1271 | deq.b17Freq | deq | CONDITIONAL | state 2, 0.6 dB |
| 1272 | deq.b17Gain | deq | CONDITIONAL | state 3, 6.0 dB |
| 1273 | deq.b17Q | deq | CONDITIONAL | state 3, 6.0 dB |
| 1274 | deq.b17Slope | deq | DEAD |  |
| 1275 | deq.b17Placement | deq | CONDITIONAL | state 2, -0.7 dB |
| 1276 | deq.b17Pan | deq | DEAD |  |
| 1277 | deq.b17Solo | deq | CONDITIONAL | state 3, 3.5 dB |
| 1278 | deq.b17Dyn | deq | CONDITIONAL | state 3, 2.3 dB |
| 1279 | deq.b17Thresh | deq | DEAD |  |
| 1280 | deq.b17Range | deq | DEAD |  |
| 1281 | deq.b17Attack | deq | DEAD |  |
| 1282 | deq.b17Release | deq | DEAD |  |
| 1283 | deq.b17Action | deq | CONDITIONAL | state 4, 6.0 dB |
| 1284 | deq.b17Knee | deq | DEAD |  |
| 1285 | deq.b17Source | deq | CONDITIONAL | state 5, 1.6 dB |
| 1286 | deq.b17Relative | deq | CONDITIONAL | state 5, 1.6 dB |
| 1287 | deq.b17Lookahead | deq | CONDITIONAL | state 5, -5.3 dB |
| 1288 | deq.b17Transient | deq | CONDITIONAL | state 5, -14.1 dB |
| 1289 | deq.b17Auto | deq | CONDITIONAL | state 5, -0.5 dB |
| 1290 | deq.b17DetLink | deq | CONDITIONAL | state 5, 0.5 dB |
| 1291 | deq.b17DetFreq | deq | CONDITIONAL | state 5, 1.6 dB |
| 1292 | deq.b17DetQ | deq | CONDITIONAL | state 5, 1.6 dB |
| 1293 | deq.b17BelowOn | deq | CONDITIONAL | state 5, -19.9 dB |
| 1294 | deq.b17BelowRange | deq | DEAD |  |
| 1295 | deq.b17BelowRatio | deq | DEAD |  |
| 1296 | deq.b17AboveOn | deq | CONDITIONAL | state 5, 1.6 dB |
| 1297 | deq.b17AboveRange | deq | CONDITIONAL | state 5, 3.1 dB |
| 1298 | deq.b17AboveRatio | deq | CONDITIONAL | state 5, 1.6 dB |
| 1299 | deq.b17RelFree | deq | CONDITIONAL | state 5, -8.2 dB |
| 1300 | deq.b17RelFreq | deq | CONDITIONAL | state 5, -13.1 dB |
| 1301 | deq.b17RelQ | deq | CONDITIONAL | state 5, -6.4 dB |
| 1302 | deq.b17RelSource | deq | CONDITIONAL | state 5, 0.7 dB |
| 1303 | deq.b18On | deq | CONDITIONAL | state 1, -18.2 dB |
| 1304 | deq.b18Type | deq | CONDITIONAL | state 4, -10.7 dB |
| 1305 | deq.b18Freq | deq | DEAD |  |
| 1306 | deq.b18Gain | deq | CONDITIONAL | state 4, -11.8 dB |
| 1307 | deq.b18Q | deq | DEAD |  |
| 1308 | deq.b18Slope | deq | DEAD |  |
| 1309 | deq.b18Placement | deq | DEAD |  |
| 1310 | deq.b18Pan | deq | DEAD |  |
| 1311 | deq.b18Solo | deq | CONDITIONAL | state 3, 6.0 dB |
| 1312 | deq.b18Dyn | deq | CONDITIONAL | state 4, 6.0 dB |
| 1313 | deq.b18Thresh | deq | DEAD |  |
| 1314 | deq.b18Range | deq | DEAD |  |
| 1315 | deq.b18Attack | deq | DEAD |  |
| 1316 | deq.b18Release | deq | DEAD |  |
| 1317 | deq.b18Action | deq | DEAD |  |
| 1318 | deq.b18Knee | deq | DEAD |  |
| 1319 | deq.b18Source | deq | DEAD |  |
| 1320 | deq.b18Relative | deq | DEAD |  |
| 1321 | deq.b18Lookahead | deq | DEAD |  |
| 1322 | deq.b18Transient | deq | DEAD |  |
| 1323 | deq.b18Auto | deq | DEAD |  |
| 1324 | deq.b18DetLink | deq | DEAD |  |
| 1325 | deq.b18DetFreq | deq | CONDITIONAL | state 3, -2.1 dB |
| 1326 | deq.b18DetQ | deq | DEAD |  |
| 1327 | deq.b18BelowOn | deq | CONDITIONAL | state 3, -6.0 dB |
| 1328 | deq.b18BelowRange | deq | DEAD |  |
| 1329 | deq.b18BelowRatio | deq | CONDITIONAL | state 3, -6.9 dB |
| 1330 | deq.b18AboveOn | deq | DEAD |  |
| 1331 | deq.b18AboveRange | deq | DEAD |  |
| 1332 | deq.b18AboveRatio | deq | CONDITIONAL | state 3, -10.6 dB |
| 1333 | deq.b18RelFree | deq | CONDITIONAL | state 3, -6.0 dB |
| 1334 | deq.b18RelFreq | deq | DEAD |  |
| 1335 | deq.b18RelQ | deq | DEAD |  |
| 1336 | deq.b18RelSource | deq | DEAD |  |
| 1337 | deq.b19On | deq | CONDITIONAL | state 1, -4.2 dB |
| 1338 | deq.b19Type | deq | CONDITIONAL | state 1, -0.9 dB |
| 1339 | deq.b19Freq | deq | CONDITIONAL | state 1, -1.9 dB |
| 1340 | deq.b19Gain | deq | CONDITIONAL | state 1, 5.0 dB |
| 1341 | deq.b19Q | deq | CONDITIONAL | state 3, -1.7 dB |
| 1342 | deq.b19Slope | deq | CONDITIONAL | state 3, -6.0 dB |
| 1343 | deq.b19Placement | deq | CONDITIONAL | state 1, -4.3 dB |
| 1344 | deq.b19Pan | deq | DEAD |  |
| 1345 | deq.b19Solo | deq | CONDITIONAL | state 1, -4.2 dB |
| 1346 | deq.b19Dyn | deq | CONDITIONAL | state 1, 2.6 dB |
| 1347 | deq.b19Thresh | deq | CONDITIONAL | state 3, -6.9 dB |
| 1348 | deq.b19Range | deq | CONDITIONAL | state 3, -2.4 dB |
| 1349 | deq.b19Attack | deq | CONDITIONAL | state 3, -6.0 dB |
| 1350 | deq.b19Release | deq | CONDITIONAL | state 3, -13.4 dB |
| 1351 | deq.b19Action | deq | CONDITIONAL | state 4, -18.6 dB |
| 1352 | deq.b19Knee | deq | DEAD |  |
| 1353 | deq.b19Source | deq | CONDITIONAL | state 3, -13.4 dB |
| 1354 | deq.b19Relative | deq | CONDITIONAL | state 3, -6.0 dB |
| 1355 | deq.b19Lookahead | deq | CONDITIONAL | state 3, -29.3 dB |
| 1356 | deq.b19Transient | deq | CONDITIONAL | state 3, -15.8 dB |
| 1357 | deq.b19Auto | deq | CONDITIONAL | state 3, -32.1 dB |
| 1358 | deq.b19DetLink | deq | CONDITIONAL | state 3, -6.0 dB |
| 1359 | deq.b19DetFreq | deq | CONDITIONAL | state 3, -9.5 dB |
| 1360 | deq.b19DetQ | deq | CONDITIONAL | state 3, -5.9 dB |
| 1361 | deq.b19BelowOn | deq | CONDITIONAL | state 3, -13.4 dB |
| 1362 | deq.b19BelowRange | deq | CONDITIONAL | state 3, -12.1 dB |
| 1363 | deq.b19BelowRatio | deq | CONDITIONAL | state 3, -6.0 dB |
| 1364 | deq.b19AboveOn | deq | CONDITIONAL | state 3, -29.7 dB |
| 1365 | deq.b19AboveRange | deq | CONDITIONAL | state 3, -5.7 dB |
| 1366 | deq.b19AboveRatio | deq | CONDITIONAL | state 3, -13.6 dB |
| 1367 | deq.b19RelFree | deq | CONDITIONAL | state 3, -23.4 dB |
| 1368 | deq.b19RelFreq | deq | DEAD |  |
| 1369 | deq.b19RelQ | deq | DEAD |  |
| 1370 | deq.b19RelSource | deq | CONDITIONAL | state 3, -20.9 dB |
| 1371 | deq.b20On | deq | CONDITIONAL | state 1, -21.2 dB |
| 1372 | deq.b20Type | deq | CONDITIONAL | state 1, -22.0 dB |
| 1373 | deq.b20Freq | deq | CONDITIONAL | state 1, -1.1 dB |
| 1374 | deq.b20Gain | deq | CONDITIONAL | state 1, 0.6 dB |
| 1375 | deq.b20Q | deq | CONDITIONAL | state 1, -10.7 dB |
| 1376 | deq.b20Slope | deq | DEAD |  |
| 1377 | deq.b20Placement | deq | CONDITIONAL | state 1, -20.7 dB |
| 1378 | deq.b20Pan | deq | DEAD |  |
| 1379 | deq.b20Solo | deq | CONDITIONAL | state 1, -21.2 dB |
| 1380 | deq.b20Dyn | deq | CONDITIONAL | state 1, -5.1 dB |
| 1381 | deq.b20Thresh | deq | DEAD |  |
| 1382 | deq.b20Range | deq | DEAD |  |
| 1383 | deq.b20Attack | deq | DEAD |  |
| 1384 | deq.b20Release | deq | DEAD |  |
| 1385 | deq.b20Action | deq | DEAD |  |
| 1386 | deq.b20Knee | deq | DEAD |  |
| 1387 | deq.b20Source | deq | DEAD |  |
| 1388 | deq.b20Relative | deq | DEAD |  |
| 1389 | deq.b20Lookahead | deq | DEAD |  |
| 1390 | deq.b20Transient | deq | DEAD |  |
| 1391 | deq.b20Auto | deq | DEAD |  |
| 1392 | deq.b20DetLink | deq | DEAD |  |
| 1393 | deq.b20DetFreq | deq | DEAD |  |
| 1394 | deq.b20DetQ | deq | DEAD |  |
| 1395 | deq.b20BelowOn | deq | DEAD |  |
| 1396 | deq.b20BelowRange | deq | DEAD |  |
| 1397 | deq.b20BelowRatio | deq | DEAD |  |
| 1398 | deq.b20AboveOn | deq | DEAD |  |
| 1399 | deq.b20AboveRange | deq | DEAD |  |
| 1400 | deq.b20AboveRatio | deq | DEAD |  |
| 1401 | deq.b20RelFree | deq | DEAD |  |
| 1402 | deq.b20RelFreq | deq | DEAD |  |
| 1403 | deq.b20RelQ | deq | DEAD |  |
| 1404 | deq.b20RelSource | deq | DEAD |  |
| 1405 | deq.b21On | deq | CONDITIONAL | state 1, -5.1 dB |
| 1406 | deq.b21Type | deq | CONDITIONAL | state 1, -5.9 dB |
| 1407 | deq.b21Freq | deq | CONDITIONAL | state 1, -8.7 dB |
| 1408 | deq.b21Gain | deq | CONDITIONAL | state 1, 3.0 dB |
| 1409 | deq.b21Q | deq | CONDITIONAL | state 2, -0.3 dB |
| 1410 | deq.b21Slope | deq | CONDITIONAL | state 2, -0.3 dB |
| 1411 | deq.b21Placement | deq | CONDITIONAL | state 1, -26.0 dB |
| 1412 | deq.b21Pan | deq | CONDITIONAL | state 1, -13.6 dB |
| 1413 | deq.b21Solo | deq | CONDITIONAL | state 1, -5.1 dB |
| 1414 | deq.b21Dyn | deq | CONDITIONAL | state 1, -5.0 dB |
| 1415 | deq.b21Thresh | deq | CONDITIONAL | state 2, -0.3 dB |
| 1416 | deq.b21Range | deq | CONDITIONAL | state 2, 4.6 dB |
| 1417 | deq.b21Attack | deq | CONDITIONAL | state 2, -0.3 dB |
| 1418 | deq.b21Release | deq | CONDITIONAL | state 2, -0.3 dB |
| 1419 | deq.b21Action | deq | CONDITIONAL | state 2, -0.3 dB |
| 1420 | deq.b21Knee | deq | CONDITIONAL | state 2, -0.3 dB |
| 1421 | deq.b21Source | deq | CONDITIONAL | state 2, -0.3 dB |
| 1422 | deq.b21Relative | deq | CONDITIONAL | state 2, 1.5 dB |
| 1423 | deq.b21Lookahead | deq | CONDITIONAL | state 2, -0.3 dB |
| 1424 | deq.b21Transient | deq | CONDITIONAL | state 2, -0.3 dB |
| 1425 | deq.b21Auto | deq | CONDITIONAL | state 2, -0.3 dB |
| 1426 | deq.b21DetLink | deq | CONDITIONAL | state 2, -0.3 dB |
| 1427 | deq.b21DetFreq | deq | CONDITIONAL | state 2, -0.3 dB |
| 1428 | deq.b21DetQ | deq | CONDITIONAL | state 2, -0.3 dB |
| 1429 | deq.b21BelowOn | deq | CONDITIONAL | state 2, -0.3 dB |
| 1430 | deq.b21BelowRange | deq | CONDITIONAL | state 2, -0.3 dB |
| 1431 | deq.b21BelowRatio | deq | CONDITIONAL | state 2, -0.3 dB |
| 1432 | deq.b21AboveOn | deq | CONDITIONAL | state 2, -0.3 dB |
| 1433 | deq.b21AboveRange | deq | CONDITIONAL | state 2, -0.3 dB |
| 1434 | deq.b21AboveRatio | deq | CONDITIONAL | state 2, -0.2 dB |
| 1435 | deq.b21RelFree | deq | CONDITIONAL | state 2, -0.3 dB |
| 1436 | deq.b21RelFreq | deq | CONDITIONAL | state 2, 4.2 dB |
| 1437 | deq.b21RelQ | deq | CONDITIONAL | state 2, -0.3 dB |
| 1438 | deq.b21RelSource | deq | CONDITIONAL | state 2, -0.3 dB |
| 1439 | deq.b22On | deq | CONDITIONAL | state 2, 4.8 dB |
| 1440 | deq.b22Type | deq | CONDITIONAL | state 2, 5.0 dB |
| 1441 | deq.b22Freq | deq | CONDITIONAL | state 2, 5.3 dB |
| 1442 | deq.b22Gain | deq | CONDITIONAL | state 2, 6.0 dB |
| 1443 | deq.b22Q | deq | CONDITIONAL | state 2, -0.3 dB |
| 1444 | deq.b22Slope | deq | CONDITIONAL | state 2, -0.2 dB |
| 1445 | deq.b22Placement | deq | CONDITIONAL | state 1, -9.1 dB |
| 1446 | deq.b22Pan | deq | DEAD |  |
| 1447 | deq.b22Solo | deq | CONDITIONAL | state 2, 0.3 dB |
| 1448 | deq.b22Dyn | deq | DEAD |  |
| 1449 | deq.b22Thresh | deq | DEAD |  |
| 1450 | deq.b22Range | deq | DEAD |  |
| 1451 | deq.b22Attack | deq | DEAD |  |
| 1452 | deq.b22Release | deq | DEAD |  |
| 1453 | deq.b22Action | deq | DEAD |  |
| 1454 | deq.b22Knee | deq | DEAD |  |
| 1455 | deq.b22Source | deq | DEAD |  |
| 1456 | deq.b22Relative | deq | DEAD |  |
| 1457 | deq.b22Lookahead | deq | DEAD |  |
| 1458 | deq.b22Transient | deq | DEAD |  |
| 1459 | deq.b22Auto | deq | DEAD |  |
| 1460 | deq.b22DetLink | deq | DEAD |  |
| 1461 | deq.b22DetFreq | deq | DEAD |  |
| 1462 | deq.b22DetQ | deq | DEAD |  |
| 1463 | deq.b22BelowOn | deq | DEAD |  |
| 1464 | deq.b22BelowRange | deq | DEAD |  |
| 1465 | deq.b22BelowRatio | deq | DEAD |  |
| 1466 | deq.b22AboveOn | deq | DEAD |  |
| 1467 | deq.b22AboveRange | deq | CONDITIONAL | state 4, -6.1 dB |
| 1468 | deq.b22AboveRatio | deq | DEAD |  |
| 1469 | deq.b22RelFree | deq | DEAD |  |
| 1470 | deq.b22RelFreq | deq | DEAD |  |
| 1471 | deq.b22RelQ | deq | DEAD |  |
| 1472 | deq.b22RelSource | deq | DEAD |  |
| 1473 | deq.b23On | deq | CONDITIONAL | state 1, -8.0 dB |
| 1474 | deq.b23Type | deq | CONDITIONAL | state 3, -5.1 dB |
| 1475 | deq.b23Freq | deq | CONDITIONAL | state 3, -6.4 dB |
| 1476 | deq.b23Gain | deq | CONDITIONAL | state 3, 2.6 dB |
| 1477 | deq.b23Q | deq | DEAD |  |
| 1478 | deq.b23Slope | deq | CONDITIONAL | state 5, -25.6 dB |
| 1479 | deq.b23Placement | deq | CONDITIONAL | state 3, -12.5 dB |
| 1480 | deq.b23Pan | deq | DEAD |  |
| 1481 | deq.b23Solo | deq | CONDITIONAL | state 3, -22.4 dB |
| 1482 | deq.b23Dyn | deq | CONDITIONAL | state 3, -5.0 dB |
| 1483 | deq.b23Thresh | deq | DEAD |  |
| 1484 | deq.b23Range | deq | DEAD |  |
| 1485 | deq.b23Attack | deq | CONDITIONAL | state 2, -13.6 dB |
| 1486 | deq.b23Release | deq | CONDITIONAL | state 2, -9.0 dB |
| 1487 | deq.b23Action | deq | DEAD |  |
| 1488 | deq.b23Knee | deq | DEAD |  |
| 1489 | deq.b23Source | deq | CONDITIONAL | state 2, -7.4 dB |
| 1490 | deq.b23Relative | deq | CONDITIONAL | state 2, -0.5 dB |
| 1491 | deq.b23Lookahead | deq | CONDITIONAL | state 2, -6.0 dB |
| 1492 | deq.b23Transient | deq | CONDITIONAL | state 2, -13.0 dB |
| 1493 | deq.b23Auto | deq | CONDITIONAL | state 2, -9.0 dB |
| 1494 | deq.b23DetLink | deq | CONDITIONAL | state 2, -9.0 dB |
| 1495 | deq.b23DetFreq | deq | CONDITIONAL | state 2, -9.0 dB |
| 1496 | deq.b23DetQ | deq | CONDITIONAL | state 2, -7.0 dB |
| 1497 | deq.b23BelowOn | deq | DEAD |  |
| 1498 | deq.b23BelowRange | deq | DEAD |  |
| 1499 | deq.b23BelowRatio | deq | DEAD |  |
| 1500 | deq.b23AboveOn | deq | CONDITIONAL | state 2, -4.3 dB |
| 1501 | deq.b23AboveRange | deq | CONDITIONAL | state 2, -4.5 dB |
| 1502 | deq.b23AboveRatio | deq | CONDITIONAL | state 2, -0.5 dB |
| 1503 | deq.b23RelFree | deq | DEAD |  |
| 1504 | deq.b23RelFreq | deq | DEAD |  |
| 1505 | deq.b23RelQ | deq | CONDITIONAL | state 5, -6.0 dB |
| 1506 | deq.b23RelSource | deq | CONDITIONAL | state 2, -7.4 dB |
| 1507 | deq.b24On | deq | CONDITIONAL | state 1, -5.0 dB |
| 1508 | deq.b24Type | deq | DEAD |  |
| 1509 | deq.b24Freq | deq | DEAD |  |
| 1510 | deq.b24Gain | deq | DEAD |  |
| 1511 | deq.b24Q | deq | DEAD |  |
| 1512 | deq.b24Slope | deq | DEAD |  |
| 1513 | deq.b24Placement | deq | DEAD |  |
| 1514 | deq.b24Pan | deq | DEAD |  |
| 1515 | deq.b24Solo | deq | CONDITIONAL | state 2, 3.7 dB |
| 1516 | deq.b24Dyn | deq | DEAD |  |
| 1517 | deq.b24Thresh | deq | DEAD |  |
| 1518 | deq.b24Range | deq | DEAD |  |
| 1519 | deq.b24Attack | deq | DEAD |  |
| 1520 | deq.b24Release | deq | DEAD |  |
| 1521 | deq.b24Action | deq | DEAD |  |
| 1522 | deq.b24Knee | deq | DEAD |  |
| 1523 | deq.b24Source | deq | DEAD |  |
| 1524 | deq.b24Relative | deq | DEAD |  |
| 1525 | deq.b24Lookahead | deq | DEAD |  |
| 1526 | deq.b24Transient | deq | DEAD |  |
| 1527 | deq.b24Auto | deq | DEAD |  |
| 1528 | deq.b24DetLink | deq | DEAD |  |
| 1529 | deq.b24DetFreq | deq | DEAD |  |
| 1530 | deq.b24DetQ | deq | DEAD |  |
| 1531 | deq.b24BelowOn | deq | DEAD |  |
| 1532 | deq.b24BelowRange | deq | DEAD |  |
| 1533 | deq.b24BelowRatio | deq | DEAD |  |
| 1534 | deq.b24AboveOn | deq | DEAD |  |
| 1535 | deq.b24AboveRange | deq | DEAD |  |
| 1536 | deq.b24AboveRatio | deq | DEAD |  |
| 1537 | deq.b24RelFree | deq | DEAD |  |
| 1538 | deq.b24RelFreq | deq | DEAD |  |
| 1539 | deq.b24RelQ | deq | DEAD |  |
| 1540 | deq.b24RelSource | deq | DEAD |  |
| 1541 | deq.b25On | deq | CONDITIONAL | state 2, 6.0 dB |
| 1542 | deq.b25Type | deq | DEAD |  |
| 1543 | deq.b25Freq | deq | DEAD |  |
| 1544 | deq.b25Gain | deq | DEAD |  |
| 1545 | deq.b25Q | deq | DEAD |  |
| 1546 | deq.b25Slope | deq | DEAD |  |
| 1547 | deq.b25Placement | deq | DEAD |  |
| 1548 | deq.b25Pan | deq | DEAD |  |
| 1549 | deq.b25Solo | deq | CONDITIONAL | state 1, -8.0 dB |
| 1550 | deq.b25Dyn | deq | DEAD |  |
| 1551 | deq.b25Thresh | deq | DEAD |  |
| 1552 | deq.b25Range | deq | DEAD |  |
| 1553 | deq.b25Attack | deq | DEAD |  |
| 1554 | deq.b25Release | deq | DEAD |  |
| 1555 | deq.b25Action | deq | DEAD |  |
| 1556 | deq.b25Knee | deq | DEAD |  |
| 1557 | deq.b25Source | deq | DEAD |  |
| 1558 | deq.b25Relative | deq | DEAD |  |
| 1559 | deq.b25Lookahead | deq | DEAD |  |
| 1560 | deq.b25Transient | deq | DEAD |  |
| 1561 | deq.b25Auto | deq | DEAD |  |
| 1562 | deq.b25DetLink | deq | DEAD |  |
| 1563 | deq.b25DetFreq | deq | DEAD |  |
| 1564 | deq.b25DetQ | deq | DEAD |  |
| 1565 | deq.b25BelowOn | deq | DEAD |  |
| 1566 | deq.b25BelowRange | deq | DEAD |  |
| 1567 | deq.b25BelowRatio | deq | DEAD |  |
| 1568 | deq.b25AboveOn | deq | CONDITIONAL | state 1, -46.6 dB |
| 1569 | deq.b25AboveRange | deq | DEAD |  |
| 1570 | deq.b25AboveRatio | deq | DEAD |  |
| 1571 | deq.b25RelFree | deq | DEAD |  |
| 1572 | deq.b25RelFreq | deq | DEAD |  |
| 1573 | deq.b25RelQ | deq | DEAD |  |
| 1574 | deq.b25RelSource | deq | DEAD |  |
| 1575 | deq.b26On | deq | CONDITIONAL | state 3, -7.8 dB |
| 1576 | deq.b26Type | deq | CONDITIONAL | state 3, -2.3 dB |
| 1577 | deq.b26Freq | deq | CONDITIONAL | state 3, -7.0 dB |
| 1578 | deq.b26Gain | deq | CONDITIONAL | state 4, 4.1 dB |
| 1579 | deq.b26Q | deq | CONDITIONAL | state 3, -6.1 dB |
| 1580 | deq.b26Slope | deq | DEAD |  |
| 1581 | deq.b26Placement | deq | CONDITIONAL | state 3, -5.5 dB |
| 1582 | deq.b26Pan | deq | DEAD |  |
| 1583 | deq.b26Solo | deq | CONDITIONAL | state 2, -18.8 dB |
| 1584 | deq.b26Dyn | deq | CONDITIONAL | state 4, -5.0 dB |
| 1585 | deq.b26Thresh | deq | DEAD |  |
| 1586 | deq.b26Range | deq | DEAD |  |
| 1587 | deq.b26Attack | deq | DEAD |  |
| 1588 | deq.b26Release | deq | DEAD |  |
| 1589 | deq.b26Action | deq | DEAD |  |
| 1590 | deq.b26Knee | deq | DEAD |  |
| 1591 | deq.b26Source | deq | CONDITIONAL | state 4, -37.7 dB |
| 1592 | deq.b26Relative | deq | CONDITIONAL | state 4, -36.8 dB |
| 1593 | deq.b26Lookahead | deq | CONDITIONAL | state 4, -5.0 dB |
| 1594 | deq.b26Transient | deq | CONDITIONAL | state 4, -25.9 dB |
| 1595 | deq.b26Auto | deq | CONDITIONAL | state 4, -36.8 dB |
| 1596 | deq.b26DetLink | deq | CONDITIONAL | state 4, -33.9 dB |
| 1597 | deq.b26DetFreq | deq | DEAD |  |
| 1598 | deq.b26DetQ | deq | DEAD |  |
| 1599 | deq.b26BelowOn | deq | CONDITIONAL | state 4, -21.6 dB |
| 1600 | deq.b26BelowRange | deq | DEAD |  |
| 1601 | deq.b26BelowRatio | deq | DEAD |  |
| 1602 | deq.b26AboveOn | deq | CONDITIONAL | state 4, -23.2 dB |
| 1603 | deq.b26AboveRange | deq | CONDITIONAL | state 4, -23.9 dB |
| 1604 | deq.b26AboveRatio | deq | CONDITIONAL | state 4, -20.4 dB |
| 1605 | deq.b26RelFree | deq | CONDITIONAL | state 2, 2.3 dB |
| 1606 | deq.b26RelFreq | deq | DEAD |  |
| 1607 | deq.b26RelQ | deq | CONDITIONAL | state 2, 0.5 dB |
| 1608 | deq.b26RelSource | deq | DEAD |  |
| 1609 | deq.b27On | deq | CONDITIONAL | state 1, -5.0 dB |
| 1610 | deq.b27Type | deq | CONDITIONAL | state 1, -5.1 dB |
| 1611 | deq.b27Freq | deq | CONDITIONAL | state 1, -7.9 dB |
| 1612 | deq.b27Gain | deq | CONDITIONAL | state 1, 2.9 dB |
| 1613 | deq.b27Q | deq | CONDITIONAL | state 1, -4.8 dB |
| 1614 | deq.b27Slope | deq | DEAD |  |
| 1615 | deq.b27Placement | deq | CONDITIONAL | state 1, -5.8 dB |
| 1616 | deq.b27Pan | deq | DEAD |  |
| 1617 | deq.b27Solo | deq | CONDITIONAL | state 4, -30.4 dB |
| 1618 | deq.b27Dyn | deq | CONDITIONAL | state 1, -5.0 dB |
| 1619 | deq.b27Thresh | deq | CONDITIONAL | state 1, -41.7 dB |
| 1620 | deq.b27Range | deq | DEAD |  |
| 1621 | deq.b27Attack | deq | CONDITIONAL | state 4, -51.2 dB |
| 1622 | deq.b27Release | deq | CONDITIONAL | state 1, -44.2 dB |
| 1623 | deq.b27Action | deq | DEAD |  |
| 1624 | deq.b27Knee | deq | DEAD |  |
| 1625 | deq.b27Source | deq | CONDITIONAL | state 1, -13.0 dB |
| 1626 | deq.b27Relative | deq | CONDITIONAL | state 1, -23.9 dB |
| 1627 | deq.b27Lookahead | deq | CONDITIONAL | state 1, -5.0 dB |
| 1628 | deq.b27Transient | deq | CONDITIONAL | state 1, -46.5 dB |
| 1629 | deq.b27Auto | deq | CONDITIONAL | state 4, -47.6 dB |
| 1630 | deq.b27DetLink | deq | CONDITIONAL | state 1, -43.5 dB |
| 1631 | deq.b27DetFreq | deq | CONDITIONAL | state 1, -46.4 dB |
| 1632 | deq.b27DetQ | deq | CONDITIONAL | state 1, -13.0 dB |
| 1633 | deq.b27BelowOn | deq | CONDITIONAL | state 4, -60.5 dB |
| 1634 | deq.b27BelowRange | deq | CONDITIONAL | state 4, -52.8 dB |
| 1635 | deq.b27BelowRatio | deq | CONDITIONAL | state 4, -60.5 dB |
| 1636 | deq.b27AboveOn | deq | CONDITIONAL | state 1, -4.6 dB |
| 1637 | deq.b27AboveRange | deq | CONDITIONAL | state 1, 3.0 dB |
| 1638 | deq.b27AboveRatio | deq | CONDITIONAL | state 1, -13.0 dB |
| 1639 | deq.b27RelFree | deq | DEAD |  |
| 1640 | deq.b27RelFreq | deq | DEAD |  |
| 1641 | deq.b27RelQ | deq | DEAD |  |
| 1642 | deq.b27RelSource | deq | CONDITIONAL | state 1, -41.6 dB |
| 1643 | deq.b28On | deq | CONDITIONAL | state 1, 1.2 dB |
| 1644 | deq.b28Type | deq | CONDITIONAL | state 3, -1.8 dB |
| 1645 | deq.b28Freq | deq | CONDITIONAL | state 5, -5.9 dB |
| 1646 | deq.b28Gain | deq | DEAD |  |
| 1647 | deq.b28Q | deq | DEAD |  |
| 1648 | deq.b28Slope | deq | DEAD |  |
| 1649 | deq.b28Placement | deq | CONDITIONAL | state 5, -9.8 dB |
| 1650 | deq.b28Pan | deq | DEAD |  |
| 1651 | deq.b28Solo | deq | DEAD |  |
| 1652 | deq.b28Dyn | deq | DEAD |  |
| 1653 | deq.b28Thresh | deq | DEAD |  |
| 1654 | deq.b28Range | deq | DEAD |  |
| 1655 | deq.b28Attack | deq | DEAD |  |
| 1656 | deq.b28Release | deq | DEAD |  |
| 1657 | deq.b28Action | deq | DEAD |  |
| 1658 | deq.b28Knee | deq | DEAD |  |
| 1659 | deq.b28Source | deq | DEAD |  |
| 1660 | deq.b28Relative | deq | DEAD |  |
| 1661 | deq.b28Lookahead | deq | DEAD |  |
| 1662 | deq.b28Transient | deq | DEAD |  |
| 1663 | deq.b28Auto | deq | DEAD |  |
| 1664 | deq.b28DetLink | deq | DEAD |  |
| 1665 | deq.b28DetFreq | deq | DEAD |  |
| 1666 | deq.b28DetQ | deq | DEAD |  |
| 1667 | deq.b28BelowOn | deq | CONDITIONAL | state 2, 4.9 dB |
| 1668 | deq.b28BelowRange | deq | DEAD |  |
| 1669 | deq.b28BelowRatio | deq | DEAD |  |
| 1670 | deq.b28AboveOn | deq | DEAD |  |
| 1671 | deq.b28AboveRange | deq | DEAD |  |
| 1672 | deq.b28AboveRatio | deq | DEAD |  |
| 1673 | deq.b28RelFree | deq | DEAD |  |
| 1674 | deq.b28RelFreq | deq | DEAD |  |
| 1675 | deq.b28RelQ | deq | DEAD |  |
| 1676 | deq.b28RelSource | deq | DEAD |  |
| 1677 | deq.b29On | deq | CONDITIONAL | state 1, 0.9 dB |
| 1678 | deq.b29Type | deq | CONDITIONAL | state 1, 6.0 dB |
| 1679 | deq.b29Freq | deq | CONDITIONAL | state 1, 1.4 dB |
| 1680 | deq.b29Gain | deq | CONDITIONAL | state 1, 3.4 dB |
| 1681 | deq.b29Q | deq | DEAD |  |
| 1682 | deq.b29Slope | deq | DEAD |  |
| 1683 | deq.b29Placement | deq | CONDITIONAL | state 1, 6.0 dB |
| 1684 | deq.b29Pan | deq | CONDITIONAL | state 1, 1.9 dB |
| 1685 | deq.b29Solo | deq | CONDITIONAL | state 1, -0.9 dB |
| 1686 | deq.b29Dyn | deq | CONDITIONAL | state 4, -5.0 dB |
| 1687 | deq.b29Thresh | deq | DEAD |  |
| 1688 | deq.b29Range | deq | DEAD |  |
| 1689 | deq.b29Attack | deq | DEAD |  |
| 1690 | deq.b29Release | deq | DEAD |  |
| 1691 | deq.b29Action | deq | DEAD |  |
| 1692 | deq.b29Knee | deq | DEAD |  |
| 1693 | deq.b29Source | deq | CONDITIONAL | state 4, -11.8 dB |
| 1694 | deq.b29Relative | deq | CONDITIONAL | state 4, -11.8 dB |
| 1695 | deq.b29Lookahead | deq | CONDITIONAL | state 4, -5.1 dB |
| 1696 | deq.b29Transient | deq | CONDITIONAL | state 4, -26.7 dB |
| 1697 | deq.b29Auto | deq | CONDITIONAL | state 4, -21.6 dB |
| 1698 | deq.b29DetLink | deq | CONDITIONAL | state 4, -11.8 dB |
| 1699 | deq.b29DetFreq | deq | DEAD |  |
| 1700 | deq.b29DetQ | deq | DEAD |  |
| 1701 | deq.b29BelowOn | deq | CONDITIONAL | state 4, -35.3 dB |
| 1702 | deq.b29BelowRange | deq | DEAD |  |
| 1703 | deq.b29BelowRatio | deq | DEAD |  |
| 1704 | deq.b29AboveOn | deq | CONDITIONAL | state 4, -3.2 dB |
| 1705 | deq.b29AboveRange | deq | CONDITIONAL | state 4, -3.8 dB |
| 1706 | deq.b29AboveRatio | deq | CONDITIONAL | state 4, -11.8 dB |
| 1707 | deq.b29RelFree | deq | CONDITIONAL | state 4, -9.7 dB |
| 1708 | deq.b29RelFreq | deq | DEAD |  |
| 1709 | deq.b29RelQ | deq | DEAD |  |
| 1710 | deq.b29RelSource | deq | CONDITIONAL | state 4, -9.7 dB |
| 1711 | deq.b30On | deq | CONDITIONAL | state 1, -0.2 dB |
| 1712 | deq.b30Type | deq | CONDITIONAL | state 1, 2.8 dB |
| 1713 | deq.b30Freq | deq | CONDITIONAL | state 1, 2.8 dB |
| 1714 | deq.b30Gain | deq | CONDITIONAL | state 1, 3.3 dB |
| 1715 | deq.b30Q | deq | CONDITIONAL | state 2, -15.2 dB |
| 1716 | deq.b30Slope | deq | CONDITIONAL | state 1, -0.5 dB |
| 1717 | deq.b30Placement | deq | CONDITIONAL | state 1, -0.5 dB |
| 1718 | deq.b30Pan | deq | DEAD |  |
| 1719 | deq.b30Solo | deq | CONDITIONAL | state 1, -0.2 dB |
| 1720 | deq.b30Dyn | deq | CONDITIONAL | state 1, -1.8 dB |
| 1721 | deq.b30Thresh | deq | DEAD |  |
| 1722 | deq.b30Range | deq | DEAD |  |
| 1723 | deq.b30Attack | deq | DEAD |  |
| 1724 | deq.b30Release | deq | DEAD |  |
| 1725 | deq.b30Action | deq | DEAD |  |
| 1726 | deq.b30Knee | deq | DEAD |  |
| 1727 | deq.b30Source | deq | DEAD |  |
| 1728 | deq.b30Relative | deq | DEAD |  |
| 1729 | deq.b30Lookahead | deq | DEAD |  |
| 1730 | deq.b30Transient | deq | DEAD |  |
| 1731 | deq.b30Auto | deq | DEAD |  |
| 1732 | deq.b30DetLink | deq | DEAD |  |
| 1733 | deq.b30DetFreq | deq | DEAD |  |
| 1734 | deq.b30DetQ | deq | DEAD |  |
| 1735 | deq.b30BelowOn | deq | DEAD |  |
| 1736 | deq.b30BelowRange | deq | DEAD |  |
| 1737 | deq.b30BelowRatio | deq | DEAD |  |
| 1738 | deq.b30AboveOn | deq | DEAD |  |
| 1739 | deq.b30AboveRange | deq | DEAD |  |
| 1740 | deq.b30AboveRatio | deq | DEAD |  |
| 1741 | deq.b30RelFree | deq | DEAD |  |
| 1742 | deq.b30RelFreq | deq | DEAD |  |
| 1743 | deq.b30RelQ | deq | DEAD |  |
| 1744 | deq.b30RelSource | deq | DEAD |  |
| 1745 | deq.b31On | deq | CONDITIONAL | state 1, 3.0 dB |
| 1746 | deq.b31Type | deq | CONDITIONAL | state 1, 6.0 dB |
| 1747 | deq.b31Freq | deq | CONDITIONAL | state 1, 6.0 dB |
| 1748 | deq.b31Gain | deq | CONDITIONAL | state 1, 1.7 dB |
| 1749 | deq.b31Q | deq | DEAD |  |
| 1750 | deq.b31Slope | deq | DEAD |  |
| 1751 | deq.b31Placement | deq | CONDITIONAL | state 1, 6.0 dB |
| 1752 | deq.b31Pan | deq | DEAD |  |
| 1753 | deq.b31Solo | deq | CONDITIONAL | state 1, -8.5 dB |
| 1754 | deq.b31Dyn | deq | CONDITIONAL | state 1, 2.3 dB |
| 1755 | deq.b31Thresh | deq | DEAD |  |
| 1756 | deq.b31Range | deq | DEAD |  |
| 1757 | deq.b31Attack | deq | DEAD |  |
| 1758 | deq.b31Release | deq | DEAD |  |
| 1759 | deq.b31Action | deq | DEAD |  |
| 1760 | deq.b31Knee | deq | DEAD |  |
| 1761 | deq.b31Source | deq | DEAD |  |
| 1762 | deq.b31Relative | deq | DEAD |  |
| 1763 | deq.b31Lookahead | deq | CONDITIONAL | state 1, 2.3 dB |
| 1764 | deq.b31Transient | deq | DEAD |  |
| 1765 | deq.b31Auto | deq | DEAD |  |
| 1766 | deq.b31DetLink | deq | DEAD |  |
| 1767 | deq.b31DetFreq | deq | DEAD |  |
| 1768 | deq.b31DetQ | deq | DEAD |  |
| 1769 | deq.b31BelowOn | deq | DEAD |  |
| 1770 | deq.b31BelowRange | deq | DEAD |  |
| 1771 | deq.b31BelowRatio | deq | DEAD |  |
| 1772 | deq.b31AboveOn | deq | DEAD |  |
| 1773 | deq.b31AboveRange | deq | DEAD |  |
| 1774 | deq.b31AboveRatio | deq | DEAD |  |
| 1775 | deq.b31RelFree | deq | DEAD |  |
| 1776 | deq.b31RelFreq | deq | DEAD |  |
| 1777 | deq.b31RelQ | deq | DEAD |  |
| 1778 | deq.b31RelSource | deq | DEAD |  |
| 1779 | deq.b32On | deq | CONDITIONAL | state 2, -12.2 dB |
| 1780 | deq.b32Type | deq | CONDITIONAL | state 3, -5.2 dB |
| 1781 | deq.b32Freq | deq | CONDITIONAL | state 3, -2.2 dB |
| 1782 | deq.b32Gain | deq | CONDITIONAL | state 5, 6.0 dB |
| 1783 | deq.b32Q | deq | CONDITIONAL | state 3, 2.3 dB |
| 1784 | deq.b32Slope | deq | CONDITIONAL | state 3, -6.1 dB |
| 1785 | deq.b32Placement | deq | CONDITIONAL | state 3, -15.0 dB |
| 1786 | deq.b32Pan | deq | CONDITIONAL | state 3, -5.5 dB |
| 1787 | deq.b32Solo | deq | CONDITIONAL | state 1, 3.4 dB |
| 1788 | deq.b32Dyn | deq | CONDITIONAL | state 5, -12.7 dB |
| 1789 | deq.b32Thresh | deq | DEAD |  |
| 1790 | deq.b32Range | deq | DEAD |  |
| 1791 | deq.b32Attack | deq | DEAD |  |
| 1792 | deq.b32Release | deq | DEAD |  |
| 1793 | deq.b32Action | deq | DEAD |  |
| 1794 | deq.b32Knee | deq | DEAD |  |
| 1795 | deq.b32Source | deq | DEAD |  |
| 1796 | deq.b32Relative | deq | DEAD |  |
| 1797 | deq.b32Lookahead | deq | DEAD |  |
| 1798 | deq.b32Transient | deq | DEAD |  |
| 1799 | deq.b32Auto | deq | DEAD |  |
| 1800 | deq.b32DetLink | deq | DEAD |  |
| 1801 | deq.b32DetFreq | deq | DEAD |  |
| 1802 | deq.b32DetQ | deq | DEAD |  |
| 1803 | deq.b32BelowOn | deq | DEAD |  |
| 1804 | deq.b32BelowRange | deq | DEAD |  |
| 1805 | deq.b32BelowRatio | deq | DEAD |  |
| 1806 | deq.b32AboveOn | deq | DEAD |  |
| 1807 | deq.b32AboveRange | deq | DEAD |  |
| 1808 | deq.b32AboveRatio | deq | DEAD |  |
| 1809 | deq.b32RelFree | deq | DEAD |  |
| 1810 | deq.b32RelFreq | deq | DEAD |  |
| 1811 | deq.b32RelQ | deq | DEAD |  |
| 1812 | deq.b32RelSource | deq | DEAD |  |
| 1814 | deq.lookaheadMs | deq | DEAD |  |
| 1815 | deq.on | deq | CONDITIONAL | state 2, -5.0 dB |
| 1818 | sup.sharpness | sup | CONDITIONAL | state 1, -34.0 dB |
| 1819 | sup.selectivity | sup | CONDITIONAL | state 1, -36.4 dB |
| 1820 | sup.attack | sup | CONDITIONAL | state 1, -12.6 dB |
| 1821 | sup.release | sup | CONDITIONAL | state 1, -26.5 dB |
| 1822 | sup.floor | sup | CONDITIONAL | state 1, -19.0 dB |
| 1823 | sup.centre | sup | CONDITIONAL | state 1, -11.7 dB |
| 1824 | sup.width | sup | CONDITIONAL | state 1, -15.9 dB |
| 1825 | sup.mix | sup | DEAD |  |
| 1826 | sup.link | sup | CONDITIONAL | state 1, -32.8 dB |
| 1827 | sup.hard | sup | CONDITIONAL | state 1, -24.3 dB |
| 1829 | sup.midSide | sup | CONDITIONAL | state 1, -30.6 dB |
| 1830 | sup.sidechain | sup | CONDITIONAL | state 1, -19.0 dB |
| 1831 | sup.focus1Amount | sup | CONDITIONAL | state 1, -23.5 dB |
| 1832 | sup.focus1Freq | sup | CONDITIONAL | state 1, -36.0 dB |
| 1833 | sup.focus1Q | sup | CONDITIONAL | state 1, -36.7 dB |
| 1834 | sup.focus2Amount | sup | CONDITIONAL | state 1, -32.3 dB |
| 1835 | sup.focus2Freq | sup | CONDITIONAL | state 1, -45.0 dB |
| 1836 | sup.focus2Q | sup | CONDITIONAL | state 1, -30.8 dB |
| 1837 | sup.focus3Amount | sup | CONDITIONAL | state 1, -22.5 dB |
| 1838 | sup.focus3Freq | sup | CONDITIONAL | state 1, -26.8 dB |
| 1839 | sup.focus3Q | sup | CONDITIONAL | state 1, -30.0 dB |
| 1840 | sup.focus4Amount | sup | CONDITIONAL | state 1, -24.6 dB |
| 1841 | sup.focus4Freq | sup | CONDITIONAL | state 1, -35.6 dB |
| 1842 | sup.focus4Q | sup | CONDITIONAL | state 1, -35.6 dB |
| 1895 | decomp.matchTarget | decomp | DEAD |  |
| 1899 | deexp.ratio | deexp | CONDITIONAL | state 3, -4.4 dB |
| 1900 | deexp.range | deexp | CONDITIONAL | state 3, -4.4 dB |
| 1901 | deexp.knee | deexp | CONDITIONAL | state 3, -41.4 dB |
| 1902 | deexp.floor | deexp | CONDITIONAL | state 3, -4.4 dB |
| 1904 | deexp.hold | deexp | CONDITIONAL | state 3, -7.0 dB |
| 1905 | deexp.release | deexp | CONDITIONAL | state 3, -7.7 dB |
| 1906 | deexp.detector | deexp | CONDITIONAL | state 3, -7.8 dB |
| 1907 | deexp.detectorWindow | deexp | DEAD |  |
| 1909 | deexp.stereoLink | deexp | CONDITIONAL | state 3, -20.3 dB |
| 1910 | deexp.keySource | deexp | CONDITIONAL | state 1, -7.7 dB |
| 1913 | deexp.mix | deexp | CONDITIONAL | state 3, -4.4 dB |
| 1914 | deexp.on | deexp | CONDITIONAL | state 3, -4.4 dB |
| 1922 | harm.bias | harm | CONDITIONAL | state 1, -9.7 dB |
| 1938 | smart.release | smart | CONDITIONAL | state 1, -0.1 dB |
| 1944 | smart.truePeak | smart | CONDITIONAL | state 1, -0.1 dB |
| 1945 | smart.autoRelease | smart | CONDITIONAL | state 1, -0.1 dB |
| 1948 | smart.bodyShare | smart | CONDITIONAL | state 1, 3.1 dB |
| 1964 | smart.split3 | smart | CONDITIONAL | state 1, 3.4 dB |
| 1968 | dither.ditherAmount | requantise | CONDITIONAL | state 1, -6.0 dB |
| 1969 | dither.shaping | requantise | CONDITIONAL | state 1, -6.0 dB |
| 1970 | dither.shapingDepth | requantise | CONDITIONAL | state 1, -6.0 dB |
| 1971 | dither.autoBlank | requantise | CONDITIONAL | state 1, -6.0 dB |
| 1972 | dither.peakLimit | requantise | CONDITIONAL | state 1, -6.0 dB |
| 1974 | dither.keepExact | requantise | CONDITIONAL | state 1, -6.0 dB |
| 1977 | inputLink | input | non-audio | gangs the two input-gain controls in the editor; the gains themselves are tested |
| 1978 | rackShown | input | non-audio | which modules the rack shows; editor only, hidden from automation |
