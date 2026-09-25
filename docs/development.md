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

## Architecture

- `oflh-core`: native paths, process birth identities, observations, typed errors,
  compiled search queries, and reusable search scratch buffers.
- `oflh-platform`: Linux procfs/pidfd, macOS libproc/fcntl, and Windows Restart
  Manager/Toolhelp/owned handles. Platform-specific unsafe calls remain here.
- `oflh-tui`: Ratatui rendering, keyboard state, and a background worker with a
  bounded pending-work slot, cooperative cancellation, and generation checks.
- `oflh`: argument handling and application composition.
- `xtask`: developer validation, release packaging, checksums, and Homebrew output.

The UI renders before scanning completes. Idle screens do not redraw on a timer.
CPU follow-up sampling reads metrics without repeating file discovery. Queries
and search fields are compiled per edit/snapshot, with scratch buffers reused.

Actions use PID plus process birth identity. Refreshes cannot silently redirect an
action to a reused PID. Confirmation defaults to Cancel and exposes hidden
selections. A focused ancestry tree retains the identities originally displayed.
Native handles/descriptors use RAII, and terminal restoration survives unwinding.

## Validation

Native CI tests Linux, macOS, and Windows on both x86-64 and ARM64. Checks cover
real locks and release, sharing modes, cancellation, stale/protected identities,
resource sampling, parent termination, Unicode paths, and independent C fixtures.
Windows termination is asynchronous: tests wait for child exit before inspecting
file contents. Unix additionally covers mappings and working directories; Linux
covers deleted files and hard links.

A real PTY/ConPTY test exercises startup, input, resizing, and quit on each native
target. State tests cover confirmation and identity safety. Text golden snapshots
cover process, lock, detail, and compact screens. Review intentional changes before
updating them with `OFLH_UPDATE_SNAPSHOTS=1 cargo test -p oflh-tui golden_screens`.
These checks do not establish identical rendering in every terminal emulator.

Unit tests live in `#[cfg(test)]` modules; Cargo integration tests are separate
executables. Release packaging builds only `oflh`, so test harnesses, C fixtures,
benchmarks, and developer tooling are absent from the distributed application.

See [AGENTS.md](../AGENTS.md) for persistent coding requirements and
[Releasing](releasing.md) for release procedures.
