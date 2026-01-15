//go:build !windows

// Package loop provides Unix-only cancellation tests for command execution helpers.
package loop

import (
	"context"
	"errors"
	"os/exec"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/testutil"
)

type threadSafeLogger struct {
	mu    sync.Mutex
	lines []string
}

func (t *threadSafeLogger) WriteLine(line string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.lines = append(t.lines, line)
}

func (t *threadSafeLogger) String() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return strings.Join(t.lines, "\n")
}

func TestRunCommandCancelKillsChildProcess(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	logger := &threadSafeLogger{}
	cmd := exec.CommandContext(ctx, "sh", "-c", "sleep 10 & echo CHILD_PID=$!; wait")

	errCh := make(chan error, 1)
	go func() {
		errCh <- RunCommand(ctx, cmd, nil, logger)
	}()

	childPID, ok := testutil.WaitForChildPID(logger.String, 2*time.Second)
	if !ok {
		cancel()
		t.Fatalf("child pid not observed in runner output")
	}

	cancel()

	select {
	case err := <-errCh:
		if err == nil {
			t.Fatalf("expected cancellation error, got nil")
		}
		if !errors.Is(err, context.Canceled) {
			var exitErr *exec.ExitError
			if !errors.As(err, &exitErr) {
				t.Fatalf("expected context.Canceled or exit error, got %v", err)
			}
			if status, ok := exitErr.Sys().(syscall.WaitStatus); !ok || !status.Signaled() {
				t.Fatalf("expected context.Canceled or signaled exit, got %v", err)
			}
		}
	case <-time.After(5 * time.Second):
		t.Fatalf("timeout waiting for cancellation")
	}

	if err := testutil.WaitForPIDExit(childPID, 2*time.Second); err != nil {
		_ = syscall.Kill(childPID, syscall.SIGKILL)
		t.Fatalf("child process still running after cancel: %v", err)
	}
}
