# Platform behavior

[Back to the README](../README.md) · [User guide](usage.md)

The workflow is shared across platforms, but discovery and lock semantics depend
on the operating system. Results are a snapshot of what the current user can
inspect. Permissions, process exits, and concurrent file activity can limit them.
When a scan has limitations, the footer shows "Results may be incomplete."
Press `?` for the full scan details.

## File discovery and termination

| Platform | Discovery | Normal termination (`k`) |
| --- | --- | --- |
| Linux | `/proc`: file descriptors, CWD, executable, mapped files, deleted-but-open files | `SIGTERM` with `pidfd` identity validation |
| macOS | `libproc`: vnode descriptors, CWD, executable, mapped files | `SIGTERM` after start-time validation |
| Windows | Restart Manager, Toolhelp modules and executables | `WM_CLOSE` for process windows |

Windows console and service processes may require explicit force termination.
Windows discovery does not cover CWD, directory handles, or deleted files.
On Linux, other mount namespaces may require running `oflh` inside the relevant
container. Elevated privileges can improve visibility but do not remove every
platform limitation.

## Lock evidence

An open file is not necessarily locked. The Locked files view requires additional
evidence:

| Platform | Evidence | Scope |
| --- | --- | --- |
| Linux | Held FLOCK, POSIX, and OFD locks from `/proc/PID/fdinfo` | Subject to permissions, namespaces, and scan timing |
| macOS | POSIX byte-range conflicts queried with `F_GETLK` | First conflicting range per readable file; flock-only locks and additional ranges may be missed |
| Windows | Read, write, or delete sharing conflicts, correlated with Restart Manager resource users | Reported users are labeled **owner unverified**; byte-range locks are not enumerated |

On Windows, a sharing conflict confirms the file is restricted, but does not
prove which reported process imposed that restriction. Permission-denied errors
alone are never classified as locks. Lock queries do not modify file contents.
Advisory locks do not necessarily prevent ordinary reads or writes.

## Access modes

The **ACCESS** column summarizes the usages matching the current search.
`read/write` means both modes were observed, possibly on different files. `cwd`
means the process uses the folder as its working directory; it does not imply
read or write access. These labels describe observed access modes, not live I/O
activity or proof of a lock.

Read access uses green, write access uses amber, and confirmed locks use muted
red. Text labels carry the meaning without relying on color.

## CPU, memory, and ancestry

All three platforms collect resident memory and up to eight observed ancestors.
Memory is RSS on Unix and working set on Windows.

CPU measures a process's share of total machine capacity over the sampling
interval: 100% means all CPUs. It requires two samples of the same process. A
lightweight metrics sample runs about one second after the initial results;
use `a` for regular updates. Unavailable metrics appear as a dash, for example when
permissions or process exit prevent inspection.

