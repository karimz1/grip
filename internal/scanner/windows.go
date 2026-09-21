//go:build windows

package scanner

import (
	"context"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"unsafe"

	"github.com/karimz1/open-file-lock-handle/internal/model"
	"golang.org/x/sys/windows"
)

type native struct{}

func New() (Scanner, error) { return &native{}, nil }

var (
	rm          = windows.NewLazySystemDLL("rstrtmgr.dll")
	rmStart     = rm.NewProc("RmStartSession")
	rmEnd       = rm.NewProc("RmEndSession")
	rmRegister  = rm.NewProc("RmRegisterResources")
	rmList      = rm.NewProc("RmGetList")
	user32      = windows.NewLazySystemDLL("user32.dll")
	enumWindows = user32.NewProc("EnumWindows")
	windowPID   = user32.NewProc("GetWindowThreadProcessId")
	postMessage = user32.NewProc("PostMessageW")
)

type rmProcess struct {
	PID     uint32
	Started windows.Filetime
}
type rmInfo struct {
	Process     rmProcess
	Name        [256]uint16
	Service     [64]uint16
	Type        uint32
	Status      uint32
	Session     uint32
	Restartable int32
}

func filetimeID(t windows.Filetime) string {
	return fmt.Sprintf("%08x%08x", t.HighDateTime, t.LowDateTime)
}
func handleIdentity(h windows.Handle, pid int) (model.Identity, error) {
	var created, exit, kernel, user windows.Filetime
	err := windows.GetProcessTimes(h, &created, &exit, &kernel, &user)
	return model.Identity{PID: pid, Started: filetimeID(created)}, err
}

