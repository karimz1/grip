package tui

import "github.com/karimz1/open-file-lock-handle/internal/model"

// Summarize the displayed usages, rather than guessing access from the process
// name or from whichever usage happened to sort first (often cwd).
func processAccess(p model.Process) string {
	var read, write, execute, mapped, cwd, reference bool
	for _, u := range p.Usages {
		cwd = cwd || u.Relation == "cwd"
		mapped = mapped || u.Relation == "mapped"
		switch u.Access {
		case "read":
			read = true
		case "write":
			write = true
		case "read/write":
			read, write = true, true
		case "execute":
			execute = true
		case "reference":
			reference = true
		}
	}
	switch {
	case read && write:
		return "read/write"
	case write:
		return "write"
	case read:
		return "read"
	case execute:
		return "execute"
	case mapped:
		return "mapped"
	case cwd:
		return "cwd"
	case reference:
		return "reference"
	default:
		return "unknown"
	}
}
