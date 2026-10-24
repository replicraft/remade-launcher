#!/usr/bin/env bash
#
# Builds an AppImage from a release binary.
#
#   packaging/linux/build-appimage.sh
#
# Run on Linux, ideally on the oldest distribution you support, so the binary
# links against an older glibc. Requires `appimagetool` on PATH, or set
# APPIMAGETOOL to its location.

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/packaging/linux"
dist="$root/dist"

app_id="wiki.remade.Launcher"
binary="remade-launcher"
arch="$(uname -m)"
version="$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" \
	| python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "remade-launcher"))')"

appimagetool="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
[[ -n "$appimagetool" ]] || { echo "appimagetool is missing: https://github.com/AppImage/appimagetool/releases" >&2; exit 1; }

cd "$root"
cargo build --release --locked -p remade-launcher -p remade-launcher-daemon

appdir="$dist/AppDir"
rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" "$appdir/usr/share/metainfo"

install -m 755 "target/release/$binary" "$appdir/usr/bin/$binary"
install -m 755 "target/release/remade-launcher-daemon" "$appdir/usr/bin/remade-launcher-daemon"
install -m 644 "$here/$app_id.desktop" "$appdir/usr/share/applications/$app_id.desktop"
install -m 644 "$here/$app_id.metainfo.xml" "$appdir/usr/share/metainfo/$app_id.metainfo.xml"
cp -R "$root/assets/linux/hicolor" "$appdir/usr/share/icons"

# AppImage expects the desktop file, icon, and launcher at the top level.
cp "$here/$app_id.desktop" "$appdir/$app_id.desktop"
cp "$root/assets/linux/hicolor/256x256/apps/$app_id.png" "$appdir/$app_id.png"
ln -s "usr/bin/$binary" "$appdir/AppRun"

mkdir -p "$dist"
ARCH="$arch" "$appimagetool" --no-appstream "$appdir" "$dist/RemadeLauncher-$version-linux-$arch.AppImage"

rm -rf "$appdir"
echo "Built $dist/RemadeLauncher-$version-linux-$arch.AppImage"
