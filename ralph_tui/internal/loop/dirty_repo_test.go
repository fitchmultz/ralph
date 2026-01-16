// Package loop provides tests for dirty repo policies.
package loop

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

func TestPreflightDirtyRepoErrorDoesNotQuarantine(t *testing.T) {
	requireTool(t, "git")
	repoRoot, pinDir := setupRepoWithQueue(t, "RQ-2001")
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	if err := os.WriteFile(filepath.Join(repoRoot, "README.md"), []byte("dirty\n"), 0o600); err != nil {
		t.Fatalf("write readme: %v", err)
	}
	untracked := filepath.Join(repoRoot, "scratch.txt")
	if err := os.WriteFile(untracked, []byte("notes"), 0o600); err != nil {
		t.Fatalf("write untracked: %v", err)
	}

	runner, err := NewRunner(Options{
		RepoRoot:        repoRoot,
		PinDir:          pinDir,
		Runner:          "codex",
		RunnerArgs:      []string{},
		ReasoningEffort: "auto",
		SleepSeconds:    0,
		MaxIterations:   1,
		OnlyTags:        []string{},
		Once:            true,
		RequireMain:     true,
		AutoCommit:      false,
		AutoPush:        false,
		DirtyRepoStart:  DirtyRepoPolicyError,
		DirtyRepoDuring: DirtyRepoPolicyQuarantine,
		AllowUntracked:  true,
		QuarantineClean: false,
		RedactionMode:   redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	err = runner.Run(context.Background())
	if err == nil {
		t.Fatalf("expected dirty repo error")
	}
	var dirtyErr *DirtyRepoError
	if !errors.As(err, &dirtyErr) {
		t.Fatalf("expected DirtyRepoError, got %T", err)
	}

	status, err := StatusDetails(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("StatusDetails failed: %v", err)
	}
	if status.IsClean(true) {
		t.Fatalf("expected dirty repo to remain dirty after preflight error")
	}
	if _, err := os.Stat(untracked); err != nil {
		t.Fatalf("expected untracked file to remain: %v", err)
	}
	branches := strings.TrimSpace(runCmd(t, repoRoot, "git", "branch", "--list", "ralph/wip/*"))
	if branches != "" {
		t.Fatalf("expected no wip branches, got %s", branches)
	}
}

func TestPreflightUntrackedBlocksWhenDisallowed(t *testing.T) {
	requireTool(t, "git")
	repoRoot, pinDir := setupRepoWithQueue(t, "RQ-2002")
	t.Setenv("RALPH_LOOP_SKIP_RUNNER_CHECK", "1")

	untracked := filepath.Join(repoRoot, "scratch.txt")
	if err := os.WriteFile(untracked, []byte("notes"), 0o600); err != nil {
		t.Fatalf("write untracked: %v", err)
	}

	runner, err := NewRunner(Options{
		RepoRoot:        repoRoot,
		PinDir:          pinDir,
		Runner:          "codex",
		RunnerArgs:      []string{},
		ReasoningEffort: "auto",
		SleepSeconds:    0,
		MaxIterations:   1,
		OnlyTags:        []string{},
		Once:            true,
		RequireMain:     true,
		AutoCommit:      false,
		AutoPush:        false,
		DirtyRepoStart:  DirtyRepoPolicyError,
		DirtyRepoDuring: DirtyRepoPolicyQuarantine,
		AllowUntracked:  false,
		QuarantineClean: false,
		RedactionMode:   redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	err = runner.Run(context.Background())
	if err == nil {
		t.Fatalf("expected dirty repo error")
	}
	if _, err := os.Stat(untracked); err != nil {
		t.Fatalf("expected untracked file to remain: %v", err)
	}
}

func setupRepoWithQueue(t *testing.T, itemID string) (string, string) {
	t.Helper()
	repoRoot := t.TempDir()
	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")

	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	if err := os.MkdirAll(pinDir, 0o700); err != nil {
		t.Fatalf("mkdir pin: %v", err)
	}
	if err := os.WriteFile(filepath.Join(repoRoot, "README.md"), []byte("base\n"), 0o600); err != nil {
		t.Fatalf("write readme: %v", err)
	}
	if err := os.WriteFile(filepath.Join(pinDir, "README.md"), []byte("pin readme\n"), 0o600); err != nil {
		t.Fatalf("write pin readme: %v", err)
	}
	if err := os.WriteFile(filepath.Join(pinDir, "lookup_table.md"), []byte("| Area | Notes |\n| --- | --- |\n"), 0o600); err != nil {
		t.Fatalf("write lookup: %v", err)
	}
	if err := os.WriteFile(filepath.Join(pinDir, "specs_builder.md"), []byte(""), 0o600); err != nil {
		t.Fatalf("write specs: %v", err)
	}
	if err := os.WriteFile(filepath.Join(pinDir, "specs_builder_docs.md"), []byte(""), 0o600); err != nil {
		t.Fatalf("write specs docs: %v", err)
	}
	if err := os.WriteFile(filepath.Join(pinDir, "implementation_done.md"), []byte("## Done\n"), 0o600); err != nil {
		t.Fatalf("write done: %v", err)
	}
	if err := os.WriteFile(filepath.Join(pinDir, "implementation_queue.md"), []byte(queueFixtureForDirtyRepo(itemID)), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "base")
	return repoRoot, pinDir
}

func queueFixtureForDirtyRepo(itemID string) string {
	return strings.TrimSpace("# Implementation Queue\n\n## Queue\n- [ ] "+itemID+" [code]: Dirty repo test. (README.md)\n  - Evidence: test\n  - Plan: test\n\n## Blocked\n\n## Parking Lot\n") + "\n"
}
