package tui

import (
	"fmt"
	"path/filepath"
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
)

func (a *App) View() tea.View {
	if a.screen == mainScreen && a.height >= 12 {
		return a.dashboard()
	}
	if a.screen == detailScreen {
		return a.detailsPage()
	}
	w := max(1, a.width-4)
	activity := ""
	if a.scanning {
		activity = "  " + []string{"◐", "◓", "◑", "◒"}[a.pulse] + " scanning"
	}
	if a.stopping {
		activity = "  requesting termination…"
	}
	title := brand.Render("oflh") + "  " + muted.Render("Open File Lock Handle") + accent.Render(activity)
	version := muted.Render(safe(a.version))
	source := ""
	lines := []string{headerLine(title, version, w), headerLine(accent.Render(safe(a.target.Path)), source, w), muted.Render(strings.Repeat("─", w))}
	var content []string
	switch a.screen {
	case confirmScreen:
		content = a.confirmView(w)
	case helpScreen:
		content = helpView()
	default:
		if a.lockedTab {
			content = a.lockedView(w)
		} else {
			content = a.listView(w)
		}
		first, second := muted.Render("1 Processes"), muted.Render("2 Locked files")
		if a.lockedTab {
			second = selectedStyle.Render("2 Locked files")
		} else {
			first = selectedStyle.Render("1 Processes")
		}
		if w < 40 {
			first, second = muted.Render("1 Proc"), muted.Render("2 Locks")
			if a.lockedTab {
				second = selectedStyle.Render("2 Locks")
			} else {
				first = selectedStyle.Render("1 Proc")
			}
		}
		content = append([]string{first + "   " + second}, content...)
	}
	footer := a.footer()
	status := a.status
	if status == "" {
		status = "read · write · mapped / executable · unknown"
	}
	statusLine := muted.Render(safe(status))
	if a.status == "" {
		statusLine = success.Render("read") + muted.Render(" · ") + warning.Render("write") + muted.Render(" · ") + accent.Render("mapped / executable") + muted.Render(" · unknown")
	}
	if a.status == "" && a.lockedTab {
		statusLine = muted.Render("Confirmed locks · permissions and scan timing limit visibility")
	}
	if a.statusError {
		statusLine = danger.Render(safe(status))
	}
	bottom := []string{statusLine}
	if len(a.result.Warnings) > 0 {
		bottom = append(bottom, warning.Render(safe(strings.Join(a.result.Warnings, " • "))))
	}
	bottom = append(bottom, muted.Render(strings.Repeat("─", w)))
	bottom = append(bottom, footer...)
	available := max(1, a.height-len(lines)-len(bottom)-2)
	if a.screen != mainScreen {
		a.offset = min(a.offset, max(0, len(content)-available))
		content = content[a.offset:]
		if len(content) > available {
			content = content[:available]
			content[len(content)-1] = muted.Render("↓ scroll for more")
		}
	}
	if len(content) > available {
		content = content[:available]
	}
	lines = append(lines, content...)
	for len(lines) < a.height-len(bottom)-2 {
		lines = append(lines, "")
	}
	lines = append(lines, bottom...)
	// Preserve the action footer even on very small terminal sizes.
	if a.height < 12 {
		lines = []string{accent.Render("oflh " + safe(a.target.Path))}
		if p := a.current(); p != nil {
			lines = append(lines, fmt.Sprintf("%d %s", p.PID, safe(p.Name)))
		}
		if a.screen == confirmScreen {
			lines = append(lines, danger.Render(fmt.Sprintf("%d targets. Enlarge to review.", len(a.pending))))
		}
		lines = append(lines, "Resize for full view · Esc back · q quit")
	}
	if len(lines) > a.height {
		lines = lines[:a.height]
	}
	for i, line := range lines {
		padding := "  "
		if a.width < 10 {
			padding = ""
		}
		lines[i] = ansi.Truncate(padding+line, a.width, "")
	}
	v := tea.NewView(strings.Join(lines, "\n"))
	v.AltScreen = true
	return v
}

func headerLine(left, right string, width int) string {
	if ansi.StringWidth(right) >= width {
		return ansi.Truncate(right, width, "…")
	}
	left = ansi.Truncate(left, max(1, width-ansi.StringWidth(right)-1), "…")
	leftWidth := ansi.StringWidth(left)
	rightWidth := ansi.StringWidth(right)
	return left + strings.Repeat(" ", width-leftWidth-rightWidth) + right
}

