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
  RAM, two-sample CPU, actionable parent identity, and the backend's lock evidence.
  It rejects a stale identity, stops only the test child, and verifies unchanged
  file contents.
- `TestNativeOpenFileIsNotALock` verifies that a shared ordinary open file is not
  mislabeled as a lock.
- `TestProgramSmokeWorkflow` runs Bubble Tea's event loop, renderer and keyboard
  decoder with pipe input. It exercises search inheritance, lock filtering, tree
  focus, confirmation, post-termination focus reset, tab switching, select-all,
  resize and quit. Termination uses an isolated fake backend.
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
