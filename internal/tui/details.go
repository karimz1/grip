package tui

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"charm.land/bubbles/v2/table"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

var frame = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(lipgloss.Color("8"))

func newUsageTable() table.Model {
	styles := table.DefaultStyles()
	styles.Header = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#A78BFA")).Padding(0, 1).BorderBottom(true).BorderStyle(lipgloss.NormalBorder()).BorderForeground(lipgloss.Color("8"))
	styles.Cell = lipgloss.NewStyle().Padding(0, 1)
	styles.Selected = selectedStyle
	return table.New(table.WithStyles(styles), table.WithFocused(true))
}

func usageColumns(w int, longestName int) []table.Column {
	if w >= 90 {
		fileWidth := min(max(26, longestName), w-56)
		return []table.Column{{Title: "FILE", Width: fileWidth}, {Title: "RELATION", Width: 12}, {Title: "ACCESS", Width: 12}, {Title: "DIRECTORY", Width: w - fileWidth - 32}}
	}
	if w >= 56 {
		return []table.Column{{Title: "FILE", Width: w - 30}, {Title: "RELATION", Width: 12}, {Title: "ACCESS", Width: 12}}
	}
	if w >= 30 {
		return []table.Column{{Title: "FILE", Width: w - 16}, {Title: "RELATION", Width: 12}}
	}
	return []table.Column{{Title: "FILE", Width: max(1, w-2)}}
}

func (a *App) rebuildUsages(reset bool) {
	if a.detail == nil {
		return
	}
	cursor := a.usageTable.Cursor()
	longestName := 0
	for _, u := range a.detail.Usages {
		if (!a.detailLocksOnly || u.Lock != "") && u.MatchesFilter(a.detailFilter.Value()) {
			name := safe(filepath.Base(u.Path))
			if u.Deleted {
				name += " (deleted)"
			}
			longestName = max(longestName, ansi.StringWidth(name))
		}
	}
	columns := usageColumns(max(1, a.width-6), longestName)
	// Clear old rows before changing the column count on resize.
	a.usageTable.SetRows(nil)
	a.usageTable.SetColumns(columns)
	a.usageRows = nil
	rows := []table.Row{}
	for _, u := range a.detail.Usages {
		if (a.detailLocksOnly && u.Lock == "") || !u.MatchesFilter(a.detailFilter.Value()) {
			continue
		}
		a.usageRows = append(a.usageRows, u)
	}
	if a.detailFilter.Value() != "" {
		sort.SliceStable(a.usageRows, func(i, j int) bool {
			return a.usageRows[i].SearchScore(a.detailFilter.Value()) > a.usageRows[j].SearchScore(a.detailFilter.Value())
		})
	}
	for _, u := range a.usageRows {
		name := filepath.Base(u.Path)
		if u.Deleted {
			name += " (deleted)"
		}
		row := table.Row{safe(name), evidenceStyle(u.Relation).Render(safe(u.Relation)), evidenceStyle(u.Access).Render(safe(u.Access)), safe(filepath.Dir(u.Path))}
		rows = append(rows, row[:len(columns)])
	}
	a.usageTable.SetRows(rows)
	if reset {
		cursor = 0
		a.pathOffset = 0
	}
	a.usageTable.SetCursor(cursor)
}

