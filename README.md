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
- **The launcher release key.** Updates to the launcher itself are signed with
  a key of their own, kept apart from the catalog keys. The build trusts none
  yet, so it checks nothing and installs nothing until one is added to
  [`crates/domain/launcher-catalog/registry/releases.json`](crates/domain/launcher-catalog/registry/releases.json).
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

## Launcher updates

The launcher keeps itself current under a setting of its own, apart from how
apps are updated. Change one and the other does not move.

| Setting | What happens |
|---|---|
| Automatic (default) | A new launcher is downloaded in the background and checked. It is switched in when you close the launcher, never while an install is running. |
| Tell me | The launcher says a new version exists. Nothing is downloaded. |
| Off | It never looks on its own. |

Set it in Settings, or with `remade config set launcher.updates notify`.
`remade self-update status | check | apply | rollback` do it by hand, and a
person asking directly is always answered, whatever the setting.

**Who updates what**

- **The desktop app** looks when it opens and switches when it closes.
- **The command line** does the same after a successful command, but only for
  a person at a terminal. A script, a pipe, `--json`, `--quiet`, `--offline`,
  CI, or `REMADE_NO_SELF_UPDATE=1` never sees a new version arrive mid-run.
- **The background updater and the assistant server** never update the
  launcher themselves. They are replaced on disk with the rest, and run the
  new version the next time they start: the updater at your next login or
  restart, the assistant server the next time an assistant launches it. On
  macOS both sit inside the app, so they move with it.

**What gets replaced.** One update archive per system holds all four programs
(`remade-launcher`, `remade`, `remade-launcher-daemon`,
`remade-launcher-mcp`) and, on macOS, `Remade Launcher.app`. Each install
replaces only what it already has:

- a folder of programs (the Windows installer, an unpacked archive) has the
  programs that are in the folder replaced, together or not at all;
- a macOS app is replaced whole, because changing a file inside would break
  its code signature.

A launcher installed by Homebrew, the Microsoft Store, Flatpak, an AppImage,
or the system package manager is left alone and only told about new releases.

**What keeps it safe.** The release is a small document signed by a key that
is only good for launcher releases. It names, for each system, a GitHub
release file with its size and SHA-256. The download must match both, the
archive is unpacked with the same path and size limits as an app, and the
program that is running must be in it. The previous version is kept so
`rollback` can put it back. An older document cannot replace a newer one, an
expired one is ignored, and a launcher is never moved to an older version.
The desktop app, the command line, and anything else on the machine take
turns through a lock, so two never update at once.

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
