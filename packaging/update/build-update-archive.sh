#!/usr/bin/env bash
#
# Builds the archive the launcher downloads to update itself.
#
#   packaging/update/build-update-archive.sh [rust-target-triple]
#
# It holds every program the launcher ships (the desktop app, `remade`, the
# background updater, and the assistant server) in one folder, plus the
# `Remade Launcher.app` bundle on macOS. Each install replaces only what it
# already has: a command-line install updates its programs, a folder install
# updates the desktop app beside them, and a macOS app is replaced whole.
#
# Produces dist/remade-update-<version>-<target>.tar.gz (.zip for Windows) and
# a matching .sha256. On macOS run packaging/macos/build-dmg.sh first: it
# leaves the signed app in target/update-app/.

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
dist="$root/dist"
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

version="$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" \
	| python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "remade-launcher"))')"

cd "$root"
cargo build --release --locked --target "$target" \
	-p remade-launcher -p remade-launcher-cli -p remade-launcher-daemon -p remade-launcher-mcp

case "$target" in
	*windows*) suffix=".exe" ;;
	*) suffix="" ;;
esac

name="remade-update-$version-$target"
stage="$dist/$name"

mkdir -p "$dist"
rm -rf "$stage"
mkdir -p "$stage"

for program in remade remade-launcher remade-launcher-daemon remade-launcher-mcp; do
	install -m 755 "target/$target/release/$program$suffix" "$stage/$program$suffix"
done

install -m 644 "$root/LICENSE" "$stage/LICENSE"

case "$target" in
	*apple-darwin*)
		app="$root/target/update-app/Remade Launcher.app"
		[[ -d "$app" ]] || { echo "Run packaging/macos/build-dmg.sh first: $app is missing" >&2; exit 1; }
		# ditto keeps links, permissions, and the code signature intact.
		ditto "$app" "$stage/Remade Launcher.app"
		;;
esac

case "$target" in
	*windows*)
		if command -v zip >/dev/null; then
			(cd "$dist" && zip -qr "$name.zip" "$name")
		else
			(cd "$dist" && 7z a -tzip -bso0 "$name.zip" "$name")
		fi
		archive="$name.zip"
		;;
	*)
		tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
		archive="$name.tar.gz"
		;;
esac

rm -rf "$stage"

if command -v sha256sum >/dev/null; then
	(cd "$dist" && sha256sum "$archive" > "$archive.sha256")
else
	(cd "$dist" && shasum -a 256 "$archive" > "$archive.sha256")
fi

echo "Built $dist/$archive"
