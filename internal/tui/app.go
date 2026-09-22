package tui

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	"charm.land/bubbles/v2/table"
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"github.com/karimz1/open-file-lock-handle/internal/scanner"
)

type screen int

const (
	mainScreen screen = iota
	detailScreen
	confirmScreen
	helpScreen
)

type scanMsg struct {
	generation int
	result     model.Result
	err        error
}
type pulseMsg struct{}
type refreshMsg struct {
	periodic bool
}
type killMsg struct {
	sent     int
	failures []string
}

type App struct {
	ctx                             context.Context
	backend                         scanner.Scanner
	target                          model.Target
	version                         string
	width, height                   int
	result                          model.Result
	visible                         []model.Process
	selected                        map[string]bool
	cursor, offset                  int
	screen                          screen
	lockedTab                       bool
	hideInspector                   bool
	ancestorSource                  *model.Process
	ancestorCursor                  int
	parentAction                    bool
	cpuWarmupDone                   bool
	sortBy                          string
	filter                          textinput.Model
	detailFilter                    textinput.Model
	detailLocksOnly                 bool
	usageTable                      table.Model
	usageRows                       []model.Usage
	pathOffset                      int
	filtering                       bool
	filterBefore                    string
	scanning, stopping, autoRefresh bool
	generation, pulse               int
	cancelScan                      context.CancelFunc
	status                          string
	statusError                     bool
	detail                          *model.Process
	pending                         []model.Process
	force, confirm                  bool
}

const autoRefreshInterval = 5 * time.Second

func New(ctx context.Context, backend scanner.Scanner, target model.Target, version string) *App {
	input := textinput.New()
	input.Placeholder = "PID, process, path… (* wildcard)"
	input.Prompt = "/ "
	input.CharLimit = 256
	input.SetWidth(60)
	detailInput := textinput.New()
	detailInput.Placeholder = "DLL, path, access… (* wildcard)"
	detailInput.Prompt = "/ "
	detailInput.CharLimit = 256
	detailInput.SetWidth(60)
	return &App{ctx: ctx, backend: backend, target: target, version: version, width: 80, height: 24, selected: make(map[string]bool), filter: input, detailFilter: detailInput, usageTable: newUsageTable()}
}

func (a *App) Init() tea.Cmd { return tea.Batch(a.startScan(), pulse()) }
func pulse() tea.Cmd {
	return tea.Tick(120*time.Millisecond, func(time.Time) tea.Msg { return pulseMsg{} })
}
func autoRefreshTick() tea.Cmd {
	return tea.Tick(autoRefreshInterval, func(time.Time) tea.Msg { return refreshMsg{periodic: true} })
}

func (a *App) startScan() tea.Cmd {
	if a.cancelScan != nil {
		a.cancelScan()
	}
	ctx, cancel := context.WithCancel(a.ctx)
	a.cancelScan = cancel
	a.generation++
	generation := a.generation
	a.scanning = true
	backend, target := a.backend, a.target
	return func() tea.Msg { result, err := backend.Scan(ctx, target); return scanMsg{generation, result, err} }
}

