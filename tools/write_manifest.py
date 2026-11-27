#!/usr/bin/env python3
"""Writes opengrid-manifest.json (schema 2) for an OpenLimiter release.

  tools/write_manifest.py --version 1.0.0-Beta --released 2026-11-23 --out DIR/opengrid-manifest.json \
      [--mac PKG] [--windows EXE] [--linux TAR_GZ]

Size and SHA-256 come from each final file (after signing, notarization and stapling).
The install paths are the ones scripts/make_installer.sh, installer/windows.iss and the
Linux archive step of .github/workflows/build.yml use.
"""
import argparse
import hashlib
import json
from pathlib import Path

NAME = "OpenLimiter"
HIGHLIGHTS = [
    "VST3: every value survives save and reopen, also in hosts that save before delivering the last changes",
    "Smaller macOS plug-ins (7.8 MB instead of 19.5 MB) and less idle CPU",
    "NaN, infinity and denormal numbers in the audio are set to 0",
    "Seventeen mastering modules in one movable chain, then Requantise",
    "macOS universal (VST3, AU, CLAP, LV2, AAX), Windows and Linux",
]


def entry(path: Path, os_name: str, arch: list, min_os: str, installs: list) -> dict:
    data = path.read_bytes()
    build_class = "modern"  # macOS 11 or later, Windows 10 or later, Linux
    if os_name == "macos":
        assert float(min_os) >= 11.0, "a modern macOS build needs 11.0 or later"
    return {
        "os": os_name,
        "arch": arch,
        "min_os": min_os,
        "build_class": build_class,
        "name": path.name,
        "size": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "installs": [{"format": f, "path": p} for f, p in installs],
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--version", required=True)
    ap.add_argument("--released", required=True)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--mac", type=Path)
    ap.add_argument("--windows", type=Path)
    ap.add_argument("--linux", type=Path)
    a = ap.parse_args()
    archives = []
    if a.mac:
        archives.append(entry(a.mac, "macos", ["universal"], "11.0", [
            ("vst3", f"Library/Audio/Plug-Ins/VST3/{NAME}.vst3"),
            ("au", f"Library/Audio/Plug-Ins/Components/{NAME}.component"),
            ("clap", f"Library/Audio/Plug-Ins/CLAP/{NAME}.clap"),
            ("lv2", f"Library/Audio/Plug-Ins/LV2/{NAME}.lv2"),
            ("aax", f"Library/Application Support/Avid/Audio/Plug-Ins/{NAME}.aaxplugin"),
        ]))
    if a.windows:
        archives.append(entry(a.windows, "windows", ["x64"], "10.0", [
            ("vst3", f"Common Files/VST3/{NAME}.vst3"),
            ("clap", f"Common Files/CLAP/{NAME}.clap"),
        ]))
    if a.linux:
        # Linux: the minimum is the glibc of the build machine (Ubuntu 22.04).
        archives.append(entry(a.linux, "linux", ["x86_64"], "2.35", [
            ("clap", f"usr/lib/clap/{NAME}.clap"),
            ("lv2", f"usr/lib/lv2/{NAME}.lv2"),
        ]))
    if not archives:
        raise SystemExit("no archive given")
    manifest = {
        "schema": 2,
        "plugin": NAME,
        "version": a.version,
        "released": a.released,
        "highlights": HIGHLIGHTS,
        "archives": archives,
    }
    a.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {a.out} with {len(archives)} archive(s)")


if __name__ == "__main__":
    main()
