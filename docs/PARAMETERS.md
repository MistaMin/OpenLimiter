# OpenLimiter parameter reference

Generated from the plugin (`cargo test --release dump_manual_params -- --ignored`). Every control, page by page, with its range and default as the editor shows them. A control that repeats per band, set or channel is listed once, with its number written as `n`. The names are the ones a host lists; two modules have a short form there ("Comp De-ess" for Compressor / De-esser, "Harmonic Comp" for Harmonic Compressor).

## Top bar and input

| Control | Range or choices | Default |
|---|---|---|
| Input Gain L | -24.0 dB to 24.0 dB | 0.0 dB |
| Input Gain R | -24.0 dB to 24.0 dB | 0.0 dB |
| Input Link | Off, On | On |
| Autogain | Off, On | Off |
| Delta | Off, On | Off |

## Classic Limiter

| Control | Range or choices | Default |
|---|---|---|
| Classic Limiter On | Off, On | On |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Release A | Slow 3 to Fast +3 | Normal |
| Soft Clip | -12 dB to OFF | +0 dB |
| Upsample | Off, On | On |
| Link | Off, On | On |
| No Latency | Off, On | Off |
| Auto Release | Off, On | On |
| Gain L | -24.0 dB to +24.0 dB | +0.0 dB |
| Gain R | -24.0 dB to +24.0 dB | +0.0 dB |
| Output | -inf dB to 0.0 dB | 0.0 dB |
| Release M | 20.0 ms to 7.00 s | 300 ms |

## Punch & Energy

| Control | Range or choices | Default |
|---|---|---|
| Punch & Energy On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Punch & Energy Curve | Off, Dense light, Dense medium, Dense heavy, Quadratic 6 dB, Quadratic 9 dB, Continuous, Quadratic 2 dB | Off |
| Punch & Energy Oversample nx | Off, On | On |
| Punch & Energy Drive | -12.0 dB to 24.0 dB | 0.0 dB |
| Punch & Energy Threshold | -24.0 dB to 6.0 dB | 0.0 dB |
| Punch & Energy Mix | 0% to 100% | 100% |
| Punch & Energy Upward amount | 0.0 dB to 10.0 dB | 0.0 dB |
| Punch & Energy Upward threshold | -60.0 dB to 0.0 dB | -16.0 dB |
| Punch & Energy Transient amount | 0% to 200% | 0% |

## Loudness MAX

| Control | Range or choices | Default |
|---|---|---|
| Loudness MAX On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Loudness MAX Drive | +0.0 dB to +20.0 dB | +0.0 dB |
| Loudness MAX Ceiling | -20.0 dB to 0.0 dB | 0.0 dB |
| Loudness MAX Character | 0.0 to 10.0 | 2.0 |
| Loudness MAX Learn Target (LUFS) | -32 LUFS to -1 LUFS | -11 LUFS |
| Loudness MAX Style | Clear, Dense, Surge, Even, Edge, Slam, Legacy, Open, Snap, Quick, Iron, Glow, Glass | Clear |
| Loudness MAX True Peak Ceiling | Off, On | Off |
| Loudness MAX Unity Gain | Off, On | Off |
| Loudness MAX Stereo Link | 0% to 100% | 100% |
| Loudness MAX Input Gains Linked | Off, On | On |
| Loudness MAX Output Gains Linked | Off, On | On |
| Loudness MAX Learn: Measured Loudness (LUFS) | no learn yet to last 0.0 LUFS | no learn yet |
| Loudness MAX Input Gain | -144 dB to +10.0 dB | +0.0 dB |
| Loudness MAX Input Gain Left | -144 dB to +10.0 dB | +0.0 dB |
| Loudness MAX Input Gain Right | -144 dB to +10.0 dB | +0.0 dB |
| Loudness MAX Output Gain | -144 dB to +10.0 dB | +0.0 dB |
| Loudness MAX Output Gain Left | -144 dB to +10.0 dB | +0.0 dB |
| Loudness MAX Output Gain Right | -144 dB to +10.0 dB | +0.0 dB |

## SimplEQ

