# Rust redesign and migration plan

Status: implementation reference, originally proposed on 2026-09-24. The Rust workspace and native release pipeline are implemented on `architecture/redesign-in-rust`. The sections below preserve the design rationale; see `development.md` for current commands and `rust-validation.md` for measured results and remaining validation scope.

## Objective and scope

Replace the Go application with a Rust executable named `oflh`, preserving its terminal interface, CLI behavior, supported platforms, and evidence semantics. Improve scan efficiency, search latency, idle work, and resource ownership. Keep Linux, macOS, and Windows support on x86-64 and ARM64.

The existing program already uses native interfaces. Rust alone does not guarantee faster scans: OS calls, filesystem traversal, permissions, and process churn can dominate. Improvements must be measured against equivalent results and visibility.

The application and UI will be Rust. Calling platform libraries remains necessary. Port Python release helpers to a Rust `xtask` before final cutover to make project-owned operational tooling Rust too. Retain the small C lock fixture as an independent interoperability test, not an application dependency; replacing it with the scanner's own Rust wrappers would weaken independent validation.

## Findings from the current implementation

| Area | Existing implementation | Redesign opportunity |
| --- | --- | --- |
| UI | Bubble Tea, Bubbles, Lip Gloss; separate scanner interface | Recreate the same screens with Ratatui and Crossterm; preserve state transitions and shortcuts |
| Linux | `/proc` enumeration; `fdinfo` read before target matching; metadata inspected for each process; pidfd termination | Reject unrelated directory paths earlier, defer enrichment, reuse parsing buffers, and benchmark bounded process concurrency |
| macOS | Native libproc through purego; manually decoded byte offsets | Typed ABI bindings behind a small audited boundary; reusable enumeration buffers and delayed enrichment |
| Windows | Toolhelp, Restart Manager batches of 128 followed by recursive correlation; 10,000-file directory cap | Measure API/session counts, deduplicate metadata queries, and tune traversal/correlation without changing attribution |
| Search | Repeated lowercasing, rune conversion, temporary fields, and per-character boolean buffers; relevance computed during sorting | Compile queries once, index searchable text once per snapshot, reuse scratch buffers, calculate ranking once per result |
| Refresh | Full scan for initial CPU follow-up; 120 ms pulse scheduled continuously | Separate metrics sampling from discovery; animate only while busy and redraw on changes |
| Safety | Process lifetime identity, default-cancel confirmation, terminal sanitization, explicit incomplete-result warnings | Preserve all contracts; strengthen resource ownership and typed evidence |

Relevant sources: `internal/scanner/{linux,darwin,windows}.go`, `internal/model/{model,search,path}.go`, `internal/tui/{app,dashboard,styles,view}.go`, native integration tests, README, and CI workflows. The included thumbnail was visually inspected; a live terminal comparison remains part of implementation validation.

## Proposed architecture

Use a modest Cargo workspace with four crates, plus `xtask`:

```text
crates/oflh-core/       identities, targets, evidence, snapshots, search
crates/oflh-platform/   Linux/macOS/Windows discovery, metrics, process actions
crates/oflh-tui/        app state, input, rendering, theme, dialogs
crates/oflh/            CLI and application composition
xtask/                 release validation, packaging, checksums
```

Dependencies flow from CLI to TUI/platform and from those crates to core. Core must not depend on terminal or OS APIs. Use target-specific dependencies for native bindings. Select compatible stable dependency versions and a minimum Rust version during scaffolding, then commit `Cargo.lock`.

Recommended foundations:

