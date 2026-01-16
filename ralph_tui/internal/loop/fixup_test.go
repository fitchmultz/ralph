// Package loop provides tests for fixup behavior.
// Entrypoint: go test ./...
package loop

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
	"github.com/mitchfultz/ralph/ralph_tui/internal/redaction"
)

func TestFixupRequeuesWhenCIPasses(t *testing.T) {
	requireTool(t, "git")
	requireTool(t, "make")

	repoRoot, pinDir, _, _ := setupFixupRepo(t, makefilePass(), func(repoRoot string, pinDir string) {
		writeFileContent(t, filepath.Join(repoRoot, "README.md"), "wip change\n")
	})

	result, err := FixupBlockedItems(context.Background(), FixupOptions{
		RepoRoot:      repoRoot,
		PinDir:        pinDir,
		ProjectType:   project.TypeCode,
		MaxAttempts:   3,
		MaxItems:      0,
		RequireMain:   true,
		AutoCommit:    false,
		AutoPush:      false,
		RedactionMode: redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("FixupBlockedItems failed: %v", err)
	}

	if result.ScannedBlocked != 1 || result.Eligible != 1 {
		t.Fatalf("unexpected scan counts: %+v", result)
	}
	if len(result.RequeuedIDs) != 1 || result.RequeuedIDs[0] != "RQ-0003" {
		t.Fatalf("expected requeued RQ-0003, got %+v", result.RequeuedIDs)
	}

	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	items, err := pin.ReadQueueItems(queuePath)
	if err != nil {
		t.Fatalf("ReadQueueItems failed: %v", err)
	}
	if items[0].ID != "RQ-0003" {
		t.Fatalf("expected RQ-0003 to be first in queue, got %s", items[0].ID)
	}
	blocked, err := pin.ReadBlockedItems(queuePath)
	if err != nil {
		t.Fatalf("ReadBlockedItems failed: %v", err)
	}
	if len(blocked) != 0 {
		t.Fatalf("expected no blocked items after requeue, got %d", len(blocked))
	}

}

func TestFixupRecordsAttemptOnFailure(t *testing.T) {
	requireTool(t, "git")
	requireTool(t, "make")

	repoRoot, pinDir, _, _ := setupFixupRepo(t, makefileFail(), func(repoRoot string, pinDir string) {
		writeFileContent(t, filepath.Join(repoRoot, "README.md"), "wip change\n")
	})

	result, err := FixupBlockedItems(context.Background(), FixupOptions{
		RepoRoot:      repoRoot,
		PinDir:        pinDir,
		ProjectType:   project.TypeCode,
		MaxAttempts:   3,
		MaxItems:      0,
		RequireMain:   true,
		AutoCommit:    false,
		AutoPush:      false,
		RedactionMode: redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("FixupBlockedItems failed: %v", err)
	}
	if len(result.FailedIDs) != 1 || result.FailedIDs[0] != "RQ-0003" {
		t.Fatalf("expected failed RQ-0003, got %+v", result.FailedIDs)
	}

	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	blocked, err := pin.ReadBlockedItems(queuePath)
	if err != nil {
		t.Fatalf("ReadBlockedItems failed: %v", err)
	}
	if len(blocked) != 1 {
		t.Fatalf("expected blocked item to remain, got %d", len(blocked))
	}
	if blocked[0].FixupAttempts != 1 {
		t.Fatalf("expected fixup attempts to be 1, got %d", blocked[0].FixupAttempts)
	}
	if blocked[0].FixupLast == "" {
		t.Fatalf("expected fixup last to be populated")
	}
}

func TestFixupSkipsCIForPinOnlyChanges(t *testing.T) {
	requireTool(t, "git")

	repoRoot, pinDir, _, _ := setupFixupRepo(t, makefileFail(), func(repoRoot string, pinDir string) {
		writeFileContent(t, filepath.Join(pinDir, "README.md"), "updated pin readme\n")
	})

	result, err := FixupBlockedItems(context.Background(), FixupOptions{
		RepoRoot:      repoRoot,
		PinDir:        pinDir,
		ProjectType:   project.TypeCode,
		MaxAttempts:   3,
		MaxItems:      0,
		RequireMain:   true,
		AutoCommit:    false,
		AutoPush:      false,
		RedactionMode: redaction.ModeSecretsOnly,
	})
	if err != nil {
		t.Fatalf("FixupBlockedItems failed: %v", err)
	}
	if len(result.RequeuedIDs) != 1 || result.RequeuedIDs[0] != "RQ-0003" {
		t.Fatalf("expected requeued RQ-0003, got %+v", result.RequeuedIDs)
	}
}

func setupFixupRepo(t *testing.T, makefile string, wipChange func(repoRoot string, pinDir string)) (string, string, string, string) {
	repoRoot := t.TempDir()
	runCmd(t, repoRoot, "git", "init", "-b", "main")
	runCmd(t, repoRoot, "git", "config", "user.email", "test@example.com")
	runCmd(t, repoRoot, "git", "config", "user.name", "Test User")

	pinDir := filepath.Join(repoRoot, ".ralph", "pin")
	if err := os.MkdirAll(pinDir, 0o700); err != nil {
		t.Fatalf("mkdir pin: %v", err)
	}

	writeFileContent(t, filepath.Join(repoRoot, "README.md"), "base\n")
	writeFileContent(t, filepath.Join(repoRoot, "Makefile"), makefile)
	writeFileContent(t, filepath.Join(pinDir, "README.md"), "pin readme\n")
	writeFileContent(t, filepath.Join(pinDir, "lookup_table.md"), "| Area | Notes |\n| --- | --- |\n")
	writeFileContent(t, filepath.Join(pinDir, pin.SpecsBuilderCodeFilename), "Specs builder\n")
	writeFileContent(t, filepath.Join(pinDir, pin.SpecsBuilderDocsFilename), "Specs builder docs\n")
	writeFileContent(t, filepath.Join(pinDir, "implementation_done.md"), "## Done\n")
	writeFileContent(t, filepath.Join(pinDir, "implementation_queue.md"), baseQueue())

	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "base")
	knownGood := strings.TrimSpace(runCmd(t, repoRoot, "git", "rev-parse", "HEAD"))

	wipBranch := "ralph/wip/RQ-0003/20260101_000000"
	runCmd(t, repoRoot, "git", "checkout", "-b", wipBranch)
	wipChange(repoRoot, pinDir)
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "wip")

	runCmd(t, repoRoot, "git", "checkout", "main")
	writeFileContent(t, filepath.Join(pinDir, "implementation_queue.md"), blockedQueue(wipBranch, knownGood))
	runCmd(t, repoRoot, "git", "add", ".")
	runCmd(t, repoRoot, "git", "commit", "-m", "blocked")

	return repoRoot, pinDir, wipBranch, knownGood
}