| Control | Range or choices | Default |
|---|---|---|
| SimplEQ On | Off, On | Off |
| SimplEQ Channel Matrix | Left/Right, Mid/Side | Left/Right |
| SimplEQ Link Channel Sets | Off, On | On |
| SimplEQ EQ On | Off, On | On |
| SimplEQ Low Cut On | Off, On | Off |
| SimplEQ Low Cut Frequency | 2 Hz to 200 Hz | 20 Hz |
| SimplEQ Band n On | Off, On | On |
| SimplEQ Band n Type | Bell, Band reject, Low shelf 3 dB/oct, Low shelf 6 dB/oct, Low shelf 12 dB/oct, High shelf 3 dB/oct, High shelf 6 dB/oct, High shelf 12 dB/oct, High-pass 12 dB/oct, Low-pass 12 dB/oct | Low shelf 3 dB/oct |
| SimplEQ Band n Gain | -12.0 dB to +12.0 dB | +0.0 dB |
| SimplEQ Band n Frequency | 20 Hz to 20.0 k | 50 Hz |
| SimplEQ Band n Width | 0.02 oct to 4.00 oct | 2.00 oct |
| SimplEQ Band n On (R/S) | Off, On | On |
| SimplEQ Band n Type (R/S) | Bell, Band reject, Low shelf 3 dB/oct, Low shelf 6 dB/oct, Low shelf 12 dB/oct, High shelf 3 dB/oct, High shelf 6 dB/oct, High shelf 12 dB/oct, High-pass 12 dB/oct, Low-pass 12 dB/oct | Low shelf 3 dB/oct |
| SimplEQ Band n Gain (R/S) | -12.0 dB to +12.0 dB | +0.0 dB |
| SimplEQ Band n Frequency (R/S) | 20 Hz to 20.0 k | 50 Hz |
| SimplEQ Band n Width (R/S) | 0.02 oct to 4.00 oct | 2.00 oct |
| SimplEQ Band n Reject Depth | -inf to 0 dB | 0 dB |
| SimplEQ Band n Reject Depth (R/S) | -inf to 0 dB | 0 dB |

## DualSone

| Control | Range or choices | Default |
|---|---|---|
| DualSone On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| DualSone Mode | Parallel, Series, Fast-led, Mix | Parallel |
| DualSone Detector | LUFS (K), RMS (flat) | LUFS (K) |
| DualSone Threshold | -40.0 dB to 0.0 dB | -18.0 dB |
| DualSone Input | -24.0 dB to 24.0 dB | 0.0 dB |
| DualSone Mix | 0% to 100% | 100% |
| DualSone Output | -24.0 dB to 24.0 dB | 0.0 dB |
| DualSone Autogain | Off, On | Off |
| DualSone Delta | Off, On | Off |
| DualSone Slow On | Off, On | On |
| DualSone Slow Ratio | 1:1 to 10:1 | 2:1 |
| DualSone Slow Attack | 50.0 ms to 5.00 s | 800 ms |
| DualSone Slow Release | 100 ms to 10.0 s | 2.50 s |
| DualSone Slow Window | 200 ms, 400 ms, 3 s | 400 ms |
| DualSone Fast On | Off, On | On |
| DualSone Fast Crest | 0.0 dB to 24.0 dB | 6.0 dB |
| DualSone Fast Ratio | 1:1 to 20:1 | 3:1 |
| DualSone Fast Attack | 500 µs to 30.0 ms | 10.0 ms |
| DualSone Fast Release | 5.00 ms to 1.00 s | 100 ms |
| DualSone Fast Lookahead | 0% to 100% | 100% |

## Clean Peak

| Control | Range or choices | Default |
|---|---|---|
| Clean Peak On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Clean Peak Input Gain | -18.0 dB to +18.0 dB | +0.0 dB |
| Clean Peak Threshold | -12.0 dB to 0.0 dB | 0.0 dB |
| Clean Peak Release | 0.1 ms to 1.04 s | 7.3 ms |
| Clean Peak Attack | 0.1 ms to 1.0 ms | 0.1 ms |
| Clean Peak Knee | 0.0 dB to 10.0 dB | 0.0 dB |
| Clean Peak Lift | 0% to 125% | 0% |
| Clean Peak True Peak Hold | Off, On | Off |
| Clean Peak Peak Guard | Off, On | On |
| Clean Peak Slow Hold | Off, On | On |
| Clean Peak Output Trim | -18.0 dB to 0.0 dB | 0.0 dB |

## Compressor / De-esser

