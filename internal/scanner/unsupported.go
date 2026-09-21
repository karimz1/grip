//go:build !linux && !darwin && !windows

package scanner

import (
	"fmt"
	"runtime"
)

func New() (Scanner, error) { return nil, fmt.Errorf("oflh does not support %s", runtime.GOOS) }
