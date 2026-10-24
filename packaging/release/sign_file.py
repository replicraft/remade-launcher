#!/usr/bin/env python3
"""Signs a file the way the Remade launcher checks signatures.

Writes a detached signature envelope:

    {"version":1,"signatures":[{"key_id":"...","algorithm":"ed25519","signature":"<base64>"}]}

The signature is over the exact bytes of the file, so serve the file
unchanged: no minifying, no re-encoding, no added newline.

The private key is the 32-byte Ed25519 seed, base64-encoded, and is read from
an environment variable so it never appears on a command line:

    SIGNING_KEY=... python3 sign_file.py --key-id remade-catalog-2026 catalog.json --out catalog.sig

Needs `pip install cryptography`.
"""

import argparse
import base64
import json
import os
import sys
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def load_key(seed_base64: str) -> Ed25519PrivateKey:
    try:
        seed = base64.b64decode(seed_base64.strip(), validate=True)
    except ValueError:
        sys.exit("The signing key is not valid base64. Keep the trailing '=' signs.")

    if len(seed) != 32:
        sys.exit(f"The signing key must be 32 bytes, not {len(seed)}.")

    return Ed25519PrivateKey.from_private_bytes(seed)


def envelope(key_id: str, key: Ed25519PrivateKey, data: bytes) -> bytes:
    signature = base64.b64encode(key.sign(data)).decode()
    document = {
        "version": 1,
        "signatures": [{"key_id": key_id, "algorithm": "ed25519", "signature": signature}],
    }

    return json.dumps(document, separators=(",", ":")).encode()


def public_key(key: Ed25519PrivateKey) -> str:
    from cryptography.hazmat.primitives import serialization

    raw = key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )

    return base64.b64encode(raw).decode()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("file", type=Path)
    parser.add_argument("--key-id", required=True)
    parser.add_argument("--seed-env", default="SIGNING_KEY")
    parser.add_argument("--out", type=Path, required=True)
    arguments = parser.parse_args()

    seed = os.environ.get(arguments.seed_env)

    if not seed:
        sys.exit(f"Set {arguments.seed_env} to the base64 signing key.")

    key = load_key(seed)

    arguments.out.write_bytes(envelope(arguments.key_id, key, arguments.file.read_bytes()))
    print(f"signed {arguments.file} as {arguments.key_id} (public key {public_key(key)})")


if __name__ == "__main__":
    main()
