package tui

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/karimz1/open-file-lock-handle/internal/model"
)

func cpuText(p model.Process) string {
	if !p.CPUKnown {
		return "—"
	}
	return fmt.Sprintf("%.1f%%", p.CPUPercent)
}
func memoryText(p model.Process) string {
	if !p.MemoryKnown {
		return "—"
	}
	if p.MemoryBytes >= 1<<30 {
		return fmt.Sprintf("%.1f GiB", float64(p.MemoryBytes)/(1<<30))
	}
	return fmt.Sprintf("%.1f MiB", float64(p.MemoryBytes)/(1<<20))
}

func (a *App) dashboard() tea.View {
	w := max(1, a.width-4)
	first, second := "1 Processes", "2 Locked files"
	if w < 48 {
		first, second = "1 Proc", "2 Locks"
	}
	left, right := tabActive.Render(first), tabInactive.Render(second)
	if a.lockedTab {
		left, right = tabInactive.Render(first), tabActive.Render(second)
	}
	tabs := brand.Render("oflh") + "  " + left + right
	if w < 32 {
		tabs = left + right
	}
	activity := "MANUAL · r refresh"
	if a.autoRefresh {
		activity = "LIVE · every 5s"
	}
	if a.scanning {
		activity = []string{"◐", "◓", "◑", "◒"}[a.pulse] + " scanning"
	}
	if a.stopping {
		activity = "requesting termination…"
	}
	mode := "Navigation · / search · Tab/→ tree"
	if a.ancestorSource != nil {
		mode = "Tree · ↑↓ choose process · k stop / x force kill · Tab/← back"
	}
	if a.filtering {
		mode = "Search · Enter apply · Esc cancel"
	}
	lines := []string{headerLine(tabs, muted.Render(safe(a.version)), w), headerLine(muted.Render(safe(a.target.Path)), accent.Render(activity), w), muted.Render(mode)}
	lines = append(lines, a.searchBox(w, "/ Search PID, process, path… · * wildcard")...)
	tableWidth, panelWidth := w, 0
	if a.ancestorSource != nil || (!a.hideInspector && w >= 116 && a.height >= 22) {
		panelWidth = min(46, w/3)
		if a.ancestorSource != nil && w < 116 {
			panelWidth = w
			tableWidth = 0
		}
		if tableWidth != 0 {
			tableWidth = w - panelWidth - 3
		}
	}
	var list []string
	if a.lockedTab {
		list = a.lockedView(tableWidth)
	} else {
		list = a.listView(tableWidth)
	}
	summary := list[0]
	order := a.sortBy
	if order == "" {
		order = "pid"
		if a.filter.Value() != "" {
			order = "match"
		}
	}
	lines = append(lines, headerLine(summary, muted.Render("sort: "+order), w))
	// The shared lists begin with a summary and three search-box lines.
	body := list[4:]
	footer := a.footer()
	status := "Enter inspect · Space select · m RAM / c CPU / n name / p PID"
	if a.lockedTab {
		status = "Confirmed locks only · Enter for full path and lock details"
	}
	if a.status != "" {
		status = safe(a.status)
	}
	bottom := []string{muted.Render(status)}
	if a.statusError {
		bottom[0] = danger.Render(status)
	}
	if len(a.result.Warnings) > 0 {
		bottom = append(bottom, warning.Render(safe(strings.Join(a.result.Warnings, " • "))))
	}
	bottom = append(bottom, muted.Render(strings.Repeat("─", w)))
	bottom = append(bottom, footer...)
	available := max(0, a.height-len(lines)-len(bottom))
	var panel []string
	if panelWidth > 0 {
		panel = a.inspector(panelWidth, available)
	}
	for i := 0; i < available; i++ {
		row := ""
		if i < len(body) {
			row = body[i]
		}
		if panelWidth > 0 && tableWidth == 0 {
			row = ""
			if i < len(panel) {
				row = panel[i]
			}
		} else if panelWidth > 0 {
			row = ansi.Truncate(row, tableWidth, "…")
			row += strings.Repeat(" ", max(0, tableWidth-ansi.StringWidth(row))) + muted.Render(" │ ")
			if i < len(panel) {
				row += panel[i]
			}
		}
		lines = append(lines, row)
	}
	lines = append(lines, bottom...)
	if len(lines) > a.height {
		lines = lines[:a.height]
	}
	for i, line := range lines {
		lines[i] = "  " + ansi.Truncate(line, w, "…")
	}
	v := tea.NewView(strings.Join(lines, "\n"))
	v.AltScreen = true
	return v
}

func (a *App) inspector(w, h int) []string {
	p := a.current()
	if a.ancestorSource != nil {
		p = a.ancestorSource
	}
	if p == nil {
		return []string{accent.Render("PROCESS INSPECTOR"), "", muted.Render("Select a result to inspect it.")}
	}
	lines := []string{accent.Render(safe(p.Name)) + muted.Render(fmt.Sprintf("  · PID %d", p.PID)), muted.Render(strings.Repeat("─", w)), "CPU " + cpuText(*p) + "   RAM " + memoryText(*p), muted.Render("CPU = share of machine · RAM = RSS"), "ACCESS " + evidenceStyle(processAccess(*p)).Render(processAccess(*p)), "", accent.Render("ANCESTRY")}
	if a.ancestorSource != nil {
		if h < 18 {
			lines = append(lines[:2], accent.Render("ANCESTRY · focused"))
		} else {
			lines[len(lines)-1] = accent.Render("ANCESTRY · focused")
		}
	}
	lines = append(lines, a.ancestryTree(p, w, max(2, h-len(lines)-5))...)
	if a.ancestorSource != nil {
		target := a.treeTarget()
		lines = append(lines, "", accent.Render("ACTION TARGET"), fmt.Sprintf("%s · PID %d", safe(target.Name), target.PID))
	}
	lines = append(lines, "", accent.Render("EXECUTABLE"))
	exe := strings.Split(ansi.Hardwrap(present(p.Executable), w, true), "\n")
	lines = append(lines, exe[:min(3, len(exe))]...)
	if len(p.Usages) > 0 {
		lines = append(lines, "", accent.Render("SELECTED PATH"))
		path := strings.Split(ansi.Hardwrap(safe(p.Usages[0].Path), w, true), "\n")
		lines = append(lines, path[:min(3, len(path))]...)
		if p.Usages[0].Lock != "" {
			lines = append(lines, warning.Render(safe(p.Usages[0].Lock)))
		}
	}
	if !p.CPUKnown {
		note := "Resource metrics unavailable."
		if p.MemoryKnown {
			note = "CPU pending · startup sample follows."
		}
		lines = append(lines, "", muted.Render(note))
	}
	if len(lines) > h && h > 0 {
		lines = lines[:h]
		lines[h-1] = muted.Render("Enter for details · i hide panel")
	}
	return lines
}
