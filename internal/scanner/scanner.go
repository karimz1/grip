// Package scanner is the only layer that knows how the host inspects or stops processes.
package scanner

import (
	"context"
	"errors"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"os"
)

type Scanner interface {
	Scan(context.Context, model.Target) (model.Result, error)
	Kill(context.Context, model.Identity, bool) error
}

var ErrChanged = errors.New("process exited or PID was reused; refresh before trying again")

func validate(id model.Identity) error {
	if id.PID <= 1 || id.PID == os.Getpid() {
		return errors.New("refusing to terminate oflh or a system process")
	}
	if id.Started == "" {
		return errors.New("process identity unavailable; refusing to terminate")
	}
	return nil
}
