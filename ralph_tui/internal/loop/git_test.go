// Package loop provides tests for git helper error reporting.
package loop

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCurrentBranchErrorIncludesStderrTail(t *testing.T) {
	requireTool(t, "git")
	repoRoot := t.TempDir()
	_, err := CurrentBranch(context.Background(), repoRoot)
	if err == nil {
		t.Fatal("expected error for non-git repo")
	}
	var gitErr *GitCommandError
	if !errors.As(err, &gitErr) {
		t.Fatalf("expected GitCommandError, got %T", err)
	}
	lines := gitErr.DetailLines()
	joined := strings.Join(lines, "\n")
	if !strings.Contains(joined, "stderr (tail):") {
		t.Fatalf("expected stderr tail in details, got: %s", joined)
	}
}

func TestAheadCountReturnsErrorWhenNoUpstream(t *testing.T) {
	requireTool(t, "git")
	repoRoot := t.TempDir()
	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")
	if err := os.WriteFile(filepath.Join(repoRoot, "README.md"), []byte("test"), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "init")

	_, err := AheadCount(context.Background(), repoRoot)
	if err == nil {
		t.Fatal("expected error when no upstream is configured")
	}
	var gitErr *GitCommandError
	if !errors.As(err, &gitErr) {
		t.Fatalf("expected GitCommandError, got %T", err)
	}
}
