package scanner

import (
	"bytes"
	"math"
	"os"
	"path/filepath"
	"testing"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

// Native CI gates: no OS skips. Each backend must discover the live child,
// supply resource/parent data, and distinguish its native lock evidence.
func TestNativeFeatureContract(t *testing.T) {
	t.Setenv("OFLH_TEST_LOCK", "posix")
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "native locked ü file.dat")
	original := bytes.Repeat([]byte("x"), 4096)
	if err := os.WriteFile(path, original, 0600); err != nil {
		t.Fatal(err)
	}
	child := startHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)
	if !p.MemoryKnown || p.MemoryBytes == 0 {
		t.Fatalf("missing memory: %+v", p)
	}
	if p.ParentPID != os.Getpid() || len(p.Ancestors) == 0 || p.Ancestors[0].PID != os.Getpid() || p.Ancestors[0].Started == "" {
		t.Fatalf("missing actionable parent: %+v", p)
	}
	found := false
	for _, u := range p.Usages {
		found = found || u.Path == path && u.Lock != ""
	}
	if !found {
		t.Fatalf("native lock evidence missing: %+v", p.Usages)
	}
	p = scanPID(t, s, path, child.Process.Pid)
	if !p.CPUKnown || math.IsNaN(p.CPUPercent) || p.CPUPercent < 0 || p.CPUPercent > 100 {
		t.Fatalf("invalid second CPU sample: %+v", p)
	}
	if err := s.Kill(t.Context(), model.Identity{PID: p.PID, Started: p.Started + "-stale"}, true); err == nil {
		t.Fatal("stale process identity accepted")
	}
	// Only our isolated helper is stopped. Probe code must leave contents intact.
	if err := s.Kill(t.Context(), p.Identity, true); err != nil {
		t.Fatal(err)
	}
	_ = child.Wait()
	data, err := os.ReadFile(path)
	if err != nil || !bytes.Equal(data, original) {
		t.Fatalf("file contents changed: %v", err)
	}
}

func TestNativeOpenFileIsNotALock(t *testing.T) {
	t.Setenv("OFLH_TEST_LOCK", "none")
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "unlocked.dat")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}
	child := startHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)
	for _, u := range p.Usages {
		if u.Lock != "" {
			t.Fatalf("ordinary open file misreported as locked: %+v", u)
		}
	}
}
