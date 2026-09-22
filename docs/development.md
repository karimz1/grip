# Development

## Test locally

```sh
go test ./...
go test -race ./...    # where the Go race detector is supported
go vet ./...
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The UI depends only on the scanner interface. Platform backends stay in
`internal/scanner`; path and usage models live in `internal/model`.
Python is used only for release tooling, never by the installed application.

Release and Homebrew publishing details are documented in [Releasing](releasing.md).

## Native platform stability gate

CI runs natively on Linux, Windows and macOS, on both amd64 and arm64. The release
workflow depends on the same matrix. No core feature test is skipped by OS.

- `TestNativeFeatureContract` starts a pipe-synchronized child, checks discovery,
  RAM, positive two-sample CPU from a busy helper, actionable parent identity, and the backend's lock evidence.
  It rejects a stale identity, stops only the test child, and verifies unchanged
  file contents.
- `TestNativeLockModes` covers POSIX read, write, and bounded-range locks on
  Unix, and read, write, and delete sharing conflicts on Windows.
- `TestNativeLockReleaseRefresh` releases a file while its helper stays alive,
  then verifies that another scan removes the lock evidence.
- `TestNativeParentTermination` discovers an isolated helper's parent, rejects a
  stale parent identity, force terminates the real parent, and verifies that the
  child releases its lock when the parent's control pipe closes. This tests the
  helper's lifecycle, not a general guarantee that stopping parents stops children.
- `TestNativeOpenFileIsNotALock` verifies that a shared ordinary open file is not
  mislabeled as a lock.
- `TestProgramSmokeWorkflow` runs Bubble Tea's event loop, renderer and keyboard
  decoder with pipe input. It exercises search inheritance, lock filtering, tree
  focus, confirmation, post-termination focus reset, tab switching, select-all,
  details refresh/auto-refresh, resize and quit. Termination uses an isolated fake backend.
- `TestProgramSmokeNativeStartup` starts the real native scanner inside the TUI
  and verifies that rendering and quit remain responsive during a scan. Completed
  native scans are validated independently by the feature contract.
- Both TUI smoke tests repeat three times per matrix runner. Unit tests, vet,
  standalone builds and packaging checks also run. The race detector runs where
  Go supports it (currently excluded only on Windows arm64).

These are headless terminal-stream tests, not claims of visual validation in
every terminal emulator. Windows/macOS runtime verification requires their native
CI jobs; cross-compilation alone is insufficient. The README documents each OS's
lock-evidence scope. Never replace unknown information with invented lock owners.

## Native lock fixture

Scanner integration tests use a small native C program located at:

    internal/scanner/testdata/lockfixture/lock-fixture.c

The fixture creates real operating-system file locks so the scanner can be
tested against native locking behavior rather than mocks. By completely bypassing 
the Go runtime and acquiring locks via authentic native OS interfaces, we guarantee
that the scanner reliably discovers locks held by real-world third-party processes.

The implementation uses the platform's native locking mechanism:

- Windows: `CreateFile` (with sharing denial) and `LockFileEx`
- macOS: POSIX `fcntl`
- Linux: POSIX `fcntl`

The fixture supports several modes:

    lockfixture open  <file>
    lockfixture read  <file>
    lockfixture write <file>
    lockfixture range <file> <start> <length>

`open` keeps a file handle open without acquiring a byte-range lock.

`read` acquires a shared/read lock over the file.

`write` acquires an exclusive/write lock over the file.

`range` acquires an exclusive/write lock over a specific byte range.

The Go integration tests compile the fixture into a temporary directory and
start it as a child process. The test runner passes standard input via a pipe 
to intentionally keep the C program alive. The fixture reports when the requested 
lock is ready, allowing the scanner to inspect the live process and definitively 
verify the detected lock state.

Compiled fixture binaries are temporary test artifacts and are not committed
to the repository.

### Requirements

Running the native integration tests requires a C compiler:

- macOS: Clang (`cc`), included with Xcode Command Line Tools
- Linux: GCC or Clang (`cc`)
- Windows: MSVC (`cl`) or MinGW GCC

The fixture intentionally uses native OS locking semantics. Lock behavior is
therefore not expected to be identical across platforms. In particular,
POSIX locks on macOS and Linux are advisory and do not necessarily prevent
unrelated processes from modifying, renaming, or deleting a file.