# Changelog

## Unreleased

- Hosts list the module group "Compressor / De-esser" as "Compressor & De-esser": hosts now read "/" as a sub-group. Parameter names, ids and automation are unchanged.
- NaN, infinity and denormal numbers at the input or output are set to 0; normal audio passes unchanged.
- Less idle CPU: PrecisionEQ's linear-phase helper thread waits until it has work instead of checking 1000 times a second (50 idle instances: 378 ms to 7 ms of CPU per second).
- VST3: values set just before a host saves are kept again (OpenGrid9 lost up to 392 of them with the newer SDK; fixed in OpenPluginSDK).

## 1.0.2 Beta (2026-11-24)

- History and Loudness Radar open in their own resizable windows (button in each header, lit while open). They stay open when the plug-in window closes, reopen where they were, and close with the plug-in.
- History: LEVELS, 2D (spectrogram) and 3D (waterfall of 0.2 s, third-octave smoothed slices) views of the output.
- Loudness radar draws short-term loudness with even strokes (it zig-zagged on drums).
- Gain reduction trace and meter: each peak, then a 60 dB/s fall, without dropping to 0 between blocks.
- Multiband: reduction over the threshold in the accent colour; readouts rise in 50 ms and fall at 20 dB/s.
- Rack: Alt-click (Option-click) removes a module, Cmd-click (Ctrl-click on Windows and Linux) resets its controls.
- Knob labels no longer collide with the knobs drawn by some pages (DRIVE, the Modes gain knobs).
- VST3: every value survives save and reopen in hosts that edit parameters right after the plug-in starts (OpenGrid9 lost up to 148).
- Harmonic Compressor: less CPU (one contiguous delay line read; the dry alignment runs only while the dry signal is heard).
- Windows and Linux keep preferences in %APPDATA% and the XDG config folder.

## 1.0.1 Beta (2026-11-24, not published)

- Editor: the analyser pages draw 20 to 100 times faster (SimplEQ 1.2 s to 15 ms a frame at Retina size, PrecisionEQ 2 s to 18 ms); long curves, fills and hatching are drawn in short pieces, and a page's moving parts are repainted in one pass.
- Loudness (LUFS, LU) and RMS readouts show whole numbers, so they no longer flicker; true peak keeps its tenth.
- History: each column shows the peak of a fixed stretch of time and scrolls in whole columns, so peaks no longer change shape as they move; levels fall at 60 dB/s between peaks. Loudness radar: wider lines, anti-aliased target dashes, less fading, level labels on a plate.
- Multiband IN / GR / OUT and the per-band reduction, and Punch & Energy's LIFT / PUNCH / CLIP, read in whole dB. No readout shows "-0.0".
- No text below 8 pt; Suppressor's lane mark that ran into the spectrum scale is gone.
- Meter scales in text ink (the grey all but vanished on the light theme); the GR scale at full colour; SOFT CLIP in the history on a plate.
- Text is Source Sans Pro Semibold, the GoodLookin DRI standard, with light-on-dark edges lifted.

## 1.0.0 Beta (2026-11-23, not published)

First public release.

- Seventeen modules in one movable chain: Classic Limiter, Punch & Energy, Loudness MAX, SimplEQ, DualSone, Clean Peak, Compressor / De-esser, Final Wall, Multiband, PrecisionEQ, DynEQ, Suppressor, Harmonic Compressor, Dynamics Restore, Floor Lifter, Overtone Generator and Smart Limiter, followed by Requantise (word length, dither and noise shaping).
- Input and output meters, true peak, RMS, BS.1770 loudness with history and radar, A/B, factory presets, four themes.
- Autogain and Delta listening aids on the top bar.
- External sidechain for the modules with a key, delay-aligned through the chain.
- PrecisionEQ: a linear-phase filter that arrives later than its wait (possible when a host renders faster than real time) now blends in from the delayed signal instead of switching with a click.
- macOS: VST3, Audio Unit, CLAP, LV2 and AAX, universal. Windows: VST3 and CLAP. Linux: CLAP and LV2 without an editor.