// detailsPage keeps metadata, search, column headers and the selected path fixed.
// Only the Bubbles table's viewport scrolls.
func (a *App) detailsPage() tea.View {
	w := max(1, a.width-4)
	if a.detail == nil {
		return tea.NewView("Process unavailable")
	}
	p := a.detail
	if a.height < 18 || a.width < 28 {
		v := tea.NewView(ansi.Truncate("oflh · enlarge terminal to inspect", a.width, "…") + "\n" + ansi.Truncate("Esc / q back", a.width, "…"))
		v.AltScreen = true
		return v
	}
	lines := []string{
		brand.Render("oflh") + muted.Render("  /  process details"),
		accent.Render(safe(p.Name)) + muted.Render(fmt.Sprintf("   PID %d   ·   %s", p.PID, present(p.User))),
		muted.Render("EXE  ") + cell(present(p.Executable), w-5),
		muted.Render("CWD  ") + cell(present(p.CWD), w-5),
	}
	if a.height >= 22 {
		parent := "unavailable"
		if p.ParentPID > 0 {
			parent = fmt.Sprintf("PID %d", p.ParentPID)
		}
		if len(p.Ancestors) > 0 {
			parent = fmt.Sprintf("%s (%d)", safe(p.Ancestors[0].Name), p.Ancestors[0].PID)
		}
		lines = append(lines, muted.Render("PARENT ")+parent, muted.Render("CPU ")+cpuText(*p)+muted.Render(" machine · RAM ")+memoryText(*p)+muted.Render(" RSS"))
	}
	lines = append(lines, a.searchBox(w, "/ Search files, DLLs, paths… · * wildcard")...)
	position := 0
	if len(a.usageRows) > 0 {
		position = a.usageTable.Cursor() + 1
	}
	count := fmt.Sprintf("%d of %d usages", len(a.usageRows), len(p.Usages))
	lockedPaths := make(map[string]bool)
	for _, u := range a.usageRows {
		if u.Lock != "" {
			lockedPaths[u.Path] = true
		}
	}
	if a.result.LockDetection || len(lockedPaths) > 0 {
		count += " · " + evidenceStyle("locked").Render(fmt.Sprintf("%d locked files", len(lockedPaths)))
	}
	if a.detailLocksOnly {
		count = "LOCKS ONLY · " + count
	}
	positionText := fmt.Sprintf("%d / %d", position, len(a.usageRows))
	lines = append(lines, headerLine(accent.Render(count), muted.Render(positionText), w))
	footer := append([]string{muted.Render(strings.Repeat("─", w))}, a.footer()...)
	if position > 0 && a.height >= 20 {
		name := safe(filepath.Base(a.usageRows[position-1].Path))
		parts := strings.Split(ansi.Hardwrap(name, max(1, w-5), true), "\n")
		previewLines := min(2, len(parts), max(0, a.height-len(lines)-len(footer)-8))
		for i, part := range parts[:previewLines] {
			label := "     "
			if i == 0 {
				label = "FILE "
			}
			lines = append(lines, muted.Render(label)+part)
		}
	}
	// Metadata, path preview and footer stay visible while the table scrolls.
	tableHeight := max(1, a.height-len(lines)-len(footer)-5)
	a.usageTable.SetWidth(w - 2)
	a.usageTable.SetHeight(tableHeight)
	tableView := a.usageTable.View()
	if len(a.usageRows) == 0 {
		message := "No usage entries match your search."
		if a.detailFilter.Value() == "" {
			message = "No usage entries available."
		}
		if a.detailLocksOnly {
			message = "No confirmed locks match your search. Press l for all usages."
			if a.detailFilter.Value() == "" {
				message = "No confirmed locks in this process snapshot. Press l for all usages."
			}
			if !a.result.LockDetection {
				message = "Lock detection is unavailable on this platform. Press l for all usages."
			}
		}
		tableView = lipgloss.NewStyle().Width(w - 2).Height(tableHeight).Render(muted.Render(ansi.Hardwrap(message, w-2, true)))
	}
	lines = append(lines, strings.Split(frame.Render(tableView), "\n")...)
	preview := []string{muted.Render("SELECTED PATH"), "", ""}
	if position > 0 {
		u := a.usageRows[position-1]
		path := safe(u.Path)
		if u.Lock != "" {
			path += " · " + safe(u.Lock)
		}
		if u.Deleted {
			path += " (deleted)"
		}
		parts := strings.Split(ansi.Hardwrap(path, w, true), "\n")
		a.pathOffset = min(a.pathOffset, max(0, len(parts)-2))
		preview[0] = muted.Render("SELECTED PATH · ") + evidenceStyle(u.Relation).Render(safe(u.Relation)) + muted.Render(" · ") + evidenceStyle(u.Access).Render(safe(u.Access))
		if len(parts) > 2 {
			preview[0] = muted.Render(fmt.Sprintf("SELECTED PATH · ←→ lines %d–%d / %d", a.pathOffset+1, min(len(parts), a.pathOffset+2), len(parts)))
		}
		for i := 0; i < 2 && a.pathOffset+i < len(parts); i++ {
			preview[i+1] = parts[a.pathOffset+i]
		}
	}
	lines = append(lines, preview...)
	lines = append(lines, footer...)
	for i, line := range lines {
		lines[i] = "  " + ansi.Truncate(line, w, "…")
	}
	v := tea.NewView(strings.Join(lines, "\n"))
	v.AltScreen = true
	return v
}

// searchBox is shared by the process, locked-file and usage views.
func (a *App) searchBox(w int, placeholder string) []string {
	input := a.activeFilter()
	search := muted.Render(placeholder)
	searchStyle := frame
	if a.filtering {
		search = input.View()
		searchStyle = searchStyle.BorderForeground(lipgloss.Color("#A78BFA"))
	} else if input.Value() != "" {
		search = accent.Render("/ ") + safe(input.Value()) + muted.Render("  · Esc clears")
	}
	return strings.Split(searchStyle.Width(w).Render(ansi.Truncate(search, max(1, w-2), "…")), "\n")
}
