# OpenLimiter 1.0.2-Beta

The first public release of OpenLimiter, a modular mastering chain by OpenGrid:
seventeen modules you switch on, order and set in one plugin, ending in a
word-length stage. It is made to be the last plugin before the master is
printed. This is a beta.

## Formats and requirements

| Download | System | Formats |
|---|---|---|
| `OpenLimiter-1.0.2-Beta-macos.pkg` | macOS 11 or later, Apple silicon and Intel (universal) | VST3, Audio Unit, CLAP, LV2, AAX |
| `OpenLimiter-1.0.2-Beta-windows-x64-setup.exe` | Windows 10 or later, x64 | VST3, CLAP |
| `OpenLimiter-1.0.2-Beta-linux-x86_64.tar.gz` | Linux x86_64 | CLAP, LV2 (no editor) |

## Install

- **macOS:** open the `.pkg` and choose the formats. It is signed with
  Developer ID and notarized by Apple; the AAX is signed with PACE Eden.
- **Windows:** run the setup and choose VST3, CLAP or both. The installer is
  not code-signed, so Windows SmartScreen may ask before it runs.
- **Linux:** unpack the archive and copy `usr/lib/clap/OpenLimiter.clap` to
  `~/.clap` and `usr/lib/lv2/OpenLimiter.lv2` to `~/.lv2` (or the system
  folders under `/usr/lib`).

## What is in it

New in this build: the History and the loudness radar open in their own
resizable windows, the History has spectrogram and waterfall views, the gain
reduction displays hold each peak and fall smoothly, and in the rack Alt-click
removes a module and Cmd-click resets it. See `CHANGELOG.md`.

- Classic Limiter, Punch & Energy, Loudness MAX, SimplEQ, DualSone, Clean
  Peak, Compressor / De-esser, Final Wall, Multiband, PrecisionEQ, DynEQ,
  Suppressor, Harmonic Compressor, Dynamics Restore, Floor Lifter, Overtone
  Generator and Smart Limiter, in any order, then Requantise.
- Input and output meters, true peak, RMS, BS.1770 loudness, a history
  (levels, spectrogram or waterfall) and a loudness radar, both of which open in
  their own resizable windows.
- Autogain and Delta listening aids. Switch both off before printing.
- External sidechain for modules with a key, A/B, factory presets, four themes.

## Checks

- macOS: auval passes; pluginval at strictness 10 passes for VST3, with and without the editor
  tests; clap-validator 34 passed, 0 failed; Avid's AAX Validator 13 passed, 0 failed (the DSP cycle count needs HDX hardware and was skipped).
- REAPER (macOS, VST3): loads, renders finite and repeatable audio, every tested parameter
  changes the sound and the editor, state survives save and reopen, the editor survives five
  close and open cycles; a sweep of all 1985 parameters found none dead.
- OpenGrid9 (macOS): VST3 and Audio Unit keep all of their values through save and reopen;
  LV2 loads.
- Windows and Linux: the plugin's tests pass on each in GitHub Actions.
- Details: `qa/2026-11-24-qa-editor-sdk/ledger.md` in the repository.

## Known limits

- No listening review has been done yet.
- Linux has no editor; the host shows its own controls.
- Windows has no LV2 and no AAX.
- The Windows and Linux builds are made by GitHub Actions and have only been
  through the automated tests, not a host.
- The pop-out windows have been tried in a host on macOS only.

## Files

- `OpenLimiter-1.0.2-Beta-macos.pkg` and `.sha256`
- `OpenLimiter-1.0.2-Beta-windows-x64-setup.exe` and `.sha256`
- `OpenLimiter-1.0.2-Beta-linux-x86_64.tar.gz` and `.sha256`
- `OpenLimiter-1.0.2-Beta-User-Manual.pdf`
- `opengrid-manifest.json`
