//go:build linux

package scanner

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"github.com/karimz1/open-file-lock-handle/internal/model"
	"golang.org/x/sys/unix"
	"os"
	"os/user"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
)

type native struct {
	root     string
	sampleMu sync.Mutex
	samples  map[string]cpuSample
}

func New() (Scanner, error) { return &native{root: "/proc"}, nil }

func (s *native) identity(pid int) (model.Identity, error) {
	b, err := os.ReadFile(filepath.Join(s.root, strconv.Itoa(pid), "stat"))
	if err != nil {
		return model.Identity{}, err
	}
	// comm is parenthesized and may itself contain spaces or closing parentheses.
	end := strings.LastIndexByte(string(b), ')')
	if end < 0 {
		return model.Identity{}, errors.New("invalid proc stat")
	}
	f := strings.Fields(string(b[end+1:]))
	if len(f) < 20 {
		return model.Identity{}, errors.New("short proc stat")
	}
	return model.Identity{PID: pid, Started: f[19]}, nil
}

func (s *native) Scan(ctx context.Context, target model.Target) (model.Result, error) {
	result := model.Result{LockDetection: true}
	entries, err := os.ReadDir(s.root)
	if err != nil {
		return result, fmt.Errorf("read proc: %w", err)
	}
	denied, foreign := 0, 0
	selfNS, _ := os.Readlink(filepath.Join(s.root, "self/ns/mnt"))
	users := make(map[string]string)
	for _, e := range entries {
		if err := ctx.Err(); err != nil {
			return result, err
		}
		pid, err := strconv.Atoi(e.Name())
		if err != nil || pid == os.Getpid() {
			continue
		}
		id, err := s.identity(pid)
		if err != nil {
			if errors.Is(err, os.ErrPermission) {
				denied++
			}
			continue
		}
		base := filepath.Join(s.root, e.Name())
		ns, _ := os.Readlink(filepath.Join(base, "ns/mnt"))
		if selfNS != "" && ns != "" && selfNS != ns {
			foreign++
			continue
		}
		p := model.Process{Identity: id, User: "unknown"}
		restricted := false
		check := func(err error) {
			if errors.Is(err, os.ErrPermission) {
				restricted = true
			}
		}
		status, err := os.ReadFile(filepath.Join(base, "status"))
		check(err)
		for _, line := range strings.Split(string(status), "\n") {
			if strings.HasPrefix(line, "Name:") {
				p.Name = strings.TrimSpace(strings.TrimPrefix(line, "Name:"))
			}
			if strings.HasPrefix(line, "Uid:") {
				f := strings.Fields(line)
				if len(f) > 2 {
					uid := f[2]
					name, ok := users[uid]
					if !ok {
						name = uid
						if u, e := user.LookupId(uid); e == nil {
							name = u.Username
						}
						users[uid] = name
					}
					p.User = name
				}
			}
		}
		add := func(path, relation, access, ref string) {
			deleted := strings.HasSuffix(path, " (deleted)")
			// /proc's deleted suffix is ambiguous for a live file literally named that.
			if deleted {
				if _, err := os.Stat(filepath.Join(base, "root", path)); err != nil {
					path = strings.TrimSuffix(path, " (deleted)")
				} else {
					deleted = false
				}
			}
			if !filepath.IsAbs(path) {
				return
			}
			var info os.FileInfo
			if ref != "" {
				info, _ = os.Stat(ref)
			}
			if target.Matches(path, info) {
				p.Usages = append(p.Usages, model.Usage{Path: path, Relation: relation, Access: access, Deleted: deleted})
			}
		}
		for _, rel := range []struct{ name, relation, access string }{{"cwd", "cwd", "directory"}, {"exe", "executable", "execute"}} {
			ref := filepath.Join(base, rel.name)
			path, err := os.Readlink(ref)
			check(err)
			if err == nil {
				if rel.name == "cwd" {
					p.CWD = path
				} else {
					p.Executable = path
				}
				add(path, rel.relation, rel.access, ref)
			}
		}
		fds, err := os.ReadDir(filepath.Join(base, "fd"))
		check(err)
		for _, fd := range fds {
			if err := ctx.Err(); err != nil {
				return result, err
			}
			ref := filepath.Join(base, "fd", fd.Name())
			path, err := os.Readlink(ref)
			check(err)
			if err != nil {
				continue
			}
			access := "unknown"
			b, err := os.ReadFile(filepath.Join(base, "fdinfo", fd.Name()))
			check(err)
			for _, line := range strings.Split(string(b), "\n") {
				if strings.HasPrefix(line, "flags:") {
					flags, e := strconv.ParseUint(strings.TrimSpace(strings.TrimPrefix(line, "flags:")), 8, 64)
					if e == nil {
						if flags&unix.O_PATH != 0 {
							access = "reference"
						} else {
							switch flags & unix.O_ACCMODE {
							case unix.O_RDONLY:
								access = "read"
							case unix.O_WRONLY:
								access = "write"
							case unix.O_RDWR:
								access = "read/write"
							}
						}
					}
				}
			}
			add(path, "open", access, ref)
			for _, lock := range fdLocks(string(b)) {
				before := len(p.Usages)
				add(path, "locked", access, ref)
				if len(p.Usages) > before {
					p.Usages[len(p.Usages)-1].Lock = lock
				}
			}
		}
		maps, err := os.Open(filepath.Join(base, "maps"))
		check(err)
		if err == nil {
			lines := bufio.NewScanner(maps)
			lines.Buffer(make([]byte, 4096), 1024*1024)
			for lines.Scan() {
				if err := ctx.Err(); err != nil {
					maps.Close()
					return result, err
				}
				line := lines.Text()
				f := strings.Fields(line)
				if len(f) < 6 {
					continue
				}
				// Preserve spaces in filenames; maps escapes embedded newlines as \\012.
				at := 0
				for i := 0; i < 5; i++ {
					for at < len(line) && line[at] == ' ' {
						at++
					}
					for at < len(line) && line[at] != ' ' {
						at++
					}
				}
				for at < len(line) && line[at] == ' ' {
					at++
				}
				path := strings.ReplaceAll(line[at:], `\012`, "\n")
				if !strings.HasPrefix(path, "/") {
					continue
				}
				if !target.Directory && target.Info != nil {
					if stat, ok := target.Info.Sys().(*syscall.Stat_t); ok {
						var major, minor uint32
						fmt.Sscanf(f[3], "%x:%x", &major, &minor)
						ino, _ := strconv.ParseUint(f[4], 10, 64)
						if ino != stat.Ino || major != unix.Major(uint64(stat.Dev)) || minor != unix.Minor(uint64(stat.Dev)) {
							continue
						}
					}
				}
				access := "mapped"
				if strings.Contains(f[1], "x") {
					access = "execute"
				} else if strings.HasPrefix(f[1], "rw") {
					access = "read/write"
				} else if strings.HasPrefix(f[1], "r") {
					access = "read"
				} else if len(f[1]) > 1 && f[1][1] == 'w' {
					access = "write"
				}
				add(path, "mapped", access, "")
			}
			check(lines.Err())
			maps.Close()
		}
		if restricted {
			denied++
		}
		if len(p.Usages) > 0 {
			current, e := s.identity(pid)
			if e == nil && current == id {
				result.Processes = append(result.Processes, p)
			}
		}
	}
	if denied > 0 {
		result.Warnings = append(result.Warnings, fmt.Sprintf("Limited visibility for %d processes (permissions). Elevated access may reveal more.", denied))
	}
	if foreign > 0 {
		result.Warnings = append(result.Warnings, fmt.Sprintf("Skipped %d processes in other mount namespaces; run oflh inside their container.", foreign))
	}
	s.enrich(ctx, &result)
	result.Normalize()
	return result, nil
}

