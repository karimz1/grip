//go:build windows

package scanner

import (
	"golang.org/x/sys/windows"
	"os"
)

func holdTestFile(path string) (func(), error) {
	name, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return nil, err
	}
	share := uint32(0)
	all := uint32(windows.FILE_SHARE_READ | windows.FILE_SHARE_WRITE | windows.FILE_SHARE_DELETE)
	switch os.Getenv("OFLH_TEST_LOCK") {
	case "none":
		share = all
	case "read":
		share = all &^ windows.FILE_SHARE_READ
	case "write":
		share = all &^ windows.FILE_SHARE_WRITE
	case "range":
		share = all &^ windows.FILE_SHARE_DELETE
	}
	h, err := windows.CreateFile(name, windows.GENERIC_READ|windows.GENERIC_WRITE, share, nil, windows.OPEN_EXISTING, windows.FILE_ATTRIBUTE_NORMAL, 0)
	if err != nil {
		return nil, err
	}
	return func() { windows.CloseHandle(h) }, nil
}
