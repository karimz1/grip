package tui

import (
	"bytes"
	"context"
	"io"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"github.com/karimz1/open-file-lock-handle/internal/scanner"
)

type smokeSnapshot struct {
	view                            string
	scanning, tree, details, locked bool
	rows                            int
	selected                        int
	stopped                         bool
}
type smokeProbe chan smokeSnapshot
type smokeModel struct{ app *App }

func (m smokeModel) Init() tea.Cmd  { return m.app.Init() }
func (m smokeModel) View() tea.View { return m.app.View() }
func (m smokeModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if reply, ok := msg.(smokeProbe); ok {
		reply <- smokeSnapshot{ansi.Strip(m.app.View().Content), m.app.scanning, m.app.ancestorSource != nil, m.app.screen == detailScreen, m.app.lockedTab, len(m.app.usageRows), len(m.app.selected), m.app.stopping}
		return m, nil
	}
	_, cmd := m.app.Update(msg)
	return m, cmd
}

type smokeScanner struct{}

func (smokeScanner) Scan(context.Context, model.Target) (model.Result, error) {
	return model.Result{LockDetection: true, Processes: []model.Process{{Identity: model.Identity{PID: 4242, Started: "123"}, Name: "SmokeApp", Ancestors: []model.Ancestor{{PID: 4241, Started: "122", Name: "Launcher"}}, Usages: []model.Usage{{Path: "/sample/Microsoft.Core.dll", Relation: "locked", Access: "read/write", Lock: "POSIX WRITE"}, {Path: "/sample/config.json", Relation: "open", Access: "read"}}}}}, nil
}
func (smokeScanner) Kill(context.Context, model.Identity, bool) error { return nil }

func runProgramSmoke(t *testing.T, backend scanner.Scanner, interact bool) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	target, err := model.NewTarget(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	a := New(ctx, backend, target, "smoke-test")
	input, write := io.Pipe()
	defer input.Close()
	defer write.Close()
	var output bytes.Buffer
	program := tea.NewProgram(smokeModel{a}, tea.WithContext(ctx), tea.WithInput(input), tea.WithOutput(&output), tea.WithWindowSize(140, 32), tea.WithEnvironment([]string{"TERM=xterm-256color"}), tea.WithoutSignalHandler(), tea.WithoutCatchPanics())
	done := make(chan error, 1)
	go func() { _, err := program.Run(); done <- err }()
	defer func() { cancel(); program.Wait() }()
	snapshot := func() smokeSnapshot {
		t.Helper()
		reply := make(smokeProbe, 1)
		program.Send(reply)
		select {
		case state := <-reply:
			return state
		case err := <-done:
			t.Fatalf("program exited early: %v", err)
		case <-ctx.Done():
			t.Fatal("TUI timed out")
		}
		return smokeSnapshot{}
	}
	until := func(predicate func(smokeSnapshot) bool) smokeSnapshot {
		t.Helper()
		for {
			state := snapshot()
			if predicate(state) {
				return state
			}
			select {
			case <-ctx.Done():
				t.Fatal("state transition timed out")
			case <-time.After(10 * time.Millisecond):
			}
		}
	}
	key := func(text string) {
		t.Helper()
		if _, err := io.WriteString(write, text); err != nil {
			t.Fatal(err)
		}
	}
	state := snapshot()
	if interact {
		state = until(func(s smokeSnapshot) bool { return !s.scanning })
	}
	// Native startup checks that the UI can render and quit even while a
	// system-wide scan is in flight. NativeFeatureContract separately waits
	// for completed scans and validates their evidence on every OS.
	if !strings.Contains(state.view, "1 Processes") {
		t.Fatalf("main view missing: %s", state.view)
	}
	if interact {
		key("/dll\r")
		until(func(s smokeSnapshot) bool { return strings.Contains(s.view, "/ dll") })
		key("\r")
		state = until(func(s smokeSnapshot) bool { return s.details && s.rows == 1 })
		if strings.Contains(state.view, "config.json") {
			t.Fatal("details search was not inherited")
		}
		key("l")
		until(func(s smokeSnapshot) bool { return strings.Contains(s.view, "LOCKS ONLY") })
		key("r")
		until(func(s smokeSnapshot) bool { return s.details && !s.scanning && s.rows == 1 })
		key("a")
		until(func(s smokeSnapshot) bool { return strings.Contains(s.view, "LIVE") })
		key("a")
		until(func(s smokeSnapshot) bool { return strings.Contains(s.view, "MANUAL") })
		key("q")
		until(func(s smokeSnapshot) bool { return !s.details })
		key("\t")
		until(func(s smokeSnapshot) bool { return s.tree })
		key("\x1b[A")
		until(func(s smokeSnapshot) bool { return strings.Contains(s.view, "Launcher · PID 4241") })
		key("k")
		until(func(s smokeSnapshot) bool { return strings.Contains(s.view, "Terminate 1 processes?") })
		key("\t\r")
		until(func(s smokeSnapshot) bool {
			return !s.tree && !s.stopped && strings.Contains(s.view, "Termination requested")
		})
		key("2")
		until(func(s smokeSnapshot) bool { return s.locked })
		key("\x01")
		until(func(s smokeSnapshot) bool { return s.selected == 1 })
		program.Send(tea.WindowSizeMsg{Width: 60, Height: 24})
		state = snapshot()
		for _, line := range strings.Split(state.view, "\n") {
			if ansi.StringWidth(line) > 60 {
				t.Fatal("resize overflow")
			}
		}
	}
	key("q")
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-ctx.Done():
		t.Fatal("quit timed out")
	}
	if output.Len() == 0 {
		t.Fatal("renderer produced no terminal output")
	}
}

// These run the real Bubble Tea event loop, renderer and keyboard decoder on
// every native CI runner. Actions use a fake scanner and never stop host apps.
func TestProgramSmokeWorkflow(t *testing.T) { runProgramSmoke(t, smokeScanner{}, true) }
func TestProgramSmokeNativeStartup(t *testing.T) {
	s, err := scanner.New()
	if err != nil {
		t.Fatal(err)
	}
	runProgramSmoke(t, s, false)
}
