//go:build !windows

// Package specs provides Unix-only runner cancellation tests.
package specs

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"
)

func TestBuildRunnerCancelKillsChildProcess(t *testing.T) {
	template := "AGENTS.md\n" + interactivePlaceholder + "\n" + innovatePlaceholder + "\nProcess group test."
	pinDir := writeSpecsPinDir(t, template)
	output := &lockedBuffer{}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	errCh := make(chan error, 1)
	go func() {
		_, err := Build(ctx, BuildOptions{
			RepoRoot:      pinDir,
			PinDir:        pinDir,
			Runner:        RunnerCodex,
			RunnerBackend: testRunnerBackend{mode: "parent"},
			Stdout:        output,
			Stderr:        output,
		})
		errCh <- err
	}()

	childPID, ok := waitForChildPID(output, 2*time.Second)
	if !ok {
		cancel()
		t.Fatalf("child pid not observed in runner output")
	}

	cancel()

	select {
	case err := <-errCh:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("expected context.Canceled, got %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatalf("timeout waiting for build cancellation")
	}

	if err := waitForPIDExit(childPID, 2*time.Second); err != nil {
		_ = syscall.Kill(childPID, syscall.SIGKILL)
		t.Fatalf("child process still running after cancel: %v", err)
	}
}

func waitForChildPID(output *lockedBuffer, timeout time.Duration) (int, bool) {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if pid, ok := parseChildPID(output.String()); ok {
			return pid, true
		}
		time.Sleep(10 * time.Millisecond)
	}
	return 0, false
}

func parseChildPID(output string) (int, bool) {
	for _, line := range strings.Split(output, "\n") {
		if strings.HasPrefix(line, "CHILD_PID=") {
			value := strings.TrimPrefix(line, "CHILD_PID=")
			pid, err := strconv.Atoi(strings.TrimSpace(value))
			if err == nil && pid > 0 {
				return pid, true
			}
		}
	}
	return 0, false
}

func waitForPIDExit(pid int, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if !isPIDRunningForTest(pid) {
			return nil
		}
		time.Sleep(25 * time.Millisecond)
	}
	return fmt.Errorf("pid %d still running", pid)
}

func isPIDRunningForTest(pid int) bool {
	if pid <= 0 {
		return false
	}
	return syscall.Kill(pid, 0) == nil
}
