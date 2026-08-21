from __future__ import annotations

import importlib.util
import json
import stat
import sys
import tempfile
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "oracle_geonames", Path(__file__).with_name("geonames.py")
)
assert SPEC and SPEC.loader
geonames = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = geonames
SPEC.loader.exec_module(geonames)


class GeoNamesToolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        for index, name in enumerate(geonames.NAMES, start=1):
            (self.upstream / name).write_bytes(f"fixture-{index}\n".encode())
        entries = []
        for name in geonames.NAMES:
            digest, length = geonames.digest(self.upstream / name)
            entries.append(geonames.LockEntry(name, digest, length))
        self.lock_path = self.root / "geonames.lock"
        geonames.write_lock(self.lock_path, "2026-08-21T00:00:00Z", entries)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_download_check_and_stage_use_one_lock(self) -> None:
        source = self.root / "cache" / "source"
        geonames.download(self.lock_path, source, self.upstream.as_uri(), 5)
        geonames.verify(geonames.read_lock(self.lock_path), source)
        output = self.root / "dist"
        geonames.stage(self.lock_path, source, output)
        target = output / "catalog" / "geonames"
        manifest = json.loads((target / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["retrieved_at"], "2026-08-21T00:00:00Z")
        self.assertIn("CC BY 4.0", (target / "ATTRIBUTION.txt").read_text(encoding="utf-8"))
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o755)
        self.assertTrue(all(stat.S_IMODE(path.stat().st_mode) == 0o644 for path in target.iterdir()))

    def test_corruption_fails_offline(self) -> None:
        source = self.root / "source"
        geonames.download(self.lock_path, source, self.upstream.as_uri(), 5)
        (source / geonames.NAMES[1]).write_bytes(b"tampered")
        with self.assertRaisesRegex(geonames.GeoNamesError, "verification failed"):
            geonames.verify(geonames.read_lock(self.lock_path), source)

    def test_failed_download_does_not_replace_verified_cache(self) -> None:
        source = self.root / "source"
        geonames.download(self.lock_path, source, self.upstream.as_uri(), 5)
        before = {name: (source / name).read_bytes() for name in geonames.NAMES}
        (self.upstream / geonames.NAMES[0]).write_bytes(b"upstream drift")
        with self.assertRaisesRegex(geonames.GeoNamesError, "verification failed"):
            geonames.download(self.lock_path, source, self.upstream.as_uri(), 5)
        self.assertEqual(before, {name: (source / name).read_bytes() for name in geonames.NAMES})

    def test_candidate_lock_never_edits_tracked_lock(self) -> None:
        tracked = self.lock_path.read_bytes()
        output = self.root / "var" / "geonames.candidate.lock"
        source = self.root / "var" / "candidate" / "source"
        geonames.candidate_lock(
            output,
            source,
            self.upstream.as_uri(),
            5,
            captured_at="2026-08-21T01:00:00Z",
        )
        self.assertEqual(self.lock_path.read_bytes(), tracked)
        self.assertIn("Candidate only", output.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
