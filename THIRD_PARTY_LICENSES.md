# Third-party licenses

OpenLimiter's own source code is MIT licensed (see `LICENSE`). The official
binaries are proprietary freeware (see `BINARY_LICENSE.txt`). That is not a
relicensing of the components below, which keep their own terms. The installers
carry copies of the licence texts in `Licenses/`.

| Component | Used for | License |
|---|---|---|
| OpenPluginSDK | plugin framework; the CLAP and LV2 targets and the VST3, Audio Unit and AAX shells | MIT or Apache License 2.0 |
| GoodLookinDri | the editor's drawing and window code (Rust, linked statically) | MIT |
| Source Sans Pro (Adobe) | the editor's lettering, embedded in GoodLookinDri | SIL Open Font License 1.1 (`Licenses/SourceSansPro-OFL.txt`) |
| Rust standard library | all code | MIT or Apache License 2.0 |
| fontdue, ttf-parser | lettering | MIT or Apache License 2.0 (fontdue also Zlib) |
| rustfft, num-complex, num-integer, num-traits, primal-check, strength_reduce, transpose | FFTs for the linear-phase EQ and the displays | MIT or Apache License 2.0 |
| hashbrown, ahash, allocator-api2, once_cell, cfg-if, log, memchr, itoa, serde, serde_json, zerocopy, zmij, raw-window-handle | support libraries | MIT or Apache License 2.0 (zerocopy also BSD 2-Clause, raw-window-handle also Zlib) |
| libc, bitflags, windows-sys, windows-link | system interfaces (per platform) | MIT or Apache License 2.0 |
| x11rb, rustix, errno, linux-raw-sys, gethostname | Linux system interfaces | MIT or Apache License 2.0 (linux-raw-sys also Apache-2.0 WITH LLVM-exception) |
| CLAP | CLAP format | MIT |
| LV2 | LV2 format: the interface is written from the specification, and no LV2 code is linked | ISC |
| VST3 SDK (Steinberg) | VST3 format | MIT |
| AudioUnitSDK (Apple) | Audio Unit format | Apache License 2.0 |
| AAX SDK (Avid) | AAX format | Avid AAX Developer Agreement / SDK License |

The AAX SDK is an external proprietary dependency under the Avid AAX Developer
Agreement and is not committed to this repository. Official AAX binaries are
digitally signed with PACE Eden for Pro Tools compatibility.

VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a
trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology,
Inc. Other format and host names are used only to describe compatibility.