func baseQueue() string {
	return strings.TrimSpace(`# Implementation Queue

## Queue
- [ ] RQ-0001 [code]: Base item. (README.md)
  - Evidence: base evidence.
  - Plan: base plan.

## Blocked

## Parking Lot
`) + "\n"
}

func blockedQueue(wipBranch string, knownGood string) string {
	content := `# Implementation Queue

## Queue
- [ ] RQ-0001 [code]: Base item. (README.md)
  - Evidence: base evidence.
  - Plan: base plan.

## Blocked
- [ ] RQ-0003 [ops]: Blocked fixture item. (README.md)
  - Evidence: blocked evidence.
  - Plan: blocked plan.
  - Blocked reason: quarantined
  - WIP branch: %s
  - Known-good: %s
  - Unblock hint: rerun fixup

## Parking Lot
`
	return fmt.Sprintf(content, wipBranch, knownGood)
}

func makefilePass() string {
	return "ci:\n\t@echo ok\n"
}

func makefileFail() string {
	return "ci:\n\t@echo fail\n\t@false\n"
}

func requireTool(t *testing.T, name string) {
	if _, err := exec.LookPath(name); err != nil {
		t.Skipf("missing %s", name)
	}
}

func runCmd(t *testing.T, dir string, name string, args ...string) string {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("command failed: %s %s: %v\n%s", name, strings.Join(args, " "), err, string(output))
	}
	return string(output)
}

func writeFileContent(t *testing.T, path string, content string) {
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("write file %s: %v", path, err)
	}
}
