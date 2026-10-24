#!/usr/bin/env bash
#
# Runs the debug build inside a throwaway "Remade Launcher.app".
#
#   packaging/macos/dev-app.sh [--release] [-- args]
#
# macOS names the menu bar, the Dock, and Cmd-Tab after the app bundle, so a
# bare `cargo run` shows the executable's file name instead. Wrapping the same
# binary in a bundle shows the real name and icon. Environment such as
# REMADE_DEMO=1 and REMADE_DATA_DIR passes straight through.

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
profile="debug"
flags=()

if [[ "${1:-}" == "--release" ]]; then
	profile="release"
	flags=(--release)
	shift
fi

[[ "${1:-}" == "--" ]] && shift

cd "$root"
cargo build "${flags[@]}" -p remade-launcher

bundle="$root/target/dev-app/Remade Launcher.app"
rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"

install -m 755 "target/$profile/remade-launcher" "$bundle/Contents/MacOS/remade-launcher"
install -m 644 "$root/assets/macos/RemadeLauncher.icns" "$bundle/Contents/Resources/RemadeLauncher.icns"
sed "s/@VERSION@/0.0.0-dev/g" "$root/packaging/macos/Info.plist" > "$bundle/Contents/Info.plist"

exec "$bundle/Contents/MacOS/remade-launcher" "$@"