func (a *App) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		a.width = max(1, msg.Width)
		a.height = max(1, msg.Height)
		a.filter.SetWidth(max(1, a.width-8))
		a.detailFilter.SetWidth(max(1, a.width-8))
		a.rebuildUsages(false)
	case pulseMsg:
		a.pulse = (a.pulse + 1) % 4
		return a, pulse()
	case refreshMsg:
		if msg.periodic && !a.autoRefresh {
			return a, nil
		}
		if a.scanning {
			if msg.periodic {
				return a, autoRefreshTick()
			}
			return a, nil
		}
		if msg.periodic {
			return a, tea.Batch(a.startScan(), autoRefreshTick())
		}
		return a, a.startScan()
	case scanMsg:
		if msg.generation != a.generation {
			return a, nil
		}
		a.scanning = false
		if msg.err != nil {
			a.status = "Scan failed: " + msg.err.Error()
			a.statusError = true
			return a, nil
		}
		var key string
		var lockUsage model.Usage
		if p := a.current(); p != nil {
			key = p.Key()
			if a.lockedTab {
				lockUsage = p.Usages[0]
			}
		}
		a.result = msg.result
		alive := make(map[string]bool)
		for _, p := range a.result.Processes {
			alive[p.Key()] = true
		}
		for k := range a.selected {
			if !alive[k] {
				delete(a.selected, k)
			}
		}
		a.refilter()
		for i, p := range a.visible {
			if p.Key() == key && (!a.lockedTab || p.Usages[0] == lockUsage) {
				a.cursor = i
				break
			}
		}
		a.refreshDetails()
		if !a.cpuWarmupDone {
			for _, p := range a.result.Processes {
				if p.MemoryKnown && !p.CPUKnown {
					a.cpuWarmupDone = true
					return a, tea.Tick(time.Second, func(time.Time) tea.Msg { return refreshMsg{} })
				}
			}
		}
	case killMsg:
		a.stopping = false
		a.statusError = len(msg.failures) > 0
		a.status = fmt.Sprintf("Termination requested for %d processes. Refreshing…", msg.sent)
		if len(msg.failures) > 0 {
			a.status += fmt.Sprintf(" %d failed: %s", len(msg.failures), strings.Join(msg.failures, "; "))
		}
		if msg.sent == 0 {
			return a, nil
		}
		// Once a termination request succeeds, stop displaying the captured
		// tree. The refreshed results become the navigation source again.
		a.ancestorSource = nil
		a.ancestorCursor = -1
		a.parentAction = false
		return a, tea.Tick(500*time.Millisecond, func(time.Time) tea.Msg { return refreshMsg{} })
	case projectLinkMsg:
		a.statusError = msg.err != nil
		if msg.err != nil {
			a.status = "Could not open browser. Open manually: " + msg.url
		} else {
			a.status = "Browser requested: " + msg.url
		}
	case tea.KeyPressMsg:
		if msg.String() == "ctrl+c" {
			if a.cancelScan != nil {
				a.cancelScan()
			}
			return a, tea.Quit
		}
		if a.filtering {
			return a, a.updateFilter(msg)
		}
		if a.screen != confirmScreen {
			switch msg.String() {
			case "R":
				return a, openProjectLink(repositoryURL)
			case "D":
				return a, openProjectLink(donateURL)
			}
		}
		switch a.screen {
		case confirmScreen:
			return a, a.updateConfirm(msg.String())
		case detailScreen:
			return a, a.updateDetails(msg.String())
		case helpScreen:
			switch msg.String() {
			case "esc", "?", "q":
				a.screen = mainScreen
				a.offset = 0
			case "down", "j":
				a.offset++
			case "up":
				a.offset = max(0, a.offset-1)
			}
		default:
			return a, a.updateMain(msg.String())
		}
	default:
		if a.filtering {
			return a, a.updateFilterInput(msg)
		}
	}
	return a, nil
}

