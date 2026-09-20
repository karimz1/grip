# grip

**Beta**

**See what's using your files.**

A terminal UI for finding and stopping processes that are using files and directories.
Find the DLL a process loaded, the file a build tool left open, or the working directory
a shell is holding—all from one place.

[![asciicast](https://asciinema.org/a/1265850.svg)](https://asciinema.org/a/1265850)

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

This project is in beta. Install from a published release or build from source.

### From source

Requires Go 1.26 or newer. The resulting binary has no external runtime requirements.

```sh
go build -o bin/grip ./cmd/grip
./bin/grip .
```

On Windows use `go build -o bin/grip.exe ./cmd/grip` and `./bin/grip.exe .`.

### Release binaries

The release pipeline produces executables and archives for these targets:

| OS | Architectures | Archive |
| --- | --- | --- |
| Linux | x86-64, ARM64 | `.tar.gz` |
| macOS | Intel, Apple Silicon | `.tar.gz` |
| Windows | x86-64, ARM64, x86 (32-bit) | `.zip` |

Download from [Releases](https://github.com/karimz1/grip/releases), extract the archive,
and put `grip` (or `grip.exe`) on your `PATH`. Each release includes `checksums.txt`;
compare the download's SHA-256 before installing. Use `sha256sum` on Linux,
`shasum -a 256` on macOS, or `Get-FileHash -Algorithm SHA256` in PowerShell.

### Homebrew

```sh
brew install karimz1/tap/grip
```

Use the fully qualified name: Homebrew already has an unrelated package called `grip`.
The [shared tap](https://github.com/karimz1/homebrew-tap) can hold additional tools.
Release automation and publishing details are described in [Releasing](docs/releasing.md).

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


## Development

```sh
go test ./...
go test -race ./...    # where the Go race detector is supported
go vet ./...
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The UI depends only on the scanner interface. Platform backends stay in
`internal/scanner`; path and usage models live in `internal/model`.
Python is used only for release tooling, never by the installed application.

See [Releasing](docs/releasing.md) for the CI matrix, tag workflow, checksums, and tap updates.

## License

[MIT](LICENSE).
