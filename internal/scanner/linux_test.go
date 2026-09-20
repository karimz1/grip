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