- Ratatui with Crossterm for Rust terminal rendering and input. Ratatui renders into a cell buffer and emits differences; rendering still requires deliberate application scheduling. [Rendering documentation](https://ratatui.rs/concepts/rendering/under-the-hood/)
- `rustix` for supported Unix operations, including Linux pidfds; narrow `libc`/libproc bindings for missing macOS operations. [pidfd API](https://docs.rs/rustix/latest/src/rustix/process/pidfd.rs.html)
- Microsoft `windows-sys` for selected Windows APIs, wrapped with owned handles and typed error handling. [Microsoft Rust for Windows documentation](https://learn.microsoft.com/en-us/windows/dev-environment/rust/rust-for-windows)
- Standard threads and bounded channels initially. The workload is largely blocking OS inspection; introduce an async runtime only if measurements and architecture justify it.

One UI thread owns mutable app state. A scan coordinator owns scan generations and dispatches bounded work. Workers return observations; a single collector assembles immutable snapshots. Keep the previous complete snapshot visible during refresh, then replace it atomically. Coalesce repeated requests, reject stale generations, and retain cancellation checks between OS operations. Blocking native calls may not be cancellable; never promise hard cancellation deadlines without an isolated helper process.

Separate backend operations for discovery, sampling known process identities, and termination. Sample CPU after roughly one second without repeating every descriptor/module scan. Preserve the definition of CPU as a share of total machine capacity, the five-second refresh option, and unavailable metric states. Metrics updates must match both process identity and snapshot generation.

## Data and safety model

- A process identity contains PID plus an OS-specific birth token, not a formatted string key.
- Model access modes, usage relations, and lock evidence as enums/flags. Distinguish observed file use, confirmed lock evidence, and Windows sharing conflicts with unverified ownership.
- Store optional metrics explicitly. Missing CPU or memory is never represented as zero.
- Retain native paths as `PathBuf`/`OsString`; create separate sanitized display/search text. Do not force lossy UTF-8 into filesystem operations.
- Use file identity where available: device/inode on Unix and native volume/file identity on Windows. Preserve hard links, symlink resolution, nonexistent targets, deleted files, replacement races, and directory boundary behavior.
- Use `OwnedFd` and RAII handle wrappers. Confine `unsafe` to platform bindings with documented invariants and ABI checks on both architectures. Forbid unsafe application logic in core and UI.
- Preserve protection of PID 0/1, self, and targets without a valid lifetime identity. Revalidate captured identities for actions, including ancestors and hidden selections.
- Linux termination retains pidfds; Windows force termination uses a validated process handle. macOS start-time validation before signaling still has a residual OS-level race; Rust cannot eliminate it. Graceful Windows window messaging likewise needs careful lifetime checks.
- Keep control/escape/bidirectional-character sanitization, Unicode-aware cell widths, and terminal restoration on normal exit, errors, and unwinding panics.

## Platform optimization strategy

### Linux

Keep `/proc` as the baseline: it already provides the required native observations. For directory targets, filter unrelated descriptor paths before reading `fdinfo` and resolving extra metadata. For a single file, preserve identity matching so hard-link aliases are not discarded by an incorrect path-only prefilter. Parse byte slices, reuse per-worker buffers, and avoid repeated `stat` parsing. Resolve usernames only for matched processes, with a bounded cache. Consider directory-relative operations where they simplify repeated lookups. Tune worker count against syscall volume and contention.

Preserve FLOCK/POSIX/OFD evidence, exclusion of waiters and leases, deleted-file handling, and mount-namespace warnings. Do not add an eBPF daemon or elevated runtime requirement to the baseline rewrite.

### macOS

Keep libproc enumeration, vnode descriptors, CWD/executable/mapping discovery, resource sampling, and `F_GETLK` evidence. Use bindings checked against the SDK rather than scattered hard-coded offsets. Bound buffer growth and validate native return lengths. Cache user resolution and repeated per-scan process lookups. Query each distinct candidate file once, retaining opened-file identity validation.

Maintain the current limitations: first POSIX conflict per readable file, potentially missing flock-only locks and additional ranges. Do not imply broader coverage merely because the backend is Rust.

### Windows

First establish parity with documented Restart Manager and Toolhelp APIs. Instrument session creation, registration, recursive correlation, process-handle opens, and account resolution separately. Cache immutable metadata by process lifetime within a scan; stream directory candidates through bounded batches. Preserve the current cap and explicit warning initially. Revisit configurable limits only after memory and latency tests.

Do not blindly parallelize Restart Manager sessions. A system-handle enumeration backend is a separate later experiment: evaluate API stability, privileges, cross-architecture behavior, potentially blocking object queries, and whether it actually improves complete scan latency. It must not block the portable rewrite or invent lock owners. Preserve existing read/write/delete conflict semantics and graceful-close behavior.

## UI preservation

Recreate Processes, Locked files, process details, ancestry inspector, help, confirmation, empty/error/loading states, and compact layouts. Preserve all current key bindings, default focus, filter inheritance, multi-selection across filtering, ancestor action scope, sorting, path paging, and refresh behavior.

Copy the existing design tokens, including accent `#A78BFA`, selected background `#6D28D9`, read `#9BC5A1`, write `#D9B77D`, and lock `#E58C8C`. Preserve terminal-controlled backgrounds and readable text labels. Match the existing column priorities and responsive layout thresholds before making visual refinements.

Keep cached view indices rather than cloned process records; recompute filtering/ranking only when the query, sort, or relevant snapshot data changes. Render visible rows. Schedule spinner updates only while busy. Compare deterministic terminal-cell snapshots at 80×24, 120×40, 160×50, and very small dimensions, plus Unicode/long paths and confirmation dialogs. Follow with real terminal verification on each OS; exact glyph appearance depends on the terminal and font.

## Implementation sequence and exit gates

1. **Freeze the behavioral and performance baseline.** Run current tests, inventory CLI/keyboard behavior, capture reproducible UI fixtures, add comparable scanner/search/render benchmarks and stage timing. Exit: a written parity matrix and repeatable baseline measurements.
2. **Build core and Rust UI shell.** Scaffold workspace and platform interface, port path/search/evidence semantics, and reproduce every screen with deterministic fake data and mocked process actions. Exit: core parity tests and UI snapshots pass; no live scanner required for visual review.
3. **Implement Linux end to end.** Port scanner and action contracts first, then optimize measured hotspots. Exit: native lock, identity, CPU, ancestry, cancellation, and termination tests pass; side-by-side correctness and performance report produced.
4. **Implement macOS and Windows.** Port native bindings and feature contracts, optimize each independently, and run on actual x86-64/ARM64 OS runners. Exit: all six native platform jobs pass. Cross-compilation alone does not satisfy this gate.
5. **Integrate and tune.** Exercise real terminal startup, input, resizing, search, refresh, details, and mocked bulk actions. Add malformed parser/property tests and fuzzing for parsers/search. Audit unsafe boundaries, resource leaks, stale results, and terminal restoration. Exit: agreed latency/memory budgets met without coverage regressions.
6. **Cut over releases.** Port release helper behavior to `xtask`, update CI/docs/Homebrew and checksums, publish only when requested, and remove Go after parity and measured results are accepted. Keep the preceding Go release available for rollback. Exit: standalone Rust artifacts for all six targets and installation/package smoke checks.

During development keep Go as the behavioral reference and build Rust as `oflh-rs`; final public executable stays `oflh`. Do not maintain a Go runtime component or bridge in the final app.

## Measurement and completion criteria

Compare optimized release builds on the same hosts, privileges, fixture data, and terminal dimensions. Record time to first frame, time to complete usable results, full refresh median/p95, input-to-frame p95, CPU and peak/steady RSS, allocation profiles, syscall/API counts, and executable size. Separate cold and warm runs; compare only equal coverage and equivalent warnings. Include file targets, directories, high descriptor counts, heavy mappings, dense/sparse Windows resource users, and 1k/10k/100k synthetic usage sets for search.

Provisional UI goals are first frame under 100 ms and ordinary input-to-frame p95 under 16 ms on declared reference hardware, with no recurring animation work when idle. Establish scanner/memory improvement budgets from the baseline rather than promising an arbitrary multiplier. A faster result obtained by silently dropping inspection coverage is a failure.

Completion requires behavioral parity, native six-target validation, reviewed unsafe boundaries, close UI similarity, and documented before/after performance. If Rust does not improve a major workload, investigate the cause and report it explicitly before calling the performance objective complete.

## Current validation status

The Rust application, native backends, terminal tests, and six-target release
pipeline are implemented. The former Go code is retained in Git history at
`65556d75d951ab5dd74145d9707058020969c48e`. See [validation report](rust-validation.md).
The exploratory benchmark/fuzzing ideas above are not all completed release gates;
only checks and measurements explicitly recorded in that report have been run.
