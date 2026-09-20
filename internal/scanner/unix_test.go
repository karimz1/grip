//go:build linux || darwin

package scanner

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestNativeGracefulTermination(t *testing.T) {
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "held.dat")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}
	child := startHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)
	if err := s.Kill(context.Background(), p.Identity, false); err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- child.Wait() }()
	select {
	case <-done:
	case <-time.After(10 * time.Second):
		t.Fatal("SIGTERM did not stop helper")
	}
}
