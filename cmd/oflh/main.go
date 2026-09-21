package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"os"

	tea "charm.land/bubbletea/v2"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"github.com/karimz1/open-file-lock-handle/internal/scanner"
	"github.com/karimz1/open-file-lock-handle/internal/tui"
	"golang.org/x/term"
)

var version = "dev"

func run(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("oflh", flag.ContinueOnError)
	flags.SetOutput(stderr)
	showVersion := flags.Bool("version", false, "print version")
	flags.Usage = func() {
		fmt.Fprintln(stderr, "oflh — Open File Lock Handle. See what's using your files.\nSource: https://github.com/karimz1/open-file-lock-handle\n\nUsage: oflh [PATH]\n\n  oflh .\n  oflh ./build\n  oflh ./foo.dll\n\nNo PATH means the current directory.\n\nOptions:")
		flags.PrintDefaults()
	}
	if err := flags.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return 0
		}
		return 2
	}
	if *showVersion {
		fmt.Fprintln(stdout, "oflh", version)
		return 0
	}
	if flags.NArg() > 1 {
		fmt.Fprintln(stderr, "oflh: expected one path; quote paths containing spaces")
		return 2
	}
	path := "."
	if flags.NArg() == 1 {
		path = flags.Arg(0)
	}
	target, err := model.NewTarget(path)
	if err != nil {
		fmt.Fprintln(stderr, "oflh:", err)
		return 1
	}
	if !term.IsTerminal(int(os.Stdin.Fd())) || !term.IsTerminal(int(os.Stdout.Fd())) {
		fmt.Fprintln(stderr, "oflh: an interactive terminal is required; run oflh . in a terminal")
		return 1
	}
	backend, err := scanner.New()
	if err != nil {
		fmt.Fprintln(stderr, "oflh:", err)
		return 1
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	if _, err := tea.NewProgram(tui.New(ctx, backend, target, version), tea.WithContext(ctx)).Run(); err != nil {
		fmt.Fprintln(stderr, "oflh:", err)
		return 1
	}
	return 0
}

func main() { os.Exit(run(os.Args[1:], os.Stdout, os.Stderr)) }
