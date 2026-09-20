package tui

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/karimz1/grip/internal/model"
)

type recordingScanner struct {
	calls []model.Identity
	force bool
	err   error
}

func (s *recordingScanner) Scan(context.Context, model.Target) (model.Result, error) {
	return model.Result{}, nil
}
func (s *recordingScanner) Kill(_ context.Context, id model.Identity, force bool) error {
	s.calls = append(s.calls, id)
	s.force = force
	return s.err
}

func TestKillConfirmationAndFailure(t *testing.T) {
	a := searchApp()
	backend := &recordingScanner{}
	a.backend = backend
	a.prepareKill("x")
	if !a.force || a.confirm || len(a.pending) != 1 {
		t.Fatal("force action must start unconfirmed")
	}
	if cmd := a.updateConfirm("enter"); cmd != nil || len(backend.calls) != 0 {
		t.Fatal("default Enter must cancel")
	}
	a.prepareKill("x")
	a.updateConfirm("esc")
	if len(backend.calls) != 0 {
		t.Fatal("Escape executed an action")
	}
	a.prepareKill("x")
	a.updateConfirm("tab")
	backend.err = errors.New("permission denied")
	cmd := a.updateConfirm("enter")
	if cmd == nil {
		t.Fatal("confirmed action missing")
	}
	result := cmd().(killMsg)
	if len(backend.calls) != 1 || !backend.force || len(result.failures) != 1 || !strings.Contains(result.failures[0], "permission denied") {
		t.Fatalf("wrong result: %+v", result)
	}
}

func TestBulkIncludesSelectedHiddenProcesses(t *testing.T) {
	a := searchApp()
	a.screen = mainScreen
	other := model.Process{Identity: model.Identity{PID: 99, Started: "456"}, Name: "hidden"}
	a.result.Processes = append(a.result.Processes, other)
	a.selected[other.Key()] = true
	a.prepareKill("K")
	if len(a.pending) != 1 || a.pending[0].PID != 99 {
		t.Fatal("hidden selected process missing")
	}
	if !strings.Contains(strings.Join(a.confirmView(76), "\n"), "hidden") {
		t.Fatal("confirmation hides affected process")
	}
}

func TestStaleScanCannotReplaceResults(t *testing.T) {
	a := searchApp()
	a.generation = 3
	a.Update(scanMsg{generation: 2, result: model.Result{}})
	if len(a.result.Processes) != 1 {
		t.Fatal("stale scan overwrote results")
	}
}
