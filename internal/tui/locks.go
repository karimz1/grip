package tui

import (
	"fmt"

	"github.com/charmbracelet/x/ansi"
)

func (a *App) lockedView(w int) []string {
	paths := make(map[string]bool)
	for _, p := range a.visible {
		paths[p.Usages[0].Path] = true
	}
	summary := fmt.Sprintf("%d locked files · %d lock entries", len(paths), len(a.visible))
	if len(a.selected) > 0 {
		summary += fmt.Sprintf(" · %d selected", len(a.selected))
	}
	lines := []string{accent.Render(summary)}
	lines = append(lines, a.searchBox(w, "/ Search PID, process, path… · * wildcard")...)

	if !a.result.LockDetection && !a.scanning {
		return append(lines, "", "Lock detection is unavailable on this platform.", muted.Render("Use 1 Processes to inspect observed file usage."))
	}
	if len(a.visible) == 0 {
		message := "No confirmed locks found in this scan."
		if a.scanning {
			message = "Scanning for file locks…"
		} else if a.filter.Value() != "" {
			message = "No locked files match your filter."
		}
		return append(lines, "", message, muted.Render("Visibility depends on permissions; press r to refresh."))
	}
	pidWidth, nameWidth := 8, 18
	if w < 62 {
		nameWidth = 10
	}
	if w < 40 {
		nameWidth = 0
		pidWidth = 7
	}
	pathWidth := max(1, w-3-pidWidth-nameWidth)
	heading := "LOCKED FILE"
	if w < 40 {
		heading = "FILE"
	}
	processCell := func(name string) string {
		if nameWidth == 0 {
			return ""
		}
		return cell(name, nameWidth)
	}
	lines = append(lines, muted.Render("   "+cell("PID", pidWidth)+processCell("PROCESS")+heading))
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
		u := p.Usages[0]
		marker := "   "
		if a.selected[p.Key()] {
			marker = " ● "
		} else if i == a.cursor {
			marker = " › "
		}
		path := u.Path
		if u.Deleted {
			path += " (deleted)"
		}
		row := marker + cell(fmt.Sprint(p.PID), pidWidth) + processCell(p.Name) + cell(path, pathWidth)
		if i == a.cursor {
			row = selectedStyle.Render(ansi.Strip(row))
		}
		lines = append(lines, row)
	}
	return lines
}
