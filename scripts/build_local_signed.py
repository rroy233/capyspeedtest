#!/usr/bin/env python3
"""Build Tauri app locally with interactive signing password input.

Defaults:
- Private key path: ~/.tauri/myapp.key
- Build command: bun run tauri build

Usage examples:
  python scripts/build_local_signed.py
  python scripts/build_local_signed.py --key ~/capyspeedtest.key
  python scripts/build_local_signed.py -- --bundles appimage
"""

from __future__ import annotations

import argparse
import getpass
import os
import subprocess
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run local signed Tauri build.")
    parser.add_argument(
        "--key",
        default="~/.tauri/myapp.key",
        help="Path to updater private key (default: ~/.tauri/myapp.key).",
    )
    parser.add_argument(
        "tauri_args",
        nargs=argparse.REMAINDER,
        help="Extra args passed to `tauri build` (prefix with `--`).",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).resolve().parent.parent

    key_path = Path(args.key).expanduser().resolve()
    if not key_path.is_file():
        print(f"Error: private key file not found: {key_path}", file=sys.stderr)
        raise SystemExit(1)

    key_content = key_path.read_text(encoding="utf-8").strip()
    if not key_content:
        print(f"Error: private key file is empty: {key_path}", file=sys.stderr)
        raise SystemExit(1)

    password = getpass.getpass("Enter signing key password (input hidden): ")

    extra_args = list(args.tauri_args)
    if extra_args and extra_args[0] == "--":
        extra_args = extra_args[1:]

    command = ["bun", "run", "tauri", "build", *extra_args]
    env = os.environ.copy()
    env["TAURI_SIGNING_PRIVATE_KEY"] = key_content
    env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = password
    # Backward-compatible aliases for older build setups.
    env["TAURI_PRIVATE_KEY"] = key_content
    env["TAURI_KEY_PASSWORD"] = password

    print(f"Using private key: {key_path}")
    print(f"Running: {' '.join(command)}")
    completed = subprocess.run(command, cwd=repo_root, check=False, env=env)
    raise SystemExit(completed.returncode)


if __name__ == "__main__":
    main()
