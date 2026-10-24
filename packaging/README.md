# Packaging

Every package is built from the same release binary (`cargo build --release`,
fat LTO, `panic = "abort"`, stripped). Output goes to `dist/`, which is ignored
by git.

| Format | How | Verified |
|---|---|---|
| macOS `.dmg` (drag-to-install, arrow background) | `packaging/macos/build-dmg.sh` | Built on macOS arm64 |
| Linux AppImage | `packaging/linux/build-appimage.sh` | Not yet built |
| Flatpak | `packaging/linux/build-flatpak.sh` | Not yet built |
| `.deb` | `cargo deb -p remade-launcher` | Not yet built |
| `.rpm` | `cargo generate-rpm -p apps/remade-launcher` | Not yet built |
| Windows NSIS installer | `cargo packager --config packaging/windows/Packager.toml --release` | Not yet built |

"Not yet built" means the configuration is written but has not been run on its
target OS. None of these may be published until a clean-machine install,
launch, and uninstall has been recorded for them.

## Known gaps

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
