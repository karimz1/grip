package tui

import (
	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"strings"
	"testing"
)

func TestProcessAccessSummary(t *testing.T) {
	for _, tc := range []struct {
		usages []model.Usage
		want   string
	}{
		{[]model.Usage{{Relation: "cwd", Access: "directory"}}, "cwd"},
		{[]model.Usage{{Relation: "cwd", Access: "directory"}, {Access: "read/write"}}, "read/write"},
		{[]model.Usage{{Access: "read"}, {Access: "write"}}, "read/write"},
		{[]model.Usage{{Access: "read"}, {Access: "execute"}}, "read"},
		{[]model.Usage{{Access: "execute"}}, "execute"},
		{[]model.Usage{{Relation: "mapped", Access: "unknown"}}, "mapped"},
		{nil, "unknown"},
	} {
		if got := processAccess(model.Process{Usages: tc.usages}); got != tc.want {
			t.Fatalf("got %q, want %q", got, tc.want)
		}
	}
	a := searchApp()
	a.updateDetails("q")
	a.width = 170
	a.height = 36
	if view := ansi.Strip(a.View().Content); !strings.Contains(view, "ACCESS") || !strings.Contains(view, "read/write") {
		t.Fatalf("missing access: %s", view)
	}
	a.filter.SetValue("eng*dll")
	a.refilter()
	if got := processAccess(*a.current()); got != "read" {
		t.Fatalf("filtered access = %q", got)
	}
}

func TestOnlyNumberKeysSwitchTabs(t *testing.T) {
	a := searchApp()
	a.updateDetails("q")
	for _, key := range []rune{tea.KeyTab} {
		a.Update(tea.KeyPressMsg{Code: key})
		if a.lockedTab {
			t.Fatal("Tab switched views")
		}
	}
	a.updateMain("2")
	for _, key := range []string{"tab", "shift+tab"} {
		a.updateMain(key)
		if !a.lockedTab {
			t.Fatal("Tab switched views")
		}
	}
	a.updateMain("1")
	if a.lockedTab {
		t.Fatal("1 did not switch views")
	}
	a.prepareKill("k")
	a.updateConfirm("tab")
	if !a.confirm {
		t.Fatal("Tab must still choose the confirmation action")
	}
}
