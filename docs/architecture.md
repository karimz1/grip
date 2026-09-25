# Architecture

`oflh` is a Cargo workspace with domain logic independent of the terminal and
native scanner implementations. The CLI constructs a backend and passes it to the
TUI through the `Backend` trait.

| Crate | Responsibility | Depends on |
| --- | --- | --- |
| `oflh-core` | Targets, process identities, observations, search, domain errors | Standard library and small support libraries |
| `oflh-platform` | Native discovery, resource sampling, termination | `oflh-core`, target-specific OS bindings |
| `oflh-tui` | Input, application state, rendering, background work | `oflh-core`, `Backend`, Ratatui/Crossterm |
| `oflh` | Arguments, terminal requirements, application composition | Core, platform, TUI |
| `xtask` | Validation and release packaging | Independent developer executable |

## Data flow

The UI draws its initial screen before submitting a scan. A worker owns the native
backend and sends snapshots to the event loop. The UI owns selection, filters,
focus, and rendering; the backend does not receive terminal state.

Each request carries a generation number. Refresh cancels obsolete work, and the
UI ignores results from older generations. A single pending-work slot coalesces
repeated requests; the event channel has a fixed capacity. Cancellation is
cooperative between native calls and cannot interrupt every OS operation.

A follow-up metrics request samples CPU and memory without repeating file discovery.
Idle screens redraw only on changes. Search fields belong to a snapshot, queries
are compiled when edited, and matching reuses scratch buffers.

## Process actions

A process identity combines its PID with its birth time. Actions revalidate that
identity before signaling; PID reuse must never redirect a captured action.
Selections and focused ancestry nodes retain identities across refreshes.
Confirmation defaults to Cancel and includes selected processes hidden by filters.

Linux uses an owned pidfd, and Windows force termination uses a validated owned
process handle. macOS checks process start time before signaling; its public APIs
do not provide a pidfd equivalent, leaving a narrow exit/reuse race. Windows normal
termination posts a close request to process windows and does not silently escalate
to force termination.

## Native boundaries

Platform-specific code lives in `oflh-platform`. Unsafe calls require documented
buffer, layout, and lifetime invariants. Native records are typed, returned lengths
are checked, and handles/descriptors are released through RAII. Core, TUI, and CLI
forbid unsafe code.

| Platform | Discovery | Lock evidence |
| --- | --- | --- |
| Linux | procfs descriptors, working directories, executables, mappings | Held FLOCK, POSIX, and OFD records |
| macOS | libproc vnode descriptors, working directories, executables, mappings | First accessible POSIX conflict from `F_GETLK` |
| Windows | Restart Manager users and Toolhelp modules/executables | Read/write/delete sharing conflicts; ownership unverified |

Open-file observations do not prove a lock. Unknown metrics stay unknown, and
permission or visibility limits produce partial-result warnings. See
[Platform behavior](../README.md#platform-behavior) for user-facing details.

## Errors and cleanup

Library operations return `oflh-core::Error`, retaining the operation and original
OS error where available. CLI errors distinguish usage errors from runtime failures.
The UI displays recoverable errors without discarding process-identity checks.

Terminal restoration and native resources use drop guards. Release builds retain
unwinding so guards can run after a panic. Unit tests, integration fixtures,
benchmarks, and developer tools are separate from the installed executable.

## Native API references

- [Linux procfs](https://www.kernel.org/doc/html/latest/filesystems/proc.html)
- [Apple libproc](https://github.com/apple-oss-distributions/xnu/blob/main/libsyscall/wrappers/libproc/libproc.h)
- [Apple process records](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info.h)
- [Windows Restart Manager](https://learn.microsoft.com/en-us/windows/win32/rstmgr/restart-manager-portal)
- [Windows Toolhelp snapshots](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot)
