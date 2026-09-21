//go:build linux

package scanner

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

func TestLinuxMetricsAndIdentity(t *testing.T) {
	root := t.TempDir()
	s := &native{root: root}
	write := func(pid int, name string, parent int, started string, ticks int) {
		t.Helper()
		dir := filepath.Join(root, fmt.Sprint(pid))
		if err := os.MkdirAll(dir, 0700); err != nil {
			t.Fatal(err)
		}
		fields := strings.Fields("S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0")
		fields[1], fields[11], fields[19], fields[21] = fmt.Sprint(parent), fmt.Sprint(ticks), started, "10"
		if err := os.WriteFile(filepath.Join(dir, "stat"), []byte(fmt.Sprintf("%d (%s) %s", pid, name, strings.Join(fields, " "))), 0600); err != nil {
			t.Fatal(err)
		}
	}
	total := func(n int) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(root, "stat"), []byte(fmt.Sprintf("cpu %d 0 0 0 0 0 0 0 999 999\n", n)), 0600); err != nil {
			t.Fatal(err)
		}
	}
	write(1, "init", 0, "1", 1)
	write(42, "app (worker)", 1, "10", 100)
	total(1000)
	result := func() model.Result {
		return model.Result{Processes: []model.Process{{Identity: model.Identity{PID: 42, Started: "10"}}}}
	}
	r := result()
	s.enrich(context.Background(), &r)
	p := r.Processes[0]
	if p.CPUKnown || !p.MemoryKnown || p.MemoryBytes != 10*uint64(os.Getpagesize()) || p.ParentPID != 1 || len(p.Ancestors) != 1 {
		t.Fatalf("bad initial metrics: %+v", p)
	}
	write(42, "app (worker)", 1, "10", 150)
	total(1200)
	r = result()
	s.enrich(context.Background(), &r)
	if !r.Processes[0].CPUKnown || r.Processes[0].CPUPercent != 25 {
		t.Fatalf("bad CPU delta: %+v", r.Processes[0])
	}
	write(42, "replacement", 1, "20", 500)
	r = result()
	s.enrich(context.Background(), &r)
	if r.Processes[0].MemoryKnown || r.Processes[0].CPUKnown {
		t.Fatal("metrics attached to reused PID")
	}
	if _, ok := parseProcStats("broken"); ok {
		t.Fatal("malformed stat accepted")
	}
}
