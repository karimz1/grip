# grip

grip is a cross-platform terminal UI for finding which processes are using your files,
directories, DLLs, and other open file handles. It can also request graceful shutdowns
or force-kill selected processes. The project is in beta, but I already use it myself;
feel free to try it and share your feedback.

**See what's using your files.**

A terminal UI for finding and stopping processes that are using files and directories.
Find the DLL a process loaded, the file a build tool left open, or the working directory
a shell is holding—all from one place.

[![asciicast](https://asciinema.org/a/1265853.svg)](https://asciinema.org/a/1265853)

```sh
grip .
```

Point it at a directory to inspect its descendants, or at one file:

```sh
grip ./build
grip ./build/plugin.dll
grip "/path/with spaces"
```

No argument means the current directory. An interactive terminal is required.

## Install

### Homebrew (recommended for Linux and macOS)

```sh
brew install karimz1/tap/grip
```

Homebrew manages the installation and updates. To update grip later, run:

```sh
brew upgrade grip
```

Use the fully qualified name because Homebrew already has an unrelated package called `grip`.
The [shared tap](https://github.com/karimz1/homebrew-tap) can hold additional tools.

### Windows and prebuilt release binaries

On Windows, download the latest archive from [Releases](https://github.com/karimz1/grip/releases).
Extract `grip.exe` and place it on your `PATH`. Prebuilt archives are also available for
Linux and macOS:

| OS | Architectures | Archive |
| --- | --- | --- |
| Linux | x86-64, ARM64 | `.tar.gz` |
| macOS | Intel, Apple Silicon | `.tar.gz` |
| Windows | x86-64, ARM64, x86 (32-bit) | `.zip` |

Each release includes `checksums.txt`;
compare the download's SHA-256 before installing. Use `sha256sum` on Linux,
`shasum -a 256` on macOS, or `Get-FileHash -Algorithm SHA256` in PowerShell.

### Build from source

Build from source if you are developing grip or need a custom build. You need Go 1.26
or newer, and you will need to build and update the binary yourself.

```sh
go build -o bin/grip ./cmd/grip
./bin/grip .
```

On Windows use `go build -o bin/grip.exe ./cmd/grip` and `./bin/grip.exe .`.

Release automation and contributor setup are documented in [Releasing](docs/releasing.md)
and [Development](docs/development.md).

## Controls

| Key | Action |
| --- | --- |
| `↑` / `↓`, `j` | Navigate |
| `Enter` | Inspect a process |
| `/` | Fuzzy search the process list or its usage table |
| `Space` | Select a process |
| `k` / `x` | Request termination / force kill |
| `K` / `X` | Act on selected processes; if none are selected, all filtered processes |
| `r` | Refresh |
| `Esc` | Cancel editing, clear a search, or go back |
| `?` | Help |
| `q` / `Ctrl+C` | Back / quit |

In details, search for a DLL, filename, path, relation, or access mode. Matching is
case-insensitive and accepts abbreviated text. Use arrow keys to browse results while
typing; `Enter` applies the search. `←` / `→` reveal more of a long selected path.
The `+88` label in the process list means **88 more usage observations**, not necessarily
88 distinct files. Details show all of them in a searchable table.

Every termination requires confirmation, with Cancel selected by default. `Tab` changes
the choice and `Enter` confirms. Filtering details does not change the target of a kill:
the action always applies to the whole process. Selections hidden by a process filter
are still included in the bulk confirmation.

## Platform support

| Platform | Discovery | Normal termination |
| --- | --- | --- |
| Linux | Native `/proc`: descriptors, CWD, executable, mapped files, deleted-but-open files | `SIGTERM`, with `pidfd` identity validation |
| macOS | Native `libproc`: vnode descriptors, CWD, executable, mapped files | `SIGTERM`, after start-time validation |
| Windows | Native Restart Manager resource correlation and Toolhelp loaded modules/executables | `WM_CLOSE` to process windows; console/service processes may require an explicit force kill |


## License

[MIT](LICENSE).
