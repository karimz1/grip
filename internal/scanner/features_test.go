package scanner

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

const nativeFixtureTimeout = 10 * time.Second

// Native CI gates: no OS skips. Each backend must discover the live child,
// supply resource/parent data, and distinguish its native lock evidence.
func TestNativeFeatureContract(t *testing.T) {
	t.Setenv("OFLH_TEST_LOCK", "posix")
	t.Setenv("OFLH_TEST_BUSY", "1")

	s, err := New()
	if err != nil {
		t.Fatal(err)
	}

	path := filepath.Join(t.TempDir(), "native locked ü file.dat")
	original := bytes.Repeat([]byte("x"), 4096)

	if err := os.WriteFile(path, original, 0600); err != nil {
		t.Fatal(err)
	}

	target, err := model.NewTarget(path)
	if err != nil {
		t.Fatal(err)
	}

	child := startHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)

	if !p.MemoryKnown || p.MemoryBytes == 0 {
		t.Fatalf("missing memory: %+v", p)
	}

	if p.ParentPID != os.Getpid() ||
		len(p.Ancestors) == 0 ||
		p.Ancestors[0].PID != os.Getpid() ||
		p.Ancestors[0].Started == "" {
		t.Fatalf("missing actionable parent: %+v", p)
	}

	found := false
	for _, u := range p.Usages {
		if target.Matches(u.Path, nil) && u.Lock != "" {
			found = true
			break
		}
	}

	if !found {
		t.Fatalf("native lock evidence missing: %+v", p.Usages)
	}

	// Allow measurable CPU time to accrue on coarse native counters.
	time.Sleep(250 * time.Millisecond)

	p = scanPID(t, s, path, child.Process.Pid)
	if !p.CPUKnown ||
		math.IsNaN(p.CPUPercent) ||
		p.CPUPercent <= 0 ||
		p.CPUPercent > 100 {
		t.Fatalf("invalid second CPU sample: %+v", p)
	}

	stale := model.Identity{
		PID:     p.PID,
		Started: p.Started + "-stale",
	}
	if err := s.Kill(t.Context(), stale, true); err == nil {
		t.Fatal("stale process identity accepted")
	}

	// Only our isolated helper is stopped. Probe code must leave contents intact.
	if err := s.Kill(t.Context(), p.Identity, true); err != nil {
		t.Fatal(err)
	}

	_ = child.Wait()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file after helper termination: %v", err)
	}
	if !bytes.Equal(data, original) {
		t.Fatal("file contents changed")
	}
}

func TestNativeOpenFileIsNotALock(t *testing.T) {
	t.Setenv("OFLH_TEST_LOCK", "none")

	s, err := New()
	if err != nil {
		t.Fatal(err)
	}

	path := filepath.Join(t.TempDir(), "unlocked.dat")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}

	child := startHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)

	for _, u := range p.Usages {
		if u.Lock != "" {
			t.Fatalf("ordinary open file misreported as locked: %+v", u)
		}
	}
}

// Releasing a handle must remove lock evidence without requiring process exit.
func TestNativeLockReleaseRefresh(t *testing.T) {
	t.Setenv("OFLH_TEST_LOCK", "posix")

	s, err := New()
	if err != nil {
		t.Fatal(err)
	}

	path := filepath.Join(t.TempDir(), "release.dat")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}

	child, input, output, _ := startControlledHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)

	if !hasLock(*p) {
		t.Fatal("initial lock not detected")
	}

	if _, err := fmt.Fprintln(input, "release"); err != nil {
		t.Fatal(err)
	}

	awaitHelperLine(t, output, "released")

	target, err := model.NewTarget(path)
	if err != nil {
		t.Fatal(err)
	}

	result, err := s.Scan(t.Context(), target)
	if err != nil {
		t.Fatal(err)
	}

	for _, process := range result.Processes {
		if hasLock(process) {
			t.Fatalf("released lock remains: %+v", process)
		}
	}

	// The same live process identity is still actionable after releasing its file.
	if err := s.Kill(t.Context(), p.Identity, true); err != nil {
		t.Fatal(err)
	}

	_ = child.Wait()
}

func hasLock(p model.Process) bool {
	for _, u := range p.Usages {
		if u.Lock != "" {
			return true
		}
	}
	return false
}

func awaitHelperLine(t *testing.T, reader *bufio.Reader, want string) {
	t.Helper()

	type result struct {
		line string
		err  error
	}

	ch := make(chan result, 1)

	go func() {
		line, err := reader.ReadString('\n')
		ch <- result{
			line: strings.TrimSpace(line),
			err:  err,
		}
	}()

	select {
	case result := <-ch:
		if result.err != nil {
			t.Fatalf("helper response: %v", result.err)
		}
		if result.line != want {
			t.Fatalf("helper response %q, want %q", result.line, want)
		}

	case <-time.After(nativeFixtureTimeout):
		t.Fatal("helper response timeout")
	}
}

