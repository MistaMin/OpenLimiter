# OpenLimiter 1.0.0-Beta

The first public release of OpenLimiter, a modular mastering chain by OpenGrid:
seventeen modules you switch on, order and set in one plugin, ending in a
word-length stage. It is made to be the last plugin before the master is
printed. This is a beta.

## Formats and requirements

| Download | System | Formats |
|---|---|---|
| `OpenLimiter-1.0.0-Beta-macos.pkg` | macOS 11 or later, Apple silicon and Intel (universal) | VST3, Audio Unit, CLAP, LV2, AAX |
| `OpenLimiter-1.0.0-Beta-windows-x64-setup.exe` | Windows 10 or later, x64 | VST3, CLAP |
| `OpenLimiter-1.0.0-Beta-linux-x86_64.tar.gz` | Linux x86_64 | CLAP, LV2 (no editor) |

## Install

- **macOS:** open the `.pkg` and choose the formats. It is signed with
  Developer ID and notarized by Apple; the AAX is signed with PACE Eden.
- **Windows:** run the setup and choose VST3, CLAP or both. The installer is
  not code-signed, so Windows SmartScreen may ask before it runs.
- **Linux:** unpack the archive and copy `usr/lib/clap/OpenLimiter.clap` to
  `~/.clap` and `usr/lib/lv2/OpenLimiter.lv2` to `~/.lv2` (or the system
  folders under `/usr/lib`).

## What is in it

- Classic Limiter, Punch & Energy, Loudness MAX, SimplEQ, DualSone, Clean
  Peak, Compressor / De-esser, Final Wall, Multiband, PrecisionEQ, DynEQ,
  Suppressor, Harmonic Compressor, Dynamics Restore, Floor Lifter, Overtone
  Generator and Smart Limiter, in any order, then Requantise.
- Input and output meters, true peak, RMS, BS.1770 loudness, history and
  loudness radar.
- Autogain and Delta listening aids. Switch both off before printing.
- External sidechain for modules with a key, A/B, factory presets, four themes.

## Checks

See `qa/2026-11-23-release-1.0.0-beta/ledger.md` in the repository.

## Known limits

- No listening review has been done yet.
- Some readouts can show "-0.0".
- Linux has no editor; the host shows its own controls.
- Windows has no LV2 and no AAX.
- The Windows and Linux builds are made by GitHub Actions and have only been
  through the automated tests, not a host.

## Files

- `OpenLimiter-1.0.0-Beta-macos.pkg` and `.sha256`
- `OpenLimiter-1.0.0-Beta-windows-x64-setup.exe` and `.sha256`
- `OpenLimiter-1.0.0-Beta-linux-x86_64.tar.gz` and `.sha256`
- `OpenLimiter-1.0.0-Beta-User-Manual.pdf`
- `opengrid-manifest.json`
