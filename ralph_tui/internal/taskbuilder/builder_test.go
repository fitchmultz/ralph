// Package taskbuilder provides tests for task builder queue generation.
package taskbuilder

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/pin"
	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

func TestFormatQueueItemBlockIncludesEvidenceAndPlan(t *testing.T) {
	block, err := FormatQueueItemBlock(FormatOptions{
		ID:          "RQ-1234",
		Tags:        []string{"code"},
		Description: "Add task builder formatting",
		Scope:       "ralph_tui/internal/taskbuilder",
		Prompt:      "Create a task builder entry from prompt input.",
	})
	if err != nil {
		t.Fatalf("FormatQueueItemBlock failed: %v", err)
	}

	if len(block) == 0 || !strings.Contains(block[0], "RQ-1234") {
		t.Fatalf("expected header with ID, got: %v", block)
	}
	if !strings.Contains(block[0], "(ralph_tui/internal/taskbuilder)") {
		t.Fatalf("expected scope in header, got: %s", block[0])
	}

	hasEvidence := false
	hasPlan := false
	for _, line := range block {
		if strings.HasPrefix(line, "  - Evidence:") {
			hasEvidence = true
		}
		if strings.HasPrefix(line, "  - Plan:") {
			hasPlan = true
		}
	}
	if !hasEvidence || !hasPlan {
		t.Fatalf("expected Evidence and Plan bullets, got: %v", block)
	}
}

func TestBuildWritesQueueItem(t *testing.T) {
	queueFixture, doneFixture := locatePinFixtures(t)

	tmpDir := t.TempDir()
	pinDir := filepath.Join(tmpDir, "pin")
	if err := os.MkdirAll(pinDir, 0o755); err != nil {
		t.Fatalf("mkdir pin dir: %v", err)
	}
	queuePath := filepath.Join(pinDir, "implementation_queue.md")
	donePath := filepath.Join(pinDir, "implementation_done.md")
	copyFile(t, queueFixture, queuePath)
	copyFile(t, doneFixture, donePath)

	result, err := Build(nil, BuildOptions{
		RepoRoot:     tmpDir,
		PinDir:       pinDir,
		ProjectType:  project.TypeCode,
		Prompt:       "Add a task builder command that formats queue items.",
		WriteToQueue: true,
		InsertAtTop:  true,
	})
	if err != nil {
		t.Fatalf("Build failed: %v", err)
	}
	if result.ID == "" {
		t.Fatalf("expected queue ID to be set")
	}

	items, err := pin.ReadQueueItems(queuePath)
	if err != nil {
		t.Fatalf("ReadQueueItems failed: %v", err)
	}
	found := false
	for _, item := range items {
		if item.ID == result.ID {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected queue to include item %s", result.ID)
	}
}

func locatePinFixtures(t *testing.T) (string, string) {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("unable to resolve fixture path")
	}
	baseDir := filepath.Dir(file)
	pinDir := filepath.Clean(filepath.Join(baseDir, "..", "pin", "testdata", "pin"))
	queue := filepath.Join(pinDir, "implementation_queue.md")
	done := filepath.Join(pinDir, "implementation_done.md")
	if _, err := os.Stat(queue); err != nil {
		t.Fatalf("fixture queue missing: %v", err)
	}
	if _, err := os.Stat(done); err != nil {
		t.Fatalf("fixture done missing: %v", err)
	}
	return queue, done
}

func copyFile(t *testing.T, src string, dst string) {
	t.Helper()
	data, err := os.ReadFile(src)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	if err := os.WriteFile(dst, data, 0o600); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
}
