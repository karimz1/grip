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

## Prepare a version

Update `workspace.package.version` in `Cargo.toml` and refresh `Cargo.lock` with
`cargo check`. Use semantic versions: `0.1.0-rc.1` for a candidate or `0.1.0` for a
stable release. Record user-visible changes in the release notes, not in temporary
README status messages.

Run `cargo xtask check`, review all six native CI jobs, and commit the version
change before tagging. The commands below use `v0.1.0` as an example; substitute
the intended version consistently.

## Package locally

```sh
cargo build --release --locked --bin oflh
cargo xtask package --version v0.1.0 --os linux --arch amd64 --binary target/release/oflh
```

Use the appropriate OS/architecture and `.exe` suffix on Windows. `package`
copies an existing executable; it does not change its embedded version. Check
`oflh --version` before packaging.

To assemble a complete release, download all six artifacts from the same passing
CI revision into `dist`, then run:

```sh
mkdir -p bin
cargo xtask assemble --version v0.1.0 --output dist --formula bin/oflh.rb
(cd dist && sha256sum --check checksums.txt)
```

The assembler rejects missing, empty, or unexpected artifacts and writes SHA-256
checksums and a Homebrew formula. Assembly is local; publication requires the
release workflow below. Formula URLs refer to the corresponding GitHub release.

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
`Formula/oflh.rb` locally. It leaves other tools' formulae untouched. The next `brew update` makes the new version
available via `brew install karimz1/tap/oflh` on Linux or macOS, Intel or ARM.
