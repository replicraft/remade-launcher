# Packaging

Every package is built from a release binary (`cargo build --release`,
fat LTO, `panic = "abort"`, stripped). Output goes to `dist/`, which is ignored
by git.

| Format | How | Verified |
|---|---|---|
| macOS `.dmg` (drag-to-install, arrow background) | `packaging/macos/build-dmg.sh` (needs `pip install dmgbuild`) | Built on macOS arm64; opened and the layout checked by eye |
| `remade` CLI archive (`.tar.gz`, `.zip` on Windows) | `packaging/cli/build-archive.sh [target]` | Built on macOS arm64; checksum and `--version` checked |
| Linux AppImage | `packaging/linux/build-appimage.sh` | Not yet built |
| Flatpak | `packaging/linux/build-flatpak.sh` | Not yet built |
| `.deb` | `cargo deb -p remade-launcher` | Not yet built |
| `.rpm` | `cargo generate-rpm -p apps/remade-launcher` | Not yet built |
| Windows NSIS installer | `cargo packager --config packaging/windows/Packager.toml --release` | Not yet built |

"Not yet built" means the configuration is written but has not been run on its
target OS. None of these may be published until a clean-machine install,
launch, and uninstall has been recorded for them.

## Known gaps

- The DMG layout is written with `dmgbuild`, not Finder scripting, so it needs no
  desktop session or Automation permission. Its background is light on
  purpose: Finder draws icon labels in dark text and cannot be told otherwise.
- The CLI is a separate package from the desktop app (`remade-cli` for deb/rpm,
  `remade-<version>-<target>` archives). Neither requires the other.

- **macOS** artifacts are ad-hoc signed. Developer ID signing and notarization
  need the owner's Apple identity (`SIGN_IDENTITY`); notarization is not
  scripted yet.
- **Windows** installers are unsigned. Signing belongs in a separate protected
  job, not in the build job.
- `Packager.toml` repeats the version from `Cargo.toml`. Keep them in step until
  the release pipeline generates it.
- The metainfo file has no `project_license`; that waits on the owner's license
  decision.
- The Flatpak manifest grants no access beyond the network and graphics.
  Installing other apps from inside a sandbox needs its own design.
- Brand artwork lives in `assets/`, copied from the planning pack. See
  `assets/PROVENANCE.md`.

## Regenerating the DMG background

`packaging/macos/make-background.py` draws the arrow with the standard library
only. Run it when the artwork changes and commit the two PNGs.
