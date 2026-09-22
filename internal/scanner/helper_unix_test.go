//go:build linux || darwin

package scanner

import (
	"golang.org/x/sys/unix"
	"os"
)

func holdTestFile(path string) (func(), error) {
	f, err := os.OpenFile(path, os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	if os.Getenv("OFLH_TEST_LOCK") == "1" {
		if err := unix.Flock(int(f.Fd()), unix.LOCK_EX); err != nil {
			f.Close()
			return nil, err
		}
	}
	if os.Getenv("OFLH_TEST_LOCK") == "posix" {
		lock := unix.Flock_t{Type: unix.F_WRLCK, Whence: 0, Start: 0, Len: 0}
		if err := unix.FcntlFlock(f.Fd(), unix.F_SETLK, &lock); err != nil {
			f.Close()
			return nil, err
		}
	}
	m, err := unix.Mmap(int(f.Fd()), 0, 4096, unix.PROT_READ, unix.MAP_PRIVATE)
	if err != nil {
		f.Close()
		return nil, err
	}
	return func() { unix.Munmap(m); f.Close() }, nil
}
