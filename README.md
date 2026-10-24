# Remade Launcher

The official launcher for unofficial apps.

Remade Launcher installs, updates, and removes apps from the signed Remade
catalog, and from public GitHub releases you choose to add yourself. It is a
native Rust application with a desktop interface and a command line, built on
one shared engine.

## Status

Early development. The engine, desktop app, and CLI work end to end against
test fixtures. Two things stand between this and a public release:

- **The official catalog keys.** The build trusts no catalog signing keys yet,
  so it will not accept any catalog from the network. That is deliberate: the
  keys belong to the Remade website's publisher and are added to
  [`crates/domain/launcher-catalog/registry/production.json`](crates/domain/launcher-catalog/registry/production.json)
  once they exist.
- **Evidence on every platform.** Everything is tested on macOS. Windows,
  Linux, and FreeBSD builds, runtime tests, and package builds are written but
  have not been run. See [`packaging/README.md`](packaging/README.md).

## Layout

| Path | What it is |
|---|---|
| `apps/remade-launcher` | The desktop app |
| `apps/remade-launcher-cli` | The `remade` command line, installable separately |
| `crates/foundation/launcher-model` | IDs, errors, product identity, platform targets |
| `crates/foundation/launcher-core` | The engine: catalog, plans, install lifecycle, recovery |
| `crates/domain/launcher-catalog` | Signed-catalog verification |
| `crates/domain/launcher-providers` | GitHub releases, source links |
| `crates/domain/launcher-resolver` | Version and package resolution, sealed install plans |
| `crates/domain/launcher-packages` | Safe archive inspection and extraction |
| `crates/infrastructure/launcher-storage` | SQLite: settings, trust state, receipts, journal |
| `crates/infrastructure/launcher-net` | The HTTPS client |
| `crates/ui/launcher-ui` | The egui interface |
| `packaging/` | DMG, AppImage, Flatpak, deb, rpm, Windows, and CLI archives |

## Build and test

```sh
cargo run -p remade-launcher          # the desktop app
cargo run -p remade-launcher-cli -- --help
cargo test --workspace
cargo clippy --workspace --all-targets
```

Code uses tabs. `cargo fmt` enforces it.

## How installs stay safe

- A catalog is accepted only if a locally trusted Ed25519 key signed its exact
  bytes, and it can never roll back to an older revision.
- Repositories are pinned by numeric ID, so a rename, transfer, or recreation
  cannot pass as the approved one.
- An install is a sealed plan. Approval is bound to its hash, so what you
  approved is exactly what runs.
- Archives are inspected before anything is written, and extraction cannot
  leave its folder. Nothing in a package runs during install.
- Every step is journaled in SQLite. After a crash, recovery rolls back or
  finishes each operation and removes only what the launcher created.
- Uninstall keeps files you edited and files the app made.
- Apps you add yourself are never reviewed by Remade, and the interface says
  so wherever it asks for approval.

## License

See [LICENSE](LICENSE).
