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

func TestAbbreviationsWithPunctuationAndWildcards(t *testing.T) {
	for _, prefix := range []string{"/build/", `C:\build\`} {
		for _, name := range []string{"FileLockExampleCli.dll", "FileLockExampleCli.deps.json", "FileLockExampleCli.runtimeconfig.json"} {
			u := Usage{Path: prefix + name}
			for _, query := range []string{"FLEC", "FilLoExaCl", "FLEC.", "FLEC*", "*FLEC*", "FLEC.*", "FilLoExaCl.*", "flec**"} {
				if !u.MatchesFilter(query) {
					t.Errorf("%q did not match %q", query, u.Path)
				}
			}
			for _, query := range []string{"FLEC.dll", "FLEC*.dll"} {
				if u.MatchesFilter(query) != (name == "FileLockExampleCli.dll") {
					t.Errorf("incorrect extension filtering: %q in %q", query, u.Path)
				}
			}
			if u.MatchesFilter("FLEC*json") != (name != "FileLockExampleCli.dll") {
				t.Errorf("incorrect JSON filtering: %s", u.Path)
			}
		}
	}
	for _, tc := range []struct {
		field, query string
		want         bool
	}{
		{"FileLockExampleCli", "FLEC.", false},
		{"FileLockExampleCli", "FLEC*", true},
		{"FileLockExampleCli.dll", "FLEC*exe", false},
		{"FileLockExampleCli.dll", "dll*FLEC", false},
		{"FileLockExampleCli.dll", "FLEC*FLEC", false},
		{"/File/Lock/Example/Cli.dll", "FLEC*", false},
		{"/FileLock/ExampleCli.dll", "FL*EC.dll", true},
		{"ÜberFileLock.dll", "ÜFL.*", true},
		{"FileLockExampleCli.deps.json", "FLEC.d.j", true},
	} {
		if got := matchesTerm(tc.query, tc.field); got != tc.want {
			t.Errorf("%q in %q = %v, want %v", tc.query, tc.field, got, tc.want)
		}
	}
}

func TestGeneralAbbreviatedStems(t *testing.T) {
	for _, tc := range []struct{ stem, short, long string }{
		{"HttpServerFactory", "HSF", "HtSerFa"},
		{"XMLDocumentReader", "XDR", "XMLDocRea"},
		{"customer_order_service", "cos", "custordser"},
		{"ÜberFileReader", "ÜFR", "ÜberFiRea"},
	} {
		for _, extension := range []string{".dll", ".deps.json", ".cs"} {
			field := "/project/build/" + tc.stem + extension
			for _, q := range []string{tc.short, tc.long} {
				for _, pattern := range []string{q, q + ".", q + "*", q + ".*", q + "*" + extension} {
					if !(Usage{Path: field}).MatchesFilter(pattern) {
						t.Errorf("%q failed for %q", pattern, field)
					}
				}
				if !(Process{Name: tc.stem}).MatchesFilter(q + "*") {
					t.Errorf("process abbreviation failed: %s", tc.stem)
				}
				if (Usage{Path: field}).MatchesFilter(q + "*.missing") {
					t.Errorf("matched absent extension: %s", field)
				}
			}
		}
	}
}
