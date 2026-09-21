# Open File Lock Handle (`oflh`)

**See what's using your files.**

Open File Lock Handle is a cross-platform terminal UI for finding which processes are using files,
directories, DLLs, and other open file handles.

Inspect why a process matched, search its usage, and request a graceful shutdown or
force kill from the same interface.

Demo of the earlier `grip` interface (before the rename and visual refresh):

[![asciicast](https://asciinema.org/a/1265853.svg)](https://asciinema.org/a/1265853)

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

### Homebrew

Previously named **grip**. The command is now `oflh`. Until the first renamed
stable release is published, use `brew install --HEAD karimz1/tap/oflh`.

After the first renamed stable release, install on Linux or macOS with:

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
| `↑` / `↓`, `j` | Navigate |
| `Enter` | Inspect a process |
| `/` | Search processes or usage |
| `Space` | Select a process |
| `k` | Request normal termination |
| `x` | Force kill |
| `K` / `X` | Act on selected processes |
| `r` | Refresh |
| `Esc` | Clear search, cancel, or go back |
| `?` | Help |
| `q` / `Ctrl+C` | Back / quit |

In the process details view, `/` searches filenames, paths, DLLs, relations, and access
modes.

Every termination requires confirmation, with **Cancel selected by default**.

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

## License

[MIT](LICENSE)