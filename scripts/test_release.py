import hashlib
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile
from release import TARGETS, assemble, formula, package, version


class ReleaseTests(unittest.TestCase):
    def test_all_archives_and_formula(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "binary"
            binary.write_bytes(b"test executable")
            output = root / "dist"
            for system, arch in TARGETS:
                path = package(binary, output, "v0.1.0", system, arch)
                first = path.read_bytes()
                package(binary, output, "v0.1.0", system, arch)
                self.assertEqual(first, path.read_bytes(), "archive must be reproducible")
                if system == "windows":
                    with zipfile.ZipFile(path) as archive:
                        self.assertEqual(set(archive.namelist()), {"grip.exe", "README.md", "LICENSE"})
                        self.assertEqual(archive.read("grip.exe"), binary.read_bytes())
                else:
                    with tarfile.open(path) as archive:
                        self.assertEqual(set(archive.getnames()), {"grip", "README.md", "LICENSE"})
                        self.assertEqual(archive.getmember("grip").mode, 0o755)
            assemble(output, "v0.1.0")
            for line in (output / "checksums.txt").read_text().splitlines():
                digest, filename = line.split("  ")
                self.assertEqual(digest, hashlib.sha256((output / filename).read_bytes()).hexdigest())
            self.assertEqual((output / "grip.rb").read_text().count('sha256 "'), 4)

    def test_reject_missing_or_unsafe_input(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                assemble(Path(directory), "v0.1.0")
        for value in ["v1; rm -rf /", "../v1.0.0", "1.0.0", "v1.0.0\n"]:
            with self.assertRaises(ValueError):
                version(value)
        with self.assertRaises(ValueError):
            formula("dev", {})


if __name__ == "__main__":
    unittest.main()