func (a *App) listView(w int) []string {
	summary := fmt.Sprintf("%d processes", len(a.result.Processes))
	if len(a.visible) != len(a.result.Processes) {
		summary = fmt.Sprintf("%d of %d processes", len(a.visible), len(a.result.Processes))
	}
	if len(a.selected) > 0 {
		summary += fmt.Sprintf(" · %d selected", len(a.selected))
	}
	lines := []string{accent.Render(summary)}
	lines = append(lines, a.searchBox(w, "/ Search PID, process, path… · * wildcard")...)

	if len(a.visible) == 0 {
		if a.scanning {
			return append(lines, "", "Discovering processes…", muted.Render("You can keep navigating while the scan runs."))
		}
		if a.filter.Value() != "" {
			return append(lines, "", "No processes match your filter.", muted.Render("Press / to edit it or Esc to clear."))
		}
		return append(lines, "", "No visible processes are using this path.", muted.Render("Press r to scan again. Permission limits may hide usage."))
	}
	if w >= 94 {
		lines = append(lines, muted.Render("   "+cell("PID", 8)+cell("PROCESS", 18)+cell("USER", 12)+cell("CPU%", 8)+cell("RAM", 11)+cell("ACCESS", 12)+"MATCHED PATH"))
	} else if w >= 62 {
		lines = append(lines, muted.Render("   "+cell("PID", 8)+cell("PROCESS", 19)+cell("ACCESS", 12)+"MATCHED PATH"))
	} else {
		lines = append(lines, muted.Render("   PID      PROCESS / MATCHED PATH"))
	}
	count := a.pageSize()
	if len(a.result.Warnings) > 0 {
		count--
	}
	count = max(1, count)
	start := max(0, a.cursor-count+1)
	if start+count > len(a.visible) {
		start = max(0, len(a.visible)-count)
	}
	for i := start; i < min(len(a.visible), start+count); i++ {
		p := a.visible[i]
		marker := "   "
		if a.selected[p.Key()] {
			marker = " ● "
		} else if i == a.cursor {
			marker = " › "
		}
		u := p.Usages[0]
		path := u.Path
		if a.target.Directory {
			if rel, err := filepath.Rel(a.target.Path, path); err == nil {
				path = rel
			}
		}
		if u.Deleted {
			path += " (deleted)"
		}
		if len(p.Usages) > 1 {
			path += fmt.Sprintf("  +%d", len(p.Usages)-1)
		}
		var row string
		if w >= 94 {
			row = marker + cell(fmt.Sprint(p.PID), 8) + cell(p.Name, 18) + cell(p.User, 12) + cell(cpuText(p), 8) + cell(memoryText(p), 11) + evidenceCell(processAccess(p), 12) + cell(path, w-72)
		} else if w >= 62 {
			row = marker + cell(fmt.Sprint(p.PID), 8) + cell(p.Name, 19) + evidenceCell(processAccess(p), 12) + cell(path, w-42)
		} else {
			row = marker + cell(fmt.Sprint(p.PID), 9) + cell(p.Name, max(1, w-12))
			if w >= 42 {
				row = marker + cell(fmt.Sprint(p.PID), 8) + cell(p.Name, 14) + cell(path, w-25)
			}
		}
		if i == a.cursor {
			row = selectedStyle.Render(row)
		}
		lines = append(lines, row)
	}
	return lines
}

func (a *App) confirmView(w int) []string {
	verb := "Terminate"
	note := "Requests a normal shutdown. Unsaved work may be lost."
	if a.force {
		verb = "FORCE KILL"
		note = "Immediate termination: no cleanup or chance to save. Data may be lost."
	}
	lines := []string{danger.Render(fmt.Sprintf("%s %d processes?", verb, len(a.pending))), ""}
	if a.parentAction {
		lines = append(lines, warning.Render("Target: selected ancestor, not the file-owning child."), warning.Render("Stopping a parent may close its application and affect its children."), "")
	}
	lines = append(lines, strings.Split(ansi.Hardwrap(note, w, true), "\n")...)
	lines = append(lines, "", muted.Render("Affected processes (including selections hidden by filters):"))
	for _, p := range a.pending {
		lines = append(lines, fmt.Sprintf("  %-9d %s", p.PID, safe(p.Name)))
	}
	return lines
}

