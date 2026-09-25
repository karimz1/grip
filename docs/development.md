# Rust development

Install Rust through rustup; `rust-toolchain.toml` pins the toolchain used by CI.
Run the repository gates with:

```sh
cargo xtask check
cargo build --release --locked --bin oflh
./target/release/oflh .
```

`check` runs standard formatting, Clippy with warnings denied, and workspace tests.
Native integration tests also require a C compiler (Clang/GCC on Unix, MSVC or
MinGW on Windows). The independent fixture in
`crates/oflh-platform/tests/fixtures/lock-fixture.c` exercises interoperability
with native locks. It is compiled into a temporary test directory only.

## Working on the code

See [Architecture](architecture.md) for crate responsibilities, data flow, and
native safety boundaries. Run a focused test while developing, then the full
validation command before submitting a pull request:

```sh
cargo test -p oflh-core
cargo test -p oflh-platform --test native
cargo test -p oflh-tui
cargo test -p oflh --test terminal
cargo xtask check
```

On Windows, run from a developer shell with the native compiler available. On
macOS, install Xcode Command Line Tools. On Linux, install a C compiler and the
usual linker/build tools from your distribution.

## Validation

Native CI tests Linux, macOS, and Windows on both x86-64 and ARM64. Checks cover
real locks and release, sharing modes, cancellation, stale/protected identities,
resource sampling, parent termination, Unicode paths, and independent C fixtures.
Unix additionally covers mappings and working directories; Linux
covers deleted files and hard links.

A real PTY/ConPTY test exercises startup, input, resizing, and quit on each native
target. State tests cover confirmation and identity safety. Text golden snapshots
cover process, lock, detail, and compact screens. Review intentional changes before
updating them with `OFLH_UPDATE_SNAPSHOTS=1 cargo test -p oflh-tui golden_screens`.
These checks do not establish identical rendering in every terminal emulator.

Unit tests live in `#[cfg(test)]` modules; Cargo integration tests are separate
executables. Release packaging builds only `oflh`, so test harnesses, C fixtures,
benchmarks, and developer tooling are absent from the distributed application.

## Documentation and performance

Build API documentation with `cargo doc --workspace --no-deps`. Public APIs should
explain their contract; native wrappers should document buffer and lifetime rules.
Use [Performance](performance.md) when measuring a scanner or startup change.

For pull-request expectations, see [Contributing](../CONTRIBUTING.md). Release
maintainers should follow [Releasing](releasing.md).
