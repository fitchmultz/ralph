//go:build !windows

// Package loop provides cancellation regression tests for the loop runner.
package loop

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

type syncLogger struct {
	mu    sync.Mutex
	lines []string
}

func (s *syncLogger) WriteLine(line string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.lines = append(s.lines, line)
}

func (s *syncLogger) String() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return strings.Join(s.lines, "\n")
}

func (s *syncLogger) Contains(sub string) bool {
	return strings.Contains(s.String(), sub)
}

func TestLoopCancelDuringRunnerSkipsQuarantine(t *testing.T) {
	requireTool(t, "git")

	itemID := "RQ-1001"
	repoRoot, pinDir := setupLoopRepo(t, "", itemID)
	installHelperRunner(t)

	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	donePath := filepath.Join(pinDir, "implementation_done.md")
	t.Setenv("RALPH_HELPER_PROCESS", "1")
	t.Setenv("RALPH_HELPER_MODE", "runner_cancel")
	t.Setenv("RALPH_QUEUE_PATH", queuePath)
	t.Setenv("RALPH_ITEM_ID", itemID)
	t.Setenv("RALPH_REPO_ROOT", repoRoot)
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	logger := &syncLogger{}
	runner, err := NewRunner(loopOptions(repoRoot, pinDir, logger, true, false))
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	doneCh := make(chan error, 1)
	go func() {
		doneCh <- runner.Run(ctx)
	}()

	if !waitForLog(logger, "RUNNER_MARKED", 5*time.Second) {
		cancel()
		t.Fatalf("expected runner to mark queue item")
	}
	cancel()

	if err := <-doneCh; err != nil {
		t.Fatalf("Run returned error: %v", err)
	}

	assertQueueMoved(t, queuePath, donePath, itemID)
	assertNoSupervisor(t, logger)
	assertNoWipBranches(t, repoRoot)
	assertLastCommitHas(t, repoRoot, itemID)
}

func TestLoopCancelDuringMakeCI(t *testing.T) {
	requireTool(t, "git")
	requireTool(t, "make")

	itemID := "RQ-1002"
	makefile := "ci:\n\t@echo CI_START\n\t@sleep 30\n"
	repoRoot, pinDir := setupLoopRepo(t, makefile, itemID)
	installHelperRunner(t)

	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	donePath := filepath.Join(pinDir, "implementation_done.md")
	t.Setenv("RALPH_HELPER_PROCESS", "1")
	t.Setenv("RALPH_HELPER_MODE", "runner_complete_with_code_change")
	t.Setenv("RALPH_QUEUE_PATH", queuePath)
	t.Setenv("RALPH_ITEM_ID", itemID)
	t.Setenv("RALPH_REPO_ROOT", repoRoot)
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	logger := &syncLogger{}
	runner, err := NewRunner(loopOptions(repoRoot, pinDir, logger, true, false))
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	doneCh := make(chan error, 1)
	go func() {
		doneCh <- runner.Run(ctx)
	}()

	if !waitForLog(logger, "CI_START", 5*time.Second) {
		cancel()
		t.Fatalf("expected make ci to start")
	}
	cancel()

	if err := <-doneCh; err != nil {
		t.Fatalf("Run returned error: %v", err)
	}

	assertQueueMoved(t, queuePath, donePath, itemID)
	assertNoSupervisor(t, logger)
	assertNoWipBranches(t, repoRoot)
	assertLastCommitHas(t, repoRoot, itemID)

	status, err := StatusPorcelain(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("StatusPorcelain failed: %v", err)
	}
	if status != "" {
		t.Fatalf("expected clean repo after cancel cleanup, got %q", status)
	}
}

func TestLoopCancelDuringPush(t *testing.T) {
	requireTool(t, "git")

	itemID := "RQ-1003"
	repoRoot, pinDir := setupLoopRepo(t, "", itemID)
	originDir := filepath.Join(t.TempDir(), "origin.git")
	if err := os.MkdirAll(originDir, 0o700); err != nil {
		t.Fatalf("mkdir origin: %v", err)
	}
	runCmd(t, originDir, "git", "init", "--bare")
	runCmd(t, repoRoot, "git", "remote", "add", "origin", originDir)
	runCmd(t, repoRoot, "git", "push", "-u", "origin", "main")

	hookDir := filepath.Join(repoRoot, ".git", "hooks")
	if err := os.MkdirAll(hookDir, 0o700); err != nil {
		t.Fatalf("mkdir hooks: %v", err)
	}
	hookPath := filepath.Join(hookDir, "pre-push")
	if err := os.WriteFile(hookPath, []byte("#!/bin/sh\nsleep 30\n"), 0o700); err != nil {
		t.Fatalf("write hook: %v", err)
	}

	installHelperRunner(t)
	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	donePath := filepath.Join(pinDir, "implementation_done.md")
	t.Setenv("RALPH_HELPER_PROCESS", "1")
	t.Setenv("RALPH_HELPER_MODE", "runner_complete_specs_only")
	t.Setenv("RALPH_QUEUE_PATH", queuePath)
	t.Setenv("RALPH_ITEM_ID", itemID)
	t.Setenv("RALPH_REPO_ROOT", repoRoot)
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	logger := &syncLogger{}
	runner, err := NewRunner(loopOptions(repoRoot, pinDir, logger, true, true))
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	doneCh := make(chan error, 1)
	go func() {
		doneCh <- runner.Run(ctx)
	}()

	if !waitForLog(logger, "Pushing", 5*time.Second) {
		cancel()
		t.Fatalf("expected push to start")
	}
	cancel()

	if err := <-doneCh; err != nil {
		t.Fatalf("Run returned error: %v", err)
	}

	assertQueueMoved(t, queuePath, donePath, itemID)
	assertNoSupervisor(t, logger)
	assertNoWipBranches(t, repoRoot)
	assertLastCommitHas(t, repoRoot, itemID)

	if !runner.pushFailed {
		t.Fatalf("expected pushFailed to be set")
	}
	if !runner.pushCanceled {
		t.Fatalf("expected pushCanceled to be set")
	}
	ahead, err := AheadCount(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("AheadCount failed: %v", err)
	}
	if ahead <= 0 {
		t.Fatalf("expected repo to be ahead after canceled push")
	}
}

