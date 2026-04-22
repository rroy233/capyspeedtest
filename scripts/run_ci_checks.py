#!/usr/bin/env python3
"""Run local checks mirroring .github/workflows/ci.yml.

Usage:
  python scripts/run_ci_checks.py
  python scripts/run_ci_checks.py --frontend-only
  python scripts/run_ci_checks.py --backend-only
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent


def run_step(title: str, command: str) -> None:
    print(f"\n=== {title} ===")
    print(f"$ {command}")
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        shell=True,
        check=False,
        env=os.environ.copy(),
    )
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def run_frontend() -> None:
    run_step("Frontend: Install dependencies", "bun install")
    run_step("Frontend: TypeScript + Build", "bun run build")
    run_step("Frontend: Unit tests", "bun run test")


def run_backend() -> None:
    dist_dir = REPO_ROOT / "dist"
    dist_dir.mkdir(parents=True, exist_ok=True)
    run_step("Backend: rustfmt --check", "cargo fmt --check --manifest-path src-tauri/Cargo.toml")
    run_step("Backend: clippy", "cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings")
    run_step("Backend: tests", "cargo test --manifest-path src-tauri/Cargo.toml")
    run_step("Cargo Fmt","cargo fmt --manifest-path src-tauri/Cargo.toml")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run local CI checks.")
    parser.add_argument("--frontend-only", action="store_true", help="Run only frontend checks.")
    parser.add_argument("--backend-only", action="store_true", help="Run only backend checks.")
    args = parser.parse_args()
    if args.frontend_only and args.backend_only:
        parser.error("--frontend-only and --backend-only cannot be used together.")
    return args


def main() -> None:
    args = parse_args()
    if args.frontend_only:
        run_frontend()
    elif args.backend_only:
        run_backend()
    else:
        run_frontend()
        run_backend()
    print("\nAll requested checks passed.")


if __name__ == "__main__":
    main()
