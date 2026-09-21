# grip

**See what's using your files.**

grip is a cross-platform terminal UI for finding which processes are using files,
directories, DLLs, and other open file handles.

Inspect why a process matched, search its usage, and request a graceful shutdown or
force kill from the same interface.

[![asciicast](https://asciinema.org/a/1265853.svg)](https://asciinema.org/a/1265853)

```sh
grip .
```

Point grip at a directory to inspect its descendants, or at a specific file:

```sh
grip ./build
grip ./build/plugin.dll
grip "/path/with spaces"
```

With no argument, grip uses the current directory.

## Install

### Homebrew

Recommended on Linux and macOS:

```sh
brew install karimz1/tap/grip
```

Update later with:

```sh
brew upgrade grip
```

The fully qualified name is required because Homebrew already has an unrelated package
named `grip`.

### Windows and release binaries

Download the latest archive from
[Releases](https://github.com/karimz1/grip/releases).

On Windows, extract `grip.exe` and place it on your `PATH`.

Prebuilt binaries are available for:

| OS | Architectures | Archive |
| --- | --- | --- |
| Linux | x86-64, ARM64 | `.tar.gz` |
| macOS | Intel, Apple Silicon | `.tar.gz` |
| Windows | x86-64, ARM64, x86 (32-bit) | `.zip` |

Each release includes `checksums.txt` for SHA-256 verification.

### Build from source

Requires Go 1.26 or newer.

```sh
go build -o bin/grip ./cmd/grip
./bin/grip .
```

On Windows:

```powershell
go build -o bin/grip.exe ./cmd/grip
.\bin\grip.exe .
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

## Platform support

| Platform | Discovery | Normal termination |
| --- | --- | --- |
| Linux | `/proc`: file descriptors, CWD, executable, mapped files, deleted-but-open files | `SIGTERM` with `pidfd` identity validation |
| macOS | `libproc`: vnode descriptors, CWD, executable, mapped files | `SIGTERM` after start-time validation |
| Windows | Restart Manager plus Toolhelp modules and executables | `WM_CLOSE` for process windows |

On Windows, console and service processes may require an explicit force kill.

## FAQ

### Why does grip exist?

Finding the process using a file often requires different tools on different operating
systems.

grip provides one interactive workflow:

**file → process → inspect → act**

Instead of starting with a PID or combining commands such as `lsof`, `fuser`, and `ps`,
you start with the file or directory you care about.

### Does grip unlock files?

Not directly.

grip finds processes that are using a file or directory. You can then inspect or terminate
those processes.

Whether that makes the file available depends on what the process was doing and how the
operating system handles it.

### Do I need root or administrator privileges?

Usually not.

Permissions determine which processes and resources grip can inspect or terminate. Some
processes may not be fully visible without elevated privileges.

### Is grip stable?

grip is currently in beta.

I primarily test it on Linux, so feedback from macOS and Windows users is especially useful.

If grip misses a process, behaves unexpectedly, or something in the UI is unclear, please
[open an issue](https://github.com/karimz1/grip/issues).

If you find grip useful, a GitHub star is appreciated.

## License

[MIT](LICENSE)