| Control | Range or choices | Default |
|---|---|---|
| Comp De-ess On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Comp De-ess Key Source | Internal, External | Internal |
| Comp De-ess Channel Mode | Ganged, Per channel, Mid / Side | Ganged |
| Comp De-ess Threshold n | -60.0 dB to 0.0 dB | 0.0 dB |
| Comp De-ess Ratio n | 1:5.0 UP to ∞:1 | 2.0:1 |
| Comp De-ess Attack n | 20 µs to 800 ms | 31.5 ms |
| Comp De-ess Release Fast n | 20 µs to 8.00 s | 100 ms |
| Comp De-ess Make-Up n | -60.0 dB to 60.0 dB | 0.0 dB |
| Comp De-ess Band n | Low band, Mid band, High band, Full band | Full band |
| Comp De-ess Band Frequency n | 41.0 Hz to 17.7 kHz | 415 Hz |
| Comp De-ess Band Width n (oct) | 0.17 oct to 8.00 oct | 2.00 oct |
| Comp De-ess Soft Knee n | 0% to 100% | 100% |
| Comp De-ess Auto Release n | Off, On | Off |
| Comp De-ess Band Solo | Off, On | Off |
| Comp De-ess Look-Ahead | OFF to 63.0 ms | OFF |
| Comp De-ess Input Gain | -60.0 dB to 12.0 dB | 0.0 dB |
| Comp De-ess Parallel Mix | 0% to 100% | 100% |
| Comp De-ess Auto Make-Up | Off, On | Off |
| Comp De-ess Detector | Peak, RMS | Peak |
| Comp De-ess Parallel | Off, Sum, Blend | Off |
| Comp De-ess Release Hold n | 20 µs to 8.00 s | 20 µs |
| Comp De-ess Release Average n | 20 µs to 8.00 s | 50.0 ms |
| Comp De-ess Release Slow n | 20 µs to 8.00 s | 200 ms |
| Comp De-ess Auto Release Time n | 630 µs to 8.00 s | 100 ms |
| Comp De-ess RMS Time | 120 µs to 8.00 s | 20.0 ms |
| Comp De-ess Limiter Drive | 0.0 dB to 30.0 dB | 0.0 dB |
| Comp De-ess Output Gain |  to  |  |
| Comp De-ess Safety Limiter | Off, On | On |
| Comp De-ess Ceiling Type | Sample -0.05 dB, Sample -0.01 dB, True peak | Sample -0.01 dB |
| Comp De-ess Key Link | Off, On | Off |
| Comp De-ess Key Filter | Off, On | Off |
| Comp De-ess Ganged Set | Set 1, Set 2 | Set 1 |

## Final Wall

| Control | Range or choices | Default |
|---|---|---|
| Final Wall On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Final Wall Style | Contour, Leveller | Contour |
| Final Wall Drive | +0.0 dB to +18.0 dB | +0.0 dB |
| Final Wall Ceiling | -30.0 dB to 0.0 dB | -0.2 dB |
| Final Wall Release | 0.0 ms to 3.00 s | 200 ms |
| Final Wall Threshold | -30.0 dB to 0.0 dB | 0.0 dB |
| Final Wall Soft Clip Threshold | -20.0 dB to 0.0 dB | 0.0 dB |
| Final Wall Soft Clip Curve | Wide knee, Continuous, Narrow knee | Wide knee |
| Final Wall Auto Profile | Universal, Dense, Natural, Voice | Universal |
| Final Wall Strict True Peak | Off, On | Off |
| Final Wall Soft Clip | Off, On | Off |
| Final Wall Auto Release | Off, On | On |

## Multiband

