# OpenLimiter 1.0.2-Beta release

Owner's decisions (2026-11-24): version 1.0.2-Beta; no standalone; Windows and Linux built by GitHub
Actions ("use github runs for Windows exes and Linux"). 1.0.1-Beta was never published.

Source: `1cf0a90` (main, pushed), OpenPluginSDK `34420a4`, clean tree for every build.

| Platform | Where | Output |
|---|---|---|
| macOS | this Mac (arm64, macOS 15.6), `scripts/build.sh`, release, universal arm64 + x86_64, minos 11.0 | VST3, AU, CLAP, LV2, AAX |
| Windows x64 | GitHub Actions run 45465010988, windows-latest | VST3, CLAP; Inno Setup installer, not code-signed |
| Linux x86_64 | GitHub Actions run 45465010988, ubuntu-22.04 | CLAP, LV2 (no editor), tar.gz |

| Check | Result | Evidence |
|---|---|---|
| QA before the release | green: see `../2026-11-24-qa-editor-sdk/ledger.md` (OpenGrid9 VST3/AU state round trip fixed by the SDK pin) | |
| Tests on Windows and Linux (77 each) | PASS | run 45465010988 |
| auval, release bundle | PASS | auval.log |
| clap-validator, release bundle | PASS (34 passed, 0 failed, 10 skipped) | clap-validator.log |
| pluginval strictness 10, VST3 and AU, release bundles | PASS | pluginval-*.log |
| AAX PACE Eden signature (wrap GUID F0BC4C20-E72E-11F1-AC32-00505692AD3E), signed and verified | PASS | aax-sign.log |
| AAX Validator 2024.6 on the signed AAX | PASS: 13 passed, 0 failed, 1 skipped (cycle_counts needs HDX). Validator copied to a path without spaces and `SysInfo.rb` patched in both copies (volumes without a container size) | aax-validator-summary.txt |
| Installer: Developer ID Installer, notarized (Accepted), stapled, Gatekeeper "Notarized Developer ID" | PASS | pkg-build.log |
| Installer payload: no loose `._` files after a full expand; `pkgbuild` stores the protected `com.apple.provenance` attribute as AppleDouble entries in the payload, restored as an attribute on install (cannot be removed with `xattr -c`) | recorded | |
| Universal slices and minos 11.0 in every macOS binary | PASS | |
| Manual PDF rebuilt and looked at (pages 2 and 3) | PASS | |
| Windows and Linux in a host; Windows code signing | not done (no machines; signing method not chosen) | |
| Listening, Pro Tools, Studio One | owner | |

| File | Bytes | SHA-256 |
|---|---|---|
| OpenLimiter-1.0.2-Beta-macos.pkg | 28632836 | ac34c514d5ac1010813735d705d404d8eb6e6e68a4cacf1378f4bacf96295b5e |
| OpenLimiter-1.0.2-Beta-windows-x64-setup.exe | 3688351 | 802b062b75a25569ae606f27e1e2f159ceb5d837c5550c70038585724bd875d8 |
| OpenLimiter-1.0.2-Beta-linux-x86_64.tar.gz | 3567413 | 53169f9936a1e861baf83a0242f9cff717263a33695cba5321f065193efeb681 |

Owner's approval (2026-11-24): "Yes, but fix the plugin manager version and updated the owners manual and all
other documentation". Done in the release commit (tag `v1.0.2-Beta`): manifest highlights name the spectrum
views and the pop-out windows; manual (Multiband display, pop-out behaviour, radar window), README build
section (`deps/`, SDK variables), release notes (what is new). That commit changes documents and the manifest
tool only, so the plug-in binaries stay those built from `1cf0a90`; the macOS `.pkg` was rebuilt because it
carries the manual PDF (notarization Accepted, stapled, Gatekeeper accepted). The Windows and Linux archives
carry only licence files, unchanged since `1cf0a90`.
