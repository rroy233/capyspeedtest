#!/usr/bin/env python3
"""Interactive version updater for this repository.

Updates:
- package.json -> version
- src-tauri/Cargo.toml -> [package].version
- src-tauri/tauri.conf.json -> version
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

SEMVER_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def die(message: str) -> None:
    print(f"Error: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        die(f"failed to parse {path}: {exc}")


def dump_json(path: Path, data: dict) -> None:
    text = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    path.write_text(text, encoding="utf-8")


def read_cargo_version(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    match = re.search(r"(?ms)^\[package\]\s*$.*?^version\s*=\s*\"([^\"]+)\"\s*$", text)
    if not match:
        die(f"could not find [package].version in {path}")
    return match.group(1)


def write_cargo_version(path: Path, new_version: str) -> None:
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(r"(?ms)(^\[package\]\s*$.*?^version\s*=\s*\")([^\"]+)(\"\s*$)")
    replaced, count = pattern.subn(rf"\g<1>{new_version}\g<3>", text, count=1)
    if count != 1:
        die(f"failed to update [package].version in {path}")
    path.write_text(replaced, encoding="utf-8")


def parse_core(version: str) -> tuple[int, int, int]:
    core = version.split("-", 1)[0].split("+", 1)[0]
    major, minor, patch = core.split(".")
    return int(major), int(minor), int(patch)


def bump_version(version: str, mode: str) -> str:
    major, minor, patch = parse_core(version)
    if mode == "major":
        return f"{major + 1}.0.0"
    if mode == "minor":
        return f"{major}.{minor + 1}.0"
    if mode == "patch":
        return f"{major}.{minor}.{patch + 1}"
    die(f"unknown bump mode: {mode}")


def confirm(prompt: str, default: bool = False) -> bool:
    suffix = " [Y/n]: " if default else " [y/N]: "
    value = input(prompt + suffix).strip().lower()
    if not value:
        return default
    return value in {"y", "yes"}


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    package_json_path = root / "package.json"
    cargo_toml_path = root / "src-tauri" / "Cargo.toml"
    tauri_conf_path = root / "src-tauri" / "tauri.conf.json"

    package_json = load_json(package_json_path)
    tauri_conf = load_json(tauri_conf_path)
    cargo_version = read_cargo_version(cargo_toml_path)
    package_version = str(package_json.get("version", ""))
    tauri_version = str(tauri_conf.get("version", ""))

    if not package_version or not tauri_version:
        die("missing version field in package.json or tauri.conf.json")

    print("Current versions:")
    print(f"- package.json:          {package_version}")
    print(f"- src-tauri/Cargo.toml:  {cargo_version}")
    print(f"- src-tauri/tauri.conf.json: {tauri_version}")

    if len({package_version, cargo_version, tauri_version}) != 1:
        print("\nWarning: versions are not aligned.")
        print("The default base version will use package.json.")

    base_version = package_version
    if not SEMVER_RE.match(base_version):
        die(f"package.json version is not semver-compatible: {base_version}")

    print("\nChoose update type:")
    print("1) patch")
    print("2) minor")
    print("3) major")
    print("4) custom")

    choice = input("Select [1-4] (default 1): ").strip() or "1"
    if choice == "1":
        new_version = bump_version(base_version, "patch")
    elif choice == "2":
        new_version = bump_version(base_version, "minor")
    elif choice == "3":
        new_version = bump_version(base_version, "major")
    elif choice == "4":
        new_version = input("Enter new version (semver): ").strip()
    else:
        die("invalid choice")

    if not SEMVER_RE.match(new_version):
        die(f"invalid semver: {new_version}")

    if new_version == base_version and len({package_version, cargo_version, tauri_version}) == 1:
        print("No changes needed.")
        return

    print("\nPlanned changes:")
    print(f"- package.json: {package_version} -> {new_version}")
    print(f"- src-tauri/Cargo.toml: {cargo_version} -> {new_version}")
    print(f"- src-tauri/tauri.conf.json: {tauri_version} -> {new_version}")

    if not confirm("Apply these changes?", default=False):
        print("Cancelled.")
        return

    package_json["version"] = new_version
    tauri_conf["version"] = new_version
    dump_json(package_json_path, package_json)
    dump_json(tauri_conf_path, tauri_conf)
    write_cargo_version(cargo_toml_path, new_version)

    print("\nDone.")
    print(f"Updated version to {new_version}")


if __name__ == "__main__":
    main()