// Use only a disposable parent/child pair, never the test runner's own parent.
func TestNativeParentTermination(t *testing.T) {
	t.Setenv("OFLH_TEST_LOCK", "posix")
	t.Setenv("OFLH_TEST_PARENT", "1")

	s, err := New()
	if err != nil {
		t.Fatal(err)
	}

	path := filepath.Join(t.TempDir(), "child-held.dat")
	if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
		t.Fatal(err)
	}

	parent, _, _, pid := startControlledHelper(t, path)
	child := scanPID(t, s, path, pid)

	if child.ParentPID != parent.Process.Pid ||
		len(child.Ancestors) == 0 ||
		child.Ancestors[0].PID != parent.Process.Pid {
		t.Fatalf("wrong parent chain: %+v", child)
	}

	id := model.Identity{
		PID:     child.Ancestors[0].PID,
		Started: child.Ancestors[0].Started,
	}

	t.Cleanup(func() {
		_ = s.Kill(t.Context(), child.Identity, true)
	})

	stale := id
	stale.Started += "-stale"

	if err := s.Kill(t.Context(), stale, true); err == nil {
		t.Fatal("stale parent identity accepted")
	}

	if err := s.Kill(t.Context(), id, true); err != nil {
		t.Fatal(err)
	}

	done := make(chan error, 1)
	go func() {
		done <- parent.Wait()
	}()

	select {
	case <-done:
	case <-time.After(nativeFixtureTimeout):
		t.Fatal("parent did not terminate")
	}

	// Child exits on its parent's pipe closing. Poll evidence, not a guessed delay.
	target, err := model.NewTarget(path)
	if err != nil {
		t.Fatal(err)
	}

	deadline := time.Now().Add(15 * time.Second)

	for {
		result, err := s.Scan(t.Context(), target)
		if err != nil {
			t.Fatal(err)
		}

		locked := false
		for _, process := range result.Processes {
			if hasLock(process) {
				locked = true
				break
			}
		}

		if !locked {
			break
		}

		if time.Now().After(deadline) {
			t.Fatal("child lock remained after parent pipe closed")
		}

		time.Sleep(50 * time.Millisecond)
	}

	if err := s.Kill(t.Context(), id, true); err == nil {
		t.Fatal("exited parent identity accepted")
	}
}

func TestNativeLockModes(t *testing.T) {
	for _, mode := range []string{"read", "write", "range"} {
		t.Run(mode, func(t *testing.T) {
			t.Setenv("OFLH_TEST_LOCK", mode)

			s, err := New()
			if err != nil {
				t.Fatal(err)
			}

			path := filepath.Join(t.TempDir(), "mode.dat")
			if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
				t.Fatal(err)
			}

			child := startHelper(t, path)
			p := scanPID(t, s, path, child.Process.Pid)

			want := expectedLockEvidence(mode)

			for _, u := range p.Usages {
				if u.Lock != "" &&
					strings.Contains(strings.ToLower(u.Lock), want) {
					return
				}
			}

			t.Fatalf("expected %s evidence, got %+v", want, p.Usages)
		})
	}
}

// TestNativeExternalLockFixture verifies that the scanner detects locks
// created by an independent native program rather than by the Go test helper.
//
// The fixture uses:
//   - Windows: CreateFile / LockFileEx
//   - macOS/Linux: fcntl
func TestNativeExternalLockFixture(t *testing.T) {
	fixture := buildNativeLockFixture(t)

	for _, mode := range []string{"open", "read", "write", "range"} {
		t.Run(mode, func(t *testing.T) {
			s, err := New()
			if err != nil {
				t.Fatal(err)
			}

			path := filepath.Join(t.TempDir(), "external-lock.dat")
			if err := os.WriteFile(path, make([]byte, 4096), 0600); err != nil {
				t.Fatal(err)
			}

			args := []string{mode, path}
			if mode == "range" {
				args = append(args, "128", "256")
			}

			cmd := exec.Command(fixture, args...)

			stdout, err := cmd.StdoutPipe()
			if err != nil {
				t.Fatalf("native lock fixture stdout: %v", err)
			}

			var stderr bytes.Buffer
			cmd.Stderr = &stderr

			if err := cmd.Start(); err != nil {
				t.Fatalf("start native lock fixture: %v", err)
			}

			waited := false

			t.Cleanup(func() {
				if waited || cmd.Process == nil {
					return
				}

				_ = cmd.Process.Kill()
				_ = cmd.Wait()
				waited = true
			})

			reader := bufio.NewReader(stdout)
			waitForNativeFixtureReady(t, reader, &stderr)

			p := scanPID(t, s, path, cmd.Process.Pid)

			if mode == "open" {
				for _, u := range p.Usages {
					if u.Lock != "" {
						t.Fatalf(
							"ordinary native open misreported as locked: %+v",
							u,
						)
					}
				}
				return
			}

			want := expectedLockEvidence(mode)

			for _, u := range p.Usages {
				if u.Lock != "" &&
					strings.Contains(strings.ToLower(u.Lock), want) {
					return
				}
			}

			t.Fatalf(
				"expected native %s lock evidence containing %q, got %+v",
				mode,
				want,
				p.Usages,
			)
		})
	}
}

