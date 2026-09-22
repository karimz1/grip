package tui

import (
	"fmt"
	"os/exec"
	"runtime"
	"strings"

	tea "charm.land/bubbletea/v2"

	"github.com/charmbracelet/x/ansi"
)

const (
	repositoryURL = "https://github.com/karimz1/open-file-lock-handle"
	donateURL     = "https://buymeacoffee.com/karimz1"
)

// Only fixed project URLs become terminal hyperlinks. Process-owned text still
// goes through safe(), so file names cannot inject clickable destinations.
func projectLink(label, url string) string {
	return ansi.SetHyperlink(url) + label + ansi.ResetHyperlink()
}

func projectLinks() string { return projectFooterLinks() }

func projectFooterLinks() string {
	return projectLink(accent.Render("R")+muted.Render(" GH"), repositoryURL) + muted.Render(" · ") + projectLink(warning.Render("D")+muted.Render(" ☕ Donate"), donateURL)
}

func projectHelpLinks(width int) []string {
	lines := []string{projectLinks(), muted.Render("R opens the repository · D opens Buy Me a Coffee.")}
	for _, url := range []string{repositoryURL, donateURL} {
		lines = append(lines, strings.Split(ansi.Hardwrap(projectLink(url, url), width, true), "\n")...)
	}
	return lines
}

// Browser requests run as commands so launching a browser never blocks input.
type projectLinkMsg struct {
	url string
	err error
}

func browserCommand(platform, url string) (*exec.Cmd, error) {
	if url != repositoryURL && url != donateURL {
		return nil, fmt.Errorf("unknown project link")
	}
	switch platform {
	case "linux":
		return exec.Command("xdg-open", url), nil
	case "darwin":
		return exec.Command("open", url), nil
	case "windows":
		return exec.Command("rundll32.exe", "url.dll,FileProtocolHandler", url), nil
	default:
		return nil, fmt.Errorf("no browser launcher for %s", platform)
	}
}

func openProjectLink(url string) tea.Cmd {
	return func() tea.Msg {
		cmd, err := browserCommand(runtime.GOOS, url)
		if err == nil {
			err = cmd.Run()
		}
		return projectLinkMsg{url: url, err: err}
	}
}
