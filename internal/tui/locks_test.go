package tui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/karimz1/open-file-lock-handle/internal/model"
)

func TestLockedTabFilteringAndActions(t *testing.T) {
	a := searchApp()
	a.updateDetails("esc")
	a.result.LockDetection = true
	a.result.Processes[0].Usages = append(a.result.Processes[0].Usages,
		model.Usage{Path: "/build/first.lock", Relation: "locked", Lock: "FLOCK WRITE"},
		model.Usage{Path: "/build/second.lock", Relation: "locked", Lock: "POSIX READ"})
	a.updateMain("2")
	if len(a.visible) != 2 {
		t.Fatal("expected separate lock rows only")
	}
	a.filter.SetValue("second")
	a.refilter()
	if len(a.visible) != 1 || a.visible[0].Usages[0].Path != "/build/second.lock" {
		t.Fatal("search matched another usage")
	}
	a.updateMain("enter")
	if len(a.usageRows) != 1 || a.usageRows[0].Lock == "" {
		t.Fatal("details must show selected lock")
	}
	a.updateDetails("esc")
	a.filter.SetValue("")
	a.refilter()
	a.updateMain("K")
	if len(a.pending) != 1 {
		t.Fatal("bulk action must deduplicate processes with multiple locks")
	}
	a.updateConfirm("esc")
	a.updateMain("1")
	if len(a.visible) != 1 || len(a.visible[0].Usages) != 4 {
		t.Fatal("original process view changed")
	}
}

func TestLockedTabRefreshAndLayout(t *testing.T) {
	a := searchApp()
	a.updateDetails("esc")
	a.result.LockDetection = true
	for i := range 40 {
		a.result.Processes[0].Usages = append(a.result.Processes[0].Usages, model.Usage{Path: "/build/" + strings.Repeat("x", i+1), Relation: "locked", Lock: "FLOCK WRITE"})
	}
	a.updateMain("2")
	a.updateMain("end")
	last := a.current().Usages[0]
	a.Update(scanMsg{generation: a.generation, result: a.result})
	if a.current().Usages[0] != last {
		t.Fatal("refresh lost selected file")
	}
	for _, size := range []struct{ w, h int }{{120, 30}, {80, 24}, {48, 20}, {28, 18}} {
		a.Update(tea.WindowSizeMsg{Width: size.w, Height: size.h})
		view := ansi.Strip(a.View().Content)
		if !strings.Contains(view, "q quit") || !strings.Contains(view, "FILE") {
			t.Fatalf("missing table or footer: %s", view)
		}
		if len(strings.Split(view, "\n")) > size.h {
			t.Fatal("height overflow")
		}
		for _, line := range strings.Split(view, "\n") {
			if ansi.StringWidth(line) > size.w {
				t.Fatal("width overflow")
			}
		}
	}
	a.result = model.Result{}
	a.refilter()
	a.width = 100
	if !strings.Contains(ansi.Strip(a.View().Content), "unavailable on this platform") {
		t.Fatal("unsupported platform must not claim no locks")
	}
}

func TestDetailsLocksOnlyKeepsSearch(t *testing.T) {
	a := searchApp()
	a.result.LockDetection = true
	a.detail.Usages = append(a.detail.Usages,
		model.Usage{Path: "/build/Engine.dll", Relation: "locked", Access: "read/write", Lock: "FLOCK WRITE"},
		model.Usage{Path: "/build/Engine.dll", Relation: "locked", Access: "read", Lock: "POSIX READ"},
		model.Usage{Path: "/build/other.json", Relation: "locked", Access: "write", Lock: "FLOCK WRITE"})
	a.detailFilter.SetValue("eng*dll")
	a.updateDetails("l")
	if !a.detailLocksOnly || len(a.usageRows) != 2 || a.detailFilter.Value() != "eng*dll" {
		t.Fatal("locks only must intersect with search")
	}
	view := ansi.Strip(a.View().Content)
	if !strings.Contains(view, "1 locked files") || !strings.Contains(view, "LOCKS ONLY") {
		t.Fatalf("missing distinct-file count: %s", view)
	}
	a.updateDetails("l")
	if len(a.usageRows) != 3 {
		t.Fatal("toggle must restore non-lock matches")
	}
	a.detailFilter.SetValue("config")
	a.updateDetails("l")
	if len(a.usageRows) != 0 || !strings.Contains(ansi.Strip(a.View().Content), "No confirmed locks") {
		t.Fatal("missing locks empty state")
	}
	a.updateDetails("q")
	a.updateMain("enter")
	if a.detailLocksOnly {
		t.Fatal("opening another snapshot should reset locks mode")
	}
}

func TestDLLSearchInLockedDetails(t *testing.T) {
	a := searchApp()
	a.result.LockDetection = true
	a.detail.Usages = nil
	for _, name := range []string{"FileLockExampleCli.dll", "FileLockExampleCli.deps.json", "FileLockExampleCli.pdb", "FileLockExampleCli.runtimeconfig.json"} {
		a.detail.Usages = append(a.detail.Usages, model.Usage{Path: "/home/karim/projects/Playground/FileLockExampleCli/bin/Debug/net10.0/" + name, Relation: "locked", Access: "read/write", Lock: "FLOCK WRITE"})
	}
	a.updateDetails("l")
	a.updateDetails("/")
	typeSearch(a, "dll")
	if len(a.usageRows) != 1 || !strings.HasSuffix(a.usageRows[0].Path, ".dll") {
		t.Fatal("plain dll search must exclude json/pdb in shared directories")
	}
}