func (a *App) refilter() {
	a.visible = nil
	for _, p := range a.result.Processes {
		if a.lockedTab {
			for _, u := range p.Usages {
				if u.Lock == "" {
					continue
				}
				row := p
				row.Usages = []model.Usage{u}
				if row.MatchesFilter(a.filter.Value()) {
					a.visible = append(a.visible, row)
				}
			}
			continue
		}
		if p.MatchesFilter(a.filter.Value()) {
			query := usageQuery(p, a.filter.Value())
			if query != "" {
				usages := make([]model.Usage, 0, len(p.Usages))
				for _, u := range p.Usages {
					if u.MatchesFilter(query) {
						usages = append(usages, u)
					}
				}
				if len(usages) == 0 {
					continue
				}
				p.Usages = usages
			}
			a.visible = append(a.visible, p)
		}
	}
	sort.SliceStable(a.visible, func(i, j int) bool {
		p, q := a.visible[i], a.visible[j]
		switch a.sortBy {
		case "":
			if a.filter.Value() != "" {
				left, right := p.SearchScore(a.filter.Value()), q.SearchScore(a.filter.Value())
				if left != right {
					return left > right
				}
			}
		case "memory":
			if p.MemoryKnown != q.MemoryKnown {
				return p.MemoryKnown
			}
			if p.MemoryBytes != q.MemoryBytes {
				return p.MemoryBytes > q.MemoryBytes
			}
		case "cpu":
			if p.CPUKnown != q.CPUKnown {
				return p.CPUKnown
			}
			if p.CPUPercent != q.CPUPercent {
				return p.CPUPercent > q.CPUPercent
			}
		case "name":
			if p.Name != q.Name {
				return strings.ToLower(p.Name) < strings.ToLower(q.Name)
			}
		}
		return p.PID < q.PID
	})
	a.cursor = min(max(0, a.cursor), max(0, len(a.visible)-1))
}
func (a *App) current() *model.Process {
	if len(a.visible) == 0 {
		return nil
	}
	return &a.visible[min(a.cursor, len(a.visible)-1)]
}

func (a *App) updateMain(key string) tea.Cmd {
	if a.ancestorSource != nil {
		return a.updateAncestry(key)
	}
	switch key {
	case "right", "tab", "shift+tab":
		if p := a.current(); p != nil {
			if a.width < 120 || a.height < 22 {
				a.status = "Enlarge the terminal to select an ancestor (120 columns, 22 rows)."
				a.statusError = false
				return nil
			}
			copy := *p
			a.ancestorSource = &copy
			a.ancestorCursor = -1
			a.hideInspector = false
		}
	case "i":
		a.hideInspector = !a.hideInspector
	case "c", "m", "n", "p":
		a.sortBy = map[string]string{"c": "cpu", "m": "memory", "n": "name", "p": "pid"}[key]
		a.cursor = 0
		a.refilter()
	case "1", "2":
		if key == "1" {
			a.lockedTab = false
		} else {
			a.lockedTab = true
		}
		a.cursor = 0
		a.refilter()
	case "q":
		if a.cancelScan != nil {
			a.cancelScan()
		}
		return tea.Quit
	case "a":
		a.autoRefresh = !a.autoRefresh
		if a.autoRefresh {
			a.status = "Auto-refresh enabled (every 5s)."
			a.statusError = false
			return autoRefreshTick()
		}
		a.status = "Auto-refresh disabled."
		a.statusError = false
	case "up":
		a.cursor = max(0, a.cursor-1)
	case "down", "j":
		a.cursor = min(max(0, len(a.visible)-1), a.cursor+1)
	case "pgup":
		a.cursor = max(0, a.cursor-a.pageSize())
	case "pgdown":
		a.cursor = min(max(0, len(a.visible)-1), a.cursor+a.pageSize())
	case "home", "g":
		a.cursor = 0
	case "end", "G":
		a.cursor = max(0, len(a.visible)-1)
	case "ctrl+a":
		all := len(a.visible) > 0
		for _, p := range a.visible {
			if !a.selected[p.Key()] {
				all = false
				break
			}
		}
		for _, p := range a.visible {
			if all {
				delete(a.selected, p.Key())
			} else {
				a.selected[p.Key()] = true
			}
		}
	case "space":
		if p := a.current(); p != nil {
			if a.selected[p.Key()] {
				delete(a.selected, p.Key())
			} else {
				a.selected[p.Key()] = true
			}
		}
	case "enter":
		if p := a.current(); p != nil {
			copy := *p
			if !a.lockedTab {
				for _, original := range a.result.Processes {
					if original.Key() == p.Key() {
						copy = original
						break
					}
				}
			}
			a.detail = &copy
			a.detailLocksOnly = false
			a.detailFilter.SetValue(usageQuery(copy, a.filter.Value()))
			a.screen = detailScreen
			a.offset = 0
			a.rebuildUsages(true)
		}
	case "/":
		a.filterBefore = a.filter.Value()
		a.filtering = true
		return a.filter.Focus()
	case "esc":
		a.filter.SetValue("")
		a.refilter()
	case "r":
		if !a.stopping {
			return a.startScan()
		}
	case "?":
		a.screen = helpScreen
		a.offset = 0
	case "k", "x", "K", "X":
		a.prepareKill(key)
	}
	return nil
}

