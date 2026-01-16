// Package pin provides tests for duplicate queue ID detection and repair.
package pin

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mitchfultz/ralph/ralph_tui/internal/project"
)

func TestDuplicateIDsDetectsCrossDuplicates(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))
	donePath := copyFixture(t, fixture.done, filepath.Join(tmpDir, "implementation_done.md"))

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}
	updated := strings.Replace(string(data), "RQ-0001", "RQ-0005", 1)
	if updated == string(data) {
		t.Fatalf("failed to introduce duplicate ID in queue")
	}
	if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	report, err := DuplicateIDs(Files{
		QueuePath: queuePath,
		DonePath:  donePath,
	})
	if err != nil {
		t.Fatalf("DuplicateIDs failed: %v", err)
	}
	if len(report.Cross) != 1 || report.Cross[0] != "RQ-0005" {
		t.Fatalf("expected cross duplicate RQ-0005, got %#v", report.Cross)
	}
	if len(report.Fixable) != 1 || report.Fixable[0] != "RQ-0005" {
		t.Fatalf("expected fixable duplicate RQ-0005, got %#v", report.Fixable)
	}
}

func TestFixDuplicateQueueIDsRenumbersQueue(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	donePath := copyFixture(t, fixture.done, files.DonePath)
	lookupPath := copyFixture(t, fixture.lookup, files.LookupPath)
	readmePath := copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

	data, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}
	updated := strings.Replace(string(data), "RQ-0001", "RQ-0005", 1)
	if updated == string(data) {
		t.Fatalf("failed to introduce duplicate ID in queue")
	}
	if err := os.WriteFile(queuePath, []byte(updated), 0o600); err != nil {
		t.Fatalf("write queue: %v", err)
	}

	result, err := FixDuplicateQueueIDs(Files{
		QueuePath:            queuePath,
		DonePath:             donePath,
		LookupPath:           lookupPath,
		ReadmePath:           readmePath,
		SpecsBuilderCodePath: files.SpecsBuilderCodePath,
		SpecsBuilderDocsPath: files.SpecsBuilderDocsPath,
	}, "", project.TypeCode)
	if err != nil {
		t.Fatalf("FixDuplicateQueueIDs failed: %v", err)
	}
	if len(result.Fixed) != 1 {
		t.Fatalf("expected 1 fix, got %#v", result.Fixed)
	}

	updatedQueue, err := os.ReadFile(queuePath)
	if err != nil {
		t.Fatalf("read updated queue: %v", err)
	}
	if strings.Contains(string(updatedQueue), "RQ-0005 [") {
		t.Fatalf("expected duplicate ID to be renumbered in queue")
	}
	if !strings.Contains(string(updatedQueue), "RQ-0006") {
		t.Fatalf("expected queue to include new ID RQ-0006")
	}

	if err := ValidatePin(files, project.TypeCode); err != nil {
		t.Fatalf("ValidatePin failed after fix: %v", err)
	}
}

func TestFixDuplicateQueueIDsRejectsDoneDuplicates(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	donePath := copyFixture(t, fixture.done, files.DonePath)
	copyFixtureSpecs(t, fixture, files)

	doneData, err := os.ReadFile(donePath)
	if err != nil {
		t.Fatalf("read done: %v", err)
	}
	duplicate := "\n- [x] RQ-0005 [docs]: Another done entry. (README.md)\n  - Evidence: duplicate ID\n  - Plan: update manually\n"
	if err := os.WriteFile(donePath, append(doneData, []byte(duplicate)...), 0o600); err != nil {
		t.Fatalf("write done: %v", err)
	}

	_, err = FixDuplicateQueueIDs(Files{
		QueuePath:            queuePath,
		DonePath:             donePath,
		SpecsBuilderCodePath: files.SpecsBuilderCodePath,
		SpecsBuilderDocsPath: files.SpecsBuilderDocsPath,
	}, "", project.TypeCode)
	if err == nil {
		t.Fatalf("expected error for done duplicates")
	}
	if !strings.Contains(err.Error(), "done log") {
		t.Fatalf("expected done log error, got %v", err)
	}
}