| Control | Range or choices | Default |
|---|---|---|
| Multiband On | Off, On | Off |
| Multiband Key Source | Internal, External | Internal |
| Multiband Mode | Downward, Parallel, Parallel Unlinked, Low-Level Lift | Downward |
| Multiband Band Limiter | Off, On | Off |
| Multiband Low Threshold | -25.0 dB to 20.0 dB | -7.0 dB |
| Multiband Low Ratio | 1.0:1 to ∞:1 | 2.0:1 |
| Multiband Low Attack | 0.3 ms to 350 ms | 50.0 ms |
| Multiband Low Release | 20.0 ms to 7.00 s | 300 ms |
| Multiband Low Make-Up | -24.0 dB to 12.0 dB | 0.0 dB |
| Multiband Low Lift Amount | 0% to 50% | 35% |
| Multiband Low Limiter Ceiling | -24.0 dB to 0.0 dB | -3.0 dB |
| Multiband Low Solo | Off, On | Off |
| Multiband Low Bypass | Off, On | Off |
| Multiband Parallel Wet Gain | OFF to +6.0 dB | +0.0 dB |
| Multiband Reference Level | -24.0 dB to 0.0 dB | -8.0 dB |
| Multiband Look-Ahead | 0.0 ms to 15.0 ms | 10.0 ms |
| Multiband Detector | Peak, Crest 6 dB, Crest 10 dB, Crest 12 dB, Crest 14 dB, Crest 16 dB, Crest 20 dB, Crest 24 dB, RMS | RMS |
| Multiband Low-Mid Threshold | -25.0 dB to 20.0 dB | -7.0 dB |
| Multiband Low-Mid Ratio | 1.0:1 to ∞:1 | 2.0:1 |
| Multiband Low-Mid Attack | 0.3 ms to 350 ms | 40.0 ms |
| Multiband Low-Mid Release | 20.0 ms to 7.00 s | 250 ms |
| Multiband Low-Mid Make-Up | -24.0 dB to 12.0 dB | 0.0 dB |
| Multiband Low-Mid Lift Amount | 0% to 50% | 35% |
| Multiband Low-Mid Limiter Ceiling | -24.0 dB to 0.0 dB | -3.0 dB |
| Multiband Low-Mid Solo | Off, On | Off |
| Multiband Low-Mid Bypass | Off, On | Off |
| Multiband Mid Threshold | -25.0 dB to 20.0 dB | -8.0 dB |
| Multiband Mid Ratio | 1.0:1 to ∞:1 | 2.0:1 |
| Multiband Mid Attack | 0.3 ms to 350 ms | 35.0 ms |
| Multiband Mid Release | 20.0 ms to 7.00 s | 200 ms |
| Multiband Mid Make-Up | -24.0 dB to 12.0 dB | 0.0 dB |
| Multiband Mid Lift Amount | 0% to 50% | 35% |
| Multiband Mid Limiter Ceiling | -24.0 dB to 0.0 dB | -3.0 dB |
| Multiband Mid Solo | Off, On | Off |
| Multiband Mid Bypass | Off, On | Off |
| Multiband High-Mid Threshold | -25.0 dB to 20.0 dB | -9.0 dB |
| Multiband High-Mid Ratio | 1.0:1 to ∞:1 | 2.0:1 |
| Multiband High-Mid Attack | 0.3 ms to 350 ms | 30.0 ms |
| Multiband High-Mid Release | 20.0 ms to 7.00 s | 175 ms |
| Multiband High-Mid Make-Up | -24.0 dB to 12.0 dB | 0.0 dB |
| Multiband High-Mid Lift Amount | 0% to 50% | 35% |
| Multiband High-Mid Limiter Ceiling | -24.0 dB to 0.0 dB | -3.0 dB |
| Multiband High-Mid Solo | Off, On | Off |
| Multiband High-Mid Bypass | Off, On | Off |
| Multiband High Threshold | -25.0 dB to 20.0 dB | -10.0 dB |
| Multiband High Ratio | 1.0:1 to ∞:1 | 2.0:1 |
| Multiband High Attack | 0.3 ms to 350 ms | 25.0 ms |
| Multiband High Release | 20.0 ms to 7.00 s | 150 ms |
| Multiband High Make-Up | -24.0 dB to 12.0 dB | 0.0 dB |
| Multiband High Lift Amount | 0% to 50% | 35% |
| Multiband High Limiter Ceiling | -24.0 dB to 0.0 dB | -3.0 dB |
| Multiband High Solo | Off, On | Off |
| Multiband High Bypass | Off, On | Off |
| Multiband Split n (Low / Low-Mid) | OFF to 4.0 kHz | 125 Hz |
| Multiband Split n (Low-Mid / Mid) | OFF to 4.0 kHz | 500 Hz |
| Multiband Split n (Mid / High-Mid) | 100 Hz to OFF | 2.0 kHz |
| Multiband Split n (High-Mid / High) | 100 Hz to OFF | 8.0 kHz |
| Multiband Floor Guard Threshold | -50.0 dB to -3.0 dB | -30.0 dB |
| Multiband Floor Guard Ratio | OFF to ∞ | OFF |
| Multiband Band Limiter Release | 10.0 ms to 1.00 s | 100 ms |

## PrecisionEQ

