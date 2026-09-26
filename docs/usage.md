# User guide

[Back to the README](../README.md) · [Platform behavior](platform-support.md)

## Getting started

Run `oflh` in an interactive terminal:

```sh
oflh                          # Current directory
oflh ./build                  # Directory and its descendants
oflh ./build/plugin.dll       # One file
oflh "/path/with spaces"       # Quote paths containing spaces
```

1. Use **Processes** (`1`) to see processes referencing the target path.
2. Press `/` to search, then `Enter` to finish typing.
3. Press `Enter` on a process to inspect its individual file usages.
4. Use **Locked files** (`2`) to narrow the list to files with lock or sharing-conflict evidence.
5. Press `r` to rescan, or `a` to enable five-second auto-refresh.

In process details, `r` refreshes file usages and metrics; `a` toggles the same
auto-refresh used by the main view. Search and lock filters remain active.

In process details, `l` toggles **Locks only** without clearing the search. The
summary counts distinct locked paths among the displayed usages. Lock labels
appear in muted red. The selected filename appears above the table, and its full
path appears below it. Use `←` and `→` to page through a long path.

On wide terminals, the side panel shows the selected process, its ancestry,
resource usage, and path details. Compact terminals retain resource and parent
information in the process details view.

## Ports

Press `3` for **Ports**, or start directly with `oflh --ports`. The default is all
visible local bindings. Press `s` to show only ports of processes associated with
the target path, or start with `oflh --here .`. File and port search text are
independent, so switching tabs does not mix their filters.

| Query | Meaning |
| --- | --- |
| `3000` or `port:3000` | Exact local port; does not match `13000` or a PID |
| `tcp 3000` | TCP listeners on port 3000 |
| `udp` | Bound UDP sockets |
| `ipv6` | IPv6 bindings |
| `127.0.0.1` | Bindings matching that address |
| `pid:424242` | Search the process ID explicitly |
| `node 3000` | Both the process text and exact port must match |

Port numbers must be between 1 and 65535. Other text uses the fragment and
wildcard rules below. Each address/protocol/port binding has its own row; IPv4
and IPv6 bindings remain separate. The initial order is by port number.

**THIS PATH** means the process has an observed file, executable, mapping, or
working-directory reference matching the target. It is an association, not proof
that a particular project created the socket. Scope follows the original target,
not the search text in the Processes tab. Windows cannot inspect working
directories through the current backend; interpreted development servers may
therefore appear only in ALL PORTS. See [platform coverage](platform-support.md#ports).

`Enter` opens the selected process's ports; `f` switches to its target-matching
file usages and back. `/` searches within details. `r` refreshes and `a` toggles
five-second auto-refresh. Port details display LIVE or MANUAL mode.

Socket discovery starts when you first open Ports or request port details, then
runs with subsequent refreshes. An entry marked **owner unavailable** has no
verified PID and cannot be terminated. Selecting multiple bindings of the same
process produces one termination target. Selections survive scope and tab changes;
confirmation includes hidden selections, as it does for file results.

## Search

Search is case-insensitive and updates as you type. The same rules apply to
Processes, Locked files, and file-usage details. Ports use exact matching for
numeric port terms as described above.

| Query | Meaning |
| --- | --- |
| `dll` | A contiguous fragment, such as the extension in `plugin.dll` |
| `MIMJWT` | Word or CamelCase prefixes in `Microsoft.IdentityModel.JsonWebTokens.dll` |
| `micro*dll` | Chunks `micro` and `dll`, in that order |
| `FLEC.` / `FLEC*` | Abbreviated stem in `FileLockExampleCli.dll` or `FileLockExampleCli.deps.json` |
| `FLEC*.json` | Abbreviated stem followed by a `.json` fragment |
| `*.dll` | A field containing `.dll` |
| `micro*dll mapped` | Both terms must match |

Plain terms match contiguous fragments or word/CamelCase prefixes. They do not
match arbitrary scattered letters across a path. `*` matches zero or more
characters, including path separators. Each chunk between wildcards supports
the same fragment and abbreviation matching. Punctuation remains literal, so
`FLEC.` requires a dot after the abbreviated stem. Patterns can match anywhere in a field;
`*.dll` is not restricted to a filename ending in `.dll`.

Filename and process-name matches rank above directory-only matches. Choosing
an explicit sort order, such as CPU or PID, overrides relevance ordering.

The matched path and `+N` count follow the active filter. When you open process
details, file-related terms carry into the details search. Process-only terms,
such as a PID, stay in the main search. Press `Esc` in details to clear its search
and see all usages again.

## Process actions

Press `Space` to select processes, or `Ctrl+A` to select or deselect all visible
processes. Selections survive filtering, so a selection may include processes
that are no longer visible.

- `k` requests normal termination of the selection, or the current process if nothing is selected.
- `x` force kills the same targets.
- Every action requires confirmation, with **Cancel** selected by default. The dialog lists the targets, including hidden selections.

To act on a parent, press `Tab` or `→` to focus the ancestry tree. It initially
selects the current process. Use `↑` to move toward its parents and `↓` to return
toward the current process. Here, `k` and `x` apply only to the highlighted tree
node, regardless of selections in the main list.

The tree retains its captured process identities across refreshes. Before
termination, the backend revalidates the target to guard against PID reuse.
Protected processes and ancestors without an available identity cannot be
stopped. A successful termination request returns focus to the results and
refreshes the list.

Stopping a parent may close its application and affect its children. It does not
recursively terminate the entire tree. `oflh` does not directly unlock files;
terminating a process may release the resources it holds.

## Keyboard reference

| Context | Key | Action |
| --- | --- | --- |
| Main view | `1` / `2` / `3` | Processes / Locked files / Ports |
| Ports | `s` | All ports / this path |
| Process details | `f` | File usages / ports |
| Lists | `↑` / `↓` | Move selection |
| Main view | `Enter` | Inspect process usages |
| Main view or details | `/` | Start search |
| Search input | `Enter` / `Esc` | Apply / cancel editing |
| Main view | `Space` | Select or deselect process |
| Main view | `Ctrl+A` | Select or deselect all visible processes |
| Process actions | `k` / `x` | Normal termination / force kill |
| Main view | `K` / `X` | Act on selection, or all filtered processes if none selected |
| Main view or details | `r` / `a` | Refresh / toggle auto-refresh |
| Main view | `n` / `p` | Sort by name / PID |
| Main view | `m` / `c` | Sort by RAM / CPU, highest first |
| Main view | `i` | Toggle side panel |
| Main view | `Tab` / `→` | Focus ancestry tree |
| Ancestry tree | `Tab` / `←` / `Esc` | Return to results |
| Ancestry tree | `Home` / `End` | Select root / current process |
| Process details | `l` | Toggle Locks only |
| Process details | `←` / `→` | Page through selected path |
| Navigation | `Esc` | Clear search, cancel, or go back |
| Navigation | `?` | Show help |
| Navigation | `R` / `D` | Open repository / donation page in your default browser |
| Navigation | `q` / `Ctrl+C` | Back / quit |

Only `1`, `2`, and `3` switch tabs. `Tab` changes table/tree focus or selects a dialog
action. Selected **Cancel** has a green background; selected **Terminate** or
**Force kill** has a red background. A pointer also identifies the choice, and
Cancel remains the default. `R` and `D` open GitHub and Donate; Help shows both
URLs near the top. While editing a search, it stays in the search input.

## Terminal support

Use an interactive terminal with Unicode and true-color support. A wider window
provides room for the process table and side panel. No particular terminal
emulator is required.
