//go:build windows

package scanner

import (
	"errors"
	"golang.org/x/sys/windows"
)

// Probe sharing compatibility without changing file contents or acquiring a
// byte-range lock. Other errors (notably access denied) are not lock evidence.
func sharingConflict(path string) string {
	name, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return ""
	}
	for _, probe := range []struct {
		access uint32
		name   string
	}{{windows.GENERIC_READ, "read"}, {windows.GENERIC_WRITE, "write"}, {windows.DELETE, "delete"}} {
		h, err := windows.CreateFile(name, probe.access, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE|windows.FILE_SHARE_DELETE, nil, windows.OPEN_EXISTING, windows.FILE_ATTRIBUTE_NORMAL, 0)
		if err == nil {
			windows.CloseHandle(h)
			continue
		}
		if errors.Is(err, windows.ERROR_SHARING_VIOLATION) {
			return "SHARING CONFLICT: " + probe.name + " denied; reported file user, lock owner unverified"
		}
	}
	return ""
}
