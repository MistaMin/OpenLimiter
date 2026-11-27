#!/bin/sh
# Builds OpenLimiter's macOS bundles into dist/: VST3, Audio Unit, CLAP, LV2 and,
# when an AAX SDK is found, AAX. Universal (arm64 and x86_64), macOS 11.0.
#   scripts/build.sh
# OpenPluginSDK is expected at deps/OpenPluginSDK, a link to the one SDK checkout (the path in
# Cargo.toml), checked out at the revision below; another revision stops the build
# (ALLOW_SDK_MISMATCH=1 overrides it for a test build). Third-party SDKs: set VST3_SDK,
# AU_SDK and AAX_SDK, or keep them in a Dependencies or SDKs folder nearby.
# Signing is optional and only from the environment (see the SDK's
# scripts/build_plugin.sh --help): CODESIGN_IDENTITY, and for AAX WRAPTOOL,
# PACE_ACCOUNT and PACE_WCGUID.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="$(cd "$ROOT/deps/OpenPluginSDK" && pwd)"
# The OpenPluginSDK revision this release is built and tested with.
SDK_REV=d93d926c09ce6ff90193d5824fc5ae831f321f0d

find_sdk() { # env override, then folder names under nearby Dependencies or SDKs folders
  if [ -n "$1" ] && [ -d "$1" ]; then echo "$1"; return 0; fi
  shift
  for base in "$ROOT/.." "$ROOT/../.." "$ROOT/../Dependencies" "$ROOT/../../Dependencies" "$ROOT/../SDKs" "$ROOT/../../SDKs" "$HOME/Dependencies" "$HOME/SDKs"; do
    for rel in "$@"; do
      [ -d "$base/$rel" ] && { (cd "$base/$rel" && pwd); return 0; }
    done
  done
  return 1
}
VST3_SDK="$(find_sdk "$VST3_SDK" vst3sdk)" || { echo "VST3 SDK not found; set VST3_SDK" >&2; exit 1; }
AU_SDK="$(find_sdk "$AU_SDK" AudioUnitSDK)" || { echo "AudioUnitSDK not found; set AU_SDK" >&2; exit 1; }
FORMATS=vst3,au,clap,lv2
if AAX_SDK="$(find_sdk "$AAX_SDK" aax-sdk-2-8-1)"; then FORMATS="$FORMATS,aax"; else AAX_SDK=""; echo "no AAX SDK: AAX is not built" >&2; fi

have="$(git -C "$SDK" rev-parse HEAD 2>/dev/null || echo unknown)"
# The checkout is shared with other plugins and moves with them: a build from another revision
# would ship SDK code nobody tested here. ALLOW_SDK_MISMATCH=1 builds anyway (a test build only).
if [ "$have" != "$SDK_REV" ]; then
  if [ -n "$ALLOW_SDK_MISMATCH" ]; then
    echo "note: OpenPluginSDK is at $have, not $SDK_REV (ALLOW_SDK_MISMATCH): a test build only" >&2
  else
    echo "OpenPluginSDK is at $have; this release is pinned to $SDK_REV." >&2
    echo "Check out $SDK_REV in deps/OpenPluginSDK, or set ALLOW_SDK_MISMATCH=1 for a test build." >&2
    exit 1
  fi
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
# Bundle versions are numbers only; the plugin itself reports the full version.
VERSION="${VERSION%%-*}"

"$SDK/scripts/build_plugin.sh" --manifest-path "$ROOT/Cargo.toml" --name OpenLimiter --version "$VERSION" \
  --bundle-id com.opengrid.openlimiter --manufacturer OpenGrid --au-subtype OLim --au-manufacturer Ogrd \
  --au-version release --ttl-bin lv2_ttl --editor --aax-pages --formats "$FORMATS" --archs universal \
  --vst3-sdk "$VST3_SDK" --au-sdk "$AU_SDK" ${AAX_SDK:+--aax-sdk "$AAX_SDK"} \
  --out "$ROOT/dist" --work-dir "$ROOT/build"
