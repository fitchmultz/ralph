// Package loop provides tests for pin-only commit helpers.
package loop

import (
	"context"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
)

func TestAutoCommitPinOnlyChangesCommits(t *testing.T) {
	requireTool(t, "git")
	repoRoot, files := setupPinRepo(t, false)

	appendLine(t, files.QueuePath, "- [ ] RQ-0002 [ui]: Added item. (README.md)")

	before := gitCommitCount(t, repoRoot)
	committed, err := AutoCommitPinOnlyChanges(context.Background(), repoRoot, files, "chore: commit pin changes (pre-loop)")
	if err != nil {
		t.Fatalf("AutoCommitPinOnlyChanges failed: %v", err)
	}
	if !committed {
		t.Fatalf("expected pin changes to be committed")
	}
	after := gitCommitCount(t, repoRoot)
	if after != before+1 {
		t.Fatalf("expected commit count to increase by 1, got %d -> %d", before, after)
	}

	status, err := StatusDetails(context.Background(), repoRoot)
	if err != nil {
		t.Fatalf("StatusDetails failed: %v", err)
	}
	if !status.IsClean(true) {
		t.Fatalf("expected clean status after auto-commit")
	}
}

func TestAutoCommitPinOnlyChangesSkipsWhenNonPinDirty(t *testing.T) {
	requireTool(t, "git")
	repoRoot, files := setupPinRepo(t, true)

	appendLine(t, files.QueuePath, "- [ ] RQ-0002 [ui]: Added item. (README.md)")
	appendLine(t, filepath.Join(repoRoot, "notes.txt"), "changed")

	before := gitCommitCount(t, repoRoot)
	committed, err := AutoCommitPinOnlyChanges(context.Background(), repoRoot, files, "chore: commit pin changes (pre-loop)")
	if err != nil {
		t.Fatalf("AutoCommitPinOnlyChanges failed: %v", err)
	}
	if committed {
		t.Fatalf("expected auto-commit to skip when non-pin files are dirty")
	}
	after := gitCommitCount(t, repoRoot)
	if after != before {
		t.Fatalf("expected commit count unchanged, got %d -> %d", before, after)
	}
}

func setupPinRepo(t *testing.T, includeExtra bool) (string, pin.Files) {
	t.Helper()
	repoRoot := t.TempDir()
	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	if err := os.MkdirAll(pinDir, 0o755); err != nil {
		t.Fatalf("create pin dir: %v", err)
	}
	files := pin.ResolveFiles(pinDir)
	queueContent := strings.Join([]string{
		"## Queue",
		"- [ ] RQ-0001 [ui]: Base task. (README.md)",
		"  - Evidence: base evidence.",
		"  - Plan: base plan.",
		"",
		"## Blocked",
		"",
		"## Parking Lot",
		"",
	}, "\n")
	writePinFile(t, files.QueuePath, queueContent)
	writePinFile(t, files.DonePath, "## Done\n")
	writePinFile(t, files.LookupPath, "## Lookup\n")
	writePinFile(t, files.ReadmePath, "Pin readme.\n")
	writePinFile(t, files.SpecsPath, "Specs builder.\n")
	if includeExtra {
		writePinFile(t, filepath.Join(repoRoot, "notes.txt"), "base\n")
	}

	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "base")
	return repoRoot, files
}

func appendLine(t *testing.T, path string, line string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	content := strings.TrimRight(string(data), "\n") + "\n" + line + "\n"
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
}

func writePinFile(t *testing.T, path string, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write file: %v", err)
	}
}

func gitCommitCount(t *testing.T, repoRoot string) int {
	t.Helper()
	out := strings.TrimSpace(runCmd(t, repoRoot, "git", "rev-list", "--count", "HEAD"))
	count, err := strconv.Atoi(out)
	if err != nil {
		t.Fatalf("parse commit count: %v", err)
	}
	return count
}
