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

## Automatic updates

Apps update in one of three modes, chosen in Settings or with
`remade updates mode set`:

| Mode | What happens |
|---|---|
| Manual (default) | The launcher looks for updates when it opens and shows an Update button. Nothing installs on its own. |
| On launcher start | Once per real launcher start, eligible updates install. Nothing runs in the background. |
| Background updater | A per-user program registered at login checks every 24 hours, and when the launcher opens. |

An automatic mode needs a recorded acknowledgement, and switching modes never
starts an update by itself. A release must be at least a minimum age old
before an automatic update installs it (one hour by default, overridable for
each app, and never applied to an update you start yourself). Each app can be
pinned, paused, or allowed pre-releases. Apps you added yourself are never
updated unattended. Every skip has a stable reason code, shown by
`remade updates check` and `remade updates decisions`.

## The command line

`remade` drives the same engine. Results are one versioned JSON object with
`--json` (or NDJSON progress with `--output ndjson`), failures carry a typed
code and a named exit status, and every change is a saved plan you approve:

```sh
remade search editor
remade plan install sample-editor      # a plan ID; nothing changes yet
remade apply <plan-id>                 # approve exactly that plan
remade install https://github.com/owner/repo --accept-unreviewed-source owner/repo
remade check && remade update --all
```

`--yes` approves a prompt but never accepts an unreviewed source: that needs
the exact `owner/repository`. `--no-input` makes any question an error.

More of what it does:

```sh
remade repair quill                    # look first; put it back if it is damaged
remade download quill --out ~/Downloads  # fetch and check the file; run nothing
remade adopt /opt/quill --yes          # watch an app you already have; never changed
remade collection export --out apps.json && remade collection preview apps.json
remade auth login --with-token < token.txt   # optional: raises GitHub's lookup limit
remade config set region.time_format 24h
```

Reviews belong to a community service that does not exist yet, so
`remade reviews` says so instead of pretending.

## For AI assistants

`remade-launcher-mcp` serves the same engine over the Model Context Protocol
on standard input and output. By default an assistant can search, inspect,
check for updates, and *plan* changes, but not make them. Applying a saved
plan needs `--allow-apply` (and `--allow-destructive` for removals), the
assistant must name the exact app and version the plan shows, and an app from
a source Remade has not reviewed can only be accepted by a person.

## Layout

| Path | What it is |
|---|---|
| `apps/remade-launcher` | The desktop app |
| `apps/remade-launcher-cli` | The `remade` command line, installable separately |
| `apps/remade-launcher-daemon` | The optional per-user background updater |
| `apps/remade-launcher-mcp` | An MCP server for AI assistants, read and plan by default |
| `crates/foundation/launcher-model` | IDs, errors, product identity, platform targets |
| `crates/foundation/launcher-core` | The engine: catalog, plans, install lifecycle, recovery |
| `crates/domain/launcher-catalog` | Signed-catalog verification |
| `crates/domain/launcher-providers` | GitHub releases, source links |
| `crates/domain/launcher-resolver` | Version and package resolution, sealed install plans |
| `crates/domain/launcher-packages` | Safe archive inspection and extraction |
| `crates/platform/launcher-background` | Login registration, the updater's socket protocol, and its schedule |
| `crates/infrastructure/launcher-storage` | SQLite: settings, trust state, receipts, journal |
| `crates/infrastructure/launcher-net` | The HTTPS client |
| `packaging/` | DMG, AppImage, Flatpak, deb, rpm, Windows, and CLI archives |

## Build and test

```sh
cargo run -p remade-launcher          # the desktop app
packaging/macos/dev-app.sh            # the same, inside an app bundle so macOS shows the real name
REMADE_DEMO=1 cargo run -p remade-launcher   # the interface with made-up apps
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
