//go:build darwin

package scanner

import (
	"bytes"
	"context"
	"encoding/binary"
	"fmt"
	"os"
	"os/user"
	"strconv"
	"unsafe"

	"github.com/ebitengine/purego"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"golang.org/x/sys/unix"
)

// libproc is part of macOS. purego calls it without an external executable or cgo.
// Structure sizes/offsets are the 64-bit ABI in XNU's bsd/sys/proc_info.h.
type native struct {
	resources resourceSampler
	pidRusage func(int32, int32, unsafe.Pointer) int32
	listPIDs  func(unsafe.Pointer, int32) int32
	pidInfo   func(int32, int32, uint64, unsafe.Pointer, int32) int32
	fdInfo    func(int32, int32, int32, unsafe.Pointer, int32) int32
	pidPath   func(int32, unsafe.Pointer, uint32) int32
}

func New() (Scanner, error) {
	lib, err := purego.Dlopen("/usr/lib/libproc.dylib", purego.RTLD_NOW|purego.RTLD_LOCAL)
	if err != nil {
		return nil, fmt.Errorf("load macOS libproc: %w", err)
	}
	s := &native{}
	for _, binding := range []struct {
		name string
		fn   any
	}{
		{"proc_pid_rusage", &s.pidRusage}, {"proc_listallpids", &s.listPIDs}, {"proc_pidinfo", &s.pidInfo},
		{"proc_pidfdinfo", &s.fdInfo}, {"proc_pidpath", &s.pidPath},
	} {
		addr, err := purego.Dlsym(lib, binding.name)
		if err != nil {
			purego.Dlclose(lib)
			return nil, err
		}
		purego.RegisterFunc(binding.fn, addr)
	}
	return s, nil
}

func cstring(b []byte) string {
	if i := bytes.IndexByte(b, 0); i >= 0 {
		b = b[:i]
	}
	return string(b)
}
func u32(b []byte) uint32 { return binary.LittleEndian.Uint32(b) }
func u64(b []byte) uint64 { return binary.LittleEndian.Uint64(b) }

func (s *native) process(pid int) (model.Process, error) {
	var b [136]byte
	if s.pidInfo(int32(pid), 3, 0, unsafe.Pointer(&b[0]), int32(len(b))) != int32(len(b)) {
		return model.Process{}, ErrChanged
	}
	p := model.Process{Identity: model.Identity{PID: pid, Started: fmt.Sprintf("%d:%d", u64(b[120:]), u64(b[128:]))}, Name: cstring(b[64:96]), User: strconv.FormatUint(uint64(u32(b[20:])), 10)}
	p.ParentPID = int(u32(b[16:]))
	if p.Name == "" {
		p.Name = cstring(b[48:64])
	}
	var path [4096]byte
	if s.pidPath(int32(pid), unsafe.Pointer(&path[0]), uint32(len(path))) > 0 {
		p.Executable = cstring(path[:])
	}
	return p, nil
}

func (s *native) identity(pid int) (model.Identity, error) {
	p, err := s.process(pid)
	return p.Identity, err
}

