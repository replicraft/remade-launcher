#!/usr/bin/env bash
#
# Builds a Flatpak bundle.
#
#   packaging/linux/build-flatpak.sh
#
# Requires flatpak, flatpak-builder, and the freedesktop 24.08 runtime and
# SDK plus the rust-stable extension:
#
#   flatpak install flathub org.freedesktop.Platform//24.08 \
#       org.freedesktop.Sdk//24.08 org.freedesktop.Sdk.Extension.rust-stable//24.08
#
# Flatpak builds are offline, so the Cargo dependencies are vendored into
# cargo-sources.json by flatpak-cargo-generator.py (from
# github.com/flatpak/flatpak-builder-tools, the `cargo` directory).

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="$root/packaging/linux"
dist="$root/dist"
build="$dist/flatpak-build"

generator="${FLATPAK_CARGO_GENERATOR:-$(command -v flatpak-cargo-generator.py || true)}"
[[ -n "$generator" ]] || { echo "flatpak-cargo-generator.py is missing; set FLATPAK_CARGO_GENERATOR" >&2; exit 1; }

mkdir -p "$dist"
python3 "$generator" "$root/Cargo.lock" -o "$dist/cargo-sources.json"

flatpak-builder --force-clean --user --install-deps-from=flathub \
	--repo="$dist/flatpak-repo" "$build" "$here/wiki.remade.Launcher.yml"

flatpak build-bundle "$dist/flatpak-repo" "$dist/RemadeLauncher.flatpak" wiki.remade.Launcher

echo "Built $dist/RemadeLauncher.flatpak"
