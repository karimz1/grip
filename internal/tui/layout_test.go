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
