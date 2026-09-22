//go:build darwin

package scanner

import (
	"context"
	"fmt"
	"os"

	"github.com/karimz1/open-file-lock-handle/internal/model"
	"golang.org/x/sys/unix"
)

// F_GETLK reports an existing POSIX lock; it does not acquire or change locks.
// The public query returns the first conflicting range, not every held lock.
func (s *native) detectLocks(ctx context.Context, r *model.Result) {
	paths := make(map[string]bool)
	for _, p := range r.Processes {
		for _, u := range p.Usages {
			paths[u.Path] = true
		}
	}
	for path := range paths {
		if ctx.Err() != nil {
			return
		}
		info, err := os.Stat(path)
		if err != nil || !info.Mode().IsRegular() {
			continue
		}
		fd, err := unix.Open(path, unix.O_RDONLY|unix.O_NONBLOCK|unix.O_CLOEXEC, 0)
		if err != nil {
			continue
		}
		// Validate the opened inode against the discovered path before attributing it.
		f := os.NewFile(uintptr(fd), path)
		opened, err := f.Stat()
		if err != nil || !os.SameFile(info, opened) {
			f.Close()
			continue
		}
		lock := unix.Flock_t{Type: unix.F_WRLCK, Whence: 0, Start: 0, Len: 0}
		err = unix.FcntlFlock(f.Fd(), unix.F_GETLK, &lock)
		f.Close()
		if err != nil || lock.Type == unix.F_UNLCK || lock.Pid <= 0 {
			continue
		}
		access := "read"
		if lock.Type == unix.F_WRLCK {
			access = "write"
		}
		end := "EOF"
		if lock.Len > 0 {
			end = fmt.Sprint(lock.Start + lock.Len - 1)
		}
		evidence := fmt.Sprintf("POSIX ADVISORY %s bytes %d–%s", access, lock.Start, end)
		for i := range r.Processes {
			p := &r.Processes[i]
			if p.PID != int(lock.Pid) {
				continue
			}
			current, err := s.identity(p.PID)
			if err == nil && current == p.Identity {
				p.Usages = append(p.Usages, model.Usage{Path: path, Relation: "locked", Access: access, Lock: evidence})
			}
			break
		}
	}
}
