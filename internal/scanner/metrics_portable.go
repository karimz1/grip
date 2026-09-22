//go:build windows || darwin

package scanner

import (
	"context"
	"runtime"
	"sync"
	"time"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

type resourceSample struct {
	cpu uint64
	at  time.Time
}
type resourceSampler struct {
	mu       sync.Mutex
	previous map[string]resourceSample
}

// The identity-keyed samples prevent PID reuse from producing false CPU deltas.
func (s *resourceSampler) enrich(ctx context.Context, r *model.Result, lookup func(int) (model.Process, error), read func(model.Identity) (uint64, uint64, bool)) {
	next := make(map[string]resourceSample)
	for i := range r.Processes {
		if ctx.Err() != nil {
			return
		}
		p := &r.Processes[i]
		if memory, cpu, ok := read(p.Identity); ok {
			p.MemoryBytes, p.MemoryKnown = memory, true
			next[p.Key()] = resourceSample{cpu, time.Now()}
		}
		// Restart Manager entries may not carry a parent; obtain it from the same
		// process lifetime before building an actionable ancestry snapshot.
		current, err := lookup(p.PID)
		if err != nil || current.Identity != p.Identity {
			continue
		}
		p.ParentPID = current.ParentPID
		seen := map[int]bool{p.PID: true}
		for pid := p.ParentPID; pid > 0 && !seen[pid] && len(p.Ancestors) < 8; {
			if ctx.Err() != nil {
				return
			}
			seen[pid] = true
			parent, err := lookup(pid)
			if err != nil {
				p.Ancestors = append(p.Ancestors, model.Ancestor{PID: pid, Name: "unavailable"})
				break
			}
			p.Ancestors = append(p.Ancestors, model.Ancestor{PID: pid, Started: parent.Started, Name: parent.Name})
			pid = parent.ParentPID
		}
	}
	if ctx.Err() != nil {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for i := range r.Processes {
		p := &r.Processes[i]
		current, ok := next[p.Key()]
		if !ok {
			continue
		}
		if previous, ok := s.previous[p.Key()]; ok && current.at.After(previous.at) && current.cpu >= previous.cpu {
			elapsed := float64(current.at.Sub(previous.at).Nanoseconds()) * float64(runtime.NumCPU())
			p.CPUPercent = min(100, 100*float64(current.cpu-previous.cpu)/elapsed)
			p.CPUKnown = true
		}
	}
	// Concurrent cancelled/older scans cannot rewind the baseline.
	for key, previous := range s.previous {
		if current, ok := next[key]; ok && !current.at.After(previous.at) {
			return
		}
	}
	s.previous = next
}
