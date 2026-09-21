package tui

import (
	tea "charm.land/bubbletea/v2"
	"fmt"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"strings"
)

// Focus holds an immutable ancestry snapshot. A refresh must never silently
// replace the process the user is about to terminate with another ancestor.
func (a *App) updateAncestry(key string) tea.Cmd {
	switch key {
	case "left", "esc", "i", "tab", "shift+tab":
		a.ancestorSource = nil
	case "up":
		a.ancestorCursor = min(len(a.ancestorSource.Ancestors)-1, a.ancestorCursor+1)
	case "down", "j":
		a.ancestorCursor = max(-1, a.ancestorCursor-1)
	case "home", "g":
		a.ancestorCursor = len(a.ancestorSource.Ancestors) - 1
	case "end", "G":
		a.ancestorCursor = -1
	case "k", "x":
		if a.height < 22 || a.width < 38 {
			a.status = "Enlarge the terminal to review the selected ancestor."
			a.statusError = true
			return nil
		}
		a.prepareKill(key)
	case "1", "2":
		a.ancestorSource = nil
		return a.updateMain(key)
	case "q":
		return tea.Quit
	}
	return nil
}

// Both focused and unfocused inspectors render the same nested tree.
func (a *App) ancestryTree(p *model.Process, w, available int) []string {
	total := len(p.Ancestors) + 1
	cursor := len(p.Ancestors)
	if a.ancestorSource != nil {
		cursor = len(p.Ancestors) - 1 - a.ancestorCursor
	}
	count := max(2, available)
	rows := []int{}
	if total > count && cursor < total-1 {
		// Reserve the final row for the current process; scroll the ancestor
		// window around the selected node so neither can disappear.
		start := max(0, cursor-(count-1)+1)
		for row := start; row < min(total-1, start+count-1); row++ {
			rows = append(rows, row)
		}
		rows = append(rows, total-1)
	} else {
		start := max(0, cursor-count+1)
		for row := start; row < min(total, start+count); row++ {
			rows = append(rows, row)
		}
	}
	lines := []string{}
	for _, row := range rows {
		index := len(p.Ancestors) - 1 - row
		name, pid := p.Name, p.PID
		if index >= 0 {
			name, pid = p.Ancestors[index].Name, p.Ancestors[index].PID
		}
		prefix := strings.Repeat("  ", min(row, 8)) + "└─ "
		label := prefix + fmt.Sprintf("%s (%d)", safe(name), pid)
		if a.ancestorSource != nil && index == a.ancestorCursor {
			lines = append(lines, selectedStyle.Render(cell(label, w)))
		} else if index < 0 {
			lines = append(lines, accent.Render(label))
		} else {
			lines = append(lines, muted.Render(label))
		}
	}
	return lines
}

func (a *App) treeTarget() model.Process {
	p := a.ancestorSource
	if a.ancestorCursor < 0 {
		return *p
	}
	parent := p.Ancestors[a.ancestorCursor]
	return model.Process{Identity: model.Identity{PID: parent.PID, Started: parent.Started}, Name: parent.Name}
}
