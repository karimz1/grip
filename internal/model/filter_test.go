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
