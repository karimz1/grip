package tui

import (
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"strings"
	"unicode"
)

var (
	accent        = lipgloss.NewStyle().Foreground(lipgloss.Color("6")).Bold(true)
	muted         = lipgloss.NewStyle().Foreground(lipgloss.Color("8"))
	warning       = lipgloss.NewStyle().Foreground(lipgloss.Color("3"))
	danger        = lipgloss.NewStyle().Foreground(lipgloss.Color("1")).Bold(true)
	selectedStyle = lipgloss.NewStyle().Reverse(true)
	brand         = lipgloss.NewStyle().Foreground(lipgloss.Color("0")).Background(lipgloss.Color("6")).Bold(true).Padding(0, 1)
)

// OS-owned names are untrusted terminal text, including OSC/CSI and bidi controls.
func safe(s string) string {
	return strings.Map(func(r rune) rune {
		if unicode.IsControl(r) || unicode.Is(unicode.Cf, r) {
			return '�'
		}
		return r
	}, s)
}
func cell(s string, w int) string {
	if w <= 0 {
		return ""
	}
	s = ansi.Truncate(safe(s), w, "…")
	return s + strings.Repeat(" ", max(0, w-ansi.StringWidth(s)))
}
func present(s string) string {
	if s == "" {
		return "unavailable"
	}
	return safe(s)
}
