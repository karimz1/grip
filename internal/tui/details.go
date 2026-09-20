package tui

import (
	"fmt"
	"path/filepath"
	"strings"

	"charm.land/bubbles/v2/table"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

var frame = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(lipgloss.Color("8"))

func newUsageTable() table.Model {
	styles := table.DefaultStyles()
	styles.Header = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("6")).Padding(0, 1).BorderBottom(true).BorderStyle(lipgloss.NormalBorder()).BorderForeground(lipgloss.Color("8"))
	styles.Cell = lipgloss.NewStyle().Padding(0, 1)
	styles.Selected = lipgloss.NewStyle().Foreground(lipgloss.Color("0")).Background(lipgloss.Color("6")).Bold(true)
	return table.New(table.WithStyles(styles), table.WithFocused(true))
}

func usageColumns(w int) []table.Column {
	if w >= 90 {
		return []table.Column{{Title: "FILE", Width: 26}, {Title: "RELATION", Width: 12}, {Title: "ACCESS", Width: 12}, {Title: "DIRECTORY", Width: w - 58}}
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
	columns := usageColumns(max(1, a.width-6))
	// Clear old rows before changing the column count on resize.
	a.usageTable.SetRows(nil)
	a.usageTable.SetColumns(columns)
	a.usageRows = nil
	rows := []table.Row{}
	for _, u := range a.detail.Usages {
		if !u.MatchesFilter(a.detailFilter.Value()) {
			continue
		}
		a.usageRows = append(a.usageRows, u)
		name := filepath.Base(u.Path)
		if u.Deleted {
			name += " (deleted)"
		}
		row := table.Row{safe(name), safe(u.Relation), safe(u.Access), safe(filepath.Dir(u.Path))}
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
		v := tea.NewView(ansi.Truncate("grip · enlarge terminal to inspect", a.width, "…") + "\n" + ansi.Truncate("Esc / q back", a.width, "…"))
		v.AltScreen = true
		return v
	}
	lines := []string{
		brand.Render("grip") + muted.Render("  /  process details"),
		accent.Render(safe(p.Name)) + muted.Render(fmt.Sprintf("   PID %d   ·   %s", p.PID, present(p.User))),
		muted.Render("EXE  ") + cell(present(p.Executable), w-5),
		muted.Render("CWD  ") + cell(present(p.CWD), w-5),
	}
	search := muted.Render("/ Search files, DLLs, paths…")
	searchStyle := frame
	if a.filtering {
		search = a.detailFilter.View()
		searchStyle = searchStyle.BorderForeground(lipgloss.Color("6"))
	} else if a.detailFilter.Value() != "" {
		search = accent.Render("/ ") + safe(a.detailFilter.Value()) + muted.Render("  · Esc clears")
	}
	lines = append(lines, strings.Split(searchStyle.Width(w).Render(ansi.Truncate(search, w-2, "…")), "\n")...)
	position := 0
	if len(a.usageRows) > 0 {
		position = a.usageTable.Cursor() + 1
	}
	count := fmt.Sprintf("%d of %d usages", len(a.usageRows), len(p.Usages))
	positionText := fmt.Sprintf("%d / %d", position, len(a.usageRows))
	lines = append(lines, accent.Render(count)+strings.Repeat(" ", max(1, w-ansi.StringWidth(count)-len(positionText)))+muted.Render(positionText))
	footer := []string{accent.Render("/ search  ↑↓ select  ←→ path  Esc back"), muted.Render("k terminate process · x force kill · q back")}
	if a.filtering {
		footer = []string{accent.Render("↑↓ browse matches  Enter apply  Esc cancel"), muted.Render("Fuzzy search · filename, path, relation, access")}
	}
	// Three preview lines, two footer lines, two borders; the table includes its header.
	tableHeight := max(3, a.height-len(lines)-7)
	a.usageTable.SetWidth(w - 2)
	a.usageTable.SetHeight(tableHeight)
	tableView := a.usageTable.View()
	if len(a.usageRows) == 0 {
		message := "No usage entries match your search."
		if a.detailFilter.Value() == "" {
			message = "No usage entries available."
		}
		tableView = lipgloss.NewStyle().Width(w - 2).Height(tableHeight).Render(muted.Render(ansi.Hardwrap(message, w-2, true)))
	}
	lines = append(lines, strings.Split(frame.Render(tableView), "\n")...)
	preview := []string{muted.Render("SELECTED PATH"), "", ""}
	if position > 0 {
		u := a.usageRows[position-1]
		path := safe(u.Path)
		if u.Deleted {
			path += " (deleted)"
		}
		parts := strings.Split(ansi.Hardwrap(path, w, true), "\n")
		a.pathOffset = min(a.pathOffset, max(0, len(parts)-2))
		preview[0] = muted.Render("SELECTED PATH · " + safe(u.Relation) + " · " + safe(u.Access))
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
