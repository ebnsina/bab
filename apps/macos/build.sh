#!/bin/sh
# Build the macOS app bundle.
#
#   ./apps/macos/build.sh [debug|release]
#
# There is no Xcode project on purpose: swiftc links the Rust staticlib directly, so
# the build stays a two-line story rather than a checked-in pbxproj nobody can read.

set -eu

PROFILE="${1:-debug}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/target/$PROFILE"
APP="$OUT/bab.app"

case "$PROFILE" in
  debug)   CARGO_FLAGS="" ;;
  release) CARGO_FLAGS="--release" ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

echo "==> building libbab ($PROFILE)"
# shellcheck disable=SC2086
cargo build -p libbab $CARGO_FLAGS --manifest-path "$ROOT/Cargo.toml"

echo "==> building bab.app"
mkdir -p "$APP/Contents/MacOS"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>bab</string>
  <key>CFBundleDisplayName</key><string>bab</string>
  <key>CFBundleIdentifier</key><string>dev.ebnsina.bab</string>
  <key>CFBundleExecutable</key><string>bab</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
PLIST

swiftc \
  -o "$APP/Contents/MacOS/bab" \
  -import-objc-header "$ROOT/crates/libbab/include/bab.h" \
  -L "$OUT" -lbab \
  -framework AppKit -framework Metal -framework QuartzCore \
  -lobjc \
  "$ROOT/apps/macos/Sources/TerminalView.swift" \
  "$ROOT/apps/macos/Sources/main.swift"

echo "==> $APP"
echo "run: open $APP    (or $APP/Contents/MacOS/bab to see stderr)"
