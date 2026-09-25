# Releases and the Homebrew tap

The public release workflow creates **drafts only** for review before publishing.

## What CI tests

Every push to `main` and every pull request runs the following matrix:

| Runner | Target |
| --- | --- |
| Ubuntu 24.04 x86-64 | linux/amd64 |
| Ubuntu 24.04 ARM64 | linux/arm64 |
| macOS 15 Intel | darwin/amd64 |
| macOS 15 Apple Silicon | darwin/arm64 |
| Windows 2025 x86-64 | windows/amd64 |
| Windows 11 ARM64 | windows/arm64 |

Every target runs formatting, Clippy, unit/native integration tests, and repeated
real terminal tests. It builds only the release `oflh` executable, checks its CLI,
and packages it with Rust `xtask`. Release jobs consume those artifacts.

Tests and developer tools are separate executables and dev-dependencies. The
application needs no Go or Python runtime. Release builds use thin LTO, one
codegen unit, stripped symbols, and unwinding for RAII terminal cleanup.

## Prepare a local candidate

The workspace currently identifies itself as `0.0.10-rc.1`.

```sh
cargo xtask check
cargo build --release --locked --bin oflh
cargo xtask package --version v0.0.10-rc.1 --os linux --arch amd64 --binary target/release/oflh
```

Use the corresponding OS/architecture and `.exe` suffix on Windows. To assemble
a complete candidate, download all six artifacts from the same green native CI
run into `dist`, then run:

```sh
cargo xtask assemble --version v0.0.10-rc.1 --output dist --formula bin/oflh-rc.rb
```

The assembler rejects missing, empty, or unexpected artifacts and writes SHA-256
checksums plus the candidate formula. Create `bin` first. Preparing locally does
not publish a release or push a tag. Candidate formula URLs become available only
if that release is subsequently published.

## Create a draft

From a reviewed commit:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The Release workflow reruns the full matrix for that tag and installs/tests the generated Homebrew formula on Linux and macOS. If any target fails, no draft
is created. A passing run uploads six raw executables and `checksums.txt` to the draft release. Version strings omit the leading `v`.

To retry an existing tag, use the workflow UI and select **that tag** as the workflow
ref, or run:

```sh
gh workflow run release.yml --ref v0.1.0 -f tag=v0.1.0
```

The workflow verifies that the tag resolves to the workflow commit, refuses to overwrite
published releases, and allows updating an existing draft. Do not move a published tag.

## Before public distribution

1. Review all six green CI jobs and the draft artifacts.
2. Decide whether to add Developer ID signing/notarization for macOS. It is not configured.
3. Verify the repository visibility, README status, and release metadata.
4. Publish the reviewed draft in GitHub Releases.
5. Publishing triggers **Update Homebrew tap**, which immediately dispatches **Update oflh** in `homebrew-tap`.

The tap updater only follows stable **published** releases. It verifies the four Unix executables against the release checksums and generates
`Formula/oflh.rb` locally. On the first renamed release, it also migrates the legacy
`grip` formula using `formula_renames.json`.
It leaves other tools' formulae untouched. The next `brew update` makes the new version
available via `brew install karimz1/tap/oflh` on Linux or macOS, Intel or ARM.

## Backend API references

- [Linux proc](https://www.kernel.org/doc/html/latest/filesystems/proc.html)
- [Apple libproc](https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.h)
- [Apple process structures](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info.h)
- [Windows Restart Manager registration](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmregisterresources)
- [Windows resource users](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist)
- [Windows Toolhelp snapshots](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot)
- [Homebrew taps](https://docs.brew.sh/Taps)
