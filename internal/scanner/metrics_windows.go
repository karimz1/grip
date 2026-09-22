//go:build windows

package scanner

import (
	"context"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"golang.org/x/sys/windows"
	"unsafe"
)

var getProcessMemoryInfo = windows.NewLazySystemDLL("psapi.dll").NewProc("GetProcessMemoryInfo")

// PROCESS_MEMORY_COUNTERS uses SIZE_T for all counters after its DWORD header.
type processMemoryCounters struct {
	Size, PageFaults                                                                                             uint32
	PeakWorkingSet, WorkingSet, PeakPagedPool, PagedPool, PeakNonPagedPool, NonPagedPool, Pagefile, PeakPagefile uintptr
}

func (s *native) resourceUsage(id model.Identity) (uint64, uint64, bool) {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_INFORMATION|windows.PROCESS_VM_READ, false, uint32(id.PID))
	if err != nil {
		return 0, 0, false
	}
	defer windows.CloseHandle(h)
	var created, exit, kernel, user windows.Filetime
	if windows.GetProcessTimes(h, &created, &exit, &kernel, &user) != nil || filetimeID(created) != id.Started {
		return 0, 0, false
	}
	memory := processMemoryCounters{}
	memory.Size = uint32(unsafe.Sizeof(memory))
	ok, _, _ := getProcessMemoryInfo.Call(uintptr(h), uintptr(unsafe.Pointer(&memory)), uintptr(memory.Size))
	if ok == 0 {
		return 0, 0, false
	}
	ticks := func(t windows.Filetime) uint64 { return uint64(t.HighDateTime)<<32 | uint64(t.LowDateTime) }
	return uint64(memory.WorkingSet), (ticks(kernel) + ticks(user)) * 100, true
}
func (s *native) enrich(ctx context.Context, r *model.Result, parents map[int]int) {
	lookup := func(pid int) (model.Process, error) {
		p, err := s.process(pid)
		p.ParentPID = parents[pid]
		return p, err
	}
	s.resources.enrich(ctx, r, lookup, s.resourceUsage)
}
