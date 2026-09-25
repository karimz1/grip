# Instructions for agents working on oflh

## Rust quality

- Follow standard `rustfmt`; format edited Rust before presenting or committing it.
- Use descriptive `snake_case` names. Single letters are limited to conventional loop indices and short, unambiguous closure parameters.
- Keep functions focused. Extract parsing, native resource management, observation collection, and UI state transitions into named helpers rather than growing monolithic functions or nested iterator expressions.
- Document public types and APIs with `///`. Explain non-obvious search, identity, cancellation, and native ABI invariants.
- Preserve typed library errors using `thiserror`, operation context, and original OS error codes. Propagate failures with `?`.
- Do not use `unwrap()` or `expect()` in production code unless failure is statically impossible and the reason is documented. Tests may assert fixture assumptions with them.
- Optimize through measured reductions in system calls, allocations, copies, and idle work. Keep the code readable. Do not claim speedups without reproducible measurements and equivalent inspection coverage.

## Architecture and safety

- Keep domain/search logic in `oflh-core`, native APIs in `oflh-platform`, terminal state/rendering in `oflh-tui`, CLI composition in `oflh`, and developer/release tooling in `xtask`.
- Core, UI, and CLI must remain safe Rust. Confine unavoidable `unsafe` to native boundaries, with a `SAFETY` comment for each block and verified buffer lengths/layouts.
- Use owned descriptors/handles and RAII. Retain native paths losslessly; sanitize control and formatting characters only for display.
- Bind actions and metrics to PID plus birth identity. Never weaken stale-identity checks, protected-process guards, default-cancel confirmation, or hidden-selection disclosure.
- Open files do not prove locks. Preserve each platform's evidence limitations, unknown metrics, and partial-result warnings. Windows resource users are not proven lock owners.
- Keep scanning off the UI thread, queues bounded, cancellation cooperative, and stale generations rejected. Avoid periodic redraws while idle.
- Preserve the current keyboard workflow and responsive terminal appearance. A focused ancestry tree retains its captured identities across refreshes.

## Required validation

- Run `cargo xtask check` before a final implementation handoff. It runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`.
- Add meaningful regression tests for native behavior, state transitions, and previously failing cases. Place unit tests in `#[cfg(test)]` modules; standard Cargo integration tests under `tests/` are also separate test executables.
- Golden terminal snapshots must be reviewed when updated. Use `OFLH_UPDATE_SNAPSHOTS=1` only for intentional UI changes, and inspect the changed fixtures.
- Validate changes to platform backends on native Linux, macOS, and Windows runners for both x86-64 and ARM64. Cross-checks are useful but do not replace native execution. Report unverified targets explicitly.
- Retain the independent C lock fixture as test-only interoperability coverage. Never compile or link fixtures, benchmarks, developer tools, or dev-dependencies into the application binary.
- Build the distributed app with `cargo build --release --locked --bin oflh`; build/package the exact executable tested for its native target.
- When a check fails, investigate the cause. Do not disable tests or relax a safety contract merely to make CI green.

## Releases and task scope

- Keep release packaging and checksums in the Rust `xtask` workflow. Release automation must consume the six tested native artifacts, reject unexpected/missing artifacts, and refuse to overwrite a published release.
- Respect requested branch names and release scope. A request for a local release candidate does not authorize pushing a release tag or publishing a release.
- Preserve unrelated work. Describe changes, validation, measured performance, and remaining limitations honestly.

## Privacy

- Never commit credentials, tokens, private keys, personal contact details, private paths, or machine hostnames in code, fixtures, screenshots, logs, or benchmark data. Public project names and links may be retained.
- Use synthetic data in examples and test fixtures. Record only the environment details needed to reproduce a measurement.
- If sensitive information is found, report its category and location without echoing the value. Remove it from current files; do not rewrite shared history or rotate credentials without authorization.