func (s *native) process(pid int) (model.Process, error) {
	h, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return model.Process{}, err
	}
	defer windows.CloseHandle(h)
	id, err := handleIdentity(h, pid)
	if err != nil {
		return model.Process{}, err
	}
	p := model.Process{Identity: id, User: "unknown"}
	var path [32768]uint16
	n := uint32(len(path))
	if windows.QueryFullProcessImageName(h, 0, &path[0], &n) == nil {
		p.Executable = windows.UTF16ToString(path[:n])
		p.Name = filepath.Base(p.Executable)
	}
	var token windows.Token
	if windows.OpenProcessToken(h, windows.TOKEN_QUERY, &token) == nil {
		defer token.Close()
		if u, err := token.GetTokenUser(); err == nil {
			p.User = u.User.Sid.String()
			if name, domain, _, err := u.User.Sid.LookupAccount(""); err == nil {
				p.User = domain + `\` + name
			}
		}
	}
	return p, nil
}

func (s *native) identity(pid int) (model.Identity, error) {
	p, err := s.process(pid)
	return p.Identity, err
}

// rmUsers registers a batch; recursive correlation below narrows only batches
// with affected processes, rather than opening one session for every unused file.
func rmUsers(paths []string) ([]rmInfo, error) {
	var session uint32
	var key [33]uint16
	code, _, _ := rmStart.Call(uintptr(unsafe.Pointer(&session)), 0, uintptr(unsafe.Pointer(&key[0])))
	if code != 0 {
		return nil, windows.Errno(code)
	}
	defer rmEnd.Call(uintptr(session))
	ptrs := make([]*uint16, len(paths))
	for i, p := range paths {
		var err error
		ptrs[i], err = windows.UTF16PtrFromString(p)
		if err != nil {
			return nil, err
		}
	}
	code, _, _ = rmRegister.Call(uintptr(session), uintptr(len(ptrs)), uintptr(unsafe.Pointer(&ptrs[0])), 0, 0, 0, 0)
	runtime.KeepAlive(ptrs)
	if code != 0 {
		return nil, windows.Errno(code)
	}
	var needed, count, reboot uint32
	var data []rmInfo
	for attempt := 0; attempt < 5; attempt++ {
		var ptr uintptr
		if len(data) > 0 {
			ptr = uintptr(unsafe.Pointer(&data[0]))
			count = uint32(len(data))
		}
		code, _, _ = rmList.Call(uintptr(session), uintptr(unsafe.Pointer(&needed)), uintptr(unsafe.Pointer(&count)), ptr, uintptr(unsafe.Pointer(&reboot)))
		runtime.KeepAlive(data)
		if code == 0 {
			if int(count) > len(data) {
				return nil, errors.New("invalid Restart Manager result")
			}
			return data[:count], nil
		}
		if code != uintptr(windows.ERROR_MORE_DATA) {
			return nil, windows.Errno(code)
		}
		if needed > 1<<20 {
			return nil, errors.New("Restart Manager result too large")
		}
		data = make([]rmInfo, int(needed)+16)
	}
	return nil, errors.New("Restart Manager resources changed too quickly; refresh")
}

func (s *native) Scan(ctx context.Context, t model.Target) (model.Result, error) {
	r := model.Result{Warnings: []string{"Windows: CWD, directory handles and deleted files are not visible. Restart Manager does not expose access modes or prove a lock."}}
	if err := ctx.Err(); err != nil {
		return r, err
	}
	limited := 0
	snapshot, err := windows.CreateToolhelp32Snapshot(windows.TH32CS_SNAPPROCESS, 0)
	if err != nil {
		return r, err
	}
	defer windows.CloseHandle(snapshot)
	entry := windows.ProcessEntry32{Size: uint32(unsafe.Sizeof(windows.ProcessEntry32{}))}
	for err = windows.Process32First(snapshot, &entry); err == nil; err = windows.Process32Next(snapshot, &entry) {
		if err := ctx.Err(); err != nil {
			return r, err
		}
		pid := int(entry.ProcessID)
		if pid <= 0 || pid == os.Getpid() {
			continue
		}
		p, err := s.process(pid)
		if err != nil {
			limited++
			continue
		}
		add := func(path, relation string) {
			info, _ := os.Stat(path)
			if t.Matches(path, info) {
				p.Usages = append(p.Usages, model.Usage{Path: path, Relation: relation, Access: "execute"})
			}
		}
		if p.Executable != "" {
			add(p.Executable, "executable")
		}
		var modules windows.Handle
		for attempt := 0; attempt < 3; attempt++ {
			modules, err = windows.CreateToolhelp32Snapshot(windows.TH32CS_SNAPMODULE|windows.TH32CS_SNAPMODULE32, uint32(pid))
			if !errors.Is(err, windows.ERROR_BAD_LENGTH) {
				break
			}
		}
		if err == nil {
			module := windows.ModuleEntry32{Size: uint32(windows.SizeofModuleEntry32)}
			for e := windows.Module32First(modules, &module); e == nil; e = windows.Module32Next(modules, &module) {
				add(windows.UTF16ToString(module.ExePath[:]), "mapped")
			}
			windows.CloseHandle(modules)
		} else {
			limited++
		}
		if len(p.Usages) > 0 {
			current, e := s.identity(pid)
			if e == nil && current == p.Identity {
				r.Processes = append(r.Processes, p)
			}
		}
	}
	if !errors.Is(err, windows.ERROR_NO_MORE_FILES) {
		return r, err
	}
	paths := []string{}
	if t.Directory {
		err = filepath.WalkDir(t.Path, func(path string, d fs.DirEntry, e error) error {
			if err := ctx.Err(); err != nil {
				return err
			}
			if e != nil {
				limited++
				return nil
			}
			if d.Type().IsRegular() {
				paths = append(paths, path)
			}
			if len(paths) >= 10000 {
				return fs.SkipAll
			}
			return nil
		})
		if err != nil {
			return r, err
		}
		if len(paths) >= 10000 {
			r.Warnings = append(r.Warnings, "Directory scan limited to 10,000 files. Narrow the target for complete coverage.")
		}
	} else {
		paths = append(paths, t.Path)
	}
	var correlate func([]string) error
	correlate = func(paths []string) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		apps, e := rmUsers(paths)
		if e != nil {
			limited++
			return nil
		}
		if len(apps) == 0 {
			return nil
		}
		if len(paths) > 1 {
			mid := len(paths) / 2
			if err := correlate(paths[:mid]); err != nil {
				return err
			}
			return correlate(paths[mid:])
		}
		for _, app := range apps {
			pid := int(app.Process.PID)
			if pid == os.Getpid() {
				continue
			}
			id := model.Identity{PID: pid, Started: filetimeID(app.Process.Started)}
			p, e := s.process(pid)
			if e != nil {
				limited++
				p = model.Process{Identity: id, Name: windows.UTF16ToString(app.Name[:]), User: "unknown"}
			} else if p.Identity != id {
				continue
			}
			p.Usages = []model.Usage{{Path: paths[0], Relation: "restart manager", Access: "unknown"}}
			r.Processes = append(r.Processes, p)
		}
		return nil
	}
	for at := 0; at < len(paths); at += 128 {
		if err := correlate(paths[at:min(at+128, len(paths))]); err != nil {
			return r, err
		}
	}
	if limited > 0 {
		r.Warnings = append(r.Warnings, fmt.Sprintf("%d process/resource inspections were unavailable (permissions or changes).", limited))
	}
	r.Normalize()
	return r, nil
}

func (s *native) Kill(ctx context.Context, id model.Identity, force bool) error {
	if err := validate(id); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	access := uint32(windows.PROCESS_QUERY_LIMITED_INFORMATION)
	if force {
		access |= windows.PROCESS_TERMINATE
	}
	h, err := windows.OpenProcess(access, false, uint32(id.PID))
	if err != nil {
		return err
	}
	defer windows.CloseHandle(h)
	current, err := handleIdentity(h, id.PID)
	if err != nil || current != id {
		return ErrChanged
	}
	if force {
		return windows.TerminateProcess(h, 1)
	}
	sent := 0
	callback := windows.NewCallback(func(hwnd, lparam uintptr) uintptr {
		var pid uint32
		windowPID.Call(hwnd, uintptr(unsafe.Pointer(&pid)))
		if int(pid) == id.PID {
			ok, _, _ := postMessage.Call(hwnd, 0x0010, 0, 0)
			if ok != 0 {
				sent++
			}
		} // WM_CLOSE
		return 1
	})
	ok, _, err := enumWindows.Call(callback, 0)
	if ok == 0 {
		return fmt.Errorf("enumerate windows: %w", err)
	}
	if sent == 0 {
		return errors.New("no window accepted a graceful close request; use force kill explicitly if needed")
	}
	return nil
}
