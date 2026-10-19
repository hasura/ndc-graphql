"""Exercise the publication preflight without creating any tags."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

CHECKER = Path(__file__).with_name("check-release-version.py").read_text()


class ReleaseVersionTests(unittest.TestCase):
    def run_check(self, ref, lock_version="0.4.0", changelog="## [0.4.0]"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "ci").mkdir()
            checker = root / "ci/check-release-version.py"
            checker.write_text(CHECKER)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.4.0"\n')
            (root / "Cargo.lock").write_text(
                "\n".join(
                    f'[[package]]\nname = "{name}"\nversion = "{lock_version}"'
                    for name in ("common", "ndc-graphql", "ndc-graphql-cli")
                )
            )
            (root / "CHANGELOG.md").write_text(changelog)
            return subprocess.run(
                ["python3", str(checker)],
                env={**os.environ, "GITHUB_REF": ref},
                capture_output=True,
                text=True,
                check=False,
            )

    def test_matching_tag(self):
        self.assertEqual(self.run_check("refs/tags/v0.4.0").returncode, 0)

    def test_non_tag_build(self):
        self.assertEqual(self.run_check("refs/heads/main").returncode, 0)

    def test_mismatched_tag(self):
        self.assertNotEqual(self.run_check("refs/tags/v0.3.0").returncode, 0)

    def test_stale_lockfile(self):
        self.assertNotEqual(
            self.run_check("refs/tags/v0.4.0", lock_version="0.3.0").returncode, 0
        )

    def test_missing_changelog(self):
        self.assertNotEqual(
            self.run_check("refs/tags/v0.4.0", changelog="## [Unreleased]").returncode,
            0,
        )


if __name__ == "__main__":
    unittest.main()