package model

import (
	"fmt"
	"strings"
)

// Rank direct filename/name hits above initials and directory-only matches.
// Membership is still decided by MatchesFilter, so ranking cannot add results.
func fieldScore(term, field string) int {
	lower := strings.ToLower(field)
	switch {
	case lower == term:
		return 100
	case strings.HasPrefix(lower, term):
		return 80
	case strings.Contains(lower, term):
		return 60
	case matchesTerm(term, field):
		return 30
	default:
		return 0
	}
}
func searchScore(query string, names, metadata []string) int {
	score := 0
	for _, term := range strings.Fields(strings.ToLower(query)) {
		best := 0
		for _, name := range names {
			best = max(best, 2*fieldScore(term, name))
		}
		for _, field := range metadata {
			best = max(best, fieldScore(term, field))
		}
		score += best
	}
	return score
}
func fileName(path string) string {
	path = strings.ReplaceAll(path, `\`, "/")
	return path[strings.LastIndexByte(path, '/')+1:]
}
func (u Usage) SearchScore(query string) int {
	return searchScore(query, []string{fileName(u.Path)}, []string{u.Path, u.Relation, u.Access, u.Lock})
}
func (p Process) SearchScore(query string) int {
	names := []string{p.Name}
	fields := []string{fmt.Sprint(p.PID), p.User, p.Executable, p.CWD}
	for _, u := range p.Usages {
		names = append(names, fileName(u.Path))
		fields = append(fields, u.Path, u.Relation, u.Access, u.Lock)
	}
	return searchScore(query, names, fields)
}
