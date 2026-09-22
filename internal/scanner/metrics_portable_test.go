//go:build windows || darwin

package scanner

import (
	"context"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"runtime"
	"testing"
	"time"
)

func TestPortableMetricsIdentityAndCPUDelta(t *testing.T) {
	p := model.Process{Identity: model.Identity{PID: 42, Started: "1"}}
	lookup := func(int) (model.Process, error) { return p, nil }
	read := func(model.Identity) (uint64, uint64, bool) { return 4096, 1000000, true }
	r := model.Result{Processes: []model.Process{p}}
	sampler := resourceSampler{}
	sampler.enrich(context.Background(), &r, lookup, read)
	if !r.Processes[0].MemoryKnown || r.Processes[0].CPUKnown {
		t.Fatal("first sample must not invent CPU usage")
	}
	sampler.previous[p.Key()] = resourceSample{cpu: 0, at: time.Now().Add(-time.Second)}
	sampler.enrich(context.Background(), &r, lookup, read)
	got := r.Processes[0].CPUPercent
	maxCPU := 0.1 / float64(runtime.NumCPU())
	if !r.Processes[0].CPUKnown || got <= 0 || got > maxCPU {
		t.Fatalf("bad CPU delta: %v", got)
	}
	p.Started = "replacement"
	r.Processes = []model.Process{p}
	sampler.enrich(context.Background(), &r, lookup, read)
	if r.Processes[0].CPUKnown {
		t.Fatal("CPU baseline crossed process lifetimes")
	}
}