| Control | Range or choices | Default |
|---|---|---|
| PrecisionEQ On | Off, On | Off |
| PrecisionEQ Processing Mode | Zero Latency, Linear Phase, Analog, Mix, Analog ZL | Zero Latency |
| PrecisionEQ Linear Phase Quality | Low, Medium, High, Maximum | Medium |
| PrecisionEQ Mix Crossover | 500 Hz, 1 kHz, 2 kHz, 4 kHz, 8 kHz | 1 kHz |
| PrecisionEQ Band n On | Off, On | On |
| PrecisionEQ Band n Solo | Off, On | Off |
| PrecisionEQ Band n Type | Bell, Low Shelf, High Shelf, High Pass, Low Pass, Notch, Band Pass, Tilt Shelf, Allpass L, Soft Bell, Flat Tilt, Flat Top, Narrow Resonance, Steep HP, Steep LP, Sword, Allpass H, E HP, E LP, E LS, E HS, E LMF, E HMF, E Low Bell, E High Bell, G LMF, BritN HP, BritN LS, BritN Bell, BritN HS, 250 Bell, 250 LS, 250 HS, VT HP, VT LP, VT LS, VT Boost, VT Atten, VT LMF, VT HMF, Tone Low, Tone High, Blue Low, G LS, G HS, Blue High, G HP, G LP | Bell |
| PrecisionEQ Band n Placement | Stereo, Mid, Side, Left, Right, Mid Side | Stereo |
| PrecisionEQ Band n Frequency | 20 to 20.0k | 60 |
| PrecisionEQ Band n Gain | -30.0 dB to +30.0 dB | +0.0 dB |
| PrecisionEQ Band n Q | 0.03 to 40.0 | 1.00 |
| PrecisionEQ Band n Slope | 0 % to 100 % | 0 % |
| PrecisionEQ Band n Pan | -100 % to 100 % | 0 % |
| PrecisionEQ Band n Phase | -100 % to 100 % | 0 % |
| PrecisionEQ Band n Split | Off, On | Off |
| PrecisionEQ Band n Split Transient Gain | -30.0 dB to +30.0 dB | +0.0 dB |
| PrecisionEQ Band n Split Tail Gain | -30.0 dB to +30.0 dB | +0.0 dB |
| PrecisionEQ Input Trim | -24.0 dB to 24.0 dB | 0.0 dB |
| PrecisionEQ Output Gain | -24.0 dB to 24.0 dB | 0.0 dB |
| PrecisionEQ Gain Scale | -200% to 200% | 100% |
| PrecisionEQ Drive | 0% to 100% | 0% |
| PrecisionEQ Stereo Width | 0% to 200% | 100% |
| PrecisionEQ Balance | -100% to 100% | 0% |
| PrecisionEQ Side Level | -inf dB to 12.0 dB | 0.0 dB |
| PrecisionEQ Auto Gain | Off, On | Off |
| PrecisionEQ Polarity Invert | Off, On | Off |
| PrecisionEQ Bypass | Off, On | Off |
| PrecisionEQ Split Source | Full Mix, Drums, Percussive, Instrument, Vocal | Full Mix |
| PrecisionEQ Split Solo | Off, Transient, Tonal | Off |
| PrecisionEQ Transient Separation | 0 % to 100 % | 50 % |
| PrecisionEQ Transient Decay | 0 % to 100 % | 50 % |
| PrecisionEQ Split Smoothing | 0.0 ms to 50.0 ms | 5.0 ms |
| PrecisionEQ FIR Window | Blackman-Harris, Kaiser, Hann, Hamming, Blackman, Rectangular | Blackman-Harris |
| PrecisionEQ FIR Window Tightening | 0% to 100% | 0% |
| PrecisionEQ Digital+ Steps | Off, 1, 2, 3, 4 | Off |
| PrecisionEQ n-bit Precision | Off, On | Off |
| PrecisionEQ nx Oversampling | Off, On | Off |

## DynEQ

| Control | Range or choices | Default |
|---|---|---|
| DynEQ On | Off, On | Off |
| DynEQ Global Lookahead | Off, On | Off |
| DynEQ Global Lookahead Time | 0.0 ms to 20.0 ms | 0.0 ms |
| DynEQ Band n On | Off, On | On |
| DynEQ Band n Dynamic | Off, On | Off |
| DynEQ Band n Type | Bell, Low Shelf, High Shelf, High Pass, Low Pass, Notch, Band Pass, Tilt Shelf, Allpass L, Soft Bell, Flat Tilt, Flat Top, Narrow Resonance, Steep HP, Steep LP, Sword, Allpass H, E HP, E LP, E LS, E HS, E LMF, E HMF, E Low Bell, E High Bell, G LMF, BritN HP, BritN LS, BritN Bell, BritN HS, 250 Bell, 250 LS, 250 HS, VT HP, VT LP, VT LS, VT Boost, VT Atten, VT LMF, VT HMF, Tone Low, Tone High, Blue Low, G LS, G HS, Blue High, G HP, G LP | Bell |
| DynEQ Band n Placement | Stereo, Mid, Side, Left, Right, Mid Side | Stereo |
| DynEQ Band n Frequency | 20 to 20.0k | 60 |
| DynEQ Band n Gain | -30.0 dB to +30.0 dB | +0.0 dB |
| DynEQ Band n Q | 0.03 to 40.0 | 1.00 |
| DynEQ Band n Slope | 0 % to 100 % | 0 % |
| DynEQ Band n Threshold | -60.0 dB to 0.0 dB | -24.0 dB |
| DynEQ Band n Attack | 1.0 ms to 500 ms | 15.0 ms |
| DynEQ Band n Release | 1.0 ms to 2.00 s | 150 ms |
| DynEQ Band n Knee | 0.0 dB to 24.0 dB | 0.0 dB |
| DynEQ Band n Transient | 0 % to 100 % | 0 % |
| DynEQ Band n Auto | Off, On | Off |
| DynEQ Band n Above On | Off, On | Off |
| DynEQ Band n Above Range | -30.0 dB to +30.0 dB | -6.0 dB |
| DynEQ Band n Above Ratio | 0 % to 200 % | 100 % |
| DynEQ Band n Below On | Off, On | Off |
| DynEQ Band n Below Range | -30.0 dB to +30.0 dB | +6.0 dB |
| DynEQ Band n Below Ratio | 0 % to 200 % | 100 % |
| DynEQ Band n Detector Source | Follow, Left, Right, Mid, Side, External | Follow |
| DynEQ Band n Pan | -100% to 100% | 0% |
| DynEQ Band n Solo | Off, On | Off |
| DynEQ Band n Detector Link | Off, On | On |
| DynEQ Band n Detector Frequency | 20 Hz to 20.0 kHz | 60 Hz |
| DynEQ Band n Detector Q | 0.03 to 40.0 | 1.00 |
| DynEQ Band n Lookahead | 0.0 ms to 20.0 ms | 0.0 ms |
| DynEQ Band n Relative | 0 % to 100 % | 0 % |
| DynEQ Band n Relative Free | Off, On | Off |
| DynEQ Band n Relative Frequency | 20 Hz to 20.0 kHz | 60 Hz |
| DynEQ Band n Relative Q | 0.03 to 40.0 | 1.00 |
| DynEQ Band n Relative Source | Energy, Left, Right, Mid, Side, External | Energy |
| DynEQ Band n Action | Above, Below | Above |
| DynEQ Band n Range | -30.0 dB to +30.0 dB | +6.0 dB |

