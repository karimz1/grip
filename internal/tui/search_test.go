package tui

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"

	tea "charm.land/bubbletea/v2"
	"github.com/karimz1/open-file-lock-handle/internal/model"
)

func searchApp() *App {
	a := New(context.Background(), nil, model.Target{Path: "/build"}, "dev")
	a.result.Processes = []model.Process{{
		Identity: model.Identity{PID: 42, Started: "123"}, Name: "test-app",
		Usages: []model.Usage{
			{Path: "/build/Engine.dll", Relation: "mapped", Access: "read"},
			{Path: "/build/config.json", Relation: "open", Access: "read/write"},
		},
	}}
	a.filter.SetValue("test-app")
	a.refilter()
	a.updateMain("enter")
	return a
}

func typeSearch(a *App, text string) {
	for _, r := range text {
		a.Update(tea.KeyPressMsg{Code: r, Text: string(r)})
	}
}

func TestDetailsSearchWorkflow(t *testing.T) {
	a := searchApp()
	a.offset = 30
	a.updateDetails("/")
	typeSearch(a, "engdll")
	if a.offset != 0 || a.detailFilter.Value() != "engdll" {
		t.Fatal("search must reset scrolling and accept live input")
	}
	view := ansi.Strip(a.View().Content)
	if !strings.Contains(view, "Engine.dll") || strings.Contains(view, "config.json") || !strings.Contains(view, "1 of 2") {
		t.Fatalf("unexpected filtered details: %s", view)
	}
	if a.filter.Value() != "test-app" || len(a.visible) != 1 {
		t.Fatal("usage search changed the process filter")
	}
	a.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	if a.filtering || a.screen != detailScreen {
		t.Fatal("Enter should apply the search and remain in details")
	}
	a.updateDetails("/")
	typeSearch(a, "missing")
	if !strings.Contains(ansi.Strip(a.View().Content), "No usage entries") {
		t.Fatal("missing empty state")
	}
	a.Update(tea.KeyPressMsg{Code: tea.KeyEscape})
	if a.detailFilter.Value() != "engdll" || a.filtering {
		t.Fatal("Escape while editing should restore the applied query")
	}
	a.updateDetails("esc")
	if a.detailFilter.Value() != "" || a.screen != detailScreen {
		t.Fatal("Escape should clear an applied query first")
	}
	a.updateDetails("esc")
	if a.screen != mainScreen || a.filter.Value() != "test-app" {
		t.Fatal("second Escape should return to the unchanged process list")
	}
}

func TestDetailsSearchPasteAndActionKeys(t *testing.T) {
	a := searchApp()
	a.updateDetails("/")
	a.Update(tea.PasteMsg{Content: "engine.dll"})
	if a.detailFilter.Value() != "engine.dll" || a.filter.Value() != "test-app" {
		t.Fatal("paste must go to the usage search")
	}
	typeSearch(a, "kxqr")
	if a.screen != detailScreen || a.stopping || len(a.pending) != 0 {
		t.Fatal("typing action keys in search must not act on the process")
	}
	a.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	a.updateDetails("q")
	a.updateMain("enter")
	if a.detailFilter.Value() != "" {
		t.Fatal("opening a process should start with all its usages")
	}
}

func TestDetailsSearchStaysVisibleWhileScrolling(t *testing.T) {
	a := searchApp()
	for range 30 {
		a.detail.Usages = append(a.detail.Usages, model.Usage{Path: "/build/another.dll", Relation: "mapped"})
	}
	a.updateDetails("/")
	typeSearch(a, "dll")
	a.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	a.updateDetails("end")
	view := ansi.Strip(a.View().Content)
	if !strings.Contains(view, "/ dll") || !strings.Contains(view, "another.dll") {
		t.Fatalf("search and matching paths should remain visible when scrolled: %s", view)
	}
}

func TestUsageTableNavigationAndResize(t *testing.T) {
	a := searchApp()
	for i := range 60 {
		a.detail.Usages = append(a.detail.Usages, model.Usage{Path: fmt.Sprintf("/build/plugins/component-%02d.dll", i), Relation: "mapped", Access: "execute"})
	}
	a.rebuildUsages(true)
	for _, size := range []struct{ w, h int }{{120, 35}, {80, 24}, {48, 20}, {28, 18}, {120, 35}} {
		a.Update(tea.WindowSizeMsg{Width: size.w, Height: size.h})
		a.View() // Establish the viewport size before page navigation.
		a.updateDetails("end")
		view := ansi.Strip(a.View().Content)
		if !strings.Contains(view, "FILE") || !strings.Contains(view, "62 / 62") {
			t.Fatalf("header or final row position missing at %dx%d: %s", size.w, size.h, view)
		}
		if a.usageTable.Cursor() != 61 {
			t.Fatal("end must select the final usage")
		}
		lines := strings.Split(view, "\n")
		if len(lines) > size.h {
			t.Fatalf("%d lines exceed terminal height %d:\n%s", len(lines), size.h, view)
		}
		for _, line := range lines {
			if ansi.StringWidth(line) > size.w {
				t.Fatalf("line exceeds terminal width: %q", line)
			}
		}
	}
	a.updateDetails("/")
	typeSearch(a, "dll")
	a.Update(tea.KeyPressMsg{Code: tea.KeyDown})
	if a.usageTable.Cursor() != 1 || !a.filtering {
		t.Fatal("arrow keys must browse matches without leaving search")
	}
}

