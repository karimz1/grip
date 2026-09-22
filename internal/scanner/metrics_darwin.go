//go:build darwin

package scanner

import (
	"context"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"unsafe"
)

// rusage_info_v0 from Apple's sys/resource.h. Time counters are nanoseconds.
type rusageInfo struct {
	UUID                                                                                                 [16]byte
	User, System, PackageWakeups, InterruptWakeups, Pageins, Wired, Resident, Footprint, Started, Exited uint64
}

func (s *native) resourceUsage(id model.Identity) (uint64, uint64, bool) {
	current, err := s.identity(id.PID)
	if err != nil || current != id {
		return 0, 0, false
	}
	var usage rusageInfo
	if s.pidRusage(int32(id.PID), 0, unsafe.Pointer(&usage)) != 0 {
		return 0, 0, false
	}
	current, err = s.identity(id.PID)
	if err != nil || current != id {
		return 0, 0, false
	}
	return usage.Resident, usage.User + usage.System, true
}
func (s *native) enrich(ctx context.Context, r *model.Result) {
	s.resources.enrich(ctx, r, s.process, s.resourceUsage)
}