## Suppressor

| Control | Range or choices | Default |
|---|---|---|
| Suppressor On | Off, On | Off |
| Suppressor Sidechain | Off to On | Off |
| Suppressor Depth | 0 % to 100 % | 0 % |
| Suppressor Sharpness | 0 % to 100 % | 50 % |
| Suppressor Selectivity | 0 % to 100 % | 35 % |
| Suppressor Floor | -100 dB to 0.0 dB | -80.0 dB |
| Suppressor Attack | 1.0 ms to 500 ms | 3.5 ms |
| Suppressor Release | 20.0 ms to 2.00 s | 79.6 ms |
| Suppressor Stereo Link | 0 % to 100 % | 100 % |
| Suppressor Mix | 0 % to 100 % | 100 % |
| Suppressor Region Centre | 20 to 20.0k | 3.00k |
| Suppressor Region Width (octaves) | 0.1 oct to ALL | ALL |
| Suppressor Mid/Side | Off, On | Off |
| Suppressor Hard Mode | Off, On | Off |
| Suppressor Delta Listen | Off, On | Off |
| Suppressor Focus n Amount | -100 % to 100 % | 0 % |
| Suppressor Focus n Frequency | 20 Hz to 20.0 kHz | 250 Hz |
| Suppressor Focus n Q | 0.03 to 40.0 | 1.00 |

## Harmonic Compressor

| Control | Range or choices | Default |
|---|---|---|
| Harmonic Comp On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Harmonic Comp Key Source | Internal, External | Internal |
| Harmonic Comp Mode | Class-A Peak, Hybrid FF/FB, Diode Bridge, Bus VCA, Fast FET, Optical, RMS VCA | Class-A Peak |
| Harmonic Comp nnd Harmonic | 0% to 100% | 0% |
| Harmonic Comp nrd Harmonic | 0% to 100% | 0% |
| Harmonic Comp nth Harmonic | 0% to 100% | 0% |
| Harmonic Comp Threshold | -60.0 dB to 0.0 dB | -18.0 dB |
| Harmonic Comp Ratio | -2.0:1 to 100:1 | 4.0:1 |
| Harmonic Comp Attack | 100 µs to 300 ms | 10.0 ms |
| Harmonic Comp Release | 5.00 ms to 3.00 s | 100 ms |
| Harmonic Comp Makeup Gain | -12.0 dB to +24.0 dB | +0.0 dB |
| Harmonic Comp Auto Attack | Off, On | Off |
| Harmonic Comp Auto Release | Off, On | Off |
| Harmonic Comp Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| Harmonic Comp Input Gain | -24.0 dB to +24.0 dB | +0.0 dB |
| Harmonic Comp Mix | 0% to 100% | 100% |
| Harmonic Comp Sidechain Enable | Off, On | Off |
| Harmonic Comp Sidechain HP Freq | 20 Hz to 500 Hz | 150 Hz |
| Harmonic Comp Sidechain Listen | Off, On | Off |
| Harmonic Comp Transient Enable | Off, On | Off |
| Harmonic Comp Transient Attack | -100% to 100% | 0% |
| Harmonic Comp Transient Sustain | -100% to 100% | 0% |
| Harmonic Comp Transient Speed | 500 µs to 50.0 ms | 10.0 ms |
| Harmonic Comp Auto Gain | Off, On | Off |
| Harmonic Comp Delta Monitor | Off, On | Off |
| Harmonic Comp Output Gain | -24.0 dB to +24.0 dB | +0.0 dB |
| Harmonic Comp Oversample | 1x, 2x, 4x, 8x | 2x |
| Harmonic Comp Oversample Filter | Min Phase, Lin Phase | Min Phase |

