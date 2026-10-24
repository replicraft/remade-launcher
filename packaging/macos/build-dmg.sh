#!/usr/bin/env bash
#
# Builds "Remade Launcher.app" and wraps it in a drag-to-install disk image.
#
#   packaging/macos/build-dmg.sh              host architecture
#   packaging/macos/build-dmg.sh --universal  arm64 + x86_64 in one binary
#
# Environment:
#   SIGN_IDENTITY  Developer ID identity for codesign. Defaults to an ad-hoc
#                  signature, which runs locally but is not notarized.
#
# Requires: Xcode command line tools and `brew install create-dmg`.

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/packaging/macos"
dist="$root/dist"

app_name="Remade Launcher"
binary="remade-launcher"
version="$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" \
	| python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "remade-launcher"))')"

command -v create-dmg >/dev/null || { echo "create-dmg is missing: brew install create-dmg" >&2; exit 1; }

# --- Build ------------------------------------------------------------------

cd "$root"

if [[ "${1:-}" == "--universal" ]]; then
	for target in aarch64-apple-darwin x86_64-apple-darwin; do
		cargo build --release --locked -p remade-launcher --target "$target"
	done

	executable="$dist/$binary-universal"
	mkdir -p "$dist"
	lipo -create -output "$executable" \
		"target/aarch64-apple-darwin/release/$binary" \
		"target/x86_64-apple-darwin/release/$binary"
	arch_label="universal"
else
	cargo build --release --locked -p remade-launcher
	executable="target/release/$binary"
	arch_label="$(uname -m)"
fi

# --- Bundle -----------------------------------------------------------------

staging="$dist/dmg-staging"
bundle="$staging/$app_name.app"

mkdir -p "$dist"
rm -rf "$staging"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"

install -m 755 "$executable" "$bundle/Contents/MacOS/$binary"
install -m 644 "$root/assets/macos/RemadeLauncher.icns" "$bundle/Contents/Resources/RemadeLauncher.icns"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$bundle/Contents/Info.plist"

codesign --force --options runtime --timestamp=none --sign "${SIGN_IDENTITY:--}" "$bundle"
codesign --verify --strict "$bundle"

# --- Disk image -------------------------------------------------------------

# A multi-representation TIFF lets Finder pick the sharp artwork on Retina.
background="$dist/dmg-background.tiff"
tiffutil -cat "$here/dmg-background.png" "$here/dmg-background@2x.png" -out "$background" >/dev/null 2>&1

dmg="$dist/RemadeLauncher-$version-macos-$arch_label.dmg"
rm -f "$dmg"

create-dmg \
	--volname "$app_name" \
	--volicon "$root/assets/macos/RemadeLauncher.icns" \
	--background "$background" \
	--window-pos 200 120 \
	--window-size 660 400 \
	--icon-size 112 \
	--text-size 13 \
	--icon "$app_name.app" 165 200 \
	--hide-extension "$app_name.app" \
	--app-drop-link 495 200 \
	--no-internet-enable \
	"$dmg" \
	"$staging"

rm -rf "$staging" "$background"

echo "Built $dmg"
