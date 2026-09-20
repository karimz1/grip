package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"os"

	tea "charm.land/bubbletea/v2"
	"github.com/karimz1/grip/internal/model"
	"github.com/karimz1/grip/internal/scanner"
	"github.com/karimz1/grip/internal/tui"
	"golang.org/x/term"
)

var version = "dev"

func run(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("grip", flag.ContinueOnError)
	flags.SetOutput(stderr)
	showVersion := flags.Bool("version", false, "print version")
	flags.Usage = func() {
		fmt.Fprintln(stderr, "grip — see what's using your files.\nSource: https://github.com/karimz1/grip\n\nUsage: grip [PATH]\n\n  grip .\n  grip ./build\n  grip ./foo.dll\n\nNo PATH means the current directory.\n\nOptions:")
		flags.PrintDefaults()
	}
	if err := flags.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return 0
		}
		return 2
	}
	if *showVersion {
		fmt.Fprintln(stdout, "grip", version)
		return 0
	}
	if flags.NArg() > 1 {
		fmt.Fprintln(stderr, "grip: expected one path; quote paths containing spaces")
		return 2
	}
	path := "."
	if flags.NArg() == 1 {
		path = flags.Arg(0)
	}
	target, err := model.NewTarget(path)
	if err != nil {
		fmt.Fprintln(stderr, "grip:", err)
		return 1
	}
	if !term.IsTerminal(int(os.Stdin.Fd())) || !term.IsTerminal(int(os.Stdout.Fd())) {
		fmt.Fprintln(stderr, "grip: an interactive terminal is required; run grip . in a terminal")
		return 1
	}
	backend, err := scanner.New()
	if err != nil {
		fmt.Fprintln(stderr, "grip:", err)
		return 1
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	if _, err := tea.NewProgram(tui.New(ctx, backend, target, version), tea.WithContext(ctx)).Run(); err != nil {
		fmt.Fprintln(stderr, "grip:", err)
		return 1
	}
	return 0
}

func main() { os.Exit(run(os.Args[1:], os.Stdout, os.Stderr)) }
