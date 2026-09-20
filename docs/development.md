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
