package tui

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

func TestProjectBrowserCommands(t *testing.T) {
	for platform, args := range map[string][]string{
		"linux":   {"xdg-open"},
		"darwin":  {"open"},
		"windows": {"rundll32.exe", "url.dll,FileProtocolHandler"},
	} {
		for _, url := range []string{repositoryURL, donateURL} {
			cmd, err := browserCommand(platform, url)
			want := append(append([]string{}, args...), url)
			if err != nil || !reflect.DeepEqual(cmd.Args, want) {
				t.Fatalf("%s launcher: %v, %v", platform, cmd, err)
			}
		}
	}
	if _, err := browserCommand("linux", "https://untrusted.example"); err == nil {
		t.Fatal("unexpected destination accepted")
	}
}

func TestProjectLinkShortcutsRespectInputAndConfirmation(t *testing.T) {
	for _, key := range []string{"R", "D"} {
		for _, screen := range []screen{mainScreen, detailScreen, helpScreen} {
			a := searchApp()
			a.screen = screen
			_, cmd := a.Update(tea.KeyPressMsg{Code: rune(key[0]), Text: key})
			if cmd == nil {
				t.Fatalf("%s shortcut not handled on %v", key, screen)
			}
		}
		a := searchApp()
		a.updateDetails("/")
		a.Update(tea.KeyPressMsg{Code: rune(key[0]), Text: key})
		if a.detailFilter.Value() != key || !a.filtering {
			t.Fatal("link shortcut stole search input")
		}
		a.filtering = false
		a.screen = confirmScreen
		_, cmd := a.Update(tea.KeyPressMsg{Code: rune(key[0]), Text: key})
		if cmd != nil || a.screen != confirmScreen {
			t.Fatal("link shortcut interrupted confirmation")
		}
	}
	a := searchApp()
	a.Update(projectLinkMsg{url: repositoryURL, err: errors.New("launcher missing")})
	if !a.statusError || !strings.Contains(a.status, repositoryURL) {
		t.Fatal("missing manual link fallback")
	}
}
