#!/usr/bin/env bash
#
# Builds the `remade` command-line tool as a standalone archive.
#
#   packaging/cli/build-archive.sh [rust-target-triple]
#
# Produces dist/remade-<version>-<target>.tar.gz (or .zip for Windows
# targets) and a matching .sha256 file. The archive holds one binary, so the
# CLI installs without the desktop app: unpack it and put `remade` on PATH.

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
dist="$root/dist"
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"

version="$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" \
	| python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "remade-launcher-cli"))')"

cd "$root"
cargo build --release --locked -p remade-launcher-cli --target "$target"

case "$target" in
	*windows*) binary="remade.exe" ;;
	*) binary="remade" ;;
esac

name="remade-$version-$target"
stage="$dist/$name"

mkdir -p "$dist"
rm -rf "$stage"
mkdir -p "$stage"

install -m 755 "target/$target/release/$binary" "$stage/$binary"
install -m 644 "$root/LICENSE" "$stage/LICENSE"

case "$target" in
	*windows*)
		(cd "$dist" && zip -qr "$name.zip" "$name")
		archive="$name.zip"
		;;
	*)
		tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
		archive="$name.tar.gz"
		;;
esac

rm -rf "$stage"

(cd "$dist" && shasum -a 256 "$archive" > "$archive.sha256")

echo "Built $dist/$archive"
