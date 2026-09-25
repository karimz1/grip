# Contributing

Bug reports, documentation fixes, and focused code changes are welcome.

## Report a bug

Search [existing issues](https://github.com/karimz1/open-file-lock-handle/issues)
before opening a new one. Include:

- `oflh --version`, operating system, architecture, and terminal application.
- Steps to reproduce, expected behavior, and what happened instead.
- Relevant warnings shown by `oflh` and whether the target is a file or directory.

Use a minimal example with temporary files where possible. Redact usernames,
private paths, hostnames, process arguments, and credentials from screenshots or
logs. Do not post sensitive data in a public issue.

## Make a change

1. Follow [Development](docs/development.md) to build and run the project.
2. Keep the change focused and follow the boundaries in [Architecture](docs/architecture.md).
3. Add regression coverage for behavior changes. Include before/after screenshots
   for terminal layout changes, using synthetic process names and paths.
4. Run `cargo xtask check` and review your diff for generated files and sensitive data.
5. Open a pull request explaining the problem, resulting behavior, and validation.

Use standard Rust formatting and descriptive names. Prefer small functions,
typed errors, owned resources, and safe Rust. Document public APIs and explain
native ABI or lifetime assumptions beside each unsafe block. Do not weaken
identity checks or confirmation behavior to simplify a change.

Native changes need tests on the affected operating systems and architectures;
cross-compilation alone cannot validate operating-system behavior. Performance
changes should include a repeatable workload and equivalent inspection coverage;
see [Performance](docs/performance.md).