func expectedLockEvidence(mode string) string {
	if runtime.GOOS == "windows" {
		if mode == "range" {
			return "delete denied"
		}
		return mode + " denied"
	}

	if mode == "range" {
		return "128"
	}

	return mode
}

func waitForNativeFixtureReady(
	t *testing.T,
	reader *bufio.Reader,
	stderr *bytes.Buffer,
) {
	t.Helper()

	type result struct {
		line string
		err  error
	}

	ready := make(chan result, 1)

	go func() {
		for {
			line, err := reader.ReadString('\n')
			line = strings.TrimSpace(line)

			if line != "" {
				t.Logf("lockfixture: %s", line)
			}

			if line == "LOCK FIXTURE READY" ||
				strings.HasPrefix(line, "READY ") {
				ready <- result{line: line}
				return
			}

			if err != nil {
				ready <- result{err: err}
				return
			}
		}
	}()

	select {
	case result := <-ready:
		if result.err != nil {
			detail := strings.TrimSpace(stderr.String())
			if detail != "" {
				t.Fatalf(
					"native lock fixture exited before ready: %v: %s",
					result.err,
					detail,
				)
			}

			if result.err == io.EOF {
				t.Fatal("native lock fixture exited before ready")
			}

			t.Fatalf(
				"native lock fixture failed before ready: %v",
				result.err,
			)
		}

	case <-time.After(nativeFixtureTimeout):
		detail := strings.TrimSpace(stderr.String())
		if detail != "" {
			t.Fatalf(
				"native lock fixture readiness timeout: %s",
				detail,
			)
		}

		t.Fatal("native lock fixture readiness timeout")
	}
}

func buildNativeLockFixture(t *testing.T) string {
	t.Helper()

	source := filepath.Join(
		"testdata",
		"lockfixture",
		"lock-fixture.c",
	)

	if _, err := os.Stat(source); err != nil {
		t.Fatalf("native lock fixture source %q: %v", source, err)
	}

	dir := t.TempDir()

	if runtime.GOOS == "windows" {
		return buildNativeLockFixtureWindows(t, source, dir)
	}

	return buildNativeLockFixtureUnix(t, source, dir)
}

func buildNativeLockFixtureUnix(
	t *testing.T,
	source string,
	dir string,
) string {
	t.Helper()

	compiler := findCompiler("cc", "clang", "gcc")
	if compiler == "" {
		failOrSkipNativeCompiler(
			t,
			"no C compiler found (tried cc, clang, gcc)",
		)
		return ""
	}

	output := filepath.Join(dir, "lockfixture")

	cmd := exec.Command(
		compiler,
		"-Wall",
		"-Wextra",
		"-O2",
		source,
		"-o",
		output,
	)

	data, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf(
			"compile native lock fixture with %s: %v\n%s",
			compiler,
			err,
			data,
		)
	}

	return output
}

func buildNativeLockFixtureWindows(
	t *testing.T,
	source string,
	dir string,
) string {
	t.Helper()

	output := filepath.Join(dir, "lockfixture.exe")

	// Prefer MSVC when its developer environment is already available.
	if compiler := findCompiler("cl.exe", "cl"); compiler != "" {
		cmd := exec.Command(
			compiler,
			"/nologo",
			"/W4",
			"/O2",
			"/Fe:"+output,
			source,
		)

		data, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf(
				"compile native lock fixture with %s: %v\n%s",
				compiler,
				err,
				data,
			)
		}

		if _, err := os.Stat(output); err != nil {
			t.Fatalf(
				"MSVC completed but fixture %q was not created: %v",
				output,
				err,
			)
		}

		return output
	}

	// Support MinGW/LLVM environments as well.
	if compiler := findCompiler(
		"gcc.exe",
		"gcc",
		"clang.exe",
		"clang",
	); compiler != "" {
		cmd := exec.Command(
			compiler,
			"-Wall",
			"-Wextra",
			"-O2",
			source,
			"-o",
			output,
		)

		data, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf(
				"compile native lock fixture with %s: %v\n%s",
				compiler,
				err,
				data,
			)
		}

		if _, err := os.Stat(output); err != nil {
			t.Fatalf(
				"compiler completed but fixture %q was not created: %v",
				output,
				err,
			)
		}

		return output
	}

	failOrSkipNativeCompiler(
		t,
		"no C compiler found (tried cl, gcc, clang)",
	)

	return ""
}

func findCompiler(names ...string) string {
	for _, name := range names {
		path, err := exec.LookPath(name)
		if err == nil {
			return path
		}
	}

	return ""
}

func failOrSkipNativeCompiler(t *testing.T, message string) {
	t.Helper()

	if os.Getenv("CI") != "" {
		t.Fatal(message)
	}

	t.Skip(message)
}