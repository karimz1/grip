package model

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestPathContainment(t *testing.T) {
	base := t.TempDir()
	for _, tc := range []struct {
		path string
		want bool
	}{{base, true}, {filepath.Join(base, "sub", "file.dll"), true}, {base + "-other", false}, {filepath.Join(base, "..", "other"), false}} {
		if Contains(base, tc.path) != tc.want {
			t.Errorf("Contains(%q,%q)", base, tc.path)
		}
	}
}

func TestTargetCanonicalAndIdentity(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "file ü with spaces.dll")
	if err := os.WriteFile(path, []byte("test"), 0600); err != nil {
		t.Fatal(err)
	}
	target, err := NewTarget(path)
	if err != nil {
		t.Fatal(err)
	}
	if !filepath.IsAbs(target.Path) || target.Directory {
		t.Fatal("incorrect file target")
	}
	alias := filepath.Join(dir, "alias")
	if err := os.Link(path, alias); err == nil {
		info, _ := os.Stat(alias)
		if !target.Matches(alias, info) {
			t.Fatal("hardlink identity was missed")
		}
	}
	if runtime.GOOS != "windows" {
		link := filepath.Join(dir, "link")
		if err := os.Symlink(path, link); err != nil {
			t.Fatal(err)
		}
		other, err := NewTarget(link)
		if err != nil || other.Path != target.Path {
			t.Fatalf("symlink: %+v %v", other, err)
		}
	}
	missing, err := NewTarget(filepath.Join(dir, "missing.dll"))
	if err != nil || !missing.Matches(missing.Path, nil) {
		t.Fatalf("missing target: %v", err)
	}
}

func TestWindowsPathSemantics(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("Windows path rules")
	}
	for _, tc := range []struct {
		parent, child string
		want          bool
	}{
		{`C:\Build`, `c:\build\Plugins\foo.dll`, true},
		{`C:\Build`, `C:\Builder\foo.dll`, false},
		{`C:\Build`, `D:\Build\foo.dll`, false},
		{`\\server\share\build`, `\\SERVER\share\build\foo.dll`, true},
		{`C:\Build`, `\\?\C:\Build\foo.dll`, true},
	} {
		if Contains(tc.parent, tc.child) != tc.want {
			t.Errorf("Contains(%q,%q)", tc.parent, tc.child)
		}
	}
}
