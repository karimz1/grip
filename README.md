# Open File Lock Handle (`oflh`)

**See what's using your files.**

Open File Lock Handle is a cross-platform terminal UI for finding which processes are using files,
directories, DLLs, and other open file handles.

Inspect why a process matched, search its usage, and request a graceful shutdown or
force kill from the same interface.

<p>
<a href="https://buymeacoffee.com/karimz1">
    <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me a Coffee" height="41" />
</a>
</p>

## Demo:

<a href="images/demo.gif">
    <img src="images/demo.gif" alt="dupster tui demo" width="100%">
</a>

View Demo in High Res: https://asciinema.org/a/1265936


```sh
oflh .
```


Point oflh at a directory to inspect its descendants, or at a specific file:

```sh
oflh ./build
oflh ./build/plugin.dll
oflh "/path/with spaces"
```

With no argument, oflh uses the current directory.

## Install

### Homebrew (recommended)

for Linux or macOS install use brew it is recommended so you get auto updates easily:

```sh
brew install karimz1/tap/oflh
```

Update later with:

```sh
brew upgrade oflh
```

### Windows and release binaries

Download the executable for your operating system and architecture from
[Releases](https://github.com/karimz1/open-file-lock-handle/releases).

On Windows, rename the downloaded executable to `oflh.exe` and place it on your `PATH`.
On Linux and macOS, rename it to `oflh`, run `chmod +x oflh`, and move it to a directory on your `PATH`.

Prebuilt binaries are available for:

| OS | Architectures | Download |
| --- | --- | --- |
| Linux | x86-64, ARM64 | Standalone executable |
| macOS | Intel, Apple Silicon | Standalone executable |
| Windows | x86-64, ARM64 | `.exe` |

New releases contain six executables and `checksums.txt` for SHA-256 verification.
GitHub also provides its standard source-code downloads. Older releases retain their original assets.

### Build from source

Requires Go 1.26 or newer.

```sh
go build -o bin/oflh ./cmd/oflh
./bin/oflh .
```

On Windows:

```powershell
go build -o bin/oflh.exe ./cmd/oflh
.\bin\oflh.exe .
```

Contributor documentation:

- [Development](docs/development.md)
- [Releasing](docs/releasing.md)

## Controls

| Key | Action |
| --- | --- |
| `1` / `2` | Switch Processes / Locked files tabs |
| `↑` / `↓`, `j` | Navigate |
| `Enter` | Inspect a process |
| `/` | Search processes or usage |
| `Space` | Select a process |
| `Ctrl+A` | Select / deselect all visible processes |
| `k` | Request normal termination of selection, or current process |
| `x` | Force kill selection, or current process |
| `K` / `X` | Act on selected processes |
| `r` | Refresh |
| `a` | Toggle five-second auto-refresh |
| `i` | Toggle process side panel on wide terminals |
| `Tab` / `→` | Focus the tree; `↑` / `↓` choose a process, `k` / `x` stop it |
| `Tab` / `←` / `Esc` | Return from the tree to the results |
| `m` / `c` | Sort by RAM / CPU, highest first |
| `n` / `p` | Sort by name / PID |
| `Esc` | Clear search, cancel, or go back |
| `?` | Help |
| `q` / `Ctrl+C` | Back / quit |

The **Locked files** tab shows platform-specific evidence with the file path and
associated process. Press `Enter` for full paths and lock details. Ordinary open
files are excluded unless there is additional lock or sharing-conflict evidence.

| Platform | Lock evidence | Limits |
| --- | --- | --- |
| Linux | Held FLOCK, POSIX and OFD locks from `/proc/PID/fdinfo` | Permissions, namespaces and scan timing limit visibility |
| macOS | Existing POSIX byte-range locks queried with `F_GETLK` | First conflicting range per readable file; flock-only locks and additional ranges may be missed |
| Windows | Confirmed read/write/delete sharing conflicts, correlated with Restart Manager resource users | Users are explicitly labeled **owner unverified**; byte-range locks are not enumerated |

Windows entries confirm a sharing conflict on the file, not which reported user
imposed it. Permission-denied errors alone are never classified as locks. Lock
queries do not modify file contents. Advisory locks need not prevent ordinary
read/write access.

In the process details view, `/` searches filenames, paths, DLLs, relations, and access
modes. Press `l` in details to toggle **Locks only** for that process without
clearing the search. The summary counts distinct locked paths among the displayed
usages, and confirmed `locked` rows use muted red text. Press `l` again for all usages.
Search boxes are always visible; press `/` to type and see results live.
The details table expands its filename column for long names and repeats the
selected filename above the table. The full path stays below it; use `←` / `→`
to page through paths longer than the preview.

Plain terms match contiguous fragments or word/CamelCase prefixes, inspired by
[JetBrains CamelHumps](https://www.jetbrains.com/help/rider/Navigation_and_Search__CamelHumps.html).
For example, `dll` matches `.dll` without picking scattered letters out of a long
directory path, and `MIMJWT` matches `Microsoft.IdentityModel.JsonWebTokens.dll`.
Filename and process-name matches rank above directory-only matches; explicit
PID, name, CPU or RAM sorting still takes precedence. Arbitrary letter skipping
is no longer used. Terms containing `*` match literal chunks in
order, ignoring case: `micro*dll` finds `Microsoft.Core.dll`, and `*.dll` finds
paths containing `.dll`. `*` matches zero or more characters, including path
separators; patterns can match anywhere in a field. Spaces combine terms, for
example `micro*dll mapped`. This works in Processes, Locked files and details.

File-related search terms carry into process details automatically. The matched
path and `+N` count reflect matching usages; clear the details search with `Esc`
to see all usages again. Process-only terms, such as the PID, stay in the main
search. Selections survive filtering; the confirmation lists all selected targets,
including those currently hidden. `Ctrl+A` toggles only the visible results.

Wide terminals show a process inspector beside the table. Linux, macOS and Windows
collect up to eight observed ancestors and resident memory (RSS / working set). CPU usage becomes
available after two scans of the same process; a follow-up scan runs automatically
about one second after the initial results. It measures its share of total
machine CPU capacity over that interval (100% means all CPUs). Enable `a` for
regular updates. Missing metrics show `—` when permissions or process exit prevent
inspection. The panel hides automatically on compact
terminals; process details retain resource and parent information.

Tab focuses the existing nested ancestry tree, initially highlighting the current
process. Up moves toward its parents; Down returns toward the current process.
Actions apply only to the highlighted tree process, even if other processes are
selected in the main list. The confirmation names that exact process.
The selected ancestry snapshot remains fixed across refreshes; its captured
process identity is revalidated before termination. Protected processes and
ancestors without an available identity cannot be stopped. Parent termination
can close its application and affect child processes; it does not recursively
send termination to the whole tree. A successful termination request releases tree
focus and clears its captured snapshot before refreshing the results.

Every termination requires confirmation, with **Cancel selected by default**.

Only `1` and `2` switch views. Tab changes table/tree focus, or chooses the action
in confirmation dialogs. While typing a search it stays in the search input.

The Processes **ACCESS** column summarizes observed access across the matching
usages (and follows the search filter). `cwd` means the process has the folder as
its current working directory; it does not imply read or write access. `read/write`
means both modes were observed, possibly on different matching files. These are
access modes, not a measurement of current I/O activity.

Access labels use restrained colors: green for read, amber for write, cyan for
mapped/executable references, and muted text for unknown access. Colors describe
observed usage, not proof of a file lock. Labels remain readable without color.

## Platform support

| Platform | Discovery | Normal termination |
| --- | --- | --- |
| Linux | `/proc`: file descriptors, CWD, executable, mapped files, deleted-but-open files | `SIGTERM` with `pidfd` identity validation |
| macOS | `libproc`: vnode descriptors, CWD, executable, mapped files | `SIGTERM` after start-time validation |
| Windows | Restart Manager plus Toolhelp modules and executables | `WM_CLOSE` for process windows |

On Windows, console and service processes may require an explicit force kill.

## FAQ

### Why does oflh exist?

Finding the process using a file often requires different tools on different operating
systems.

oflh provides one interactive workflow:

**file → process → inspect → act**

Instead of starting with a PID or combining commands such as `lsof`, `fuser`, and `ps`,
you start with the file or directory you care about.

### Does oflh unlock files?

Not directly.

oflh finds processes that are using a file or directory. You can then inspect or terminate
those processes.

Whether that makes the file available depends on what the process was doing and how the
operating system handles it.

### Do I need root or administrator privileges?

Usually not.

Permissions determine which processes and resources oflh can inspect or terminate. Some
processes may not be fully visible without elevated privileges.

### Is oflh stable?

oflh is currently in beta.

I primarily test it on Linux, so feedback from macOS and Windows users is especially useful.

If oflh misses a process, behaves unexpectedly, or something in the UI is unclear, please
[open an issue](https://github.com/karimz1/open-file-lock-handle/issues).

If you find oflh useful, a GitHub star is appreciated.

## Does the tool have a new name ?

Yes. During early beta, the tool was named grip. However, starting with v0.0.5, it was officially renamed to Open File Lock Handle (oflh). So older release have the old name.

## License

[MIT](LICENSE)