## Dynamics Restore

| Control | Range or choices | Default |
|---|---|---|
| Dynamics Restore On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Dynamics Restore Key Source | Internal, External | Internal |
| Dynamics Restore Detector | Peak, RMS, Transient | Peak |
| Dynamics Restore Match | Off, On | Off |
| Dynamics Restore Match Target Crest | to 6 dB to to 24 dB | to 12 dB |
| Dynamics Restore Threshold | -60.0 dB to 0.0 dB | -18.0 dB |
| Dynamics Restore Ratio | 0.2:1 to 1.0:1 | 0.7:1 |
| Dynamics Restore Range | 0.0 dB to 24.0 dB | 6.0 dB |
| Dynamics Restore Attack | 10 µs to 100 ms | 1.00 ms |
| Dynamics Restore Release | 1.00 ms to 2.00 s | 80.0 ms |
| Dynamics Restore Ceiling | Off, On | On |
| Dynamics Restore Ceiling Level | -24.0 dB to 0.0 dB | -1.0 dB |
| Dynamics Restore Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| Dynamics Restore Look-Ahead | 0.0 ms to 10.0 ms | 0.0 ms |
| Dynamics Restore Mix | 0% to 100% | 100% |
| Dynamics Restore RMS Window | 1.00 ms to 500 ms | 30.0 ms |
| Dynamics Restore Transient Crest | 0.0 dB to 24.0 dB | 6.0 dB |
| Dynamics Restore Ceiling Type | Sample peak, True peak | True peak |
| Dynamics Restore Stereo Link | 0% to 100% | 100% |
| Dynamics Restore Key High-Pass | OFF to 2.0 kHz | OFF |
| Dynamics Restore Key Low-Pass | 500 Hz to OFF | OFF |
| Dynamics Restore Output Trim | -24.0 dB to 0.0 dB | 0.0 dB |

## Floor Lifter

| Control | Range or choices | Default |
|---|---|---|
| Floor Lifter On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Floor Lifter Key Source | Internal, External | Internal |
| Floor Lifter Detector | Peak, RMS | RMS |
| Floor Lifter Threshold | -80.0 dB to 0.0 dB | -40.0 dB |
| Floor Lifter Ratio | OFF to 8.0:1 | 2.0:1 |
| Floor Lifter Range | 0.0 dB to 36.0 dB | 12.0 dB |
| Floor Lifter Floor | -inf dB to -20.0 dB | -72.0 dB |
| Floor Lifter Release | 1.00 ms to 5.00 s | 150 ms |
| Floor Lifter Attack | 10 µs to 100 ms | 500 µs |
| Floor Lifter Hold | 0 µs to 500 ms | 20.0 ms |
| Floor Lifter Look-Ahead | 0.0 ms to 10.0 ms | 2.0 ms |
| Floor Lifter Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| Floor Lifter RMS Window | 1.00 ms to 500 ms | 10.0 ms |
| Floor Lifter Mix | 0% to 100% | 100% |
| Floor Lifter Stereo Link | 0% to 100% | 100% |
| Floor Lifter Key High-Pass | OFF to 2.0 kHz | OFF |
| Floor Lifter Key Low-Pass | 500 Hz to OFF | OFF |

## Overtone Generator

