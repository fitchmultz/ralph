//go:build !windows

// Package loop provides runner inactivity regression coverage.
package loop

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestLoopRunnerInactivityResetsChanges(t *testing.T) {
	requireTool(t, "git")

	itemID := "RQ-1004"
	repoRoot, pinDir := setupLoopRepo(t, "", itemID)
	installHelperRunner(t)

	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	t.Setenv("RALPH_HELPER_PROCESS", "1")
	t.Setenv("RALPH_HELPER_MODE", "runner_inactivity")
	t.Setenv("RALPH_QUEUE_PATH", queuePath)
	t.Setenv("RALPH_ITEM_ID", itemID)
	t.Setenv("RALPH_REPO_ROOT", repoRoot)
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	logger := &syncLogger{}
	opts := loopOptions(repoRoot, pinDir, logger, false, false)
	opts.RunnerInactivitySeconds = 1
	runner, err := NewRunner(opts)
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	doneCh := make(chan error, 1)
	go func() {
		doneCh <- runner.Run(ctx)
	}()

	if !waitForLog(logger, "Runner inactive for", 5*time.Second) {
		cancel()
		t.Fatalf("expected inactivity reset log")
	}
	cancel()

	if err := <-doneCh; err != nil {
		t.Fatalf("Run returned error: %v", err)
	}

	status, err := StatusPorcelain(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("StatusPorcelain failed: %v", err)
	}
	if status != "" {
		t.Fatalf("expected clean repo after inactivity reset, got %q", status)
	}

	queueData, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}
	if !strings.Contains(string(queueData), itemID) {
		t.Fatalf("expected item %s to remain in queue", itemID)
	}

	assertNoWipBranches(t, repoRoot)
}
