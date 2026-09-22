package scanner

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/karimz1/open-file-lock-handle/internal/model"
)

// A pipe handshake makes discovery deterministic; no sleep is used to guess readiness.
func TestScannerHelperProcess(t *testing.T) {
	if os.Getenv("OFLH_SCANNER_HELPER") != "1" {
		return
	}
	path := os.Getenv("OFLH_SCANNER_FILE")
	if err := os.Chdir(filepath.Dir(path)); err != nil {
		os.Exit(2)
	}
	if os.Getenv("OFLH_TEST_PARENT") == "1" {
		child := exec.Command(os.Args[0], "-test.run=^TestScannerHelperProcess$")
		child.Env = append(os.Environ(), "OFLH_TEST_PARENT=0")
		input, err := child.StdinPipe()
		if err != nil {
			os.Exit(4)
		}
		output, err := child.StdoutPipe()
		if err != nil {
			os.Exit(4)
		}
		child.Stderr = os.Stderr
		if child.Start() != nil {
			os.Exit(4)
		}
		line, _ := bufio.NewReader(output).ReadString('\n')
		if strings.TrimSpace(line) != "ready" {
			os.Exit(4)
		}
		fmt.Printf("ready %d\n", child.Process.Pid)
		_, _ = io.Copy(input, os.Stdin)
		input.Close()
		_ = child.Wait()
		os.Exit(0)
	}
	if os.Getenv("OFLH_TEST_BUSY") == "1" {
		go func() {
			var counter atomic.Uint64
			for {
				counter.Add(1)
			}
		}()
	}
	closeFile, err := holdTestFile(path)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(3)
	}
	fmt.Println("ready")
	input := bufio.NewScanner(os.Stdin)
	for input.Scan() {
		if input.Text() == "release" && closeFile != nil {
			closeFile()
			closeFile = nil
			fmt.Println("released")
			continue
		}
		break
	}
	if closeFile != nil {
		closeFile()
	}
	os.Exit(0)
}

func startHelper(t *testing.T, path string) *exec.Cmd {
	t.Helper()
	cmd, _, _, _ := startControlledHelper(t, path)
	return cmd
}

func startControlledHelper(t *testing.T, path string) (*exec.Cmd, io.WriteCloser, *bufio.Reader, int) {
	t.Helper()
	cmd := exec.Command(os.Args[0], "-test.run=^TestScannerHelperProcess$")
	cmd.Env = append(os.Environ(), "OFLH_SCANNER_HELPER=1", "OFLH_SCANNER_FILE="+path)
	in, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	out, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		in.Close()
		if cmd.ProcessState == nil {
			cmd.Process.Kill()
			cmd.Wait()
		}
	})
	reader := bufio.NewReader(out)
	pid := cmd.Process.Pid
	ready := make(chan string, 1)
	go func() { line, _ := reader.ReadString('\n'); ready <- line }()
	select {
	case line := <-ready:
		fields := strings.Fields(line)
		if len(fields) == 0 || fields[0] != "ready" {
			t.Fatalf("helper failed: %q", line)
		}
		if len(fields) == 2 {
			pid, err = strconv.Atoi(fields[1])
			if err != nil {
				t.Fatal(err)
			}
		}
	case <-time.After(20 * time.Second):
		t.Fatal("helper readiness timed out")
	}
	return cmd, in, reader, pid
}

func scanPID(t *testing.T, s Scanner, path string, pid int) *model.Process {
	t.Helper()
	target, err := model.NewTarget(path)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Second)
	defer cancel()
	result, err := s.Scan(ctx, target)
	if err != nil {
		t.Fatal(err)
	}
	for _, p := range result.Processes {
		if p.PID == pid {
			return &p
		}
	}
	t.Fatalf("PID %d not discovered for %q; warnings: %v", pid, path, result.Warnings)
	return nil
}

func TestNativeDiscoveryAndTermination(t *testing.T) {
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	path := filepath.Join(dir, "Unicode ü file.dll")
	if err := os.WriteFile(path, []byte(strings.Repeat("x", 4096)), 0600); err != nil {
		t.Fatal(err)
	}
	child := startHelper(t, path)
	p := scanPID(t, s, path, child.Process.Pid)
	if p.Started == "" || p.Executable == "" {
		t.Fatalf("missing process metadata: %+v", p)
	}
	if len(p.Usages) == 0 {
		t.Fatal("missing usage evidence")
	}
	directoryProcess := scanPID(t, s, dir, child.Process.Pid)
	if runtime.GOOS != "windows" {
		cwd, mapped := false, false
		for _, u := range directoryProcess.Usages {
			cwd = cwd || u.Relation == "cwd"
			mapped = mapped || u.Relation == "mapped"
		}
		if !cwd || !mapped {
			t.Fatalf("expected cwd and mapped evidence: %+v", directoryProcess.Usages)
		}
	}
	wrong := p.Identity
	wrong.Started += "-stale"
	if err := s.Kill(context.Background(), wrong, true); err == nil {
		t.Fatal("stale identity was accepted")
	}
	if runtime.GOOS == "windows" {
		if err := s.Kill(context.Background(), p.Identity, false); err == nil {
			t.Fatal("console helper should not claim graceful WM_CLOSE succeeded")
		}
	}
	if err := s.Kill(context.Background(), p.Identity, true); err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- child.Wait() }()
	select {
	case <-done:
	case <-time.After(10 * time.Second):
		t.Fatal("force termination did not stop helper")
	}
	if err := s.Kill(context.Background(), p.Identity, true); err == nil {
		t.Fatal("exited process was accepted")
	}
}

func TestNativeCancelledScan(t *testing.T) {
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	target, err := model.NewTarget(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := s.Scan(ctx, target); err != context.Canceled {
		t.Fatalf("cancelled scan: %v", err)
	}
}

func TestRefuseUnsafeIdentities(t *testing.T) {
	s, err := New()
	if err != nil {
		t.Fatal(err)
	}
	for _, id := range []model.Identity{{PID: 0, Started: "x"}, {PID: 1, Started: "x"}, {PID: -3, Started: "x"}, {PID: os.Getpid(), Started: "x"}, {PID: 42}} {
		if err := s.Kill(context.Background(), id, true); err == nil {
			t.Errorf("accepted unsafe identity %+v", id)
		}
	}
}
