package model

import (
	"fmt"
	"sort"
	"strings"
)

// Identity binds actions to a process lifetime, not just a reusable PID.
type Identity struct {
	PID     int
	Started string
}

func (i Identity) Key() string { return fmt.Sprintf("%d:%s", i.PID, i.Started) }

type Process struct {
	Identity
	Name       string
	User       string
	Executable string
	CWD        string
	Usages     []Usage
}

type Usage struct {
	Path     string
	Relation string
	Access   string
	Deleted  bool
}

type Result struct {
	Processes []Process
	Warnings  []string
}

// Normalize merges duplicate observations while retaining distinct access evidence.
func (r *Result) Normalize() {
	groups := make(map[string]*Process)
	var order []string
	for _, p := range r.Processes {
		key := p.Key()
		if previous := groups[key]; previous != nil {
			previous.Usages = append(previous.Usages, p.Usages...)
		} else {
			copy := p
			groups[key] = &copy
			order = append(order, key)
		}
	}
	r.Processes = nil
	for _, key := range order {
		p := groups[key]
		seen := make(map[Usage]bool)
		usages := make([]Usage, 0, len(p.Usages))
		for _, u := range p.Usages {
			if !seen[u] {
				seen[u] = true
				usages = append(usages, u)
			}
		}
		p.Usages = usages
		sort.Slice(p.Usages, func(i, j int) bool {
			a, b := p.Usages[i], p.Usages[j]
			if a.Path != b.Path {
				return a.Path < b.Path
			}
			if a.Relation != b.Relation {
				return a.Relation < b.Relation
			}
			return a.Access < b.Access
		})
		r.Processes = append(r.Processes, *p)
	}
	sort.Slice(r.Processes, func(i, j int) bool { return r.Processes[i].PID < r.Processes[j].PID })
}

// MatchesFilter applies ordered-subsequence matching to each whitespace-separated term.
func (p Process) MatchesFilter(query string) bool {
	fields := []string{fmt.Sprint(p.PID), p.Name, p.User, p.Executable, p.CWD}
	for _, u := range p.Usages {
		fields = append(fields, u.Path, u.Relation, u.Access)
	}
	return matchesFields(query, fields)
}

// MatchesFilter searches one observation, so terms must match the same usage.
func (u Usage) MatchesFilter(query string) bool {
	return matchesFields(query, []string{u.Path, u.Relation, u.Access})
}

func matchesFields(query string, fields []string) bool {
	for _, term := range strings.Fields(strings.ToLower(query)) {
		matched := false
		for _, field := range fields {
			if subsequence(term, strings.ToLower(field)) {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}
	return true
}

func subsequence(needle, haystack string) bool {
	n := []rune(needle)
	at := 0
	for _, c := range haystack {
		if at < len(n) && c == n[at] {
			at++
		}
	}
	return at == len(n)
}