func (s *native) Scan(ctx context.Context, t model.Target) (model.Result, error) {
	r := model.Result{LockDetection: true, Warnings: []string{"macOS: lock queries report the first POSIX conflict per readable file; flock-only locks and additional ranges may not be visible."}}
	if err := ctx.Err(); err != nil {
		return r, err
	}
	n := s.listPIDs(nil, 0)
	if n <= 0 {
		return r, fmt.Errorf("macOS process enumeration unavailable")
	}
	var pids []int32
	for attempt := 0; attempt < 4; attempt++ {
		pids = make([]int32, int(n)+256)
		n = s.listPIDs(unsafe.Pointer(&pids[0]), int32(len(pids)*4))
		if n < 0 {
			return r, fmt.Errorf("macOS process enumeration failed")
		}
		if int(n) < len(pids) {
			pids = pids[:n]
			break
		}
		if attempt == 3 {
			return r, fmt.Errorf("process list changed too quickly; refresh")
		}
	}
	limited := 0
	users := map[string]string{}
	for _, pid := range pids {
		if err := ctx.Err(); err != nil {
			return r, err
		}
		if pid <= 0 || int(pid) == os.Getpid() {
			continue
		}
		p, err := s.process(int(pid))
		if err != nil {
			limited++
			continue
		}
		if name, ok := users[p.User]; ok {
			p.User = name
		} else {
			uid := p.User
			if u, err := user.LookupId(uid); err == nil {
				p.User = u.Username
			}
			users[uid] = p.User
		}
		partial := false
		add := func(path, relation, access string) {
			if path == "" {
				return
			}
			info, _ := os.Stat(path)
			if t.Matches(path, info) {
				p.Usages = append(p.Usages, model.Usage{Path: path, Relation: relation, Access: access})
			}
		}
		add(p.Executable, "executable", "execute")
		var cwd [2352]byte // two vnode_info_path records: 152 bytes metadata + 1024 path.
		if s.pidInfo(pid, 9, 0, unsafe.Pointer(&cwd[0]), int32(len(cwd))) == int32(len(cwd)) {
			p.CWD = cstring(cwd[152:1176])
			add(p.CWD, "cwd", "directory")
		} else {
			partial = true
		}
		fdSize := s.pidInfo(pid, 1, 0, nil, 0)
		if fdSize > 0 && fdSize < 32*1024*1024 {
			fds := make([]byte, int(fdSize)+1024)
			n := s.pidInfo(pid, 1, 0, unsafe.Pointer(&fds[0]), int32(len(fds)))
			if n <= 0 {
				partial = true
			}
			if n == int32(len(fds)) {
				partial = true
			}
			for at := 0; at+8 <= int(n); at += 8 {
				if err := ctx.Err(); err != nil {
					return r, err
				}
				if u32(fds[at+4:]) != 1 {
					continue
				} // PROX_FDTYPE_VNODE
				var b [1200]byte
				if s.fdInfo(pid, int32(u32(fds[at:])), 2, unsafe.Pointer(&b[0]), int32(len(b))) != int32(len(b)) {
					partial = true
					continue
				}
				access := "unknown"
				switch u32(b[:]) & 3 {
				case 1:
					access = "read"
				case 2:
					access = "write"
				case 3:
					access = "read/write"
				}
				add(cstring(b[176:]), "open", access)
			}
		} else if fdSize < 0 {
			partial = true
		}
		address := uint64(0)
		for regions := 0; regions < 65536; regions++ {
			if err := ctx.Err(); err != nil {
				return r, err
			}
			var b [1272]byte
			if s.pidInfo(pid, 8, address, unsafe.Pointer(&b[0]), int32(len(b))) != int32(len(b)) {
				break
			}
			access := "mapped"
			flags := u32(b[:])
			if flags&4 != 0 {
				access = "execute"
			} else if flags&3 == 3 {
				access = "read/write"
			} else if flags&1 != 0 {
				access = "read"
			} else if flags&2 != 0 {
				access = "write"
			}
			add(cstring(b[248:]), "mapped", access)
			next := u64(b[80:]) + u64(b[88:])
			if next <= address {
				break
			}
			address = next
			if regions == 65535 {
				partial = true
			}
		}
		if partial {
			limited++
		}
		if len(p.Usages) > 0 {
			current, err := s.identity(int(pid))
			if err == nil && current == p.Identity {
				r.Processes = append(r.Processes, p)
			}
		}
	}
	if limited > 0 {
		r.Warnings = append(r.Warnings, fmt.Sprintf("%d processes could not be fully inspected (permissions or process changes).", limited))
	}
	s.detectLocks(ctx, &r)
	r.Normalize()
	s.enrich(ctx, &r)
	return r, ctx.Err()
}

func (s *native) Kill(ctx context.Context, id model.Identity, force bool) error {
	if err := validate(id); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	current, err := s.identity(id.PID)
	if err != nil || current != id {
		return ErrChanged
	}
	signal := unix.SIGTERM
	if force {
		signal = unix.SIGKILL
	}
	// macOS has no public pidfd equivalent; start time is checked immediately before kill.
	return unix.Kill(id.PID, signal)
}
