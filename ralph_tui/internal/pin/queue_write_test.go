// Package pin provides tests for queue writing helpers.
package pin

import (
	"path/filepath"
	"testing"
)

func TestInsertQueueItemInsertsAtTop(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	files := ResolveFiles(tmpDir)
	queuePath := copyFixture(t, fixture.queue, files.QueuePath)
	_ = copyFixture(t, fixture.done, files.DonePath)
	_ = copyFixture(t, fixture.lookup, files.LookupPath)
	_ = copyFixture(t, fixture.readme, files.ReadmePath)
	copyFixtureSpecs(t, fixture, files)

	block := []string{
		"- [ ] RQ-9999 [code]: Add queue insert test. (ralph_tui/internal/pin/pin.go)",
		"  - Evidence: Verify queue insertion behavior.",
		"  - Plan: Insert the new item at the top of the Queue section.",
	}

	if err := InsertQueueItem(queuePath, block, InsertQueueOptions{InsertAtTop: true}); err != nil {
		t.Fatalf("InsertQueueItem failed: %v", err)
	}

	lines, err := readLines(queuePath)
	if err != nil {
		t.Fatalf("read queue: %v", err)
	}

	queueIndex := -1
	for idx, line := range lines {
		if line == "## Queue" {
			queueIndex = idx
			break
		}
	}
	if queueIndex < 0 {
		t.Fatalf("expected Queue section header")
	}
	if queueIndex+1 >= len(lines) {
		t.Fatalf("expected queue items after header")
	}
	if lines[queueIndex+1] != block[0] {
		t.Fatalf("expected inserted item at top, got: %q", lines[queueIndex+1])
	}
}

func TestInsertQueueItemValidatesBlock(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))

	block := []string{
		"- [ ] RQ-9999 [code]: Missing evidence and plan. (ralph_tui/internal/pin/pin.go)",
	}

	if err := InsertQueueItem(queuePath, block, InsertQueueOptions{InsertAtTop: true}); err == nil {
		t.Fatalf("expected InsertQueueItem to fail for invalid block")
	}
}

func TestMoveQueueItemToDoneMovesBlock(t *testing.T) {
	fixture := mustLocateFixtures(t)

	tmpDir := t.TempDir()
	queuePath := copyFixture(t, fixture.queue, filepath.Join(tmpDir, "implementation_queue.md"))
	donePath := copyFixture(t, fixture.done, filepath.Join(tmpDir, "implementation_done.md"))

	found, err := MoveQueueItemToDone(queuePath, donePath, "RQ-0001", DoneWriteOptions{
		Prepend:        true,
		RetentionLimit: 0,
	})
	if err != nil {
		t.Fatalf("MoveQueueItemToDone failed: %v", err)
	}
	if !found {
		t.Fatalf("expected to move RQ-0001")
	}

	queueItems, err := ReadQueueItems(queuePath)
	if err != nil {
		t.Fatalf("ReadQueueItems failed: %v", err)
	}
	for _, item := range queueItems {
		if item.ID == "RQ-0001" {
			t.Fatalf("expected RQ-0001 to be removed from queue")
		}
	}

	doneItems, err := ReadDoneItems(donePath)
	if err != nil {
		t.Fatalf("ReadDoneItems failed: %v", err)
	}
	foundDone := false
	for _, item := range doneItems {
		if item.ID == "RQ-0001" {
			foundDone = true
			if !item.Checked {
				t.Fatalf("expected moved done item to be checked")
			}
			break
		}
	}
	if !foundDone {
		t.Fatalf("expected to find RQ-0001 in done items")
	}
}
