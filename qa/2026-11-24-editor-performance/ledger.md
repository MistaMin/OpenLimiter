# Editor performance, readouts, Semibold text (2026-11-24)

Owner 2026-11-24: "The GPU usage is really crazy. On an M1 Max machine the FFT with the other meters make this
plugin unusable. Also, the LUFS numbers move to fast since they are not whole numbers, rms as well. Also, We
updated the standards on the Goodlookindri that fonts looked brittle and that semi bold and bold text should
be used."

## Cause

GoodLookin DRI rasterises a polygon or polyline by testing every segment at every pixel of its bounding box,
and the canvas replays the whole moving scene once per damage rectangle with no clip culling. The analyser
pages drew 160-point spectrum fills and 400-point band fills as single polygons across the plot, several
times per frame: at Retina 2x a SimplEQ frame took 1.2 s and a PrecisionEQ frame 2.0 s, 30 times a second.

## Fixes

- `kit::curve` draws long polylines in pieces of 8 segments; `kit::area` / `kit::between` fill one device-pixel
  column at a time on whole pixels (eight columns share one rectangle), so neighbours meet without seams.
- The pages that called `fill_poly` for curve areas (Floor Lifter, Harmonic Compressor, Dynamics Restore,
  Multiband, DualSone, Smart Limiter) use `kit::between`; `hatch` draws its diagonals in short pieces.
- `render` gives the canvas one rectangle around all damage (one replay); the host still gets the small ones.
- `header_taken` respects clips (a clipped plot fill no longer counts as header room taken).
- Loudness (LUFS, LU) and RMS readouts in whole numbers (`live::whole`, rounded -0 shows 0); true peak keeps
  a tenth for the -1 dBTP ceiling.
- GoodLookin DRI 3fe0426 (Source Sans Pro Semibold, lifted light-on-dark edges) through a `[patch]` in
  Cargo.toml; the editor's own text in `raster.rs` applies the same edge lift. CI checks out 3fe0426.
  Overtone Generator's description shortened to "2nd–5th, alias-free" to fit the wider face.

## Evidence

`cargo test --release --lib frame_cost -- --ignored --nocapture` (2x, 30 Hz ticks with audio, dark theme,
this M1 Max, load average about 24 from other sessions):

| Page | Before ms/frame | After ms/frame | Window dirty |
|---|---|---|---|
| limiter | 56.8 | 17.5 | 46% |
| eq (SimplEQ) | 1203 | 14.5 | 51% |
| peq (PrecisionEQ) | 2009 | 17.8 | 55% |
| deexp (Floor Lifter) | 580* | 7.0 | 43% |
| hcomp | 167* | 13.3 | 46% |
| decomp | 128* | 18.6 | 46% |
| bands | 97* | 22.3 | 51% |
| deq / sup | 70* | 17.4 / 16.7 | 60 / 53% |
| smart | 67* | 13.0 | 48% |

\* measured after the `kit` change, before the page change (no earlier all-page run).

- Tests: 74 passed (new `column_fill_matches_the_polygon`: mean difference < 0.5 against the polygon, no
  seam in a flat translucent band).
- Screenshots looked at: SimplEQ with live spectrum and three bands moved, Floor Lifter (hatch, wedge),
  readouts in whole numbers, Semibold text.

## Not verified

- GPU use in a host window on the owner's Retina display: not measured. The test REAPER is hidden and does
  not draw, so its GPU reading (1.7%) says nothing. Needs the owner's check with the new build.
- The SDK's Cocoa view builds a CGImage of the whole frame on every draw (DeviceRGB); whether that costs GPU
  time on a P3 display is not measured. A change there is an OpenPluginSDK change.
- Toolkit root cause (no clip culling, every segment per pixel) is in GoodLookin DRI: not changed here.

## Second pass (owner 2026-11-24: history dances, radar lines brittle, multiband numbers too fast, text)

