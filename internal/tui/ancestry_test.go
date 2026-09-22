package tui

import (
	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"github.com/karimz1/open-file-lock-handle/internal/scanner"
	"strings"
	"testing"
)

func ancestryApp() *App {
	a := searchApp()
	a.updateDetails("q")
	a.width = 170
	a.height = 36
	a.result.Processes[0].Ancestors = []model.Ancestor{{PID: 90, Started: "10", Name: "debugger"}, {PID: 80, Started: "9", Name: "rider"}, {PID: 1, Started: "1", Name: "systemd"}}
	a.refilter()
	a.selected[a.current().Key()] = true
	a.updateMain("right")
	return a
}

func TestAncestorActionUsesCapturedIdentity(t *testing.T) {
	a := ancestryApp()
	backend := &recordingScanner{}
	a.backend = backend
	a.updateMain("up")
	a.updateMain("up")
	if view := ansi.Strip(a.View().Content); !strings.Contains(view, "ACTION TARGET") || !strings.Contains(view, "rider · PID 80") {
		t.Fatalf("target missing: %s", view)
	}
	// Refreshed ancestry has a different process. The selected snapshot stays fixed.
	replacement := a.result.Processes[0]
	replacement.Ancestors = []model.Ancestor{{PID: 90, Started: "99", Name: "replacement"}}
	a.Update(scanMsg{generation: a.generation, result: model.Result{Processes: []model.Process{replacement}}})
	a.updateMain("k")
	if len(a.pending) != 1 || a.pending[0].PID != 80 || a.pending[0].Started != "9" || !a.parentAction {
		t.Fatal("action did not target the captured parent")
	}
	if !strings.Contains(strings.Join(a.confirmView(100), "\n"), "selected ancestor") {
		t.Fatal("confirmation must identify ancestor action")
	}
	if a.updateConfirm("enter") != nil || len(backend.calls) != 0 {
		t.Fatal("default must cancel")
	}
	a.updateMain("x")
	a.updateConfirm("tab")
	backend.err = scanner.ErrChanged
	cmd := a.updateConfirm("enter")
	if cmd == nil {
		t.Fatal("missing action")
	}
	msg := cmd().(killMsg)
	if len(backend.calls) != 1 || backend.calls[0] != (model.Identity{PID: 80, Started: "9"}) || len(msg.failures) != 1 || msg.sent != 0 || !backend.force {
		t.Fatal("identity failure must not claim termination")
	}
}

func TestAncestorProtectionAndNavigation(t *testing.T) {
	a := ancestryApp()
	a.updateMain("home")
	a.updateMain("k")
	if a.screen != mainScreen || len(a.pending) != 0 {
		t.Fatal("system ancestor must be protected")
	}
	a.ancestorSource.Ancestors = []model.Ancestor{{PID: 88, Name: "unknown"}}
	a.ancestorCursor = 0
	a.updateMain("x")
	if a.screen != mainScreen || len(a.pending) != 0 {
		t.Fatal("unknown identity must be protected")
	}
	a.updateMain("esc")
	if a.ancestorSource != nil || len(a.selected) != 1 {
		t.Fatal("back must preserve original selection")
	}
	a = ancestryApp()
	a.Update(tea.WindowSizeMsg{Width: 60, Height: 24})
	view := ansi.Strip(a.View().Content)
	if !strings.Contains(view, "ANCESTRY") || !strings.Contains(view, "test-app") {
		t.Fatal("resize hid selected ancestor")
	}
	for _, line := range strings.Split(view, "\n") {
		if ansi.StringWidth(line) > 60 {
			t.Fatal("width overflow")
		}
	}
	if len(strings.Split(view, "\n")) > 24 {
		t.Fatal("height overflow")
	}
	a.updateMain("2")
	if a.ancestorSource != nil || !a.lockedTab {
		t.Fatal("number key should leave ancestry and change views")
	}
}

func TestTabKeepsNestedTreeAndCurrentProcess(t *testing.T) {
	a := ancestryApp()
	a.updateMain("tab")
	if a.ancestorSource != nil || a.lockedTab {
		t.Fatal("Tab should return focus to results")
	}
	a.updateMain("tab")
	if a.ancestorSource == nil || a.ancestorCursor != -1 || a.lockedTab {
		t.Fatal("Tab should focus current process in tree")
	}
	view := ansi.Strip(a.View().Content)
	for _, label := range []string{"ANCESTRY", "└─ systemd", "  └─ rider", "    └─ debugger", "      └─ test-app", "test-app · PID 42"} {
		if !strings.Contains(view, label) {
			t.Fatalf("missing nested tree entry %q: %s", label, view)
		}
	}
	a.updateMain("up")
	if a.treeTarget().PID != 90 {
		t.Fatal("Up must move to immediate parent")
	}
	a.updateMain("down")
	if a.treeTarget().PID != 42 {
		t.Fatal("Down must return to current process")
	}
	a.updateMain("k")
	if len(a.pending) != 1 || a.pending[0].PID != 42 || a.parentAction {
		t.Fatal("current node action targeted a parent")
	}
}

func TestShortTreeKeepsSelectedAndCurrentNodes(t *testing.T) {
	a := ancestryApp()
	for i := 0; i < len(a.ancestorSource.Ancestors); i++ {
		a.ancestorCursor = i
		view := ansi.Strip(strings.Join(a.ancestryTree(a.ancestorSource, 60, 2), "\n"))
		if !strings.Contains(view, a.treeTarget().Name) || !strings.Contains(view, "test-app (42)") {
			t.Fatalf("tree hid selected or current process: %s", view)
		}
	}
}

func TestSuccessfulTerminationLeavesTreeFocus(t *testing.T) {
	for _, parent := range []bool{false, true} {
		a := ancestryApp()
		a.backend = &recordingScanner{}
		if parent {
			a.updateMain("up")
		}
		a.updateMain("k")
		a.updateConfirm("tab")
		cmd := a.updateConfirm("enter")
		if cmd == nil {
			t.Fatal("missing confirmed termination command")
		}
		_, refresh := a.Update(cmd())
		if refresh == nil || a.ancestorSource != nil || a.parentAction || a.screen != mainScreen {
			t.Fatal("successful termination must release tree focus and refresh")
		}
		a.Update(scanMsg{generation: a.generation, result: model.Result{}})
		view := ansi.Strip(a.View().Content)
		if strings.Contains(view, "ANCESTRY · focused") || strings.Contains(view, "ACTION TARGET") || strings.Contains(view, "test-app (42)") {
			t.Fatalf("stale process tree survived refresh: %s", view)
		}
	}
}

func TestFailedTerminationKeepsTreeForReview(t *testing.T) {
	a := ancestryApp()
	_, cmd := a.Update(killMsg{failures: []string{"permission denied"}})
	if cmd != nil || a.ancestorSource == nil || !a.statusError {
		t.Fatal("failure should keep the target available for review")
	}
}