func loopOptions(repoRoot string, pinDir string, logger Logger, autoCommit bool, autoPush bool) Options {
	return Options{
		RepoRoot:          repoRoot,
		PinDir:            pinDir,
		PromptPath:        "",
		SupervisorPrompt:  "",
		Runner:            "codex",
		RunnerArgs:        []string{},
		ReasoningEffort:   "auto",
		SleepSeconds:      0,
		MaxIterations:     1,
		MaxStalled:        0,
		MaxRepairAttempts: 0,
		OnlyTags:          []string{},
		Once:              true,
		RequireMain:       true,
		AutoCommit:        autoCommit,
		AutoPush:          autoPush,
		RedactionMode:     redaction.ModeSecretsOnly,
		Logger:            logger,
	}
}

func setupLoopRepo(t *testing.T, makefile string, itemID string) (string, string) {
	repoRoot := t.TempDir()
	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")

	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	if err := os.MkdirAll(pinDir, 0o700); err != nil {
		t.Fatalf("mkdir pin: %v", err)
	}

	writeFileContent(t, filepath.Join(repoRoot, "README.md"), "base\n")
	if makefile != "" {
		writeFileContent(t, filepath.Join(repoRoot, "Makefile"), makefile)
	}
	writeFileContent(t, filepath.Join(pinDir, "README.md"), "pin readme\n")
	writeFileContent(t, filepath.Join(pinDir, "lookup_table.md"), "| Area | Notes |\n| --- | --- |\n")
	writeFileContent(t, filepath.Join(pinDir, "specs_builder.md"), "")
	writeFileContent(t, filepath.Join(pinDir, "implementation_done.md"), "## Done\n")
	writeFileContent(t, filepath.Join(pinDir, "implementation_queue.md"), queueFixture(itemID))

	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "base")
	return repoRoot, pinDir
}

func queueFixture(itemID string) string {
	return strings.TrimSpace(fmt.Sprintf(`# Implementation Queue

## Queue
- [ ] %s [code]: Cancel test item. (README.md)
  - Evidence: test evidence.
  - Plan: test plan.

## Blocked

## Parking Lot
`, itemID)) + "\n"
}

func installHelperRunner(t *testing.T) {
	t.Helper()
	binDir := t.TempDir()
	helperPath := filepath.Join(binDir, "codex")
	if err := os.Symlink(os.Args[0], helperPath); err != nil {
		t.Fatalf("symlink helper: %v", err)
	}
	t.Setenv("PATH", fmt.Sprintf("%s%c%s", binDir, os.PathListSeparator, os.Getenv("PATH")))
}

func waitForLog(logger *syncLogger, needle string, timeout time.Duration) bool {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if logger.Contains(needle) {
			return true
		}
		time.Sleep(20 * time.Millisecond)
	}
	return false
}

func assertQueueMoved(t *testing.T, queuePath string, donePath string, itemID string) {
	t.Helper()
	queueData, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}
	if strings.Contains(string(queueData), itemID) {
		t.Fatalf("expected item %s to be removed from queue", itemID)
	}
	doneData, err := os.ReadFile(donePath)
	if err != nil {
		t.Fatalf("read done: %v", err)
	}
	if !strings.Contains(string(doneData), itemID) {
		t.Fatalf("expected item %s to be moved to done", itemID)
	}
}

func assertNoSupervisor(t *testing.T, logger *syncLogger) {
	t.Helper()
	if logger.Contains("Supervisor attempt") {
		t.Fatalf("expected no supervisor attempts, got logs: %s", logger.String())
	}
}

func assertNoWipBranches(t *testing.T, repoRoot string) {
	t.Helper()
	branches := strings.TrimSpace(runCmd(t, repoRoot, "git", "branch", "--list", "ralph/wip/*"))
	if branches != "" {
		t.Fatalf("expected no wip branches, got %s", branches)
	}
}

func assertLastCommitHas(t *testing.T, repoRoot string, fragment string) {
	t.Helper()
	subject := strings.TrimSpace(runCmd(t, repoRoot, "git", "log", "-1", "--pretty=%s"))
	if !strings.Contains(subject, fragment) {
		t.Fatalf("expected last commit to contain %q, got %q", fragment, subject)
	}
}