| Control | Range or choices | Default |
|---|---|---|
| Overtone Generator On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Overtone Generator Curve | Polynomial, Soft saturator, Tape knee | Polynomial |
| Overtone Generator Channels | Stereo, Mid-Side, Mid only, Side only | Stereo |
| Overtone Generator nnd harmonic | -80.0 dB to -12.0 dB | -30.0 dB |
| Overtone Generator nrd harmonic | -80.0 dB to -12.0 dB | -40.0 dB |
| Overtone Generator nth harmonic | -80.0 dB to -12.0 dB | -80.0 dB |
| Overtone Generator Drive | -24.0 dB to +24.0 dB | +0.0 dB |
| Overtone Generator Bias | -100% to 100% | 0% |
| Overtone Generator Mix | 0% to 100% | 100% |
| Overtone Generator Harmonics only | Off, On | Off |
| Overtone Generator Auto gain | Off, On | On |
| Overtone Generator Source low cut | OFF to 5.0 kHz | OFF |
| Overtone Generator Source high cut | 500 Hz to OFF | OFF |
| Overtone Generator Harmonics low cut | OFF to 5.0 kHz | OFF |
| Overtone Generator Harmonics high cut | 500 Hz to OFF | OFF |
| Overtone Generator Output trim | -24.0 dB to +0.0 dB | +0.0 dB |
| Overtone Generator Anti-alias | ADAA, ADAA2, Oversample 2x, Oversample 4x, None | ADAA |

## Smart Limiter

| Control | Range or choices | Default |
|---|---|---|
| Smart Limiter On | Off, On | Off |
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Smart Limiter Engine | Pure, Smooth, Precise, Punch, Multiband, Glide | Pure |
| Smart Limiter Drive | +0.0 dB to +24.0 dB | +0.0 dB |
| Smart Limiter Ceiling | -20.0 dB to 0.0 dB | -0.2 dB |
| Smart Limiter Release | 5.0 ms to 2.00 s | 100 ms |
| Smart Limiter Lookahead | 0.5 ms to 15.0 ms | 1.5 ms |
| Smart Limiter Knee | 0.0 dB to 10.0 dB | 0.0 dB |
| Smart Limiter Channel Mode | Stereo, Mid-Side | Stereo |
| Smart Limiter Transient Link | 0% to 100% | 100% |
| Smart Limiter Release Link | 0% to 100% | 100% |
| Smart Limiter True Peak | Off, On | On |
| Smart Limiter Gain Match | Off, On | Off |
| Smart Limiter Delta | Off to On | Off |
| Smart Limiter Auto Release | Off, On | On |
| Smart Limiter Body Share | 0% to 100% | 50% |
| Smart Limiter Join Width | 0.0 dB to 12.0 dB | 6.0 dB |
| Smart Limiter Pause Hold | Off, On | On |
| Smart Limiter Clip Headroom | 0.0 dB to 3.0 dB | 1.0 dB |
| Smart Limiter Band Focus | 0% to 100% | 100% |
| Smart Limiter Crossover Mode | Off, On | Off |
| Smart Limiter Oversampling | 4x, 8x, 16x | 4x |
| Smart Limiter Oversampling Offline | Same, 4x, 8x, 16x | 16x |
| Smart Limiter Slow Mode | Peak Body, Loudness Crest | Peak Body |
| Smart Limiter Slow Window | 200 ms, 400 ms, 3 s | 400 ms |
| Smart Limiter Crest Headroom | 4.0 dB to 18.0 dB | 10.0 dB |
| Smart Limiter Bass Hold Max | 0.0 ms to 20.0 ms | 14.0 ms |
| Smart Limiter K-Weighting | Off, On | On |
| Smart Limiter Clip Knee | 0.10 to 1.00 | 0.40 |
| Smart Limiter Split n | 40.0 Hz to 400 Hz | 120 Hz |

## Requantise

| Control | Range or choices | Default |
|---|---|---|
| Threshold | -25.0 dB to 0.1 dB | 0.0 dB |
| Requantise Word Length | Off, 24 bit, 22 bit, 20 bit, 18 bit, 16 bit, 12 bit, 8 bit | Off |
| Requantise Noise Shaping | Off, Tilt 1, Tilt 2, Top Octave, Tilt Soft, Rise 1, Rise 2, Rise 3, Rise 4, Rise 5, Rise 6, Valley 1, Valley 2, Valley 3 | Off |
| Requantise Dither Amount | 0% to 125% | 100% |
| Requantise Shaping Depth | 0% to 100% | 100% |
| Requantise Auto-Blank | Off, On | Off |
| Requantise Silence Stays Silent | Off, On | Off |
| Requantise DC Filter | Off, On | Off |
| Requantise Noise Peak Limit (steps, 0 = off) | Off to ±32 steps | Off |

## Setup

| Control | Range or choices | Default |
|---|---|---|
| Delay L | 0.0 ms to 300 ms | 0.0 ms |
| Delay R | 0.0 ms to 300 ms | 0.0 ms |
| Delay Fine L | 0.00 to 100 | 0.00 |
| Delay Fine R | 0.00 to 100 | 0.00 |
| Delay Unit | MS, 24, 25, 30 | MS |
| Phase Inv L | Off, On | Off |
| Phase Inv R | Off, On | Off |
| Headroom | 0.0 dB to 24.0 dB | 0.0 dB |
