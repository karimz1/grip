<p align="center">
  <img src="images/oflh-logo.svg" alt="Open File Lock Handle (oflh)" width="760">
</p>

<a id="oflh--find-locked-files-and-the-processes-using-them"></a>

# oflh — Find which process is using a file

[![Platforms](https://img.shields.io/badge/platforms-Linux_%C2%B7_macOS_%C2%B7_Windows-64748b?style=flat)](#platform-behavior)
[![GitHub Downloads](https://img.shields.io/github/downloads/karimz1/open-file-lock-handle/total.svg)](https://github.com/karimz1/open-file-lock-handle/releases)
[![CI](https://github.com/karimz1/open-file-lock-handle/actions/workflows/ci.yml/badge.svg)](https://github.com/karimz1/open-file-lock-handle/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[![Listed on AwesomeTUI](https://img.shields.io/badge/AwesomeTUI-listed-64748b?style=flat)](https://awesometui.com/open-file-lock-handle)
[![Listed on AlternativeTo](https://img.shields.io/badge/AlternativeTo-listed-64748b?style=flat)](https://alternativeto.net/software/oflh-open-file-lock-handle/about/)

**Open File Lock Handle (`oflh`) finds processes using a file or directory on
Windows, Linux, and macOS.** It is a command-line tool with an interactive
terminal UI (TUI) for investigating locked files, open file handles, and mapped
files. Start with a path to see which processes reference it and what they have open.

```sh
oflh ./build             # Find processes using files in a directory
oflh ./build/plugin.dll  # Investigate a file or DLL in use
oflh .                   # Inspect the current directory
oflh --port 3000         # Find the process using a local port
```

[Install](#installation) · [Quick start](#getting-started) ·
[Keyboard shortcuts](#keyboard-reference) · [Platform support](#platform-behavior) ·
[User guide](docs/usage.md)

<a href="images/demo.gif">
  <img src="images/demo.gif" alt="oflh terminal UI showing processes using a target path, file access modes, and process ancestry" width="100%">
</a>

Demo with sample processes and paths.
[Watch the terminal recording](https://asciinema.org/a/HVwfkoVJfOi5ckDa).

## When to use oflh

- **A file is “in use by another process.”** Find processes referencing it before retrying a rename, move, or delete.
- **A build cannot replace a DLL or executable.** Inspect the output directory to find a running application or development tool still using its files.
- **You want to clean up a directory.** See which visible processes reference files beneath it, then inspect their file usages and parent processes.
- **You are troubleshooting a file lock.** Switch to the Locked files view to examine the lock or sharing-conflict evidence available on your OS.

An open file is **not necessarily locked**. `oflh` separates file usage from lock
evidence and does not directly unlock files. Stopping a process may release its
resources; it can also interrupt work in that application.

<a id="what-makes-it-different"></a>

## Why oflh?

Use it alongside `lsof`, `fuser`, or Task Manager when you want to start with a
path and investigate interactively, without knowing a process name or PID.

- **Search quickly:** filter process names and file paths with fragments, CamelCase abbreviations, and wildcards.
- **Inspect the context:** view individual file usages, access modes, parent processes, CPU, and memory.
- **Follow changes:** sort results, rescan on demand, or enable five-second auto-refresh.
- **Act from the same interface:** request termination with confirmation and process identity checks.

Written in Rust, `oflh` uses native OS interfaces. See [platform behavior](#platform-behavior)
for discovery coverage and lock-detection limits.

## Installation

### Homebrew

On macOS or Linux:

```sh
brew install karimz1/tap/oflh
```

To update:

```sh
brew update
brew upgrade oflh
```

### Standalone binaries

Download the executable for your operating system and CPU from the
[latest release](https://github.com/karimz1/open-file-lock-handle/releases/latest):

| Platform | x86-64 (Intel / AMD) | ARM64 |
| --- | --- | --- |
| Linux | `oflh-linux-amd64` | `oflh-linux-arm64` |
| macOS | `oflh-darwin-amd64` | `oflh-darwin-arm64` (Apple Silicon) |
| Windows | `oflh-windows-amd64.exe` | `oflh-windows-arm64.exe` |

Open a terminal in the download folder. On **Linux or macOS**, rename the downloaded
file to `oflh`, then make it executable and run it against the folder you want to inspect:

```sh
chmod +x ./oflh
./oflh "/path/to/project"
```

On **Windows**, rename the downloaded file to `oflh.exe` and run it in PowerShell:

```powershell
.\oflh.exe "C:\projects\example"
```

You can run it this way without changing `PATH`. To use the shorter `oflh` command
from any folder, put the executable in a directory listed in your `PATH` environment
variable. Otherwise, keep using its full path or `./oflh` (`.\oflh.exe` in PowerShell)
from the download folder.

Releases include `checksums.txt` for SHA-256 verification.

### Build from source

Install Rust with rustup. The repository pins its toolchain. From a checkout:

```sh
cargo build --release --locked --bin oflh
./target/release/oflh .
```

On Windows:

```powershell
cargo build --release --locked --bin oflh
.\target\release\oflh.exe .
```

## Getting started

Run `oflh [PATH]` in an interactive terminal. With no path, it inspects the current
directory. A directory target includes its descendants.

```sh
oflh
oflh "/path/with spaces"
oflh --help
oflh --version
```

Windows PowerShell example:

```powershell
oflh "C:\projects\example\build\plugin.dll"
```

1. Open a file or directory with `oflh` and select a process in **Processes** (`1`).
2. Press `/` to search and `Enter` to finish typing. For example, `dll` finds a fragment; `micro*dll` matches chunks in order.
3. Press `Enter` on a process to inspect its file usages, or `2` to open **Locked files**.
4. Close the application normally if possible. If needed, `k` requests termination and `x` requests force kill; both require confirmation.
5. Press `r` to rescan and check whether the file is still in use.

<a id="search"></a>

Use `?` for help. See the user guide for [search syntax](docs/usage.md#search),
[process actions](docs/usage.md#process-actions), and the
[full keyboard reference](docs/usage.md#keyboard-reference).

## Find processes using ports

Open **Ports** (`3`) to inspect local TCP listeners and bound UDP sockets. Search
with `/` for an exact port number, process name, protocol, or address. Press `s`
to switch between **ALL PORTS** and **THIS PATH**, which shows ports belonging to
processes observed using the target file or directory.

```sh
oflh --ports             # All visible local port bindings
oflh --port 3000         # Exact local port, TCP or UDP
oflh --here .            # Ports of processes referencing this folder
oflh --port 3000 --here . # Combine port and folder filters
```

`Enter` opens process port details; `f` switches between ports and file usages.
Normal termination and force kill use the same confirmation and identity checks
as file inspection. Unknown owners cannot be terminated. A bound port does not
prove that it is reachable over the network.

See [port search and scope](docs/usage.md#ports) for examples and
[platform limitations](docs/platform-support.md#ports).

## Keyboard reference

| Key | Action |
| --- | --- |
| `1` / `2` / `3` | Processes / Locked files / Ports |
| `s` in Ports | Switch all ports / this path |
| `f` in details | Switch file usages / ports |
| `↑` / `↓` | Move through results |
| `/` | Search |
| `Enter` | Finish search editing / open process details |
| `r` / `a` | Refresh / toggle five-second auto-refresh |
| `Space` | Select or deselect a process |
| `k` / `x` | Request termination / force kill |
| `Tab` | Switch focus between results and ancestry tree |
| `?` | Show help and scan limitations |
| `Esc` | Clear search, cancel, or go back |
| `q` / `Ctrl+C` | Back / quit |

<a id="process-actions"></a>

Process actions use the selection, or the current process if nothing is selected.
Selections survive filtering; confirmation lists hidden selections too, and
**Cancel** is the default. When the ancestry tree has focus, actions apply only
to the highlighted ancestor. See [process actions](docs/usage.md#process-actions)
before stopping a parent application.

<a id="file-discovery-and-termination"></a>
<a id="lock-evidence"></a>
<a id="access-modes"></a>
<a id="cpu-memory-and-ancestry"></a>

## Platform behavior

Binaries are available for **Linux, macOS, and Windows on x86-64 and ARM64**.
The interface is shared, but file discovery and lock detection depend on the OS.

| Platform | File usage discovery | Lock evidence |
| --- | --- | --- |
| Linux | Open descriptors, working directories, executables, mapped files, deleted-but-open files via `/proc` | Held FLOCK, POSIX, and OFD locks |
| macOS | Vnode descriptors, working directories, executables, mapped files via `libproc` | POSIX byte-range conflicts; flock-only locks may be missed |
| Windows | Restart Manager resource users, modules and executables via Toolhelp | Read, write, or delete sharing conflicts; reported owners are unverified |

On Windows, a sharing conflict does not prove which reported process caused it.
Discovery does not cover working directories, directory handles, or deleted files,
and byte-range locks are not enumerated.

Results are a snapshot limited by permissions, process exits, and concurrent file
activity. When the footer says **“Results may be incomplete,”** press `?` for
details. See [platform support and limitations](docs/platform-support.md) for the
full detection scope, termination behavior, and metric definitions.

## Common questions

### Can oflh unlock a file?

It can help you find and stop a process using the file. It does not remove locks
directly or bypass OS permissions. Close the application normally first when
possible; force killing can lose unsaved work.

### Why is a process listed but the Locked files view is empty?

File usage and file locks are different. A process can have an open descriptor
or mapped file without holding a detectable lock. The Locked files view requires
additional evidence, and each platform has detection limits.

### Why are some processes or locks missing?

Permissions and platform coverage limit what can be inspected. Elevated
privileges may improve visibility, but cannot guarantee complete results. On
Linux, inspecting another container may require running `oflh` inside its mount
namespace. Check `?` for scan warnings and the [platform reference](docs/platform-support.md).

### Does oflh support scripts or JSON output?

File inspection requires an interactive terminal; `oflh` currently has no JSON
or non-interactive scan output. `--help` and `--version` work without a TTY.

<a id="performance"></a>
<a id="testing-and-development"></a>
<a id="terminal-recommendation"></a>

## Documentation and development

- [User guide](docs/usage.md): search syntax, keyboard shortcuts, process actions, and terminal support.
- [Platform reference](docs/platform-support.md): discovery, lock evidence, and permissions.
- [Contributing](CONTRIBUTING.md) and [development](docs/development.md): build and test instructions.
- [Architecture](docs/architecture.md): native backends and safety boundaries.
- [Performance measurements](docs/performance.md): benchmark methodology, results, and the historical Go-to-Rust comparison.
- [Releasing](docs/releasing.md): maintainer packaging instructions.

Run `cargo xtask check` for formatting, Clippy, and workspace tests. CI covers
native Linux, macOS, and Windows on x86-64 and ARM64.

<a id="project-information"></a>

`oflh` is in beta. [Report a bug](https://github.com/karimz1/open-file-lock-handle/issues)
with the version, operating system, and steps to reproduce it.

Licensed under [MIT](LICENSE). [Support development](https://buymeacoffee.com/karimz1).
