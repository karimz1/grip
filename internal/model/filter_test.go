package model

import "testing"

func TestUsageMatchesFilter(t *testing.T) {
	u := Usage{Path: "/build/Über Engine.dll", Relation: "mapped", Access: "read"}
	for _, tc := range []struct {
		query string
		want  bool
	}{
		{"", true},
		{"ENGDLL", true},
		{"über dll mapped", true},
		{"dll write", false},
		{"missing.dll", false},
	} {
		if got := u.MatchesFilter(tc.query); got != tc.want {
			t.Errorf("MatchesFilter(%q) = %v; want %v", tc.query, got, tc.want)
		}
	}
}

func TestWildcardFilters(t *testing.T) {
	p := Process{Name: "MicrosoftHost", User: "alice", Usages: []Usage{{Path: `/build/Microsoft.Core.dll`, Relation: "mapped"}}}
	for _, tc := range []struct {
		query string
		want  bool
	}{
		{"micro*dll", true}, {"MICRO**DLL mapped", true}, {"*.dll", true},
		{"micro*host alice", true}, {"micro*exe", false}, {"dll*micro", false},
		{"mcrdll", false}, {"MiCoDll", true}, {"mcr*dll", false}, {"*", true}, {"micro*missing", false},
		{"micro*dll write", false}, {"micro*alice", false},
	} {
		if got := p.MatchesFilter(tc.query); got != tc.want {
			t.Errorf("%q: got %v, want %v", tc.query, got, tc.want)
		}
	}
	for _, tc := range []struct {
		path, query string
		want        bool
	}{
		{`C:\build\Micro.Core.dll`, `micro*dll`, true},
		{"/build/Über.dll", "ÜB*dll", true},
		{"/build/microdll", "micro*dll", true},
		{"/build/micro/plugins/a.dll", "micro*dll", true},
		{"/build/a[1].dll", "*[1]*dll", true},
		{"/build/a1.dll", "*[1]*dll", false},
	} {
		if got := (Usage{Path: tc.path}).MatchesFilter(tc.query); got != tc.want {
			t.Errorf("%q in %q: got %v", tc.query, tc.path, got)
		}
	}
}

func TestSmartFileSearchDoesNotSkipThroughDirectories(t *testing.T) {
	dir := "/home/karim/projects/Playground/FileLockExampleCli/bin/Debug/net10.0/"
	for _, name := range []string{"FileLockExampleCli.dll", "FileLockExampleCli.deps.json", "FileLockExampleCli.pdb", "FileLockExampleCli.runtimeconfig.json"} {
		u := Usage{Path: dir + name, Relation: "locked", Access: "read/write", Lock: "FLOCK ADVISORY WRITE bytes 0–EOF"}
		for _, q := range []string{"dll", "*dll"} {
			if got := u.MatchesFilter(q); got != (name == "FileLockExampleCli.dll") {
				t.Errorf("%s matched %s = %v", q, name, got)
			}
		}
	}
	for _, q := range []string{"MIMJWT", "mimjwt", "MicroIdMod", "identity", "JWTdll"} {
		if !(Usage{Path: dir + "Microsoft.IdentityModel.JsonWebTokens.dll"}).MatchesFilter(q) {
			t.Errorf("missing CamelCase/fragment match %q", q)
		}
	}
	if !(Usage{Path: `C:\build\some_long_file.dll`}).MatchesFilter("slf") {
		t.Fatal("snake_case initials missing")
	}
	if !(Process{Name: "MicrosoftHost"}).MatchesFilter("MH") {
		t.Fatal("process-name initials missing")
	}
	if (Usage{Path: "/alpha/beta/gamma.json"}).MatchesFilter("abg") {
		t.Fatal("initials must not span path components")
	}
	if (Usage{Path: "/build/dll/config.json"}).SearchScore("dll") >= (Usage{Path: "/build/library.dll"}).SearchScore("dll") {
		t.Fatal("filename hits must rank first")
	}
}
