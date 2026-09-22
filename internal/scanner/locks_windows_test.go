//go:build windows

package scanner

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSharingProbeDoesNotTreatErrorsAsLocks(t *testing.T) {
	path := filepath.Join(t.TempDir(), "missing")
	if sharingConflict(path) != "" {
		t.Fatal("missing file is not a sharing conflict")
	}
	if err := os.WriteFile(path, []byte("unchanged"), 0600); err != nil {
		t.Fatal(err)
	}
	if sharingConflict(path) != "" {
		t.Fatal("unopened file is not locked")
	}
	b, err := os.ReadFile(path)
	if err != nil || string(b) != "unchanged" {
		t.Fatal("probe changed file")
	}
}
