package tui

import (
	"errors"
	"strings"
	"testing"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

func TestDetailsRefreshReplacesUsagesPreservingSearch(t *testing.T) {
	a := searchApp()
	a.detailLocksOnly = true
	a.detailFilter.SetValue("dll")
	locked := model.Usage{Path: "/build/plugin.dll", Relation: "locked", Lock: "POSIX WRITE"}
	p := *a.detail
	p.Usages = []model.Usage{locked}
	a.Update(scanMsg{generation: a.generation, result: model.Result{Processes: []model.Process{p}, LockDetection: true}})
	if len(a.usageRows) != 1 {
		t.Fatal("initial lock missing")
	}
	p.Usages = []model.Usage{{Path: locked.Path, Relation: "open", Access: "read"}}
	cmd := a.updateDetails("r")
	if cmd == nil || !a.scanning {
		t.Fatal("details refresh did not start scan")
	}
	a.Update(scanMsg{generation: a.generation, result: model.Result{Processes: []model.Process{p}, LockDetection: true}})
	if a.screen != detailScreen || len(a.usageRows) != 0 || a.detailFilter.Value() != "dll" || !a.detailLocksOnly {
		t.Fatal("refresh retained stale locks or lost filters")
	}
	a.updateDetails("l")
	if len(a.usageRows) != 1 || a.usageRows[0].Relation != "open" {
		t.Fatal("updated open usage missing")
	}
	if a.updateDetails("a") == nil || !a.autoRefresh {
		t.Fatal("details auto refresh not enabled")
	}
	_, cmd = a.Update(refreshMsg{periodic: true})
	if cmd == nil || !a.scanning {
		t.Fatal("periodic refresh not started")
	}
	a.updateDetails("a")
	if a.autoRefresh {
		t.Fatal("auto refresh not disabled")
	}
}

func TestDetailsRefreshKeepsSelectionAndRejectsOldScans(t *testing.T) {
	a := searchApp()
	p := *a.detail
	first := model.Usage{Path: "/build/a.dll", Relation: "open"}
	selected := model.Usage{Path: "/build/z.dll", Relation: "open"}
	p.Usages = []model.Usage{selected}
	a.Update(scanMsg{generation: a.generation, result: model.Result{Processes: []model.Process{p}}})
	p.Usages = []model.Usage{first, selected}
	a.Update(scanMsg{generation: a.generation, result: model.Result{Processes: []model.Process{p}}})
	if a.usageRows[a.usageTable.Cursor()] != selected {
		t.Fatal("selected file moved after refresh")
	}
	a.Update(scanMsg{generation: a.generation - 1, result: model.Result{}})
	if a.detail == nil {
		t.Fatal("old scan replaced details")
	}
	a.Update(scanMsg{generation: a.generation, err: errors.New("test scan failure")})
	if a.detail == nil || !strings.Contains(a.View().Content, "test scan failure") {
		t.Fatal("scan failure must retain details and show error")
	}
}

func TestDetailsRefreshAbsentOrReusedIdentity(t *testing.T) {
	for _, reused := range []bool{false, true} {
		a := searchApp()
		result := model.Result{}
		if reused {
			p := *a.detail
			p.Started += "-new"
			result.Processes = []model.Process{p}
		}
		a.filtering = true
		a.Update(scanMsg{generation: a.generation, result: result})
		if a.detail != nil || a.screen != mainScreen || a.filtering || len(a.usageRows) != 0 {
			t.Fatal("stale details remain after process disappearance or PID reuse")
		}
	}
}
