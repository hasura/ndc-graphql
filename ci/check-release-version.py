#!/usr/bin/env python3
"""Fail before any publication when a release tag and source disagree."""
import os
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parent.parent
version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
lock = tomllib.loads((root / "Cargo.lock").read_text())
for package in lock["package"]:
    if package["name"] in {"common", "ndc-graphql", "ndc-graphql-cli"} and "source" not in package:
        assert package["version"] == version, f"Stale lockfile version: {package['name']}"
ref = os.environ.get("GITHUB_REF", "")
if ref.startswith("refs/tags/v"):
    assert ref == f"refs/tags/v{version}", f"Tag {ref} does not match {version}"
    assert f"## [{version}]" in (root / "CHANGELOG.md").read_text(), "Missing changelog section"
print(f"Release version validated: {version}")
