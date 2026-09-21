//go:build linux

package scanner

import (
	"context"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

type cpuSample struct{ ticks, total uint64 }
type procStats struct {
	name, started string
	parent        int
	ticks, memory uint64
	memoryKnown   bool
}

func parseProcStats(data string) (procStats, bool) {
	start, end := strings.IndexByte(data, '('), strings.LastIndexByte(data, ')')
	if start < 0 || end <= start {
		return procStats{}, false
	}
	f := strings.Fields(data[end+1:])
	if len(f) < 22 {
		return procStats{}, false
	}
	parent, err1 := strconv.Atoi(f[1])
	user, err2 := strconv.ParseUint(f[11], 10, 64)
	system, err3 := strconv.ParseUint(f[12], 10, 64)
	rss, err4 := strconv.ParseUint(f[21], 10, 64)
	if err1 != nil || err2 != nil || err3 != nil {
		return procStats{}, false
	}
	return procStats{name: data[start+1 : end], started: f[19], parent: parent, ticks: user + system, memory: rss * uint64(os.Getpagesize()), memoryKnown: err4 == nil}, true
}

func (s *native) procStats(pid int) (procStats, bool) {
	b, err := os.ReadFile(filepath.Join(s.root, strconv.Itoa(pid), "stat"))
	if err != nil {
		return procStats{}, false
	}
	return parseProcStats(string(b))
}

func cpuTotal(data string) uint64 {
	line, _, _ := strings.Cut(data, "\n")
	f := strings.Fields(line)
	if len(f) < 9 || f[0] != "cpu" {
		return 0
	}
	var total uint64
	// Guest time is already included in user/nice; don't count it twice.
	for _, value := range f[1:9] {
		n, err := strconv.ParseUint(value, 10, 64)
		if err != nil {
			return 0
		}
		total += n
	}
	return total
}

func (s *native) enrich(ctx context.Context, result *model.Result) {
	b, _ := os.ReadFile(filepath.Join(s.root, "stat"))
	total := cpuTotal(string(b))
	next := make(map[string]cpuSample)
	for i := range result.Processes {
		if ctx.Err() != nil {
			return
		}
		p := &result.Processes[i]
		st, ok := s.procStats(p.PID)
		if !ok || st.started != p.Started {
			continue
		}
		p.ParentPID, p.MemoryBytes, p.MemoryKnown = st.parent, st.memory, st.memoryKnown
		next[p.Key()] = cpuSample{st.ticks, total}
		seen := map[int]bool{p.PID: true}
		for pid := st.parent; pid > 0 && !seen[pid] && len(p.Ancestors) < 8; {
			seen[pid] = true
			parent, ok := s.procStats(pid)
			if !ok {
				p.Ancestors = append(p.Ancestors, model.Ancestor{PID: pid, Name: "unavailable"})
				break
			}
			p.Ancestors = append(p.Ancestors, model.Ancestor{PID: pid, Name: parent.name, Started: parent.started})
			pid = parent.parent
		}
	}
	if ctx.Err() != nil || total == 0 {
		return
	}
	s.sampleMu.Lock()
	defer s.sampleMu.Unlock()
	for i := range result.Processes {
		p := &result.Processes[i]
		current, ok := next[p.Key()]
		if !ok {
			continue
		}
		if previous, exists := s.samples[p.Key()]; exists && current.total > previous.total && current.ticks >= previous.ticks {
			p.CPUPercent = min(100, 100*float64(current.ticks-previous.ticks)/float64(current.total-previous.total))
			p.CPUKnown = true
		}
	}
	// Older cancelled scans must not replace newer samples.
	for _, previous := range s.samples {
		if previous.total >= total {
			return
		}
	}
	s.samples = next
}