func TestUsageTableLongPathPreview(t *testing.T) {
	a := searchApp()
	a.detail.Usages = []model.Usage{{Path: "/build/" + strings.Repeat("long-directory/", 30) + "last.dll", Relation: "mapped"}}
	a.rebuildUsages(true)
	for range 40 {
		a.updateDetails("right")
	}
	view := ansi.Strip(a.View().Content)
	if !strings.Contains(view, "last.dll") {
		t.Fatal("path paging must make the full path accessible")
	}
	a.updateDetails("/")
	typeSearch(a, "absent")
	if a.usageTable.SelectedRow() != nil {
		t.Fatal("empty results must not retain a selected usage")
	}
}

func TestWildcardSearchInMainAndDetails(t *testing.T) {
	a := searchApp()
	a.updateDetails("esc")
	a.filter.SetValue("")
	a.refilter()
	view := ansi.Strip(a.View().Content)
	if !strings.Contains(view, "╭") || !strings.Contains(view, "/ Search PID") {
		t.Fatalf("startup search box missing: %s", view)
	}
	a.updateMain("/")
	typeSearch(a, "eng*dll")
	if len(a.visible) != 1 {
		t.Fatal("main wildcard should match usage")
	}
	a.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	a.updateMain("enter")
	if a.detailFilter.Value() != "eng*dll" {
		t.Fatal("main wildcard was not inherited")
	}
	a.updateDetails("/")
	if len(a.usageRows) != 1 || !strings.Contains(a.usageRows[0].Path, "Engine.dll") {
		t.Fatal("details wildcard did not match")
	}
	typeSearch(a, "*absent")
	if len(a.usageRows) != 0 || !strings.Contains(ansi.Strip(a.View().Content), "No usage entries") {
		t.Fatal("missing wildcard empty state")
	}
	a.Update(tea.KeyPressMsg{Code: tea.KeyEscape})
	a.updateDetails("q")
	a.updateMain("/")
	typeSearch(a, "*absent")
	if len(a.visible) != 0 {
		t.Fatal("main wildcard should show no matches")
	}
	a.Update(tea.KeyPressMsg{Code: tea.KeyEscape})
	if a.filter.Value() != "eng*dll" || len(a.visible) != 1 {
		t.Fatal("cancel should restore previous search")
	}
}

func TestMainSearchCountsAndInheritedFileTerms(t *testing.T) {
	a := searchApp()
	a.updateDetails("q")
	p := &a.result.Processes[0]
	p.Usages = []model.Usage{{Path: "/build/aws-core.dll"}, {Path: "/build/aws-s3.dll"}, {Path: "/build/other.dll"}}
	a.filter.SetValue("42 aws*dll")
	a.refilter()
	if len(a.visible) != 1 || len(a.visible[0].Usages) != 2 {
		t.Fatal("main should retain only the matching usages")
	}
	if !strings.Contains(ansi.Strip(a.View().Content), "+1") {
		t.Fatal("+ count must reflect filtered usages")
	}
	a.updateMain("enter")
	if a.detailFilter.Value() != "aws*dll" || len(a.usageRows) != 2 || len(a.detail.Usages) != 3 {
		t.Fatal("details must inherit file terms and retain original usages")
	}
	a.updateDetails("esc")
	if len(a.usageRows) != 3 {
		t.Fatal("clearing details search must restore all usages")
	}
	a.updateDetails("esc")
	if a.filter.Value() != "42 aws*dll" {
		t.Fatal("details clear changed main filter")
	}
}

func TestLongFilenameUsesAvailableWidth(t *testing.T) {
	a := searchApp()
	name := "Microsoft.IdentityModel.JsonWebTokens.dll"
	a.detail.Usages = []model.Usage{{Path: "/build/" + name, Relation: "mapped", Access: "read"}}
	a.Update(tea.WindowSizeMsg{Width: 170, Height: 36})
	if a.usageTable.Columns()[0].Width < ansi.StringWidth(name) {
		t.Fatal("file column wastes available space")
	}
	view := ansi.Strip(a.View().Content)
	if strings.Count(view, name) < 3 {
		t.Fatalf("filename should appear in table, filename preview and full path: %s", view)
	}
	a.detail.Usages[0].Path = "/build/" + strings.Repeat("LongName", 30) + ".dll"
	for _, size := range []struct{ w, h int }{{28, 18}, {48, 20}, {80, 22}, {120, 24}} {
		a.Update(tea.WindowSizeMsg{Width: size.w, Height: size.h})
		view = ansi.Strip(a.View().Content)
		if len(strings.Split(view, "\n")) > size.h {
			t.Fatalf("path preview overflows at %dx%d: %s", size.w, size.h, view)
		}
		for _, line := range strings.Split(view, "\n") {
			if ansi.StringWidth(line) > size.w {
				t.Fatal("width overflow")
			}
		}
	}
}
