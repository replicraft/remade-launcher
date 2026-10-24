#!/usr/bin/env python3
"""Builds and signs the launcher release feed from a published GitHub release.

The launcher updates itself from one small signed document that names, for
each system, the update archive to download with its size and SHA-256.
This script writes that document (launcher-release.json) and its signature
(launcher-release.sig) from what is on a GitHub release:

    gh release view v0.2.0 --json assets > assets.json
    gh release download v0.2.0 --pattern SHA256SUMS.txt
    RELEASE_SIGNING_KEY=... python3 make-release-feed.py \
        --repo owner/remade-launcher --tag v0.2.0 \
        --assets assets.json --sums SHA256SUMS.txt \
        --key-id remade-updater-2026 --seed-env RELEASE_SIGNING_KEY --out-dir feed

The document is valid for 60 days and carries a revision that is the time it
was made, so re-running it later always produces a newer one. Re-run it at
least every few weeks (the release-feed workflow does), or launchers stop
trusting an expired feed.
"""

import argparse
import json
import os
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

from sign_file import envelope, load_key

# Rust target triple -> (os, architecture) as the launcher names them.
TARGETS = {
    "aarch64-apple-darwin": ("macos", "aarch64"),
    "x86_64-apple-darwin": ("macos", "x86_64"),
    "x86_64-pc-windows-msvc": ("windows", "x86_64"),
    "aarch64-pc-windows-msvc": ("windows", "aarch64"),
    "x86_64-unknown-linux-gnu": ("linux", "x86_64"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64"),
}

VERSION = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


def checksums(path: Path) -> dict[str, str]:
    found = {}

    for line in path.read_text().splitlines():
        parts = line.split()

        if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
            found[parts[1].lstrip("*")] = parts[0]

    return found


def utc(moment: datetime) -> str:
    return moment.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def build(
    repo: str, tag: str, assets: list[dict], sums: dict[str, str], now: datetime, days: int, feed_id: str
) -> dict:
    version = tag.removeprefix("v")

    if not VERSION.match(version):
        sys.exit(f"{tag} is not a plain release version (a pre-release is never offered).")

    sizes = {asset["name"]: asset["size"] for asset in assets}
    artifacts = []

    for triple, (system, architecture) in TARGETS.items():
        for extension in ("tar.gz", "zip"):
            name = f"remade-update-{version}-{triple}.{extension}"

            if name not in sizes:
                continue

            if name not in sums:
                sys.exit(f"{name} has no checksum in SHA256SUMS.txt.")

            artifacts.append(
                {
                    "os": system,
                    "arch": architecture,
                    "name": name,
                    "url": f"https://github.com/{repo}/releases/download/{tag}/{name}",
                    "sha256": sums[name],
                    "size": sizes[name],
                }
            )

    if not artifacts:
        sys.exit(f"{tag} has no update archives (remade-update-{version}-<target>).")

    return {
        "schema_version": 1,
        "feed_id": feed_id,
        "channel": "stable",
        "revision": int(now.timestamp()),
        "issued_at": utc(now),
        "expires_at": utc(now + timedelta(days=days)),
        "version": version,
        "notes_url": f"https://github.com/{repo}/releases/tag/{tag}",
        "artifacts": artifacts,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--repo", required=True, help="owner/repository")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--assets", type=Path, required=True, help="output of `gh release view --json assets`")
    parser.add_argument("--sums", type=Path, required=True, help="SHA256SUMS.txt of the release")
    parser.add_argument("--key-id", required=True)
    parser.add_argument("--seed-env", default="SIGNING_KEY")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--now", help="override the time, as 2030-01-10T00:00:00Z (for tests)")
    parser.add_argument("--days", type=int, default=60)
    parser.add_argument("--feed-id", default="wiki.remade.launcher-releases")
    arguments = parser.parse_args()

    seed = os.environ.get(arguments.seed_env)

    if not seed:
        sys.exit(f"Set {arguments.seed_env} to the base64 signing key.")

    now = (
        datetime.strptime(arguments.now, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
        if arguments.now
        else datetime.now(timezone.utc).replace(microsecond=0)
    )
    assets = json.loads(arguments.assets.read_text())["assets"]
    document = build(arguments.repo, arguments.tag, assets, checksums(arguments.sums), now, arguments.days, arguments.feed_id
    )

    # What is signed is exactly what is written, byte for byte.
    body = json.dumps(document, separators=(",", ":")).encode()

    arguments.out_dir.mkdir(parents=True, exist_ok=True)
    (arguments.out_dir / "launcher-release.json").write_bytes(body)
    (arguments.out_dir / "launcher-release.sig").write_bytes(
        envelope(arguments.key_id, load_key(seed), body)
    )

    print(f"wrote the feed for {arguments.tag}: {len(document['artifacts'])} archives, valid until {document['expires_at']}")


if __name__ == "__main__":
    main()
