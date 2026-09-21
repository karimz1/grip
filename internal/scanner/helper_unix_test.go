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
	m, err := unix.Mmap(int(f.Fd()), 0, 4096, unix.PROT_READ, unix.MAP_PRIVATE)
	if err != nil {
		f.Close()
		return nil, err
	}
	return func() { unix.Munmap(m); f.Close() }, nil
}
