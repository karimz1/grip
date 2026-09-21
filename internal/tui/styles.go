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
	brand         = lipgloss.NewStyle().Foreground(lipgloss.Color("6")).Bold(true)
	success       = lipgloss.NewStyle().Foreground(lipgloss.Color("2"))
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

// Keep evidence labels readable without color; never imply that usage proves a lock.
func evidenceStyle(value string) lipgloss.Style {
	switch value {
	case "read":
		return success
	case "write", "read/write", "deleted":
		return warning
	case "mapped", "executable", "execute":
		return accent
	case "unknown", "unavailable":
		return muted
	default:
		return lipgloss.NewStyle()
	}
}
func evidenceCell(value string, width int) string {
	return evidenceStyle(value).Render(cell(value, width))
}

type shortcut struct{ key, label string }

// Wrap complete hints rather than clipping commands at the right terminal edge.
func shortcutLines(width int, hints ...shortcut) []string {
	lines := []string{}
	line := ""
	for _, hint := range hints {
		keyStyle := accent
		if hint.key == "x" || hint.key == "X" {
			keyStyle = danger
		}
		part := keyStyle.Render(hint.key) + " " + muted.Render(hint.label)
		sep := muted.Render(" · ")
		if line != "" && ansi.StringWidth(line+sep+part) > width {
			lines = append(lines, line)
			line = ""
		}
		if line != "" {
			line += sep
		}
		line += ansi.Truncate(part, width, "…")
	}
	if line != "" {
		lines = append(lines, line)
	}
	return lines
}
