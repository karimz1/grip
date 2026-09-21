package tui

import (
	tea "charm.land/bubbletea/v2"
	"fmt"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"os"
)

func (a *App) prepareKill(key string) {
	if a.stopping {
		return
	}
	a.pending = nil
	a.parentAction = false
	a.force = key == "x" || key == "X"
	a.confirm = false
	a.offset = 0
	if a.screen == mainScreen && a.ancestorSource != nil {
		target := a.treeTarget()
		if target.Started == "" || target.PID <= 1 || target.PID == os.Getpid() {
			a.status = "This ancestor cannot be terminated: protected process or identity unavailable."
			a.statusError = true
			return
		}
		a.pending = []model.Process{target}
		a.parentAction = a.ancestorCursor >= 0
	} else if key == "K" || key == "X" || (a.screen == mainScreen && len(a.selected) > 0) {
		candidates := a.result.Processes
		if len(a.selected) == 0 {
			candidates = a.visible
		}
		seen := make(map[string]bool)
		for _, p := range candidates {
			if seen[p.Key()] {
				continue
			}
			seen[p.Key()] = true
			if len(a.selected) > 0 {
				if a.selected[p.Key()] {
					a.pending = append(a.pending, p)
				}
			} else if p.MatchesFilter(a.filter.Value()) {
				a.pending = append(a.pending, p)
			}
		}
	} else if a.screen == detailScreen && a.detail != nil {
		a.pending = append(a.pending, *a.detail)
	} else if p := a.current(); p != nil {
		a.pending = append(a.pending, *p)
	}
	if len(a.pending) > 0 {
		a.screen = confirmScreen
	}
}

func (a *App) updateConfirm(key string) tea.Cmd {
	switch key {
	case "esc", "q", "n":
		a.pending = nil
		a.screen = mainScreen
		a.offset = 0
	case "tab", "left", "right":
		a.confirm = !a.confirm
	case "up":
		a.offset = max(0, a.offset-1)
	case "down", "j":
		a.offset++
	case "pgdown":
		a.offset += a.pageSize()
	case "pgup":
		a.offset = max(0, a.offset-a.pageSize())
	case "enter":
		if a.confirm && (a.height < 12 || a.width < 38) {
			a.status = "Enlarge the terminal to review the processes before confirming."
			return nil
		}
		if !a.confirm {
			a.pending = nil
			a.screen = mainScreen
			a.offset = 0
			return nil
		}
		pending := append([]model.Process(nil), a.pending...)
		force := a.force
		ctx, backend := a.ctx, a.backend
		a.pending = nil
		a.screen = mainScreen
		a.offset = 0
		a.stopping = true
		a.status = "Requesting termination…"
		a.statusError = false
		return func() tea.Msg {
			result := killMsg{}
			for _, p := range pending {
				if err := ctx.Err(); err != nil {
					result.failures = append(result.failures, "cancelled remaining actions")
					break
				}
				if err := backend.Kill(ctx, p.Identity, force); err != nil {
					result.failures = append(result.failures, fmt.Sprintf("PID %d: %s", p.PID, err))
				} else {
					result.sent++
				}
			}
			return result
		}
	}
	return nil
}
