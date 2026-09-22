package model

import (
	"fmt"
	"sort"
	"strings"
	"unicode"
)

// Identity binds actions to a process lifetime, not just a reusable PID.
type Identity struct {
	PID     int
	Started string
}

func (i Identity) Key() string { return fmt.Sprintf("%d:%s", i.PID, i.Started) }

type Process struct {
	Identity
	Name        string
	User        string
	Executable  string
	CWD         string
	Usages      []Usage
	ParentPID   int
	Ancestors   []Ancestor // Nearest parent first; observations are best effort.
	MemoryBytes uint64
	MemoryKnown bool
	CPUPercent  float64 // Share of total machine CPU capacity between scans.
	CPUKnown    bool
}

type Ancestor struct {
	Started string
	PID     int
	Name    string
}

type Usage struct {
	Path     string
	Relation string
	Access   string
	Deleted  bool
	Lock     string // Confirmed kernel lock type, mode and byte range; empty for ordinary usage.
}

type Result struct {
	Processes     []Process
	Warnings      []string
	LockDetection bool
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
			if a.Access != b.Access {
				return a.Access < b.Access
			}
			return a.Lock < b.Lock
		})
		r.Processes = append(r.Processes, *p)
	}
	sort.Slice(r.Processes, func(i, j int) bool { return r.Processes[i].PID < r.Processes[j].PID })
}

// MatchesFilter applies fragment/CamelCase matching with optional wildcards.
func (p Process) MatchesFilter(query string) bool {
	fields := []string{fmt.Sprint(p.PID), p.Name, p.User, p.Executable, p.CWD}
	for _, u := range p.Usages {
		fields = append(fields, u.Path, u.Relation, u.Access, u.Lock)
	}
	return matchesFields(query, fields)
}

// MatchesFilter searches one observation, so terms must match the same usage.
func (u Usage) MatchesFilter(query string) bool {
	return matchesFields(query, []string{u.Path, u.Relation, u.Access, u.Lock})
}

func matchesFields(query string, fields []string) bool {
	for _, term := range strings.Fields(strings.ToLower(query)) {
		matched := false
		for _, field := range fields {
			if matchesTerm(term, field) {
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

// Match each wildcard-separated chunk using the same fragment/CamelCase rules.
// Taking the earliest ending match leaves the most space for subsequent chunks.
func matchesTerm(term, field string) bool {
	term = strings.ToLower(term)
	if !strings.Contains(term, "*") && strings.Contains(strings.ToLower(field), term) {
		return true
	}
	chars := []rune(field)
	start := 0
	for _, part := range strings.Split(term, "*") {
		end := smartMatchEnd([]rune(part), chars, start)
		if end < 0 {
			return false
		}
		start = end
	}
	return true
}

// Return the earliest exclusive end of a contiguous fragment or abbreviation.
// Abbreviation gaps stay within one path component and land at word boundaries
// or literal punctuation, so FLEC.dll can match FileLockExampleCli.dll. Keep the
// original field boundaries when matching after a wildcard.
func smartMatchEnd(query, chars []rune, start int) int {
	if len(query) == 0 {
		return start
	}
	best := len(chars) + 1
	for i := start; i+len(query) <= len(chars); i++ {
		matched := true
		for j, q := range query {
			if unicode.ToLower(chars[i+j]) != q {
				matched = false
				break
			}
		}
		if matched {
			best = i + len(query)
			break
		}
	}
	previous := make([]bool, len(chars))
	for qi, q := range query {
		next := make([]bool, len(chars))
		earlier := false
		for j := start; j < len(chars) && j < best; j++ {
			c := chars[j]
			if c == '/' || c == '\\' {
				earlier = false
				continue
			}
			punctuation := !unicode.IsLetter(c) && !unicode.IsDigit(c)
			boundary := j == 0 || !unicode.IsLetter(chars[j-1]) && !unicode.IsDigit(chars[j-1]) || unicode.IsUpper(c) && (unicode.IsLower(chars[j-1]) || j+1 < len(chars) && unicode.IsLower(chars[j+1]))
			if unicode.ToLower(c) == q {
				if qi == 0 {
					next[j] = boundary || punctuation
				} else {
					next[j] = j > start && previous[j-1] || (boundary || punctuation) && earlier
				}
			}
			earlier = earlier || previous[j]
		}
		previous = next
	}
	for j := start; j < best && j < len(chars); j++ {
		if previous[j] {
			return j + 1
		}
	}
	if best <= len(chars) {
		return best
	}
	return -1
}
