package tui

import (
	"fmt"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/karimz1/open-file-lock-handle/internal/model"
)

func TestMainFooterAndLastRowStayVisible(t *testing.T) {
	a := searchApp()
	a.updateDetails("esc")
	a.filter.SetValue("")
	a.result.Processes = nil
	for i := range 80 {
		a.result.Processes = append(a.result.Processes, model.Process{Identity: model.Identity{PID: 100 + i}, Name: fmt.Sprintf("process-%02d", i), Usages: []model.Usage{{Path: "/build/log.txt", Relation: "open", Access: "read/write"}}})
	}
	a.refilter()
	for _, size := range []struct{ w, h int }{{140, 32}, {100, 24}, {80, 24}, {48, 20}, {28, 18}} {
		a.Update(tea.WindowSizeMsg{Width: size.w, Height: size.h})
		a.updateMain("end")
		view := ansi.Strip(a.View().Content)
		if !strings.Contains(view, "process-79") || !strings.Contains(view, "q quit") {
			t.Fatalf("selected row or quit hidden at %dx%d:\n%s", size.w, size.h, view)
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
}

func TestInspectorAndResourceSorting(t *testing.T) {
	a := searchApp()
	a.screen = mainScreen
	a.filter.SetValue("")
	a.result.Processes[0].Ancestors = []model.Ancestor{{PID: 20, Name: "launcher"}, {PID: 1, Name: "systemd"}}
	a.result.Processes[0].MemoryKnown = true
	a.result.Processes[0].MemoryBytes = 10 << 20
	a.result.Processes = append(a.result.Processes, model.Process{Identity: model.Identity{PID: 50}, Name: "busy", MemoryKnown: true, MemoryBytes: 1 << 30, CPUKnown: true, CPUPercent: 12.5, Usages: []model.Usage{{Path: "/build/busy"}}})
	a.refilter()
	a.Update(tea.WindowSizeMsg{Width: 170, Height: 36})
	view := ansi.Strip(a.View().Content)
	for _, label := range []string{"1 Processes", "2 Locked files", "ANCESTRY", "launcher", "CPU%", "RAM", "10.0 MiB"} {
		if !strings.Contains(view, label) {
			t.Fatalf("missing %q: %s", label, view)
		}
	}
	a.updateMain("m")
	if a.current().Name != "busy" {
		t.Fatal("memory sort did not put largest process first")
	}
	a.updateMain("c")
	if a.current().Name != "busy" {
		t.Fatal("CPU sort must place known usage first")
	}
	a.updateMain("i")
	if strings.Contains(ansi.Strip(a.View().Content), "ANCESTRY") {
		t.Fatal("panel toggle did not hide it")
	}
	a.updateMain("i")
	a.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
	if strings.Contains(ansi.Strip(a.View().Content), "ANCESTRY") {
		t.Fatal("compact viewport must hide inspector")
	}
}

func TestNarrowFooterKeepsEveryActionAndProjectLink(t *testing.T) {
	for _, size := range []struct{ w, h int }{{160, 32}, {80, 24}, {79, 24}, {60, 20}, {48, 20}, {28, 18}} {
		for _, details := range []bool{false, true} {
			a := searchApp()
			a.result.Warnings = []string{"Some processes could not be inspected."}
			if !details {
				a.screen = mainScreen
			}
			a.Update(tea.WindowSizeMsg{Width: size.w, Height: size.h})
			raw := a.View().Content
			view := ansi.Strip(raw)
			hints := []string{"r refresh", "/ search", "k stop", "x force kill", "R GH", "D ☕ Donate"}
			if details {
				hints = append(hints, "a auto", "l locks only", "↑↓ select", "←→ path", "Esc back")
			} else {
				hints = append(hints, "1/2 tabs", "↑↓ move", "Enter inspect", "Space select", "Ctrl+A all", "Tab/→ tree", "i panel", "m/c RAM/CPU sort", "a auto off", "? help", "q quit")
			}
			for _, hint := range hints {
				if !strings.Contains(view, hint) {
					t.Fatalf("missing %q at %dx%d, details=%v:\n%s", hint, size.w, size.h, details, view)
				}
			}
			if !strings.Contains(raw, repositoryURL) || !strings.Contains(raw, donateURL) {
				t.Fatal("project hyperlinks lost")
			}
			lines := strings.Split(view, "\n")
			if len(lines) > size.h {
				t.Fatalf("height overflow at %dx%d", size.w, size.h)
			}
			for _, line := range lines {
				if ansi.StringWidth(line) > size.w {
					t.Fatalf("width overflow: %q", line)
				}
			}
		}
	}
}
