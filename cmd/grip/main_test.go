package main

import (
	"bytes"
	"strings"
	"testing"
)

func TestCLI(t *testing.T) {
	for _, tc := range []struct {
		args []string
		code int
		text string
	}{{[]string{"--version"}, 0, "grip dev"}, {[]string{"--help"}, 0, "Usage: grip [PATH]"}, {[]string{"a", "b"}, 2, "expected one path"}, {[]string{"--missing"}, 2, "flag provided but not defined"}} {
		var out bytes.Buffer
		if code := run(tc.args, &out, &out); code != tc.code || !strings.Contains(out.String(), tc.text) {
			t.Errorf("%v: code=%d output=%q", tc.args, code, out.String())
		}
	}
}