- History (`live::paint_plot`): columns anchored to the count of entries ever pushed (`hist_count`), each the
  peak (min gain) of its stretch; 60 dB/s fall between peaks. Three consecutive frames looked at: every beat
  peak keeps its shape.
- Radar: half line width 0.6 k + 0.6 px, input line alpha 0.55 to 0.95, fade to 60 % (was 35 %), target
  dashes anti-aliased along the arc, level labels 9 pt on a plot-colour plate.
- Whole dB: Multiband IN / GR / OUT and per-band GR; Punch & Energy header. "-0.0" removed in Suppressor and
  Punch & Energy boxes.
- Text: 30 calls at 7 to 7.5 pt raised to 8 pt. New test `text_is_whole_apart_and_readable` (every page, both
  heights): no text cut by its clip, no overlapping labels, nothing under 8 pt. Every page looked at in 1x
  screenshots; Suppressor's "-6" lane mark ran into the spectrum's "+0" and was removed.
- Not covered by the test: text drawn by the live layer (meters, readouts) and text running past a box that
  has no clip; checked by eye only.

## On-screen measurement (2026-11-24, 05:13 to 05:28, unattended)

Own REAPER 7.80 instance (unhidden, frontmost), VST3, noise.wav looping, light theme, 30 s per state with
`sample.sh` (ioreg "Device Utilization %", top for REAPER and WindowServer). Displays: 1920x1080 and
1080x1920, both **1x** (no Retina display attached; backing scale 1). Load average 4.4 to 6.4.

**Found first:** the .pkg install put 1.0.0 in `/Library/Audio/Plug-Ins` (VST3, AU, CLAP, AAX, LV2). REAPER and
the AU registry load that copy, not the 1.0.1 build in `~/Library`. Every host on this Mac runs 1.0.0 until
the system copies are removed or replaced. To measure 1.0.1 the user bundle was renamed for the run
(`OpenLimiter101.vst3`, project file pointed at it, then renamed back); `lsof` confirmed which binary loaded.

| State | Build | REAPER CPU % | WindowServer % | GPU % |
|---|---|---|---|---|
| editor closed (REAPER behind other windows) | 1.0.0 | 13.0 | 17.7 | 0.7 |
| editor closed (REAPER frontmost) | 1.0.1 | 17.2 | 27.0 | 5.4 |
| Classic limiter page | 1.0.0 | 53.7 | 28.8 | 3.7 |
| Classic limiter page | 1.0.1 | 33.3 | 29.6 | 4.3 |
| SimplEQ, POST analyser | 1.0.0 | **101.8** (main thread pegged) | 12.6 | 0.7 |
| SimplEQ, POST analyser | 1.0.1 | 31.6 | 29.6 | 4.6 |
| PrecisionEQ, POST analyser | 1.0.0 | not measured: editor ignored the click (frozen) | | |
| PrecisionEQ, POST analyser | 1.0.1 | 31.8 | 29.7 | 6.8 |

- 1.0.0 reproduces the owner's report: on SimplEQ REAPER's main thread is saturated, the editor stops taking
  clicks and draws few frames (hence its low WindowServer and GPU numbers).
- 1.0.1: the editor adds about 15 % of one core to REAPER and about 2 to 3 % WindowServer / GPU over the
  closed state, on every page measured. GPU is not the bottleneck at any point.
- What is left (`sample` 5 s on PrecisionEQ, main thread 3554 samples): `cocoa::draw_rect` 553, of which
  DRI `Framebuffer::replay` (software raster) about 400 and the full-frame `CGContextDrawImage` 47 (1.3 %).
  The Cocoa CGImage path is not a cost worth an SDK change; the remaining load is the DRI raster
  (no clip culling), already listed above.
- Not measured: a Retina (2x) display, where raster work is 4x the pixels (the in-process 2x benchmark
  above covers it: 14 to 22 ms per frame); the dark theme; other pages on screen.
- Left behind: REAPER's plug-in cache has a stale `OpenLimiter101.vst3` line; REAPER drops it on its next scan.
