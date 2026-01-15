//go:build !windows

// Package specs provides Unix-only runner cancellation tests.
package specs

import (
	"context"
	"errors"
	"syscall"
	"testing"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/testutil"
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

	childPID, ok := testutil.WaitForChildPID(output.String, 2*time.Second)
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

	if err := testutil.WaitForPIDExit(childPID, 2*time.Second); err != nil {
		_ = syscall.Kill(childPID, syscall.SIGKILL)
		t.Fatalf("child process still running after cancel: %v", err)
	}
}
