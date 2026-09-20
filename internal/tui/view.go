package tui

import (
	"fmt"
	"path/filepath"
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
)

func (a *App) View() tea.View {
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
	lines := []string{brand.Render("grip") + "  " + muted.Render("see what's using your files") + accent.Render(activity), accent.Render(cell(a.target.Path, w)), muted.Render(strings.Repeat("─", w))}
	var content []string
	switch a.screen {
	case confirmScreen:
		content = a.confirmView(w)
	case helpScreen:
		content = helpView()
	default:
		content = a.listView(w)
	}
	footer := a.footer()
	status := a.status
	if status == "" {
		status = "Usage does not necessarily mean a file is locked."
	}
	statusLine := muted.Render(safe(status))
	if a.statusError {
		statusLine = danger.Render(safe(status))
	}
	bottom := []string{muted.Render(strings.Repeat("─", w)), statusLine}
	if len(a.result.Warnings) > 0 {
		bottom = append(bottom, warning.Render(safe(strings.Join(a.result.Warnings, " • "))))
	}
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
		lines = []string{accent.Render("grip " + safe(a.target.Path))}
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

func (a *App) listView(w int) []string {
	summary := fmt.Sprintf("%d processes", len(a.result.Processes))
	if len(a.visible) != len(a.result.Processes) {
		summary = fmt.Sprintf("%d of %d processes", len(a.visible), len(a.result.Processes))
	}
	if len(a.selected) > 0 {
		summary += fmt.Sprintf(" · %d selected", len(a.selected))
	}
	lines := []string{accent.Render(summary)}
	if a.filtering {
		lines = append(lines, a.filter.View())
	} else if a.filter.Value() != "" {
		lines = append(lines, muted.Render("/ "+safe(a.filter.Value())+"  · Esc clears"))
	} else {
		lines = append(lines, "")
	}
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
		lines = append(lines, muted.Render("   "+cell("PID", 8)+cell("PROCESS", 20)+cell("USER", 13)+cell("RELATION", 13)+cell("ACCESS", 12)+"MATCHED PATH"))
	} else if w >= 62 {
		lines = append(lines, muted.Render("   "+cell("PID", 8)+cell("PROCESS", 19)+cell("RELATION", 12)+"MATCHED PATH"))
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
			row = marker + cell(fmt.Sprint(p.PID), 8) + cell(p.Name, 20) + cell(p.User, 13) + cell(u.Relation, 13) + cell(u.Access, 12) + cell(path, w-69)
		} else if w >= 62 {
			row = marker + cell(fmt.Sprint(p.PID), 8) + cell(p.Name, 19) + cell(u.Relation, 12) + cell(path, w-42)
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
	lines = append(lines, strings.Split(ansi.Hardwrap(note, w, true), "\n")...)
	lines = append(lines, "", muted.Render("Affected processes (including selections hidden by filters):"))
	for _, p := range a.pending {
		lines = append(lines, fmt.Sprintf("  %-9d %s", p.PID, safe(p.Name)))
	}
	return lines
}

func (a *App) footer() []string {
	if a.filtering {
		return []string{accent.Render("Enter apply   Esc cancel")}
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
		return []string{cancel + "   " + action, muted.Render("Tab choose · Enter confirm · Esc cancel · ↑↓ review")}
	case detailScreen:
		return []string{accent.Render("/ search usages   ↑↓ scroll   Esc clear / back"), accent.Render("k terminate process   x force kill process")}
	case helpScreen:
		return []string{accent.Render("↑↓ scroll   Esc back")}
	default:
		if a.width < 65 {
			return []string{accent.Render("↑↓ move  Enter inspect  Space select"), accent.Render("k stop  x force  K/X bulk  / filter  r refresh  a auto  ?  q")}
		}
		return []string{accent.Render("↑↓ navigate  Enter inspect  Space select  / filter"), accent.Render("k terminate  x force  K/X bulk  r refresh  a auto  ? help  q quit")}
	}
}

func helpView() []string {
	return []string{accent.Render("A LITTLE GRIP GOES A LONG WAY"), "", "↑ / ↓, j      Navigate processes", "PgUp / PgDn    Move one page", "Home / End     First / last process", "Enter          Inspect all matching paths", "Space          Toggle process selection", "/              Fuzzy filter (PID, name, user, path, access)", "Esc            Clear filter / back / cancel", "r              Refresh; cancels the previous scan", "a              Toggle five-second auto-refresh", "k              Request termination of current process", "x              Force kill current process", "K / X          Selected processes; if none, all filtered processes", "Tab            Choose Cancel / Terminate in confirmation", "?              Show this help", "q / Ctrl+C     Quit", "", warning.Render("Every termination requires confirmation. Cancel is the default."), "Selections survive filtering; bulk confirmation includes every target.", "Process identities are revalidated before any termination.", "", accent.Render("READING THE EVIDENCE"), "open           An observed file descriptor", "cwd            The current working directory", "executable     The process executable", "mapped         A mapped file or loaded module", "restart manager  Windows reports an application using a resource", "unknown        The OS does not expose this information", "", "A file being open does not prove it is locked.", "Permission restrictions and races can make results incomplete."}
}