func (a *App) updateFilter(msg tea.KeyPressMsg) tea.Cmd {
	input := a.activeFilter()
	if a.screen == detailScreen {
		switch msg.String() {
		case "up", "down", "pgup", "pgdown":
			return a.updateDetails(msg.String())
		}
	}
	switch msg.String() {
	case "enter":
		a.filtering = false
		input.Blur()
		return nil
	case "esc":
		a.filtering = false
		input.Blur()
		input.SetValue(a.filterBefore)
		a.filterChanged()
		return nil
	}
	return a.updateFilterInput(msg)
}

func (a *App) activeFilter() *textinput.Model {
	if a.screen == detailScreen {
		return &a.detailFilter
	}
	return &a.filter
}

func (a *App) filterChanged() {
	if a.screen == detailScreen {
		a.offset = 0
		a.rebuildUsages(true)
		return
	}
	a.cursor = 0
	a.refilter()
}

func (a *App) updateFilterInput(msg tea.Msg) tea.Cmd {
	input := a.activeFilter()
	before := input.Value()
	updated, cmd := input.Update(msg)
	*input = updated
	if input.Value() != before {
		a.filterChanged()
	}
	return cmd
}

func (a *App) updateDetails(key string) tea.Cmd {
	switch key {
	case "r", "a":
		return a.updateMain(key)
	case "l":
		a.detailLocksOnly = !a.detailLocksOnly
		a.rebuildUsages(true)
	case "/":
		a.filterBefore = a.detailFilter.Value()
		a.filtering = true
		a.offset = 0
		return a.detailFilter.Focus()
	case "esc":
		if a.detailFilter.Value() != "" {
			a.detailFilter.SetValue("")
			a.offset = 0
			a.rebuildUsages(true)
			return nil
		}
		a.screen = mainScreen
		a.offset = 0
	case "q", "enter":
		a.screen = mainScreen
		a.offset = 0
	case "down", "j":
		a.usageTable.MoveDown(1)
		a.pathOffset = 0
	case "up":
		a.usageTable.MoveUp(1)
		a.pathOffset = 0
	case "pgdown":
		a.usageTable.MoveDown(max(1, a.usageTable.Height()))
		a.pathOffset = 0
	case "pgup":
		a.usageTable.MoveUp(max(1, a.usageTable.Height()))
		a.pathOffset = 0
	case "home", "g":
		a.usageTable.GotoTop()
		a.pathOffset = 0
	case "end", "G":
		a.usageTable.GotoBottom()
		a.pathOffset = 0
	case "left":
		a.pathOffset = max(0, a.pathOffset-1)
	case "right":
		a.pathOffset++
	case "k", "x":
		a.prepareKill(key)
	}
	return nil
}

func (a *App) pageSize() int { return max(1, a.height-10-len(a.footer())) }

// Carry file-related terms into details, leaving process-only terms (such as a
// PID or username) in the process search where they belong.
func usageQuery(p model.Process, query string) string {
	var terms []string
	for _, term := range strings.Fields(query) {
		for _, u := range p.Usages {
			if u.MatchesFilter(term) {
				terms = append(terms, term)
				break
			}
		}
	}
	return strings.Join(terms, " ")
}
