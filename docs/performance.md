# Performance

Measurements help identify regressions in startup, scanning, and memory use.
Compare identical workloads and observation coverage before claiming an improvement.

## Go and Rust comparison (Linux)

Measured 2026-09-25 on Fedora x86-64, kernel 7.2.7, 16 logical CPUs. Go baseline:
`65556d75d951ab5dd74145d9707058020969c48e`, built with
`go build -trimpath -ldflags='-s -w'`. Rust revision: `012c79bb72177d234c1a4c7609cfd5c614beb7de`, built with
`cargo build --release --locked --bin oflh`.
These are warm runs on one shared workstation, not cold-cache or universal OS
performance claims. Raw results: [JSON](measurements/linux-2026-09-25.json).

| Measurement | Go | Rust |
| --- | ---: | ---: |
| Directory scan median / p95 | 218.8 / 227.8 ms | 100.6 / 114.7 ms |
| Single-file scan median / p95 | 218.4 / 233.8 ms | 96.8 / 106.6 ms |
| Missing-file scan median / p95 | 208.9 / 220.5 ms | 108.0 / 112.1 ms |
| Scanner peak RSS, directory | 11,808 KiB | 2,708 KiB |
| First frame, median of 5 | 42.5 ms | 1.7 ms |
| TUI RSS after 3 seconds, median of 5 | 16,840 KiB | 4,436 KiB |
| Stripped executable | 4,530,441 bytes | 1,085,944 bytes |

The fixture owner held 512 read/write handles to 4 KiB files: 256 inside the
target directory and 256 outside it, with 64 writable mappings and 16 exclusive
flocks inside the target. Thirty sequential scans per target ran with the same
user and live fixture. `/usr/bin/time -f %M` measured peak scanner RSS. Both
implementations returned one process for the directory/file and zero for a
missing file. A separate comparison sorted PID, birth identity, path, relation,
access, deleted status, and lock text: all 336 directory rows, 3 single-file rows,
and 0 missing-file rows were identical. This establishes fixture parity, not
exhaustive equivalence under arbitrary process churn.

The TUI measurement used a 160×40 PTY, `TERM=xterm-256color`, first emitted `oflh`
text as the first-frame marker, terminal query replies, and `/proc/PID/status`
for RSS. Five runs per implementation used the same live fixture. The recorded
CPU-tick interval was 1.2–3.0 seconds after launch and may include follow-up work;
it is not an isolated CPU benchmark. Rust consumed zero ticks in that interval.

To repeat scanner measurements with an equivalent live fixture:

```sh
cargo build --release --locked -p oflh-platform --example scan_bench
/usr/bin/time -f %M target/release/examples/scan_bench PATH 30
target/release/examples/scan_bench PATH --dump
```

The harness is developer-only. Compare Go using `scanner.New().Scan` against
`model.NewTarget(PATH)` in the baseline checkout, timing the same 30 iterations
and sorting the same observation fields. Keep fixture lifetimes, privileges,
process population, and scan targets fixed during each comparison.

## Interpreting results

These figures cover Linux x86-64 only. They do not measure Windows or macOS,
cold filesystem caches, or every process workload. Permissions, descriptor counts,
filesystem latency, and process churn affect scan cost.

When investigating a regression, record the operating system, architecture, CPU
count, compiler version, build profile, fixture size, and target type. Include
median and p95 scan times and verify that both runs observe the same files and
lock evidence. Separate the first visible frame from completed scan results.
Keep hostnames, usernames, private paths, and application data out of shared results.
