package model

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

type Target struct {
	Path      string
	Directory bool
	Info      os.FileInfo
}

func NewTarget(path string) (Target, error) {
	if path == "" {
		path = "."
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return Target{}, err
	}
	// Resolve the longest existing ancestor, including for deleted-but-open files.
	resolved, err := canonical(abs)
	if err != nil {
		return Target{}, fmt.Errorf("resolve path: %w", err)
	}
	info, err := os.Stat(resolved)
	if err != nil && !os.IsNotExist(err) {
		return Target{}, err
	}
	directory := info != nil && info.IsDir()
	return Target{Path: resolved, Directory: directory, Info: info}, nil
}

func canonical(path string) (string, error) {
	resolved, err := filepath.EvalSymlinks(path)
	if err == nil {
		return filepath.Clean(resolved), nil
	}
	if !os.IsNotExist(err) {
		return "", err
	}
	parent := filepath.Dir(path)
	if parent == path {
		return filepath.Clean(path), nil
	}
	resolved, err = canonical(parent)
	if err != nil {
		return "", err
	}
	return filepath.Join(resolved, filepath.Base(path)), nil
}

func normalize(path string) string {
	path = filepath.Clean(path)
	if runtime.GOOS == "windows" {
		if strings.HasPrefix(path, `\\?\UNC\`) {
			path = `\\` + strings.TrimPrefix(path, `\\?\UNC\`)
		} else {
			path = strings.TrimPrefix(path, `\\?\`)
		}
		path = strings.ToLower(path)
	}
	return path
}

func Contains(parent, child string) bool {
	rel, err := filepath.Rel(normalize(parent), normalize(child))
	return err == nil && !filepath.IsAbs(rel) && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

func (t Target) Matches(path string, info os.FileInfo) bool {
	if t.Directory {
		return Contains(t.Path, path)
	}
	if t.Info != nil && info != nil {
		return os.SameFile(t.Info, info)
	}
	return normalize(t.Path) == normalize(path)
}
