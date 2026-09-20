# Releases and the Homebrew tap

The public release workflow creates **drafts only** for review before publishing.

## What CI tests

Every push to `main` and every pull request runs the following matrix:

| Runner | Target | Race detector |
| --- | --- | --- |
| Ubuntu 24.04 x86-64 | linux/amd64 | Yes |
| Ubuntu 24.04 ARM64 | linux/arm64 | Yes |
| macOS 15 Intel | darwin/amd64 | Yes |
| macOS 15 Apple Silicon | darwin/arm64 | Yes |
| Windows 2025 x86-64 | windows/amd64 | Yes |
| Windows 11 ARM | windows/arm64 | No (Go does not support it) |
| Windows 2025 x86-64, 32-bit Go target | windows/386 | No (Go does not support it) |

Tests start child processes using an explicit pipe handshake. They cover an open file
with spaces and Unicode, directory descendants, process metadata, cancellation, stale
identity rejection, force termination, and attempts to terminate exited or protected
identities. Unix tests also cover CWD, mapped files and SIGTERM. Linux additionally
checks deleted-but-open files. Windows verifies that normal termination of a console
helper returns an honest error instead of silently force-killing it.

Path tests run on the host OS, including Windows drive, separator, case and UNC rules.
UI tests verify searching, resizing, table navigation and safe confirmation behavior.
Packaging tests verify archive contents, executable permissions, reproducibility,
checksums and the four-platform Homebrew formula.

The no-cgo tests exercise the same runtime configuration as release binaries. The race
pass is an additional check. Each job builds, runs `--version` and `--help`, then packages
its binary. Release jobs consume these tested artifacts; they do not rebuild on Linux
and assume the result works elsewhere.

## Create a draft

From a reviewed commit:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The Release workflow reruns the full matrix for that tag and installs/tests the generated Homebrew formula on Linux and macOS. If any target fails, no draft
is created. A passing run uploads seven raw executables, seven archives, `checksums.txt`
and a generated `grip.rb` to the draft release. Version strings omit the leading `v`.

To retry an existing tag, use the workflow UI and select **that tag** as the workflow
ref, or run:

```sh
gh workflow run release.yml --ref v0.1.0 -f tag=v0.1.0
```

The workflow verifies that the tag resolves to the workflow commit, refuses to overwrite
published releases, and allows updating an existing draft. Do not move a published tag.

## Before public distribution

1. Review all seven green CI jobs and the draft artifacts.
2. Decide whether to add Developer ID signing/notarization for macOS. It is not configured.
3. Verify the repository visibility, README status, and release metadata.
4. Publish the reviewed draft in GitHub Releases.
5. Run **Update grip** in `homebrew-tap`, or wait for its daily scheduled run.

The tap updater only follows stable **published** releases. It verifies the formula and
all referenced archives against the release checksums, then commits just `Formula/grip.rb`.
It leaves other tools' formulae untouched. The next `brew update` makes the new version
available via `brew install karimz1/tap/grip` on Linux or macOS, Intel or ARM.

## Backend API references

- [Linux proc](https://www.kernel.org/doc/html/latest/filesystems/proc.html)
- [Apple libproc](https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.h)
- [Apple process structures](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info.h)
- [Windows Restart Manager registration](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmregisterresources)
- [Windows resource users](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist)
- [Windows Toolhelp snapshots](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot)
- [Homebrew taps](https://docs.brew.sh/Taps)
