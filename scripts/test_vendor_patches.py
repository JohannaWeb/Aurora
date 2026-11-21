"""Tests for vendor_patches.py, against a synthetic crate (no network)."""

import io
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vendor_patches  # noqa: E402


def crate_archive(name: str, version: str, files: dict) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        for path, text in files.items():
            data = text.encode()
            info = tarfile.TarInfo(f"{name}-{version}/{path}")
            info.size = len(data)
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


class VendorPatchesTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.patches = root / "patches"
        self.out = root / "out"
        (self.patches / "demo-crate-0.3.0-beta.4").mkdir(parents=True)
        archive = crate_archive("demo-crate", "0.3.0-beta.4", {
            "Cargo.toml": "[package]\nname = \"demo-crate\"\n",
            "src/lib.rs": "pub fn answer() -> u32 {\n    41\n}\n",
        })
        for name, value in [("PATCHES", self.patches), ("OUT", self.out), ("ROOT", root)]:
            patcher = mock.patch.object(vendor_patches, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        patcher = mock.patch.object(vendor_patches, "crate_archive", lambda crate: archive)
        patcher.start()
        self.addCleanup(patcher.stop)

    def tearDown(self):
        self.tmp.cleanup()

    def crate(self):
        (crate,) = vendor_patches.crates()
        return crate

    def commit_change(self, tree: Path):
        (tree / "src" / "lib.rs").write_text("pub fn answer() -> u32 {\n    42\n}\n")
        vendor_patches.git(tree, "commit", "-q", "-am", "Fix the answer")

    def test_parses_prerelease_versions(self):
        crate = self.crate()
        self.assertEqual((crate.name, crate.version), ("demo-crate", "0.3.0-beta.4"))

    def test_index_paths_follow_the_sparse_index_layout(self):
        self.assertEqual(vendor_patches.index_path("a"), "1/a")
        self.assertEqual(vendor_patches.index_path("ab"), "2/ab")
        self.assertEqual(vendor_patches.index_path("abc"), "3/a/abc")
        self.assertEqual(vendor_patches.index_path("Blitz-Dom"), "bl/it/blitz-dom")

    def test_regen_then_apply_round_trips(self):
        crate = self.crate()
        self.assertTrue(vendor_patches.apply(crate, force=False))
        self.commit_change(crate.tree)
        vendor_patches.regen(crate)
        self.assertEqual(len(crate.patches()), 1)
        self.assertIn("Fix the answer", crate.patches()[0].read_text())
        # The series matches the tree, so applying again is a no-op.
        self.assertFalse(vendor_patches.apply(crate, force=False))
        # A fresh tree from the series has the change.
        subprocess.run(["rm", "-rf", str(crate.tree)], check=True)
        self.assertTrue(vendor_patches.apply(crate, force=False))
        self.assertIn("42", (crate.tree / "src" / "lib.rs").read_text())

    def test_apply_keeps_unregenerated_work(self):
        crate = self.crate()
        vendor_patches.apply(crate, force=False)
        self.commit_change(crate.tree)
        (crate.series / "0001-other.patch").write_text("")  # the series changed
        with self.assertRaises(SystemExit) as raised:
            vendor_patches.apply(crate, force=False)
        self.assertIn("run regen", str(raised.exception))

    def test_regen_refuses_uncommitted_changes(self):
        crate = self.crate()
        vendor_patches.apply(crate, force=False)
        (crate.tree / "src" / "lib.rs").write_text("dirty\n")
        with self.assertRaises(SystemExit):
            vendor_patches.regen(crate)


if __name__ == "__main__":
    unittest.main()