func (s *native) Kill(ctx context.Context, id model.Identity, force bool) error {
	if err := validate(id); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	// A pidfd pins the process throughout validation and signal delivery.
	fd, err := unix.PidfdOpen(id.PID, 0)
	if err != nil {
		return fmt.Errorf("open process safely (requires Linux 5.3+): %w", err)
	}
	defer unix.Close(fd)
	current, err := s.identity(id.PID)
	if err != nil || current != id {
		return ErrChanged
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	signal := unix.SIGTERM
	if force {
		signal = unix.SIGKILL
	}
	if err := unix.PidfdSendSignal(fd, signal, nil, 0); err != nil {
		return fmt.Errorf("signal PID %d: %w", id.PID, err)
	}
	return nil
}

// fdLocks accepts held kernel locks only, excluding blocked requests and leases.
func fdLocks(info string) []string {
	var locks []string
	for _, line := range strings.Split(info, "\n") {
		f := strings.Fields(line)
		if len(f) != 9 || f[0] != "lock:" {
			continue
		}
		if f[2] != "FLOCK" && f[2] != "POSIX" && f[2] != "OFDLCK" {
			continue
		}
		if f[4] != "READ" && f[4] != "WRITE" {
			continue
		}
		locks = append(locks, fmt.Sprintf("%s %s %s bytes %s–%s", f[2], f[3], f[4], f[7], f[8]))
	}
	return locks
}
