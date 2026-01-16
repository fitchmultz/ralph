//go:build !windows

package procgroup

import (
	"bytes"
	"context"
	"errors"
	"os/exec"
	"sync"
	"syscall"
	"testing"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/testutil"
)

type threadSafeBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

func (b *threadSafeBuffer) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.Write(p)
}

func (b *threadSafeBuffer) String() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.String()
}

func TestConfigureSetsProcessGroup(t *testing.T) {
	cmd := exec.Command("sh", "-c", "echo ready")

	Configure(cmd)

	if cmd.SysProcAttr == nil {
		t.Fatalf("expected SysProcAttr to be initialized")
	}
	if !cmd.SysProcAttr.Setpgid {
		t.Fatalf("expected Setpgid to be true")
	}
	if cmd.Cancel == nil {
		t.Fatalf("expected Cancel handler to be set")
	}
}

func TestConfigureCancelKillsProcessGroup(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	cmd := exec.CommandContext(ctx, "sh", "-c", "sleep 10 & echo CHILD_PID=$!; wait")
	Configure(cmd)

	var output threadSafeBuffer
	cmd.Stdout = &output
	cmd.Stderr = &output

	errCh := make(chan error, 1)
	go func() {
		errCh <- cmd.Run()
	}()

	childPID, ok := testutil.WaitForChildPID(output.String, 2*time.Second)
	if !ok {
		cancel()
		t.Fatalf("child pid not observed in command output")
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
			status, ok := exitErr.Sys().(syscall.WaitStatus)
			if !ok || !status.Signaled() {
				t.Fatalf("expected signaled exit, got %v", err)
			}
		}
	case <-time.After(12 * time.Second):
		t.Fatalf("timeout waiting for cancellation")
	}

	if err := testutil.WaitForPIDExit(childPID, 2*time.Second); err != nil {
		_ = syscall.Kill(childPID, syscall.SIGKILL)
		t.Fatalf("child process still running after cancel: %v", err)
	}
}
