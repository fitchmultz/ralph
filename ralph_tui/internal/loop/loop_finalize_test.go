// Package loop provides finalize iteration regression tests.
package loop

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

func TestFinalizeIterationAllowsPinOnlyHeadAdvance(t *testing.T) {
	requireTool(t, "git")
	repoRoot := t.TempDir()
	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	writePinFiles(t, pinDir, queueWithItem(), "## Done\n", "lookup\n", "readme\n")

	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "init")

	headBefore, err := HeadSHA(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("HeadSHA failed: %v", err)
	}

	writePinFiles(t, pinDir, queueEmpty(), doneWithItem(), "lookup\n", "readme\n")
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "pin update")

	runner, err := NewRunner(Options{
		RepoRoot:        repoRoot,
		PinDir:          pinDir,
		Runner:          "codex",
		ReasoningEffort: "auto",
		DirtyRepoStart:  DirtyRepoPolicyError,
		DirtyRepoDuring: DirtyRepoPolicyQuarantine,
		AllowUntracked:  true,
		RedactionMode:   redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	err = runner.finalizeIteration(context.Background(), "RQ-0001", "- [ ] RQ-0001 [code]: Test item. (x)", headBefore, FinalizeOptions{Mode: FinalizeModeNormal})
	if err != nil {
		t.Fatalf("expected pin-only head advance to be allowed, got error: %v", err)
	}
}

func TestFinalizeIterationRejectsNonPinHeadAdvance(t *testing.T) {
	requireTool(t, "git")
	repoRoot := t.TempDir()
	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	writePinFiles(t, pinDir, queueWithItem(), "## Done\n", "lookup\n", "readme\n")

	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "init")

	headBefore, err := HeadSHA(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("HeadSHA failed: %v", err)
	}

	writeFile(t, filepath.Join(repoRoot, "README.md"), "non-pin change\n")
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "non-pin update")

	runner, err := NewRunner(Options{
		RepoRoot:        repoRoot,
		PinDir:          pinDir,
		Runner:          "codex",
		ReasoningEffort: "auto",
		DirtyRepoStart:  DirtyRepoPolicyError,
		DirtyRepoDuring: DirtyRepoPolicyQuarantine,
		AllowUntracked:  true,
		RedactionMode:   redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("NewRunner failed: %v", err)
	}

	err = runner.finalizeIteration(context.Background(), "RQ-0001", "- [ ] RQ-0001 [code]: Test item. (x)", headBefore, FinalizeOptions{Mode: FinalizeModeNormal})
	if err == nil {
		t.Fatal("expected error for non-pin head advance, got nil")
	}
}

func writePinFiles(t *testing.T, pinDir string, queue string, done string, lookup string, readme string) {
	t.Helper()
	if err := os.MkdirAll(pinDir, 0o700); err != nil {
		t.Fatalf("mkdir pin dir: %v", err)
	}
	writeFile(t, filepath.Join(pinDir, "implementation_queue.md"), queue)
	writeFile(t, filepath.Join(pinDir, "implementation_done.md"), done)
	writeFile(t, filepath.Join(pinDir, "lookup_table.md"), lookup)
	writeFile(t, filepath.Join(pinDir, "README.md"), readme)
}

func queueWithItem() string {
	return "## Queue\n" +
		"- [ ] RQ-0001 [code]: Test item. (x)\n" +
		"  - Evidence: test evidence.\n" +
		"  - Plan: test plan.\n" +
		"\n## Blocked\n\n## Parking Lot\n"
}

func queueEmpty() string {
	return "## Queue\n\n## Blocked\n\n## Parking Lot\n"
}

func doneWithItem() string {
	return "## Done\n" +
		"- [x] RQ-0001 [code]: Test item. (x)\n" +
		"  - Evidence: test evidence.\n" +
		"  - Plan: test plan.\n"
}