func (a *App) footer() []string {
	w := max(1, a.width-4)
	if a.screen == mainScreen && a.ancestorSource != nil {
		return shortcutLines(w, shortcut{"↑↓", "process"}, shortcut{"k", "stop target"}, shortcut{"x", "force kill target"}, shortcut{"Tab/←", "back"})
	}
	if a.filtering {
		return shortcutLines(w, shortcut{"Enter", "apply"}, shortcut{"Esc", "cancel"})
	}
	switch a.screen {
	case confirmScreen:
		cancel, action := "[ Cancel ]", "[ Terminate ]"
		if a.force {
			action = "[ Force kill ]"
		}
		if a.confirm {
			action = danger.Reverse(true).Render(action)
		} else {
			cancel = selectedStyle.Render(cancel)
		}
		return append([]string{cancel + "   " + action}, shortcutLines(w, shortcut{"Tab", "choose"}, shortcut{"Enter", "confirm"}, shortcut{"Esc", "cancel"}, shortcut{"↑↓", "review"})...)
	case detailScreen:
		lockHint := shortcut{"l", "locks only"}
		if a.detailLocksOnly {
			lockHint.label = "all usages"
		}
		if a.width < 40 {
			return shortcutLines(w, shortcut{"/", "search"}, shortcut{"Esc", "back"})
		}
		if a.width < 80 {
			return shortcutLines(w, shortcut{"r", "refresh"}, shortcut{"a", "auto"}, shortcut{"/", "search"}, lockHint, shortcut{"↑↓", "select"}, shortcut{"←→", "path"}, shortcut{"Esc", "back"})
		}
		return shortcutLines(w, shortcut{"r", "refresh"}, shortcut{"a", "auto"}, shortcut{"/", "search"}, lockHint, shortcut{"↑↓", "select"}, shortcut{"←→", "path"}, shortcut{"k", "stop"}, shortcut{"x", "force kill"}, shortcut{"Esc", "back"})
	case helpScreen:
		return shortcutLines(w, shortcut{"↑↓", "scroll"}, shortcut{"Esc", "back"})
	default:
		auto := "auto off"
		if a.autoRefresh {
			auto = "auto on"
		}
		hints := []shortcut{{"1/2", "tabs"}, {"/", "search"}, {"↑↓", "move"}, {"Enter", "inspect"}, {"Space", "select"}, {"Ctrl+A", "all"}, {"Tab/→", "tree"}, {"i", "panel"}, {"m/c", "RAM/CPU sort"}, {"a", auto}, {"r", "refresh"}, {"k", "stop"}, {"x", "force kill"}, {"?", "help"}, {"q", "quit"}}
		if a.width < 80 {
			hints = []shortcut{{"1/2", "tabs"}, {"/", "search"}, {"↑↓", "move"}, {"Enter", "inspect"}, {"?", "help"}, {"q", "quit"}}
		}
		return shortcutLines(w, hints...)
	}
}

func helpView() []string {
	return []string{accent.Render("OPEN FILE LOCK HANDLE"), muted.Render("Source: https://github.com/karimz1/open-file-lock-handle"), "", "1 / 2          Processes / locked files", "↑ / ↓, j      Navigate processes", "PgUp / PgDn    Move one page", "Home / End     First / last process", "Enter          Inspect all matching paths", "Space          Toggle process selection", "Ctrl+A         Select / deselect all visible processes", "/              Search PID, name, user, path, access", "*              Wildcard in search: micro*dll, *.dll", "               Fragments / CamelCase initials; spaces combine terms.", "Esc            Clear filter / back / cancel", "r              Refresh; cancels the previous scan", "a              Toggle five-second auto-refresh", "i              Toggle process side panel (wide terminals)", "Tab / →        Focus tree; ↑↓ choose process, k/x stop it", "Tab / ← / Esc  Leave tree selection", "l              In details: toggle confirmed locks only", "m / c          Sort by RAM / CPU (highest first)", "n / p          Sort by name / PID", "k / x          Stop / force kill selection, or current process", "K / X          Selected processes; if none, all filtered processes", "Tab            Choose Cancel / Terminate in confirmation", "?              Show this help", "q / Ctrl+C     Quit", "", warning.Render("Every termination requires confirmation. Cancel is the default."), "Selections survive filtering; bulk confirmation includes every target.", "Process identities are revalidated before any termination.", "", accent.Render("READING THE EVIDENCE"), "locked         Platform lock or sharing-conflict evidence", "open           An observed file descriptor", "cwd            The current working directory", "executable     The process executable", "mapped         A mapped file or loaded module", "restart manager  Windows reports an application using a resource", "unknown        The OS does not expose this information", "", "A file being open does not prove it is locked.", "Permission restrictions and races can make results incomplete."}
}
