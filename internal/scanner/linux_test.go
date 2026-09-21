//go:build linux

package scanner

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDeletedFileAndReplacement(t *testing.T) {
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "deleted file.bin")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}
	child := startHelper(t, path)
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	p := scanPID(t, s, path, child.Process.Pid)
	found := false
	for _, u := range p.Usages {
		found = found || u.Deleted
	}
	if !found {
		t.Fatal("deleted usage missing")
	}
	if err := os.WriteFile(path, []byte("replacement"), 0600); err != nil {
		t.Fatal(err)
	}
	// Directory scans still show the old inode; exact scans must not attribute it to the replacement.
	p = scanPID(t, s, dir, child.Process.Pid)
	found = false
	for _, u := range p.Usages {
		found = found || u.Deleted
	}
	if !found {
		t.Fatal("old inode should remain visible for directory")
	}
}

func TestConfirmedLocksOnly(t *testing.T) {
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "locked file")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}
	unlocked := startHelper(t, path)
	for _, u := range scanPID(t, s, path, unlocked.Process.Pid).Usages {
		if u.Lock != "" {
			t.Fatal("ordinary open file reported as locked")
		}
	}
	t.Setenv("OFLH_TEST_LOCK", "1")
	locked := startHelper(t, path)
	found := false
	for _, u := range scanPID(t, s, path, locked.Process.Pid).Usages {
		if u.Path == path && u.Relation == "locked" && u.Lock == "FLOCK ADVISORY WRITE bytes 0–EOF" {
			found = true
		}
	}
	if !found {
		t.Fatal("real flock not detected")
	}
}

func TestFDLocksExcludeWaitersAndLeases(t *testing.T) {
	input := "lock: 1: -> POSIX ADVISORY WRITE 42 00:13:12 0 EOF\nlock: 2: LEASE ACTIVE READ 42 00:13:12 0 EOF\nlock: 3: OFDLCK ADVISORY READ -1 00:13:12 4 8\nlock: broken\n"
	locks := fdLocks(input)
	if len(locks) != 1 || locks[0] != "OFDLCK ADVISORY READ bytes 4–8" {
		t.Fatalf("unexpected locks: %v", locks)
	}
}
