#!/bin/sh
# OpenKumite.app from a built openkumite (.github/workflows/release.yml: the universal one): the program, its icon
# (openkumite.icns, from packaging/icons.sh) and Info.plist with the version; signed ad hoc, as macOS runs no
# unsigned code on Apple Silicon (not notarized: a downloaded copy is still blocked the first time). The converted
# game data can go in OpenKumite.app/Contents/Resources/assets (README, Assets).
# usage: sh packaging/macos/bundle.sh BINARY VERSION OUTDIR   (VERSION without the v: 1.2.3)
set -e
bin=$1 version=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
app=$out/OpenKumite.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/openkumite"
cp "$here/openkumite.icns" "$app/Contents/Resources/"
cp "$here/Info.plist" "$app/Contents/"
# (a pre-release's suffix is not a bundle version: 1.2.3-rc1 is 1.2.3 there)
plutil -replace CFBundleShortVersionString -string "${version%%-*}" "$app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "${version%%-*}" "$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
echo "$app"
