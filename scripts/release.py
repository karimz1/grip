"""Reproducible release archives and a verified Homebrew formula (stdlib only)."""
import argparse
import gzip
import hashlib
import io
from pathlib import Path
import re
import tarfile
import zipfile

TARGETS = [("linux", "amd64"), ("linux", "arm64"), ("darwin", "amd64"),
           ("darwin", "arm64"), ("windows", "amd64"), ("windows", "arm64"), ("windows", "386")]


def version(value):
    if value != "dev" and not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", value):
        raise ValueError("version must be dev or a v-prefixed semantic version")
    return value


def archive_name(tag, system, arch):
    return f"grip_{version(tag)}_{system}_{arch}." + ("zip" if system == "windows" else "tar.gz")


def package(binary, output, tag, system, arch):
    if (system, arch) not in TARGETS:
        raise ValueError("unsupported target")
    output.mkdir(parents=True, exist_ok=True)
    name = "grip.exe" if system == "windows" else "grip"
    data = binary.read_bytes()
    if not data:
        raise ValueError("empty executable")
    raw = output / (f"grip-{system}-{arch}" + (".exe" if system == "windows" else ""))
    raw.write_bytes(data)
    raw.chmod(0o755)
    files = {name: data, "README.md": Path("README.md").read_bytes(), "LICENSE": Path("LICENSE").read_bytes()}
    destination = output / archive_name(tag, system, arch)
    if system == "windows":
        with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for filename, contents in files.items():
                item = zipfile.ZipInfo(filename, (1980, 1, 1, 0, 0, 0))
                item.external_attr = (0o755 if filename == name else 0o644) << 16
                item.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(item, contents)
    else:
        with destination.open("wb") as out:
            with gzip.GzipFile(filename="", mode="wb", fileobj=out, mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w") as archive:
                    for filename, contents in files.items():
                        item = tarfile.TarInfo(filename)
                        item.size = len(contents)
                        item.mode = 0o755 if filename == name else 0o644
                        archive.addfile(item, io.BytesIO(contents))
    return destination


def formula(tag, checksums, repository="karimz1/grip"):
    version(tag)
    if tag == "dev":
        raise ValueError("Homebrew requires a tagged release")
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repository):
        raise ValueError("invalid GitHub repository")
    result = ['class Grip < Formula', '  desc "See which processes are using your files"',
              f'  homepage "https://github.com/{repository}"', f'  version "{tag[1:]}"',
              '  license "MIT"', '  conflicts_with "homebrew/core/grip", because: "both install a grip executable"', '']
    for system, block in [("darwin", "macos"), ("linux", "linux")]:
        result.append(f"  on_{block} do")
        for arch, brewarch in [("arm64", "arm"), ("amd64", "intel")]:
            name = archive_name(tag, system, arch)
            digest = checksums[name]
            if not re.fullmatch(r"[0-9a-f]{64}", digest):
                raise ValueError("invalid SHA-256 digest")
            result += [f"    on_{brewarch} do", f'      url "https://github.com/{repository}/releases/download/{tag}/{name}"',
                       f'      sha256 "{digest}"', '    end']
        result += ['  end', '']
    result += ['  def install', '    bin.install "grip"', '  end', '', '  test do',
               '    assert_match "grip #{version}", shell_output("#{bin}/grip --version")', '  end', 'end', '']
    return "\n".join(result)


def assemble(output, tag):
    expected = []
    for system, arch in TARGETS:
        expected += [archive_name(tag, system, arch), f"grip-{system}-{arch}" + (".exe" if system == "windows" else "")]
    missing = [name for name in expected if not (output / name).is_file()]
    if missing:
        raise ValueError(f"missing tested artifacts: {missing}")
    sums = {name: hashlib.sha256((output / name).read_bytes()).hexdigest() for name in sorted(expected)}
    (output / "grip.rb").write_text(formula(tag, sums), encoding="utf-8")
    sums["grip.rb"] = hashlib.sha256((output / "grip.rb").read_bytes()).hexdigest()
    (output / "checksums.txt").write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(sums.items())), encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["package", "assemble", "validate"])
    parser.add_argument("--version", required=True, type=version)
    parser.add_argument("--os")
    parser.add_argument("--arch")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path, default=Path("dist"))
    args = parser.parse_args()
    if args.action == "package":
        package(args.binary, args.output, args.version, args.os, args.arch)
    elif args.action == "assemble":
        assemble(args.output, args.version)
